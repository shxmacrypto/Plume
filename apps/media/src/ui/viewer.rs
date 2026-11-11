//! The photo viewer and the video player: the stage, the top bar, the arrows, the player's controls, the
//! subtitles and the info panel.

use super::{App, CONTROLS_H, Hit, INFO_W, TOP_H, draw, draw::ic};
use crate::{
    i18n::{date_time, fill, tr},
    library,
    library::clock,
    srt,
};
use plume_ui::{Align, Canvas, Color, Font, PaintCx, Rect, icons, icons::Icon, m, rect};
use std::time::Instant;

const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

impl App {
    pub fn paint_viewer(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let p = self.pal.clone();
        let (w, h) = (cx.w, cx.h);
        c.clear(p.stage);
        let st = self.stage();
        let is_video = self.open.as_ref().is_some_and(|o| o.video.is_some());
        if is_video {
            self.paint_video(c, st);
        } else {
            self.paint_photo(c, st);
            if self.bars_shown() {
                self.paint_arrows(c, st);
            }
        }
        let bars = self.bars_shown();
        if self.full.is_none() {
            let bar = rect(0.0, 0.0, w, TOP_H);
            c.fill(bar, if p.dark { Color::hex(0x151719) } else { p.side });
            c.hline(0.0, TOP_H - 1.0, w, p.line);
            self.paint_topbar(c, w, p.t1, is_video);
            self.paint_caption(c, w, cx.maximized, false);
        } else if bars {
            c.gradient_v(rect(0.0, 0.0, w, TOP_H + 30.0), Color::hex(0x000000).fade(0.6), Color::hex(0x000000).fade(0.0));
            self.paint_topbar(c, w, WHITE, is_video);
        }
        if self.set.info_open && self.full.is_none() {
            self.paint_info(c, rect(w - INFO_W, TOP_H, INFO_W, h - TOP_H));
        }
    }

    fn paint_topbar(&mut self, c: &mut Canvas, w: f32, ink: Color, is_video: bool) {
        let Some(o) = &self.open else { return };
        let name = o.path().file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let (pos, n) = (o.pos + 1, o.seq.len());
        let fav = self.store.is_favorite(o.path());
        let zoom = o.photo.as_ref().filter(|p| p.dec.is_some()).map(|p| (p.zoom, p.fit));
        self.icon_btn(c, &icons::BACK, rect(8.0, 6.0, 36.0, 36.0), Hit::Back, ink, false);
        // buttons from the right, before the window's own
        let mut x = w - if self.full.is_none() { 3.0 * m::CAPTION_W + 8.0 } else { 12.0 };
        fn btn(app: &mut App, c: &mut Canvas, x: &mut f32, icon: &Icon, hit: Hit, ink: Color, on: bool) {
            *x -= 38.0;
            app.icon_btn(c, icon, rect(*x, 6.0, 36.0, 36.0), hit, ink, on);
        }
        let full = self.full.is_some();
        btn(self, c, &mut x, if full { &ic::UNFULL } else { &ic::FULL }, Hit::Fullscreen, ink, false);
        if !is_video {
            let on = self.slideshow.is_some();
            btn(self, c, &mut x, &ic::SLIDES, Hit::Slideshow, ink, on);
        }
        let on = self.set.info_open;
        btn(self, c, &mut x, &ic::INFO, Hit::Info, ink, on);
        // the star
        x -= 38.0;
        let star = rect(x, 6.0, 36.0, 36.0);
        if self.hovered(&Hit::Fav) {
            c.fill_round(star, 18.0, ink.fade(0.1));
        }
        let sq = rect(star.x + 9.0, star.y + 9.0, 18.0, 18.0);
        if fav {
            c.icon_fill(&icons::STAR, sq, Color::hex(0xffd60a));
        } else {
            c.icon(&icons::STAR, sq, ink);
        }
        self.hits.add(star, Hit::Fav);
        if let Some((z, fit)) = zoom {
            btn(self, c, &mut x, &ic::ROT_R, Hit::RotR, ink, false);
            btn(self, c, &mut x, &ic::ROT_L, Hit::RotL, ink, false);
            x -= 12.0;
            btn(self, c, &mut x, &ic::ZOOM_IN, Hit::ZoomIn, ink, false);
            let label = format!("{}%", (z * 100.0).round());
            let lw = 58.0;
            x -= lw + 2.0;
            let r = rect(x, 10.0, lw, 28.0);
            let hot = self.hovered(&Hit::ZoomFit);
            c.fill_round(
                r,
                14.0,
                ink.fade(if hot {
                    0.16
                } else if fit {
                    0.0
                } else {
                    0.08
                }),
            );
            c.text(&label, r, Font::SmallBold, ink, Align::Center);
            self.hits.add(r, Hit::ZoomFit);
            btn(self, c, &mut x, &ic::ZOOM_OUT, Hit::ZoomOut, ink, false);
        }
        // the name and where it is in the sequence
        let tx = 52.0;
        let room = (x - tx - 12.0).max(40.0);
        let nw = c.text_width(&name, Font::BodyBold).min(room - 80.0).max(20.0);
        c.text(&name, rect(tx, 0.0, nw, TOP_H), Font::BodyBold, ink, Align::Left);
        if n > 1 {
            c.text(
                &fill(tr("{a} of {b}"), &[&pos.to_string(), &n.to_string()]),
                rect(tx + nw + 10.0, 1.0, 120.0, TOP_H),
                Font::Small,
                ink.fade(0.6),
                Align::Left,
            );
        }
    }

    fn paint_photo(&mut self, c: &mut Canvas, st: Rect) {
        let p = self.pal.clone();
        let scale = self.scale;
        let rt = c.target();
        let rt_id = draw::target_id(&rt);
        let path = self.open.as_ref().map(|o| o.path().to_path_buf()).unwrap_or_default();
        self.hits.add(st, Hit::Stage);
        let Some(ph) = self.open.as_mut().and_then(|o| o.photo.as_mut()) else { return };
        if let Some(dec) = ph.dec.clone() {
            if ph.bmp_rt != rt_id {
                ph.bmps.clear();
                ph.bmp_rt = rt_id;
            }
            let n = dec.frames.len();
            if ph.bmps.len() != n {
                ph.bmps = vec![None; n];
            }
            // animated GIF: the next frame when its time has come
            if n > 1 && Instant::now() >= ph.next_frame {
                ph.frame = (ph.frame + 1) % n;
                ph.next_frame = Instant::now() + dec.delays.get(ph.frame).copied().unwrap_or(std::time::Duration::from_millis(100));
            }
            let f = ph.frame.min(n - 1);
            if ph.bmps[f].is_none() {
                ph.bmps[f] = draw::make_bitmap(&rt, &dec.frames[f]);
            }
            let Some((iw, ih)) = ph.dims() else { return };
            let (dw, dh) = (iw * ph.zoom / scale, ih * ph.zoom / scale);
            let (cx, cy) = (st.x + st.w / 2.0 + ph.ox, st.y + st.h / 2.0 + ph.oy);
            let dest = rect(cx - dw / 2.0, cy - dh / 2.0, dw, dh);
            // shrinking: smooth; enlarging past 3× the decoded pixels: sharp pixels
            let shown_w = if ph.turns % 2 == 1 { dec.h() } else { dec.w() };
            let smooth = dw * scale / shown_w.max(1) as f32 * 1.0 < 3.0;
            if let Some(b) = ph.bmps[f].clone() {
                c.clip(st);
                draw::picture(c, &b, dest, None, ph.turns, 1.0, smooth);
                c.unclip();
            }
            return;
        }
        let err = ph.error.clone();
        let since = ph.opened.elapsed();
        // while it decodes: its thumbnail, if there is one
        let key = self.lib.iter().find(|i| i.path == path).map(|i| i.key());
        if let Some(b) = key.and_then(|k| self.bitmaps.get(k)) {
            let s = unsafe { b.GetPixelSize() };
            let (bw, bh) = (s.width as f32, s.height as f32);
            let k = ((st.w - 16.0) / bw).min((st.h - 16.0) / bh).min(4.0);
            let r = rect(st.x + (st.w - bw * k) / 2.0, st.y + (st.h - bh * k) / 2.0, bw * k, bh * k);
            draw::picture(c, &b, r, None, 0, 0.9, true);
        }
        if let Some(e) = err {
            let msg = if e == "no codec" {
                tr("Windows has no codec for this format. HEIC and RAW need the free extensions from the Microsoft Store.")
            } else {
                tr("This picture can't be opened.")
            };
            self.paint_message(c, st, msg, &path);
        } else if since.as_millis() > 300 && key.is_none() {
            c.text(tr("Loading…"), rect(st.x, st.y + st.h / 2.0 - 12.0, st.w, 24.0), Font::Body, p.t3.mix(WHITE, 0.4), Align::Center);
        }
    }

    fn paint_message(&mut self, c: &mut Canvas, st: Rect, msg: &str, path: &std::path::Path) {
        let cy = st.y + st.h / 2.0;
        draw::circle(c, st.x + st.w / 2.0, cy - 40.0, 28.0, WHITE.fade(0.08));
        c.icon(&ic::EMPTY, rect(st.x + st.w / 2.0 - 14.0, cy - 54.0, 28.0, 28.0), WHITE.fade(0.7));
        let w = (st.w - 80.0).min(520.0);
        let x = st.x + (st.w - w) / 2.0;
        draw::text_wrap(c, msg, rect(x, cy, w, 60.0), 15.0, 600, WHITE.fade(0.9), Align::Center);
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        c.text(&name, rect(x, cy + 48.0, w, 20.0), Font::Small, WHITE.fade(0.5), Align::Center);
    }

    fn paint_video(&mut self, c: &mut Canvas, st: Rect) {
        let rt = c.target();
        let rt_id = draw::target_id(&rt);
        c.fill(st, Color::hex(0x000000));
        self.hits.add(st, Hit::Stage);
        let path = self.open.as_ref().map(|o| o.path().to_path_buf()).unwrap_or_default();
        let Some(v) = self.open.as_mut().and_then(|o| o.video.as_mut()) else { return };
        if v.bmp_rt != rt_id {
            v.bmp = None;
            v.preview_bmp = None;
            v.bmp_rt = rt_id;
        }
        if let Some(pl) = &mut v.player {
            pl.tick();
            if pl.fresh || (v.bmp.is_none() && pl.w > 0) {
                if v.bmp.is_none() || v.bmp_size != (pl.w, pl.h) {
                    v.bmp = draw::empty_bitmap(&rt, pl.w, pl.h);
                    v.bmp_size = (pl.w, pl.h);
                }
                if let Some(b) = &v.bmp {
                    pl.with_pixels(|_, _, stride, data| unsafe {
                        let _ = b.CopyFromMemory(None, data.as_ptr() as *const _, stride);
                    });
                }
                if pl.fresh {
                    v.frames += 1;
                }
                pl.fresh = false;
            }
        }
        if let Some(b) = v.bmp.clone() {
            let (bw, bh) = (v.bmp_size.0 as f32, v.bmp_size.1 as f32);
            let k = (st.w / bw).min(st.h / bh);
            let r = rect(st.x + (st.w - bw * k) / 2.0, st.y + (st.h - bh * k) / 2.0, bw * k, bh * k);
            draw::picture(c, &b, r, None, 0, 1.0, true);
        }
        let error = v.error.clone();
        if let Some(_e) = error {
            self.paint_message(c, st, tr("This video can't be played."), &path);
            return;
        }
        let shown = self.controls_shown();
        // subtitles, above the controls when they show
        let t = self.player().map_or(0.0, |p| p.position());
        if let Some(text) = self.video().and_then(|v| srt::at(&v.cues, t)) {
            let size = (st.h * 0.045).clamp(17.0, 40.0);
            let maxw = st.w * 0.8;
            let bottom = st.y + st.h - if shown { CONTROLS_H + 8.0 } else { 36.0 };
            let lines: Vec<&str> = text.lines().collect();
            let lw = lines.iter().map(|l| draw::text_width(c, l, size, 600)).fold(0.0f32, f32::max).min(maxw);
            let th = draw::wrap_height(c, &text, maxw, size, 600);
            let r = rect(st.x + (st.w - maxw) / 2.0, bottom - th, maxw, th);
            c.fill_round(rect(st.x + (st.w - lw) / 2.0 - 12.0, r.y - 4.0, lw + 24.0, th + 8.0), 8.0, Color::hex(0x000000).fade(0.55));
            draw::text_wrap(c, &text, r, size, 600, WHITE, Align::Center);
        }
        if shown {
            self.paint_controls(c, st);
        }
    }

    fn paint_controls(&mut self, c: &mut Canvas, st: Rect) {
        let p = self.pal.clone();
        let cb = rect(st.x, st.y + st.h - CONTROLS_H, st.w, CONTROLS_H);
        c.gradient_v(cb, Color::hex(0x000000).fade(0.0), Color::hex(0x000000).fade(0.78));
        self.hits.add(rect(cb.x, cb.y + 24.0, cb.w, cb.h - 24.0), Hit::Controls);
        let (pos, dur, paused, muted) = match self.player() {
            Some(pl) => (pl.position(), pl.duration(), pl.paused() || pl.ended(), self.set.muted || !crate::settings::sound_allowed()),
            None => (0.0, 0.0, true, true),
        };
        let seeking = self.video().and_then(|v| v.seeking);
        let shown_pos = seeking.unwrap_or(pos);
        // the seek bar
        let bar = rect(cb.x + 16.0, cb.y + 30.0, cb.w - 32.0, 22.0);
        let hot = self.hovered(&Hit::Seek) || seeking.is_some();
        let th = if hot { 6.0 } else { 4.0 };
        let track = rect(bar.x + 8.0, bar.y + (bar.h - th) / 2.0, bar.w - 16.0, th);
        c.fill_round(track, th / 2.0, WHITE.fade(0.28));
        let k = if dur > 0.0 { (shown_pos / dur).clamp(0.0, 1.0) as f32 } else { 0.0 };
        c.fill_round(rect(track.x, track.y, track.w * k, th), th / 2.0, p.accent);
        if hot {
            draw::circle(c, track.x + track.w * k, track.y + th / 2.0, 7.0, WHITE);
        }
        self.hits.add(bar, Hit::Seek);
        // the frame above the bar where the pointer is
        if hot && dur > 0.0 {
            let ht = seeking.unwrap_or_else(|| self.seek_time(self.mouse.x));
            self.paint_preview(c, track, ht, dur);
        }
        // the buttons
        let y = cb.y + 56.0;
        let mut x = cb.x + 12.0;
        self.icon_btn(c, if paused { &ic::PLAY } else { &ic::PAUSE }, rect(x, y, 40.0, 40.0), Hit::PlayPause, WHITE, false);
        x += 42.0;
        if self.open.as_ref().is_some_and(|o| o.seq.len() > 1) {
            self.icon_btn(c, &icons::BACK, rect(x, y + 2.0, 36.0, 36.0), Hit::Prev, WHITE, false);
            x += 36.0;
            self.icon_btn(c, &icons::FORWARD, rect(x, y + 2.0, 36.0, 36.0), Hit::Next, WHITE, false);
            x += 40.0;
        }
        self.icon_btn(c, if muted || self.set.volume <= 0.0 { &ic::MUTED } else { &ic::VOLUME }, rect(x, y + 2.0, 36.0, 36.0), Hit::Mute, WHITE, false);
        x += 38.0;
        // the volume
        let vr = rect(x, y + 10.0, 96.0, 20.0);
        let vt = rect(vr.x + 6.0, vr.y + 8.0, vr.w - 12.0, 4.0);
        c.fill_round(vt, 2.0, WHITE.fade(0.28));
        let vol = if muted { 0.0 } else { self.set.volume };
        c.fill_round(rect(vt.x, vt.y, vt.w * vol, 4.0), 2.0, WHITE);
        if self.hovered(&Hit::Volume) || matches!(self.drag, Some(super::Drag::Volume)) {
            draw::circle(c, vt.x + vt.w * vol, vt.y + 2.0, 6.0, WHITE);
        }
        self.hits.add(vr, Hit::Volume);
        x += 108.0;
        let times = if dur > 0.0 { format!("{} / {}", clock(shown_pos), clock(dur)) } else { clock(shown_pos) };
        c.text(&times, rect(x, y, 200.0, 40.0), Font::SmallBold, WHITE.fade(0.9), Align::Left);
        // right side
        let mut rx = cb.x + cb.w - 12.0;
        rx -= 40.0;
        self.icon_btn(c, if self.full.is_some() { &ic::UNFULL } else { &ic::FULL }, rect(rx, y, 40.0, 40.0), Hit::Fullscreen, WHITE, false);
        let rate: f64 = self.video().map_or(1.0, |v| v.rate);
        let label = if (rate - 1.0).abs() < 0.01 { "1×".to_string() } else { format!("{rate}×") };
        let lw = c.text_width(&label, Font::SmallBold) + 22.0;
        rx -= lw + 6.0;
        let sr = rect(rx, y + 8.0, lw, 24.0);
        let hot = self.hovered(&Hit::Speed);
        c.fill_round(sr, 12.0, WHITE.fade(if hot { 0.22 } else { 0.12 }));
        c.text(&label, sr, Font::SmallBold, WHITE, Align::Center);
        self.hits.add(sr, Hit::Speed);
        let (ntracks, nsubs, subs_on) = self.video().map_or((0, 0, false), |v| (v.tracks.len(), v.subs.len(), v.sub.is_some()));
        if ntracks > 1 {
            rx -= 42.0;
            self.icon_btn(c, &ic::AUDIO, rect(rx, y, 40.0, 40.0), Hit::Audio, WHITE, false);
        }
        if nsubs > 0 {
            rx -= 42.0;
            self.icon_btn(c, &ic::CC, rect(rx, y, 40.0, 40.0), Hit::Subs, WHITE, subs_on);
        }
    }

    fn paint_preview(&mut self, c: &mut Canvas, track: Rect, t: f64, dur: f64) {
        let rt = c.target();
        let x = track.x + track.w * (t / dur).clamp(0.0, 1.0) as f32;
        let Some(v) = self.open.as_mut().and_then(|o| o.video.as_mut()) else { return };
        if v.preview_bmp.is_none()
            && let Some((pt, px)) = &v.preview
        {
            v.preview_bmp = draw::make_bitmap(&rt, px).map(|b| (*pt, b));
        }
        let label = clock(t);
        let (pw, ph): (f32, f32) = match &v.preview_bmp {
            Some((_, b)) => {
                let s = unsafe { b.GetPixelSize() };
                let w = 176.0;
                (w, w * s.height as f32 / s.width.max(1) as f32)
            }
            None => (0.0, 0.0),
        };
        let bx = (x - pw.max(60.0) / 2.0).clamp(track.x, track.x + track.w - pw.max(60.0));
        let by = track.y - 18.0 - ph - 22.0;
        if let Some((_, b)) = v.preview_bmp.clone() {
            let r = rect(bx, by, pw, ph);
            c.fill_round(r.inset(-2.0), 8.0, WHITE.fade(0.9));
            draw::picture(c, &b, r, None, 0, 1.0, true);
        }
        let lw = c.text_width(&label, Font::SmallBold) + 16.0;
        let lr = rect(bx + pw.max(60.0) / 2.0 - lw / 2.0, track.y - 36.0, lw, 22.0);
        c.fill_round(lr, 11.0, Color::hex(0x000000).fade(0.7));
        c.text(&label, lr, Font::SmallBold, WHITE, Align::Center);
    }

    fn paint_info(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.fill(r, if p.dark { Color::hex(0x17191b) } else { p.card });
        c.vline(r.x, r.y, r.h, p.line);
        self.hits.add(r, Hit::Panel);
        draw::text(c, tr("Info"), rect(r.x + 20.0, r.y + 14.0, r.w - 40.0, 28.0), 18.0, 700, p.t1, Align::Left);
        let Some(o) = &self.open else { return };
        let path = o.path().to_path_buf();
        let vsize = o.video.as_ref().map(|v| v.bmp_size).filter(|s| s.0 > 0);
        let vtracks = o.video.as_ref().map(|v| (v.tracks.len(), v.subs.len()));
        let Some(info) = self.info.as_ref().filter(|i| i.path == path) else {
            c.text(tr("Loading…"), rect(r.x + 20.0, r.y + 56.0, r.w - 40.0, 20.0), Font::Small, p.t3, Align::Left);
            return;
        };
        let mut rows: Vec<(&str, String)> = vec![];
        rows.push((tr("File"), path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()));
        rows.push((tr("Folder"), path.parent().map(|s| s.display().to_string()).unwrap_or_default()));
        let dims = info.dims.or(vsize);
        if let Some((w, h)) = dims {
            let mp = w as f64 * h as f64 / 1e6;
            rows.push((tr("Dimensions"), if mp >= 1.0 { format!("{w} × {h} ({mp:.1} MP)") } else { format!("{w} × {h}") }));
        }
        if info.duration > 0.0 {
            rows.push((tr("Duration"), clock(info.duration)));
        }
        rows.push((tr("Size"), library::bytes(info.size)));
        let e = &info.exif;
        match e.taken {
            Some(t) => {
                let (y, mo, d, h, mi) = library::parts(t);
                rows.push((tr("Taken"), date_time(y, mo, d, h, mi)));
            }
            None => {
                let (y, mo, d, h, mi) = library::parts(info.modified);
                rows.push((tr("Modified"), date_time(y, mo, d, h, mi)));
            }
        }
        if let Some(cam) = &e.camera {
            rows.push((tr("Camera"), cam.clone()));
        }
        if let Some(l) = &e.lens {
            rows.push((tr("Lens"), l.clone()));
        }
        let expo: Vec<String> = [&e.exposure, &e.aperture, &e.iso, &e.focal].iter().filter_map(|v| (*v).clone()).collect();
        if !expo.is_empty() {
            rows.push((tr("Exposure"), expo.join(" · ")));
        }
        if let Some(f) = &e.flash {
            rows.push(("Flash", f.clone()));
        }
        if let Some((lat, lon)) = e.gps {
            rows.push((tr("Location"), format!("{lat:.5}, {lon:.5}")));
        }
        if let Some((nt, ns)) = vtracks {
            if nt > 1 {
                rows.push((tr("Audio tracks"), nt.to_string()));
            }
            if ns > 0 {
                rows.push((tr("Subtitles"), ns.to_string()));
            }
        }
        let mut y = r.y + 56.0;
        let w = r.w - 40.0;
        for (label, value) in rows {
            c.text(label, rect(r.x + 20.0, y, w, 18.0), Font::Small, p.t3, Align::Left);
            y += 19.0;
            let h = draw::text_wrap(c, &value, rect(r.x + 20.0, y, w, 400.0), 13.0, 400, p.t1, Align::Left);
            y += h.max(18.0) + 12.0;
            if y > r.y + r.h - 20.0 {
                break;
            }
        }
    }

    /// The arrows on the sides of the stage (photos), shown when the pointer comes near.
    pub fn paint_arrows(&mut self, c: &mut Canvas, st: Rect) {
        let n = self.open.as_ref().map_or(0, |o| o.seq.len());
        if n < 2 || self.slideshow.is_some() {
            return;
        }
        let near_left = self.mouse.x >= st.x && self.mouse.x < st.x + 140.0 && self.mouse.y > st.y;
        let near_right = self.mouse.x > st.x + st.w - 140.0 && self.mouse.x <= st.x + st.w && self.mouse.y > st.y;
        let cy = st.y + st.h / 2.0;
        for (near, hit, icon, x) in [(near_left, Hit::Prev, &icons::BACK, st.x + 16.0), (near_right, Hit::Next, &icons::FORWARD, st.x + st.w - 60.0)] {
            let r = rect(x, cy - 22.0, 44.0, 44.0);
            if near || self.hovered(&hit) {
                let hot = self.hovered(&hit);
                draw::circle(c, r.x + 22.0, r.y + 22.0, 22.0, Color::hex(0x000000).fade(if hot { 0.7 } else { 0.45 }));
                c.icon(icon, rect(r.x + 13.0, r.y + 13.0, 18.0, 18.0), WHITE);
            }
            self.hits.add(r, hit);
        }
    }
}
