//! Repeating events: the part of iCalendar's RRULE that calendars really use (DAILY, WEEKLY with BYDAY, MONTHLY by
//! day of the month or by weekday, YEARLY, INTERVAL, COUNT, UNTIL, BYMONTH, BYSETPOS, WKST). Occurrences are
//! computed on the wall clock of the event's zone, so a 9:00 meeting stays at 9:00 across daylight saving changes,
//! and only for the range on screen.

use crate::{
    date::{Date, Wall, days_in_month},
    tz::Zone,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Freq {
    Daily,
    #[default]
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    /// A date (all-day events): the last day that may hold an occurrence.
    Date(Date),
    /// A UTC instant ("…Z").
    Utc(i64),
    /// A wall-clock time in the event's zone.
    Local(Wall),
}

pub const DAY_CODES: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RRule {
    pub freq: Freq,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<Until>,
    /// (ordinal, weekday 0 = Monday): ordinal 0 = every such weekday, 2 = the second, -1 = the last.
    pub by_day: Vec<(i32, u32)>,
    pub by_month_day: Vec<i32>,
    pub by_month: Vec<u32>,
    pub by_set_pos: Vec<i32>,
    /// First day of the week (0 = Monday), for WEEKLY with an interval.
    pub wkst: u32,
    /// Parts this program does not use, kept to write them back.
    pub extra: Vec<String>,
}

fn parse_until(v: &str) -> Option<Until> {
    let v = v.trim();
    if v.len() == 8 {
        return Date::parse_iso(v).map(Until::Date);
    }
    let (d, t) = v.split_once('T')?;
    let date = Date::parse_iso(d)?;
    let utc = t.ends_with('Z');
    let t = t.trim_end_matches('Z');
    if t.len() < 6 {
        return None;
    }
    let w = Wall::new(date, t[..2].parse().ok()?, t[2..4].parse().ok()?, t[4..6].parse().ok()?);
    Some(if utc { Until::Utc(w.naive()) } else { Until::Local(w) })
}

impl RRule {
    pub fn new(freq: Freq) -> RRule {
        RRule { freq, interval: 1, ..Default::default() }
    }

    /// Parses "FREQ=WEEKLY;BYDAY=MO,WE;UNTIL=20261231T225959Z". None for frequencies this program does not
    /// repeat by (hourly and finer) or a broken rule.
    pub fn parse(s: &str) -> Option<RRule> {
        let mut r = RRule { interval: 1, ..Default::default() };
        let mut freq = None;
        for part in s.trim().trim_start_matches("RRULE:").split(';').filter(|p| !p.is_empty()) {
            let (k, v) = part.split_once('=')?;
            let list = || v.split(',').map(str::trim).filter(|x| !x.is_empty());
            match k.trim().to_ascii_uppercase().as_str() {
                "FREQ" => {
                    freq = Some(match v.trim().to_ascii_uppercase().as_str() {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        _ => return None,
                    })
                }
                "INTERVAL" => r.interval = v.trim().parse::<u32>().ok().filter(|&n| n >= 1)?,
                "COUNT" => r.count = Some(v.trim().parse().ok()?),
                "UNTIL" => r.until = Some(parse_until(v)?),
                "BYDAY" => {
                    for d in list() {
                        let d = d.to_ascii_uppercase();
                        let (num, code) = d.split_at(d.len().checked_sub(2)?);
                        let wd = DAY_CODES.iter().position(|c| *c == code)? as u32;
                        let n = if num.is_empty() { 0 } else { num.trim_start_matches('+').parse::<i32>().ok()? };
                        r.by_day.push((n, wd));
                    }
                }
                "BYMONTHDAY" => {
                    for d in list() {
                        r.by_month_day.push(d.parse::<i32>().ok().filter(|n| *n != 0 && n.abs() <= 31)?);
                    }
                }
                "BYMONTH" => {
                    for m in list() {
                        r.by_month.push(m.parse::<u32>().ok().filter(|m| (1..=12).contains(m))?);
                    }
                }
                "BYSETPOS" => {
                    for p in list() {
                        r.by_set_pos.push(p.parse::<i32>().ok().filter(|n| *n != 0)?);
                    }
                }
                "WKST" => r.wkst = DAY_CODES.iter().position(|c| c.eq_ignore_ascii_case(v.trim()))? as u32,
                _ => r.extra.push(part.to_string()),
            }
        }
        r.freq = freq?;
        Some(r)
    }

    /// Back to text (without "RRULE:").
    pub fn to_ics(&self) -> String {
        let mut s = format!(
            "FREQ={}",
            match self.freq {
                Freq::Daily => "DAILY",
                Freq::Weekly => "WEEKLY",
                Freq::Monthly => "MONTHLY",
                Freq::Yearly => "YEARLY",
            }
        );
        if self.interval > 1 {
            s += &format!(";INTERVAL={}", self.interval);
        }
        if let Some(c) = self.count {
            s += &format!(";COUNT={c}");
        }
        match self.until {
            Some(Until::Date(d)) => s += &format!(";UNTIL={}", d.compact()),
            Some(Until::Utc(t)) => {
                let w = Wall::from_naive(t);
                s += &format!(";UNTIL={}T{:02}{:02}{:02}Z", w.date.compact(), w.hour(), w.minute(), w.second())
            }
            Some(Until::Local(w)) => s += &format!(";UNTIL={}T{:02}{:02}{:02}", w.date.compact(), w.hour(), w.minute(), w.second()),
            None => {}
        }
        if !self.by_day.is_empty() {
            let days: Vec<String> =
                self.by_day.iter().map(|(n, d)| if *n == 0 { DAY_CODES[*d as usize].to_string() } else { format!("{n}{}", DAY_CODES[*d as usize]) }).collect();
            s += &format!(";BYDAY={}", days.join(","));
        }
        let nums = |v: &[i32]| v.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        if !self.by_month_day.is_empty() {
            s += &format!(";BYMONTHDAY={}", nums(&self.by_month_day));
        }
        if !self.by_month.is_empty() {
            s += &format!(";BYMONTH={}", self.by_month.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","));
        }
        if !self.by_set_pos.is_empty() {
            s += &format!(";BYSETPOS={}", nums(&self.by_set_pos));
        }
        if self.wkst != 0 {
            s += &format!(";WKST={}", DAY_CODES[self.wkst as usize]);
        }
        for e in &self.extra {
            s.push(';');
            s += e;
        }
        s
    }

    fn day_allowed(&self, d: Date) -> bool {
        (self.by_month.is_empty() || self.by_month.contains(&d.m))
            && (self.by_month_day.is_empty() || self.by_month_day.iter().any(|&md| month_day(d.y, d.m, md) == Some(d.d)))
            && (self.by_day.is_empty() || self.by_day.iter().any(|&(_, wd)| wd == d.weekday()))
    }

    /// The candidate days of one month (MONTHLY, or a month of a YEARLY rule), sorted.
    fn month_days(&self, y: i32, m: u32, start: Date) -> Vec<Date> {
        let mut out: Vec<Date> = Vec::new();
        if !self.by_month_day.is_empty() {
            for &md in &self.by_month_day {
                if let Some(d) = month_day(y, m, md) {
                    let date = Date::new(y, m, d);
                    if self.by_day.is_empty() || self.by_day.iter().any(|&(_, wd)| wd == date.weekday()) {
                        out.push(date);
                    }
                }
            }
        } else if !self.by_day.is_empty() {
            for &(n, wd) in &self.by_day {
                out.extend(nth_weekdays(y, m, n, wd));
            }
        } else if start.d <= days_in_month(y, m) {
            out.push(Date::new(y, m, start.d));
        }
        out.sort();
        out.dedup();
        out
    }

    fn set_pos(&self, mut v: Vec<Date>) -> Vec<Date> {
        if self.by_set_pos.is_empty() || v.is_empty() {
            return v;
        }
        let n = v.len() as i32;
        let mut keep: Vec<Date> = self
            .by_set_pos
            .iter()
            .filter_map(|&p| {
                let i = if p > 0 { p - 1 } else { n + p };
                (0..n).contains(&i).then(|| v[i as usize])
            })
            .collect();
        keep.sort();
        keep.dedup();
        v.clear();
        keep
    }

    /// The candidate days of period `k` (k-th interval after the start's period), sorted.
    fn period(&self, start: Date, k: i64) -> Vec<Date> {
        let step = k * self.interval as i64;
        match self.freq {
            Freq::Daily => {
                let d = start.add_days(step);
                if self.day_allowed(d) { vec![d] } else { vec![] }
            }
            Freq::Weekly => {
                let ws = start.week_start(self.wkst).add_days(step * 7);
                let mut v: Vec<Date> = if self.by_day.is_empty() {
                    vec![ws.add_days(((start.weekday() + 7 - self.wkst) % 7) as i64)]
                } else {
                    self.by_day.iter().map(|&(_, wd)| ws.add_days(((wd + 7 - self.wkst) % 7) as i64)).collect()
                };
                v.sort();
                v.dedup();
                v.retain(|d| self.by_month.is_empty() || self.by_month.contains(&d.m));
                self.set_pos(v)
            }
            Freq::Monthly => {
                let m = start.first_of_month().add_months_raw(step as i32);
                if !self.by_month.is_empty() && !self.by_month.contains(&m.m) {
                    return vec![];
                }
                self.set_pos(self.month_days(m.y, m.m, start))
            }
            Freq::Yearly => {
                let y = start.y + step as i32;
                let mut v = Vec::new();
                if self.by_month.is_empty() && self.by_month_day.is_empty() && !self.by_day.is_empty() {
                    // BYDAY alone in a yearly rule: weekdays counted in the whole year
                    for &(n, wd) in &self.by_day {
                        v.extend(nth_weekdays_of_year(y, n, wd));
                    }
                    v.sort();
                    v.dedup();
                } else {
                    let months = if self.by_month.is_empty() { vec![start.m] } else { self.by_month.clone() };
                    for m in months {
                        v.extend(self.month_days(y, m, start));
                    }
                    v.sort();
                    v.dedup();
                }
                self.set_pos(v)
            }
        }
    }

    /// Number of periods from the start's to the one holding `d` (may be negative).
    fn periods_until(&self, start: Date, d: Date) -> i64 {
        let n = self.interval.max(1) as i64;
        let raw = match self.freq {
            Freq::Daily => d.days() - start.days(),
            Freq::Weekly => (d.week_start(self.wkst).days() - start.week_start(self.wkst).days()) / 7,
            Freq::Monthly => (d.y as i64 * 12 + d.m as i64) - (start.y as i64 * 12 + start.m as i64),
            Freq::Yearly => (d.y - start.y) as i64,
        };
        raw.div_euclid(n)
    }

    fn passed_until(&self, wall: Wall, utc: i64) -> bool {
        match self.until {
            None => false,
            Some(Until::Date(d)) => wall.date > d,
            Some(Until::Utc(u)) => utc > u,
            Some(Until::Local(w)) => wall > w,
        }
    }

    /// Start instants of the occurrences that touch [from, to): an occurrence lasting `dur` seconds and starting
    /// at s is kept when s < to and s + dur > from (or s >= from for an instant). `start` is the first occurrence on
    /// the wall clock of `zone` (all-day events: Zone::Utc on floating midnights). At most `max` instants.
    pub fn expand(&self, start: Wall, zone: &Zone, dur: i64, from: i64, to: i64, max: usize) -> Vec<i64> {
        let mut out = Vec::new();
        let keep = |s: i64| s < to && (s + dur.max(1) > from);
        let first = zone.to_utc(start);
        let mut emitted: u32 = 0;
        // the start always counts as the first occurrence
        if keep(first) {
            out.push(first);
        }
        emitted += 1;
        if self.count.is_some_and(|c| emitted >= c) || first >= to {
            return out;
        }
        // without COUNT, jump close to the range instead of walking from the start
        let mut k: i64 = 0;
        if self.count.is_none() {
            let target = zone.to_wall(from - dur.max(0)).date.add_days(-2);
            k = (self.periods_until(start.date, target) - 1).max(0);
        }
        let mut empty_run = 0;
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > 200_000 || out.len() >= max {
                break;
            }
            let days = self.period(start.date, k);
            k += 1;
            if days.is_empty() {
                empty_run += 1;
                // a rule that never matches (31 February): give up after a long stretch
                if empty_run > 2_000 {
                    break;
                }
                continue;
            }
            empty_run = 0;
            let mut stop = false;
            for d in days {
                let wall = Wall { date: d, secs: start.secs };
                if wall <= start {
                    continue;
                }
                let utc = zone.to_utc(wall);
                if self.passed_until(wall, utc) || utc >= to || d.y > 9_999 {
                    stop = true;
                    break;
                }
                emitted += 1;
                if keep(utc) {
                    out.push(utc);
                    if out.len() >= max {
                        stop = true;
                        break;
                    }
                }
                if self.count.is_some_and(|c| emitted >= c) {
                    stop = true;
                    break;
                }
            }
            if stop {
                break;
            }
        }
        out
    }
}

/// Day `md` of a month (negative: from the end), if the month has it.
fn month_day(y: i32, m: u32, md: i32) -> Option<u32> {
    let n = days_in_month(y, m) as i32;
    let d = if md > 0 { md } else { n + md + 1 };
    (1..=n).contains(&d).then_some(d as u32)
}

/// The `n`-th weekday `wd` of a month (0 = all of them, negative = from the end).
fn nth_weekdays(y: i32, m: u32, n: i32, wd: u32) -> Vec<Date> {
    let first = Date::new(y, m, 1);
    let len = days_in_month(y, m);
    let off = (wd + 7 - first.weekday()) % 7;
    let all: Vec<Date> = (0..).map(|i| 1 + off + 7 * i).take_while(|&d| d <= len).map(|d| Date::new(y, m, d)).collect();
    pick(all, n)
}

fn nth_weekdays_of_year(y: i32, n: i32, wd: u32) -> Vec<Date> {
    let first = Date::new(y, 1, 1);
    let off = (wd + 7 - first.weekday()) % 7;
    let len = if crate::date::is_leap(y) { 366 } else { 365 };
    let all: Vec<Date> = (0..).map(|i| off as i64 + 7 * i).take_while(|&d| d < len).map(|d| first.add_days(d)).collect();
    pick(all, n)
}

fn pick(all: Vec<Date>, n: i32) -> Vec<Date> {
    if n == 0 {
        return all;
    }
    let i = if n > 0 { n - 1 } else { all.len() as i32 + n };
    if i >= 0 && (i as usize) < all.len() { vec![all[i as usize]] } else { vec![] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::DAY;

    fn d(y: i32, m: u32, dd: u32) -> Date {
        Date::new(y, m, dd)
    }
    fn at(date: Date, h: u32, m: u32) -> Wall {
        Wall::new(date, h, m, 0)
    }
    /// Dates of the occurrences in UTC (floating), from `from` to `to`.
    fn dates(rule: &str, start: Date, from: Date, to: Date) -> Vec<Date> {
        let r = RRule::parse(rule).unwrap();
        r.expand(Wall { date: start, secs: 0 }, &Zone::Utc, DAY, from.days() * DAY, to.days() * DAY, 10_000)
            .into_iter()
            .map(|t| Date::from_days(t / DAY))
            .collect()
    }

    #[test]
    fn parse_and_write() {
        let r = RRule::parse("FREQ=MONTHLY;INTERVAL=2;BYDAY=2TU,-1FR;COUNT=10;WKST=SU;X-NAME=1").unwrap();
        assert_eq!(r.freq, Freq::Monthly);
        assert_eq!(r.interval, 2);
        assert_eq!(r.by_day, vec![(2, 1), (-1, 4)]);
        assert_eq!(r.wkst, 6);
        assert_eq!(r.to_ics(), "FREQ=MONTHLY;INTERVAL=2;COUNT=10;BYDAY=2TU,-1FR;WKST=SU;X-NAME=1");
        assert_eq!(RRule::parse("FREQ=HOURLY"), None);
        assert_eq!(RRule::parse("INTERVAL=2"), None);
        assert_eq!(RRule::parse("FREQ=DAILY;INTERVAL=0"), None);
        let u = RRule::parse("FREQ=DAILY;UNTIL=20261231T225959Z").unwrap();
        assert_eq!(u.until, Some(Until::Utc(at(d(2026, 12, 31), 22, 59).naive() + 59)));
        assert_eq!(u.to_ics(), "FREQ=DAILY;UNTIL=20261231T225959Z");
        assert_eq!(RRule::parse("FREQ=YEARLY;UNTIL=20300101").unwrap().until, Some(Until::Date(d(2030, 1, 1))));
    }

    #[test]
    fn daily() {
        let v = dates("FREQ=DAILY", d(2026, 11, 1), d(2026, 11, 1), d(2026, 11, 5));
        assert_eq!(v, vec![d(2026, 11, 1), d(2026, 11, 2), d(2026, 11, 3), d(2026, 11, 4)]);
        let v = dates("FREQ=DAILY;INTERVAL=3", d(2026, 11, 1), d(2026, 11, 1), d(2026, 11, 12));
        assert_eq!(v, vec![d(2026, 11, 1), d(2026, 11, 4), d(2026, 11, 7), d(2026, 11, 10)]);
        // far range: the jump lands on the right phase
        let v = dates("FREQ=DAILY;INTERVAL=3", d(2026, 11, 1), d(2027, 11, 1), d(2027, 11, 8));
        assert!(v.iter().all(|x| (x.days() - d(2026, 11, 1).days()) % 3 == 0));
        assert_eq!(v.len(), 2);
        // BYDAY limits a daily rule
        let v = dates("FREQ=DAILY;BYDAY=SA,SU", d(2026, 11, 1), d(2026, 11, 1), d(2026, 11, 15));
        assert_eq!(v, vec![d(2026, 11, 1), d(2026, 11, 7), d(2026, 11, 8), d(2026, 11, 14)]);
    }

    #[test]
    fn count_and_until() {
        let v = dates("FREQ=DAILY;COUNT=3", d(2026, 11, 1), d(2026, 10, 1), d(2027, 1, 1));
        assert_eq!(v, vec![d(2026, 11, 1), d(2026, 11, 2), d(2026, 11, 3)]);
        // COUNT counts occurrences before the range too
        let v = dates("FREQ=DAILY;COUNT=5", d(2026, 11, 1), d(2026, 11, 4), d(2027, 1, 1));
        assert_eq!(v, vec![d(2026, 11, 4), d(2026, 11, 5)]);
        let v = dates("FREQ=WEEKLY;UNTIL=20261115", d(2026, 11, 1), d(2026, 10, 1), d(2027, 1, 1));
        assert_eq!(v, vec![d(2026, 11, 1), d(2026, 11, 8), d(2026, 11, 15)]);
        let v = dates("FREQ=WEEKLY;COUNT=1", d(2026, 11, 1), d(2026, 10, 1), d(2027, 1, 1));
        assert_eq!(v, vec![d(2026, 11, 1)]);
    }

    #[test]
    fn weekly_byday_and_interval() {
        // Monday 2 November 2026, every Monday, Wednesday, Friday
        let v = dates("FREQ=WEEKLY;BYDAY=MO,WE,FR", d(2026, 11, 2), d(2026, 11, 1), d(2026, 11, 10));
        assert_eq!(v, vec![d(2026, 11, 2), d(2026, 11, 4), d(2026, 11, 6), d(2026, 11, 9)]);
        // every other week on Tuesday and Thursday, starting on a Thursday
        let v = dates("FREQ=WEEKLY;INTERVAL=2;BYDAY=TU,TH", d(2026, 11, 5), d(2026, 11, 1), d(2026, 12, 1));
        assert_eq!(v, vec![d(2026, 11, 5), d(2026, 11, 17), d(2026, 11, 19)]);
        // the start counts even if it is not on a listed day
        let v = dates("FREQ=WEEKLY;BYDAY=MO;COUNT=3", d(2026, 11, 4), d(2026, 11, 1), d(2027, 1, 1));
        assert_eq!(v, vec![d(2026, 11, 4), d(2026, 11, 9), d(2026, 11, 16)]);
        // WKST changes which weeks an interval skips (RFC 5545 example)
        let mo = dates("FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=MO", d(1997, 8, 5), d(1997, 8, 1), d(1997, 9, 30));
        assert_eq!(mo, vec![d(1997, 8, 5), d(1997, 8, 10), d(1997, 8, 19), d(1997, 8, 24)]);
        let su = dates("FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=SU", d(1997, 8, 5), d(1997, 8, 1), d(1997, 9, 30));
        assert_eq!(su, vec![d(1997, 8, 5), d(1997, 8, 17), d(1997, 8, 19), d(1997, 8, 31)]);
        // far range with an interval keeps the phase
        let far = dates("FREQ=WEEKLY;INTERVAL=2", d(2026, 11, 2), d(2027, 6, 1), d(2027, 6, 30));
        assert!(far.iter().all(|x| (x.days() - d(2026, 11, 2).days()) % 14 == 0), "{far:?}");
        assert_eq!(far.len(), 2);
    }

    #[test]
    fn monthly() {
        // by day of the month, skipping months without a 31st
        let v = dates("FREQ=MONTHLY", d(2026, 1, 31), d(2026, 1, 1), d(2026, 9, 1));
        assert_eq!(v, vec![d(2026, 1, 31), d(2026, 3, 31), d(2026, 5, 31), d(2026, 7, 31), d(2026, 8, 31)]);
        // the second Tuesday
        let v = dates("FREQ=MONTHLY;BYDAY=2TU", d(2026, 11, 10), d(2026, 11, 1), d(2027, 2, 1));
        assert_eq!(v, vec![d(2026, 11, 10), d(2026, 12, 8), d(2027, 1, 12)]);
        // the last Friday
        let v = dates("FREQ=MONTHLY;BYDAY=-1FR", d(2026, 11, 27), d(2026, 11, 1), d(2027, 2, 1));
        assert_eq!(v, vec![d(2026, 11, 27), d(2026, 12, 25), d(2027, 1, 29)]);
        // the last day, and the 1st and 15th
        let v = dates("FREQ=MONTHLY;BYMONTHDAY=-1", d(2026, 1, 31), d(2026, 1, 1), d(2026, 4, 1));
        assert_eq!(v, vec![d(2026, 1, 31), d(2026, 2, 28), d(2026, 3, 31)]);
        let v = dates("FREQ=MONTHLY;BYMONTHDAY=1,15;COUNT=4", d(2026, 11, 1), d(2026, 1, 1), d(2028, 1, 1));
        assert_eq!(v, vec![d(2026, 11, 1), d(2026, 11, 15), d(2026, 12, 1), d(2026, 12, 15)]);
        // the last weekday of the month (BYSETPOS)
        let v = dates("FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1", d(2026, 10, 30), d(2026, 10, 1), d(2027, 1, 1));
        assert_eq!(v, vec![d(2026, 10, 30), d(2026, 11, 30), d(2026, 12, 31)]);
        // every 3 months, far away
        let v = dates("FREQ=MONTHLY;INTERVAL=3", d(2026, 1, 15), d(2030, 1, 1), d(2030, 12, 31));
        assert_eq!(v, vec![d(2030, 1, 15), d(2030, 4, 15), d(2030, 7, 15), d(2030, 10, 15)]);
        // Friday the 13th
        let v = dates("FREQ=MONTHLY;BYDAY=FR;BYMONTHDAY=13", d(2026, 2, 13), d(2026, 1, 1), d(2027, 12, 31));
        assert_eq!(v, vec![d(2026, 2, 13), d(2026, 3, 13), d(2026, 11, 13), d(2027, 8, 13)]);
    }

    #[test]
    fn yearly() {
        let v = dates("FREQ=YEARLY", d(2024, 2, 29), d(2024, 1, 1), d(2033, 1, 1));
        assert_eq!(v, vec![d(2024, 2, 29), d(2028, 2, 29), d(2032, 2, 29)]);
        // the 4th Thursday of November
        let v = dates("FREQ=YEARLY;BYMONTH=11;BYDAY=4TH", d(2026, 11, 26), d(2026, 1, 1), d(2029, 1, 1));
        assert_eq!(v, vec![d(2026, 11, 26), d(2027, 11, 25), d(2028, 11, 23)]);
        // last Sunday of March and October
        let v = dates("FREQ=YEARLY;BYMONTH=3,10;BYDAY=-1SU", d(2026, 3, 29), d(2026, 1, 1), d(2027, 12, 31));
        assert_eq!(v, vec![d(2026, 3, 29), d(2026, 10, 25), d(2027, 3, 28), d(2027, 10, 31)]);
        let v = dates("FREQ=YEARLY;INTERVAL=2;COUNT=3", d(2026, 7, 14), d(2026, 1, 1), d(2040, 1, 1));
        assert_eq!(v, vec![d(2026, 7, 14), d(2028, 7, 14), d(2030, 7, 14)]);
        // a rule that never matches ends instead of looping
        assert_eq!(dates("FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=30", d(2026, 1, 1), d(2026, 1, 2), d(2100, 1, 1)), Vec::<Date>::new());
    }

    #[test]
    fn dst_keeps_the_wall_clock() {
        let paris = Zone::find("Europe/Paris").unwrap();
        let r = RRule::parse("FREQ=WEEKLY").unwrap();
        // Thursday 15 October 2026, 9:00 in Paris; summer time ends on the 25th
        let start = at(d(2026, 10, 15), 9, 0);
        let from = paris.to_utc(at(d(2026, 10, 1), 0, 0));
        let to = paris.to_utc(at(d(2026, 11, 1), 0, 0));
        let v = r.expand(start, &paris, 3600, from, to, 100);
        assert_eq!(v.len(), 3);
        let walls: Vec<Wall> = v.iter().map(|&t| paris.to_wall(t)).collect();
        assert!(walls.iter().all(|w| w.hour() == 9 && w.minute() == 0), "{walls:?}");
        // in UTC: 7:00 before the change, 8:00 after
        assert_eq!(Wall::from_naive(v[1]).hour(), 7);
        assert_eq!(Wall::from_naive(v[2]).hour(), 8);
        // and in spring
        let spring = r.expand(at(d(2027, 3, 18), 9, 0), &paris, 3600, paris.to_utc(at(d(2027, 3, 1), 0, 0)), paris.to_utc(at(d(2027, 4, 2), 0, 0)), 100);
        assert_eq!(spring.iter().map(|&t| Wall::from_naive(t).hour()).collect::<Vec<_>>(), vec![8, 8, 7]);
    }

    #[test]
    fn until_in_utc_against_local_times() {
        let paris = Zone::find("Europe/Paris").unwrap();
        // daily at 23:30 Paris (22:30 UTC in winter) until 2026-12-03 22:30Z: the 3rd is the last one
        let r = RRule::parse("FREQ=DAILY;UNTIL=20261203T223000Z").unwrap();
        let v = r.expand(at(d(2026, 12, 1), 23, 30), &paris, 1800, 0, i64::MAX / 2, 100);
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn multi_day_occurrences_touch_the_range() {
        // a three-day event every week: the one that started before the range but still runs is kept
        let r = RRule::parse("FREQ=WEEKLY").unwrap();
        let start = Wall { date: d(2026, 11, 6), secs: 0 };
        let from = d(2026, 11, 14).days() * DAY;
        let v = r.expand(start, &Zone::Utc, 3 * DAY, from, from + DAY, 10);
        assert_eq!(v, vec![d(2026, 11, 13).days() * DAY]);
    }

    #[test]
    fn max_limits_output() {
        let r = RRule::parse("FREQ=DAILY").unwrap();
        assert_eq!(r.expand(Wall { date: d(2026, 1, 1), secs: 0 }, &Zone::Utc, DAY, 0, i64::MAX / 4, 50).len(), 50);
    }
}
