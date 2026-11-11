//! The library: sidebar, the timeline grid (month by month), videos, folders, favorites; and what shows above
//! everything (caption buttons, menus, messages).

use super::{App, Hit, Page, Row, SIDEBAR_W, TOP_H, Thumb, draw};
use crate::{
    i18n::{fill, tr},
    library,
};
use plume_ui::{Align, Canvas, Color, Font, PaintCx, Rect, icons, icons::Icon, m, rect};

/// Top of the grid, under the page's title.
const GRID_Y: f32 = 100.0;
const PAD: f32 = 28.0;

impl App {
    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.hits.clear();
        self.size = (cx.w, cx.h);
        self.scale = c.scale();
        let rt = c.target();
        if self.bitmaps.begin(&rt) {
            self.thumbs.retain(|_, t| *t == Thumb::Failed);
        }
        if self.open.is_some() {
            self.paint_viewer(c, cx);
        } else {
            c.clear(self.pal.bg);
            self.paint_sidebar(c, cx.h);
            match self.page.clone() {
                Page::Settings => self.paint_settings(c, cx),
                Page::Folders => self.paint_folders(c, cx),
                _ => self.paint_grid_page(c, cx),
            }
            if self.full.is_none() {
                self.paint_caption(c, cx.w, cx.maximized, false);
            }
        }
        self.paint_menu(c, cx.w, cx.h);
        self.paint_toast(c, cx.w, cx.h);
        // thumbnails far from view are let go (graphics memory)
        let keep = (self.cols * 30).max(180);
        self.bitmaps.trim(keep);
    }

    pub fn paint_caption(&mut self, c: &mut Canvas, w: f32, maximized: bool, over: bool) {
        let p = self.pal.clone();
        let x0 = w - 3.0 * m::CAPTION_W;
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(x0 + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, 34.0);
            let hot = self.hovered(&id) || self.pressed.as_ref() == Some(&id);
            if hot {
                c.fill(
                    r,
                    if id == Hit::Close {
                        Color::hex(0xc42b1c)
                    } else if over {
                        Color::hex(0xffffff).fade(0.14)
                    } else {
                        p.t1.fade(0.1)
                    },
                );
            }
            let ink = if id == Hit::Close && hot {
                Color::hex(0xffffff)
            } else if over {
                Color::hex(0xffffff).fade(0.85)
            } else {
                p.t2
            };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    pub fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, ink: Color, on: bool) {
        let hot = self.hovered(&hit);
        if hot || on {
            c.fill_round(r, r.h / 2.0, ink.fade(if on { 0.16 } else { 0.1 }));
        }
        let s = 18.0f32.min(r.w * 0.56).round();
        let q = rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s);
        if icon.name == draw::ic::PLAY.name || icon.name == draw::ic::PAUSE.name {
            c.icon_fill(icon, q, ink);
        } else {
            c.icon(icon, q, ink);
        }
        self.hits.add(r, hit);
    }

    /// A rounded button: primary (accent) or quiet.
    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, primary: bool, hit: Hit) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit);
        let fill = if primary {
            if hot { p.accent.mix(Color::hex(0xffffff), 0.12) } else { p.accent }
        } else if hot {
            p.t1.fade(0.15)
        } else {
            p.t1.fade(0.08)
        };
        c.fill_round(r, r.h / 2.0, fill);
        c.text(label, r, Font::BodyBold, if primary { p.on_accent } else { p.t1 }, Align::Center);
        self.hits.add(r, hit);
    }

    fn paint_sidebar(&mut self, c: &mut Canvas, h: f32) {
        let p = self.pal.clone();
        let r = rect(0.0, 0.0, SIDEBAR_W, h);
        c.fill(r, p.side);
        // the name, with a small mark
        c.fill_round(rect(16.0, 15.0, 22.0, 22.0), 6.0, p.accent);
        c.icon(&draw::ic::PHOTOS, rect(20.0, 19.0, 14.0, 14.0), p.on_accent);
        c.text("Plume Media", rect(42.0, 12.0, SIDEBAR_W - 50.0, 28.0), Font::Title, p.t1, Align::Left);
        let mut y = 64.0;
        let pages = [
            (Page::Timeline, &draw::ic::PHOTOS, tr("Timeline")),
            (Page::Videos, &draw::ic::VIDEO, tr("Videos")),
            (Page::Folders, &icons::FOLDER, tr("Folders")),
            (Page::Favorites, &icons::STAR, tr("Favorites")),
        ];
        for (pg, icon, label) in pages {
            let on = self.page == pg || (matches!(self.page, Page::Folder(_)) && pg == Page::Folders);
            self.nav_item(c, rect(10.0, y, SIDEBAR_W - 20.0, 36.0), icon, label, on, Hit::Nav(pg));
            y += 40.0;
        }
        y += 10.0;
        c.hline(22.0, y, SIDEBAR_W - 44.0, p.line);
        y += 12.0;
        self.nav_item(c, rect(10.0, y, SIDEBAR_W - 20.0, 36.0), &icons::PLUS, tr("Add a folder"), false, Hit::AddFolder);
        // the scan, while it runs
        if let Some(pr) = &self.scanning {
            let n = pr.files.load(std::sync::atomic::Ordering::Relaxed);
            c.text(tr("Looking for photos and videos…"), rect(22.0, h - 104.0, SIDEBAR_W - 34.0, 18.0), Font::Small, p.t3, Align::Left);
            c.text(&format!("{n}"), rect(22.0, h - 86.0, SIDEBAR_W - 34.0, 18.0), Font::Small, p.t3, Align::Left);
        }
        let on = self.page == Page::Settings;
        self.nav_item(c, rect(10.0, h - 56.0, SIDEBAR_W - 20.0, 36.0), &icons::GEAR, tr("Settings"), on, Hit::Nav(Page::Settings));
    }

    fn nav_item(&mut self, c: &mut Canvas, r: Rect, icon: &Icon, label: &str, on: bool, hit: Hit) {
        let p = self.pal.clone();
        let hot = self.hovered(&hit);
        if on {
            c.fill_round(r, 9.0, p.accent.fade(if p.dark { 0.16 } else { 0.14 }));
        } else if hot {
            c.fill_round(r, 9.0, p.t1.fade(0.06));
        }
        let ink = if on { p.accent } else { p.t2 };
        c.icon(icon, rect(r.x + 12.0, r.y + 10.0, 16.0, 16.0), ink);
        c.text(label, rect(r.x + 40.0, r.y, r.w - 48.0, r.h), if on { Font::BodyBold } else { Font::Body }, if on { p.t1 } else { p.t2 }, Align::Left);
        self.hits.add(r, hit);
    }

    /// Title and subtitle of a page.
    fn page_title(&mut self, c: &mut Canvas, x: f32, w: f32, title: &str, sub: &str) {
        let p = self.pal.clone();
        draw::text(c, title, rect(x, 38.0, w, 34.0), 26.0, 700, p.t1, Align::Left);
        c.text(sub, rect(x, 70.0, w, 20.0), Font::Small, p.t3, Align::Left);
    }

    fn count_text(n: usize) -> String {
        if n == 1 { tr("1 item").to_string() } else { fill(tr("{n} items"), &[&group(n)]) }
    }

    fn paint_grid_page(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let p = self.pal.clone();
        let x0 = SIDEBAR_W + PAD;
        let gw = (cx.w - x0 - PAD - 6.0).max(200.0);
        let (title, back) = match &self.page {
            Page::Timeline => (tr("Timeline").to_string(), false),
            Page::Videos => (tr("Videos").to_string(), false),
            Page::Favorites => (tr("Favorites").to_string(), false),
            Page::Folder(d) => (d.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| d.display().to_string()), true),
            _ => (String::new(), false),
        };
        let tx = if back {
            self.icon_btn(c, &icons::BACK, rect(x0 - 6.0, 41.0, 32.0, 32.0), Hit::Nav(Page::Folders), p.t1, false);
            x0 + 34.0
        } else {
            x0
        };
        let sub = match &self.page {
            Page::Folder(d) => format!("{} · {}", Self::count_text(self.list.len()), d.display()),
            _ => Self::count_text(self.list.len()),
        };
        self.page_title(c, tx, gw - 160.0, &title, &sub);
        if self.list.is_empty() {
            self.paint_empty(c, rect(SIDEBAR_W, GRID_Y, cx.w - SIDEBAR_W, cx.h - GRID_Y));
            return;
        }
        if (self.rows_w - gw).abs() > 0.5 {
            self.layout(gw);
        }
        self.view_h = cx.h - GRID_Y;
        self.scroll_to = self.scroll_to.clamp(0.0, self.max_scroll());
        // smooth scrolling
        let d = self.scroll_to - self.scroll;
        self.scroll = if d.abs() < 0.5 { self.scroll_to } else { self.scroll + d * 0.3 };
        let area = rect(x0, GRID_Y, gw, self.view_h);
        let (top, bottom) = (self.scroll, self.scroll + self.view_h);
        // what to have ready: a screen above and below
        let (pre_top, pre_bottom) = (top - self.view_h, bottom + self.view_h);
        let first = self.rows.partition_point(|r| row_y(r) + self.tile + 60.0 < pre_top);
        {
            let mut want = self.wanted.lock().unwrap();
            want.clear();
            for r in self.rows[first..].iter() {
                if row_y(r) > pre_bottom {
                    break;
                }
                if let Row::Tiles { first, count, .. } = r {
                    for k in *first..first + count {
                        want.insert(self.lib[self.list[k]].key());
                    }
                }
            }
        }
        // asked first, served last: the screens around, then what is seen
        let rows: Vec<Row> = self.rows[first..].iter().take_while(|r| row_y(r) <= pre_bottom).cloned().collect();
        for r in &rows {
            let y = row_y(r);
            if (y + self.tile < top || y > bottom)
                && let Row::Tiles { first, count, .. } = r
            {
                for k in *first..first + count {
                    let idx = self.list[k];
                    let key = self.lib[idx].key();
                    if !self.bitmaps.has(key) && !self.thumb_px.contains_key(&key) {
                        self.request_thumb(idx);
                    }
                }
            }
        }
        c.clip(area);
        for r in rows {
            let y = GRID_Y + row_y(&r) - self.scroll;
            if y > cx.h || y + self.tile + 60.0 < GRID_Y {
                continue;
            }
            match r {
                Row::Head { title, n, .. } => {
                    let tw = draw::text_width(c, &title, 18.0, 600);
                    draw::text(c, &title, rect(x0, y + 14.0, gw, 28.0), 18.0, 600, p.t1, Align::Left);
                    c.text(&Self::count_text(n), rect(x0 + tw + 12.0, y + 18.0, 200.0, 22.0), Font::Small, p.t3, Align::Left);
                }
                Row::Tiles { first, count, .. } => {
                    for k in first..first + count {
                        let col = (k - first) as f32;
                        let tr_ = rect(x0 + col * (self.tile + 4.0), y, self.tile, self.tile);
                        self.paint_tile(c, k, tr_, area);
                    }
                }
            }
        }
        c.unclip();
        self.paint_scrollbar(c, cx.w - 9.0, GRID_Y, self.view_h);
    }

    fn paint_tile(&mut self, c: &mut Canvas, k: usize, r: Rect, area: Rect) {
        let p = self.pal.clone();
        let idx = self.list[k];
        let it = self.lib[idx].clone();
        let key = it.key();
        let hit = Hit::Tile(k);
        match self.thumb_bitmap(c, key) {
            Some(b) => {
                let turns = self.store.rotation(&it.path);
                draw::cover(c, &b, r, turns);
            }
            None => {
                c.fill(r, p.tile);
                if self.thumbs.get(&key) == Some(&Thumb::Failed) {
                    let s = (r.w * 0.22).min(32.0);
                    c.icon(if it.video { &draw::ic::VIDEO } else { &draw::ic::EMPTY }, rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s), p.t3);
                } else {
                    self.request_thumb(idx);
                }
            }
        }
        if it.video {
            let dur = self.durs.get(&key).copied().filter(|d| *d > 0.0).map(library::clock);
            let w = 26.0 + dur.as_ref().map_or(0.0, |d| c.text_width(d, Font::SmallBold) + 6.0);
            let b = rect(r.x + 6.0, r.y + r.h - 28.0, w, 22.0);
            c.fill_round(b, 11.0, Color::hex(0x000000).fade(0.55));
            c.icon_fill(&draw::ic::PLAY, rect(b.x + 6.0, b.y + 5.0, 12.0, 12.0), Color::hex(0xffffff));
            if let Some(d) = dur {
                c.text(&d, rect(b.x + 22.0, b.y, w - 24.0, 22.0), Font::SmallBold, Color::hex(0xffffff), Align::Left);
            }
        }
        if self.store.is_favorite(&it.path) {
            let s = rect(r.x + r.w - 26.0, r.y + 6.0, 20.0, 20.0);
            draw::circle(c, s.x + 10.0, s.y + 10.0, 10.0, Color::hex(0x000000).fade(0.45));
            c.icon_fill(&icons::STAR, rect(s.x + 4.0, s.y + 4.0, 12.0, 12.0), Color::hex(0xffd60a));
        }
        if self.hovered(&hit) {
            c.fill(r, Color::hex(0xffffff).fade(0.1));
            c.stroke_round(r, 0.0, p.accent, 2.0);
        }
        let vis = r.intersect(&area);
        if !vis.is_empty() {
            self.hits.add(vis, hit);
        }
    }

    pub fn paint_scrollbar(&mut self, c: &mut Canvas, x: f32, y: f32, h: f32) {
        if self.content_h <= self.view_h + 1.0 {
            return;
        }
        let th = (h * h / self.content_h).max(40.0);
        let ty = y + (self.scroll / self.max_scroll().max(1.0)) * (h - th);
        let hot = self.hovered(&Hit::Scrollbar) || self.drag.as_ref().is_some_and(|d| matches!(d, super::Drag::Scroll { .. }));
        let w = if hot { 8.0 } else { 5.0 };
        c.fill_round(rect(x + (8.0 - w) / 2.0, ty, w, th), w / 2.0, self.pal.t1.fade(if hot { 0.4 } else { 0.22 }));
        self.hits.add(rect(x - 4.0, ty, 14.0, th), Hit::Scrollbar);
    }

    fn paint_empty(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        let cy = r.y + r.h * 0.38;
        let cxm = r.x + r.w / 2.0;
        if self.scanning.is_some() && self.lib.is_empty() {
            c.text(tr("Looking for photos and videos…"), rect(r.x, cy, r.w, 24.0), Font::Title, p.t2, Align::Center);
            return;
        }
        let (title, text, add) = match self.page {
            Page::Favorites => (tr("No favorites yet"), tr("Press the star on a photo or a video to find it here."), false),
            Page::Videos if !self.lib.is_empty() => (tr("No videos yet"), tr("Add a folder to see its photos and videos here."), true),
            _ => (tr("No photos or videos yet"), tr("Add a folder to see its photos and videos here."), true),
        };
        draw::circle(c, cxm, cy - 30.0, 34.0, p.accent.fade(0.14));
        c.icon(&draw::ic::PHOTOS, rect(cxm - 16.0, cy - 46.0, 32.0, 32.0), p.accent);
        draw::text(c, title, rect(r.x, cy + 16.0, r.w, 30.0), 20.0, 700, p.t1, Align::Center);
        c.text(text, rect(r.x, cy + 48.0, r.w, 22.0), Font::Body, p.t3, Align::Center);
        if add {
            let w = c.text_width(tr("Add a folder"), Font::BodyBold) + 40.0;
            self.pill(c, rect(cxm - w / 2.0, cy + 86.0, w, 36.0), tr("Add a folder"), true, Hit::EmptyAdd);
        }
    }

    fn paint_folders(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let p = self.pal.clone();
        let x0 = SIDEBAR_W + PAD;
        let gw = (cx.w - x0 - PAD - 6.0).max(200.0);
        let n = self.folders.len();
        let sub = if n == 1 { tr("1 folder").to_string() } else { fill(tr("{n} folders"), &[&group(n)]) };
        self.page_title(c, x0, gw, tr("Folders"), &sub);
        if self.folders.is_empty() {
            self.paint_empty(c, rect(SIDEBAR_W, GRID_Y, cx.w - SIDEBAR_W, cx.h - GRID_Y));
            return;
        }
        let gap = 16.0;
        let cols = (((gw + gap) / (220.0 + gap)).floor() as usize).max(1);
        let cw = (gw - gap * (cols as f32 - 1.0)) / cols as f32;
        let ch = cw * 0.75 + 52.0;
        let rows = n.div_ceil(cols);
        self.content_h = rows as f32 * (ch + gap) + 24.0;
        self.view_h = cx.h - GRID_Y;
        self.scroll_to = self.scroll_to.clamp(0.0, self.max_scroll());
        let d = self.scroll_to - self.scroll;
        self.scroll = if d.abs() < 0.5 { self.scroll_to } else { self.scroll + d * 0.3 };
        let area = rect(x0, GRID_Y, gw, self.view_h);
        {
            let mut want = self.wanted.lock().unwrap();
            want.clear();
            for (_, items) in &self.folders {
                if let Some(&i) = items.first() {
                    want.insert(self.lib[i].key());
                }
            }
        }
        c.clip(area);
        for i in 0..n {
            let (row, col) = (i / cols, i % cols);
            let y = GRID_Y + row as f32 * (ch + gap) - self.scroll;
            if y > cx.h || y + ch < GRID_Y {
                continue;
            }
            let r = rect(x0 + col as f32 * (cw + gap), y, cw, ch);
            let hit = Hit::FolderCard(i);
            let hot = self.hovered(&hit);
            c.fill_round(r, 12.0, if hot { p.card.mix(p.accent, 0.08) } else { p.card });
            let img = rect(r.x + 8.0, r.y + 8.0, r.w - 16.0, cw * 0.75 - 8.0);
            let (dir, items) = self.folders[i].clone();
            let first = items.first().copied();
            match first.map(|k| self.lib[k].key()).and_then(|key| self.thumb_bitmap(c, key)) {
                Some(b) => draw::cover(c, &b, img, 0),
                None => {
                    c.fill_round(img, 8.0, p.tile);
                    c.icon(&icons::FOLDER, rect(img.x + img.w / 2.0 - 14.0, img.y + img.h / 2.0 - 14.0, 28.0, 28.0), p.t3);
                    if let Some(k) = first
                        && self.thumbs.get(&self.lib[k].key()) != Some(&Thumb::Failed)
                    {
                        self.request_thumb(k);
                    }
                }
            }
            let name = dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| dir.display().to_string());
            c.text(&name, rect(r.x + 12.0, img.y + img.h + 6.0, r.w - 24.0, 22.0), Font::BodyBold, p.t1, Align::Left);
            c.text(&Self::count_text(items.len()), rect(r.x + 12.0, img.y + img.h + 26.0, r.w - 24.0, 18.0), Font::Small, p.t3, Align::Left);
            let vis = r.intersect(&area);
            if !vis.is_empty() {
                self.hits.add(vis, hit);
            }
        }
        c.unclip();
        self.paint_scrollbar(c, cx.w - 9.0, GRID_Y, self.view_h);
    }

    fn paint_menu(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(menu) = self.menu.clone() else { return };
        let p = self.pal.clone();
        let mw = menu.items.iter().map(|(l, _, _)| c.text_width(l, Font::Body)).fold(0.0f32, f32::max).max(140.0) + 56.0;
        let mh = menu.items.len() as f32 * 34.0 + 12.0;
        let x = (menu.anchor.x + menu.anchor.w / 2.0 - if menu.up { mw / 2.0 } else { 0.0 }).clamp(8.0, w - mw - 8.0);
        let y = if menu.up { menu.anchor.y - mh - 8.0 } else { (menu.anchor.y + menu.anchor.h + 4.0).min(h - mh - 8.0) };
        let r = rect(x, y.max(8.0), mw, mh);
        c.fill_round(r.offset(0.0, 4.0), 12.0, Color::hex(0x000000).fade(0.25));
        c.fill_round(r, 12.0, if p.dark { Color::hex(0x26292c) } else { Color::hex(0xffffff) });
        c.stroke_round(r, 12.0, p.line, 1.0);
        for (i, (label, on, _)) in menu.items.iter().enumerate() {
            let ir = rect(r.x + 6.0, r.y + 6.0 + i as f32 * 34.0, r.w - 12.0, 34.0);
            let hit = Hit::Menu(i);
            if self.hovered(&hit) {
                c.fill_round(ir, 8.0, p.t1.fade(0.08));
            }
            if *on {
                c.icon(&icons::CHECK, rect(ir.x + 10.0, ir.y + 10.0, 14.0, 14.0), p.accent);
            }
            c.text(label, rect(ir.x + 34.0, ir.y, ir.w - 40.0, ir.h), Font::Body, p.t1, Align::Left);
            self.hits.add(ir, hit);
        }
        // a click elsewhere closes it (see MouseDown); the menu itself catches clicks between its items
        self.hits.add(r, Hit::Panel);
        for i in 0..menu.items.len() {
            let ir = rect(r.x + 6.0, r.y + 6.0 + i as f32 * 34.0, r.w - 12.0, 34.0);
            self.hits.add(ir, Hit::Menu(i));
        }
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((text, _)) = self.toast.clone() else { return };
        let tw = (c.text_width(&text, Font::Body) + 36.0).min(w - 40.0);
        let bottom = if self.video().is_some() { CONTROLS_BOTTOM } else { 32.0 };
        let r = rect((w - tw) / 2.0, h - bottom - 44.0, tw, 40.0);
        c.fill_round(r, 20.0, Color::hex(0x1f2225).fade(0.96));
        c.stroke_round(r, 20.0, Color::hex(0xffffff).fade(0.08), 1.0);
        c.text(&text, r.inset_xy(18.0, 0.0), Font::Body, Color::hex(0xffffff), Align::Center);
    }
}

const CONTROLS_BOTTOM: f32 = 120.0;

fn row_y(r: &Row) -> f32 {
    match r {
        Row::Head { y, .. } | Row::Tiles { y, .. } => *y,
    }
}

/// 12345 → "12,345" (English) or "12 345" (French).
pub fn group(n: usize) -> String {
    let s = n.to_string();
    let sep = if crate::i18n::french() { '\u{202f}' } else { ',' };
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}

const _TOP: f32 = TOP_H;

#[cfg(test)]
mod tests {
    #[test]
    fn grouping() {
        let _l = crate::i18n::TEST_LOCK.lock();
        crate::i18n::set("en");
        assert_eq!(super::group(1234567), "1,234,567");
        assert_eq!(super::group(12), "12");
    }
}
