//! The event card: title, times or all day, repetition, place, notes, calendar, reminder, color.

use super::{App, Field, Id, OccRef};
use crate::{
    date::{DAY, Date, Wall},
    i18n::{self, fill, tr},
    model::{Event, PALETTE, Store, new_uid},
    quickadd,
    recur::{Freq, RRule, Until},
    tz::{self, Zone},
};
use plume_ui::{Align, Canvas, Font, PaintCx, rect};

/// Reminder choices, in seconds before (-1: none).
const REMINDERS: [i64; 10] = [-1, 0, 300, 600, 900, 1800, 3600, 7200, 86_400, 2 * 86_400];

pub struct Editor {
    /// The occurrence being changed (None: a new event).
    pub target: Option<OccRef>,
    pub title: String,
    pub location: String,
    pub notes: String,
    pub all_day: bool,
    /// Local wall-clock times; for all-day events the end's date is the last day.
    pub start: Wall,
    pub end: Wall,
    pub cal: usize,
    pub color: Option<String>,
    pub reminder: i64,
    pub other_alarms: Vec<i64>,
    /// 0 none, 1 daily, 2 weekly, 3 monthly, 4 yearly.
    pub freq: u8,
    pub interval: u32,
    pub days: Vec<u32>,
    pub month_by_weekday: bool,
    /// 0 never, 1 after a number of times, 2 on a date.
    pub ends: u8,
    pub count: u32,
    pub until: Date,
    /// Text of the date / time / number field being typed.
    pub buf: Option<(Field, String)>,
    pub error: Option<String>,
    /// The original rule, to keep what this card does not show (BYMONTH, BYSETPOS…).
    keep_rule: Option<RRule>,
}

/// "second Tuesday": the weekday's rank in its month (-1: the last).
pub fn rank_in_month(d: Date) -> i32 {
    let n = ((d.d - 1) / 7 + 1) as i32;
    if d.d + 7 > crate::date::days_in_month(d.y, d.m) && n >= 4 { -1 } else { n }
}

fn ordinal(n: i32) -> &'static str {
    match n {
        1 => tr("first"),
        2 => tr("second"),
        3 => tr("third"),
        4 => tr("fourth"),
        5 => tr("fifth"),
        _ => tr("last"),
    }
}

/// "Every week on Monday, Wednesday", "Every 2 months", "Monthly on the second Tuesday, 5 times"…
pub fn describe(r: &RRule, start: Date) -> String {
    let n = r.interval.max(1);
    let every = |one: &'static str, many: &'static str| if n == 1 { tr(one).to_string() } else { fill(tr(many), &[&n.to_string()]) };
    let mut s = match r.freq {
        Freq::Daily => every("Every day", "Every {} days"),
        Freq::Weekly => {
            let base = every("Every week", "Every {} weeks");
            let mut days: Vec<u32> = r.by_day.iter().map(|(_, d)| *d).collect();
            if days.is_empty() {
                days.push(start.weekday());
            }
            if n == 1 && days.len() == 5 && (0..5).all(|d| days.contains(&d)) {
                tr("Every weekday").to_string()
            } else {
                let names: Vec<&str> = days.iter().map(|d| i18n::day(*d)).collect();
                format!("{base} {}", fill(tr("on {}"), &[&names.join(", ")]))
            }
        }
        Freq::Monthly => match r.by_day.first() {
            Some(&(k, wd)) if k != 0 && r.by_month_day.is_empty() => {
                let base = every("Every month", "Every {} months");
                format!("{base} {}", fill(tr("on the {}"), &[&format!("{} {}", ordinal(k), i18n::day(wd))]))
            }
            _ => {
                let day = r.by_month_day.first().map(|d| d.to_string()).unwrap_or_else(|| start.d.to_string());
                format!("{} {}", every("Every month", "Every {} months"), fill(tr("on day {}"), &[&day]))
            }
        },
        Freq::Yearly => every("Every year", "Every {} years"),
    };
    if let Some(c) = r.count {
        s = format!("{s}, {}", fill(tr("{} times"), &[&c.to_string()]));
    }
    match r.until {
        Some(Until::Date(d)) => s = format!("{s}, {}", fill(tr("until {}"), &[&i18n::short_date(d)])),
        Some(Until::Utc(t)) => s = format!("{s}, {}", fill(tr("until {}"), &[&i18n::short_date(tz::date_at(&Zone::local(), t))])),
        Some(Until::Local(w)) => s = format!("{s}, {}", fill(tr("until {}"), &[&i18n::short_date(w.date)])),
        None => {}
    }
    s
}

impl Editor {
    pub fn new_event(cal: usize, start: Wall, end: Wall, all_day: bool, reminder: i64) -> Editor {
        let end = if all_day {
            Wall { date: end.date.max(start.date), secs: 0 }
        } else if end <= start {
            Wall::from_naive(start.naive() + 3600)
        } else {
            end
        };
        Editor {
            target: None,
            title: String::new(),
            location: String::new(),
            notes: String::new(),
            all_day,
            start: if all_day { Wall { date: start.date, secs: 9 * 3600 } } else { start },
            end: if all_day { Wall { date: end.date, secs: 10 * 3600 } } else { end },
            cal,
            color: None,
            reminder: if all_day { -1 } else { reminder },
            other_alarms: Vec::new(),
            freq: 0,
            interval: 1,
            days: vec![start.date.weekday()],
            month_by_weekday: false,
            ends: 0,
            count: 10,
            until: start.date.add_months(3),
            buf: None,
            error: None,
            keep_rule: None,
        }
    }

    pub fn from_event(ev: &Event, r: &OccRef, rule: Option<RRule>) -> Editor {
        let z = Zone::local();
        let (start, end) = if r.all_day {
            let a = tz::date_at(&z, r.start);
            let b = tz::date_at(&z, r.end - 1).max(a);
            (Wall { date: a, secs: 9 * 3600 }, Wall { date: b, secs: 10 * 3600 })
        } else {
            (z.to_wall(r.start), z.to_wall(r.end))
        };
        let mut e = Editor::new_event(r.cal, start, end, r.all_day, -1);
        e.target = Some(r.clone());
        e.title = ev.title.clone();
        e.location = ev.location.clone();
        e.notes = ev.notes.clone();
        e.color = ev.color.clone();
        e.start = start;
        e.end = end;
        e.reminder = ev.alarms.first().copied().unwrap_or(-1);
        e.other_alarms = ev.alarms.iter().skip(1).copied().collect();
        if let Some(rule) = rule {
            e.freq = match rule.freq {
                Freq::Daily => 1,
                Freq::Weekly => 2,
                Freq::Monthly => 3,
                Freq::Yearly => 4,
            };
            e.interval = rule.interval.max(1);
            if rule.freq == Freq::Weekly && !rule.by_day.is_empty() {
                e.days = rule.by_day.iter().map(|(_, d)| *d).collect();
            }
            e.month_by_weekday = rule.freq == Freq::Monthly && rule.by_day.iter().any(|(k, _)| *k != 0);
            if let Some(c) = rule.count {
                e.ends = 1;
                e.count = c;
            }
            match rule.until {
                Some(Until::Date(d)) => (e.ends, e.until) = (2, d),
                Some(Until::Utc(t)) => (e.ends, e.until) = (2, tz::date_at(&z, t)),
                Some(Until::Local(w)) => (e.ends, e.until) = (2, w.date),
                None => {}
            }
            e.keep_rule = Some(rule);
        }
        e
    }

    pub fn text_of(&self, f: Field, h24: bool) -> String {
        if let Some((g, t)) = &self.buf
            && *g == f
        {
            return t.clone();
        }
        match f {
            Field::Title => self.title.clone(),
            Field::Location => self.location.clone(),
            Field::Notes => self.notes.clone(),
            Field::StartDate => self.start.date.iso(),
            Field::EndDate => self.end.date.iso(),
            Field::StartTime => i18n::time(self.start.secs, h24),
            Field::EndTime => i18n::time(self.end.secs, h24),
            Field::Interval => self.interval.to_string(),
            Field::Count => self.count.to_string(),
            Field::Until => self.until.iso(),
            Field::Quick | Field::Dialog => String::new(),
        }
    }

    pub fn set_text(&mut self, f: Field, text: String) {
        match f {
            Field::Title => self.title = text,
            Field::Location => self.location = text,
            Field::Notes => self.notes = text,
            _ => self.buf = Some((f, text)),
        }
    }

    /// Reads the typed date, time or number (when the field is left).
    pub fn commit(&mut self, f: Field, today: Date) {
        let Some((g, text)) = self.buf.take() else { return };
        if g != f {
            return;
        }
        let fr = i18n::french();
        let dur = self.end.naive() - self.start.naive();
        match f {
            Field::StartDate | Field::EndDate | Field::Until => match quickadd::parse_date(&text, today, fr) {
                Some(d) => {
                    self.error = None;
                    match f {
                        Field::StartDate => {
                            self.start.date = d;
                            self.end = Wall::from_naive(self.start.naive() + dur);
                        }
                        Field::EndDate => {
                            self.end.date = d;
                            self.fix_end();
                        }
                        _ => self.until = d.max(self.start.date),
                    }
                }
                None => self.error = Some(tr("Not a date").into()),
            },
            Field::StartTime | Field::EndTime => match quickadd::parse_time(&text) {
                Some(t) => {
                    self.error = None;
                    if f == Field::StartTime {
                        self.start.secs = t;
                        self.end = Wall::from_naive(self.start.naive() + dur);
                    } else {
                        self.end.secs = t;
                        self.fix_end();
                    }
                }
                None => self.error = Some(tr("Not a time").into()),
            },
            Field::Interval => self.interval = text.trim().parse::<u32>().ok().filter(|n| *n >= 1).unwrap_or(self.interval).min(999),
            Field::Count => self.count = text.trim().parse::<u32>().ok().filter(|n| *n >= 1).unwrap_or(self.count).min(9_999),
            _ => {}
        }
    }

    /// An end before the start: an hour after it (or the same day).
    fn fix_end(&mut self) {
        if self.all_day {
            if self.end.date < self.start.date {
                self.end.date = self.start.date;
            }
        } else if self.end <= self.start {
            // "ends at 1:00" after a 23:00 start: the next day
            let next = Wall { date: self.start.date.add_days(1), secs: self.end.secs };
            self.end =
                if self.end.date == self.start.date && next.naive() - self.start.naive() < DAY { next } else { Wall::from_naive(self.start.naive() + 3600) };
        }
    }

    /// The field after `f` (Enter, Tab).
    pub fn next_field(&self, f: Option<Field>) -> Option<Field> {
        let order: Vec<Field> = if self.all_day {
            vec![Field::Title, Field::StartDate, Field::EndDate, Field::Location, Field::Notes]
        } else {
            vec![Field::Title, Field::StartDate, Field::StartTime, Field::EndDate, Field::EndTime, Field::Location, Field::Notes]
        };
        let i = order.iter().position(|x| Some(*x) == f)?;
        order.get(i + 1).copied()
    }

    pub fn click(&mut self, id: &Id) {
        match id {
            Id::EdAllDay => {
                self.all_day = !self.all_day;
                if !self.all_day && self.end <= self.start {
                    self.end = Wall::from_naive(self.start.naive() + 3600);
                }
                if self.all_day && self.end.date < self.start.date {
                    self.end.date = self.start.date;
                }
            }
            Id::EdFreq(k) => {
                self.freq = *k;
                if self.days.is_empty() {
                    self.days.push(self.start.date.weekday());
                }
            }
            Id::EdDay(d) => {
                if self.days.contains(d) {
                    if self.days.len() > 1 {
                        self.days.retain(|x| x != d);
                    }
                } else {
                    self.days.push(*d);
                    self.days.sort();
                }
            }
            Id::EdMonthBy(b) => self.month_by_weekday = *b,
            Id::EdEnds(k) => self.ends = *k,
            Id::EdReminder => {
                let i = REMINDERS.iter().position(|r| *r == self.reminder).map(|i| i + 1).unwrap_or(0);
                self.reminder = REMINDERS[i % REMINDERS.len()];
            }
            Id::EdColor(c) => self.color = c.map(|k| PALETTE[k % PALETTE.len()].to_string()),
            Id::EdStep(f, dir) => {
                self.buf = None;
                let dir = *dir as i64;
                let dur = self.end.naive() - self.start.naive();
                match f {
                    Field::StartDate => {
                        self.start.date = self.start.date.add_days(dir);
                        self.end = Wall::from_naive(self.start.naive() + dur);
                    }
                    Field::StartTime => {
                        self.start = Wall::from_naive(self.start.naive() + dir * 900);
                        self.end = Wall::from_naive(self.start.naive() + dur);
                    }
                    Field::EndDate => {
                        self.end.date = self.end.date.add_days(dir);
                        self.fix_end();
                    }
                    Field::EndTime => {
                        self.end = Wall::from_naive(self.end.naive() + dir * 900);
                        self.fix_end();
                    }
                    Field::Interval => self.interval = (self.interval as i64 + dir).clamp(1, 999) as u32,
                    Field::Count => self.count = (self.count as i64 + dir).clamp(1, 9_999) as u32,
                    Field::Until => self.until = self.until.add_days(dir).max(self.start.date),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub fn rule(&self) -> Option<RRule> {
        let freq = match self.freq {
            1 => Freq::Daily,
            2 => Freq::Weekly,
            3 => Freq::Monthly,
            4 => Freq::Yearly,
            _ => return None,
        };
        let mut r = match &self.keep_rule {
            Some(k) if k.freq == freq => RRule { count: None, until: None, ..k.clone() },
            _ => RRule::new(freq),
        };
        r.interval = self.interval.max(1);
        match freq {
            Freq::Weekly => {
                r.by_day = self.days.iter().map(|d| (0, *d)).collect();
                if r.by_day == [(0, self.start.date.weekday())] {
                    r.by_day.clear();
                }
            }
            Freq::Monthly => {
                if self.month_by_weekday {
                    r.by_day = vec![(rank_in_month(self.start.date), self.start.date.weekday())];
                    r.by_month_day.clear();
                } else if r.by_day.iter().any(|(k, _)| *k != 0) {
                    r.by_day.clear();
                }
            }
            _ => {}
        }
        match self.ends {
            1 => r.count = Some(self.count.max(1)),
            2 => r.until = Some(if self.all_day { Until::Date(self.until) } else { Until::Utc(Zone::local().to_utc(Wall { date: self.until, secs: 86_399 })) }),
            _ => {}
        }
        Some(r)
    }

    pub fn build(&self, _store: &Store) -> Event {
        let z = Zone::local();
        let (start, end) = if self.all_day {
            (self.start.date.days() * DAY, (self.end.date.max(self.start.date).days() + 1) * DAY)
        } else {
            let s = z.to_utc(self.start);
            (s, z.to_utc(self.end).max(s))
        };
        let mut alarms: Vec<i64> = Vec::new();
        if self.reminder >= 0 {
            alarms.push(self.reminder);
        }
        alarms.extend(self.other_alarms.iter().copied().filter(|a| *a != self.reminder));
        Event {
            uid: new_uid(),
            title: if self.title.trim().is_empty() { tr("Untitled").to_string() } else { self.title.trim().to_string() },
            location: self.location.trim().to_string(),
            notes: self.notes.trim().to_string(),
            color: self.color.clone(),
            all_day: self.all_day,
            start,
            end,
            tzid: if self.all_day { String::new() } else { z.tzid() },
            rrule: self.rule(),
            exdates: Vec::new(),
            recurrence_id: None,
            alarms,
            stamp: tz::now(),
        }
    }
}

impl App {
    /// The editor card.
    pub(super) fn paint_editor(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(e) = self.editor.as_ref() else { return };
        let t = self.t.clone();
        let h24 = self.h24();
        let w = (cx.w - 40.0).min(600.0);
        let new = e.target.is_none();
        let rows_extra = match e.freq {
            0 => 0.0,
            2 | 3 => 88.0,
            _ => 44.0,
        };
        let h = 516.0 + rows_extra;
        let top = (plume_ui::m::TITLE_H + 12.0).max((cx.h - h) / 2.0);
        let card = rect((cx.w - w) / 2.0, top, w, h);
        self.paint_scrim(c, cx, false);
        self.card(c, card);
        let x = card.x + 24.0;
        let iw = w - 48.0;
        let mut y = card.y + 18.0;
        c.text(if new { tr("New event") } else { tr("Edit event") }, rect(x, y, iw - 40.0, 26.0), Font::Large, t.t1, Align::Left);
        self.icon_btn(c, rect(card.right() - 48.0, y - 2.0, 30.0, 30.0), &plume_ui::icons::CLOSE, Id::EdCancel);
        y += 40.0;
        // title
        let e = self.editor.as_ref().expect("editor");
        let (title, loc, notes) = (e.title.clone(), e.location.clone(), e.notes.clone());
        self.field_box(c, rect(x, y, iw, 38.0), Field::Title, &title, tr("Title"), Font::Title);
        y += 50.0;
        // when
        let e = self.editor.as_ref().expect("editor");
        let all_day = e.all_day;
        let (sd, st, ed, et) = (e.start.date, e.start.secs, e.end.date, e.end.secs);
        self.toggle_row(c, rect(x, y, 170.0, 32.0), tr("All day"), all_day, Id::EdAllDay);
        y += 42.0;
        let label_w = 58.0;
        let date_w = 150.0;
        let time_w = 96.0;
        for (k, (date, secs, df, tf)) in [(sd, st, Field::StartDate, Field::StartTime), (ed, et, Field::EndDate, Field::EndTime)].into_iter().enumerate() {
            let yy = y + k as f32 * 40.0;
            c.text(if k == 0 { tr("Starts") } else { tr("Ends") }, rect(x, yy, label_w, 32.0), Font::Body, t.t3, Align::Left);
            let mut xx = x + label_w;
            self.field_box(c, rect(xx, yy, date_w, 32.0), df, &format!("{} {}", i18n::day_short(date.weekday()), i18n::short_date(date)), "", Font::Body);
            xx += date_w + 4.0;
            self.stepper(c, xx, yy, df);
            xx += 60.0;
            if !all_day {
                self.field_box(c, rect(xx, yy, time_w, 32.0), tf, &i18n::time(secs, h24), "", Font::Body);
                xx += time_w + 4.0;
                self.stepper(c, xx, yy, tf);
            }
        }
        y += 84.0;
        // repeat
        let e = self.editor.as_ref().expect("editor");
        let (freq, days, by_wd, ends, interval, count, until) = (e.freq, e.days.clone(), e.month_by_weekday, e.ends, e.interval, e.count, e.until);
        c.text(tr("Repeat"), rect(x, y, label_w + 40.0, 32.0), Font::Body, t.t3, Align::Left);
        let opts: Vec<(String, Id, bool)> = [tr("None"), tr("Day"), tr("Week"), tr("Month"), tr("Year")]
            .iter()
            .enumerate()
            .map(|(k, l)| (l.to_string(), Id::EdFreq(k as u8), freq == k as u8))
            .collect();
        let _ = self.seg(c, x + 98.0, y, 32.0, &opts);
        y += 44.0;
        if freq == 2 {
            let ws = self.set.week_start();
            let mut xx = x + 98.0;
            for k in 0..7 {
                let d = (ws + k) % 7;
                let label = i18n::day_short(d);
                let r = rect(xx, y, 44.0, 30.0);
                self.chip(c, r, &label, days.contains(&d), Id::EdDay(d));
                xx += 48.0;
            }
            y += 44.0;
        } else if freq == 3 {
            let a = fill(tr("on day {}"), &[&sd.d.to_string()]);
            let b = fill(tr("on the {}"), &[&format!("{} {}", ordinal(rank_in_month(sd)), i18n::day(sd.weekday()))]);
            let wa = c.text_width(&a, Font::Body) + 28.0;
            let wb = c.text_width(&b, Font::Body) + 28.0;
            self.chip(c, rect(x + 98.0, y, wa, 30.0), &a, !by_wd, Id::EdMonthBy(false));
            self.chip(c, rect(x + 98.0 + wa + 8.0, y, wb, 30.0), &b, by_wd, Id::EdMonthBy(true));
            y += 44.0;
        }
        if freq != 0 {
            c.text(tr("Every"), rect(x + 98.0, y, 80.0, 32.0), Font::Body, t.t3, Align::Left);
            let ew = c.text_width(tr("Every"), Font::Body) + 10.0;
            self.field_box(c, rect(x + 98.0 + ew, y, 54.0, 32.0), Field::Interval, &interval.to_string(), "", Font::Body);
            let mut xx = x + 98.0 + ew + 70.0;
            let ends_opts: Vec<(String, Id, bool)> = vec![
                (tr("Never").to_string(), Id::EdEnds(0), ends == 0),
                (tr("After").to_string(), Id::EdEnds(1), ends == 1),
                (tr("Until").to_string(), Id::EdEnds(2), ends == 2),
            ];
            xx += self.seg(c, xx, y, 32.0, &ends_opts) + 8.0;
            match ends {
                1 => {
                    self.field_box(c, rect(xx, y, 64.0, 32.0), Field::Count, &count.to_string(), "", Font::Body);
                    c.text(tr("times"), rect(xx + 70.0, y, 60.0, 32.0), Font::Body, t.t3, Align::Left);
                }
                2 => self.field_box(c, rect(xx, y, (card.right() - 24.0 - xx).max(90.0), 32.0), Field::Until, &i18n::short_date(until), "", Font::Body),
                _ => {}
            }
            y += 44.0;
        }
        // place, notes
        self.field_box(c, rect(x, y, iw, 32.0), Field::Location, &loc, tr("Location"), Font::Body);
        y += 42.0;
        self.field_box(c, rect(x, y, iw, 32.0), Field::Notes, &notes, tr("Notes"), Font::Body);
        y += 46.0;
        // calendar, reminder
        let e = self.editor.as_ref().expect("editor");
        let (cal, rem, col) = (e.cal, e.reminder, e.color.clone());
        let cal_name = self.store.cals.get(cal).map(|c| c.name.clone()).unwrap_or_default();
        let cal_col = self.store.cals.get(cal).map(|c| super::color(&c.color)).unwrap_or(t.accent);
        let half = (iw - 12.0) / 2.0;
        self.pick(c, rect(x, y, half, 34.0), tr("Calendar"), &cal_name, Some(cal_col), Id::EdCal);
        let rem_text = if rem < 0 { tr("None").to_string() } else { i18n::reminder(rem) };
        self.pick(c, rect(x + half + 12.0, y, half, 34.0), tr("Reminder"), &rem_text, None, Id::EdReminder);
        y += 46.0;
        // color
        c.text(tr("Color"), rect(x, y, 70.0, 26.0), Font::Body, t.t3, Align::Left);
        let mut xx = x + 98.0;
        let r0 = rect(xx, y + 1.0, 24.0, 24.0);
        c.fill_round(r0, 12.0, cal_col);
        if col.is_none() {
            c.stroke_round(r0.inset(-3.0), 15.0, t.t1, 2.0);
        }
        self.hits.add(r0, Id::EdColor(None));
        xx += 34.0;
        for (k, p) in PALETTE.iter().enumerate() {
            let r = rect(xx, y + 1.0, 24.0, 24.0);
            c.fill_round(r, 12.0, super::color(p));
            if col.as_deref() == Some(*p) {
                c.stroke_round(r.inset(-3.0), 15.0, t.t1, 2.0);
            }
            self.hits.add(r, Id::EdColor(Some(k)));
            xx += 32.0;
        }
        // footer
        let by = card.bottom() - 56.0;
        if let Some(err) = self.editor.as_ref().and_then(|e| e.error.clone()) {
            c.text(&err, rect(x, by - 24.0, iw, 20.0), Font::Small, t.red, Align::Left);
        }
        if !new {
            self.btn(c, rect(x, by, 110.0, 36.0), tr("Delete"), 2, Id::EdDelete);
        }
        let sw = 120.0;
        self.btn(c, rect(card.right() - 24.0 - sw, by, sw, 36.0), tr("Save"), 1, Id::EdSave);
        self.btn(c, rect(card.right() - 24.0 - 2.0 * sw - 10.0, by, sw, 36.0), tr("Cancel"), 0, Id::EdCancel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_from_the_card() {
        let start = Wall::new(Date::new(2026, 11, 10), 18, 0, 0); // Tuesday
        let mut e = Editor::new_event(0, start, Wall::new(Date::new(2026, 11, 10), 19, 0, 0), false, 600);
        assert_eq!(e.rule(), None);
        e.click(&Id::EdFreq(2));
        assert_eq!(e.rule().unwrap().to_ics(), "FREQ=WEEKLY");
        e.click(&Id::EdDay(3));
        assert_eq!(e.rule().unwrap().to_ics(), "FREQ=WEEKLY;BYDAY=TU,TH");
        e.click(&Id::EdFreq(3));
        e.click(&Id::EdMonthBy(true));
        assert_eq!(e.rule().unwrap().to_ics(), "FREQ=MONTHLY;BYDAY=2TU");
        e.ends = 1;
        e.count = 4;
        assert_eq!(e.rule().unwrap().to_ics(), "FREQ=MONTHLY;COUNT=4;BYDAY=2TU");
        assert_eq!(rank_in_month(Date::new(2026, 11, 24)), -1);
        assert_eq!(rank_in_month(Date::new(2026, 11, 17)), 3);
        // moving the start keeps the length
        e.set_text(Field::StartTime, "20:30".into());
        e.commit(Field::StartTime, Date::new(2026, 11, 11));
        assert_eq!((e.start.secs, e.end.secs), (20 * 3600 + 1800, 21 * 3600 + 1800));
        e.set_text(Field::EndTime, "1:00".into());
        e.commit(Field::EndTime, Date::new(2026, 11, 11));
        assert_eq!(e.end, Wall::new(Date::new(2026, 11, 11), 1, 0, 0));
        e.set_text(Field::StartDate, "nope".into());
        e.commit(Field::StartDate, Date::new(2026, 11, 11));
        assert!(e.error.is_some());
    }

    #[test]
    fn describes() {
        i18n::set("en");
        let d = Date::new(2026, 11, 10);
        assert_eq!(describe(&RRule::parse("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR").unwrap(), d), "Every weekday");
        assert_eq!(describe(&RRule::parse("FREQ=WEEKLY;INTERVAL=2").unwrap(), d), "Every 2 weeks on Tuesday");
        assert_eq!(describe(&RRule::parse("FREQ=MONTHLY;BYDAY=-1FR;COUNT=3").unwrap(), d), "Every month on the last Friday, 3 times");
        assert_eq!(describe(&RRule::parse("FREQ=YEARLY;UNTIL=20301231").unwrap(), d), "Every year, until Dec 31");
    }
}
