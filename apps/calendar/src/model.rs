//! The calendars and their events, on disk in the data folder: `calendars.json` lists the calendars, each one's
//! events are an .ics file in `events\`. Subscribed calendars (an ICS address) keep their last download the same way
//! and are read-only. Occurrences of repeating events are computed only for the range on screen.

use crate::{
    date::{DAY, Date},
    ics,
    recur::RRule,
    tz::{self, Zone},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Event {
    pub uid: String,
    pub title: String,
    pub location: String,
    pub notes: String,
    /// "#rrggbb", or None for the calendar's color.
    pub color: Option<String>,
    pub all_day: bool,
    /// UTC instants; for all-day events, floating midnights (days × 86 400). The end is exclusive.
    pub start: i64,
    pub end: i64,
    /// The zone the event was written in ("" = floating: the PC's zone; "UTC").
    pub tzid: String,
    pub rrule: Option<RRule>,
    /// Occurrences removed from the series (their start instants).
    pub exdates: Vec<i64>,
    /// For a changed occurrence of a series: the start it replaces (same UID as the series).
    pub recurrence_id: Option<i64>,
    /// Reminders, in seconds before the start (negative: after).
    pub alarms: Vec<i64>,
    pub stamp: i64,
}

impl Event {
    /// The zone repeating times follow: UTC for all-day events (floating dates).
    pub fn zone(&self) -> Zone {
        if self.all_day {
            return Zone::Utc;
        }
        Zone::find(&self.tzid).unwrap_or_else(Zone::local)
    }
}

pub fn hash(s: &str) -> u64 {
    // FNV-1a: stable between runs (file names, generated UIDs)
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub fn new_uid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}-{:x}@plume-calendar", t, std::process::id(), n)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Calendar {
    pub id: String,
    pub name: String,
    pub color: String,
    pub visible: bool,
    /// Subscribed calendars: the address (read-only).
    pub url: Option<String>,
    /// Last successful download (UTC seconds).
    pub refreshed: i64,
    pub error: Option<String>,
    /// "local", "ics-url" (later: an account).
    pub provider: String,
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar {
            id: String::new(),
            name: String::new(),
            color: PALETTE[0].into(),
            visible: true,
            url: None,
            refreshed: 0,
            error: None,
            provider: "local".into(),
        }
    }
}

impl Calendar {
    pub fn read_only(&self) -> bool {
        provider_for(self).is_some_and(|p| p.read_only())
    }
}

/// Calendar and event colors: soft enough for dark and light, distinct from each other.
pub const PALETTE: [&str; 10] = ["#818cf8", "#f472b6", "#34d399", "#fbbf24", "#60a5fa", "#f87171", "#a78bfa", "#2dd4bf", "#fb923c", "#94a3b8"];

/// An occurrence on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Occ {
    /// Index of the calendar and of the event in it.
    pub cal: usize,
    pub ev: usize,
    /// Instants on the PC's clock (all-day: local midnights).
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    /// Original start in the series (a repeating event's occurrence), to edit or skip just this one.
    pub rid: Option<i64>,
}

/// Something that fills a calendar from elsewhere. Today: an ICS address; later, accounts (they will add
/// `push` / `remove` and stop being read-only).
pub trait Provider: Send {
    fn kind(&self) -> &'static str;
    fn read_only(&self) -> bool {
        true
    }
    fn pull(&self) -> Result<ics::Parsed, String>;
    // for account providers (not used by ICS addresses)
    #[allow(dead_code)]
    fn push(&self, _ev: &Event) -> Result<(), String> {
        Err("read-only".into())
    }
    #[allow(dead_code)]
    fn remove(&self, _uid: &str) -> Result<(), String> {
        Err("read-only".into())
    }
}

/// A calendar published as an .ics address (webcal:// too).
pub struct IcsUrl {
    pub url: String,
}

pub fn normalize_url(url: &str) -> String {
    let u = url.trim();
    if let Some(rest) = u.strip_prefix("webcal://").or_else(|| u.strip_prefix("webcals://")) {
        return format!("https://{rest}");
    }
    if !u.contains("://") {
        return format!("https://{u}");
    }
    u.to_string()
}

impl Provider for IcsUrl {
    fn kind(&self) -> &'static str {
        "ics-url"
    }
    fn pull(&self) -> Result<ics::Parsed, String> {
        let url = normalize_url(&self.url);
        let text = if let Some(path) = url.strip_prefix("file:///") {
            // local files: tests, and calendars synced by another program
            std::fs::read_to_string(path.replace("%20", " ")).map_err(|e| e.to_string())?
        } else {
            crate::net::get_text(&url)?
        };
        if !text.contains("BEGIN:VCALENDAR") {
            return Err("not a calendar".into());
        }
        Ok(ics::parse(&text))
    }
}

pub fn provider_for(cal: &Calendar) -> Option<Box<dyn Provider>> {
    cal.url.as_ref().map(|u| Box::new(IcsUrl { url: u.clone() }) as Box<dyn Provider>)
}

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_CALENDAR_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Calendar")
}

#[derive(Default)]
pub struct Store {
    pub cals: Vec<Calendar>,
    pub events: Vec<Vec<Event>>,
    /// Changes at every edit: cached occurrences are recomputed.
    pub version: u64,
}

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> bool {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_ok()
}

impl Store {
    fn cals_file() -> PathBuf {
        data_dir().join("calendars.json")
    }
    fn events_file(id: &str) -> PathBuf {
        data_dir().join("events").join(format!("{id}.ics"))
    }

    pub fn load() -> Store {
        let cals: Vec<Calendar> = std::fs::read(Self::cals_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut s = Store { cals, ..Default::default() };
        for c in &s.cals {
            let text = std::fs::read_to_string(Self::events_file(&c.id)).unwrap_or_default();
            s.events.push(ics::parse(&text).events);
        }
        if s.cals.is_empty() {
            s.add_calendar(crate::i18n::tr("Personal"), None);
        }
        s
    }

    pub fn save_list(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(&self.cals) {
            write_atomic(&Self::cals_file(), &b);
        }
    }

    pub fn save_cal(&self, i: usize) {
        if let (Some(c), Some(evs)) = (self.cals.get(i), self.events.get(i)) {
            write_atomic(&Self::events_file(&c.id), ics::write(&c.name, Some(&c.color), evs).as_bytes());
        }
    }

    pub fn touch(&mut self) {
        self.version += 1;
    }

    pub fn index(&self, id: &str) -> Option<usize> {
        self.cals.iter().position(|c| c.id == id)
    }

    /// The first calendar new events can go to.
    pub fn default_cal(&self) -> Option<usize> {
        self.cals.iter().position(|c| !c.read_only() && c.visible).or_else(|| self.cals.iter().position(|c| !c.read_only()))
    }

    pub fn add_calendar(&mut self, name: &str, url: Option<String>) -> usize {
        let used: HashSet<&str> = self.cals.iter().map(|c| c.color.as_str()).collect();
        let color = PALETTE.iter().find(|p| !used.contains(*p)).unwrap_or(&PALETTE[self.cals.len() % PALETTE.len()]).to_string();
        let id = format!("{:x}", hash(&new_uid()));
        self.cals.push(Calendar {
            id,
            name: name.to_string(),
            color,
            provider: if url.is_some() { "ics-url".into() } else { "local".into() },
            url,
            ..Default::default()
        });
        self.events.push(Vec::new());
        self.save_list();
        self.save_cal(self.cals.len() - 1);
        self.touch();
        self.cals.len() - 1
    }

    pub fn remove_calendar(&mut self, i: usize) {
        if i >= self.cals.len() {
            return;
        }
        let c = self.cals.remove(i);
        self.events.remove(i);
        let _ = std::fs::remove_file(Self::events_file(&c.id));
        self.save_list();
        self.touch();
    }

    /// Adds or replaces (same UID and occurrence) events in calendar `i`; returns how many.
    pub fn merge(&mut self, i: usize, evs: Vec<Event>) -> usize {
        let n = evs.len();
        let list = &mut self.events[i];
        for e in evs {
            match list.iter().position(|x| x.uid == e.uid && x.recurrence_id == e.recurrence_id) {
                Some(k) => list[k] = e,
                None => list.push(e),
            }
        }
        self.save_cal(i);
        self.touch();
        n
    }

    /// A subscription's new content replaces the old.
    pub fn replace(&mut self, i: usize, evs: Vec<Event>) {
        self.events[i] = evs;
        self.save_cal(i);
        self.touch();
    }

    /// Occurrences of the visible calendars touching [from, to) (UTC instants), sorted by start.
    pub fn occurrences(&self, from: i64, to: i64) -> Vec<Occ> {
        let local = Zone::local();
        let mut out = Vec::new();
        // floating range for all-day dates
        let f_from = tz::date_at(&local, from).days() * DAY;
        let f_to = (tz::date_at(&local, to - 1).days() + 1) * DAY;
        for (ci, cal) in self.cals.iter().enumerate() {
            if !cal.visible {
                continue;
            }
            let evs = &self.events[ci];
            let overridden: HashSet<(&str, i64)> = evs.iter().filter_map(|e| e.recurrence_id.map(|r| (e.uid.as_str(), r))).collect();
            for (ei, e) in evs.iter().enumerate() {
                let dur = (e.end - e.start).max(0);
                let to_local = |s: i64| -> (i64, i64) {
                    if e.all_day {
                        let d0 = Date::from_days(s.div_euclid(DAY));
                        let d1 = Date::from_days((s + dur.max(DAY)).div_euclid(DAY));
                        (tz::day_start(&local, d0), tz::day_start(&local, d1))
                    } else {
                        (s, s + dur)
                    }
                };
                let (a, b) = if e.all_day { (f_from, f_to) } else { (from, to) };
                match &e.rrule {
                    Some(r) if e.recurrence_id.is_none() => {
                        let zone = e.zone();
                        let start = zone.to_wall(e.start);
                        for s in r.expand(start, &zone, dur, a, b, 2_000) {
                            if e.exdates.contains(&s) || overridden.contains(&(e.uid.as_str(), s)) {
                                continue;
                            }
                            let (ls, le) = to_local(s);
                            out.push(Occ { cal: ci, ev: ei, start: ls, end: le, all_day: e.all_day, rid: Some(s) });
                        }
                    }
                    _ => {
                        if e.start < b && (e.start + dur.max(1)) > a {
                            let (ls, le) = to_local(e.start);
                            out.push(Occ { cal: ci, ev: ei, start: ls, end: le, all_day: e.all_day, rid: e.recurrence_id });
                        }
                    }
                }
            }
        }
        out.sort_by_key(|x| (!x.all_day, x.start, -(x.end - x.start)));
        out
    }

    pub fn event(&self, o: &Occ) -> &Event {
        &self.events[o.cal][o.ev]
    }

    pub fn color(&self, o: &Occ) -> String {
        self.event(o).color.clone().unwrap_or_else(|| self.cals[o.cal].color.clone())
    }

    /// Removes one occurrence of a series (it becomes an exception).
    pub fn skip_occurrence(&mut self, cal: usize, ev: usize, rid: i64) {
        let uid = self.events[cal][ev].uid.clone();
        let list = &mut self.events[cal];
        if list[ev].recurrence_id.is_some() {
            // a changed occurrence: remove it, and the series must not show the original either
            list.remove(ev);
            if let Some(m) = list.iter_mut().find(|e| e.uid == uid && e.recurrence_id.is_none())
                && !m.exdates.contains(&rid)
            {
                m.exdates.push(rid);
            }
        } else if !list[ev].exdates.contains(&rid) {
            list[ev].exdates.push(rid);
        }
        self.save_cal(cal);
        self.touch();
    }

    /// Removes an event; a series goes with its changed occurrences.
    pub fn delete_event(&mut self, cal: usize, ev: usize) {
        let e = self.events[cal].remove(ev);
        if e.recurrence_id.is_none() && e.rrule.is_some() {
            self.events[cal].retain(|x| x.uid != e.uid);
        }
        self.save_cal(cal);
        self.touch();
    }

    /// Changes one occurrence of a series: a new event with the same UID replaces it.
    pub fn change_occurrence(&mut self, cal: usize, ev: usize, rid: i64, mut new: Event) {
        let master = &self.events[cal][ev];
        new.uid = master.uid.clone();
        new.rrule = None;
        new.exdates.clear();
        new.recurrence_id = Some(rid);
        new.stamp = tz::now();
        let list = &mut self.events[cal];
        match list.iter().position(|x| x.uid == new.uid && x.recurrence_id == Some(rid)) {
            Some(k) => list[k] = new,
            None => list.push(new),
        }
        self.save_cal(cal);
        self.touch();
    }

    /// Moves a whole series by `delta` seconds (its exceptions follow).
    pub fn shift_series(&mut self, cal: usize, ev: usize, delta: i64) {
        let e = &mut self.events[cal][ev];
        e.start += delta;
        e.end += delta;
        for x in &mut e.exdates {
            *x += delta;
        }
        e.stamp = tz::now();
        self.save_cal(cal);
        self.touch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Wall;

    fn ev(title: &str, start: Wall, mins: i64, rule: Option<&str>) -> Event {
        let z = Zone::find("Europe/Paris").unwrap();
        let s = z.to_utc(start);
        Event {
            uid: new_uid(),
            title: title.into(),
            start: s,
            end: s + mins * 60,
            tzid: "Europe/Paris".into(),
            rrule: rule.and_then(RRule::parse),
            ..Default::default()
        }
    }

    fn store_with(evs: Vec<Event>) -> Store {
        Store { cals: vec![Calendar { id: "t".into(), name: "T".into(), ..Default::default() }], events: vec![evs], version: 0 }
    }

    #[test]
    fn series_exceptions_and_overrides() {
        let mut s = store_with(vec![ev("Yoga", Wall::new(Date::new(2026, 11, 2), 18, 0, 0), 60, Some("FREQ=WEEKLY"))]);
        let from = Date::new(2026, 11, 1).days() * DAY;
        let to = Date::new(2026, 12, 1).days() * DAY;
        let occ = s.occurrences(from, to);
        assert_eq!(occ.len(), 5);
        // skip the second one, move the third one an hour later
        let second = occ[1].rid.unwrap();
        let third = occ[2].clone();
        s.events[0][0].exdates.push(second);
        let mut moved = s.events[0][0].clone();
        moved.start = third.start + 3600;
        moved.end = third.end + 3600;
        let uid = s.events[0][0].uid.clone();
        s.events[0].push(Event { recurrence_id: third.rid, rrule: None, uid, ..moved });
        let occ = s.occurrences(from, to);
        assert_eq!(occ.len(), 4);
        assert!(occ.iter().any(|o| o.start == third.start + 3600 && o.rid == third.rid));
        assert!(!occ.iter().any(|o| o.start == third.start));
        // deleting the series removes the moved occurrence too
        s.delete_event(0, 0);
        assert!(s.events[0].is_empty());
    }

    #[test]
    fn all_day_and_hidden() {
        let day = Date::new(2026, 11, 11).days() * DAY;
        let mut s = store_with(vec![Event { uid: "h".into(), title: "Holiday".into(), all_day: true, start: day, end: day + DAY, ..Default::default() }]);
        let local = Zone::local();
        let from = tz::day_start(&local, Date::new(2026, 11, 11));
        let occ = s.occurrences(from, from + DAY);
        assert_eq!(occ.len(), 1);
        assert_eq!(occ[0].start, from);
        assert!(s.occurrences(from + DAY, from + 2 * DAY).is_empty());
        s.cals[0].visible = false;
        assert!(s.occurrences(from, from + DAY).is_empty());
    }

    #[test]
    fn urls() {
        assert_eq!(normalize_url("webcal://example.com/a.ics"), "https://example.com/a.ics");
        assert_eq!(normalize_url("example.com/a.ics"), "https://example.com/a.ics");
        assert_eq!(normalize_url(" http://x/y "), "http://x/y");
    }
}
