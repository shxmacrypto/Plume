//! The frame of the window (top bar, pages, caption buttons), the Drives page, and what every page shares: buttons,
//! check boxes, menus, dialogs and messages.

use super::{App, Hit, Modal, TOP_H, View};
use crate::{
    clean::human,
    i18n::{fill, tr},
};
use plume_ui::{Align, Canvas, Color, Font, PaintCx, Rect, icons, icons::Icon, m, rect};
use windows::{
    Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant,
        WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnDemand,
    },
    core::Interface,
};

/// Plume Clean's own icons (16 × 16, round strokes like Plume's).
pub mod ic {
    use plume_ui::icons::{Icon, Part::*};

    macro_rules! icons {
        ($($id:ident = [$($part:expr),+ $(,)?];)+) => {
            $(pub const $id: Icon = Icon { name: concat!("clean_", stringify!($id)), vb: 16.0, parts: &[$($part),+] };)+
        };
    }

    icons! {
        MAP = [R(2.0, 2.5, 12.0, 11.0, 2.0), P("M7.5 2.5v11M7.5 8h6.5M10.8 8v5.5")];
        SPARKLE = [P("M8 2.5c.4 2.6 1.4 3.6 4 4-2.6.4-3.6 1.4-4 4-.4-2.6-1.4-3.6-4-4 2.6-.4 3.6-1.4 4-4z"), P("M12.5 10.5v3M11 12h3")];
        SHIELD = [P("M8 2 13 4v4c0 3-2.2 5.2-5 6-2.8-.8-5-3-5-6V4z")];
        DUP = [R(2.5, 4.5, 8.0, 9.0, 1.5), P("M5.5 4.5V3.5A1 1 0 0 1 6.5 2.5h6a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1h-2")];
        CLOCK = [C(8.0, 8.0, 5.8), P("M8 4.8V8l2.2 1.6")];
        GLOBE = [C(8.0, 8.0, 5.8), P("M2.2 8h11.6M8 2.2c1.8 1.6 2.6 3.6 2.6 5.8S9.8 12.2 8 13.8C6.2 12.2 5.4 10.2 5.4 8S6.2 3.8 8 2.2z")];
        IMAGE = [R(2.0, 3.0, 12.0, 10.0, 2.0), C(6.0, 6.8, 1.2), P("m2.5 12 3.8-3.6 2.4 2.2 2-1.8 2.8 2.6")];
        BUG = [R(5.0, 5.0, 6.0, 8.5, 3.0), P("M6.2 5a1.8 1.8 0 0 1 3.6 0M2.5 9h2.5M11 9h2.5M3 5.5l2 1.3M13 5.5l-2 1.3M3 13l2-1.5M13 13l-2-1.5")];
        BOX = [P("M2.5 5 8 2.5 13.5 5v6L8 13.5 2.5 11z"), P("M2.5 5 8 7.5 13.5 5M8 7.5v6")];
        BIG = [P("M4 2h5l3 3v9H4z"), P("M6.5 11.5h3M8 7v4.5M6.5 8.5 8 7l1.5 1.5")];
        EMPTY = [P("M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z"), P("M6 9h4")];
        DOWN = [P("M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10")];
    }
}

pub fn decode_png(wic: &IWICImagingFactory, bytes: &[u8], px: u32) -> Option<IWICBitmapSource> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame: IWICBitmapSource = dec.GetFrame(0).ok()?.cast().ok()?;
        let scaler = wic.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, px, px, WICBitmapInterpolationModeFant).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        conv.cast().ok()
    }
}

/// Colors of the map's blocks, in turn.
pub const HUES: [u32; 8] = [0x84cc16, 0x60a5fa, 0xfbbf24, 0xf472b6, 0xa78bfa, 0x2dd4bf, 0xfb923c, 0x94a3b8];

impl App {
    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    /// A card: Plume's card color, round corners, a hairline around.
    pub fn card(&self, c: &mut Canvas, r: Rect) {
        c.fill_round(r, 16.0, self.pal.card);
        c.stroke_round(r, 16.0, self.pal.line, 1.0);
    }

    /// A round button: primary (accent) or quiet. Returns its width.
    pub fn pill(&mut self, c: &mut Canvas, x: f32, y: f32, label: &str, primary: bool, hit: Hit) -> f32 {
        let w = c.text_width(label, Font::BodyBold) + 32.0;
        let r = rect(x, y, w, 32.0);
        self.pill_in(c, r, label, primary, hit);
        w
    }

    pub fn pill_in(&mut self, c: &mut Canvas, r: Rect, label: &str, primary: bool, hit: Hit) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit);
        let down = hot && self.pressed.as_ref() == Some(&hit);
        let fill = match (primary, hot, down) {
            (true, _, true) => p.accent.mix(Color::hex(0), 0.18),
            (true, true, _) => p.accent.mix(Color::hex(0xffffff), 0.12),
            (true, _, _) => p.accent,
            (false, _, true) => p.fill2.over(p.fill.over(p.card)),
            (false, true, _) => p.fill2,
            (false, _, _) => p.fill,
        };
        c.fill_round(r, r.h / 2.0, fill);
        c.text(label, r, Font::BodyBold, if primary { p.on_accent } else { p.t1 }, Align::Center);
        self.hits.add(r, hit);
    }

    /// A button that cannot be pressed now.
    pub fn pill_off(&mut self, c: &mut Canvas, r: Rect, label: &str) {
        let p = self.pal.clone();
        c.fill_round(r, r.h / 2.0, p.fill);
        c.text(label, r, Font::BodyBold, p.t3, Align::Center);
    }

    /// A pill drawn right-aligned, ending at `right`. Returns its left edge.
    pub fn pill_right(&mut self, c: &mut Canvas, right: f32, y: f32, label: &str, primary: bool, hit: Hit) -> f32 {
        let w = c.text_width(label, Font::BodyBold) + 32.0;
        self.pill_in(c, rect(right - w, y, w, 32.0), label, primary, hit);
        right - w
    }

    pub fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit);
        if hot {
            c.fill_round(r, r.h / 2.0, p.fill);
        }
        c.icon(icon, r.centered(16.0, 16.0), if hot { p.t1 } else { p.t2 });
        self.hits.add(r, hit);
    }

    /// A segmented control: a pill track, the chosen segment raised. Each item: label, what it does, chosen.
    /// Returns the track.
    pub fn segmented(&mut self, c: &mut Canvas, x: f32, y: f32, items: &[(String, Hit, bool)]) -> Rect {
        let p = self.pal.clone();
        let widths: Vec<f32> = items.iter().map(|(l, _, _)| c.text_width(l, Font::BodyBold) + 30.0).collect();
        let track = rect(x, y, widths.iter().sum::<f32>() + 6.0, 34.0);
        c.fill_round(track, 17.0, p.fill);
        let mut sx = x + 3.0;
        for ((label, hit, on), w) in items.iter().zip(widths) {
            let r = rect(sx, y + 3.0, w, 28.0);
            if *on {
                c.fill_round(r, 14.0, p.fill2.over(p.fill.over(p.card)));
                c.stroke_round(r, 14.0, p.ring, 1.0);
            } else if self.hovered(hit) {
                c.fill_round(r, 14.0, p.hover);
            }
            c.text(label, r, Font::BodyBold, if *on { p.t1 } else { p.t2 }, Align::Center);
            self.hits.add(r, hit.clone());
            sx += w;
        }
        track
    }

    /// An iOS-style switch, on = accent.
    pub fn switch(&mut self, c: &mut Canvas, x: f32, y: f32, on: bool) -> Rect {
        let p = self.pal.clone();
        let r = rect(x, y, 44.0, 26.0);
        c.fill_round(r, 13.0, if on { p.accent } else { p.fill2 });
        let knob = if on { r.right() - 23.0 } else { r.x + 3.0 };
        c.fill_round(rect(knob, r.y + 3.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
        r
    }

    /// Nothing to show yet: an icon, a title, one line, and the button that starts something.
    pub fn empty_state(&mut self, c: &mut Canvas, area: Rect, icon: &Icon, title: &str, line: &str, button: Option<(&str, Hit)>) {
        let p = self.pal.clone();
        let h = 56.0 + 14.0 + 28.0 + 24.0 + if button.is_some() { 52.0 } else { 0.0 };
        let y = area.y + (area.h - h) / 2.0;
        let cx = area.x + area.w / 2.0;
        c.fill_round(rect(cx - 28.0, y, 56.0, 56.0), 18.0, p.fill);
        c.icon(icon, rect(cx - 10.0, y + 18.0, 20.0, 20.0), p.t2);
        c.text(title, rect(area.x, y + 70.0, area.w, 28.0), Font::Large, p.t1, Align::Center);
        c.text(line, rect(area.x + 16.0, y + 98.0, area.w - 32.0, 22.0), Font::Body, p.t3, Align::Center);
        if let Some((label, hit)) = button {
            let w = c.text_width(label, Font::BodyBold) + 36.0;
            self.pill_in(c, rect(cx - w / 2.0, y + 136.0, w, 34.0), label, true, hit);
        }
    }

    /// A check box (round corners), ticked or not.
    pub fn check(&mut self, c: &mut Canvas, r: Rect, on: bool, enabled: bool) {
        let p = self.pal.clone();
        if !enabled {
            c.stroke_round(r, 6.0, p.t4, 1.5);
            return;
        }
        if on {
            c.fill_round(r, 6.0, p.accent);
            c.icon_stroke(&icons::CHECK, r.inset(3.0), p.on_accent, 2.4);
        } else {
            c.stroke_round(r, 6.0, p.t3, 1.6);
        }
    }

    /// A small colored label ("Needs administrator").
    pub fn tag(&mut self, c: &mut Canvas, x: f32, y: f32, label: &str, color: Color) -> f32 {
        let w = c.text_width(label, Font::SmallBold) + 18.0;
        c.fill_round(rect(x, y, w, 22.0), 11.0, color.fade(0.16));
        c.text(label, rect(x, y, w, 22.0), Font::SmallBold, color, Align::Center);
        w
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let p = self.pal.clone();
        self.hits.clear();
        c.clear(p.bg);
        let (w, h) = (cx.w, cx.h);
        let body = rect(0.0, TOP_H, w, h - TOP_H);
        match self.view {
            View::Drives => self.paint_drives(c, body),
            View::Map => self.paint_map(c, body),
            View::Cleanup => self.paint_cleanup(c, body),
            View::Settings => self.paint_settings(c, body),
        }
        self.paint_top(c, w, cx);
        self.paint_menu(c, w, h);
        self.paint_modal(c, w, h);
        self.paint_toast(c, w, h);
    }

    fn paint_top(&mut self, c: &mut Canvas, w: f32, cx: &PaintCx) {
        let p = self.pal.clone();
        c.gradient_v(rect(0.0, 0.0, w, TOP_H), p.hdr1, p.hdr2);
        c.hline(0.0, TOP_H - 0.5, w, p.line);
        let wic = self.wic.clone();
        c.image(1, rect(14.0, (TOP_H - 18.0) / 2.0, 18.0, 18.0), 1.0, || decode_png(&wic, crate::LOGO, 64));
        c.text("Plume Clean", rect(40.0, 0.0, 110.0, TOP_H), Font::BodyBold, if cx.active { p.t1 } else { p.t3 }, Align::Left);
        // the pages: a segmented control with icons
        let tabs: [(View, &Icon, &str); 3] = [(View::Drives, &icons::DRIVE, "Drives"), (View::Map, &ic::MAP, "Map"), (View::Cleanup, &ic::SPARKLE, "Cleanup")];
        let widths: Vec<f32> = tabs.iter().map(|(_, _, l)| c.text_width(tr(l), Font::BodyBold) + 54.0).collect();
        let track = rect(152.0, (TOP_H - 34.0) / 2.0, widths.iter().sum::<f32>() + 6.0, 34.0);
        c.fill_round(track, 17.0, p.fill);
        let mut x = track.x + 3.0;
        for ((v, icon, label), tw) in tabs.into_iter().zip(widths) {
            let r = rect(x, track.y + 3.0, tw, 28.0);
            let on = self.view == v;
            let hit = Hit::Tab(v);
            if on {
                c.fill_round(r, 14.0, p.fill2.over(p.fill.over(p.hdr2)));
                c.stroke_round(r, 14.0, p.ring, 1.0);
            } else if self.hovered(&hit) {
                c.fill_round(r, 14.0, p.hover);
            }
            c.icon(icon, rect(r.x + 14.0, r.y + 6.0, 16.0, 16.0), if on { p.ink } else { p.t2 });
            c.text(tr(label), rect(r.x + 36.0, r.y, tw - 42.0, r.h), Font::BodyBold, if on { p.t1 } else { p.t2 }, Align::Left);
            self.hits.add(r, hit);
            x += tw;
        }
        let cap_x = w - 3.0 * m::CAPTION_W;
        let gear = rect(cap_x - 46.0, (TOP_H - 34.0) / 2.0, 34.0, 34.0);
        if self.view == View::Settings {
            c.fill_round(gear, 17.0, p.fill2);
            c.stroke_round(gear, 17.0, p.ring, 1.0);
            c.icon(&icons::GEAR, gear.centered(16.0, 16.0), p.ink);
            self.hits.add(gear, Hit::Tab(View::Settings));
        } else {
            self.icon_btn(c, &icons::GEAR, gear, Hit::Tab(View::Settings));
        }
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, TOP_H - 8.0);
            let hot = self.hovered(&id) || self.pressed.as_ref() == Some(&id);
            if hot {
                c.fill(r, if id == Hit::Close { p.close_hover } else { p.hover });
            }
            let ink = match (id == Hit::Close && hot, cx.active) {
                (true, _) => Color::hex(0xffffff),
                (false, true) => p.t1,
                (false, false) => p.t3,
            };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    /// Big page title and a line under it. Returns the y below.
    pub fn heading(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, sub: &str) -> f32 {
        let p = self.pal.clone();
        c.text(title, rect(x, y, w, 34.0), Font::Hero, p.t1, Align::Left);
        if sub.is_empty() {
            return y + 48.0;
        }
        c.text(sub, rect(x, y + 36.0, w, 20.0), Font::Body, p.t3, Align::Left);
        y + 72.0
    }

    fn paint_drives(&mut self, c: &mut Canvas, body: Rect) {
        let p = self.pal.clone();
        let x0 = body.x + 32.0;
        let w = body.w - 64.0;
        let mut y = self.heading(c, x0, body.y + 28.0, w, tr("Your drives"), tr("Pick a drive to see what fills it. Nothing is changed by a scan."));
        let n = self.drives.len().max(1);
        let gap = 14.0;
        let cols = ((w + gap) / 336.0).floor().clamp(1.0, n as f32) as usize;
        let cw = (w - (cols as f32 - 1.0) * gap) / cols as f32;
        let drives = self.drives.clone();
        for (i, d) in drives.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let r = rect(x0 + col as f32 * (cw + gap), y + row as f32 * (128.0 + gap), cw, 128.0);
            self.card(c, r);
            c.fill_round(rect(r.x + 20.0, r.y + 20.0, 40.0, 40.0), 12.0, p.fill);
            let icon = if d.root.to_string_lossy().len() <= 3 { &icons::DRIVE } else { &icons::FOLDER };
            c.icon(icon, rect(r.x + 30.0, r.y + 30.0, 20.0, 20.0), p.t2);
            c.text(&d.title(), rect(r.x + 72.0, r.y + 18.0, r.w - 200.0, 24.0), Font::Title, p.t1, Align::Left);
            let line = fill(tr("{free} free of {total}"), &[&human(d.free), &human(d.total)]);
            c.text(&line, rect(r.x + 72.0, r.y + 40.0, r.w - 200.0, 20.0), Font::Small, p.t3, Align::Left);
            // the bar: used, free
            let bar = rect(r.x + 20.0, r.y + 76.0, r.w - 40.0, 8.0);
            c.fill_round(bar, 4.0, p.fill);
            let k = if d.total > 0 { d.used() as f32 / d.total as f32 } else { 0.0 };
            let ink = if k > 0.9 {
                p.red
            } else if k > 0.75 {
                p.warn
            } else {
                p.accent
            };
            c.fill_round(rect(bar.x, bar.y, (bar.w * k).max(8.0), bar.h), 4.0, ink);
            c.text(&fill(tr("{used} used"), &[&human(d.used())]), rect(r.x + 20.0, r.y + 92.0, r.w / 2.0, 20.0), Font::Small, p.t2, Align::Left);
            c.text(&format!("{:.0} %", k * 100.0), rect(r.x + r.w / 2.0, r.y + 92.0, r.w / 2.0 - 20.0, 20.0), Font::SmallBold, p.t2, Align::Right);
            let scanning_this = self.scanning.is_some() && self.target.as_deref() == Some(d.root.as_path());
            let label = if scanning_this { tr("Scanning…") } else { tr("Scan") };
            let bw = c.text_width(label, Font::BodyBold) + 36.0;
            self.pill_in(c, rect(r.right() - 20.0 - bw, r.y + 20.0, bw, 34.0), label, true, Hit::ScanDrive(i));
        }
        y += drives.len().div_ceil(cols) as f32 * (128.0 + gap);
        // a folder
        let r = rect(x0, y, w.min(680.0), 76.0);
        self.card(c, r);
        c.fill_round(rect(r.x + 20.0, r.y + 18.0, 40.0, 40.0), 12.0, p.fill);
        c.icon(&icons::FOLDER, rect(r.x + 30.0, r.y + 28.0, 20.0, 20.0), p.t2);
        c.text(tr("A folder"), rect(r.x + 72.0, r.y + 14.0, r.w - 260.0, 24.0), Font::Title, p.t1, Align::Left);
        c.text(tr("Scan only one folder, faster."), rect(r.x + 72.0, r.y + 38.0, r.w - 260.0, 20.0), Font::Small, p.t3, Align::Left);
        self.pill_right(c, r.right() - 20.0, r.y + 22.0, tr("Choose a folder…"), false, Hit::ScanFolder);
        if let Some(t) = self.tree.clone() {
            let r2 = rect(x0, r.bottom() + gap, w.min(680.0), 64.0);
            self.card(c, r2);
            c.icon(&ic::MAP, rect(r2.x + 30.0, r2.y + 22.0, 20.0, 20.0), p.ink);
            let s = fill(tr("Last scan: {path}, {size}"), &[&t.root.to_string_lossy(), &human(t.node(0).size)]);
            c.text(&s, rect(r2.x + 72.0, r2.y, r2.w - 260.0, r2.h), Font::Body, p.t1, Align::Left);
            self.pill_right(c, r2.right() - 20.0, r2.y + 16.0, tr("Open the map"), false, Hit::OpenMap);
        }
    }

    fn paint_menu(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(menu) = &self.menu else { return };
        let p = self.pal.clone();
        let items: Vec<String> = menu.items.iter().map(|(l, _)| l.clone()).collect();
        let mw = items.iter().map(|l| c.text_width(l, Font::Body)).fold(160.0f32, f32::max) + 44.0;
        let mh = items.len() as f32 * 36.0 + 12.0;
        let x = menu.x.min(w - mw - 8.0).max(8.0);
        let y = if menu.y + mh > h - 8.0 { (menu.y - mh).max(8.0) } else { menu.y };
        let b = rect(x, y, mw, mh);
        c.fill_round(b.offset(0.0, 6.0).inset(-2.0), 16.0, Color::hex(0).fade(if p.dark { 0.3 } else { 0.12 }));
        c.fill_round(b, 14.0, p.solid);
        c.stroke_round(b, 14.0, p.ring, 1.0);
        for (i, label) in items.iter().enumerate() {
            let row = rect(x + 6.0, y + 6.0 + i as f32 * 36.0, mw - 12.0, 36.0);
            let hit = Hit::MenuItem(i);
            if self.hovered(&hit) {
                c.fill_round(row, 10.0, p.hover);
            }
            c.text(label, rect(row.x + 12.0, row.y, row.w - 20.0, row.h), Font::Body, p.t1, Align::Left);
            self.hits.add(row, hit);
        }
    }

    fn paint_modal(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(modal) = self.modal.clone() else { return };
        let p = self.pal.clone();
        // everything under the dialog stops answering
        self.hits.clear();
        c.fill(rect(0.0, 0.0, w, h), Color::hex(0).fade(if p.dark { 0.5 } else { 0.3 }));
        let (title, lines, ok) = match modal {
            Modal::Confirm => {
                let mut lines = Vec::new();
                for s in self.sugg.iter().filter(|s| s.selected && s.blocked.is_none()) {
                    let n = if s.kind.review() { s.items.iter().filter(|i| i.selected).count() as u64 } else { s.count };
                    let size = if s.kind == crate::clean::Kind::RecycleBin { s.size } else { s.selected_size() };
                    lines.push(format!("•  {} — {}, {}", tr(s.kind.title()), human(size), crate::i18n::items(n)));
                }
                lines.push(String::new());
                if self.sugg.iter().any(|s| s.selected && s.kind == crate::clean::Kind::RecycleBin) {
                    lines.push(tr("The Recycle Bin is emptied: what is in it is gone for good.").to_string());
                }
                lines.push(if self.places.sandbox.is_some() {
                    tr("Test mode: files in the test folder are removed for good.").to_string()
                } else if self.set.recycle {
                    tr("Everything else goes to the Recycle Bin: you can still get it back.").to_string()
                } else {
                    tr("Files are removed for good (the Recycle Bin is off in the settings).").to_string()
                });
                (tr("Clean now?").to_string(), lines, tr("Clean"))
            }
            Modal::Elevate => (
                tr("Restart as administrator?").to_string(),
                vec![
                    tr("Windows' own files can only be cleaned with administrator rights.").to_string(),
                    tr("Plume Clean closes and opens again; Windows asks you first.").to_string(),
                ],
                tr("Restart"),
            ),
        };
        let mw = (w - 80.0).min(560.0);
        let line_h = 22.0;
        let mh = 68.0 + lines.len() as f32 * line_h + 80.0;
        let b = rect((w - mw) / 2.0, (h - mh) / 2.0, mw, mh);
        c.fill_round(b.offset(0.0, 10.0).inset(-2.0), 22.0, Color::hex(0).fade(if p.dark { 0.35 } else { 0.12 }));
        c.fill_round(b, 20.0, p.solid);
        c.stroke_round(b, 20.0, p.ring, 1.0);
        c.text(&title, rect(b.x + 24.0, b.y + 22.0, b.w - 48.0, 28.0), Font::Large, p.t1, Align::Left);
        for (i, l) in lines.iter().enumerate() {
            c.text(l, rect(b.x + 24.0, b.y + 64.0 + i as f32 * line_h, b.w - 48.0, line_h), Font::Body, p.t2, Align::Left);
        }
        let by = b.bottom() - 58.0;
        let ow = c.text_width(ok, Font::BodyBold) + 36.0;
        self.pill_in(c, rect(b.right() - 24.0 - ow, by, ow, 34.0), ok, true, Hit::ModalOk);
        let cancel = tr("Cancel");
        let cw = c.text_width(cancel, Font::BodyBold) + 36.0;
        self.pill_in(c, rect(b.right() - 24.0 - ow - 10.0 - cw, by, cw, 34.0), cancel, false, Hit::ModalCancel);
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((text, _)) = self.toast.clone() else { return };
        let p = self.pal.clone();
        let tw = (c.text_width(&text, Font::Body) + 40.0).min(w - 40.0);
        let r = rect((w - tw) / 2.0, h - 118.0, tw, 38.0);
        c.fill_round(r, 19.0, p.solid2);
        c.stroke_round(r, 19.0, p.ring, 1.0);
        c.text(&text, r.inset_xy(16.0, 0.0), Font::Body, p.t1, Align::Center);
        self.hits.add(r, Hit::Toast);
    }
}
