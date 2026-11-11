//! Reminders: which ones fall due between two checks. The window checks every half minute (and at start, for the
//! last quarter of an hour it may have missed); how they are shown is the window's business.

use crate::model::{Occ, Store, data_dir};

#[derive(Clone, Debug, PartialEq)]
pub struct Due {
    pub cal: usize,
    pub ev: usize,
    pub uid: String,
    pub title: String,
    pub location: String,
    /// The occurrence (PC's instants).
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    /// Seconds before the start.
    pub before: i64,
    /// When it is due.
    pub at: i64,
    pub color: String,
}

impl Due {
    /// Same reminder (to drop duplicates).
    pub fn key(&self) -> (String, i64, i64) {
        (self.uid.clone(), self.start, self.before)
    }
}

fn of(store: &Store, o: &Occ, before: i64) -> Due {
    let e = store.event(o);
    Due {
        cal: o.cal,
        ev: o.ev,
        uid: e.uid.clone(),
        title: e.title.clone(),
        location: e.location.clone(),
        start: o.start,
        end: o.end,
        all_day: o.all_day,
        before,
        at: o.start - before,
        color: store.color(o),
    }
}

/// Reminders due in (from, to], in order.
pub fn due(store: &Store, from: i64, to: i64) -> Vec<Due> {
    if to <= from {
        return Vec::new();
    }
    let (mut lo, mut hi) = (0i64, 0i64);
    for evs in &store.events {
        for e in evs {
            for &a in &e.alarms {
                lo = lo.min(a);
                hi = hi.max(a);
            }
        }
    }
    let hi = hi.min(30 * 86_400);
    let lo = lo.max(-30 * 86_400);
    let mut out: Vec<Due> = Vec::new();
    // an occurrence starting at s rings at s - a: s in (from + a, to + a]
    for o in store.occurrences(from + lo, to + hi + 1) {
        for &a in &store.event(&o).alarms {
            let at = o.start - a;
            if at > from && at <= to {
                out.push(of(store, &o, a));
            }
        }
    }
    out.sort_by_key(|d| d.at);
    out.dedup_by_key(|d| d.key());
    out
}

fn file() -> std::path::PathBuf {
    data_dir().join("reminders.json")
}

/// The last moment reminders were checked (to not ring twice after a restart).
pub fn load_last() -> Option<i64> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(file()).ok()?).ok()?;
    v["last"].as_i64()
}

pub fn save_last(t: i64) {
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(file(), serde_json::json!({ "last": t }).to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        date::{DAY, Date, Wall},
        model::{Calendar, Event},
        recur::RRule,
        tz::Zone,
    };

    #[test]
    fn rings_once_at_the_right_time() {
        let z = Zone::local();
        let s = z.to_utc(Wall::new(Date::new(2026, 11, 12), 12, 30, 0));
        let day = Date::new(2026, 11, 13).days() * DAY;
        let store = Store {
            cals: vec![Calendar { id: "c".into(), ..Default::default() }],
            events: vec![vec![
                Event { uid: "lunch".into(), title: "Lunch".into(), start: s, end: s + 3600, alarms: vec![600, 0], ..Default::default() },
                Event {
                    uid: "daily".into(),
                    title: "Pills".into(),
                    start: s - 3 * DAY,
                    end: s - 3 * DAY,
                    alarms: vec![300],
                    rrule: RRule::parse("FREQ=DAILY"),
                    ..Default::default()
                },
                Event { uid: "hol".into(), title: "Holiday".into(), all_day: true, start: day, end: day + DAY, alarms: vec![15 * 3600], ..Default::default() },
            ]],
            version: 0,
        };
        // 12:20 is due (lunch 10 min before), with the daily one at 12:25
        let d = due(&store, s - 700, s - 590);
        assert_eq!(d.iter().map(|d| d.title.as_str()).collect::<Vec<_>>(), vec!["Lunch"]);
        assert_eq!(d[0].before, 600);
        let d = due(&store, s - 590, s);
        assert_eq!(d.iter().map(|d| (d.title.as_str(), d.before)).collect::<Vec<_>>(), vec![("Pills", 300), ("Lunch", 0)]);
        // nothing twice
        assert!(due(&store, s, s + 60).is_empty());
        // the all-day event rings at 9:00 the day before (local midnight minus 15 h)
        let nine = z.to_utc(Wall::new(Date::new(2026, 11, 12), 9, 0, 0));
        let d = due(&store, nine - 30, nine + 30);
        assert_eq!(d.iter().map(|d| d.title.as_str()).collect::<Vec<_>>(), vec!["Holiday"]);
    }
}
