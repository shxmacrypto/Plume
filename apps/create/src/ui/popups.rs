//! Lists and cards over the window: the app menu, blend modes, fonts, the colour picker, and the dialogs
//! (export, new size, canvas size, shortcuts, settings).

use super::{Act, App, Ct, Dialog, Field, Hit, Popup, Sg, Sl, draw};
use crate::{
    engine::{doc::Rgba, templates::PALETTES},
    i18n::tr,
    wic::Format,
};
use plume_ui::{Align, Canvas, Color, Font, PaintCx, Rect, rect};
use windows::{
    Win32::Graphics::{
        Direct2D::{Common::D2D1_COLOR_F, D2D1_DRAW_TEXT_OPTIONS},
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
        },
    },
    core::{PCWSTR, w},
};

const FONT_ROW: f32 = 32.0;

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

fn rgb_to_hsv(c: [f32; 3]) -> (f32, f32, f32) {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let d = mx - mn;
    let h = if d <= 1e-6 {
        0.0
    } else if mx == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0) / 6.0
    } else if mx == c[1] {
        ((c[2] - c[0]) / d + 2.0) / 6.0
    } else {
        ((c[0] - c[1]) / d + 4.0) / 6.0
    };
    (h, if mx > 0.0 { d / mx } else { 0.0 }, mx)
}

fn to_rgba(h: f32, s: f32, v: f32, a: f32) -> Rgba {
    let c = hsv_to_rgb(h, s, v);
    let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgba([q(c[0]), q(c[1]), q(c[2]), q(a)])
}

fn col(c: Rgba) -> Color {
    let f = c.f();
    Color { r: f[0], g: f[1], b: f[2], a: f[3] }
}

impl App {
    pub fn open_colour(&mut self, target: Ct, anchor: Rect) {
        let c = self.colour_get(target);
        let f = c.f();
        let (h, s, v) = rgb_to_hsv([f[0], f[1], f[2]]);
        if target != Ct::Brush && self.field_before.is_none() {
            self.field_before = Some(self.doc.clone());
        }
        self.popup = Some(Popup::Colour { target, anchor, h, s, v, a: f[3] });
    }

    /// Sets the picker's colour (and what it edits).
    pub fn picker_set(&mut self, c: Rgba) {
        let Some(Popup::Colour { target, h, s, v, a, .. }) = &mut self.popup else { return };
        let f = c.f();
        let (nh, ns, nv) = rgb_to_hsv([f[0], f[1], f[2]]);
        // keep the hue when the colour has none (grey)
        if ns > 0.001 && nv > 0.001 {
            *h = nh;
        }
        *s = ns;
        *v = nv;
        *a = f[3];
        let t = *target;
        self.colour_set(t, c);
    }

    pub fn pick_at(&mut self, p: plume_ui::Pt) {
        let drag = match &self.drag {
            Some(super::Drag::PickSv(r)) => (0, *r),
            Some(super::Drag::PickHue(r)) => (1, *r),
            Some(super::Drag::PickAlpha(r)) => (2, *r),
            _ => return,
        };
        let Some(Popup::Colour { target, h, s, v, a, .. }) = &mut self.popup else { return };
        let r = drag.1;
        let tx = ((p.x - r.x) / r.w).clamp(0.0, 1.0);
        let ty = ((p.y - r.y) / r.h).clamp(0.0, 1.0);
        match drag.0 {
            0 => {
                *s = tx;
                *v = 1.0 - ty;
            }
            1 => *h = tx.min(0.999),
            _ => *a = tx,
        }
        let c = to_rgba(*h, *s, *v, *a);
        let t = *target;
        self.colour_set(t, c);
    }

    fn popup_card(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.fill_round(r.offset(0.0, 6.0), 16.0, Color::hex(0x000000).fade(if p.dark { 0.35 } else { 0.12 }));
        c.fill_round(r, 16.0, p.solid);
        c.stroke_round(r, 16.0, p.ring, 1.0);
        self.hits.add(r, Hit::Popup);
    }

    pub fn paint_popup(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(popup) = self.popup.clone() else { return };
        let p = self.pal.clone();
        let (w, h) = (cx.w, cx.h);
        match popup {
            Popup::Menu(menu) => {
                const ROW: f32 = 36.0;
                let mw = menu.items.iter().map(|(l, _, _)| c.text_width(l, Font::Body)).fold(0.0f32, f32::max).max(160.0) + 56.0;
                let mh = menu.items.len() as f32 * ROW + 12.0;
                let x = (menu.anchor.right() - mw).clamp(12.0, w - mw - 12.0);
                let y = (menu.anchor.bottom() + 4.0).min(h - mh - 12.0);
                let r = rect(x, y, mw, mh);
                self.popup_card(c, r);
                for (i, (label, on, _)) in menu.items.iter().enumerate() {
                    let ir = rect(r.x + 6.0, r.y + 6.0 + i as f32 * ROW, r.w - 12.0, ROW);
                    let hit = Hit::MenuItem(i);
                    if self.hover.as_ref() == Some(&hit) {
                        c.fill_round(ir, 10.0, p.hover);
                    }
                    if *on {
                        c.icon(&plume_ui::icons::CHECK, rect(ir.x + 10.0, ir.y + 11.0, 14.0, 14.0), p.ink);
                    }
                    c.text(label, rect(ir.x + 32.0, ir.y, ir.w - 40.0, ir.h), Font::Body, p.t1, Align::Left);
                    self.hits.add(ir, hit);
                }
            }
            Popup::Blend(anchor) => {
                const ROW: f32 = 30.0;
                let modes = Self::blend_menu_items();
                let cur = self.primary().map(|l| l.blend);
                let mh = modes.len() as f32 * ROW + 12.0;
                let mw = anchor.w.max(200.0);
                let x = (anchor.x).clamp(12.0, w - mw - 12.0);
                let y = if anchor.bottom() + 4.0 + mh < h - 12.0 { anchor.bottom() + 4.0 } else { (h - mh - 12.0).max(plume_ui::m::TITLE_H + 4.0) };
                let r = rect(x, y, mw, mh);
                self.popup_card(c, r);
                for (i, b) in modes.iter().enumerate() {
                    let ir = rect(r.x + 6.0, r.y + 6.0 + i as f32 * ROW, r.w - 12.0, ROW);
                    let hit = Hit::MenuItem(i);
                    if self.hover.as_ref() == Some(&hit) {
                        c.fill_round(ir, 9.0, p.hover);
                    }
                    if cur == Some(*b) {
                        c.icon(&plume_ui::icons::CHECK, rect(ir.x + 10.0, ir.y + 8.0, 14.0, 14.0), p.ink);
                    }
                    c.text(tr(b.label()), rect(ir.x + 32.0, ir.y, ir.w - 40.0, ir.h), Font::Body, p.t1, Align::Left);
                    self.hits.add(ir, hit);
                }
            }
            Popup::Fonts { anchor, scroll, .. } => {
                let mw = anchor.w.max(260.0);
                let mh = 440.0f32.min(h - 80.0);
                let x = anchor.x.clamp(12.0, w - mw - 12.0);
                let y = if anchor.bottom() + 4.0 + mh < h - 12.0 { anchor.bottom() + 4.0 } else { (anchor.y - mh - 4.0).max(52.0) };
                let r = rect(x, y, mw, mh);
                self.popup_card(c, r);
                let search = rect(r.x + 10.0, r.y + 10.0, r.w - 20.0, 34.0);
                let editing = self.field == Some(Field::FontSearch);
                c.fill_round(search, 10.0, p.fill.over(p.solid));
                if !editing {
                    c.icon(&plume_ui::icons::SEARCH, rect(search.x + 10.0, search.y + 9.0, 16.0, 16.0), p.t3);
                    c.text(tr("Search fonts"), rect(search.x + 32.0, search.y, search.w - 40.0, search.h), Font::Body, p.t3, Align::Left);
                } else {
                    c.stroke_round(search, 10.0, p.accent, 1.5);
                }
                self.hits.add(search, Hit::Field(Field::FontSearch));
                let list = rect(r.x + 6.0, search.bottom() + 6.0, r.w - 12.0, r.bottom() - search.bottom() - 12.0);
                let fonts = self.font_list();
                let max = (fonts.len() as f32 * FONT_ROW - list.h).max(0.0);
                let scroll = scroll.min(max);
                if let Some(Popup::Fonts { scroll: s, .. }) = &mut self.popup {
                    *s = scroll;
                }
                let cur = self.primary().and_then(|l| if let crate::engine::doc::Kind::Text(t) = &l.kind { Some(t.font.clone()) } else { None });
                c.clip(list);
                let first = (scroll / FONT_ROW).floor() as usize;
                let n = (list.h / FONT_ROW).ceil() as usize + 1;
                for (i, name) in fonts.iter().enumerate().skip(first).take(n) {
                    let ir = rect(list.x, list.y + i as f32 * FONT_ROW - scroll, list.w, FONT_ROW);
                    let hit = Hit::FontItem(i);
                    if self.hover.as_ref() == Some(&hit) {
                        c.fill_round(ir, 9.0, p.hover);
                    }
                    if cur.as_deref() == Some(name.as_str()) {
                        c.icon(&plume_ui::icons::CHECK, rect(ir.x + 8.0, ir.y + 9.0, 14.0, 14.0), p.ink);
                    }
                    self.font_text(c, name, rect(ir.x + 30.0, ir.y, ir.w - 36.0, ir.h), p.t1);
                    self.hits.add(ir, hit);
                }
                c.unclip();
                if fonts.is_empty() {
                    c.text(tr("No font matches"), list, Font::Body, p.t3, Align::Center);
                }
            }
            Popup::Colour { target, anchor, h: hue, s, v, a } => {
                let mw = 268.0;
                let x = if anchor.x - mw - 12.0 > 12.0 { anchor.x - mw - 12.0 } else { (anchor.right() + 12.0).min(w - mw - 12.0) };
                let mh = 448.0;
                let y = (anchor.y - 20.0).clamp(52.0, (h - mh - 12.0).max(52.0));
                let r = rect(x, y, mw, mh);
                self.popup_card(c, r);
                let inner = r.inset(16.0);
                // saturation and value
                let sv = rect(inner.x, inner.y, inner.w, 150.0);
                let base = hsv_to_rgb(hue, 1.0, 1.0);
                c.fill_round(sv, 8.0, Color { r: base[0], g: base[1], b: base[2], a: 1.0 });
                self.gradient_h(c, sv, Color::hex(0xffffff), Color::hex(0xffffff).fade(0.0));
                c.gradient_v(sv, Color::hex(0x000000).fade(0.0), Color::hex(0x000000));
                c.stroke_round(sv, 8.0, p.line, 1.0);
                draw::circle(c, sv.x + s * sv.w, sv.y + (1.0 - v) * sv.h, 7.0, None, Some((Color::hex(0xffffff), 2.0)));
                draw::circle(c, sv.x + s * sv.w, sv.y + (1.0 - v) * sv.h, 8.5, None, Some((Color::hex(0x000000).fade(0.4), 1.0)));
                self.hits.add(sv, Hit::PickSv);
                // hue
                let hb = rect(inner.x, sv.bottom() + 14.0, inner.w, 14.0);
                let n = 36;
                for k in 0..n {
                    let c0 = hsv_to_rgb(k as f32 / n as f32, 1.0, 1.0);
                    c.fill(rect(hb.x + hb.w * k as f32 / n as f32, hb.y, hb.w / n as f32 + 0.6, hb.h), Color { r: c0[0], g: c0[1], b: c0[2], a: 1.0 });
                }
                c.stroke_round(hb, 3.0, p.line, 1.0);
                draw::circle(c, hb.x + hue * hb.w, hb.y + hb.h / 2.0, 8.0, Some(Color::hex(0xffffff)), Some((Color::hex(0x000000).fade(0.3), 1.0)));
                self.hits.add(hb.inset_xy(0.0, -4.0), Hit::PickHue);
                // opacity
                let ab = rect(inner.x, hb.bottom() + 14.0, inner.w, 14.0);
                draw::checker(c, ab, 7.0, false);
                let cur = to_rgba(hue, s, v, 1.0);
                self.gradient_h(c, ab, col(cur).fade(0.0), col(cur));
                c.stroke_round(ab, 3.0, p.line, 1.0);
                draw::circle(c, ab.x + a * ab.w, ab.y + ab.h / 2.0, 8.0, Some(Color::hex(0xffffff)), Some((Color::hex(0x000000).fade(0.3), 1.0)));
                self.hits.add(ab.inset_xy(0.0, -4.0), Hit::PickAlpha);
                // hex
                let hy = ab.bottom() + 14.0;
                let now = to_rgba(hue, s, v, a);
                c.fill_round(rect(inner.x, hy, 34.0, 34.0), 9.0, col(now));
                c.stroke_round(rect(inner.x, hy, 34.0, 34.0), 9.0, p.line, 1.0);
                let hexr = rect(inner.x + 42.0, hy, inner.w - 42.0 - 64.0, 34.0);
                let editing = self.field == Some(Field::Hex);
                c.fill_round(hexr, 10.0, p.fill.over(p.solid));
                if editing {
                    c.stroke_round(hexr, 10.0, p.accent, 1.5);
                } else {
                    c.text(&now.to_hex(), hexr.inset_xy(10.0, 0.0), Font::Body, p.t1, Align::Left);
                }
                self.hits.add(hexr, Hit::Field(Field::Hex));
                c.text(&format!("{} %", (a * 100.0).round()), rect(hexr.right(), hy, 64.0, 34.0), Font::Body, p.t2, Align::Right);
                // palettes and the colours used lately
                let mut py = hy + 48.0;
                let sw = (inner.w - 5.0 * 6.0) / 6.0;
                let recent: Vec<Rgba> = self.set.colours.iter().filter_map(|h| Rgba::parse(h)).take(6).collect();
                if !recent.is_empty() {
                    c.text(tr("Recent"), rect(inner.x, py, inner.w, 18.0), Font::Small, p.t3, Align::Left);
                    py += 20.0;
                    for (i, rc) in recent.iter().enumerate() {
                        let q = rect(inner.x + i as f32 * (sw + 6.0), py, sw, 20.0);
                        c.fill_round(q, 6.0, col(*rc));
                        c.stroke_round(q, 6.0, p.line, 1.0);
                        self.hits.add(q, Hit::PaletteColour(*rc));
                    }
                    py += 28.0;
                }
                let rows = ((r.bottom() - 12.0 - py) / 22.0).floor().max(0.0) as usize;
                for (_, pal) in PALETTES.iter().take(rows) {
                    for (i, hex) in pal.iter().enumerate() {
                        let q = rect(inner.x + i as f32 * (sw + 6.0), py, sw, 16.0);
                        let rc = Rgba::hex(*hex);
                        c.fill_round(q, 5.0, col(rc));
                        c.stroke_round(q, 5.0, p.line, 1.0);
                        self.hits.add(q, Hit::PaletteColour(rc));
                    }
                    py += 22.0;
                }
                let _ = target;
            }
        }
    }

    fn gradient_h(&mut self, c: &mut Canvas, r: Rect, a: Color, b: Color) {
        let n = 32;
        for k in 0..n {
            let t = (k as f32 + 0.5) / n as f32;
            c.fill(rect(r.x + r.w * k as f32 / n as f32, r.y, r.w / n as f32 + 0.6, r.h), a.mix(b, t));
        }
    }

    /// A font's name written in that font.
    fn font_text(&mut self, c: &mut Canvas, family: &str, r: Rect, color: Color) {
        if !self.font_formats.contains_key(family) {
            let wide: Vec<u16> = family.encode_utf16().chain(Some(0)).collect();
            let f = unsafe {
                c.gfx.dw.CreateTextFormat(
                    PCWSTR(wide.as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    15.0,
                    w!("en-us"),
                )
            };
            let Ok(f) = f else {
                c.text(family, r, Font::Body, color, Align::Left);
                return;
            };
            unsafe {
                let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                if let Ok(sign) = c.gfx.dw.CreateEllipsisTrimmingSign(&f) {
                    let _ = f.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 }, &sign);
                }
            }
            if self.font_formats.len() > 400 {
                self.font_formats.clear();
            }
            self.font_formats.insert(family.to_string(), f);
        }
        let f = self.font_formats.get(family).cloned().unwrap();
        let rt = c.target();
        let text: Vec<u16> = family.encode_utf16().collect();
        unsafe {
            if let Ok(b) = rt.CreateSolidColorBrush(&D2D1_COLOR_F { r: color.r, g: color.g, b: color.b, a: color.a }, None) {
                rt.DrawText(&text, &f, &draw::rf(r), &b, D2D1_DRAW_TEXT_OPTIONS(2 | 4), DWRITE_MEASURING_MODE_NATURAL);
            }
        }
    }

    pub fn paint_dialog(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(d) = self.dialog.clone() else { return };
        let p = self.pal.clone();
        let (w, h) = (cx.w, cx.h);
        c.fill(rect(0.0, 0.0, w, h), Color::hex(0x000000).fade(if p.dark { 0.5 } else { 0.32 }));
        self.hits.add(rect(0.0, plume_ui::m::TITLE_H, w, h), Hit::Scrim);
        let (dw, dh) = match &d {
            Dialog::Export { format, .. } => (440.0, if *format == Format::Png { 330.0 } else { 400.0 }),
            Dialog::Custom { .. } | Dialog::Canvas { .. } => (400.0, 236.0),
            Dialog::Shortcuts => (640.0, 560.0f32.min(h - 80.0)),
            Dialog::Settings => (420.0, 260.0),
        };
        let r = rect((w - dw) / 2.0, ((h - dh) / 2.0).max(plume_ui::m::TITLE_H + 8.0), dw, dh);
        c.fill_round(r.offset(0.0, 8.0), 20.0, Color::hex(0x000000).fade(0.25));
        c.fill_round(r, 20.0, p.solid);
        c.stroke_round(r, 20.0, p.ring, 1.0);
        self.hits.add(r, Hit::Dialog);
        let x = r.x + 24.0;
        let iw = r.w - 48.0;
        let mut y = r.y + 22.0;
        let buttons = |app: &mut App, c: &mut Canvas, primary: &str, act: Act| {
            let by = r.bottom() - 24.0 - 36.0;
            let pr = rect(r.right() - 24.0 - 128.0, by, 128.0, 36.0);
            app.pill(c, pr, primary, true, Hit::Act(act));
            app.pill(c, rect(pr.x - 8.0 - 104.0, by, 104.0, 36.0), tr("Cancel"), false, Hit::Act(Act::CloseDialog));
        };
        match d {
            Dialog::Export { format, w: ew, h: eh, .. } => {
                c.text(tr("Export"), rect(x, y, iw, 24.0), Font::Large, p.t1, Align::Left);
                y += 40.0;
                let fi = match format {
                    Format::Png => 0,
                    Format::Jpeg => 1,
                    Format::Webp => 2,
                };
                self.segmented(c, rect(x, y, iw, 34.0), &["PNG", "JPEG", "WebP"], fi, Sg::Format);
                y += 48.0;
                c.text(tr("Size in pixels"), rect(x, y, iw, 20.0), Font::Body, p.t2, Align::Left);
                y += 26.0;
                let fw = (iw - 30.0) / 2.0;
                let we = self.field == Some(Field::ExportW);
                let he = self.field == Some(Field::ExportH);
                let (ews, ehs) = (ew.to_string(), eh.to_string());
                self.field_box(c, rect(x, y, fw, 34.0), if we { "" } else { &ews }, Field::ExportW);
                c.text("×", rect(x + fw, y, 30.0, 34.0), Font::Body, p.t3, Align::Center);
                self.field_box(c, rect(x + fw + 30.0, y, fw, 34.0), if he { "" } else { &ehs }, Field::ExportH);
                y += 44.0;
                let si = if ew * 2 == self.doc.w {
                    0
                } else if ew == self.doc.w {
                    1
                } else if ew == self.doc.w * 2 {
                    2
                } else {
                    3
                };
                self.segmented(c, rect(x, y, iw, 32.0), &["50 %", "100 %", "200 %"], si, Sg::ExportSize);
                y += 46.0;
                if format != Format::Png {
                    y += self.slider(c, x, y, iw, tr("Quality"), Sl::ExportQuality);
                    let note = if format == Format::Webp {
                        tr("WebP is written without loss; a lower quality rounds the colours a little for a smaller file.")
                    } else {
                        tr("JPEG has no transparency: see-through parts become white.")
                    };
                    c.text(note, rect(x, y - 4.0, iw, 18.0), Font::Small, p.t3, Align::Left);
                } else {
                    c.text(tr("PNG keeps transparency."), rect(x, y - 6.0, iw, 18.0), Font::Small, p.t3, Align::Left);
                }
                buttons(self, c, tr("Export…"), Act::ExportNow);
            }
            Dialog::Custom { w: cw, h: ch } | Dialog::Canvas { w: cw, h: ch } => {
                let custom = matches!(self.dialog, Some(Dialog::Custom { .. }));
                c.text(if custom { tr("Custom size") } else { tr("Canvas size") }, rect(x, y, iw, 24.0), Font::Large, p.t1, Align::Left);
                y += 36.0;
                let note = if custom { tr("Width and height in pixels.") } else { tr("Layers stay centred; nothing is scaled.") };
                c.text(note, rect(x, y, iw, 20.0), Font::Body, p.t3, Align::Left);
                y += 34.0;
                let fw = (iw - 30.0) / 2.0;
                let (fwid, fhei) = if custom { (Field::CustomW, Field::CustomH) } else { (Field::CanvasW, Field::CanvasH) };
                let we = self.field.as_ref() == Some(&fwid);
                let he = self.field.as_ref() == Some(&fhei);
                let (cws, chs) = (cw.to_string(), ch.to_string());
                self.field_box(c, rect(x, y, fw, 34.0), if we { "" } else { &cws }, fwid);
                c.text("×", rect(x + fw, y, 30.0, 34.0), Font::Body, p.t3, Align::Center);
                self.field_box(c, rect(x + fw + 30.0, y, fw, 34.0), if he { "" } else { &chs }, fhei);
                buttons(self, c, if custom { tr("Create") } else { tr("Apply") }, Act::StartCustom);
            }
            Dialog::Shortcuts => {
                c.text(tr("Keyboard shortcuts"), rect(x, y, iw, 24.0), Font::Large, p.t1, Align::Left);
                y += 40.0;
                let list: [(&str, &str); 30] = [
                    ("V", "Move and transform"),
                    ("M / L", "Select / Lasso"),
                    ("C", "Crop and straighten"),
                    ("B", "Brush"),
                    ("E", "Eraser"),
                    ("G", "Fill"),
                    ("I", "Eyedropper"),
                    ("T", "Text"),
                    ("U", "Shapes"),
                    ("S", "Clone and heal (Alt+click: source)"),
                    ("K", "Remove background"),
                    ("H / Space", "Hand"),
                    ("[ ]", "Brush smaller / bigger"),
                    ("X", "Swap the two colours"),
                    ("Ctrl+Z", "Undo"),
                    ("Ctrl+Y", "Redo"),
                    ("Ctrl+S", "Save the project"),
                    ("Ctrl+O", "Open"),
                    ("Ctrl+E", "Export"),
                    ("Ctrl+J", "Duplicate"),
                    ("Ctrl+G", "Group (Shift: ungroup)"),
                    ("Ctrl+[ ]", "Layer down / up"),
                    ("Ctrl+A / D", "Select all / none"),
                    ("Ctrl+Shift+I", "Invert the selection"),
                    ("Ctrl+0 / 1", "Fit / 100 %"),
                    ("Ctrl+wheel", "Zoom"),
                    ("Arrows", "Nudge (Shift: 10 px)"),
                    ("Delete", "Delete the layer"),
                    ("Ctrl+' / ;", "Grid / snapping"),
                    ("Enter / Esc", "Apply / cancel"),
                ];
                let colw = (iw - 20.0) / 2.0;
                let rows = list.len().div_ceil(2);
                let rh = ((r.bottom() - 24.0 - 52.0 - y) / rows as f32).min(26.0);
                for (i, (k, what)) in list.iter().enumerate() {
                    let cx0 = x + (i / rows) as f32 * (colw + 20.0);
                    let cy0 = y + (i % rows) as f32 * rh;
                    let kw = c.text_width(k, Font::SmallBold) + 14.0;
                    let kr = rect(cx0, cy0 + 2.0, kw, rh - 4.0);
                    c.fill_round(kr, 6.0, p.fill);
                    c.text(k, kr, Font::SmallBold, p.t1, Align::Center);
                    c.text(tr(what), rect(cx0 + kw + 10.0, cy0, colw - kw - 10.0, rh), Font::Body, p.t2, Align::Left);
                }
                let pr = rect(r.right() - 24.0 - 104.0, r.bottom() - 24.0 - 36.0, 104.0, 36.0);
                self.pill(c, pr, tr("Close"), true, Hit::Act(Act::CloseDialog));
            }
            Dialog::Settings => {
                c.text(tr("Settings"), rect(x, y, iw, 24.0), Font::Large, p.t1, Align::Left);
                y += 44.0;
                c.text(tr("Theme"), rect(x, y, 110.0, 32.0), Font::Body, p.t2, Align::Left);
                let ti = match self.set.theme.as_str() {
                    "dark" => 1,
                    "light" => 2,
                    _ => 0,
                };
                self.segmented(c, rect(x + 110.0, y, iw - 110.0, 32.0), &[tr("System"), tr("Dark"), tr("Light")], ti, Sg::Theme);
                y += 46.0;
                c.text(tr("Language"), rect(x, y, 110.0, 32.0), Font::Body, p.t2, Align::Left);
                let li = match self.set.lang.as_str() {
                    "en" => 1,
                    "fr" => 2,
                    _ => 0,
                };
                self.segmented(c, rect(x + 110.0, y, iw - 110.0, 32.0), &[tr("System"), "English", "Français"], li, Sg::Lang);
                let pr = rect(r.right() - 24.0 - 104.0, r.bottom() - 24.0 - 36.0, 104.0, 36.0);
                self.pill(c, pr, tr("Close"), true, Hit::Act(Act::CloseDialog));
            }
        }
    }
}
