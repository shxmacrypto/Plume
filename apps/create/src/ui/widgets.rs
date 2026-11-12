//! The small controls, in the Plume look: pill buttons, icon buttons, sliders, segmented controls, switches,
//! colour swatches and fields.

use super::{App, Ct, Field, Hit, Sg, Sl, Tg};
use plume_ui::{Align, Canvas, Color, Font, Rect, icons::Icon, rect};

impl App {
    /// A rounded button: primary (accent) or quiet.
    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, primary: bool, hit: Hit) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit);
        let down = hot && self.pressed.as_ref() == Some(&hit);
        let fill = match (primary, hot, down) {
            (true, _, true) => p.accent.mix(Color::hex(0x000000), 0.14),
            (true, true, _) => p.accent.mix(Color::hex(0xffffff), 0.12),
            (true, ..) => p.accent,
            (false, _, true) => p.fill2.mix(Color::hex(0x000000), 0.1),
            (false, true, _) => p.fill2,
            (false, ..) => p.fill,
        };
        c.fill_round(r, r.h / 2.0, fill);
        c.text(label, r.inset_xy(10.0, 0.0), Font::BodyBold, if primary { p.on_accent } else { p.t1 }, Align::Center);
        self.hits.add(r, hit);
    }

    /// A pill with an icon before its label.
    pub fn pill_icon(&mut self, c: &mut Canvas, r: Rect, icon: &Icon, label: &str, primary: bool, hit: Hit) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit);
        let fill = match (primary, hot) {
            (true, true) => p.accent.mix(Color::hex(0xffffff), 0.12),
            (true, false) => p.accent,
            (false, true) => p.fill2,
            (false, false) => p.fill,
        };
        c.fill_round(r, r.h / 2.0, fill);
        let ink = if primary { p.on_accent } else { p.t1 };
        let tw = c.text_width(label, Font::BodyBold);
        let total = 16.0 + 8.0 + tw;
        let x = r.x + ((r.w - total) / 2.0).max(10.0);
        c.icon(icon, rect(x, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), ink);
        c.text(label, rect(x + 24.0, r.y, r.right() - x - 24.0 - 8.0, r.h), Font::BodyBold, ink, Align::Left);
        self.hits.add(r, hit);
    }

    /// A round icon button; `on`: shown chosen.
    pub fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, on: bool) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit);
        if on {
            c.fill_round(r, (r.h / 2.0).min(10.0), p.accent.fade(0.16));
        } else if hot {
            c.fill_round(r, (r.h / 2.0).min(10.0), p.hover);
        }
        let s = 18.0f32.min(r.w * 0.56).round();
        let q = rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s);
        c.icon(icon, q, if on { p.ink } else { p.t2 });
        self.hits.add(r, hit);
    }

    /// A label on the left and a value on the right, the track below. Returns the height used.
    pub fn slider(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, sl: Sl) -> f32 {
        let p = self.pal.clone();
        let (v, lo, hi) = self.slider_get(sl);
        let text = self.slider_text(sl, v);
        c.text(label, rect(x, y, w - 70.0, 20.0), Font::Body, p.t2, Align::Left);
        let vr = rect(x + w - 66.0, y, 66.0, 20.0);
        let editing = self.field == Some(Field::Value(sl));
        if !editing {
            let hot = self.hovered(&Hit::Field(Field::Value(sl)));
            if hot {
                c.fill_round(vr.inset_xy(0.0, 0.0), 6.0, p.hover);
            }
            c.text(&text, vr.inset_xy(4.0, 0.0), Font::Body, p.t1, Align::Right);
        } else {
            c.fill_round(vr, 6.0, p.fill.over(p.bg));
        }
        self.hits.add(vr, Hit::Field(Field::Value(sl)));
        let track = rect(x, y + 26.0, w, 18.0);
        let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
        let ty = track.y + track.h / 2.0;
        c.fill_round(rect(track.x, ty - 2.0, track.w, 4.0), 2.0, p.fill2);
        // centred sliders (−…+) fill from their zero
        let zero = if lo < 0.0 && hi > 0.0 { (-lo / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
        let (a, b) = if t >= zero { (zero, t) } else { (t, zero) };
        c.fill_round(rect(track.x + a * track.w, ty - 2.0, (b - a) * track.w, 4.0), 2.0, p.accent);
        let kx = track.x + t * track.w;
        let hot = self.hovered(&Hit::Slider(sl)) || matches!(&self.drag, Some(super::Drag::Slider { sl: s, .. }) if *s == sl);
        super::draw::circle(c, kx, ty, if hot { 8.0 } else { 7.0 }, Some(Color::hex(0xffffff)), Some((Color::hex(0x000000).fade(0.18), 1.0)));
        self.hits.add(track.inset_xy(-6.0, 0.0), Hit::Slider(sl));
        48.0
    }

    /// A pill track with segments; `sel`: the chosen one.
    pub fn segmented(&mut self, c: &mut Canvas, r: Rect, items: &[&str], sel: usize, sg: Sg) {
        let p = self.pal.clone();
        c.fill_round(r, r.h / 2.0, p.fill);
        let w = r.w / items.len() as f32;
        for (i, label) in items.iter().enumerate() {
            let s = rect(r.x + i as f32 * w, r.y, w, r.h).inset(2.0);
            let hit = Hit::Seg(sg, i);
            if i == sel {
                c.fill_round(s, s.h / 2.0, p.fill2.over(p.solid));
                c.stroke_round(s, s.h / 2.0, p.ring, 1.0);
            } else if self.hovered(&hit) {
                c.fill_round(s, s.h / 2.0, p.hover);
            }
            c.text(label, s.inset_xy(4.0, 0.0), if i == sel { Font::BodyBold } else { Font::Body }, if i == sel { p.t1 } else { p.t2 }, Align::Center);
            self.hits.add(s, hit);
        }
    }

    /// Segments showing icons.
    pub fn segmented_icons(&mut self, c: &mut Canvas, r: Rect, items: &[&Icon], sel: usize, sg: Sg) {
        let p = self.pal.clone();
        c.fill_round(r, r.h / 2.0, p.fill);
        let w = r.w / items.len() as f32;
        for (i, ic) in items.iter().enumerate() {
            let s = rect(r.x + i as f32 * w, r.y, w, r.h).inset(2.0);
            let hit = Hit::Seg(sg, i);
            if i == sel {
                c.fill_round(s, s.h / 2.0, p.fill2.over(p.solid));
                c.stroke_round(s, s.h / 2.0, p.ring, 1.0);
            } else if self.hovered(&hit) {
                c.fill_round(s, s.h / 2.0, p.hover);
            }
            let q = s.centered(16.0, 16.0);
            c.icon(ic, q, if i == sel { p.ink } else { p.t2 });
            self.hits.add(s, hit);
        }
    }

    /// An iOS-style switch with its label on the left.
    pub fn switch(&mut self, c: &mut Canvas, row: Rect, label: &str, on: bool, tg: Tg) -> f32 {
        let p = self.pal.clone();
        let (x, y, w) = (row.x, row.y, row.w);
        c.text(label, rect(x, y, w - 56.0, 30.0), Font::Body, p.t2, Align::Left);
        let s = rect(x + w - 44.0, y + 4.0, 44.0, 22.0);
        c.fill_round(s, 11.0, if on { p.accent } else { p.fill2 });
        let kx = if on { s.right() - 11.0 } else { s.x + 11.0 };
        super::draw::circle(c, kx, s.y + 11.0, 9.0, Some(Color::hex(0xffffff)), None);
        self.hits.add(row, Hit::Toggle(tg));
        34.0
    }

    /// A colour swatch (checkerboard under see-through colours).
    pub fn swatch(&mut self, c: &mut Canvas, r: Rect, colour: crate::engine::doc::Rgba, ct: Ct) {
        let p = self.pal.clone();
        if colour.0[3] < 255 {
            super::draw::checker(c, r.inset(1.0), 5.0, false);
        }
        let f = colour.f();
        c.fill_round(r, 8.0, Color { r: f[0], g: f[1], b: f[2], a: f[3] });
        c.stroke_round(r, 8.0, if self.hovered(&Hit::Swatch(ct)) { p.t3 } else { p.line.mix(p.t4, 0.5) }, 1.0);
        self.hits.add(r, Hit::Swatch(ct));
    }

    /// A field showing a value; a click edits it.
    pub fn field_box(&mut self, c: &mut Canvas, r: Rect, text: &str, f: Field) {
        let p = self.pal.clone();
        let editing = self.field.as_ref() == Some(&f);
        c.fill_round(r, 10.0, p.fill.over(p.bg));
        if editing {
            c.stroke_round(r, 10.0, p.accent, 1.5);
        } else {
            if self.hovered(&Hit::Field(f.clone())) {
                c.stroke_round(r, 10.0, p.t4, 1.0);
            }
            c.text(text, r.inset_xy(10.0, 0.0), Font::Body, p.t1, Align::Left);
        }
        self.hits.add(r, Hit::Field(f));
    }

    pub fn section(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        c.text(title, rect(x, y, w, 22.0), Font::Title, self.pal.t1, Align::Left);
        30.0
    }
}
