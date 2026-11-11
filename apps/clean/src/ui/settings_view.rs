//! The settings page: look, language, how files are removed, and the limits of the suggestions.

use super::{App, Hit};
use crate::{
    clean::human,
    i18n::{fill, tr},
    settings::{DUP_MB, LARGE_MB, OLD_DAYS},
};
use plume_ui::{Align, Canvas, Font, Rect, rect};

impl App {
    /// A row of choices; `on` is the chosen one.
    fn choices(&mut self, c: &mut Canvas, x: f32, y: f32, key: &'static str, labels: &[String], on: usize) {
        let p = self.pal.clone();
        let mut xx = x;
        for (i, l) in labels.iter().enumerate() {
            let w = c.text_width(l, Font::BodyBold) + 30.0;
            let r = rect(xx, y, w, 32.0);
            let hit = Hit::Choice(key, i);
            let fill = if i == on {
                p.accent.fade(if p.dark { 0.24 } else { 0.16 })
            } else if self.hovered(&hit) {
                p.t1.fade(0.12)
            } else {
                p.t1.fade(0.06)
            };
            c.fill_round(r, 16.0, fill);
            c.text(l, r, Font::BodyBold, if i == on { p.t1 } else { p.t2 }, Align::Center);
            self.hits.add(r, hit);
            xx += w + 8.0;
        }
    }

    fn section(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, sub: &str) -> f32 {
        let p = self.pal.clone();
        c.text(title, rect(x, y, w, 22.0), Font::Title, p.t1, Align::Left);
        if !sub.is_empty() {
            c.text(sub, rect(x, y + 22.0, w, 20.0), Font::Small, p.t3, Align::Left);
            return y + 50.0;
        }
        y + 30.0
    }

    pub fn paint_settings(&mut self, c: &mut Canvas, body: Rect) {
        let p = self.pal.clone();
        let x0 = body.x + 32.0;
        let w = (body.w - 64.0).min(820.0);
        let mut y = self.heading(c, x0, body.y + 28.0, w, tr("Settings"), "");
        let card = rect(x0, y, w, 470.0);
        c.fill_round(card, 16.0, p.card);
        let x = x0 + 24.0;
        let iw = w - 48.0;
        y += 20.0;

        y = self.section(c, x, y, iw, tr("Theme"), "");
        let on = ["system", "dark", "light"].iter().position(|t| *t == self.set.theme).unwrap_or(0);
        self.choices(c, x, y, "theme", &[tr("System").into(), tr("Dark").into(), tr("Light").into()], on);
        y += 50.0;

        y = self.section(c, x, y, iw, tr("Language"), "");
        let on = ["system", "en", "fr"].iter().position(|t| *t == self.set.lang).unwrap_or(0);
        self.choices(c, x, y, "lang", &[tr("System").into(), "English".into(), "Français".into()], on);
        y += 50.0;

        y = self.section(c, x, y, iw, tr("Removed files"), tr("In the Recycle Bin you can still get them back until you empty it."));
        let r = rect(x, y, 44.0, 26.0);
        let on = self.set.recycle;
        c.fill_round(r, 13.0, if on { p.accent } else { p.t1.fade(0.18) });
        let knob = if on { r.right() - 23.0 } else { r.x + 3.0 };
        c.fill_round(rect(knob, r.y + 3.0, 20.0, 20.0), 10.0, plume_ui::Color::hex(0xffffff));
        c.text(tr("Send to the Recycle Bin"), rect(r.right() + 12.0, y, 300.0, 26.0), Font::Body, p.t1, Align::Left);
        self.hits.add(rect(x, y, 340.0, 26.0), Hit::Toggle("recycle"));
        y += 48.0;

        y = self.section(c, x, y, iw, tr("Old downloads"), tr("Not touched for at least:"));
        let labels: Vec<String> = OLD_DAYS.iter().map(|d| fill(tr("{n} days"), &[&d.to_string()])).collect();
        let on = OLD_DAYS.iter().position(|d| *d == self.set.old_days).unwrap_or(1);
        self.choices(c, x, y, "old_days", &labels, on);
        y += 50.0;

        y = self.section(c, x, y, iw, tr("Large files and duplicates"), tr("Large files from, and duplicates looked for from:"));
        let labels: Vec<String> = LARGE_MB.iter().map(|m| human(m * 1_000_000)).collect();
        let on = LARGE_MB.iter().position(|m| *m == self.set.large_mb).unwrap_or(1);
        self.choices(c, x, y, "large_mb", &labels, on);
        let labels: Vec<String> = DUP_MB.iter().map(|m| human(m * 1_000_000)).collect();
        let on = DUP_MB.iter().position(|m| *m == self.set.dup_mb).unwrap_or(1);
        self.choices(c, x + iw / 2.0, y, "dup_mb", &labels, on);

        let about = rect(x0, card.bottom() + 16.0, w, 60.0);
        c.fill_round(about, 16.0, p.card);
        c.text(&format!("Plume Clean {}", plume_update::VERSION), rect(about.x + 24.0, about.y + 8.0, 300.0, 22.0), Font::BodyBold, p.t1, Align::Left);
        c.text(
            tr("Nothing goes online. Scans stay on this PC and are not saved."),
            rect(about.x + 24.0, about.y + 30.0, about.w - 48.0, 20.0),
            Font::Small,
            p.t3,
            Align::Left,
        );
    }
}
