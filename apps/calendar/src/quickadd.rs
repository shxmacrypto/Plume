//! The quick-add box: "Lunch with Max tomorrow 12:30", "Dentiste jeudi 14h-15h", "Holiday 24/12 all day".
//! Simple rules, English and French understood whatever the interface language: day words (today, tomorrow,
//! weekdays, "next"), dates (14/11, 2026-11-14, 14 novembre, Nov 14), times (12:30, 14h, 9am, noon), ranges
//! (14h-16h, from 2pm to 4pm), lengths (for 2 hours, pendant 1h30), "all day", and "@ place". The rest is the title.

use crate::date::{Date, days_in_month};

#[derive(Clone, Debug, PartialEq)]
pub struct Quick {
    pub title: String,
    pub date: Date,
    /// Seconds after midnight; None = all-day.
    pub start: Option<u32>,
    /// End time (seconds after midnight of `date`, may pass 24 h), when given.
    pub end: Option<u32>,
    pub location: String,
}

const MONTHS: [&[&str]; 12] = [
    &["january", "jan", "janvier", "janv"],
    &["february", "feb", "février", "fevrier", "févr", "fevr", "fév", "fev"],
    &["march", "mar", "mars"],
    &["april", "apr", "avril", "avr"],
    &["may", "mai"],
    &["june", "jun", "juin"],
    &["july", "jul", "juillet", "juil"],
    &["august", "aug", "août", "aout"],
    &["september", "sep", "sept", "septembre"],
    &["october", "oct", "octobre"],
    &["november", "nov", "novembre"],
    &["december", "dec", "décembre", "decembre", "déc"],
];

const WEEKDAYS: [&[&str]; 7] = [
    &["monday", "mon", "lundi"],
    &["tuesday", "tue", "tues", "mardi"],
    &["wednesday", "wed", "mercredi"],
    &["thursday", "thu", "thurs", "jeudi"],
    &["friday", "fri", "vendredi"],
    &["saturday", "sat", "samedi"],
    &["sunday", "dimanche"],
];

/// Words that only link the others ("at", "on"…): dropped when they end up at the edge of the title or before
/// a date or a time.
const LINKS: &[&str] = &[
    "at", "on", "the", "à", "a", "le", "la", "on", "for", "from", "de", "du", "pour", "pendant", "this", "ce", "cet", "cette", "-", "@", "to", "until", "till",
    "jusqu'à", "and", "et", "vers", "around",
];

fn clean(w: &str) -> String {
    w.trim_matches(|c: char| matches!(c, ',' | ';' | '!' | '?' | '(' | ')')).trim_end_matches('.').to_lowercase().replace('’', "'")
}

fn month_of(w: &str) -> Option<u32> {
    let w = w.trim_end_matches('.');
    MONTHS.iter().position(|names| names.contains(&w)).map(|i| i as u32 + 1)
}

fn weekday_of(w: &str) -> Option<u32> {
    WEEKDAYS.iter().position(|names| names.contains(&w)).map(|i| i as u32)
}

/// "14", "14th", "1st", "1er", "2nd" → day of the month.
fn day_num(w: &str) -> Option<u32> {
    let digits: String = w.chars().take_while(|c| c.is_ascii_digit()).collect();
    let rest = &w[digits.len()..];
    if digits.is_empty() || !matches!(rest, "" | "st" | "nd" | "rd" | "th" | "er" | "e" | "ème") {
        return None;
    }
    digits.parse().ok().filter(|d| (1..=31).contains(d))
}

/// A time: "12:30", "12h30", "12h", "9am", "9:30pm", "noon", "midi". `bare`: a plain number is accepted (after
/// "at", "à", "from"…). Returns seconds after midnight.
fn time_of(w: &str, bare: bool) -> Option<u32> {
    match w {
        "noon" | "midi" => return Some(12 * 3600),
        "midnight" | "minuit" => return Some(0),
        _ => {}
    }
    let (body, pm) = if let Some(b) = w.strip_suffix("pm").or_else(|| w.strip_suffix("p.m")) {
        (b, Some(true))
    } else if let Some(b) = w.strip_suffix("am").or_else(|| w.strip_suffix("a.m")) {
        (b, Some(false))
    } else {
        (w, None)
    };
    let hd: String = body.chars().take_while(|c| c.is_ascii_digit()).collect();
    if hd.is_empty() || hd.len() > 2 {
        return None;
    }
    let rest = &body[hd.len()..];
    let (sep, mins) = match rest.chars().next() {
        Some(c @ (':' | 'h' | 'H')) => (Some(c), &rest[1..]),
        None => (None, ""),
        _ => return None,
    };
    if !mins.is_empty() && (mins.len() != 2 || !mins.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    if sep.is_none() && pm.is_none() && !bare {
        return None;
    }
    if sep == Some(':') && mins.is_empty() {
        return None;
    }
    let mut h: u32 = hd.parse().ok()?;
    let m: u32 = if mins.is_empty() { 0 } else { mins.parse().ok()? };
    if m > 59 {
        return None;
    }
    match pm {
        Some(true) if h < 12 => h += 12,
        Some(false) if h == 12 => h = 0,
        Some(_) if h > 12 => return None,
        _ => {}
    }
    (h <= 24).then_some(h.min(23) * 3600 + if h == 24 { 59 * 60 } else { m * 60 })
}

/// A length: "2h", "1h30", "90min", "2 hours" (number then unit).
fn length_of(w: &str, next: Option<&str>) -> Option<(u32, usize)> {
    let digits: String = w.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    if digits.is_empty() {
        return None;
    }
    let n: f32 = digits.replace(',', ".").parse().ok()?;
    let unit = &w[digits.len()..];
    let secs = |u: &str| -> Option<u32> {
        match u {
            "h" | "hr" | "hrs" | "hour" | "hours" | "heure" | "heures" => Some(3600),
            "m" | "min" | "mins" | "minute" | "minutes" => Some(60),
            _ => None,
        }
    };
    if unit.is_empty() {
        let u = secs(next?)?;
        return Some(((n * u as f32) as u32, 2));
    }
    if let Some((h, m)) = unit.split_once('h').filter(|(a, b)| a.is_empty() && b.len() == 2 && b.bytes().all(|x| x.is_ascii_digit())) {
        let _ = h;
        return Some(((n * 3600.0) as u32 + m.parse::<u32>().ok()? * 60, 1));
    }
    Some(((n * secs(unit)? as f32) as u32, 1))
}

/// "14/11", "14/11/2026", "11/14" (month first when the day cannot be the month), "2026-11-14", "14.11.2026".
fn numeric_date(w: &str, today: Date, fr: bool) -> Option<Date> {
    if let Some(d) = Date::parse_iso(w) {
        return Some(d);
    }
    let sep = if w.contains('/') {
        '/'
    } else if w.matches('.').count() >= 1 && w.chars().all(|c| c.is_ascii_digit() || c == '.') {
        '.'
    } else {
        return None;
    };
    let parts: Vec<&str> = w.split(sep).collect();
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    let a: u32 = parts[0].parse().ok()?;
    let b: u32 = parts[1].parse().ok()?;
    let (d, m) = if a > 12 {
        (a, b)
    } else if b > 12 {
        (b, a)
    } else if fr || sep == '.' {
        (a, b)
    } else {
        (b, a)
    };
    let y = match parts.get(2) {
        Some(y) if y.len() == 2 => 2000 + y.parse::<i32>().ok()?,
        Some(y) => y.parse().ok()?,
        None => {
            // without a year: the next such date
            let this = Date::new(today.y, m, d);
            if this.valid() && this < today { today.y + 1 } else { today.y }
        }
    };
    let date = Date::new(y, m, d);
    date.valid().then_some(date)
}

/// A date typed in a field: "2026-11-14", "14/11", "14 nov", "tomorrow", "lundi"…
pub fn parse_date(text: &str, today: Date, fr: bool) -> Option<Date> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(d) = numeric_date(&clean(t), today, fr) {
        return Some(d);
    }
    // "Thu 12 Nov 2026" as the field shows it: the weekday is not needed
    let words: Vec<String> = t.split_whitespace().map(clean).filter(|w| weekday_of(w).is_none() || t.split_whitespace().count() == 1).collect();
    let q = parse(&words.join(" "), today, fr);
    (q.title.is_empty() && (q.date != today || words.iter().any(|w| matches!(w.as_str(), "today" | "aujourd'hui")))).then_some(q.date).or_else(|| {
        let dd: Vec<&str> = words.iter().map(String::as_str).collect();
        match dd.as_slice() {
            [a, b, y] => {
                let y: i32 = y.trim_end_matches(',').parse().ok()?;
                let (d, m) = match (day_num(a), month_of(b)) {
                    (Some(d), Some(m)) => (d, m),
                    _ => (day_num(b.trim_end_matches(','))?, month_of(a)?),
                };
                let date = Date::new(y, m, d);
                date.valid().then_some(date)
            }
            _ => None,
        }
    })
}

/// A time typed in a field: "14:30", "14h", "2pm", "9".
pub fn parse_time(text: &str) -> Option<u32> {
    let t = clean(text).replace(' ', "");
    time_of(&t, true)
}

pub fn parse(text: &str, today: Date, fr: bool) -> Quick {
    let raw: Vec<&str> = text.split_whitespace().collect();
    let words: Vec<String> = raw.iter().map(|w| clean(w)).collect();
    let n = words.len();
    let mut used = vec![false; n];
    let mut date: Option<Date> = None;
    let mut start: Option<u32> = None;
    let mut end: Option<u32> = None;
    let mut length: Option<u32> = None;
    let mut all_day = false;
    let mut location = String::new();
    let mut evening = false;
    let w = |i: usize| words.get(i).map(String::as_str).unwrap_or("");

    let mut i = 0;
    while i < n {
        let cur = w(i);
        let prev = if i > 0 { w(i - 1) } else { "" };
        let mut take = 0usize;
        // location: "@ place" or "@place" (to the end, or until a date/time word is met later on)
        if (cur == "@" || (cur.starts_with('@') && cur.len() > 1)) && time_of(cur.trim_start_matches('@'), true).is_none() {
            let mut parts: Vec<&str> = Vec::new();
            if cur.len() > 1 {
                parts.push(&raw[i][1..]);
            }
            let mut j = i + 1;
            while j < n && !is_when(w(j), w(j + 1), today, fr) {
                parts.push(raw[j]);
                j += 1;
            }
            location = parts.join(" ");
            for u in used.iter_mut().take(j).skip(i) {
                *u = true;
            }
            i = j;
            continue;
        }
        // all day
        if (cur == "all" && w(i + 1) == "day") || (cur == "toute" && w(i + 1) == "la" && w(i + 2).starts_with("journ")) || cur == "all-day" {
            all_day = true;
            take = if cur == "all" {
                2
            } else if cur == "all-day" {
                1
            } else {
                3
            };
        }
        // relative days
        else if cur == "today" || cur == "aujourd'hui" || cur == "auj" {
            date = Some(today);
            take = 1;
        } else if cur == "tonight" || (cur == "ce" && w(i + 1) == "soir") {
            date = Some(today);
            evening = true;
            take = if cur == "ce" { 2 } else { 1 };
        } else if cur == "tomorrow" || cur == "demain" {
            date = Some(today.add_days(1));
            take = 1;
        } else if cur == "après-demain" || cur == "apres-demain" || (cur == "après" && w(i + 1) == "demain") {
            date = Some(today.add_days(2));
            take = if cur == "après" { 2 } else { 1 };
        } else if cur == "day" && w(i + 1) == "after" && w(i + 2) == "tomorrow" {
            date = Some(today.add_days(2));
            take = 3;
        } else if (cur == "in" || cur == "dans") && w(i + 1).parse::<i64>().is_ok() {
            let k: i64 = w(i + 1).parse().unwrap_or(0);
            let mult = match w(i + 2) {
                "day" | "days" | "jour" | "jours" => Some(1),
                "week" | "weeks" | "semaine" | "semaines" => Some(7),
                _ => None,
            };
            if let Some(m) = mult {
                date = Some(today.add_days(k * m));
                take = 3;
            }
        }
        // weekdays, "next monday", "lundi prochain"
        else if let Some(wd) = weekday_of(cur).or(if cur == "next" || cur == "this" { weekday_of(w(i + 1)) } else { None }) {
            let next = cur == "next" || w(i + 1) == "prochain";
            let mut ahead = (wd as i64 + 7 - today.weekday() as i64) % 7;
            if next && ahead == 0 {
                ahead = 7;
            }
            date = Some(today.add_days(ahead));
            take = if cur == "next" || cur == "this" || w(i + 1) == "prochain" { 2 } else { 1 };
        }
        // "14 novembre", "14 nov 2027", "nov 14", "november 14th"
        else if let (Some(d), Some(m)) = (day_num(cur), month_of(w(i + 1))) {
            let (y, extra) = match w(i + 2).parse::<i32>() {
                Ok(y) if (1900..=2200).contains(&y) => (y, 1),
                _ => (if Date::new(today.y, m, d) < today { today.y + 1 } else { today.y }, 0),
            };
            if d <= days_in_month(y, m) {
                date = Some(Date::new(y, m, d));
                take = 2 + extra;
            }
        } else if let (Some(m), Some(d)) = (month_of(cur), day_num(w(i + 1))) {
            let (y, extra) = match w(i + 2).parse::<i32>() {
                Ok(y) if (1900..=2200).contains(&y) => (y, 1),
                _ => (if Date::new(today.y, m, d) < today { today.y + 1 } else { today.y }, 0),
            };
            if d <= days_in_month(y, m) {
                date = Some(Date::new(y, m, d));
                take = 2 + extra;
            }
        }
        // "le 14", "on the 14th"
        else if let Some(d) = day_num(cur).filter(|_| matches!(prev, "le" | "the" | "on") && !cur.chars().all(|c| c.is_ascii_digit()) || prev == "le") {
            let mut dt = Date::new(today.y, today.m, d);
            if !dt.valid() || dt < today {
                dt = Date::new(today.y, today.m, 1).add_months_raw(1);
                dt.d = d;
            }
            if dt.valid() {
                date = Some(dt);
                take = 1;
            }
        } else if let Some(d) = numeric_date(cur, today, fr) {
            date = Some(d);
            take = 1;
        }
        // lengths: "for 2 hours", "pendant 1h30"
        else if matches!(prev, "for" | "pendant" | "pour" | "durant")
            && let Some((secs, k)) = length_of(cur, words.get(i + 1).map(String::as_str))
        {
            length = Some(secs);
            take = k;
        }
        // ranges in one word: "14h-16h", "12:30-14:00", "2-4pm"
        else if let Some((a, b)) = cur.split_once('-').filter(|(a, b)| !a.is_empty() && !b.is_empty())
            && let Some(e) = time_of(b, true)
            && let Some(s) =
                time_of(a, true).map(|s| if b.ends_with("pm") && !a.ends_with("am") && s < 12 * 3600 && s + 12 * 3600 <= e { s + 12 * 3600 } else { s })
            && (time_of(a, false).is_some() || time_of(b, false).is_some())
        {
            start = Some(s);
            end = Some(e);
            take = 1;
        }
        // times: "12:30", "à 14h", "at 9", "9 am"
        else {
            let bare = matches!(prev, "at" | "à" | "a" | "@" | "from" | "de" | "to" | "until" | "till" | "jusqu'à" | "-" | "->" | "vers" | "around");
            let mut t = time_of(cur.trim_start_matches('@'), bare);
            let mut k = 1;
            if t.is_none() && matches!(w(i + 1), "am" | "pm" | "a.m" | "p.m" | "h") {
                t = time_of(&format!("{cur}{}", w(i + 1)), true);
                k = 2;
            }
            if let Some(t) = t {
                let is_end = start.is_some() && matches!(prev, "to" | "until" | "till" | "jusqu'à" | "-" | "->" | "à" | "a" | "and" | "et");
                if is_end {
                    end = Some(t);
                } else if start.is_none() {
                    start = Some(t);
                }
                take = k;
            }
        }
        if take > 0 {
            for u in used.iter_mut().skip(i).take(take) {
                *u = true;
            }
            // the linking word just before ("at", "à", "le", "from"…)
            let mut j = i;
            while j > 0 && !used[j - 1] && LINKS.contains(&w(j - 1)) {
                used[j - 1] = true;
                j -= 1;
            }
            i += take;
        } else {
            i += 1;
        }
    }
    if evening && start.is_none() {
        start = Some(19 * 3600);
    }
    // "from 2 to 4pm": the start borrows the afternoon
    if let (Some(s), Some(e)) = (start, end)
        && e > 12 * 3600
        && s < 12 * 3600
        && s + 12 * 3600 < e
        && s < 8 * 3600
    {
        start = Some(s + 12 * 3600);
    }
    if all_day {
        start = None;
        end = None;
    }
    if let (Some(s), None) = (start, end) {
        end = Some(s + length.unwrap_or(3600));
    }
    if let (Some(s), Some(e)) = (start, end)
        && e <= s
    {
        end = Some(e + 24 * 3600);
    }
    let mut title: Vec<&str> = raw.iter().zip(used.iter()).filter(|(_, u)| !**u).map(|(r, _)| *r).collect();
    // linking words left at the edges
    while title.last().is_some_and(|l| LINKS.contains(&clean(l).as_str())) {
        title.pop();
    }
    while title.first().is_some_and(|l| LINKS.contains(&clean(l).as_str()) && clean(l) != "a") {
        title.remove(0);
    }
    let title = title.join(" ").trim_matches(|c: char| c == ',' || c == '-' || c.is_whitespace()).to_string();
    Quick { title, date: date.unwrap_or(today), start, end, location }
}

/// Whether the word starts a date or a time (where a location given with "@" stops).
fn is_when(cur: &str, next: &str, today: Date, fr: bool) -> bool {
    matches!(cur, "today" | "tomorrow" | "tonight" | "demain" | "aujourd'hui" | "après-demain" | "apres-demain" | "next")
        || weekday_of(cur).is_some()
        || numeric_date(cur, today, fr).is_some()
        || time_of(cur, false).is_some()
        || (day_num(cur).is_some() && month_of(next).is_some())
        || (matches!(cur, "at" | "à" | "le" | "on") && (time_of(next, true).is_some() || weekday_of(next).is_some() || numeric_date(next, today, fr).is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Wednesday 11 November 2026
    const TODAY: Date = Date::new(2026, 11, 11);

    fn q(s: &str) -> Quick {
        parse(s, TODAY, false)
    }
    fn qf(s: &str) -> Quick {
        parse(s, TODAY, true)
    }
    fn hm(h: u32, m: u32) -> Option<u32> {
        Some(h * 3600 + m * 60)
    }

    #[test]
    fn english() {
        let x = q("Lunch with Max tomorrow 12:30");
        assert_eq!(x.title, "Lunch with Max");
        assert_eq!(x.date, Date::new(2026, 11, 12));
        assert_eq!((x.start, x.end), (hm(12, 30), hm(13, 30)));

        let x = q("Dentist on Friday at 3pm");
        assert_eq!(x.title, "Dentist");
        assert_eq!(x.date, Date::new(2026, 11, 13));
        assert_eq!(x.start, hm(15, 0));

        let x = q("Team meeting next wednesday from 2 to 4pm");
        assert_eq!(x.title, "Team meeting");
        assert_eq!(x.date, Date::new(2026, 11, 18));
        assert_eq!((x.start, x.end), (hm(14, 0), hm(16, 0)));

        let x = q("Call mom 9am for 30 min");
        assert_eq!(x.title, "Call mom");
        assert_eq!(x.date, TODAY);
        assert_eq!((x.start, x.end), (hm(9, 0), hm(9, 30)));

        let x = q("Holiday 12/24 all day");
        assert_eq!(x.title, "Holiday");
        assert_eq!(x.date, Date::new(2026, 12, 24));
        assert_eq!(x.start, None);

        let x = q("Concert Nov 20th 8:30pm @ Le Zénith");
        assert_eq!(x.title, "Concert");
        assert_eq!(x.date, Date::new(2026, 11, 20));
        assert_eq!(x.start, hm(20, 30));
        assert_eq!(x.location, "Le Zénith");

        let x = q("Birthday party");
        assert_eq!((x.title.as_str(), x.date, x.start), ("Birthday party", TODAY, None));

        let x = q("Standup 9:15-9:30 in 2 days");
        assert_eq!(x.date, Date::new(2026, 11, 13));
        assert_eq!((x.start, x.end), (hm(9, 15), hm(9, 30)));
        assert_eq!(x.title, "Standup");

        let x = q("Flight 2027-01-05 at noon");
        assert_eq!((x.date, x.start), (Date::new(2027, 1, 5), hm(12, 0)));
        assert_eq!(x.title, "Flight");

        // a date that passed this year means next year
        assert_eq!(q("Party 3/1").date, Date::new(2027, 3, 1));
        assert_eq!(q("Party 1/3").date, Date::new(2027, 1, 3));
        assert_eq!(q("Movie tonight").start, hm(19, 0));
        assert_eq!(q("Gym wednesday").date, TODAY);
    }

    #[test]
    fn french() {
        let x = qf("Déjeuner avec Max demain 12h30");
        assert_eq!(x.title, "Déjeuner avec Max");
        assert_eq!(x.date, Date::new(2026, 11, 12));
        assert_eq!((x.start, x.end), (hm(12, 30), hm(13, 30)));

        let x = qf("Dentiste jeudi à 14h");
        assert_eq!(x.title, "Dentiste");
        assert_eq!(x.date, Date::new(2026, 11, 12));
        assert_eq!(x.start, hm(14, 0));

        let x = qf("Réunion lundi prochain de 10h à 11h30");
        assert_eq!(x.title, "Réunion");
        assert_eq!(x.date, Date::new(2026, 11, 16));
        assert_eq!((x.start, x.end), (hm(10, 0), hm(11, 30)));

        let x = qf("Cours de piano le 20 novembre 17h-18h");
        assert_eq!(x.title, "Cours de piano");
        assert_eq!(x.date, Date::new(2026, 11, 20));
        assert_eq!((x.start, x.end), (hm(17, 0), hm(18, 0)));

        let x = qf("Anniversaire de Léa 03/12 toute la journée");
        assert_eq!(x.title, "Anniversaire de Léa");
        assert_eq!(x.date, Date::new(2026, 12, 3));
        assert_eq!(x.start, None);

        let x = qf("Match après-demain 20h pendant 2h");
        assert_eq!(x.title, "Match");
        assert_eq!(x.date, Date::new(2026, 11, 13));
        assert_eq!((x.start, x.end), (hm(20, 0), hm(22, 0)));

        let x = qf("Pot de départ ce soir");
        assert_eq!((x.title.as_str(), x.date, x.start), ("Pot de départ", TODAY, hm(19, 0)));

        let x = qf("Médecin le 3");
        assert_eq!(x.date, Date::new(2026, 12, 3));
        assert_eq!(x.title, "Médecin");

        let x = qf("Fête 1er décembre");
        assert_eq!(x.date, Date::new(2026, 12, 1));
        assert_eq!(x.title, "Fête");

        let x = qf("Train dans 2 semaines à 7h05 @ Gare de Lyon");
        assert_eq!(x.date, Date::new(2026, 11, 25));
        assert_eq!(x.start, hm(7, 5));
        assert_eq!(x.location, "Gare de Lyon");
        assert_eq!(x.title, "Train");
    }

    #[test]
    fn times() {
        assert_eq!(time_of("12:30", false), hm(12, 30));
        assert_eq!(time_of("12h", false), hm(12, 0));
        assert_eq!(time_of("9am", false), hm(9, 0));
        assert_eq!(time_of("12am", false), hm(0, 0));
        assert_eq!(time_of("12pm", false), hm(12, 0));
        assert_eq!(time_of("9:45pm", false), hm(21, 45));
        assert_eq!(time_of("9", false), None);
        assert_eq!(time_of("9", true), hm(9, 0));
        assert_eq!(time_of("25:00", false), None);
        assert_eq!(time_of("13pm", false), None);
        assert_eq!(time_of("1234", true), None);
        assert_eq!(parse_time("2 pm"), hm(14, 0));
        assert_eq!(parse_time("7h05"), hm(7, 5));
        assert_eq!(parse_date("2026-12-01", TODAY, false), Some(Date::new(2026, 12, 1)));
        assert_eq!(parse_date("tomorrow", TODAY, false), Some(Date::new(2026, 11, 12)));
        assert_eq!(parse_date("today", TODAY, false), Some(TODAY));
        assert_eq!(parse_date("3 janvier 2027", TODAY, true), Some(Date::new(2027, 1, 3)));
        assert_eq!(parse_date("Thursday, November 12, 2026", TODAY, false), Some(Date::new(2026, 11, 12)));
        assert_eq!(parse_date("nonsense", TODAY, false), None);
        // an evening that ends after midnight
        let x = q("Party saturday 22:00-02:00");
        assert_eq!((x.start, x.end), (hm(22, 0), Some(26 * 3600)));
    }
}
