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
pub const HUES: [u32; 8] = [0x34d399, 0x60a5fa, 0xfbbf24, 0xf472b6, 0xa78bfa, 0x2dd4bf, 0xfb923c, 0x94a3b8];

impl App {
    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
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
        let fill = if primary {
            if hot { p.accent.mix(Color::hex(0xffffff), 0.14) } else { p.accent }
        } else if hot {
            p.t1.fade(0.16)
        } else {
            p.t1.fade(0.08)
        };
        c.fill_round(r, r.h / 2.0, fill);
        c.text(label, r, Font::BodyBold, if primary { p.on_accent } else { p.t1 }, Align::Center);
        self.hits.add(r, hit);
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
            c.fill_round(r, r.h / 2.0, p.t1.fade(0.1));
        }
        let s = 16.0;
        c.icon(icon, r.centered(s, s), if hot { p.t1 } else { p.t2 });
        self.hits.add(r, hit);
    }

    /// A check box (round corners), ticked or not.
    pub fn check(&mut self, c: &mut Canvas, r: Rect, on: bool, enabled: bool) {
        let p = self.pal.clone();
        if !enabled {
            c.stroke_round(r, 6.0, p.t3.fade(0.4), 1.5);
            return;
        }
        if on {
            c.fill_round(r, 6.0, p.accent);
            c.icon_stroke(&icons::CHECK, r.inset(3.0), p.on_accent, 2.4);
        } else {
            c.stroke_round(r, 6.0, p.t2.fade(0.8), 1.6);
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
        c.fill(rect(0.0, 0.0, w, TOP_H), p.bar);
        c.hline(0.0, TOP_H - 0.5, w, p.line);
        let wic = self.wic.clone();
        c.image(1, rect(18.0, 14.0, 28.0, 28.0), 1.0, || decode_png(&wic, crate::LOGO, 64));
        c.text("Plume Clean", rect(56.0, 0.0, 130.0, TOP_H), Font::Large, p.t1, Align::Left);
        // the pages
        let tabs: [(View, &Icon, &str); 3] = [(View::Drives, &icons::DRIVE, "Drives"), (View::Map, &ic::MAP, "Map"), (View::Cleanup, &ic::SPARKLE, "Cleanup")];
        let mut x = 200.0;
        for (v, icon, label) in tabs {
            let label = tr(label);
            let tw = c.text_width(label, Font::BodyBold) + 50.0;
            let r = rect(x, 12.0, tw, 32.0);
            let on = self.view == v;
            let hit = Hit::Tab(v);
            let fill = if on {
                p.accent.fade(if p.dark { 0.2 } else { 0.14 })
            } else if self.hovered(&hit) {
                p.t1.fade(0.07)
            } else {
                Color::TRANSPARENT
            };
            c.fill_round(r, 16.0, fill);
            let ink = if on { p.accent } else { p.t2 };
            c.icon(icon, rect(r.x + 14.0, r.y + 8.0, 16.0, 16.0), ink);
            c.text(label, rect(r.x + 36.0, r.y, tw - 42.0, r.h), Font::BodyBold, if on { p.t1 } else { p.t2 }, Align::Left);
            self.hits.add(r, hit);
            x += tw + 6.0;
        }
        let cap_x = w - 3.0 * m::CAPTION_W;
        let gear = rect(cap_x - 46.0, 12.0, 34.0, 32.0);
        let on = self.view == View::Settings;
        if on {
            c.fill_round(gear, 16.0, p.accent.fade(0.2));
        }
        self.icon_btn(c, &icons::GEAR, gear, Hit::Tab(View::Settings));
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, 36.0);
            let hot = self.hovered(&id) || self.pressed.as_ref() == Some(&id);
            if hot {
                c.fill(r, if id == Hit::Close { Color::hex(0xc42b1c) } else { p.t1.fade(0.1) });
            }
            c.glyph(glyph, r, if id == Hit::Close && hot { Color::hex(0xffffff) } else { p.t2 });
            self.hits.add(r, id);
        }
    }

    /// Big page title and a line under it. Returns the y below.
    pub fn heading(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, sub: &str) -> f32 {
        let p = self.pal.clone();
        c.text(title, rect(x, y, w, 34.0), Font::Hero, p.t1, Align::Left);
        if sub.is_empty() {
            return y + 44.0;
        }
        c.text(sub, rect(x, y + 36.0, w, 20.0), Font::Body, p.t3, Align::Left);
        y + 70.0
    }

    fn paint_drives(&mut self, c: &mut Canvas, body: Rect) {
        let p = self.pal.clone();
        let x0 = body.x + 32.0;
        let w = body.w - 64.0;
        let mut y = self.heading(c, x0, body.y + 28.0, w, tr("Your drives"), tr("Pick a drive to see what fills it. Nothing is changed by a scan."));
        let n = self.drives.len().max(1);
        let cols = ((w + 16.0) / 336.0).floor().clamp(1.0, n as f32) as usize;
        let cw = (w - (cols as f32 - 1.0) * 16.0) / cols as f32;
        let drives = self.drives.clone();
        for (i, d) in drives.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let r = rect(x0 + col as f32 * (cw + 16.0), y + row as f32 * 140.0, cw, 124.0);
            c.fill_round(r, 16.0, p.card);
            c.stroke_round(r, 16.0, p.line, 1.0);
            c.fill_round(rect(r.x + 18.0, r.y + 18.0, 40.0, 40.0), 12.0, p.accent.fade(0.16));
            c.icon(if d.root.to_string_lossy().len() <= 3 { &icons::DRIVE } else { &icons::FOLDER }, rect(r.x + 28.0, r.y + 28.0, 20.0, 20.0), p.accent);
            c.text(&d.title(), rect(r.x + 70.0, r.y + 16.0, r.w - 196.0, 24.0), Font::Title, p.t1, Align::Left);
            let line = fill(tr("{free} free of {total}"), &[&human(d.free), &human(d.total)]);
            c.text(&line, rect(r.x + 70.0, r.y + 38.0, r.w - 196.0, 20.0), Font::Small, p.t3, Align::Left);
            // the bar: used, free
            let bar = rect(r.x + 18.0, r.y + 72.0, r.w - 36.0, 10.0);
            c.fill_round(bar, 5.0, p.t1.fade(0.1));
            let k = if d.total > 0 { d.used() as f32 / d.total as f32 } else { 0.0 };
            let ink = if k > 0.9 {
                Color::hex(0xf87171)
            } else if k > 0.75 {
                p.warn
            } else {
                p.accent
            };
            c.fill_round(rect(bar.x, bar.y, (bar.w * k).max(10.0), bar.h), 5.0, ink);
            c.text(&fill(tr("{used} used"), &[&human(d.used())]), rect(r.x + 18.0, r.y + 90.0, r.w / 2.0, 20.0), Font::Small, p.t2, Align::Left);
            c.text(&format!("{:.0} %", k * 100.0), rect(r.x + r.w / 2.0, r.y + 90.0, r.w / 2.0 - 18.0, 20.0), Font::SmallBold, p.t2, Align::Right);
            let scanning_this = self.scanning.is_some() && self.target.as_deref() == Some(d.root.as_path());
            let label = if scanning_this { tr("Scanning…") } else { tr("Scan") };
            self.pill_in(c, rect(r.right() - 112.0, r.y + 20.0, 94.0, 32.0), label, true, Hit::ScanDrive(i));
        }
        y += drives.len().div_ceil(cols) as f32 * 140.0 + 8.0;
        // a folder
        let r = rect(x0, y, w.min(680.0), 76.0);
        c.fill_round(r, 16.0, p.card);
        c.stroke_round(r, 16.0, p.line, 1.0);
        c.icon(&icons::FOLDER, rect(r.x + 22.0, r.y + 28.0, 20.0, 20.0), p.t2);
        c.text(tr("A folder"), rect(r.x + 56.0, r.y + 14.0, r.w - 240.0, 24.0), Font::Title, p.t1, Align::Left);
        c.text(tr("Scan only one folder, faster."), rect(r.x + 56.0, r.y + 38.0, r.w - 240.0, 20.0), Font::Small, p.t3, Align::Left);
        self.pill_right(c, r.right() - 18.0, r.y + 22.0, tr("Choose a folder…"), false, Hit::ScanFolder);
        if let Some(t) = self.tree.clone() {
            let y2 = r.bottom() + 16.0;
            let r2 = rect(x0, y2, w.min(680.0), 64.0);
            c.fill_round(r2, 16.0, p.card);
            c.icon(&ic::MAP, rect(r2.x + 22.0, r2.y + 22.0, 20.0, 20.0), p.accent);
            let s = fill(tr("Last scan: {path}, {size}"), &[&t.root.to_string_lossy(), &human(t.node(0).size)]);
            c.text(&s, rect(r2.x + 56.0, r2.y, r2.w - 220.0, r2.h), Font::Body, p.t1, Align::Left);
            self.pill_right(c, r2.right() - 18.0, r2.y + 16.0, tr("Open the map"), false, Hit::OpenMap);
        }
    }

    fn paint_menu(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(menu) = &self.menu else { return };
        let p = self.pal.clone();
        let items: Vec<String> = menu.items.iter().map(|(l, _)| l.clone()).collect();
        let mw = items.iter().map(|l| c.text_width(l, Font::Body)).fold(160.0f32, f32::max) + 44.0;
        let mh = items.len() as f32 * 34.0 + 12.0;
        let x = menu.x.min(w - mw - 8.0).max(8.0);
        let y = if menu.y + mh > h - 8.0 { (menu.y - mh).max(8.0) } else { menu.y };
        let b = rect(x, y, mw, mh);
        c.fill_round(b.offset(0.0, 6.0).inset(-2.0), 14.0, Color::hex(0).fade(0.25));
        c.fill_round(b, 12.0, if p.dark { Color::hex(0x2a2a30) } else { Color::hex(0xffffff) });
        c.stroke_round(b, 12.0, p.line, 1.0);
        for (i, label) in items.iter().enumerate() {
            let row = rect(x + 6.0, y + 6.0 + i as f32 * 34.0, mw - 12.0, 32.0);
            let hit = Hit::MenuItem(i);
            if self.hovered(&hit) {
                c.fill_round(row, 8.0, p.t1.fade(0.08));
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
        c.fill(rect(0.0, 0.0, w, h), Color::hex(0).fade(0.55));
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
        let mw = (w - 80.0).min(520.0);
        let line_h = 22.0;
        let mh = 64.0 + lines.len() as f32 * line_h + 76.0;
        let b = rect((w - mw) / 2.0, (h - mh) / 2.0, mw, mh);
        c.fill_round(b.offset(0.0, 8.0).inset(-2.0), 20.0, Color::hex(0).fade(0.3));
        c.fill_round(b, 18.0, p.card);
        c.stroke_round(b, 18.0, p.line, 1.0);
        c.text(&title, rect(b.x + 26.0, b.y + 22.0, b.w - 52.0, 28.0), Font::Large, p.t1, Align::Left);
        for (i, l) in lines.iter().enumerate() {
            c.text(l, rect(b.x + 26.0, b.y + 62.0 + i as f32 * line_h, b.w - 52.0, line_h), Font::Body, p.t2, Align::Left);
        }
        let by = b.bottom() - 56.0;
        let left = self.pill_right(c, b.right() - 24.0, by, ok, true, Hit::ModalOk);
        self.pill_right(c, left - 10.0, by, tr("Cancel"), false, Hit::ModalCancel);
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((text, _)) = self.toast.clone() else { return };
        let p = self.pal.clone();
        let tw = c.text_width(&text, Font::Body) + 40.0;
        let r = rect((w - tw) / 2.0, h - 72.0, tw, 40.0);
        c.fill_round(r, 20.0, if p.dark { Color::hex(0x2e2e34) } else { Color::hex(0x1c1c20) });
        c.text(&text, r, Font::Body, Color::hex(0xffffff), Align::Center);
        self.hits.add(r, Hit::Toast);
    }
}
