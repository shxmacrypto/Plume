//! iCalendar files (.ics, RFC 5545): read and written by hand. Folded lines, escaped text, dates with and without a
//! zone, all-day dates, durations, RRULE / EXDATE / RECURRENCE-ID, VALARM reminders, and VTIMEZONE blocks (used when
//! Windows does not know the zone's name: its standard offset is kept).

use crate::{
    date::{DAY, Date, Wall},
    model::Event,
    recur::RRule,
    tz::{Zone, parse_offset},
};
use std::collections::HashMap;

/// What a file holds.
#[derive(Debug, Default)]
pub struct Parsed {
    pub name: Option<String>,
    pub color: Option<String>,
    pub events: Vec<Event>,
    /// Events left out (broken dates, cancelled).
    pub skipped: usize,
}

#[derive(Debug, Clone)]
struct Line {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Line {
    fn param(&self, k: &str) -> Option<&str> {
        self.params.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str())
    }
}

/// Joins folded lines (a line starting with a space or a tab continues the previous one).
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.trim_start_matches('\u{feff}').split('\n') {
        let l = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = l.strip_prefix(' ').or_else(|| l.strip_prefix('\t'))
            && let Some(last) = out.last_mut()
        {
            last.push_str(rest);
            continue;
        }
        if !l.trim().is_empty() {
            out.push(l.to_string());
        }
    }
    out
}

/// "DTSTART;TZID=\"Europe/Paris\";X=1:20261111T090000" → name, parameters, value.
fn parse_line(s: &str) -> Option<Line> {
    let mut name = String::new();
    let mut params = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] != b';' && b[i] != b':' {
        i += 1;
    }
    name.push_str(&s[..i]);
    while i < b.len() && b[i] == b';' {
        i += 1;
        let k0 = i;
        while i < b.len() && b[i] != b'=' && b[i] != b':' && b[i] != b';' {
            i += 1;
        }
        let key = s[k0..i].to_string();
        let mut val = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            loop {
                if i < b.len() && b[i] == b'"' {
                    let q0 = i + 1;
                    i += 1;
                    while i < b.len() && b[i] != b'"' {
                        i += 1;
                    }
                    val.push_str(&s[q0..i.min(b.len())]);
                    i = (i + 1).min(b.len());
                } else {
                    let v0 = i;
                    while i < b.len() && b[i] != b';' && b[i] != b':' && b[i] != b',' {
                        i += 1;
                    }
                    val.push_str(&s[v0..i]);
                }
                if i < b.len() && b[i] == b',' {
                    val.push(',');
                    i += 1;
                    continue;
                }
                break;
            }
        }
        params.push((key, val));
    }
    if i >= b.len() || b[i] != b':' {
        return None;
    }
    Some(Line { name: name.trim().to_ascii_uppercase(), params, value: s[i + 1..].to_string() })
}

pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Folds a content line at 75 octets, never inside a character.
pub fn fold(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 8);
    let mut n = 0;
    for c in line.chars() {
        let len = c.len_utf8();
        if n + len > 75 {
            out.push_str("\r\n ");
            n = 1;
        }
        out.push(c);
        n += len;
    }
    out.push_str("\r\n");
    out
}

/// "P1D", "PT1H30M", "-PT15M", "P2W", "P1DT12H" → seconds.
pub fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    let (sign, rest) = match s.as_bytes().first()? {
        b'-' => (-1, &s[1..]),
        b'+' => (1, &s[1..]),
        _ => (1, s),
    };
    let rest = rest.strip_prefix('P')?;
    let mut total = 0i64;
    let mut num = String::new();
    let mut time = false;
    let mut any = false;
    for c in rest.chars() {
        match c {
            'T' => time = true,
            '0'..='9' => num.push(c),
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                any = true;
                total += n * match (c, time) {
                    ('W', false) => 7 * DAY,
                    ('D', false) => DAY,
                    ('H', true) => 3600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
            }
        }
    }
    (any && num.is_empty()).then_some(sign * total)
}

pub fn format_duration(secs: i64) -> String {
    let (sign, s) = if secs < 0 { ("-", -secs) } else { ("", secs) };
    if s == 0 {
        return "PT0S".into();
    }
    if s % DAY == 0 {
        return if s % (7 * DAY) == 0 { format!("{sign}P{}W", s / (7 * DAY)) } else { format!("{sign}P{}D", s / DAY) };
    }
    let mut out = format!("{sign}P");
    if s >= DAY {
        out += &format!("{}D", s / DAY);
    }
    out.push('T');
    let r = s % DAY;
    if r >= 3600 {
        out += &format!("{}H", r / 3600);
    }
    if r % 3600 >= 60 {
        out += &format!("{}M", r % 3600 / 60);
    }
    if r % 60 > 0 {
        out += &format!("{}S", r % 60);
    }
    out
}

/// A date or date-time value: (instant, all-day, tzid). All-day dates are floating midnights (days × 86 400);
/// floating times ("20261111T090000", no zone) are read in the PC's zone and keep an empty tzid.
fn parse_dt(value: &str, tzid: Option<&str>, value_type: Option<&str>, zones: &HashMap<String, Zone>) -> Option<(i64, bool, String)> {
    let v = value.trim();
    if value_type.is_some_and(|t| t.eq_ignore_ascii_case("DATE")) || (v.len() == 8 && !v.contains('T')) {
        let d = Date::parse_iso(&v[..8.min(v.len())])?;
        return Some((d.days() * DAY, true, String::new()));
    }
    let (d, t) = v.split_once('T')?;
    let date = Date::parse_iso(d)?;
    let utc = t.ends_with('Z') || t.ends_with('z');
    let t = t.trim_end_matches(['Z', 'z']);
    if t.len() < 4 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let h: u32 = t[..2].parse().ok()?;
    let m: u32 = t[2..4].parse().ok()?;
    let s: u32 = if t.len() >= 6 { t[4..6].parse().ok()? } else { 0 };
    if h > 24 || m > 59 || s > 60 {
        return None;
    }
    let wall = Wall { date, secs: (h * 3600 + m * 60 + s.min(59)).min(86_399) };
    if utc {
        return Some((wall.naive(), false, "UTC".into()));
    }
    if let Some(id) = tzid.filter(|i| !i.trim().is_empty()) {
        if let Some(z) = Zone::find(id) {
            return Some((z.to_utc(wall), false, z.tzid()));
        }
        if let Some(z) = zones.get(id) {
            return Some((z.to_utc(wall), false, z.tzid()));
        }
    }
    Some((Zone::local().to_utc(wall), false, String::new()))
}

/// A list of dates (EXDATE can hold several).
fn parse_dt_list(l: &Line, zones: &HashMap<String, Zone>) -> Vec<i64> {
    l.value.split(',').filter_map(|v| parse_dt(v, l.param("TZID"), l.param("VALUE"), zones).map(|x| x.0)).collect()
}

/// Colors written as "#rrggbb" (or the few CSS names calendars use).
fn color_of(v: &str) -> Option<String> {
    let v = v.trim();
    if v.starts_with('#') && (v.len() == 7 || v.len() == 9) && v[1..].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Some(v[..7].to_ascii_lowercase());
    }
    let named = match v.to_ascii_lowercase().as_str() {
        "red" | "crimson" => "#ef4444",
        "orange" | "darkorange" => "#f97316",
        "gold" | "yellow" => "#eab308",
        "green" | "forestgreen" | "seagreen" => "#22c55e",
        "teal" | "turquoise" => "#14b8a6",
        "blue" | "royalblue" | "dodgerblue" => "#3b82f6",
        "purple" | "violet" | "rebeccapurple" => "#8b5cf6",
        "pink" | "hotpink" | "deeppink" => "#ec4899",
        "brown" | "sienna" => "#a16207",
        "gray" | "grey" | "slategray" => "#64748b",
        _ => return None,
    };
    Some(named.into())
}

/// A reminder as written: (offset, relative to the end?, absolute instant?).
type Alarm = (i64, bool, Option<i64>);

#[derive(Default)]
struct Draft {
    ev: Event,
    start: Option<(i64, bool, String)>,
    end: Option<i64>,
    duration: Option<i64>,
    cancelled: bool,
    alarms: Vec<Alarm>,
}

pub fn parse(text: &str) -> Parsed {
    let mut out = Parsed::default();
    let lines: Vec<Line> = unfold(text).iter().filter_map(|l| parse_line(l)).collect();
    // zones described in the file, for TZIDs Windows does not know
    let mut zones: HashMap<String, Zone> = HashMap::new();
    {
        let mut tzid: Option<String> = None;
        let mut in_std = false;
        let mut std_off: Option<i32> = None;
        for l in &lines {
            match (l.name.as_str(), l.value.trim().to_ascii_uppercase().as_str()) {
                ("BEGIN", "VTIMEZONE") => {
                    tzid = None;
                    std_off = None;
                }
                ("BEGIN", "STANDARD") => in_std = true,
                ("END", "STANDARD") => in_std = false,
                ("TZID", _) => tzid = Some(l.value.trim().to_string()),
                ("TZOFFSETTO", _) if in_std || std_off.is_none() => std_off = parse_offset(&l.value),
                ("END", "VTIMEZONE") => {
                    if let (Some(id), Some(o)) = (tzid.take(), std_off.take())
                        && Zone::find(&id).is_none()
                    {
                        zones.insert(id, Zone::Fixed(o));
                    }
                }
                _ => {}
            }
        }
    }
    let mut depth: Vec<String> = Vec::new();
    let mut cur: Option<Draft> = None;
    let mut alarm: Option<(Option<Alarm>, String)> = None;
    for l in &lines {
        let top = depth.last().map(String::as_str).unwrap_or("");
        match l.name.as_str() {
            "BEGIN" => {
                let what = l.value.trim().to_ascii_uppercase();
                if what == "VEVENT" && top == "VCALENDAR" {
                    cur = Some(Draft::default());
                } else if what == "VALARM" && cur.is_some() {
                    alarm = Some((None, String::new()));
                }
                depth.push(what);
                continue;
            }
            "END" => {
                let what = l.value.trim().to_ascii_uppercase();
                if what == "VALARM" {
                    if let (Some((Some(a), action)), Some(d)) = (alarm.take(), cur.as_mut())
                        && !action.eq_ignore_ascii_case("NONE")
                    {
                        d.alarms.push(a);
                    }
                } else if what == "VEVENT"
                    && let Some(d) = cur.take()
                {
                    match finish(d) {
                        Some(ev) => out.events.push(ev),
                        None => out.skipped += 1,
                    }
                }
                depth.pop();
                continue;
            }
            _ => {}
        }
        if top == "VCALENDAR" {
            match l.name.as_str() {
                "X-WR-CALNAME" | "NAME" if out.name.is_none() => out.name = Some(unescape(&l.value)).filter(|n| !n.trim().is_empty()),
                "X-APPLE-CALENDAR-COLOR" | "X-WR-CALCOLOR" | "COLOR" | "X-PLUME-COLOR" if out.color.is_none() => out.color = color_of(&l.value),
                _ => {}
            }
            continue;
        }
        if top == "VALARM" {
            if let Some((a, action)) = alarm.as_mut() {
                match l.name.as_str() {
                    "TRIGGER" => {
                        if l.param("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE-TIME")) {
                            *a = parse_dt(&l.value, None, None, &zones).map(|x| (0, false, Some(x.0)));
                        } else if let Some(d) = parse_duration(&l.value) {
                            *a = Some((d, l.param("RELATED").is_some_and(|r| r.eq_ignore_ascii_case("END")), None));
                        }
                    }
                    "ACTION" => *action = l.value.trim().to_string(),
                    _ => {}
                }
            }
            continue;
        }
        let Some(d) = cur.as_mut().filter(|_| top == "VEVENT") else { continue };
        match l.name.as_str() {
            "UID" => d.ev.uid = l.value.trim().to_string(),
            "SUMMARY" => d.ev.title = unescape(&l.value),
            "LOCATION" => d.ev.location = unescape(&l.value),
            "DESCRIPTION" => d.ev.notes = unescape(&l.value),
            "DTSTART" => d.start = parse_dt(&l.value, l.param("TZID"), l.param("VALUE"), &zones),
            "DTEND" => d.end = parse_dt(&l.value, l.param("TZID"), l.param("VALUE"), &zones).map(|x| x.0),
            "DURATION" => d.duration = parse_duration(&l.value),
            "RRULE" => d.ev.rrule = RRule::parse(&l.value),
            "EXDATE" => d.ev.exdates.extend(parse_dt_list(l, &zones)),
            "RECURRENCE-ID" => d.ev.recurrence_id = parse_dt(&l.value, l.param("TZID"), l.param("VALUE"), &zones).map(|x| x.0),
            "STATUS" => d.cancelled = l.value.trim().eq_ignore_ascii_case("CANCELLED"),
            "COLOR" | "X-PLUME-COLOR" => d.ev.color = color_of(&l.value).or(d.ev.color.take()),
            "DTSTAMP" | "LAST-MODIFIED" => {
                if let Some((t, _, _)) = parse_dt(&l.value, None, None, &zones) {
                    d.ev.stamp = d.ev.stamp.max(t);
                }
            }
            _ => {}
        }
    }
    // cancelled occurrences of a series become exceptions of the series
    let cancelled: Vec<(String, i64)> = std::mem::take(&mut out.events)
        .into_iter()
        .filter_map(|e| {
            if e.title == "\u{0}cancelled" {
                e.recurrence_id.map(|r| (e.uid.clone(), r))
            } else {
                out.events.push(e);
                None
            }
        })
        .collect();
    for (uid, rid) in cancelled {
        if let Some(m) = out.events.iter_mut().find(|e| e.uid == uid && e.recurrence_id.is_none()) {
            m.exdates.push(rid);
        }
    }
    out
}

fn finish(d: Draft) -> Option<Event> {
    let (start, all_day, tzid) = d.start?;
    let mut ev = d.ev;
    if d.cancelled {
        // kept only to cancel an occurrence of a series (see `parse`)
        ev.recurrence_id?;
        ev.title = "\u{0}cancelled".into();
    }
    ev.all_day = all_day;
    ev.tzid = if all_day { String::new() } else { tzid };
    ev.start = start;
    ev.end = match (d.end, d.duration) {
        (Some(e), _) if e >= start => e,
        (_, Some(dur)) if dur >= 0 => start + dur,
        _ if all_day => start + DAY,
        _ => start,
    };
    if all_day && ev.end <= ev.start {
        ev.end = ev.start + DAY;
    }
    if ev.uid.is_empty() {
        ev.uid = format!("{}-{}@plume", start, crate::model::hash(&ev.title));
    }
    for (off, rel_end, abs) in d.alarms {
        let before = match abs {
            Some(t) => ev.start - t,
            None if rel_end => -(ev.end - ev.start + off),
            None => -off,
        };
        if !ev.alarms.contains(&before) {
            ev.alarms.push(before);
        }
    }
    ev.alarms.sort();
    Some(ev)
}

/// An instant written for a property: all-day dates as VALUE=DATE, zoned times with their TZID, else UTC.
fn dt_prop(name: &str, t: i64, ev: &Event) -> String {
    if ev.all_day {
        return format!("{name};VALUE=DATE:{}", Date::from_days(t.div_euclid(DAY)).compact());
    }
    let z = ev.zone();
    match z {
        Zone::Utc => {
            let w = Wall::from_naive(t);
            format!("{name}:{}T{:02}{:02}{:02}Z", w.date.compact(), w.hour(), w.minute(), w.second())
        }
        _ => {
            let w = z.to_wall(t);
            format!("{name};TZID={}:{}T{:02}{:02}{:02}", z.tzid(), w.date.compact(), w.hour(), w.minute(), w.second())
        }
    }
}

fn stamp(t: i64) -> String {
    let w = Wall::from_naive(t);
    format!("{}T{:02}{:02}{:02}Z", w.date.compact(), w.hour(), w.minute(), w.second())
}

/// A whole calendar as an .ics file.
pub fn write(name: &str, color: Option<&str>, events: &[Event]) -> String {
    let mut s = String::new();
    let mut put = |line: String| s.push_str(&fold(&line));
    put("BEGIN:VCALENDAR".into());
    put("VERSION:2.0".into());
    put("PRODID:-//Plume//Plume Calendar//EN".into());
    put("CALSCALE:GREGORIAN".into());
    put(format!("X-WR-CALNAME:{}", escape(name)));
    if let Some(c) = color {
        put(format!("X-APPLE-CALENDAR-COLOR:{c}"));
    }
    // one VTIMEZONE per zone used
    let mut done: Vec<String> = Vec::new();
    let mut tz_blocks = String::new();
    for ev in events.iter().filter(|e| !e.all_day) {
        let z = ev.zone();
        let id = z.tzid();
        if matches!(z, Zone::Utc) || done.contains(&id) {
            continue;
        }
        if let Some(block) = z.vtimezone(Date::from_days(ev.start.div_euclid(DAY)).y) {
            tz_blocks.push_str(&block);
        }
        done.push(id);
    }
    drop(put);
    s.push_str(&tz_blocks);
    for ev in events {
        s.push_str(&write_event(ev));
    }
    s.push_str("END:VCALENDAR\r\n");
    s
}

pub fn write_event(ev: &Event) -> String {
    let mut s = String::new();
    let mut put = |line: String| s.push_str(&fold(&line));
    put("BEGIN:VEVENT".into());
    put(format!("UID:{}", ev.uid));
    put(format!("DTSTAMP:{}", stamp(if ev.stamp > 0 { ev.stamp } else { crate::tz::now() })));
    put(dt_prop("DTSTART", ev.start, ev));
    put(dt_prop("DTEND", ev.end.max(ev.start), ev));
    if let Some(r) = ev.recurrence_id {
        put(dt_prop("RECURRENCE-ID", r, ev));
    }
    put(format!("SUMMARY:{}", escape(&ev.title)));
    if !ev.location.is_empty() {
        put(format!("LOCATION:{}", escape(&ev.location)));
    }
    if !ev.notes.is_empty() {
        put(format!("DESCRIPTION:{}", escape(&ev.notes)));
    }
    if let Some(c) = &ev.color {
        put(format!("X-PLUME-COLOR:{c}"));
    }
    if let Some(r) = &ev.rrule {
        put(format!("RRULE:{}", r.to_ics()));
    }
    for x in &ev.exdates {
        put(dt_prop("EXDATE", *x, ev));
    }
    for a in &ev.alarms {
        put("BEGIN:VALARM".into());
        put("ACTION:DISPLAY".into());
        put(format!("DESCRIPTION:{}", escape(&ev.title)));
        put(format!("TRIGGER:{}", format_duration(-a)));
        put("END:VALARM".into());
    }
    put("END:VEVENT".into());
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recur::Freq;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\nX-WR-CALNAME:Club\\, saison 2026\r\nX-WR-CALCOLOR:#FF8800\r\n\
BEGIN:VTIMEZONE\r\nTZID:Custom/Zone\r\nBEGIN:STANDARD\r\nDTSTART:19700101T000000\r\nTZOFFSETFROM:+0300\r\nTZOFFSETTO:+0300\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nUID:a1@test\r\nDTSTAMP:20261101T120000Z\r\nDTSTART;TZID=Europe/Paris:20261112T183000\r\nDTEND;TZID=Europe/Paris:20261112T200000\r\n\
SUMMARY:Entra\u{ee}nement\\; salle 2\r\nLOCATION:Gymnase\\, rue des Lilas\r\nDESCRIPTION:Apporter\\nles ballons\r\nRRULE:FREQ=WEEKLY;BYDAY=TH\r\n\
EXDATE;TZID=Europe/Paris:20261119T183000\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT30M\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:a1@test\r\nRECURRENCE-ID;TZID=Europe/Paris:20261126T183000\r\nDTSTART;TZID=Europe/Paris:20261126T190000\r\nDTEND;TZID=Europe/Paris:20261126T203000\r\nSUMMARY:Entra\u{ee}nement (tard)\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:a1@test\r\nRECURRENCE-ID;TZID=Europe/Paris:20261203T183000\r\nDTSTART;TZID=Europe/Paris:20261203T183000\r\nSTATUS:CANCELLED\r\nSUMMARY:x\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:h1\r\nDTSTART;VALUE=DATE:20261111\r\nDTEND;VALUE=DATE:20261112\r\nSUMMARY:Armistice\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:h2\r\nDTSTART:20261224\r\nSUMMARY:No end\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:z1\r\nDTSTART:20261115T100000Z\r\nDURATION:PT1H30M\r\nSUMMARY:Utc call with a long title that will need to be folded on more than one line for sure\r\nBEGIN:VALARM\r\nTRIGGER;RELATED=END:-PT10M\r\nACTION:DISPLAY\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:c1\r\nDTSTART;TZID=Custom/Zone:20261116T120000\r\nDTEND;TZID=Custom/Zone:20261116T130000\r\nSUMMARY:Custom\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:bad\r\nDTSTART:garbage\r\nSUMMARY:Broken\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn reads_a_calendar() {
        let p = parse(SAMPLE);
        assert_eq!(p.name.as_deref(), Some("Club, saison 2026"));
        assert_eq!(p.color.as_deref(), Some("#ff8800"));
        assert_eq!(p.skipped, 1);
        let ev = |uid: &str| p.events.iter().find(|e| e.uid == uid && e.recurrence_id.is_none()).unwrap();
        let a = ev("a1@test");
        assert_eq!(a.title, "Entra\u{ee}nement; salle 2");
        assert_eq!(a.location, "Gymnase, rue des Lilas");
        assert_eq!(a.notes, "Apporter\nles ballons");
        assert_eq!(a.tzid, "Europe/Paris");
        assert_eq!(Wall::from_naive(a.start), Wall::new(Date::new(2026, 11, 12), 17, 30, 0));
        assert_eq!(a.end - a.start, 5400);
        assert_eq!(a.rrule.as_ref().unwrap().freq, Freq::Weekly);
        assert_eq!(a.alarms, vec![1800]);
        // the explicit exception and the cancelled occurrence
        assert_eq!(a.exdates.len(), 2);
        let late = p.events.iter().find(|e| e.recurrence_id.is_some()).unwrap();
        assert_eq!(late.title, "Entra\u{ee}nement (tard)");
        assert_eq!(p.events.iter().filter(|e| e.uid == "a1@test").count(), 2);
        let h = ev("h1");
        assert!(h.all_day);
        assert_eq!(h.start, Date::new(2026, 11, 11).days() * DAY);
        assert_eq!(h.end - h.start, DAY);
        assert_eq!(ev("h2").end - ev("h2").start, DAY);
        let z = ev("z1");
        assert_eq!(z.end - z.start, 5400);
        // ten minutes before the end = 80 minutes after the start
        assert_eq!(z.alarms, vec![-4800]);
        let c = ev("c1");
        assert_eq!(Wall::from_naive(c.start).hour(), 9);
        assert_eq!(c.tzid, "UTC+0300");
    }

    #[test]
    fn round_trip() {
        let p = parse(SAMPLE);
        let text = write("Club; 2026", Some("#ff8800"), &p.events);
        assert!(text.lines().all(|l| l.len() <= 75), "{text}");
        assert!(text.contains("BEGIN:VTIMEZONE\r\nTZID:Europe/Paris"));
        assert!(text.contains("DTSTART;VALUE=DATE:20261111"));
        assert!(text.contains("TRIGGER:-PT30M"));
        let again = parse(&text);
        assert_eq!(again.name.as_deref(), Some("Club; 2026"));
        assert_eq!(again.events.len(), p.events.len());
        for (a, b) in p.events.iter().zip(again.events.iter()) {
            assert_eq!(a.uid, b.uid);
            assert_eq!(a.title, b.title);
            assert_eq!(a.start, b.start, "{}", a.title);
            assert_eq!(a.end, b.end);
            assert_eq!(a.all_day, b.all_day);
            assert_eq!(a.rrule, b.rrule);
            assert_eq!(a.exdates, b.exdates);
            assert_eq!(a.recurrence_id, b.recurrence_id);
            assert_eq!(a.alarms, b.alarms);
            assert_eq!(a.notes, b.notes);
            assert_eq!(a.location, b.location);
        }
    }

    #[test]
    fn lines_and_values() {
        assert_eq!(unfold("A:1\r\n  2\r\n\tthree\r\nB:x\n"), vec!["A:1 2three", "B:x"]);
        let l = parse_line("ATTENDEE;CN=\"Doe; John\";ROLE=REQ:mailto:j@x").unwrap();
        assert_eq!(l.param("CN"), Some("Doe; John"));
        assert_eq!(l.value, "mailto:j@x");
        assert_eq!(escape("a,b;c\\d\ne"), "a\\,b\\;c\\\\d\\ne");
        assert_eq!(unescape(&escape("a,b;c\\d\ne")), "a,b;c\\d\ne");
        let long = "é".repeat(60);
        let f = fold(&format!("SUMMARY:{long}"));
        assert!(f.split("\r\n").all(|l| l.len() <= 75));
        assert_eq!(unfold(&f)[0], format!("SUMMARY:{long}"));
        assert_eq!(parse_duration("PT1H30M"), Some(5400));
        assert_eq!(parse_duration("-P1DT2H"), Some(-(DAY + 7200)));
        assert_eq!(parse_duration("P2W"), Some(14 * DAY));
        assert_eq!(parse_duration("P"), None);
        assert_eq!(format_duration(-600), "-PT10M");
        assert_eq!(format_duration(DAY), "P1D");
        assert_eq!(format_duration(DAY + 90), "P1DT1M30S");
        for d in [-600, 0, 3600, DAY, 7 * DAY, DAY + 3660] {
            assert_eq!(parse_duration(&format_duration(d)), Some(d));
        }
    }
}
