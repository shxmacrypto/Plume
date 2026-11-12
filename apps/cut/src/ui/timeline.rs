//! The timeline: its tool row, the ruler, the tracks and their clips (filmstrips, sound outlines, texts,
//! transitions), the playhead; and everything the mouse does in the window after a press — scrub, move and trim
//! clips (with snapping), drop media from the bin, drag sliders and the handles of the preview.

use super::{App, Dialog, Drag, ExportState, Handle, Hit, Tab, draw, draw::ic, panels::thumb_key, props, text_presets, track_label};
use crate::{
    i18n::tr,
    model::{self, Id, MediaKind, TrackKind, snap},
};
use plume_ui::{Align, Canvas, Color, Cx, Mods, Pt, Rect, icons, rect};

pub const HEAD_W: f32 = 140.0;
const TOOL_H: f32 = 44.0;
const RULER_H: f32 = 26.0;
const ZOOM_MIN: f32 = 0.02;
const ZOOM_MAX: f32 = 40.0;

pub fn row_h(k: TrackKind) -> f32 {
    match k {
        TrackKind::Video => 60.0,
        TrackKind::Overlay => 44.0,
        TrackKind::Text => 34.0,
        TrackKind::Audio | TrackKind::Music => 42.0,
    }
}

impl App {
    pub fn kind_color(&self, k: TrackKind) -> Color {
        let p = &self.pal;
        match k {
            TrackKind::Video => p.accent,
            TrackKind::Overlay => p.accent.mix(p.orange, 0.5),
            TrackKind::Text => p.orange,
            TrackKind::Audio => p.green,
            TrackKind::Music => p.green.mix(p.accent, 0.35),
        }
    }

    pub fn frame_at(&self, x: f32) -> i64 {
        ((x - self.tl_x0 + self.tl_scroll) / self.zoom).round().max(0.0) as i64
    }

    pub fn x_of(&self, f: i64) -> f32 {
        self.tl_x0 + f as f32 * self.zoom - self.tl_scroll
    }

    /// The row (index in `tl_rows`) under a height; above the first: the first, below the last: the last.
    fn row_at(&self, y: f32) -> Option<usize> {
        if self.tl_rows.is_empty() {
            return None;
        }
        let i = self.tl_rows.iter().position(|(_, ry, rh)| y >= *ry && y < ry + rh);
        Some(i.unwrap_or(if y < self.tl_rows[0].1 { 0 } else { self.tl_rows.len() - 1 }))
    }

    pub fn paint_timeline(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        self.tl_rect = r;
        self.card(c, r);
        let inner = r.inset_xy(12.0, 8.0);
        // ── tools ──
        let ty = inner.y + 4.0;
        let mut x = inner.x;
        let has_sel = !self.selected.is_empty();
        for (icon, tool, enabled) in
            [(&ic::SCISSORS, "split", self.project.duration() > 0), (&icons::TRASH, "delete", has_sel), (&ic::RIPPLE, "ripple", has_sel)]
        {
            self.icon_btn(c, icon, rect(x, ty, 34.0, 34.0), Hit::Tool(tool), p.t2, enabled);
            x += 38.0;
        }
        c.vline(x + 4.0, ty + 8.0, 18.0, p.line);
        x += 12.0;
        self.icon_btn(c, &ic::TEXT, rect(x, ty, 34.0, 34.0), Hit::Tool("text"), p.t2, true);
        x += 38.0;
        let snap_on = self.set.snap;
        if snap_on {
            c.fill_round(rect(x, ty, 34.0, 34.0), 17.0, p.accent.fade(0.16));
        }
        self.icon_btn(c, &ic::MAGNET, rect(x, ty, 34.0, 34.0), Hit::Tool("snap"), if snap_on { p.accent } else { p.t2 }, true);
        // zoom on the right
        let mut zx = inner.right() - 34.0;
        self.icon_btn(c, &ic::FIT, rect(zx, ty, 34.0, 34.0), Hit::Tool("fit"), p.t2, true);
        zx -= 38.0;
        self.icon_btn(c, &ic::ZOOM_IN, rect(zx, ty, 34.0, 34.0), Hit::Tool("zoom_in"), p.t2, true);
        let bar = rect(zx - 128.0, ty + 15.0, 120.0, 4.0);
        c.fill_round(bar, 2.0, p.fill2);
        let k = ((self.zoom / ZOOM_MIN).ln() / (ZOOM_MAX / ZOOM_MIN).ln()).clamp(0.0, 1.0);
        c.fill_round(rect(bar.x, bar.y, bar.w * k, 4.0), 2.0, p.accent);
        let knob = rect(bar.x + bar.w * k - 7.0, bar.y - 5.0, 14.0, 14.0);
        c.fill_round(knob, 7.0, Color::hex(0xffffff));
        c.stroke_round(knob, 7.0, Color::hexa(0x000000, 0.25), 1.0);
        self.hits.add(rect(bar.x - 6.0, ty, bar.w + 12.0, 34.0), Hit::ZoomBar);
        self.icon_btn(c, &ic::ZOOM_OUT, rect(bar.x - 42.0, ty, 34.0, 34.0), Hit::Tool("zoom_out"), p.t2, true);
        // ── ruler and tracks ──
        let area = rect(inner.x, inner.y + TOOL_H, inner.w, inner.h - TOOL_H);
        let lanes_x = area.x + HEAD_W;
        let lanes_w = area.w - HEAD_W;
        self.tl_x0 = lanes_x + 4.0;
        let ruler = rect(lanes_x, area.y, lanes_w, RULER_H);
        let rows_area = rect(area.x, area.y + RULER_H + 4.0, area.w, area.h - RULER_H - 4.0);
        let total_h: f32 = self.project.tracks.iter().map(|t| row_h(t.kind) + 6.0).sum();
        self.tl_vscroll = self.tl_vscroll.clamp(0.0, (total_h - rows_area.h).max(0.0));
        self.paint_ruler(c, ruler);
        self.hits.add(rows_area, Hit::TrackArea);
        c.clip(rows_area);
        let mut y = rows_area.y - self.tl_vscroll;
        self.tl_rows.clear();
        let fps = self.project.fps();
        for t in 0..self.project.tracks.len() {
            let tr = self.project.tracks[t].clone();
            let h = row_h(tr.kind);
            self.tl_rows.push((t, y, h));
            // header
            let head = rect(area.x, y, HEAD_W - 8.0, h);
            c.fill_round(head, 10.0, p.fill.fade(0.6));
            let kc = self.kind_color(tr.kind);
            c.fill_round(rect(head.x + 8.0, head.y + (h - 20.0) / 2.0, 4.0, 20.0), 2.0, kc);
            draw::text(c, track_label(tr.kind), rect(head.x + 18.0, head.y, head.w - 80.0, h), 12.0, 600, p.t2, Align::Left);
            let mut bx = head.right() - 30.0;
            if tr.kind != TrackKind::Text {
                let icon = if tr.muted { &ic::MUTE } else { &ic::SPEAKER };
                self.icon_btn(c, icon, rect(bx, head.y + (h - 26.0) / 2.0, 26.0, 26.0), Hit::TrackMute(t), if tr.muted { p.red } else { p.t3 }, true);
                bx -= 28.0;
            }
            if tr.kind.visual() {
                let icon = if tr.hidden { &ic::EYE_OFF } else { &ic::EYE };
                self.icon_btn(c, icon, rect(bx, head.y + (h - 26.0) / 2.0, 26.0, 26.0), Hit::TrackHide(t), if tr.hidden { p.red } else { p.t3 }, true);
            }
            // lane
            let lane = rect(lanes_x, y, lanes_w, h);
            c.fill_round(lane, 10.0, p.fill.fade(0.35));
            c.clip(lane);
            for (i, clip) in tr.clips.iter().enumerate() {
                let x0 = self.x_of(clip.start);
                let x1 = self.x_of(clip.end());
                if x1 < lane.x || x0 > lane.right() {
                    continue;
                }
                let cr = rect(x0 + 1.0, y + 3.0, (x1 - x0 - 2.0).max(3.0), h - 6.0);
                self.paint_clip(c, cr, clip, tr.kind, tr.muted || tr.hidden);
                self.hits.add(cr, Hit::Clip(clip.id));
                if cr.w > 18.0 {
                    self.hits.add(rect(cr.x - 2.0, cr.y, 8.0, cr.h), Hit::TrimL(clip.id));
                    self.hits.add(rect(cr.right() - 6.0, cr.y, 8.0, cr.h), Hit::TrimR(clip.id));
                }
                if let Some(tx) = clip.trans.filter(|_| i > 0) {
                    let mark = rect(x0 - 9.0, y + h / 2.0 - 9.0, 18.0, 18.0);
                    c.fill_round(mark, 6.0, p.solid2);
                    c.stroke_round(mark, 6.0, kc, 1.5);
                    c.icon(&ic::TRANSITION, mark.centered(12.0, 12.0), p.t1);
                    self.hits.add(mark, Hit::TransMark(clip.id));
                    let _ = tx.kind;
                }
            }
            c.unclip();
            y += h + 6.0;
        }
        // the bin's media on its way
        if let Drag::FromBin { media, on: true, .. } = self.drag
            && rows_area.contains(self.mouse)
            && let Some(m) = self.project.media(media).cloned()
            && let Some((t, ry, rh)) = self.drop_target(&m, self.mouse)
        {
            let at = self.drop_frame(&m, self.mouse.x);
            let len = self.project.natural_len(&m);
            let g = rect(self.x_of(at), ry + 3.0, len as f32 * self.zoom, rh - 6.0);
            let kc = self.kind_color(self.project.tracks[t].kind);
            c.fill_round(g, 8.0, kc.fade(0.35));
            c.stroke_round(g, 8.0, kc, 1.5);
        }
        c.unclip();
        let _ = fps;
        // playhead and snap line
        let px = self.x_of(self.playhead);
        if px >= lanes_x - 1.0 && px <= lanes_x + lanes_w {
            c.fill(rect(px - 0.75, ruler.y + 4.0, 1.5, area.bottom() - ruler.y - 4.0), p.t1);
            let knob = rect(px - 6.0, ruler.y + 2.0, 12.0, 16.0);
            c.fill_round(knob, 4.0, p.accent);
        }
        if let Some(s) = self.snap_line {
            let sx = self.x_of(s);
            c.fill(rect(sx - 0.5, ruler.y, 1.0, area.bottom() - ruler.y), p.orange);
        }
        if self.project.tracks.iter().all(|t| t.clips.is_empty()) {
            draw::text(c, tr("Drag media here, or click + on a clip of the bin"), rect(lanes_x, rows_area.y, lanes_w, 60.0), 13.0, 400, p.t3, Align::Center);
        }
    }

    fn paint_ruler(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        let fps = self.project.fps.max(1) as i64;
        let steps_s = [0.0, 0.0, 0.0, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0];
        let mut step = fps * 3600;
        for (i, s) in steps_s.iter().enumerate() {
            let f = match i {
                0 => 1,
                1 => 5,
                2 => 10,
                _ => (*s * fps as f64).round() as i64,
            };
            if f as f32 * self.zoom >= 90.0 {
                step = f.max(1);
                break;
            }
        }
        let first = self.frame_at(r.x) / step * step;
        let mut f = first;
        c.clip(r);
        while self.x_of(f) < r.right() {
            let x = self.x_of(f);
            c.vline(x, r.y + 12.0, r.h - 12.0, p.t4);
            for k in 1..5 {
                let xm = self.x_of(f) + k as f32 * step as f32 * self.zoom / 5.0;
                c.vline(xm, r.y + 19.0, r.h - 19.0, p.t4.fade(0.6));
            }
            let s = f / fps;
            let label = if step < fps { format!("{}:{:02}:{:02}", s / 60, s % 60, f % fps) } else { format!("{}:{:02}", s / 60, s % 60) };
            draw::text(c, &label, rect(x + 4.0, r.y, 80.0, 16.0), 12.0, 400, p.t3, Align::Left);
            f += step;
        }
        c.unclip();
        self.hits.add(r, Hit::Ruler);
    }

    fn paint_clip(&mut self, c: &mut Canvas, cr: Rect, clip: &model::Clip, kind: TrackKind, off: bool) {
        let p = self.pal.clone();
        let kc = self.kind_color(kind);
        let selected = self.selected.contains(&clip.id);
        let base = kc.fade(if off { 0.15 } else { 0.32 }).over(p.card.over(p.bg));
        c.fill_round(cr, 8.0, base);
        c.clip(cr.inset(1.0));
        let fps = self.project.fps();
        let media = self.project.media_of(clip).cloned();
        let name = match (&clip.text, &media) {
            (Some(st), _) => st.text.lines().next().unwrap_or("").to_string(),
            (None, Some(m)) => m.name(),
            _ => String::new(),
        };
        if let Some(m) = &media {
            // filmstrip
            if m.visual()
                && let Some(strip) = self.strips.get(&m.id).cloned()
                && !strip.is_empty()
            {
                let th = cr.h - 2.0;
                let tw = th * 16.0 / 9.0;
                let mut x = cr.x.max(self.tl_rect.x + HEAD_W - tw);
                x = cr.x + ((x - cr.x) / tw).floor() * tw;
                let wic = self.wic.clone();
                while x < cr.right() && x < self.tl_rect.right() {
                    let mid = self.frame_at(x + tw / 2.0);
                    let idx = if m.kind == MediaKind::Image || m.duration <= 0.0 {
                        0
                    } else {
                        ((clip.src_at(mid, fps) / m.duration * strip.len() as f64).floor().max(0.0) as usize).min(strip.len() - 1)
                    };
                    let f = strip[idx].clone();
                    c.image(thumb_key(self.generation, m.id, idx), rect(x, cr.y + 1.0, tw, th), if off { 0.35 } else { 0.85 }, || draw::wic_source(&wic, &f));
                    x += tw;
                }
            }
            // sound outline
            if (kind.sound() || !m.visual())
                && m.has_audio
                && let Some(wave) = self.waves.get(&m.id).cloned()
                && !wave.is_empty()
            {
                let mid = cr.y + cr.h * 0.6;
                let amp = cr.h * 0.35 * clip.volume.min(2.0) as f32;
                let mut x = cr.x.max(self.tl_rect.x);
                while x < cr.right().min(self.tl_rect.right()) {
                    let src = clip.src_at(self.frame_at(x), fps);
                    let i = (src * 20.0).floor() as usize;
                    let v = wave.get(i).copied().unwrap_or(0.0);
                    let hh = (v * amp).max(0.5);
                    c.fill(rect(x, mid - hh, 1.5, hh * 2.0), kc.fade(if off { 0.4 } else { 0.9 }));
                    x += 3.0;
                }
            }
        }
        // name and badges, on a band
        if cr.w > 30.0 {
            let band = rect(cr.x, cr.y, cr.w, 18.0);
            if media.as_ref().is_some_and(|m| m.visual()) {
                c.fill(band, Color::hexa(0x000000, 0.45));
            }
            let ink = if media.as_ref().is_some_and(|m| m.visual()) { Color::hex(0xffffff) } else { p.t1 };
            let mut label = name;
            if (clip.speed - 1.0).abs() > 1e-3 {
                label = format!("{:.2}×  {label}", clip.speed);
            }
            if clip.text.is_some() {
                let r2 = rect(cr.x + 8.0, cr.y, cr.w - 12.0, cr.h);
                c.icon(&ic::TEXT, rect(r2.x, r2.y + (r2.h - 12.0) / 2.0, 12.0, 12.0), ink);
                draw::text(c, &label, rect(r2.x + 18.0, r2.y, r2.w - 18.0, r2.h), 12.0, 600, ink, Align::Left);
            } else {
                draw::text(c, &label, rect(cr.x + 6.0, cr.y, cr.w - 10.0, 18.0), 12.0, 600, ink, Align::Left);
            }
        }
        c.unclip();
        if selected {
            c.stroke_round(cr, 8.0, p.t1, 2.0);
            // trim grips
            if cr.w > 18.0 {
                c.fill_round(rect(cr.x + 2.0, cr.y + cr.h / 2.0 - 8.0, 3.0, 16.0), 1.5, p.t1);
                c.fill_round(rect(cr.right() - 5.0, cr.y + cr.h / 2.0 - 8.0, 3.0, 16.0), 1.5, p.t1);
            }
        } else if self.hovered(&Hit::Clip(clip.id)) {
            c.stroke_round(cr, 8.0, kc, 1.5);
        }
    }

    // ───────────────────────── dropping media ─────────────────────────

    /// The track a media would land on at `p`: the row under the mouse when it takes it, else its home track.
    fn drop_target(&self, m: &model::Media, p: Pt) -> Option<(usize, f32, f32)> {
        let row = self.row_at(p.y)?;
        let (t, y, h) = self.tl_rows[row];
        if self.project.tracks[t].kind.accepts(Some(m.kind)) {
            return Some((t, y, h));
        }
        let home = self.project.home_track(m.kind)?;
        self.tl_rows.iter().find(|r| r.0 == home).copied()
    }

    fn drop_frame(&self, m: &model::Media, x: f32) -> i64 {
        let f = self.frame_at(x);
        if !self.set.snap {
            return f;
        }
        let len = self.project.natural_len(m);
        let pts = self.project.snap_points(&[], self.playhead);
        let thr = (10.0 / self.zoom).ceil() as i64;
        match (snap(f, &pts, thr), snap(f + len, &pts, thr)) {
            (Some(a), _) => a,
            (None, Some(b)) => b - len,
            _ => f,
        }
    }

    // ───────────────────────── the mouse ─────────────────────────

    pub fn press(&mut self, h: Hit, p: Pt, clicks: u8, mods: Mods, cx: &mut Cx) {
        if self.edit_for.is_some() && !matches!(h, Hit::Field(_)) {
            self.edit_for = None;
        }
        match h {
            Hit::Ruler | Hit::TrackArea => {
                if matches!(h, Hit::TrackArea) && !mods.ctrl {
                    self.selected.clear();
                }
                self.drag = Drag::Scrub;
                let f = self.frame_at(p.x);
                self.seek_scrub(f, cx);
            }
            Hit::Clip(id) => {
                if mods.ctrl {
                    if let Some(i) = self.selected.iter().position(|s| *s == id) {
                        self.selected.remove(i);
                        return;
                    }
                    self.selected.push(id);
                } else if !self.selected.contains(&id) {
                    self.selected = vec![id];
                }
                if clicks == 2 && self.project.clip(id).is_some_and(|c| c.text.is_some()) {
                    self.selected = vec![id];
                    self.tab = Tab::Text;
                    self.edit_for = Some("text");
                    self.edit_shown = None;
                    return;
                }
                let grab = self.frame_at(p.x);
                let grab_track = self.row_at(p.y).unwrap_or(0);
                let ids = self.selected.clone();
                self.drag = Drag::Move { ids, grab, grab_track, before: Box::new(self.project.clone()), moved: false, start: p };
            }
            Hit::TrimL(id) | Hit::TrimR(id) => {
                self.selected = vec![id];
                let before = Box::new(self.project.clone());
                self.trim_left = matches!(h, Hit::TrimL(_));
                self.drag = if self.trim_left { Drag::TrimL { id, before } } else { Drag::TrimR { id, before } };
            }
            Hit::TransMark(id) => {
                self.selected = vec![id];
                self.tab = if self.project.clip(id).is_some_and(|c| c.text.is_some()) { Tab::Motion } else { Tab::Picture };
            }
            Hit::Media(id) => {
                self.media_sel = Some(id);
                self.drag = Drag::FromBin { media: id, start: p, on: false };
            }
            Hit::Slider(prop) => {
                let Some(id) = self.selected.first().copied() else { return };
                let Some(track) = self.slider_rects.get(&prop).copied() else { return };
                let before = Box::new(self.project.clone());
                if clicks == 2 {
                    let def = prop.range().2;
                    self.edit(|pr| props::set(pr, id, prop, def));
                    return;
                }
                self.drag = Drag::Slider { prop, track, before };
                self.drag_to(p, mods, cx);
            }
            Hit::Handle(handle) => {
                let Some(clip) = self.sel().cloned() else { return };
                self.drag = Drag::Handle { handle, id: clip.id, start: p, orig: Box::new(clip), before: Box::new(self.project.clone()) };
            }
            Hit::PreviewArea => {
                // a click on a picture of the preview selects its clip
                if let Some(id) = self.clip_at_preview(p) {
                    self.selected = vec![id];
                    if let Some(clip) = self.sel().cloned() {
                        self.drag = Drag::Handle { handle: Handle::Body, id, start: p, orig: Box::new(clip), before: Box::new(self.project.clone()) };
                    }
                } else {
                    self.selected.clear();
                }
            }
            Hit::ZoomBar => {
                let Some(r) = self.hits.rect_of(|h| *h == Hit::ZoomBar) else { return };
                self.drag = Drag::ZoomBar { track: r.inset_xy(6.0, 0.0) };
                self.drag_to(p, mods, cx);
            }
            _ => {}
        }
    }

    /// The topmost picture of the preview under `p` (by its box, not turned).
    fn clip_at_preview(&self, p: Pt) -> Option<Id> {
        let fr = self.frame_rect;
        let layers = self.project.layers(self.playhead);
        for l in layers.iter().rev() {
            let clip = self.project.clip(l.clip)?;
            let q = match &clip.text {
                Some(st) => {
                    let size = st.size as f32 * fr.h;
                    let w = size * 0.6 * st.text.lines().map(|s| s.chars().count()).max().unwrap_or(1) as f32;
                    let h = size * 1.4 * st.text.lines().count().max(1) as f32;
                    let cxp = fr.x + fr.w / 2.0 + clip.x as f32 * fr.w;
                    let cyp = fr.y + fr.h / 2.0 + clip.y as f32 * fr.h;
                    rect(cxp - w / 2.0, cyp - h / 2.0, w, h)
                }
                None => {
                    let m = self.project.media_of(clip)?;
                    let pl = crate::render::place(clip, m.width as f64, m.height as f64, fr.w as f64, fr.h as f64);
                    rect(fr.x + (pl.cx - pl.w / 2.0) as f32, fr.y + (pl.cy - pl.h / 2.0) as f32, pl.w as f32, pl.h as f32)
                }
            };
            if q.contains(p) && fr.contains(p) {
                return Some(clip.id);
            }
        }
        None
    }

    /// Moves the playhead while scrubbing (the preview follows with exact frames).
    fn seek_scrub(&mut self, f: i64, cx: &mut Cx) {
        self.stop_play();
        self.playhead = f.clamp(0, self.project.duration().max(0));
        cx.repaint();
    }

    pub fn drag_to(&mut self, p: Pt, mods: Mods, cx: &mut Cx) {
        let mut drag = std::mem::replace(&mut self.drag, Drag::None);
        self.drag_step(&mut drag, p, mods, cx);
        self.drag = drag;
    }

    fn drag_step(&mut self, drag: &mut Drag, p: Pt, mods: Mods, cx: &mut Cx) {
        let thr = (8.0 / self.zoom).ceil() as i64;
        let snapping = self.set.snap && !mods.alt;
        let playhead = self.playhead;
        match drag {
            Drag::None => {}
            Drag::Scrub => {
                let f = self.frame_at(p.x);
                self.seek_scrub(f, cx);
            }
            Drag::Move { ids, grab, grab_track, before, moved, start } => {
                if (p.x - start.x).abs() < 3.0 && (p.y - start.y).abs() < 3.0 && !*moved {
                    return;
                }
                *moved = true;
                let mut delta = self.frame_at(p.x) - *grab;
                let row = self.row_at(p.y).unwrap_or(*grab_track);
                let shift = row as i32 - *grab_track as i32;
                self.snap_line = None;
                if snapping {
                    let clips: Vec<&model::Clip> = ids.iter().filter_map(|id| before.clip(*id)).collect();
                    let s0 = clips.iter().map(|c| c.start).min().unwrap_or(0);
                    let e0 = clips.iter().map(|c| c.end()).max().unwrap_or(0);
                    let pts = before.snap_points(ids, playhead);
                    let a = snap(s0 + delta, &pts, thr).map(|v| (v - (s0 + delta), v));
                    let b = snap(e0 + delta, &pts, thr).map(|v| (v - (e0 + delta), v));
                    let best = match (a, b) {
                        (Some(x), Some(y)) => Some(if x.0.abs() <= y.0.abs() { x } else { y }),
                        (x, y) => x.or(y),
                    };
                    if let Some((adj, at)) = best {
                        delta += adj;
                        self.snap_line = Some(at);
                    }
                }
                let mut next = (**before).clone();
                if next.move_clips(ids, delta, shift).is_ok() {
                    self.project = next;
                } else {
                    let mut flat = (**before).clone();
                    self.project = if flat.move_clips(ids, delta, 0).is_ok() { flat } else { (**before).clone() };
                }
                cx.repaint();
            }
            Drag::TrimL { id, before } | Drag::TrimR { id, before } => {
                let left = self.trim_left;
                let mut f = self.frame_at(p.x);
                self.snap_line = None;
                if snapping && let Some(s) = snap(f, &before.snap_points(&[*id], playhead), thr) {
                    f = s;
                    self.snap_line = Some(s);
                }
                let mut next = (**before).clone();
                if left {
                    next.trim_start(*id, f);
                } else {
                    next.trim_end(*id, f);
                }
                self.project = next;
                cx.repaint();
            }
            Drag::FromBin { start, on, .. } => {
                if !*on && ((p.x - start.x).abs() > 4.0 || (p.y - start.y).abs() > 4.0) {
                    *on = true;
                }
                cx.repaint();
            }
            Drag::Slider { prop, track, before } => {
                let Some(id) = self.selected.first().copied() else { return };
                let (lo, hi, def) = prop.range();
                let mut v = lo + ((p.x - track.x) / track.w).clamp(0.0, 1.0) as f64 * (hi - lo);
                // a little pull towards the default value
                if (v - def).abs() < (hi - lo) * 0.015 {
                    v = def;
                }
                let mut next = (**before).clone();
                props::set(&mut next, id, *prop, v);
                self.project = next;
                cx.repaint();
            }
            Drag::Handle { handle, id, start, orig, before } => {
                let fr = self.frame_rect;
                let (cx0, cy0) = (fr.x + fr.w / 2.0 + orig.x as f32 * fr.w, fr.y + fr.h / 2.0 + orig.y as f32 * fr.h);
                let mut next = (**before).clone();
                if let Some(c) = next.clip_mut(*id) {
                    match handle {
                        Handle::Body => {
                            let mut nx = orig.x + ((p.x - start.x) / fr.w) as f64;
                            let mut ny = orig.y + ((p.y - start.y) / fr.h) as f64;
                            // the centre lines hold the picture a little
                            if snapping && nx.abs() < 0.012 {
                                nx = 0.0;
                            }
                            if snapping && ny.abs() < 0.012 {
                                ny = 0.0;
                            }
                            c.x = nx.clamp(-1.0, 1.0);
                            c.y = ny.clamp(-1.0, 1.0);
                        }
                        Handle::Corner(_) => {
                            let d0 = ((start.x - cx0).powi(2) + (start.y - cy0).powi(2)).sqrt().max(1.0);
                            let d1 = ((p.x - cx0).powi(2) + (p.y - cy0).powi(2)).sqrt();
                            c.scale = (orig.scale * (d1 / d0) as f64).clamp(0.1, 4.0);
                        }
                        Handle::Rotate => {
                            let a0 = (start.y - cy0).atan2(start.x - cx0);
                            let a1 = (p.y - cy0).atan2(p.x - cx0);
                            let mut deg = orig.rotation + (a1 - a0).to_degrees() as f64;
                            deg = (deg + 180.0).rem_euclid(360.0) - 180.0;
                            if mods.shift {
                                deg = (deg / 15.0).round() * 15.0;
                            } else if snapping && (deg / 90.0 - (deg / 90.0).round()).abs() < 0.03 {
                                deg = (deg / 90.0).round() * 90.0;
                            }
                            c.rotation = deg;
                        }
                    }
                }
                self.project = next;
                cx.repaint();
            }
            Drag::ZoomBar { track } => {
                let k = ((p.x - track.x) / track.w).clamp(0.0, 1.0);
                let z = ZOOM_MIN * (ZOOM_MAX / ZOOM_MIN).powf(k);
                self.zoom_by(z / self.zoom, None);
                cx.repaint();
            }
        }
    }

    /// The end of a press. True when it was a drag (not a click).
    pub fn end_drag(&mut self, p: Pt, _mods: Mods, cx: &mut Cx) -> bool {
        self.snap_line = None;
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        match drag {
            Drag::None => false,
            Drag::Scrub | Drag::ZoomBar { .. } => true,
            Drag::Move { before, moved, ids, .. } => {
                if moved {
                    let ok = ids.iter().all(|id| self.project.find(*id).is_some());
                    if !ok {
                        self.project = *before;
                    } else if self.project == *before {
                        self.say(tr(model::NO_ROOM), cx);
                    } else {
                        self.commit(*before);
                    }
                }
                true
            }
            Drag::TrimL { before, .. } | Drag::TrimR { before, .. } | Drag::Slider { before, .. } | Drag::Handle { before, .. } => {
                self.commit(*before);
                true
            }
            Drag::FromBin { media, on, .. } => {
                if !on {
                    return false;
                }
                let inside =
                    self.tl_rows.iter().any(|(_, y, h)| p.y >= *y - 20.0 && p.y < y + h + 20.0) && p.x >= self.tl_x0 - 20.0 && self.tl_rect.contains(p);
                if !inside {
                    return true;
                }
                let Some(m) = self.project.media(media).cloned() else { return true };
                let Some((t, _, _)) = self.drop_target(&m, p) else { return true };
                let at = self.drop_frame(&m, p.x);
                let r = self.edit(|pr| pr.add_media(media, t, at));
                match r {
                    Ok(id) => self.selected = vec![id],
                    Err(e) => self.say(tr(e), cx),
                }
                true
            }
        }
    }

    /// A click (press and release on the same thing).
    pub fn act(&mut self, hit: Hit, mods: Mods, cx: &mut Cx) {
        let _ = mods;
        match hit {
            Hit::Min => cx.after(|w| w.minimize()),
            Hit::Max => cx.after(|w| w.toggle_maximize()),
            Hit::Close => cx.after(|w| w.close()),
            Hit::Home => self.go_home(cx),
            Hit::Help => self.dialog = Some(Dialog::Help),
            Hit::Gear => {
                self.dialog = Some(Dialog::Settings);
            }
            Hit::Export => {
                if !matches!(self.export, ExportState::Running { .. }) {
                    self.export = ExportState::Idle;
                }
                self.dialog = Some(Dialog::Export);
                self.check_hw(cx);
            }
            Hit::Undo => self.undo(cx),
            Hit::Redo => self.redo(cx),
            Hit::NewProject => {
                self.new_project(cx);
            }
            Hit::OpenProject => self.pick_project(cx),
            Hit::Recent(i) => {
                let files: Vec<_> = self.set.recent.iter().filter(|f| f.is_file()).cloned().collect();
                if let Some(f) = files.get(i) {
                    self.open_project(&f.clone(), cx);
                }
            }
            Hit::Import => self.pick_media(cx),
            Hit::MediaAdd(id) => self.append_media(id, cx),
            Hit::MediaMusic(id) => {
                let at = self.playhead;
                match self.edit(|p| p.add_music(id, at)) {
                    Ok(c) => self.selected = vec![c],
                    Err(e) => self.say(tr(e), cx),
                }
            }
            Hit::TextPreset(i) => {
                if let Some((_, st)) = text_presets().into_iter().nth(i) {
                    self.add_text(st, cx);
                }
            }
            Hit::TransPreset(i) => {
                let kind = [None, Some(model::TransKind::Crossfade), Some(model::TransKind::Dip)][i.min(2)];
                self.set_transition(kind, cx);
            }
            Hit::InstallTools => self.install_tools(cx),
            Hit::Play => self.toggle_play(cx),
            Hit::StepBack => self.seek(self.playhead - 1, cx),
            Hit::StepFwd => self.seek(self.playhead + 1, cx),
            Hit::Aspect => {
                let i = props::ASPECTS.iter().position(|a| *a == self.project.aspect).map_or(0, |i| (i + 1) % props::ASPECTS.len());
                let a = props::ASPECTS[i];
                self.edit(|p| p.aspect = a);
            }
            Hit::Chip(key, i) => self.chip(key, i, cx),
            Hit::Toggle(name) => match name {
                "hw" => {
                    self.set.hw = !self.set.hw;
                    self.set.save();
                    if self.set.hw {
                        self.check_hw(cx);
                    }
                }
                "duck" => {
                    self.edit(|p| p.duck = !p.duck);
                }
                "bold" => self.edit_text_style(|st| st.bold = !st.bold),
                _ => {}
            },
            Hit::Swatch(key, i) => self.swatch(key, i),
            Hit::Field(name) => {
                self.edit_for = Some(name);
                self.edit_shown = None;
                self.typing = None;
            }
            Hit::Action(a) => match a {
                "fit" | "fill" | "reset_pos" | "reset_color" => self.clip_action(a),
                "export_go" => self.start_export(cx),
                "export_cancel" => self.cancel_export(),
                "export_close" | "dialog_close" => {
                    if !matches!(self.export, ExportState::Running { .. }) {
                        self.export = ExportState::Idle;
                    }
                    self.dialog = None;
                }
                "export_show" => {
                    if let ExportState::Done { path, .. } = &self.export {
                        self.show_in_folder(&path.clone());
                    }
                }
                "out_dir" => {
                    if let Some(d) = plume_ui::dialogs::pick_folder(self.win.hwnd, tr("Save exports in")) {
                        self.set.out_dir = Some(d);
                        self.set.save();
                    }
                }
                _ => {}
            },
            Hit::Tool(t) => match t {
                "split" => self.split(cx),
                "delete" => self.delete(false, cx),
                "ripple" => self.delete(true, cx),
                "text" => {
                    self.add_text(model::TextStyle { text: tr("Your text").into(), ..model::TextStyle::default() }, cx);
                }
                "snap" => {
                    self.set.snap = !self.set.snap;
                    self.set.save();
                }
                "zoom_in" => self.zoom_by(1.4, None),
                "zoom_out" => self.zoom_by(1.0 / 1.4, None),
                "fit" => self.fit_zoom(),
                _ => {}
            },
            Hit::TrackMute(t) => {
                self.edit(|p| {
                    if let Some(tr) = p.tracks.get_mut(t) {
                        tr.muted = !tr.muted;
                    }
                });
            }
            Hit::TrackHide(t) => {
                self.edit(|p| {
                    if let Some(tr) = p.tracks.get_mut(t) {
                        tr.hidden = !tr.hidden;
                    }
                });
            }
            Hit::Toast => self.toast = None,
            Hit::DialogBg if !matches!(self.export, ExportState::Running { .. }) => {
                if self.dialog == Some(Dialog::Export) {
                    self.export = ExportState::Idle;
                }
                self.dialog = None;
            }
            _ => {}
        }
        cx.repaint();
    }
}
