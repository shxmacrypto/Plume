//! The settings page: Plume's look (section titles, cards, segmented controls, switches, pill buttons).

use super::{App, Hit};
use crate::i18n::tr;
use plume_ui::{Align, Canvas, Color, Font, Rect, icons, rect};

/// Inner padding of a card.
const PAD: f32 = 20.0;

impl App {
    /// A row: label (and a line under it) on the left, leaving `reserve` on the right for its control; returns the
    /// row for the control.
    #[allow(clippy::too_many_arguments)]
    fn row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, sub: &str, reserve: f32) -> Rect {
        let p = self.pal.clone();
        let h = if sub.is_empty() { 52.0 } else { 64.0 };
        let r = rect(x, y, w, h);
        let tw = (w - 2.0 * PAD - reserve - 16.0).max(80.0);
        let top = if sub.is_empty() { rect(x + PAD, y, tw, h) } else { rect(x + PAD, y + 10.0, tw, 22.0) };
        c.text(label, top, Font::Body, p.t1, Align::Left);
        if !sub.is_empty() {
            c.text(sub, rect(x + PAD, y + 34.0, tw, 20.0), Font::Small, p.t3, Align::Left);
        }
        r
    }

    /// Width of a segmented control (as `choices` draws it).
    fn choices_w(c: &mut Canvas, labels: &[&str]) -> f32 {
        labels.iter().map(|l| c.text_width(l, Font::SmallBold) + 26.0).sum::<f32>() + 6.0
    }

    /// An iOS-style switch, on = accent.
    fn toggle(&mut self, c: &mut Canvas, r: Rect, on: bool, name: &'static str) {
        let p = self.pal.clone();
        let sw = rect(r.right() - PAD - 46.0, r.y + (r.h - 26.0) / 2.0, 46.0, 26.0);
        c.fill_round(sw, 13.0, if on { p.accent } else { p.fill2 });
        let k = if on { sw.right() - 23.0 } else { sw.x + 3.0 };
        c.fill_round(rect(k, sw.y + 3.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
        self.hits.add(r, Hit::Toggle(name));
    }

    /// A segmented control: a pill track, the chosen segment raised.
    fn choices(&mut self, c: &mut Canvas, r: Rect, labels: &[&str], cur: usize, name: &'static str) {
        let p = self.pal.clone();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::SmallBold) + 26.0).collect();
        let total = Self::choices_w(c, labels);
        let mut x = r.right() - PAD - total + 3.0;
        let bg = rect(x - 3.0, r.y + (r.h - 34.0) / 2.0, total, 34.0);
        c.fill_round(bg, 17.0, p.fill);
        for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
            let b = rect(x, bg.y + 3.0, w, 28.0);
            let hit = Hit::Choice(name, i);
            if i == cur {
                c.fill_round(b, 14.0, p.fill2);
                c.stroke_round(b, 14.0, p.ring, 1.0);
            } else if self.hovered(&hit) {
                c.fill_round(b, 14.0, p.hover);
            }
            c.text(l, b, if i == cur { Font::SmallBold } else { Font::Small }, if i == cur { p.t1 } else { p.t2 }, Align::Center);
            self.hits.add(b, hit);
            x += w;
        }
    }

    /// A section: its title, then a card of `h` holding its rows. Returns where the rows start.
    fn card(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, h: f32) -> f32 {
        let p = self.pal.clone();
        c.text(title, rect(x + 4.0, y, w, 24.0), Font::Title, p.t1, Align::Left);
        let r = rect(x, y + 32.0, w, h);
        c.fill_round(r, 18.0, p.card);
        c.stroke_round(r, 18.0, p.line, 1.0);
        y + 32.0
    }

    fn divider(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32) {
        let line = self.pal.line;
        c.hline(x + PAD, y, w - 2.0 * PAD, line);
    }

    pub fn paint_settings(&mut self, c: &mut Canvas, body: Rect) {
        let p = self.pal.clone();
        c.fill(body, p.area);
        let w = 640.0f32.min(body.w - 48.0);
        let x = body.x + (body.w - w) / 2.0;
        let mut y = body.y + 24.0;
        let back = rect(x - 4.0, y, 36.0, 36.0);
        if self.hovered(&Hit::Btn("close_settings")) {
            c.fill_round(back, 18.0, p.hover);
        }
        c.icon(&icons::BACK, back.centered(16.0, 16.0), p.t1);
        self.hits.add(back, Hit::Btn("close_settings"));
        c.text(tr("Settings"), rect(x + 44.0, y, w - 44.0, 36.0), Font::Hero, p.t1, Align::Left);
        y += 64.0;

        // appearance
        let mut ry = self.card(c, x, y, w, tr("Appearance"), 168.0);
        let labels = [tr("System"), tr("Dark"), tr("Light")];
        let cw = Self::choices_w(c, &labels);
        let r = self.row(c, x, ry, w, tr("Theme"), "", cw);
        let cur = ["system", "dark", "light"].iter().position(|t| *t == self.set.theme).unwrap_or(0);
        self.choices(c, r, &labels, cur, "theme");
        ry += r.h;
        self.divider(c, x, ry, w);
        let labels = [tr("System"), "English", "Français"];
        let cw = Self::choices_w(c, &labels);
        let r = self.row(c, x, ry, w, tr("Language"), "", cw);
        let cur = ["system", "en", "fr"].iter().position(|t| *t == self.set.lang).unwrap_or(0);
        self.choices(c, r, &labels, cur, "lang");
        ry += r.h;
        self.divider(c, x, ry, w);
        let r = self.row(c, x, ry, w, tr("Dark pages"), tr("Shows pages in dark colors (the file is not changed)."), 46.0);
        let on = self.set.dark_pages;
        self.toggle(c, r, on, "dark_pages");
        y = ry + r.h + 28.0;

        // reading
        let mut ry = self.card(c, x, y, w, tr("Reading"), 116.0);
        let labels = [tr("Fit width"), tr("Whole page"), "100 %"];
        let cw = Self::choices_w(c, &labels);
        let r = self.row(c, x, ry, w, tr("Zoom when a file opens"), "", cw);
        let cur = ["width", "page", "100"].iter().position(|t| *t == self.set.zoom).unwrap_or(0);
        self.choices(c, r, &labels, cur, "zoom");
        ry += r.h;
        self.divider(c, x, ry, w);
        let r = self.row(c, x, ry, w, tr("Reopen files where I left them"), tr("Remembers the last page read in each file."), 46.0);
        let on = self.set.remember_page;
        self.toggle(c, r, on, "remember_page");
        y = ry + r.h + 28.0;

        // signature and default app
        let mut ry = self.card(c, x, y, w, tr("Signature and files"), 128.0);
        let has = self.sig.is_some();
        let (label, id) = if has { (tr("Forget"), "forget_signature") } else { (tr("Draw…"), "new_signature") };
        let bw = c.text_width(label, Font::BodyBold) + 36.0;
        let r = self.row(c, x, ry, w, tr("Your signature"), if has { tr("Saved on this computer only.") } else { tr("None yet.") }, bw);
        self.pill(c, rect(r.right() - PAD - bw, r.y + 14.0, bw, 36.0), label, false, Hit::Btn(id));
        ry += r.h;
        self.divider(c, x, ry, w);
        let label = tr("Default apps…");
        let bw = c.text_width(label, Font::BodyBold) + 36.0;
        let r = self.row(c, x, ry, w, tr("Open PDF files with Plume PDF"), tr("Choose it in Windows' default apps."), bw);
        self.pill(c, rect(r.right() - PAD - bw, r.y + 14.0, bw, 36.0), label, false, Hit::Btn("default_app"));
        y = ry + r.h + 24.0;

        c.text(
            &format!("Plume PDF {} · {}", plume_update::VERSION, tr("Nothing leaves your computer.")),
            rect(x + 4.0, y, w, 22.0),
            Font::Small,
            p.t3,
            Align::Left,
        );
    }
}
