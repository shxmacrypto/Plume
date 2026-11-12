//! The settings page: look, language, how files are removed, and the limits of the suggestions. Grouped in cards,
//! one setting per row: its name and a line on the left, its control on the right.

use super::{App, Hit};
use crate::{
    clean::human,
    i18n::{fill, tr},
    settings::{DUP_MB, LARGE_MB, OLD_DAYS},
};
use plume_ui::{Align, Canvas, Font, Rect, rect};

const ROW: f32 = 64.0;

impl App {
    /// A segmented control of choices, right-aligned on a settings row; `on` is the chosen one.
    fn choices(&mut self, c: &mut Canvas, right: f32, y: f32, key: &'static str, labels: &[String], on: usize) {
        let w: f32 = labels.iter().map(|l| c.text_width(l, Font::BodyBold) + 30.0).sum::<f32>() + 6.0;
        let items: Vec<(String, Hit, bool)> = labels.iter().enumerate().map(|(i, l)| (l.clone(), Hit::Choice(key, i), i == on)).collect();
        self.segmented(c, right - w, y + (ROW - 34.0) / 2.0, &items);
    }

    /// One row of a settings card: the name, a line under it, a hairline above when it is not the first.
    fn row(&mut self, c: &mut Canvas, r: Rect, title: &str, sub: &str, first: bool, control_w: f32) {
        let p = self.pal.clone();
        if !first {
            c.hline(r.x, r.y, r.w, p.line);
        }
        let text_w = r.w - control_w - 24.0;
        if sub.is_empty() {
            c.text(title, rect(r.x, r.y, text_w, ROW), Font::Body, p.t1, Align::Left);
        } else {
            c.text(title, rect(r.x, r.y + 12.0, text_w, 20.0), Font::Body, p.t1, Align::Left);
            c.text(sub, rect(r.x, r.y + 32.0, text_w, 18.0), Font::Small, p.t3, Align::Left);
        }
    }

    fn group(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, rows: usize) -> Rect {
        let p = self.pal.clone();
        c.text(title, rect(x + 4.0, y, w, 22.0), Font::Title, p.t1, Align::Left);
        let card = rect(x, y + 30.0, w, rows as f32 * ROW + 8.0);
        self.card(c, card);
        card
    }

    pub fn paint_settings(&mut self, c: &mut Canvas, body: Rect) {
        let p = self.pal.clone();
        let x0 = body.x + 32.0;
        let w = (body.w - 64.0).min(820.0);
        let y = self.heading(c, x0, body.y + 28.0, w, tr("Settings"), "");

        // look
        let card = self.group(c, x0, y, w, tr("Look"), 2);
        let (rx, rw) = (card.x + 20.0, card.w - 40.0);
        let right = card.right() - 20.0;
        let mut ry = card.y + 4.0;
        self.row(c, rect(rx, ry, rw, ROW), tr("Theme"), "", true, 300.0);
        let on = ["system", "dark", "light"].iter().position(|t| *t == self.set.theme).unwrap_or(0);
        self.choices(c, right, ry, "theme", &[tr("System").into(), tr("Dark").into(), tr("Light").into()], on);
        ry += ROW;
        self.row(c, rect(rx, ry, rw, ROW), tr("Language"), "", false, 300.0);
        let on = ["system", "en", "fr"].iter().position(|t| *t == self.set.lang).unwrap_or(0);
        self.choices(c, right, ry, "lang", &[tr("System").into(), "English".into(), "Français".into()], on);

        // cleanup
        let card = self.group(c, x0, card.bottom() + 24.0, w, tr("Cleanup"), 4);
        let mut ry = card.y + 4.0;
        self.row(c, rect(rx, ry, rw, ROW), tr("Send to the Recycle Bin"), tr("In the Recycle Bin you can still get them back until you empty it."), true, 44.0);
        let sw = self.switch(c, right - 44.0, ry + (ROW - 26.0) / 2.0, self.set.recycle);
        self.hits.add(rect(sw.x - 8.0, ry, 60.0, ROW), Hit::Toggle("recycle"));
        ry += ROW;
        self.row(c, rect(rx, ry, rw, ROW), tr("Old downloads"), tr("Not touched for at least:"), false, 420.0);
        let labels: Vec<String> = OLD_DAYS.iter().map(|d| fill(tr("{n} days"), &[&d.to_string()])).collect();
        let on = OLD_DAYS.iter().position(|d| *d == self.set.old_days).unwrap_or(1);
        self.choices(c, right, ry, "old_days", &labels, on);
        ry += ROW;
        self.row(c, rect(rx, ry, rw, ROW), tr("Large files"), tr("From this size:"), false, 340.0);
        let labels: Vec<String> = LARGE_MB.iter().map(|m| human(m * 1_000_000)).collect();
        let on = LARGE_MB.iter().position(|m| *m == self.set.large_mb).unwrap_or(1);
        self.choices(c, right, ry, "large_mb", &labels, on);
        ry += ROW;
        self.row(c, rect(rx, ry, rw, ROW), tr("Duplicate files"), tr("Looked for among files from:"), false, 340.0);
        let labels: Vec<String> = DUP_MB.iter().map(|m| human(m * 1_000_000)).collect();
        let on = DUP_MB.iter().position(|m| *m == self.set.dup_mb).unwrap_or(1);
        self.choices(c, right, ry, "dup_mb", &labels, on);

        // about
        let about = rect(x0, card.bottom() + 14.0, w, 64.0);
        self.card(c, about);
        c.text(&format!("Plume Clean {}", plume_update::VERSION), rect(about.x + 20.0, about.y + 12.0, 300.0, 20.0), Font::BodyBold, p.t1, Align::Left);
        c.text(
            tr("Nothing goes online. Scans stay on this PC and are not saved."),
            rect(about.x + 20.0, about.y + 32.0, about.w - 40.0, 20.0),
            Font::Small,
            p.t3,
            Align::Left,
        );
    }
}
