//! The settings page.

use super::{App, Id};
use crate::{
    i18n::{fill, tr},
    tz::Zone,
};
use plume_ui::{Align, Canvas, Font, Rect, icons, rect};

impl App {
    fn section(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, rows: f32) -> Rect {
        let t = self.t.clone();
        c.text(title, rect(x + 4.0, y, w, 24.0), Font::SmallBold, t.t3, Align::Left);
        let card = rect(x, y + 28.0, w, rows);
        c.fill_round(card, 14.0, t.card);
        c.stroke_round(card, 14.0, t.ring, 1.0);
        card
    }

    /// A row: label (and a line under it) on the left, the control drawn by `f` on the right.
    fn row_label(&mut self, c: &mut Canvas, r: Rect, label: &str, sub: Option<&str>) {
        let t = self.t.clone();
        match sub {
            Some(s) => {
                c.text(label, rect(r.x + 18.0, r.y + 10.0, r.w * 0.45, 20.0), Font::Body, t.t1, Align::Left);
                c.text(s, rect(r.x + 18.0, r.y + 30.0, r.w * 0.5, 18.0), Font::Small, t.t3, Align::Left);
            }
            None => c.text(label, rect(r.x + 18.0, r.y, r.w * 0.45, r.h), Font::Body, t.t1, Align::Left),
        }
    }

    fn seg_right(&mut self, c: &mut Canvas, r: Rect, key: &'static str, opts: &[(&'static str, &str)], current: &str) {
        let items: Vec<(String, Id, bool)> = opts.iter().map(|&(v, l)| (l.to_string(), Id::Set(key, v), v == current)).collect();
        let w: f32 = items.iter().map(|(l, _, _)| c.text_width(l, Font::BodyBold) + 26.0).sum::<f32>() + 6.0;
        self.seg(c, r.right() - 18.0 - w, r.y + (r.h - 32.0) / 2.0, 32.0, &items);
    }

    fn switch_right(&mut self, c: &mut Canvas, r: Rect, key: &'static str, on: bool) {
        self.toggle(c, rect(r.right() - 18.0 - 40.0, r.y, 40.0, r.h), on, Id::Set(key, if on { "off" } else { "on" }));
    }

    pub(super) fn paint_settings(&mut self, c: &mut Canvas, area: Rect) {
        let t = self.t.clone();
        let w = (area.w - 48.0).min(760.0);
        let x = area.x + (area.w - w) / 2.0;
        let head = rect(area.x, area.y, area.w, 64.0);
        self.icon_btn(c, rect(x - 4.0, head.y + 16.0, 32.0, 32.0), &icons::BACK, Id::SetBack);
        c.text(tr("Settings"), rect(x + 36.0, head.y, w, head.h), Font::Hero, t.t1, Align::Left);
        let body = rect(area.x, head.bottom(), area.w, area.bottom() - head.bottom());
        c.clip(body);
        let mut y = body.y + 8.0 - self.settings_scroll;
        let rh = 54.0;
        let set = self.set.clone();
        // general
        let card = self.section(c, x, y, w, tr("General"), rh * 5.0);
        let rows: Vec<Rect> = (0..5).map(|k| rect(card.x, card.y + k as f32 * rh, card.w, rh)).collect();
        for r in rows.iter().skip(1) {
            c.hline(r.x + 18.0, r.y, r.w - 36.0, t.sep);
        }
        self.row_label(c, rows[0], tr("Language"), None);
        self.seg_right(c, rows[0], "lang", &[("system", tr("System")), ("en", "English"), ("fr", "Français")], &set.lang);
        self.row_label(c, rows[1], tr("Theme"), None);
        self.seg_right(c, rows[1], "theme", &[("system", tr("System")), ("dark", tr("Dark")), ("light", tr("Light"))], &set.theme);
        self.row_label(c, rows[2], tr("First day of the week"), None);
        self.seg_right(
            c,
            rows[2],
            "first_day",
            &[("auto", tr("Automatic")), ("mon", tr("Monday")), ("sun", tr("Sunday")), ("sat", tr("Saturday"))],
            &set.first_day,
        );
        self.row_label(c, rows[3], tr("Time format"), None);
        self.seg_right(c, rows[3], "clock", &[("auto", tr("Automatic")), ("24", tr("24-hour")), ("12", tr("12-hour"))], &set.clock);
        self.row_label(c, rows[4], tr("Week numbers"), Some(tr("Show the week numbers in the month and the week.")));
        self.switch_right(c, rows[4], "week_numbers", set.week_numbers);
        y = card.bottom() + 24.0;
        // reminders
        let card = self.section(c, x, y, w, tr("Reminders"), rh * 4.0);
        let rows: Vec<Rect> = (0..4).map(|k| rect(card.x, card.y + k as f32 * rh, card.w, rh)).collect();
        for r in rows.iter().skip(1) {
            c.hline(r.x + 18.0, r.y, r.w - 36.0, t.sep);
        }
        self.row_label(c, rows[0], tr("Default reminder"), Some(tr("Given to new events.")));
        let cur = set.default_reminder.to_string();
        self.seg_right(
            c,
            rows[0],
            "default_reminder",
            &[("-1", tr("None")), ("0", "0 min"), ("5", "5 min"), ("10", "10 min"), ("30", "30 min"), ("60", "1 h")],
            &cur,
        );
        self.row_label(c, rows[1], tr("Reminder style"), None);
        self.seg_right(c, rows[1], "reminder_style", &[("popup", tr("Plume popup")), ("toast", tr("Windows notification"))], &set.reminder_style);
        self.row_label(c, rows[2], tr("Keep running near the clock"), Some(tr("Closing the window keeps Plume Calendar running for reminders.")));
        self.switch_right(c, rows[2], "background", set.background);
        self.row_label(c, rows[3], tr("Start with Windows"), Some(tr("Starts in the background, near the clock, for reminders.")));
        let auto = self.autostart;
        self.switch_right(c, rows[3], "autostart", auto);
        y = card.bottom() + 24.0;
        // about
        let card = self.section(c, x, y, w, tr("About"), 150.0);
        c.text(&format!("Plume Calendar {}", plume_update::VERSION), rect(card.x + 18.0, card.y + 14.0, w - 36.0, 22.0), Font::Title, t.t1, Align::Left);
        c.text_block(
            tr("Your agenda, offline. Only the calendars you subscribe to go online, to download them."),
            rect(card.x + 18.0, card.y + 42.0, w - 36.0, 40.0),
            Font::Body,
            t.t2,
        );
        let zone = Zone::local();
        let zname = zone.windows_name().unwrap_or_else(|| zone.tzid());
        c.text(
            &fill(tr("Time zone: {}"), &[&format!("{} ({})", zone.tzid(), zname)]),
            rect(card.x + 18.0, card.y + 80.0, w - 36.0, 20.0),
            Font::Small,
            t.t3,
            Align::Left,
        );
        let bw = c.text_width(tr("Open the data folder"), Font::BodyBold) + 30.0;
        self.btn(c, rect(card.x + 18.0, card.bottom() - 46.0, bw, 32.0), tr("Open the data folder"), 0, Id::OpenData);
        y = card.bottom() + 24.0;
        c.unclip();
        let total = y + self.settings_scroll - body.y;
        let max = (total - body.h).max(0.0);
        self.settings_scroll = self.settings_scroll.min(max);
    }
}
