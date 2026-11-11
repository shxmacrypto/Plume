//! The settings page.

use super::{App, Hit, draw};
use crate::i18n::tr;
use plume_ui::{Align, Canvas, Color, Font, Rect, icons, rect};

impl App {
    /// A row: label (and a line under it) on the left; returns the row for the control on the right.
    fn row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, sub: &str) -> Rect {
        let p = self.pal.clone();
        let h = if sub.is_empty() { 52.0 } else { 64.0 };
        let r = rect(x, y, w, h);
        c.text(
            label,
            rect(x + 18.0, y + if sub.is_empty() { 0.0 } else { 10.0 }, w * 0.5, if sub.is_empty() { h } else { 22.0 }),
            Font::Body,
            p.t1,
            Align::Left,
        );
        if !sub.is_empty() {
            c.text(sub, rect(x + 18.0, y + 34.0, w * 0.55, 20.0), Font::Small, p.t3, Align::Left);
        }
        r
    }

    fn toggle(&mut self, c: &mut Canvas, r: Rect, on: bool, name: &'static str) {
        let p = self.pal.clone();
        let sw = rect(r.right() - 66.0, r.y + (r.h - 26.0) / 2.0, 46.0, 26.0);
        c.fill_round(sw, 13.0, if on { p.accent } else { p.t1.fade(0.2) });
        let k = if on { sw.right() - 23.0 } else { sw.x + 3.0 };
        c.fill_round(rect(k, sw.y + 3.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
        self.hits.add(r, Hit::Toggle(name));
    }

    fn choices(&mut self, c: &mut Canvas, r: Rect, labels: &[&str], cur: usize, name: &'static str) {
        let p = self.pal.clone();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::Small) + 26.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 6.0;
        let mut x = r.right() - 18.0 - total + 3.0;
        let bg = rect(x - 3.0, r.y + (r.h - 34.0) / 2.0, total, 34.0);
        c.fill_round(bg, 17.0, p.t1.fade(0.07));
        for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
            let b = rect(x, bg.y + 3.0, w, 28.0);
            let hit = Hit::Choice(name, i);
            if i == cur {
                c.fill_round(b, 14.0, p.accent);
            } else if self.hovered(&hit) {
                c.fill_round(b, 14.0, p.t1.fade(0.08));
            }
            c.text(l, b, Font::Small, if i == cur { p.on_accent } else { p.t1 }, Align::Center);
            self.hits.add(b, hit);
            x += w;
        }
    }

    /// A group of rows on a card.
    fn card(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, h: f32) -> f32 {
        let p = self.pal.clone();
        c.text(title, rect(x + 4.0, y, w, 24.0), Font::SmallBold, p.t3, Align::Left);
        let r = rect(x, y + 28.0, w, h);
        c.fill_round(r, 14.0, p.card);
        c.stroke_round(r, 14.0, p.line, 1.0);
        y + 28.0
    }

    pub fn paint_settings(&mut self, c: &mut Canvas, body: Rect) {
        let p = self.pal.clone();
        c.fill(body, p.area);
        let w = 640.0f32.min(body.w - 48.0);
        let x = body.x + (body.w - w) / 2.0;
        let mut y = body.y + 24.0;
        let back = rect(x - 4.0, y, 36.0, 36.0);
        if self.hovered(&Hit::Btn("close_settings")) {
            c.fill_round(back, 18.0, p.t1.fade(0.08));
        }
        c.icon(&icons::BACK, back.centered(16.0, 16.0), p.t1);
        self.hits.add(back, Hit::Btn("close_settings"));
        draw::text(c, tr("Settings"), rect(x + 40.0, y, w - 40.0, 36.0), 24.0, 700, p.t1, Align::Left);
        y += 60.0;

        // appearance
        let mut ry = self.card(c, x, y, w, tr("Appearance"), 168.0);
        let r = self.row(c, x, ry, w, tr("Theme"), "");
        let cur = ["system", "dark", "light"].iter().position(|t| *t == self.set.theme).unwrap_or(0);
        self.choices(c, r, &[tr("System"), tr("Dark"), tr("Light")], cur, "theme");
        ry += r.h;
        c.hline(x + 18.0, ry, w - 36.0, p.line);
        let r = self.row(c, x, ry, w, tr("Language"), "");
        let cur = ["system", "en", "fr"].iter().position(|t| *t == self.set.lang).unwrap_or(0);
        self.choices(c, r, &[tr("System"), "English", "Français"], cur, "lang");
        ry += r.h;
        c.hline(x + 18.0, ry, w - 36.0, p.line);
        let r = self.row(c, x, ry, w, tr("Dark pages"), tr("Shows pages in dark colors (the file is not changed)."));
        let on = self.set.dark_pages;
        self.toggle(c, r, on, "dark_pages");
        y = ry + r.h + 24.0;

        // reading
        let mut ry = self.card(c, x, y, w, tr("Reading"), 116.0);
        let r = self.row(c, x, ry, w, tr("Zoom when a file opens"), "");
        let cur = ["width", "page", "100"].iter().position(|t| *t == self.set.zoom).unwrap_or(0);
        self.choices(c, r, &[tr("Fit width"), tr("Whole page"), "100 %"], cur, "zoom");
        ry += r.h;
        c.hline(x + 18.0, ry, w - 36.0, p.line);
        let r = self.row(c, x, ry, w, tr("Reopen files where I left them"), tr("Remembers the last page read in each file."));
        let on = self.set.remember_page;
        self.toggle(c, r, on, "remember_page");
        y = ry + r.h + 24.0;

        // signature and default app
        let mut ry = self.card(c, x, y, w, tr("Signature and files"), 128.0);
        let has = self.sig.is_some();
        let r = self.row(c, x, ry, w, tr("Your signature"), if has { tr("Saved on this computer only.") } else { tr("None yet.") });
        let (label, id) = if has { (tr("Forget"), "forget_signature") } else { (tr("Draw…"), "new_signature") };
        let bw = c.text_width(label, Font::BodyBold) + 32.0;
        self.pill(c, rect(r.right() - 18.0 - bw, r.y + 14.0, bw, 36.0), label, false, Hit::Btn(id));
        ry += r.h;
        c.hline(x + 18.0, ry, w - 36.0, p.line);
        let r = self.row(c, x, ry, w, tr("Open PDF files with Plume PDF"), tr("Choose it in Windows' default apps."));
        let label = tr("Default apps…");
        let bw = c.text_width(label, Font::BodyBold) + 32.0;
        self.pill(c, rect(r.right() - 18.0 - bw, r.y + 14.0, bw, 36.0), label, false, Hit::Btn("default_app"));
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
