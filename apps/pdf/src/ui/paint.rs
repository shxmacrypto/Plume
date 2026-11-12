//! Painting: title bar with tabs, tools, sidebar (thumbnails, outline), the pages and what lies over them,
//! the navigation pill, search, menus, dialogs, messages and the start screen.

use super::{
    App, BAR_H, Drag, EditFor, Fit, HL_COLORS, Hit, INK_COLORS, Modal, SIDE_W, Status, THUMB_W, TITLE_H, Tool,
    draw::{self, ic},
};
use crate::{
    edit::{FieldValue, Kind},
    i18n::{fill, tr},
    render::Job,
    text,
};
use plume_ui::{Align, Canvas, Color, EditStyle, Font, PaintCx, Rect, icons, icons::Icon, m, rect};
use std::time::Duration;
use windows::{
    Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant,
        WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnDemand,
    },
    core::Interface,
};

/// The logo (PNG in the program), decoded to `px` pixels.
fn logo(wic: &IWICImagingFactory, px: u32) -> Option<IWICBitmapSource> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(crate::LOGO).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let s = wic.CreateBitmapScaler().ok()?;
        s.Initialize(&frame, px, px, WICBitmapInterpolationModeFant).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&s, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        let bmp = wic.CreateBitmapFromSource(&conv, windows::Win32::Graphics::Imaging::WICBitmapCacheOnLoad).ok()?;
        bmp.cast().ok()
    }
}

fn col3(c: [f32; 3]) -> Color {
    Color { r: c[0], g: c[1], b: c[2], a: 1.0 }
}

impl App {
    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, active: bool, enabled: bool) {
        let p = self.pal.clone();
        let hot = enabled && self.hovered(&hit);
        if active {
            c.fill_round(r, r.h / 2.0, p.accent.fade(0.16));
        } else if hot {
            c.fill_round(r, r.h / 2.0, if self.pressed.as_ref() == Some(&hit) { p.fill2 } else { p.hover });
        }
        let ink = if active {
            p.accent
        } else if !enabled {
            p.t3.fade(0.6)
        } else if hot {
            p.t1
        } else {
            p.t2
        };
        c.icon(icon, r.centered(16.0, 16.0), ink);
        if enabled {
            self.hits.add(r, hit);
        }
    }

    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, primary: bool, hit: Hit) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit);
        let down = hot && self.pressed.as_ref() == Some(&hit);
        let fill = match (primary, hot, down) {
            (true, _, true) => p.accent.mix(Color::hex(0x000000), 0.18),
            (true, true, _) => p.accent.mix(Color::hex(0xffffff), 0.1),
            (true, ..) => p.accent,
            (false, _, true) => p.fill2.over(p.card).mix(Color::hex(0x000000), 0.08),
            (false, true, _) => p.fill2,
            (false, ..) => p.fill,
        };
        c.fill_round(r, r.h / 2.0, fill);
        c.text(label, r, Font::BodyBold, if primary { p.on_accent } else { p.t1 }, Align::Center);
        self.hits.add(r, hit);
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let p = self.pal.clone();
        self.hits.clear();
        self.thumb_rects.clear();
        self.pending_thumbs.clear();
        c.clear(p.chrome);
        let (w, h) = (cx.w, cx.h);
        self.paint_title(c, w, cx);
        let body = rect(0.0, TITLE_H, w, h - TITLE_H);
        if self.settings_open {
            self.paint_settings(c, body);
        } else if self.tabs.is_empty() {
            self.paint_start(c, body);
        } else {
            self.paint_bar(c, rect(0.0, TITLE_H, w, BAR_H));
            let mut rest = rect(0.0, TITLE_H + BAR_H, w, h - TITLE_H - BAR_H);
            self.view = rest;
            if self.set.sidebar && w > 700.0 {
                let side = rest.cut_left(SIDE_W);
                self.view = rest;
                self.relayout();
                self.paint_side(c, side);
            } else {
                self.relayout();
            }
            self.paint_pages(c, rest);
            self.paint_nav(c, rest);
            if self.search_open {
                self.paint_search(c, rest);
            }
        }
        self.paint_menu(c, w, h);
        let field = self.paint_modal(c, w, h);
        self.paint_toast(c, w, h);
        self.place_edit(field);
    }

    // ───────────────────────── title bar ─────────────────────────

    fn paint_title(&mut self, c: &mut Canvas, w: f32, cx: &PaintCx) {
        let p = self.pal.clone();
        c.gradient_v(rect(0.0, 0.0, w, TITLE_H), p.hdr1, p.hdr2);
        let wic = self.wic.clone();
        let px = (18.0 * c.scale()).round() as u32;
        c.image(0xF00D_0000 + px as u64, rect(14.0, (TITLE_H - 18.0) / 2.0, 18.0, 18.0), 1.0, || logo(&wic, px));
        let cap_x = w - 3.0 * m::CAPTION_W;
        let name_w = c.text_width("Plume PDF", Font::BodyBold);
        c.text("Plume PDF", rect(40.0, 0.0, name_w + 4.0, TITLE_H), Font::BodyBold, if cx.active { p.t1 } else { p.t3 }, Align::Left);
        let mut x = 40.0 + name_w + 18.0;
        let n = self.tabs.len();
        let avail = cap_x - x - 60.0;
        let tw = if n > 0 { (avail / n as f32).clamp(90.0, 220.0) } else { 0.0 };
        for i in 0..n {
            let r = rect(x, 6.0, tw - 4.0, TITLE_H - 6.0);
            if r.right() > cap_x - 44.0 {
                break;
            }
            let active = i == self.cur && !self.settings_open;
            let hit = Hit::Tab(i);
            if active {
                c.fill_round(rect(r.x, r.y, r.w, r.h + 10.0), 10.0, p.hdr2);
            } else if self.hovered(&hit) {
                c.fill_round(rect(r.x, r.y + 2.0, r.w, r.h - 6.0), 10.0, p.hover);
            }
            self.hits.add(r, hit);
            let t = &self.tabs[i];
            let name = t.name.clone();
            let dirty = t.dirty;
            c.icon(&ic::DOC, rect(r.x + 10.0, r.y + (r.h - 14.0) / 2.0 - 1.0, 14.0, 14.0), if active { p.accent } else { p.t3 });
            c.text(
                &name,
                rect(r.x + 30.0, r.y, r.w - 58.0, r.h - 2.0),
                if active { Font::BodyBold } else { Font::Body },
                if active { p.t1 } else { p.t2 },
                Align::Left,
            );
            let xr = rect(r.right() - 26.0, r.y + (r.h - 22.0) / 2.0 - 1.0, 22.0, 22.0);
            let xh = Hit::TabClose(i);
            if self.hovered(&xh) {
                c.fill_round(xr, 11.0, p.hover);
            }
            if dirty && !self.hovered(&xh) {
                c.fill_round(xr.centered(7.0, 7.0), 3.5, p.accent);
            } else {
                c.icon(&icons::CLOSE, xr.centered(9.0, 9.0), p.t2);
            }
            self.hits.add(xr, xh);
            x += tw;
        }
        if n > 0 {
            let r = rect(x + 2.0, 7.0, 28.0, 28.0);
            self.icon_btn(c, &icons::PLUS, r, Hit::NewTab, false, true);
        }
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, TITLE_H - 8.0);
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

    // ───────────────────────── toolbar ─────────────────────────

    fn paint_bar(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.fill(r, p.hdr2);
        c.hline(r.x, r.bottom() - 0.5, r.w, p.line);
        let editable = self.tab().is_some_and(|t| t.editable());
        let can_undo = self.tab().is_some_and(|t| !t.undo.is_empty());
        let dirty = self.tab().is_some_and(|t| t.dirty);
        let y = r.y + (r.h - 34.0) / 2.0;
        let mut x = r.x + 14.0;
        let b = |x: f32| rect(x, y, 34.0, 34.0);
        self.icon_btn(c, &icons::SIDEBAR, b(x), Hit::Btn("sidebar"), false, true);
        x += 38.0;
        self.icon_btn(c, &icons::FOLDER, b(x), Hit::Btn("open"), false, true);
        x += 38.0;
        self.icon_btn(c, &ic::SAVE, b(x), Hit::Btn("save"), false, editable);
        if dirty {
            c.fill_round(rect(x + 25.0, y + 6.0, 7.0, 7.0), 3.5, p.accent);
        }
        x += 38.0;
        self.icon_btn(c, &ic::PRINT, b(x), Hit::Btn("print"), false, true);
        x += 38.0;
        self.icon_btn(c, &ic::UNDO, b(x), Hit::Btn("undo"), false, can_undo);
        x += 44.0;
        c.vline(x - 4.0, y + 6.0, 22.0, p.line);
        let tools: [(Tool, &Icon); 6] = [
            (Tool::Hand, &ic::POINTER),
            (Tool::Highlight, &ic::MARKER),
            (Tool::Note, &ic::NOTE),
            (Tool::Ink, &ic::PEN),
            (Tool::Text, &ic::TEXT),
            (Tool::Sign, &ic::SIGN),
        ];
        for (t, icon) in tools {
            let active = self.tool == t;
            self.icon_btn(c, icon, b(x), Hit::Tool(t), active, t == Tool::Hand || editable);
            x += 38.0;
        }
        // colors of the current tool
        let swatches: Option<(&[[f32; 3]; 4], usize)> = match self.tool {
            Tool::Highlight => Some((&HL_COLORS, self.hl_color)),
            Tool::Ink | Tool::Text => Some((&INK_COLORS, self.ink_color)),
            _ => None,
        };
        if let Some((cols, cur)) = swatches {
            x += 8.0;
            for (i, col) in cols.iter().enumerate() {
                let s = rect(x, r.y + (r.h - 20.0) / 2.0, 20.0, 20.0);
                c.fill_round(s, 10.0, col3(*col));
                c.stroke_round(s, 10.0, p.t3.fade(0.6), 1.0);
                if i == cur {
                    c.stroke_round(s.inset(-3.0), 13.0, p.t1, 1.6);
                }
                self.hits.add(s.inset(-3.0), Hit::Swatch(i));
                x += 28.0;
            }
        }
        // the tool's hint, when there is room
        let hint = match self.tool {
            Tool::Hand => "",
            Tool::Highlight => tr("Drag over text to highlight it"),
            Tool::Note => tr("Click to add a note"),
            Tool::Ink => tr("Draw on the page"),
            Tool::Text => tr("Click or drag to add text"),
            Tool::Sign => tr("Click to place your signature"),
        };
        let mut rx = r.right() - 12.0;
        let right: [(&Icon, &'static str, bool); 4] = [
            (&icons::GEAR, "settings", self.settings_open),
            (&icons::MORE, "more", self.menu.as_ref().is_some_and(|m| m.more)),
            (&ic::MOON, "dark_pages", self.set.dark_pages),
            (&icons::SEARCH, "search", self.search_open),
        ];
        for (icon, id, active) in right {
            rx -= 38.0;
            self.icon_btn(c, icon, b(rx), Hit::Btn(id), active, true);
        }
        if !hint.is_empty() && rx - x > 160.0 {
            c.text(hint, rect(x + 12.0, r.y, rx - x - 20.0, r.h), Font::Small, p.t3, Align::Left);
        }
    }

    // ───────────────────────── sidebar ─────────────────────────

    fn paint_side(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.fill(r, p.side);
        c.vline(r.right() - 0.5, r.y, r.h, p.line);
        // Pages | Outline
        let seg = rect(r.x + 16.0, r.y + 10.0, r.w - 32.0, 32.0);
        c.fill_round(seg, 16.0, p.fill);
        for (i, (label, icon)) in [(tr("Pages"), &ic::PAGES), (tr("Outline"), &ic::OUTLINE)].into_iter().enumerate() {
            let b = rect(seg.x + 3.0 + i as f32 * (seg.w - 6.0) / 2.0, seg.y + 3.0, (seg.w - 6.0) / 2.0, 26.0);
            let active = self.side_tab == i as u8;
            if active {
                c.fill_round(b, 13.0, p.fill2);
                c.stroke_round(b, 13.0, p.ring, 1.0);
            } else if self.hovered(&Hit::SideTab(i as u8)) {
                c.fill_round(b, 13.0, p.hover);
            }
            let tw = c.text_width(label, Font::SmallBold);
            let ix = b.x + (b.w - tw - 20.0) / 2.0;
            c.icon(icon, rect(ix, b.y + 6.0, 14.0, 14.0), if active { p.accent } else { p.t3 });
            c.text(label, rect(ix + 20.0, b.y, tw + 4.0, b.h), Font::SmallBold, if active { p.t1 } else { p.t2 }, Align::Left);
            self.hits.add(b, Hit::SideTab(i as u8));
        }
        let sel_n = self.tab().map(|t| t.sel.len()).unwrap_or(0);
        let foot = if self.side_tab == 0 && sel_n > 0 { 46.0 } else { 0.0 };
        let list = rect(r.x, r.y + 52.0, r.w, r.h - 52.0 - foot);
        c.clip(list);
        if self.side_tab == 0 {
            self.paint_thumbs(c, list);
        } else {
            self.paint_outline(c, list);
        }
        c.unclip();
        if foot > 0.0 {
            let f = rect(r.x, r.bottom() - foot, r.w, foot);
            c.hline(f.x, f.y, f.w, p.line);
            let label = if sel_n == 1 { tr("1 page").to_string() } else { fill(tr("{n} pages"), &[&sel_n.to_string()]) };
            c.text(&label, rect(f.x + 16.0, f.y, 70.0, f.h), Font::Small, p.t2, Align::Left);
            let editable = self.tab().is_some_and(|t| t.editable());
            let mut x = f.right() - 12.0;
            for (icon, id) in [(&icons::TRASH, "delete_pages"), (&ic::EXTRACT, "extract"), (&ic::ROT_R, "rotate_r"), (&ic::ROT_L, "rotate_l")] {
                x -= 32.0;
                self.icon_btn(c, icon, rect(x, f.y + 7.0, 32.0, 32.0), Hit::Btn(id), false, editable);
            }
        }
    }

    fn paint_thumbs(&mut self, c: &mut Canvas, list: Rect) {
        let p = self.pal.clone();
        let Some(t) = self.tab() else { return };
        let sizes = t.sizes();
        let (ver, sel, dark) = (t.ver, t.sel.clone(), self.set.dark_pages);
        let cur = self.current_page();
        let scale = c.scale();
        let mut y = list.y + 8.0 - self.side_scroll;
        let mut wanted = Vec::new();
        let drag_to = match &self.drag {
            Some(Drag::Thumb { active: true, to, .. }) => Some(*to),
            _ => None,
        };
        for (i, s) in sizes.iter().enumerate() {
            let k = (THUMB_W / s.0.max(1.0)).min(150.0 / s.1.max(1.0));
            let (tw, th) = ((s.0 * k).round(), (s.1 * k).round());
            let item = rect(list.x + 8.0, y, list.w - 16.0, th + 30.0);
            let tr_ = rect(list.x + (list.w - tw) / 2.0, y + 6.0, tw, th);
            if item.bottom() >= list.y && item.y <= list.bottom() {
                if i == cur {
                    c.fill_round(item, 10.0, p.hover);
                }
                draw::shadow(c, tr_, 1.0, 0.8);
                c.fill(tr_, if dark { p.paper_dark } else { Color::hex(0xffffff) });
                let job = Job { ver, page: i, w: (tw * scale).round() as u32, h: (th * scale).round() as u32, dark };
                if let Some(px) = self.pixels(&job) {
                    self.blit(c, &job, &px, tr_);
                } else if let Some((j, px)) = self.fallback(&job) {
                    self.blit(c, &j, &px, tr_);
                }
                wanted.push(job);
                if sel.contains(&i) {
                    c.stroke_round(tr_.inset(-3.0), 4.0, p.accent, 2.0);
                }
                c.text(
                    &(i + 1).to_string(),
                    rect(item.x, tr_.bottom() + 2.0, item.w, 20.0),
                    Font::Small,
                    if sel.contains(&i) { p.accent } else { p.t2 },
                    Align::Center,
                );
                self.hits.add(item.intersect(&list), Hit::Thumb(i));
            }
            self.thumb_rects.push((i, item));
            if drag_to == Some(i) {
                c.fill_round(rect(item.x + 6.0, item.y - 3.0, item.w - 12.0, 3.0), 1.5, p.accent);
            }
            y += th + 34.0;
        }
        if drag_to == Some(sizes.len()) {
            c.fill_round(rect(list.x + 14.0, y - 2.0, list.w - 28.0, 3.0), 1.5, p.accent);
        }
        self.side_h = y + self.side_scroll - list.y + 8.0;
        self.side_scroll = self.side_scroll.clamp(0.0, (self.side_h - list.h).max(0.0));
        self.pending_thumbs = wanted;
    }

    fn paint_outline(&mut self, c: &mut Canvas, list: Rect) {
        let p = self.pal.clone();
        let Some(t) = self.tab() else { return };
        let items = t.outline.clone();
        if items.is_empty() {
            c.icon(&ic::OUTLINE, rect(list.x + (list.w - 20.0) / 2.0, list.y + 40.0, 20.0, 20.0), p.t3);
            c.text(tr("No bookmarks in this file."), rect(list.x + 16.0, list.y + 68.0, list.w - 32.0, 24.0), Font::Small, p.t3, Align::Center);
            self.side_h = 0.0;
            return;
        }
        let mut y = list.y + 4.0 - self.side_scroll;
        for (i, it) in items.iter().enumerate() {
            let row = rect(list.x + 8.0, y, list.w - 16.0, 40.0);
            if row.bottom() >= list.y && row.y <= list.bottom() {
                let hit = Hit::Outline(i);
                if self.hovered(&hit) {
                    c.fill_round(row, 10.0, p.hover);
                }
                let ind = 10.0 + it.level.min(5) as f32 * 14.0;
                c.text(
                    &it.title,
                    rect(row.x + ind, row.y, row.w - ind - 34.0, row.h),
                    if it.level == 0 { Font::BodyBold } else { Font::Body },
                    p.t1,
                    Align::Left,
                );
                c.text(&(it.page + 1).to_string(), rect(row.right() - 34.0, row.y, 28.0, row.h), Font::Small, p.t3, Align::Right);
                self.hits.add(row.intersect(&list), hit);
            }
            y += 42.0;
        }
        self.side_h = y + self.side_scroll - list.y + 8.0;
    }

    // ───────────────────────── pages ─────────────────────────

    fn paint_pages(&mut self, c: &mut Canvas, area: Rect) {
        let p = self.pal.clone();
        c.fill(area, p.area);
        let Some(t) = self.tab() else { return };
        match &t.status {
            Status::Loading if t.win_sizes.is_empty() && t.geoms.is_empty() => {
                c.text(tr("Opening…"), area, Font::Body, p.t3, Align::Center);
                let thumbs = std::mem::take(&mut self.pending_thumbs);
                self.want(thumbs);
                return;
            }
            Status::Password { .. } => {
                let mid = area.centered(380.0, 160.0);
                c.icon(&ic::LOCK, rect(mid.x + (mid.w - 28.0) / 2.0, mid.y, 28.0, 28.0), p.t2);
                c.text(tr("This file is protected"), rect(mid.x, mid.y + 40.0, mid.w, 26.0), Font::Large, p.t1, Align::Center);
                c.text(tr("It needs a password to open."), rect(mid.x, mid.y + 70.0, mid.w, 22.0), Font::Body, p.t3, Align::Center);
                let bw = c.text_width(tr("Enter the password"), Font::BodyBold) + 40.0;
                self.pill(c, rect(mid.x + (mid.w - bw) / 2.0, mid.y + 108.0, bw, 36.0), tr("Enter the password"), true, Hit::Btn("password"));
                return;
            }
            Status::Failed(e) => {
                let mid = area.centered(460.0, 100.0);
                c.icon(&ic::DOC, rect(mid.x + (mid.w - 28.0) / 2.0, mid.y, 28.0, 28.0), p.t2);
                c.text(tr("This file could not be opened"), rect(mid.x, mid.y + 40.0, mid.w, 26.0), Font::Large, p.t1, Align::Center);
                let e = e.clone();
                c.text(&e, rect(mid.x, mid.y + 70.0, mid.w, 22.0), Font::Body, p.t3, Align::Center);
                return;
            }
            _ => {}
        }
        let k = t.k();
        let (ver, sx, sy, dark) = (t.ver, t.sx, t.sy, self.set.dark_pages);
        let rects = t.rects.clone();
        let scale = c.scale();
        let settled = self.zoom_changed.elapsed() > Duration::from_millis(160);
        let center = sy + area.h / 2.0;
        let mut wanted: Vec<(f32, Job)> = Vec::new();
        c.clip(area);
        for (i, r) in rects.iter().enumerate() {
            let sr = rect(r.x - sx + area.x, r.y - sy + area.y, r.w, r.h);
            let near = sr.bottom() >= area.y - area.h * 0.6 && sr.y <= area.bottom() + area.h * 0.6;
            if !near {
                continue;
            }
            // pixels: at most 6000 wide (larger is shown stretched)
            let lim = (6000.0 / (sr.w * scale).max(1.0)).min(1.0);
            let job = Job { ver, page: i, w: (sr.w * scale * lim).round().max(1.0) as u32, h: (sr.h * scale * lim).round().max(1.0) as u32, dark };
            let visible = sr.bottom() >= area.y && sr.y <= area.bottom();
            if !visible {
                // just above or below: asked for ahead of time
                if settled {
                    wanted.push((f32::MAX / 2.0, job));
                }
                continue;
            }
            draw::shadow(c, sr, 2.0, if p.dark { 2.2 } else { 1.0 });
            c.fill(sr, if dark { p.paper_dark } else { Color::hex(0xffffff) });
            let exact = self.pixels(&job);
            match exact {
                Some(px) => self.blit(c, &job, &px, sr),
                None => {
                    let fb = self.fallback(&job);
                    let has = fb.is_some();
                    if let Some((j, px)) = fb {
                        self.blit(c, &j, &px, sr);
                    }
                    if settled || !has {
                        wanted.push(((r.y + r.h / 2.0 - center).abs(), job));
                    }
                }
            }
            self.hits.add(sr.intersect(&area), Hit::Page(i));
            self.paint_overlays(c, i, sr, k);
        }
        self.paint_drag_preview(c, k);
        c.unclip();
        // scrollbar
        if let Some(t) = self.tab() {
            let (ch, sy) = (t.content.1, t.sy);
            if ch > area.h + 1.0 {
                let th = (area.h * area.h / ch).max(36.0);
                let ty = area.y + (area.h - th) * (sy / (ch - area.h));
                let bar = rect(area.right() - 9.0, ty + 2.0, 6.0, th - 4.0);
                let hot = self.hovered(&Hit::VScroll) || matches!(self.drag, Some(Drag::VScroll { .. }));
                c.fill_round(bar, 3.0, p.t3.fade(if hot { 0.8 } else { 0.45 }));
                self.hits.add(rect(area.right() - 14.0, ty, 14.0, th), Hit::VScroll);
            }
        }
        // the page in the middle first, thumbnails after the pages
        wanted.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut jobs: Vec<Job> = std::mem::take(&mut self.pending_thumbs);
        jobs.extend(wanted.into_iter().map(|w| w.1));
        self.want(jobs);
    }

    /// Search matches, form fields and the annotations being made, over page `i` (screen rect `sr`).
    fn paint_overlays(&mut self, c: &mut Canvas, i: usize, sr: Rect, k: f32) {
        let p = self.pal.clone();
        let Some(t) = self.tab() else { return };
        let g = t.geom(i);
        let to = |v: [f32; 4]| rect(sr.x + v[0] * k, sr.y + v[1] * k, v[2] * k, v[3] * k);
        // search
        if self.search_open && !t.found.is_empty() {
            let cur = t.found.get(t.found_i).copied();
            let boxes: Vec<(bool, Rect)> = t
                .found
                .iter()
                .filter(|f| f.page == i)
                .flat_map(|f| {
                    let is_cur = Some(*f) == cur;
                    t.texts.get(i).map(|pt_| text::line_boxes(pt_, f.start, f.len)).unwrap_or_default().into_iter().map(move |b| (is_cur, b))
                })
                .map(|(cur, b)| (cur, to(g.view_rect(b))))
                .collect();
            for (is_cur, b) in boxes {
                let b = b.inset(-1.5);
                c.fill_round(b, 2.0, if is_cur { Color::hexa(0xff9f0a, 0.5) } else { Color::hexa(0xffd60a, 0.35) });
                if is_cur {
                    c.stroke_round(b, 2.0, Color::hex(0xff9f0a), 1.5);
                }
            }
        }
        let Some(t) = self.tab() else { return };
        // form fields
        let fields: Vec<(usize, Rect, bool, Option<FieldValue>)> = t
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.page == i)
            .map(|(j, f)| (j, to(g.view_rect(f.rect)), f.check, t.values.get(&f.id).cloned()))
            .collect();
        for (j, fr, check, pending) in fields {
            let hit = Hit::Field(j);
            let editing = self.edit_for == Some(EditFor::Field(j));
            c.fill(fr, Color::hexa(0x3b82f6, if self.hovered(&hit) || editing { 0.16 } else { 0.08 }));
            c.stroke_round(fr, 1.0, Color::hexa(0x3b82f6, 0.55), 1.0);
            match pending {
                Some(FieldValue::Text(s)) if !editing => {
                    c.fill(fr.inset(1.0), Color::hex(0xffffff));
                    let size = (fr.h * 0.62).clamp(6.0, 14.0 * k);
                    draw::text_wrap(
                        c,
                        &s,
                        rect(fr.x + 2.0 * k, fr.y + (fr.h - size * 1.2) / 2.0, fr.w, fr.h),
                        Some("Arial"),
                        size,
                        400,
                        Color::hex(0x000000),
                        Align::Left,
                    );
                }
                Some(FieldValue::Check(on)) if check => {
                    c.fill(fr.inset(1.0), Color::hex(0xffffff));
                    if on {
                        let s = fr.w.min(fr.h);
                        let q = fr.centered(s, s);
                        draw::polyline(
                            c,
                            &[(q.x + s * 0.2, q.y + s * 0.48), (q.x + s * 0.42, q.y + s * 0.72), (q.x + s * 0.8, q.y + s * 0.24)],
                            (s * 0.12).max(1.0),
                            Color::hex(0x000000),
                        );
                    }
                }
                _ => {}
            }
            self.hits.add(fr, hit);
        }
        // annotations being made
        let Some(t) = self.tab() else { return };
        let annots: Vec<(usize, crate::edit::Annot)> = t.annots.iter().enumerate().filter(|(_, a)| a.page == i).map(|(j, a)| (j, a.clone())).collect();
        for (j, a) in annots {
            let r = to(a.rect);
            let selected = self.selected == Some(j);
            match a.kind {
                Kind::Highlight => {
                    for b in &a.boxes {
                        let br = to(*b);
                        c.fill(br, col3(a.color).fade(0.42));
                        self.hits.add(br, Hit::Annot(j));
                    }
                }
                Kind::Note => {
                    c.fill_round(r, 3.0 * k.min(2.0), col3(a.color));
                    c.stroke_round(r, 3.0 * k.min(2.0), col3(a.color).mix(Color::hex(0x000000), 0.45), 1.0);
                    for (fy, fw) in [(0.32, 0.56), (0.5, 0.56), (0.68, 0.38)] {
                        c.fill(rect(r.x + r.w * 0.22, r.y + r.h * fy - 0.6, r.w * fw, 1.3), Color::hexa(0x000000, 0.55));
                    }
                    if (selected || self.hovered(&Hit::Annot(j))) && !a.text.is_empty() {
                        let w = 220.0;
                        let h = c.text_height(&a.text, w - 20.0, Font::Small) + 16.0;
                        let b = rect(r.right() + 6.0, r.y, w, h);
                        draw::shadow(c, b, 8.0, 1.0);
                        c.fill_round(b, 8.0, Color::hex(0xfff8d6));
                        c.text_block(&a.text, rect(b.x + 10.0, b.y + 8.0, w - 20.0, h), Font::Small, Color::hex(0x1a1a1a));
                    }
                    self.hits.add(r, Hit::Annot(j));
                }
                Kind::Ink | Kind::Signature => {
                    for s in &a.strokes {
                        let pts: Vec<(f32, f32)> = s.iter().map(|q| (r.x + q[0] * r.w, r.y + q[1] * r.h)).collect();
                        draw::polyline(c, &pts, (a.width * k).max(1.0), col3(a.color));
                    }
                    self.hits.add(r.inset(-4.0), Hit::Annot(j));
                }
                Kind::Text => {
                    if a.text.is_empty() {
                        c.stroke_round(r, 2.0, p.accent.fade(0.7), 1.0);
                    }
                    draw::text_wrap(
                        c,
                        &a.text,
                        rect(r.x + 2.0 * k, r.y + 2.0 * k, (r.w - 4.0 * k).max(1.0), r.h * 4.0),
                        Some("Arial"),
                        a.width * k,
                        400,
                        col3(a.color),
                        Align::Left,
                    );
                    self.hits.add(r, Hit::Annot(j));
                }
            }
            if selected {
                let fr = r;
                draw::frame(c, fr, p.accent);
                if matches!(a.kind, Kind::Ink | Kind::Signature | Kind::Text) {
                    let hr = rect(fr.right() - 2.0, fr.bottom() - 2.0, 10.0, 10.0);
                    c.fill_round(hr, 5.0, p.accent);
                    c.stroke_round(hr, 5.0, Color::hex(0xffffff), 1.5);
                    self.hits.add(hr.inset(-4.0), Hit::Handle(j));
                }
            }
        }
    }

    fn paint_drag_preview(&mut self, c: &mut Canvas, k: f32) {
        let p = self.pal.clone();
        match self.drag.clone() {
            Some(Drag::Highlight { page, a, b }) => {
                let s = self.to_screen(page, a.0.min(b.0), a.1.min(b.1));
                let r = rect(s.x, s.y, (a.0 - b.0).abs() * k, (a.1 - b.1).abs() * k);
                c.fill(r, col3(HL_COLORS[self.hl_color]).fade(0.3));
            }
            Some(Drag::TextBox { page, a, b }) => {
                let s = self.to_screen(page, a.0.min(b.0), a.1.min(b.1));
                let r = rect(s.x, s.y, (a.0 - b.0).abs() * k, (a.1 - b.1).abs() * k);
                c.stroke_round(r, 2.0, p.accent, 1.5);
            }
            Some(Drag::Ink { page, pts }) => {
                let s: Vec<(f32, f32)> = pts.iter().map(|q| self.to_screen(page, q.0, q.1)).map(|q| (q.x, q.y)).collect();
                draw::polyline(c, &s, 2.0 * k, col3(INK_COLORS[self.ink_color]));
            }
            _ => {}
        }
    }

    // ───────────────────────── navigation pill, search ─────────────────────────

    fn paint_nav(&mut self, c: &mut Canvas, area: Rect) {
        let p = self.pal.clone();
        let Some(t) = self.tab() else { return };
        if t.status != Status::Ready {
            return;
        }
        let (n, zoom, fit) = (t.pages(), t.zoom, t.fit);
        let cur = self.current_page();
        let w = 404.0;
        let r = rect(area.x + (area.w - w) / 2.0, area.bottom() - 54.0, w, 40.0);
        if r.x < area.x + 4.0 {
            return;
        }
        draw::shadow(c, r, 20.0, if p.dark { 2.0 } else { 1.0 });
        c.fill_round(r, 20.0, p.card);
        c.stroke_round(r, 20.0, p.line, 1.0);
        let mut x = r.x + 6.0;
        let b = |x: f32| rect(x, r.y + 4.0, 32.0, 32.0);
        self.icon_btn(c, &icons::BACK, b(x), Hit::Btn("prev_page"), false, cur > 0);
        x += 34.0;
        let pb = rect(x, r.y + 6.0, 92.0, 28.0);
        let editing = self.edit_for == Some(EditFor::PageBox);
        c.fill_round(pb, 14.0, if editing || self.hovered(&Hit::PageBox) { p.fill2 } else { p.fill });
        if !editing {
            c.text(&format!("{} / {n}", cur + 1), pb, Font::BodyBold, p.t1, Align::Center);
        }
        self.hits.add(pb, Hit::PageBox);
        self.page_box = pb;
        x += 94.0;
        self.icon_btn(c, &icons::FORWARD, b(x), Hit::Btn("next_page"), false, cur + 1 < n);
        x += 38.0;
        c.vline(x - 2.0, r.y + 10.0, 20.0, p.line);
        self.icon_btn(c, &ic::MINUS, b(x), Hit::Btn("zoom_out"), false, true);
        x += 32.0;
        let zr = rect(x, r.y + 6.0, 56.0, 28.0);
        let zh = Hit::Btn("zoom_100");
        if self.hovered(&zh) {
            c.fill_round(zr, 14.0, p.hover);
        }
        c.text(&format!("{}%", (zoom * 100.0).round()), zr, Font::Small, p.t1, Align::Center);
        self.hits.add(zr, zh);
        x += 56.0;
        self.icon_btn(c, &icons::PLUS, b(x), Hit::Btn("zoom_in"), false, true);
        x += 38.0;
        c.vline(x - 2.0, r.y + 10.0, 20.0, p.line);
        self.icon_btn(c, &ic::FIT_W, b(x), Hit::Btn("fit_width"), fit == Fit::Width, true);
        x += 34.0;
        self.icon_btn(c, &ic::FIT_P, b(x), Hit::Btn("fit_page"), fit == Fit::Page, true);
    }

    fn paint_search(&mut self, c: &mut Canvas, area: Rect) {
        let p = self.pal.clone();
        let w = 388.0f32.min(area.w - 24.0);
        let r = rect(area.right() - w - 16.0, area.y + 12.0, w, 44.0);
        draw::shadow(c, r, 16.0, if p.dark { 2.0 } else { 1.0 });
        c.fill_round(r, 16.0, p.card);
        c.stroke_round(r, 16.0, p.line, 1.0);
        let field = rect(r.x + 6.0, r.y + 6.0, w - 184.0, 32.0);
        let focused = self.edit_for == Some(EditFor::Search);
        c.fill_round(field, 16.0, p.fill);
        if focused {
            c.stroke_round(field, 16.0, p.accent.fade(0.7), 1.5);
        }
        c.icon(&icons::SEARCH, rect(field.x + 11.0, field.y + 9.0, 14.0, 14.0), p.t2);
        if !focused {
            let (s, ink) = if self.query.is_empty() { (tr("Find in the document"), p.t3) } else { (self.query.as_str(), p.t1) };
            let s = s.to_string();
            c.text(&s, rect(field.x + 30.0, field.y, field.w - 36.0, field.h), Font::Body, ink, Align::Left);
        }
        self.hits.add(field, Hit::SearchBox);
        self.edit_rect = Some(rect(field.x + 30.0, field.y + 6.0, field.w - 36.0, field.h - 12.0));
        let Some(t) = self.tab() else { return };
        let label = if self.query.trim().is_empty() {
            String::new()
        } else if !t.texts_ready {
            tr("Reading…").to_string()
        } else if t.texts.iter().all(|x| x.chars.is_empty()) {
            tr("No text").to_string()
        } else if t.found.is_empty() {
            tr("No results").to_string()
        } else {
            fill(tr("{i} of {n}"), &[&(t.found_i + 1).to_string(), &t.found.len().to_string()])
        };
        let has = !t.found.is_empty();
        let mut x = r.right() - 104.0;
        c.text(&label, rect(field.right() + 4.0, r.y, x - field.right() - 6.0, r.h), Font::Small, p.t2, Align::Center);
        let b = |x: f32| rect(x, r.y + 6.0, 32.0, 32.0);
        self.icon_btn(c, &ic::CHEV_UP, b(x), Hit::Btn("search_prev"), false, has);
        x += 33.0;
        self.icon_btn(c, &icons::CHEVRON_DOWN, b(x), Hit::Btn("search_next"), false, has);
        x += 33.0;
        self.icon_btn(c, &icons::CLOSE, b(x), Hit::Btn("search_close"), false, true);
    }

    // ───────────────────────── start screen ─────────────────────────

    fn paint_start(&mut self, c: &mut Canvas, body: Rect) {
        let p = self.pal.clone();
        c.fill(body, p.area);
        let w = 520.0f32.min(body.w - 40.0);
        let x = body.x + (body.w - w) / 2.0;
        let recent: Vec<std::path::PathBuf> = self.mem.recent.iter().take(7).cloned().collect();
        let list_h = if recent.is_empty() { 0.0 } else { 40.0 + recent.len() as f32 * 52.0 };
        let total = 250.0 + list_h;
        let mut y = body.y + ((body.h - total) / 2.0).max(24.0);
        let wic = self.wic.clone();
        let px = (72.0 * c.scale()).round() as u32;
        c.image(0xF00D_1000 + px as u64, rect(x + (w - 72.0) / 2.0, y, 72.0, 72.0), 1.0, || logo(&wic, px));
        y += 84.0;
        c.text("Plume PDF", rect(x, y, w, 36.0), Font::Hero, p.t1, Align::Center);
        y += 38.0;
        c.text(tr("Read, sign, annotate and merge PDF files."), rect(x, y, w, 22.0), Font::Body, p.t3, Align::Center);
        y += 40.0;
        let (b1, b2) = (tr("Open a PDF"), tr("Merge PDFs…"));
        let w1 = c.text_width(b1, Font::BodyBold) + 40.0;
        let w2 = c.text_width(b2, Font::BodyBold) + 40.0;
        let bx = x + (w - w1 - w2 - 12.0) / 2.0;
        self.pill(c, rect(bx, y, w1, 36.0), b1, true, Hit::Btn("open"));
        self.pill(c, rect(bx + w1 + 12.0, y, w2, 36.0), b2, false, Hit::Btn("merge"));
        y += 52.0;
        c.text(tr("Or drop PDF files on this window."), rect(x, y, w, 20.0), Font::Small, p.t3, Align::Center);
        y += 36.0;
        if !recent.is_empty() {
            c.text(tr("Recent"), rect(x + 4.0, y, w, 24.0), Font::Title, p.t1, Align::Left);
            y += 32.0;
            let card = rect(x, y, w, recent.len() as f32 * 52.0 + 8.0);
            c.fill_round(card, 18.0, p.card);
            c.stroke_round(card, 18.0, p.line, 1.0);
            for (i, path) in recent.iter().enumerate() {
                let row = rect(card.x + 4.0, card.y + 4.0 + i as f32 * 52.0, card.w - 8.0, 52.0);
                let hit = Hit::Recent(i);
                if self.hovered(&hit) {
                    c.fill_round(row, 10.0, p.hover);
                }
                c.icon(&ic::DOC, rect(row.x + 14.0, row.y + 16.0, 20.0, 20.0), p.accent);
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let dir = path.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
                c.text(&name, rect(row.x + 46.0, row.y + 7.0, row.w - 90.0, 20.0), Font::BodyBold, p.t1, Align::Left);
                c.text(&dir, rect(row.x + 46.0, row.y + 26.0, row.w - 90.0, 18.0), Font::Small, p.t3, Align::Left);
                self.hits.add(row, hit);
                let xr = rect(row.right() - 36.0, row.y + 13.0, 26.0, 26.0);
                if self.hovered(&Hit::RecentForget(i)) || self.hovered(&Hit::Recent(i)) {
                    self.icon_btn(c, &icons::CLOSE, xr, Hit::RecentForget(i), false, true);
                } else {
                    self.hits.add(xr, Hit::RecentForget(i));
                }
            }
        }
    }

    // ───────────────────────── menus, dialogs, messages ─────────────────────────

    fn paint_menu(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let p = self.pal.clone();
        let Some(menu) = &self.menu else { return };
        let items = menu.items.clone();
        let mw = items.iter().map(|(l, _)| c.text_width(l, Font::Body)).fold(160.0, f32::max) + 36.0;
        let mh = items.len() as f32 * 38.0 + 12.0;
        let x = menu.x.clamp(6.0, (w - mw - 6.0).max(6.0));
        let y = menu.y.clamp(TITLE_H, (h - mh - 6.0).max(TITLE_H));
        let r = rect(x, y, mw, mh);
        draw::shadow(c, r, 16.0, if p.dark { 2.5 } else { 1.2 });
        c.fill_round(r, 16.0, p.card.over(p.chrome));
        c.stroke_round(r, 16.0, p.line, 1.0);
        for (i, (label, _)) in items.iter().enumerate() {
            let row = rect(r.x + 6.0, r.y + 6.0 + i as f32 * 38.0, r.w - 12.0, 38.0);
            let hit = Hit::Menu(i);
            if self.hovered(&hit) {
                c.fill_round(row, 10.0, p.hover);
            }
            c.text(label, rect(row.x + 12.0, row.y, row.w - 20.0, row.h), Font::Body, p.t1, Align::Left);
            self.hits.add(row, hit);
        }
    }

    /// The open dialog; returns where its text field goes (if it has one).
    fn paint_modal(&mut self, c: &mut Canvas, w: f32, h: f32) -> Option<Rect> {
        let p = self.pal.clone();
        let m = self.modal.clone()?;
        c.fill(rect(0.0, 0.0, w, h), Color::hexa(0x000000, if p.dark { 0.45 } else { 0.28 }));
        self.hits.add(rect(0.0, 0.0, w, h), Hit::Shade);
        let name = self.tab().map(|t| t.name.clone()).unwrap_or_default();
        let (title, body, ok, alt, field_h): (String, String, String, Option<String>, f32) = match &m {
            Modal::Password { wrong } => (
                tr("Password needed").into(),
                if *wrong {
                    tr("That password is not right. Try again.").to_string()
                } else {
                    fill(tr("“{name}” is protected. Enter its password to open it."), &[&name])
                },
                tr("Open").into(),
                None,
                40.0,
            ),
            Modal::ConfirmSave => (
                tr("Replace the original file?").into(),
                fill(tr("“{name}” will be overwritten with your changes. You can save a copy instead."), &[&name]),
                tr("Replace").into(),
                Some(tr("Save a copy…").into()),
                0.0,
            ),
            Modal::ConfirmClose(i) => {
                let n = self.tabs.get(*i).map(|t| t.name.clone()).unwrap_or_default();
                (tr("Close without saving?").into(), fill(tr("Changes to “{name}” will be lost."), &[&n]), tr("Close without saving").into(), None, 0.0)
            }
            Modal::ConfirmQuit => (tr("Quit without saving?").into(), tr("Some files have changes that are not saved.").into(), tr("Quit").into(), None, 0.0),
            Modal::Split => (
                tr("Split into files").into(),
                tr("Pages for each new file, separated by commas. Example: 1-3, 4-6, 7").into(),
                tr("Choose a folder…").into(),
                None,
                40.0,
            ),
            Modal::NoteText { .. } => (tr("Note").into(), tr("Ctrl+Enter to finish.").into(), tr("Done").into(), None, 120.0),
            Modal::TextBox { .. } => (tr("Text").into(), tr("Ctrl+Enter to finish.").into(), tr("Done").into(), None, 120.0),
            Modal::Signature { .. } => (
                tr("Draw your signature").into(),
                tr("It is kept on this computer only, ready to place on any page.").into(),
                tr("Save signature").into(),
                Some(tr("Clear").into()),
                180.0,
            ),
        };
        let mw = if matches!(m, Modal::Signature { .. }) { 520.0 } else { 460.0f32 };
        let body_h = c.text_height(&body, mw - 48.0, Font::Body);
        let mh = 24.0 + 30.0 + 8.0 + body_h + 16.0 + if field_h > 0.0 { field_h + 16.0 } else { 0.0 } + 36.0 + 24.0;
        let r = rect((w - mw) / 2.0, ((h - mh) / 2.0).max(TITLE_H), mw, mh);
        draw::shadow(c, r, 20.0, if p.dark { 3.0 } else { 1.5 });
        c.fill_round(r, 20.0, p.card.over(p.chrome));
        c.stroke_round(r, 20.0, p.line, 1.0);
        let mut y = r.y + 24.0;
        c.text(&title, rect(r.x + 24.0, y, r.w - 48.0, 30.0), Font::Large, p.t1, Align::Left);
        y += 38.0;
        let wrong = matches!(m, Modal::Password { wrong: true });
        c.text_block(&body, rect(r.x + 24.0, y, r.w - 48.0, body_h), Font::Body, if wrong { p.accent } else { p.t2 });
        y += body_h + 16.0;
        let mut field = None;
        if field_h > 0.0 {
            let f = rect(r.x + 24.0, y, r.w - 48.0, field_h);
            if let Modal::Signature { strokes, .. } = &m {
                c.fill_round(f, 14.0, Color::hex(0xffffff));
                c.stroke_round(f, 14.0, p.line, 1.0);
                c.hline(f.x + 24.0, f.bottom() - 40.0, f.w - 48.0, Color::hexa(0x000000, 0.18));
                if strokes.iter().all(|s| s.is_empty()) {
                    c.text(
                        tr("Sign here with the mouse or a pen"),
                        rect(f.x, f.bottom() - 36.0, f.w, 30.0),
                        Font::Small,
                        Color::hexa(0x000000, 0.35),
                        Align::Center,
                    );
                }
                for s in strokes {
                    draw::polyline(c, s, 2.4, Color::hex(0x0f1f6b));
                }
                self.hits.add(f, Hit::Pad);
                if let Some(Modal::Signature { pad, .. }) = self.modal.as_mut() {
                    *pad = f;
                }
            } else {
                c.fill_round(f, 12.0, p.fill.over(p.card.over(p.chrome)));
                c.stroke_round(f, 12.0, p.accent.fade(0.6), 1.5);
                self.hits.add(f, Hit::ModalField);
                field = Some(f);
            }
            y += field_h + 16.0;
        }
        // buttons, on the right: [alt] [cancel] [ok]
        let ow = c.text_width(&ok, Font::BodyBold) + 36.0;
        let cw = c.text_width(tr("Cancel"), Font::BodyBold) + 36.0;
        let mut bx = r.right() - 24.0 - ow;
        self.pill(c, rect(bx, y, ow, 36.0), &ok, true, Hit::ModalOk);
        bx -= cw + 8.0;
        self.pill(c, rect(bx, y, cw, 36.0), tr("Cancel"), false, Hit::ModalCancel);
        if let Some(a) = alt {
            let aw = c.text_width(&a, Font::BodyBold) + 36.0;
            self.pill(c, rect(r.x + 24.0, y, aw, 36.0), &a, false, Hit::ModalAlt);
        }
        field
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let p = self.pal.clone();
        let Some((text, _)) = &self.toast else { return };
        let text = text.clone();
        let tw = (c.text_width(&text, Font::Body) + 40.0).min(w - 40.0);
        let r = rect((w - tw) / 2.0, h - 112.0, tw, 38.0);
        draw::shadow(c, r, 19.0, 1.0);
        c.fill_round(r, 19.0, p.solid2);
        c.stroke_round(r, 19.0, p.ring, 1.0);
        c.text(&text, r.inset_xy(16.0, 0.0), Font::Body, p.t1, Align::Center);
    }

    /// The window's single text field: in search, the page box, a dialog or a form field.
    fn place_edit(&mut self, modal_field: Option<Rect>) {
        let p = self.pal.clone();
        let k = self.tab().map(|t| t.k()).unwrap_or(1.0);
        let want: Option<(Rect, String, Color, bool, bool, f32)> = match self.edit_for {
            Some(EditFor::Modal) => modal_field.map(|r| {
                let multi = matches!(self.modal, Some(Modal::NoteText { .. } | Modal::TextBox { .. }));
                let pw = matches!(self.modal, Some(Modal::Password { .. }));
                (r.inset_xy(12.0, if multi { 8.0 } else { 10.0 }), self.modal_text.clone(), p.fill.over(p.card.over(p.chrome)), pw, multi, 14.0)
            }),
            Some(EditFor::Search) if self.search_open && self.modal.is_none() && !self.tabs.is_empty() && !self.settings_open => {
                self.edit_rect.map(|r| (r, self.query.clone(), p.fill.over(p.card.over(p.area)), false, false, 13.0))
            }
            Some(EditFor::PageBox) if self.modal.is_none() => {
                Some((self.page_box.inset_xy(10.0, 5.0), self.page_box_text.clone(), p.fill2.over(p.card.over(p.area)), false, false, 13.0))
            }
            Some(EditFor::Field(i)) if self.modal.is_none() => {
                let t = self.tab();
                let fr = t.and_then(|t| {
                    let f = t.fields.get(i)?;
                    let g = t.geom(f.page);
                    let v = g.view_rect(f.rect);
                    let s = self.view_rect_screen(f.page, v);
                    self.view.contains(plume_ui::pt(s.x + 1.0, s.y + 1.0)).then_some(s)
                });
                let text = match self.field_value(i) {
                    Some(FieldValue::Text(s)) => s,
                    _ => String::new(),
                };
                fr.map(|r| (r.inset(1.0), text, Color::hex(0xffffff), false, false, (r.h * 0.6).clamp(9.0, 13.0 * k.max(1.0))))
            }
            _ => None,
        };
        match want {
            Some((r, text, bg, password, multiline, size)) => {
                if self.edit_shown != self.edit_for {
                    let fg = if matches!(self.edit_for, Some(EditFor::Field(_))) { Color::hex(0x000000) } else { p.t1 };
                    let style = EditStyle { fg, bg, size, select: Some((0, text.encode_utf16().count())), placeholder: String::new(), password, multiline };
                    self.win.edit_show(r, &text, &style);
                    self.win.edit_focus();
                    self.edit_shown = self.edit_for;
                } else {
                    self.win.edit_place(r);
                }
            }
            None => {
                if self.edit_shown.is_some() {
                    self.win.edit_hide();
                    self.edit_shown = None;
                }
            }
        }
    }
}
