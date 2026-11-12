//! The window's parts: title bar, start screen, media bin, preview (drawn from the clips' frames, with the handles
//! of the selected picture), the dialogs (export, shortcuts, settings), the toast, and the small controls they share.

use super::{App, BinTab, Dialog, ExportState, Handle, Hit, Screen, TOP_H, Tools, draw, draw::ic, text_presets};
use crate::{
    export,
    ffmpeg::fit_box,
    i18n::{fill, tr},
    model::{self, MediaKind, TrackKind, clock, timecode},
    preview::{self, Key},
    render,
};
use plume_ui::{Align, Canvas, Color, EditStyle, PaintCx, Rect, icons, icons::Icon, m, rect};
use std::path::PathBuf;

pub const PAD: f32 = 16.0;
pub const GAP: f32 = 12.0;
pub const BIN_W: f32 = 300.0;
pub const PROPS_W: f32 = 320.0;

impl App {
    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let p = self.pal.clone();
        self.hits.clear();
        c.clear(p.bg);
        let (w, h) = (cx.w, cx.h);
        self.advance_play(cx);
        let mut fields: Vec<(&'static str, Rect)> = Vec::new();
        match self.screen {
            Screen::Start => self.paint_start(c, rect(0.0, TOP_H, w, h - TOP_H)),
            Screen::Edit => {
                let mut body = rect(PAD, TOP_H, w - 2.0 * PAD, h - TOP_H - PAD);
                let tl_h = (body.h * 0.44).clamp(250.0, 480.0);
                let tl = body.cut_bottom(tl_h);
                body.h -= GAP;
                let bin = body.cut_left(BIN_W);
                body.cut_left(GAP);
                let props = body.cut_right(PROPS_W);
                body.cut_right(GAP);
                self.paint_timeline(c, tl);
                self.paint_bin(c, bin);
                self.paint_preview(c, body);
                self.paint_props(c, props, &mut fields);
            }
        }
        self.paint_top(c, w);
        self.paint_caption(c, w, cx);
        if let Some(d) = self.dialog {
            self.paint_dialog(c, d, w, h);
        }
        self.paint_toast(c, w, h);
        self.place_edit(&fields);
    }

    /// Moves the playhead while playing or shuttling.
    fn advance_play(&mut self, cx: &mut PaintCx) {
        if let Some(pl) = &mut self.player {
            let (f, ended) = pl.tick();
            self.playhead = f;
            if ended {
                self.stop_play();
                self.playhead = self.project.duration();
            } else {
                cx.animate = true;
            }
        } else if self.shuttle != 0.0 {
            let (at, from) = self.shuttle_at;
            let f = from + (at.elapsed().as_secs_f64() * self.shuttle) as i64;
            let end = self.project.duration();
            self.playhead = f.clamp(0, end);
            if f <= 0 || f >= end {
                self.shuttle = 0.0;
            } else {
                cx.animate = true;
            }
        }
    }

    // ───────────────────────── controls ─────────────────────────

    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, primary: bool, hit: Hit, enabled: bool) {
        let p = self.pal.clone();
        let hot = enabled && self.hovered(&hit);
        let down = hot && self.pressed.as_ref() == Some(&hit);
        let fill = if primary && enabled {
            if down {
                p.accent.mix(Color::hex(0x000000), 0.12)
            } else if hot {
                p.accent.mix(Color::hex(0xffffff), 0.12)
            } else {
                p.accent
            }
        } else if down {
            p.fill2.mix(Color::hex(0x000000), 0.1)
        } else if hot {
            p.fill2
        } else {
            p.fill
        };
        c.fill_round(r, r.h / 2.0, fill);
        let ink = if primary && enabled {
            p.on_accent
        } else if enabled {
            p.t1
        } else {
            p.t3
        };
        draw::text(c, label, r, 13.0, 600, ink, Align::Center);
        if enabled {
            self.hits.add(r, hit);
        }
    }

    /// A pill with an icon before its label.
    pub fn pill_icon(&mut self, c: &mut Canvas, r: Rect, icon: &Icon, label: &str, primary: bool, hit: Hit) {
        self.pill(c, r, "", primary, hit, true);
        let ink = if primary { self.pal.on_accent } else { self.pal.t1 };
        let tw = draw::text_width(c, label, 13.0, 600);
        let x = r.x + (r.w - tw - 22.0) / 2.0;
        c.icon(icon, rect(x, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), ink);
        draw::text(c, label, rect(x + 22.0, r.y, tw + 4.0, r.h), 13.0, 600, ink, Align::Left);
    }

    pub fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, color: Color, enabled: bool) {
        let hot = enabled && self.hovered(&hit);
        if hot {
            c.fill_round(r, r.h / 2.0, if self.pressed.as_ref() == Some(&hit) { self.pal.fill2 } else { self.pal.hover });
        }
        let s = 16.0f32.min(r.w - 8.0);
        let ink = if !enabled {
            self.pal.t4
        } else if hot {
            self.pal.t1
        } else {
            color
        };
        c.icon(icon, rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s), ink);
        if enabled {
            self.hits.add(r, hit);
        }
    }

    /// A segmented control in `r` (its height 32): the chosen one raised.
    pub fn segmented(&mut self, c: &mut Canvas, r: Rect, labels: &[&str], cur: usize, key: &'static str) {
        let p = self.pal.clone();
        c.fill_round(r, r.h / 2.0, p.fill);
        let n = labels.len().max(1) as f32;
        let sw = (r.w - 6.0) / n;
        for (i, l) in labels.iter().enumerate() {
            let s = rect(r.x + 3.0 + i as f32 * sw, r.y + 3.0, sw, r.h - 6.0);
            let hit = Hit::Chip(key, i);
            if i == cur {
                c.fill_round(s, s.h / 2.0, p.fill2.over(p.card.over(p.bg)));
                c.stroke_round(s, s.h / 2.0, p.ring, 1.0);
            } else if self.hovered(&hit) {
                c.fill_round(s, s.h / 2.0, p.hover);
            }
            draw::text(c, l, s.inset_xy(4.0, 0.0), 12.0, 600, if i == cur { p.t1 } else { p.t2 }, Align::Center);
            self.hits.add(s, hit);
        }
    }

    pub fn switch(&mut self, c: &mut Canvas, x: f32, y: f32, on: bool, hit: Hit) {
        let p = self.pal.clone();
        let r = rect(x, y, 40.0, 24.0);
        c.fill_round(r, 12.0, if on { p.accent } else { p.fill2 });
        let k = if on { r.x + 18.0 } else { r.x + 2.0 };
        c.fill_round(rect(k, r.y + 2.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
        self.hits.add(r, hit);
    }

    // ───────────────────────── title bar ─────────────────────────

    fn paint_top(&mut self, c: &mut Canvas, w: f32) {
        let p = self.pal.clone();
        let logo = rect(20.0, (TOP_H - 18.0) / 2.0, 18.0, 18.0);
        let wic = self.wic.clone();
        c.image(7, logo, 1.0, || crate::logo(&wic));
        draw::text(c, "Plume Cut", rect(46.0, 0.0, 120.0, TOP_H), 13.0, 600, p.t1, Align::Left);
        let cap_x = w - 3.0 * m::CAPTION_W;
        let mut x = cap_x - 12.0;
        if self.screen == Screen::Edit {
            let nw = draw::text_width(c, "Plume Cut", 13.0, 600);
            let mut lx = 46.0 + nw + 14.0;
            self.icon_btn(c, &icons::HOME, rect(lx, (TOP_H - 32.0) / 2.0, 32.0, 32.0), Hit::Home, p.t2, true);
            lx += 40.0;
            let name = self.project.name.clone();
            draw::text(c, &name, rect(lx, 0.0, (w * 0.3).min(360.0), TOP_H), 13.0, 400, p.t2, Align::Left);
            // the undo pair, centred
            let cxm = w / 2.0;
            let (u, r) = (self.history.can_undo(), self.history.can_redo());
            self.icon_btn(c, &ic::UNDO, rect(cxm - 38.0, (TOP_H - 32.0) / 2.0, 32.0, 32.0), Hit::Undo, p.t2, u);
            self.icon_btn(c, &ic::REDO, rect(cxm + 6.0, (TOP_H - 32.0) / 2.0, 32.0, 32.0), Hit::Redo, p.t2, r);
            let ew = 112.0;
            x -= ew;
            self.pill_icon(c, rect(x, (TOP_H - 34.0) / 2.0, ew, 34.0), &ic::EXPORT, tr("Export"), true, Hit::Export);
            x -= 8.0;
        }
        x -= 36.0;
        self.icon_btn(c, &icons::GEAR, rect(x, (TOP_H - 36.0) / 2.0, 36.0, 36.0), Hit::Gear, p.t2, true);
        x -= 40.0;
        self.icon_btn(c, &ic::KEYBOARD, rect(x, (TOP_H - 36.0) / 2.0, 36.0, 36.0), Hit::Help, p.t2, true);
    }

    fn paint_caption(&mut self, c: &mut Canvas, w: f32, cx: &PaintCx) {
        let p = self.pal.clone();
        let cap_x = w - 3.0 * m::CAPTION_W;
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, 34.0);
            let hot = self.hovered(&id) || self.pressed.as_ref() == Some(&id);
            if hot {
                c.fill(r, if id == Hit::Close { p.close_hover } else { p.hover });
            }
            let ink = if id == Hit::Close && hot { Color::hex(0xffffff) } else { p.t2 };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    pub fn card(&self, c: &mut Canvas, r: Rect) {
        c.fill_round(r, 16.0, self.pal.card);
        c.stroke_round(r, 16.0, self.pal.line, 1.0);
    }

    // ───────────────────────── start screen ─────────────────────────

    fn paint_start(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        let cw = 640.0f32.min(r.w - 48.0);
        let x = r.x + (r.w - cw) / 2.0;
        let mut y = r.y + (r.h * 0.08).max(24.0);
        let wic = self.wic.clone();
        c.image(8, rect(x, y, 56.0, 56.0), 1.0, || crate::logo(&wic));
        y += 72.0;
        draw::text(c, "Plume Cut", rect(x, y, cw, 34.0), 24.0, 700, p.t1, Align::Left);
        y += 36.0;
        draw::text(c, tr("Cut, arrange and export your videos, on this PC."), rect(x, y, cw, 22.0), 13.0, 400, p.t3, Align::Left);
        y += 40.0;
        self.pill_icon(c, rect(x, y, 170.0, 36.0), &icons::PLUS, tr("New project"), true, Hit::NewProject);
        self.pill_icon(c, rect(x + 182.0, y, 170.0, 36.0), &icons::FOLDER, tr("Open project"), false, Hit::OpenProject);
        y += 60.0;
        if let Some(b) = self.tools_banner(c, x, y, cw) {
            y = b + 16.0;
        }
        draw::text(c, tr("Recent projects"), rect(x, y, cw, 24.0), 17.0, 600, p.t1, Align::Left);
        y += 34.0;
        let recent: Vec<PathBuf> = self.set.recent.iter().filter(|f| f.is_file()).take(8).cloned().collect();
        if recent.is_empty() {
            let box_ = rect(x, y, cw, 150.0);
            self.card(c, box_);
            c.icon(&ic::FILM, rect(x + cw / 2.0 - 12.0, y + 26.0, 24.0, 24.0), p.t3);
            draw::text(c, tr("No projects yet"), rect(x, y + 60.0, cw, 22.0), 15.0, 600, p.t1, Align::Center);
            draw::text(c, tr("Start a new one, or drop videos on this window."), rect(x, y + 86.0, cw, 20.0), 13.0, 400, p.t3, Align::Center);
            return;
        }
        let box_ = rect(x, y, cw, recent.len() as f32 * 52.0 + 16.0);
        self.card(c, box_);
        let mut ry = y + 8.0;
        for (i, f) in recent.iter().enumerate() {
            let row = rect(x + 8.0, ry, cw - 16.0, 48.0);
            let hit = Hit::Recent(i);
            if self.hovered(&hit) {
                c.fill_round(row, 10.0, p.hover);
            }
            c.fill_round(rect(row.x + 10.0, row.y + 8.0, 32.0, 32.0), 9.0, p.accent.fade(0.16));
            c.icon(&ic::FILM, rect(row.x + 18.0, row.y + 16.0, 16.0, 16.0), p.accent);
            let (name, len) = self.recent_info(f);
            draw::text(c, &name, rect(row.x + 54.0, row.y + 5.0, row.w - 230.0, 20.0), 13.0, 600, p.t1, Align::Left);
            let dir = format!("{len} · {}", f.display());
            draw::text(c, &dir, rect(row.x + 54.0, row.y + 24.0, row.w - 230.0, 18.0), 12.0, 400, p.t3, Align::Left);
            if let Ok(t) = std::fs::metadata(f).and_then(|m| m.modified()) {
                draw::text(c, &ago(t), rect(row.right() - 170.0, row.y, 160.0, 48.0), 12.0, 400, p.t3, Align::Right);
            }
            self.hits.add(row, hit);
            ry += 52.0;
        }
    }

    /// FFmpeg missing / downloading / failed. Returns the bottom of the banner.
    fn tools_banner(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32) -> Option<f32> {
        let p = self.pal.clone();
        let (line, button): (String, bool) = match &self.tools {
            Tools::Ready => return None,
            Tools::Missing => (tr("Plume Cut reads and exports videos with FFmpeg (about 100 MB, downloaded once).").to_string(), true),
            Tools::Installing(f) => (fill(tr("Downloading FFmpeg… {}%"), &[&format!("{:.0}", f * 100.0)]), false),
            Tools::Failed(e) => (format!("{}: {e}", tr("FFmpeg could not be installed")), true),
        };
        let h = if button { 104.0 } else { 64.0 };
        let r = rect(x, y, w, h);
        self.card(c, r);
        draw::text_wrap(c, &line, rect(x + 18.0, y + 14.0, w - 36.0, 40.0), 13.0, 400, p.t2, Align::Left);
        if let Tools::Installing(f) = self.tools {
            let bar = rect(x + 18.0, y + 44.0, w - 36.0, 6.0);
            c.fill_round(bar, 3.0, p.fill);
            c.fill_round(rect(bar.x, bar.y, bar.w * f, 6.0), 3.0, p.accent);
        }
        if button {
            self.pill(c, rect(x + 18.0, y + 54.0, 150.0, 34.0), tr("Install FFmpeg"), true, Hit::InstallTools, true);
        }
        Some(y + h)
    }

    // ───────────────────────── media bin ─────────────────────────

    fn paint_bin(&mut self, c: &mut Canvas, r: Rect) {
        self.card(c, r);
        let inner = r.inset(14.0);
        let tabs = [tr("Media"), tr("Text"), tr("Transitions")];
        let cur = match self.bin_tab {
            BinTab::Media => 0,
            BinTab::Text => 1,
            BinTab::Transitions => 2,
        };
        self.segmented(c, rect(inner.x, inner.y, inner.w, 32.0), &tabs, cur, "bin");
        let area = rect(inner.x, inner.y + 44.0, inner.w, inner.h - 44.0);
        c.clip(rect(r.x, area.y - 2.0, r.w, area.h + 16.0));
        let top = area.y - self.bin_scroll;
        let end = match self.bin_tab {
            BinTab::Media => self.paint_media(c, area, top),
            BinTab::Text => self.paint_text_presets(c, area, top),
            BinTab::Transitions => self.paint_trans_presets(c, area, top),
        };
        c.unclip();
        self.bin_h = (end + self.bin_scroll - area.bottom()).max(0.0);
    }

    fn paint_media(&mut self, c: &mut Canvas, area: Rect, top: f32) -> f32 {
        let p = self.pal.clone();
        let mut y = top;
        if let Some(b) = self.tools_banner(c, area.x, y, area.w) {
            y = b + 12.0;
        }
        if self.project.media.is_empty() && self.importing == 0 {
            let cy = y + (area.bottom() - y) / 2.0 - 70.0;
            c.fill_round(rect(area.x + area.w / 2.0 - 22.0, cy, 44.0, 44.0), 14.0, p.accent.fade(0.16));
            c.icon(&ic::IMPORT, rect(area.x + area.w / 2.0 - 10.0, cy + 12.0, 20.0, 20.0), p.accent);
            draw::text(c, tr("No media yet"), rect(area.x, cy + 56.0, area.w, 22.0), 15.0, 600, p.t1, Align::Center);
            draw::text_wrap(
                c,
                tr("Import videos, pictures and music, or drop them here."),
                rect(area.x + 10.0, cy + 82.0, area.w - 20.0, 40.0),
                13.0,
                400,
                p.t3,
                Align::Center,
            );
            self.pill(c, rect(area.x + area.w / 2.0 - 70.0, cy + 128.0, 140.0, 34.0), tr("Import"), true, Hit::Import, true);
            return cy + 170.0;
        }
        self.pill_icon(c, rect(area.x, y, area.w, 34.0), &ic::IMPORT, tr("Import media"), false, Hit::Import);
        y += 46.0;
        if self.importing > 0 {
            draw::text(c, &fill(tr("Reading {} file(s)…"), &[&self.importing.to_string()]), rect(area.x, y, area.w, 20.0), 12.0, 400, p.t3, Align::Left);
            y += 26.0;
        }
        let gap = 10.0;
        let tw = (area.w - gap) / 2.0;
        let th = tw * 9.0 / 16.0;
        let media = self.project.media.clone();
        for (i, m) in media.iter().enumerate() {
            let (col, row) = (i % 2, i / 2);
            let tile = rect(area.x + col as f32 * (tw + gap), y + row as f32 * (th + 32.0), tw, th);
            let hit = Hit::Media(m.id);
            let sel = self.media_sel == Some(m.id);
            c.fill_round(tile, 10.0, p.fill);
            if let Some(f) = self.thumbs.get(&m.id).cloned() {
                let wic = self.wic.clone();
                c.clip(tile);
                c.image(thumb_key(self.generation, m.id, 0), tile, 1.0, || draw::wic_source(&wic, &f));
                c.unclip();
            } else {
                let icon = match m.kind {
                    MediaKind::Audio => &ic::NOTE,
                    MediaKind::Image => &ic::IMAGE,
                    MediaKind::Video => &ic::FILM,
                };
                c.icon(icon, tile.centered(20.0, 20.0), if m.missing { p.red } else { p.t3 });
            }
            if m.kind != MediaKind::Image {
                let d = clock(m.duration);
                let dw = draw::text_width(c, &d, 12.0, 600) + 12.0;
                let badge = rect(tile.right() - dw - 6.0, tile.bottom() - 24.0, dw, 18.0);
                c.fill_round(badge, 9.0, Color::hexa(0x000000, 0.6));
                draw::text(c, &d, badge, 12.0, 600, Color::hex(0xffffff), Align::Center);
            }
            if sel {
                c.stroke_round(tile, 10.0, p.accent, 2.0);
            } else if self.hovered(&hit) || self.hovered(&Hit::MediaAdd(m.id)) {
                c.stroke_round(tile, 10.0, p.ring.mix(p.t3, 0.5), 1.0);
            }
            self.hits.add(tile, hit);
            if self.hovered(&Hit::Media(m.id)) || self.hovered(&Hit::MediaAdd(m.id)) || self.hovered(&Hit::MediaMusic(m.id)) {
                let add = rect(tile.right() - 32.0, tile.y + 6.0, 26.0, 26.0);
                c.fill_round(add, 13.0, p.accent);
                c.icon(&icons::PLUS, add.centered(14.0, 14.0), p.on_accent);
                self.hits.add(add, Hit::MediaAdd(m.id));
                // a sound can also go under everything, as the music
                if m.kind == MediaKind::Audio {
                    let mu = rect(add.x - 32.0, add.y, 26.0, 26.0);
                    c.fill_round(mu, 13.0, p.solid2);
                    c.icon(&ic::NOTE, mu.centered(14.0, 14.0), p.t1);
                    self.hits.add(mu, Hit::MediaMusic(m.id));
                }
            }
            let name = if m.missing { format!("{} ({})", m.name(), tr("missing")) } else { m.name() };
            draw::text(c, &name, rect(tile.x + 2.0, tile.bottom() + 4.0, tile.w - 4.0, 20.0), 12.0, 400, if m.missing { p.red } else { p.t2 }, Align::Left);
        }
        y + media.len().div_ceil(2) as f32 * (th + 32.0)
    }

    fn paint_text_presets(&mut self, c: &mut Canvas, area: Rect, top: f32) -> f32 {
        let p = self.pal.clone();
        let mut y = top;
        draw::text_wrap(c, tr("Click to add a text at the playhead."), rect(area.x, y, area.w, 36.0), 12.0, 400, p.t3, Align::Left);
        y += 28.0;
        for (i, (name, st)) in text_presets().into_iter().enumerate() {
            let r = rect(area.x, y, area.w, 72.0);
            let hit = Hit::TextPreset(i);
            c.fill_round(r, 12.0, if self.hovered(&hit) { p.fill2 } else { p.fill });
            let sample = rect(r.x + 10.0, r.y + 8.0, r.w - 20.0, 38.0);
            c.fill_round(sample, 8.0, Color::hex(0x101012));
            let size = (st.size as f32 * 140.0).clamp(12.0, 24.0);
            let (tw, th) = draw::measure(c, &st.text, &st.font, size, if st.bold { 700 } else { 400 });
            if st.bg_opacity > 0.0 {
                let b = rect(sample.x + (sample.w - tw) / 2.0 - 6.0, sample.y + (sample.h - th) / 2.0 - 2.0, tw + 12.0, th + 4.0);
                c.fill(b, super::swatch_color(&st.bg).fade(st.bg_opacity as f32));
            }
            draw::video_text(
                c,
                &st.text,
                rect(sample.x, sample.y + (sample.h - th) / 2.0, sample.w, th),
                &st.font,
                size,
                if st.bold { 700 } else { 400 },
                super::swatch_color(&st.color),
            );
            draw::text(c, name, rect(r.x + 12.0, r.y + 48.0, r.w - 24.0, 20.0), 12.0, 600, p.t2, Align::Left);
            self.hits.add(r, hit);
            y += 82.0;
        }
        y
    }

    fn paint_trans_presets(&mut self, c: &mut Canvas, area: Rect, top: f32) -> f32 {
        let p = self.pal.clone();
        let mut y = top;
        draw::text_wrap(c, tr("Select a clip that follows another, then pick how it begins."), rect(area.x, y, area.w, 40.0), 12.0, 400, p.t3, Align::Left);
        y += 44.0;
        let cur = self.sel().map(|c| c.trans.map(|t| t.kind));
        let items = [(tr("Cut"), None), (tr("Crossfade"), Some(model::TransKind::Crossfade)), (tr("Dip to black"), Some(model::TransKind::Dip))];
        for (i, (name, kind)) in items.into_iter().enumerate() {
            let r = rect(area.x, y, area.w, 64.0);
            let hit = Hit::TransPreset(i);
            let on = cur == Some(kind);
            c.fill_round(
                r,
                12.0,
                if on {
                    p.accent.fade(0.16)
                } else if self.hovered(&hit) {
                    p.fill2
                } else {
                    p.fill
                },
            );
            if on {
                c.stroke_round(r, 12.0, p.accent.fade(0.5), 1.0);
            }
            // a little picture of it: two frames meeting
            let a = rect(r.x + 12.0, r.y + 14.0, 30.0, 36.0);
            let b = rect(r.x + 36.0, r.y + 14.0, 30.0, 36.0);
            c.fill_round(a, 6.0, p.t3.fade(0.6));
            match kind {
                None => c.fill_round(b, 6.0, p.accent.fade(0.9)),
                Some(model::TransKind::Crossfade) => c.fill_round(b, 6.0, p.accent.fade(0.55)),
                Some(model::TransKind::Dip) => {
                    c.fill_round(rect(r.x + 30.0, r.y + 14.0, 18.0, 36.0), 4.0, Color::hex(0x000000));
                    c.fill_round(rect(r.x + 48.0, r.y + 14.0, 18.0, 36.0), 6.0, p.accent.fade(0.9));
                }
            }
            draw::text(c, name, rect(r.x + 80.0, r.y, r.w - 90.0, r.h), 13.0, 600, if on { p.accent } else { p.t1 }, Align::Left);
            self.hits.add(r, hit);
            y += 74.0;
        }
        y
    }

    // ───────────────────────── preview ─────────────────────────

    fn paint_preview(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        self.card(c, r);
        let mut inner = r.inset(14.0);
        let bar = inner.cut_bottom(44.0);
        inner.h -= 8.0;
        // the frame, letterboxed
        let ratio = self.project.ratio() as f32;
        let (fw, fh) = if inner.w / inner.h > ratio { (inner.h * ratio, inner.h) } else { (inner.w, inner.w / ratio) };
        let fr = rect((inner.x + (inner.w - fw) / 2.0).round(), (inner.y + (inner.h - fh) / 2.0).round(), fw.round(), fh.round());
        self.frame_rect = fr;
        self.hits.add(inner, Hit::PreviewArea);
        c.fill(fr, Color::hex(0x000000));
        let scale = self.win.scale();
        self.max_px = ((fr.w.max(fr.h) * scale / 64.0).ceil() as u32 * 64).clamp(256, 960);
        c.clip(fr);
        let played = self.player.as_ref().and_then(|pl| pl.current.clone());
        match played {
            Some(f) => {
                if let Some(b) = draw::bitmap(c, &f) {
                    draw::draw_bitmap(c, &b, fr, None, 1.0, 0.0);
                }
            }
            None => self.compose(c, fr),
        }
        c.unclip();
        if self.project.duration() == 0 && self.player.is_none() {
            draw::text(c, tr("Add clips to the timeline to see them here"), fr, 13.0, 400, Color::hexa(0xffffff, 0.55), Align::Center);
        }
        if !self.playing() {
            // the handles may reach past the frame, never past the preview's card
            let card = r.inset(1.0);
            c.clip(card);
            self.paint_handles(c, fr, card);
            c.unclip();
        }
        // controls
        let fps = self.project.fps;
        let tc = format!("{}  /  {}", timecode(self.playhead, fps), timecode(self.project.duration(), fps));
        draw::text(c, &tc, rect(bar.x + 4.0, bar.y, bar.w / 2.0 - 70.0, bar.h), 12.0, 600, p.t2, Align::Left);
        let cxm = bar.x + bar.w / 2.0;
        self.icon_btn(c, &ic::STEP_BACK, rect(cxm - 64.0, bar.y + 6.0, 32.0, 32.0), Hit::StepBack, p.t2, true);
        let play = rect(cxm - 20.0, bar.y + 2.0, 40.0, 40.0);
        let hot = self.hovered(&Hit::Play);
        c.fill_round(play, 20.0, if hot { p.accent.mix(Color::hex(0xffffff), 0.12) } else { p.accent });
        c.icon_fill(if self.playing() { &ic::PAUSE } else { &ic::PLAY }, play.centered(16.0, 16.0), p.on_accent);
        self.hits.add(play, Hit::Play);
        self.icon_btn(c, &ic::STEP_FWD, rect(cxm + 32.0, bar.y + 6.0, 32.0, 32.0), Hit::StepFwd, p.t2, true);
        let label = format!("{}:{}", self.project.aspect[0], self.project.aspect[1]);
        let aw = 76.0;
        let ar = rect(bar.right() - aw, bar.y + 6.0, aw, 32.0);
        self.pill(c, ar, &label, false, Hit::Aspect, true);
    }

    /// The frame at the playhead drawn from each clip's own picture (exact frames come back from FFmpeg; meanwhile
    /// the last one of each media stands in).
    fn compose(&mut self, c: &mut Canvas, fr: Rect) {
        let layers = self.project.layers(self.playhead);
        let fps = self.project.fps();
        let mut wanted: Vec<(Key, PathBuf)> = Vec::new();
        for l in layers {
            let Some(clip) = self.project.clip(l.clip).cloned() else { continue };
            let track_kind = self.project.tracks[l.track].kind;
            if let Some(st) = &clip.text {
                self.draw_text_layer(c, fr, &clip, st, l.alpha * (1.0 - l.black));
                continue;
            }
            let Some(m) = self.project.media_of(&clip).cloned() else { continue };
            if m.missing {
                continue;
            }
            let (dw, dh) = fit_box(m.width, m.height, self.max_px);
            let ms = if m.kind == MediaKind::Image { 0 } else { ((l.src * fps).floor() / fps * 1000.0).round() as i64 };
            let key = Key { media: m.id, ms, w: dw, h: dh };
            let frame = match self.cache.get(&key) {
                Some(f) => Some(f),
                None => {
                    wanted.push((key, m.path.clone()));
                    self.cache.last.get(&m.id).cloned().filter(|f| f.w == dw && f.h == dh)
                }
            };
            let Some(frame) = frame else { continue };
            let frame = if clip.has_color() { std::sync::Arc::new(preview::adjust(&frame, clip.brightness, clip.contrast, clip.saturation)) } else { frame };
            let pl = render::place(&clip, m.width as f64, m.height as f64, fr.w as f64, fr.h as f64);
            let [cl, ct, cr, cb] = render::crop_of(&clip);
            let dst = rect(fr.x + (pl.cx - pl.w / 2.0) as f32, fr.y + (pl.cy - pl.h / 2.0) as f32, pl.w as f32, pl.h as f32);
            let src = rect(cl as f32 * dw as f32, ct as f32 * dh as f32, (1.0 - cl - cr) as f32 * dw as f32, (1.0 - ct - cb) as f32 * dh as f32);
            let mut alpha = (clip.opacity * l.alpha) as f32;
            if track_kind != TrackKind::Video {
                alpha *= 1.0 - l.black as f32;
            }
            if let Some(b) = draw::bitmap(c, &frame) {
                draw::draw_bitmap(c, &b, dst, Some(src), alpha, clip.rotation as f32);
            }
            if track_kind == TrackKind::Video && l.black > 0.0 {
                c.fill(fr, Color::hexa(0x000000, l.black as f32));
            }
        }
        self.request_frames(wanted);
    }

    /// Where a text clip sits in the frame: its box (padding included) and its text's size.
    pub fn text_box(&mut self, c: &mut Canvas, fr: Rect, clip: &model::Clip, st: &model::TextStyle) -> (Rect, f32, f32, f32) {
        let fps = self.project.fps();
        let t = self.playhead as f64 / fps;
        let (dx, dy, alpha) = render::text_motion(st, t, clip.start as f64 / fps, clip.end() as f64 / fps);
        let size = (st.size as f32 * fr.h).max(2.0);
        let (tw, th) = draw::measure(c, &st.text, &st.font, size, if st.bold { 700 } else { 400 });
        let cxp = fr.x + fr.w / 2.0 + ((clip.x + dx) as f32) * fr.w;
        let cyp = fr.y + fr.h / 2.0 + ((clip.y + dy) as f32) * fr.h;
        let pad = (size * 0.3).round();
        (rect(cxp - tw / 2.0 - pad, cyp - th / 2.0 - pad, tw + 2.0 * pad, th + 2.0 * pad), tw, th, alpha as f32)
    }

    fn draw_text_layer(&mut self, c: &mut Canvas, fr: Rect, clip: &model::Clip, st: &model::TextStyle, k: f64) {
        if st.text.trim().is_empty() {
            return;
        }
        let (b, tw, th, alpha) = self.text_box(c, fr, clip, st);
        let a = alpha * k as f32;
        if a <= 0.0 {
            return;
        }
        let size = (st.size as f32 * fr.h).max(2.0);
        if st.bg_opacity > 0.01 {
            c.fill(b, super::swatch_color(&st.bg).fade(st.bg_opacity as f32 * a));
        }
        let tq = rect(b.x + (b.w - tw) / 2.0, b.y + (b.h - th) / 2.0, tw, th);
        draw::video_text(c, &st.text, tq, &st.font, size, if st.bold { 700 } else { 400 }, super::swatch_color(&st.color).fade(a));
    }

    /// Box, corners and the turning knob of the selected picture or text.
    fn paint_handles(&mut self, c: &mut Canvas, fr: Rect, card: Rect) {
        let Some(clip) = self.sel().cloned() else { return };
        let Some((t, _)) = self.project.find(clip.id) else { return };
        if !self.project.tracks[t].kind.visual() || self.playhead < clip.start || self.playhead >= clip.end() {
            return;
        }
        let p = self.pal.clone();
        let (q, deg) = match &clip.text {
            Some(st) => (self.text_box(c, fr, &clip, st).0, 0.0),
            None => {
                let Some(m) = self.project.media_of(&clip) else { return };
                let pl = render::place(&clip, m.width as f64, m.height as f64, fr.w as f64, fr.h as f64);
                (rect(fr.x + (pl.cx - pl.w / 2.0) as f32, fr.y + (pl.cy - pl.h / 2.0) as f32, pl.w as f32, pl.h as f32), clip.rotation as f32)
            }
        };
        draw::outline_turned(c, q, deg, p.accent, 1.5);
        let (cx0, cy0) = (q.x + q.w / 2.0, q.y + q.h / 2.0);
        // the body: its box without the turn (good enough to grab)
        self.hits.add(q.intersect(&fr.inset(-40.0)).intersect(&card), Hit::Handle(Handle::Body));
        if clip.text.is_none() {
            let corners = [(q.x, q.y), (q.right(), q.y), (q.right(), q.bottom()), (q.x, q.bottom())];
            for (i, (x, y)) in corners.into_iter().enumerate() {
                let (x, y) = draw::turn(x, y, cx0, cy0, deg);
                let h = rect(x - 6.0, y - 6.0, 12.0, 12.0);
                c.fill_round(h, 6.0, Color::hex(0xffffff));
                c.stroke_round(h, 6.0, p.accent, 1.5);
                if card.contains(plume_ui::pt(x, y)) {
                    self.hits.add(h.inset(-4.0), Hit::Handle(Handle::Corner(i as u8)));
                }
            }
            // the turning knob above the picture, or just inside it when there is no room above
            let (mut kx, mut ky) = draw::turn(cx0, q.y - 26.0, cx0, cy0, deg);
            if !card.inset(8.0).contains(plume_ui::pt(kx, ky)) {
                (kx, ky) = draw::turn(cx0, q.y + 26.0, cx0, cy0, deg);
            }
            let (tx, ty) = draw::turn(cx0, q.y, cx0, cy0, deg);
            draw::line(c, tx, ty, kx, ky, p.accent, 1.5);
            let k = rect(kx - 7.0, ky - 7.0, 14.0, 14.0);
            c.fill_round(k, 7.0, p.accent);
            if card.contains(plume_ui::pt(kx, ky)) {
                self.hits.add(k.inset(-4.0), Hit::Handle(Handle::Rotate));
            }
        }
    }

    // ───────────────────────── dialogs ─────────────────────────

    fn paint_dialog(&mut self, c: &mut Canvas, d: Dialog, w: f32, h: f32) {
        let p = self.pal.clone();
        c.fill(rect(0.0, 0.0, w, h), Color::hexa(0x000000, if p.dark { 0.5 } else { 0.3 }));
        self.hits.add(rect(0.0, 0.0, w, h), Hit::DialogBg);
        let (cw, ch) = match d {
            Dialog::Export => (560.0, 470.0),
            Dialog::Help => (680.0, 520.0),
            Dialog::Settings => (520.0, 420.0),
        };
        let r = rect(((w - cw) / 2.0).round(), ((h - ch) / 2.0).round(), cw, ch);
        c.fill_round(r, 20.0, p.solid);
        c.stroke_round(r, 20.0, p.ring, 1.0);
        self.hits.add(r, Hit::DialogCard);
        let inner = r.inset(24.0);
        match d {
            Dialog::Export => self.paint_export(c, inner),
            Dialog::Help => self.paint_help(c, inner),
            Dialog::Settings => self.paint_settings(c, inner),
        }
    }

    fn row_label(&self, c: &mut Canvas, y: f32, x: f32, w: f32, label: &str) {
        draw::text(c, label, rect(x, y, w, 18.0), 12.0, 600, self.pal.t2, Align::Left);
    }

    fn paint_export(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        draw::text(c, tr("Export"), rect(r.x, r.y, r.w, 26.0), 17.0, 600, p.t1, Align::Left);
        let mut y = r.y + 40.0;
        match self.export.clone() {
            ExportState::Running { frac, note, started } => {
                draw::text(c, tr("Exporting…"), rect(r.x, y, r.w, 22.0), 15.0, 600, p.t1, Align::Left);
                y += 34.0;
                let bar = rect(r.x, y, r.w, 8.0);
                c.fill_round(bar, 4.0, p.fill);
                c.fill_round(rect(bar.x, bar.y, (bar.w * frac as f32).max(8.0), 8.0), 4.0, p.accent);
                y += 20.0;
                let el = started.elapsed().as_secs_f64();
                let left =
                    if frac > 0.02 && el > 1.0 { format!(" · {}", fill(tr("about {} left"), &[&clock(el * (1.0 - frac) / frac)])) } else { String::new() };
                draw::text(c, &format!("{:.0} %{left}", frac * 100.0), rect(r.x, y, r.w, 20.0), 13.0, 400, p.t2, Align::Left);
                y += 24.0;
                if !note.is_empty() {
                    draw::text(c, crate::i18n::tr_dyn(&note), rect(r.x, y, r.w, 20.0), 12.0, 400, p.t3, Align::Left);
                }
                self.pill(c, rect(r.right() - 120.0, r.bottom() - 36.0, 120.0, 36.0), tr("Cancel"), false, Hit::Action("export_cancel"), true);
                return;
            }
            ExportState::Done { path, size } => {
                c.fill_round(rect(r.x, y, 44.0, 44.0), 14.0, p.green.fade(0.18));
                c.icon(&icons::CHECK, rect(r.x + 12.0, y + 12.0, 20.0, 20.0), p.green);
                draw::text(c, tr("Your video is ready"), rect(r.x + 58.0, y, r.w - 58.0, 24.0), 15.0, 600, p.t1, Align::Left);
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                draw::text(c, &format!("{name} · {}", export::size_text(size)), rect(r.x + 58.0, y + 22.0, r.w - 58.0, 22.0), 13.0, 400, p.t3, Align::Left);
                self.pill(c, rect(r.right() - 120.0, r.bottom() - 36.0, 120.0, 36.0), tr("Done"), true, Hit::Action("export_close"), true);
                self.pill(c, rect(r.right() - 288.0, r.bottom() - 36.0, 156.0, 36.0), tr("Show in folder"), false, Hit::Action("export_show"), true);
                return;
            }
            ExportState::Failed(e) => {
                draw::text_wrap(c, &format!("{}: {e}", tr("The export failed")), rect(r.x, y, r.w, 60.0), 13.0, 400, p.red, Align::Left);
                y += 64.0;
            }
            ExportState::Idle => {}
        }
        self.row_label(c, y, r.x, r.w, tr("Resolution"));
        y += 24.0;
        let res = [720, 1080, 2160];
        let cur = res.iter().position(|v| *v == self.set.export_res).unwrap_or(1);
        self.segmented(c, rect(r.x, y, r.w, 34.0), &["720p", "1080p", "4K"], cur, "res");
        y += 50.0;
        self.row_label(c, y, r.x, r.w, tr("Format"));
        y += 24.0;
        let proj = format!("{} ({}:{})", tr("Project"), self.project.aspect[0], self.project.aspect[1]);
        let labels = [proj.as_str(), tr("16:9 YouTube"), tr("9:16 TikTok, Reels"), tr("1:1 Square")];
        let cur = export::ASPECTS.iter().position(|(k, _)| *k == self.set.export_aspect).unwrap_or(0);
        self.segmented(c, rect(r.x, y, r.w, 34.0), &labels, cur, "aspect");
        y += 50.0;
        self.row_label(c, y, r.x, r.w, tr("Video codec"));
        y += 24.0;
        let cur = if self.set.export_codec == "hevc" { 1 } else { 0 };
        self.segmented(c, rect(r.x, y, 260.0, 34.0), &[tr("H.264 (everywhere)"), "H.265"], cur, "codec");
        y += 50.0;
        draw::text(c, tr("Use the graphics card"), rect(r.x, y, r.w - 60.0, 24.0), 13.0, 400, p.t1, Align::Left);
        self.switch(c, r.right() - 40.0, y, self.set.hw, Hit::Toggle("hw"));
        y += 26.0;
        if let Some(l) = self.hw_label.clone() {
            draw::text(c, &l, rect(r.x, y, r.w, 18.0), 12.0, 400, p.t3, Align::Left);
        }
        y += 30.0;
        let spec = self.spec();
        let (ow, oh) = render::out_size(spec.aspect.unwrap_or(self.project.aspect), spec.short);
        let info = format!("{ow} × {oh} · {} fps · {} · {}", self.project.fps, clock(self.project.duration() as f64 / self.project.fps()), spec.dir.display());
        draw::text(c, &info, rect(r.x, y, r.w - 110.0, 20.0), 12.0, 400, p.t3, Align::Left);
        self.pill(c, rect(r.right() - 100.0, y - 8.0, 100.0, 32.0), tr("Change"), false, Hit::Action("out_dir"), true);
        self.pill(c, rect(r.right() - 120.0, r.bottom() - 36.0, 120.0, 36.0), tr("Export"), true, Hit::Action("export_go"), true);
        self.pill(c, rect(r.right() - 252.0, r.bottom() - 36.0, 120.0, 36.0), tr("Cancel"), false, Hit::Action("dialog_close"), true);
    }

    fn paint_help(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        draw::text(c, tr("Keyboard shortcuts"), rect(r.x, r.y, r.w, 26.0), 17.0, 600, p.t1, Align::Left);
        let keys: [(&str, &str); 22] = [
            ("Space", tr("Play / pause")),
            ("J  K  L", tr("Backwards, stop, forwards (again: faster)")),
            ("←  →", tr("One frame (Shift: one second)")),
            ("↑  ↓", tr("Previous / next cut")),
            ("Home  End", tr("Start / end")),
            ("S", tr("Split at the playhead")),
            ("Delete", tr("Delete the selected clips")),
            ("Shift+Delete", tr("Delete and close the gap")),
            ("Ctrl+click", tr("Select several clips")),
            ("Ctrl+A", tr("Select everything")),
            ("Ctrl+Z", tr("Undo")),
            ("Ctrl+Y", tr("Redo")),
            ("T", tr("Add a text")),
            ("N", tr("Snapping on or off")),
            ("+  −", tr("Zoom the timeline")),
            ("Z", tr("Show the whole timeline")),
            ("Ctrl+wheel", tr("Zoom around the mouse")),
            ("Ctrl+I", tr("Import media")),
            ("Ctrl+E", tr("Export")),
            ("Ctrl+S", tr("Save now (saving is automatic)")),
            ("Ctrl+N  Ctrl+O", tr("New project / open a project")),
            ("Esc", tr("Clear the selection, close a dialog")),
        ];
        let col_w = (r.w - 24.0) / 2.0;
        let per = keys.len().div_ceil(2);
        for (i, (k, what)) in keys.iter().enumerate() {
            let (col, row) = (i / per, i % per);
            let x = r.x + col as f32 * (col_w + 24.0);
            let y = r.y + 44.0 + row as f32 * 36.0;
            let kw = draw::text_width(c, k, 12.0, 600) + 16.0;
            let kr = rect(x, y + 4.0, kw, 24.0);
            c.fill_round(kr, 8.0, p.fill);
            draw::text(c, k, kr, 12.0, 600, p.t1, Align::Center);
            draw::text(c, what, rect(x + kw + 10.0, y, col_w - kw - 10.0, 32.0), 12.0, 400, p.t2, Align::Left);
        }
        self.pill(c, rect(r.right() - 110.0, r.bottom() - 36.0, 110.0, 36.0), tr("Close"), true, Hit::Action("dialog_close"), true);
    }

    fn paint_settings(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        draw::text(c, tr("Settings"), rect(r.x, r.y, r.w, 26.0), 17.0, 600, p.t1, Align::Left);
        let mut y = r.y + 44.0;
        self.row_label(c, y, r.x, r.w, tr("Appearance"));
        y += 24.0;
        let cur = match self.set.theme.as_str() {
            "dark" => 1,
            "light" => 2,
            _ => 0,
        };
        self.segmented(c, rect(r.x, y, r.w, 34.0), &[tr("Like Windows"), tr("Dark"), tr("Light")], cur, "theme");
        y += 50.0;
        self.row_label(c, y, r.x, r.w, tr("Language"));
        y += 24.0;
        let cur = match self.set.lang.as_str() {
            "en" => 1,
            "fr" => 2,
            _ => 0,
        };
        self.segmented(c, rect(r.x, y, r.w, 34.0), &[tr("Like Windows"), "English", "Français"], cur, "lang");
        y += 54.0;
        draw::text(c, tr("Export with the graphics card"), rect(r.x, y, r.w - 60.0, 24.0), 13.0, 400, p.t1, Align::Left);
        self.switch(c, r.right() - 40.0, y, self.set.hw, Hit::Toggle("hw"));
        y += 40.0;
        let ff = match &self.tools {
            Tools::Ready => format!("FFmpeg: {}", crate::tools::tools_dir().display()),
            Tools::Missing => tr("FFmpeg is not installed yet").to_string(),
            Tools::Installing(f) => fill(tr("Downloading FFmpeg… {}%"), &[&format!("{:.0}", f * 100.0)]),
            Tools::Failed(e) => e.clone(),
        };
        draw::text(c, &ff, rect(r.x, y, r.w - 170.0, 20.0), 12.0, 400, p.t3, Align::Left);
        if matches!(self.tools, Tools::Missing | Tools::Failed(_)) {
            self.pill(c, rect(r.right() - 150.0, y - 8.0, 150.0, 32.0), tr("Install FFmpeg"), false, Hit::InstallTools, true);
        }
        y += 30.0;
        draw::text(c, &format!("{}: {}", tr("Exports go to"), self.set.out_dir().display()), rect(r.x, y, r.w - 110.0, 20.0), 12.0, 400, p.t3, Align::Left);
        self.pill(c, rect(r.right() - 100.0, y - 8.0, 100.0, 32.0), tr("Change"), false, Hit::Action("out_dir"), true);
        self.pill(c, rect(r.right() - 110.0, r.bottom() - 36.0, 110.0, 36.0), tr("Close"), true, Hit::Action("dialog_close"), true);
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((text, _)) = self.toast.clone() else { return };
        let p = self.pal.clone();
        let tw = (draw::text_width(c, &text, 13.0, 600) + 40.0).min(w - 40.0);
        let r = rect((w - tw) / 2.0, h - 72.0, tw, 40.0);
        c.fill_round(r, 20.0, p.solid2);
        c.stroke_round(r, 20.0, p.ring, 1.0);
        draw::text(c, &text, r.inset_xy(16.0, 0.0), 13.0, 600, p.t1, Align::Center);
        self.hits.add(r, Hit::Toast);
    }

    /// The window's single text field, over the field being typed in.
    fn place_edit(&mut self, fields: &[(&'static str, Rect)]) {
        let p = self.pal.clone();
        let want = self.edit_for.and_then(|k| fields.iter().find(|(f, _)| *f == k).map(|(_, r)| (k, *r)));
        match want {
            Some((k, r)) => {
                let multi = k == "text";
                let inner = if multi { r.inset_xy(10.0, 8.0) } else { rect(r.x + 10.0, r.y + 7.0, r.w - 20.0, r.h - 14.0) };
                if self.edit_shown != Some(k) {
                    let text = match k {
                        "text" => self.sel().and_then(|c| c.text.as_ref()).map(|t| t.text.clone()).unwrap_or_default(),
                        _ => self.project.name.clone(),
                    };
                    let style = EditStyle {
                        fg: p.t1,
                        bg: p.fill2.over(p.card.over(p.bg)),
                        size: 13.0,
                        select: Some((0, text.encode_utf16().count())),
                        placeholder: String::new(),
                        password: false,
                        multiline: multi,
                    };
                    self.win.edit_show(inner, &text, &style);
                    self.win.edit_focus();
                    self.edit_shown = Some(k);
                } else {
                    self.win.edit_place(inner);
                }
            }
            None => {
                if self.edit_shown.is_some() {
                    self.win.edit_hide();
                    self.edit_shown = None;
                }
                self.edit_for = None;
                self.typing = None;
            }
        }
    }
}

/// Key of a small picture kept by the canvas: media, index.
pub fn thumb_key(generation: u64, media: u64, i: usize) -> u64 {
    0x1000_0000_0000_0000 | ((generation & 0xfff) << 48) | ((media & 0xffff_ffff) << 16) | (i as u64 & 0xffff)
}

fn ago(t: std::time::SystemTime) -> String {
    let s = t.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    if s < 60 {
        tr("just now").to_string()
    } else if s < 3600 {
        fill(tr("{} min ago"), &[&(s / 60).to_string()])
    } else if s < 86_400 {
        fill(tr("{} h ago"), &[&(s / 3600).to_string()])
    } else {
        fill(tr("{} days ago"), &[&(s / 86_400).to_string()])
    }
}
