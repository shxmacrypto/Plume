//! "Now playing": the whole window for the song. Its cover (animated when Apple Music has a motion cover), a living
//! background (the cover's colors drifting, the cover blurred, or a GIF), the controls, and the lyrics that follow
//! the song line by line, or the queue.

use super::{
    App, Hit, Panel,
    draw::{self, ic},
    ease,
};
use crate::{
    art,
    i18n::{fill, tr},
    library::clock,
};
use plume_ui::{Align, Canvas, Color, Font, Rect, icons, rect};

impl App {
    /// The moving background of "Now playing".
    fn paint_backdrop(&mut self, c: &mut Canvas, r: Rect) {
        let base = Color::hex(0x0b0b0e);
        c.fill(r, base);
        let src = self.current().and_then(art::source);
        let pal = src.as_deref().and_then(|s| self.palette_of(s)).unwrap_or_else(|| vec![self.pal.accent, Color::hex(0x6d4aff)]);
        let t = if self.set.reduce_motion { 0.0 } else { self.t0.elapsed().as_secs_f32() };
        let mut drawn = false;
        match self.set.background.as_str() {
            "gif" => {
                drawn = self.paint_gif(c, r, 1.0);
            }
            "blur" => {
                if let Some(s) = &src
                    && let Some(path) = art::cached(s)
                {
                    let rt = c.target();
                    let id = art::key(s);
                    let rt_id = windows::core::Interface::as_raw(&rt) as usize;
                    if self.blur.as_ref().is_none_or(|(k, _, r_)| *k != id || *r_ != rt_id)
                        && let Some(src_bmp) = art::decode_file(&self.wic, &path, 256)
                        && let Ok(b) = unsafe { rt.CreateBitmapFromWicBitmap(&src_bmp, None) }
                    {
                        self.blur = Some((id, b, rt_id));
                    }
                    if let Some((_, b, _)) = &self.blur {
                        drawn = draw::blurred(&rt, b, r, 40.0);
                    }
                }
            }
            "plain" => drawn = true,
            _ => {}
        }
        if !drawn {
            // the cover's colors, drifting slowly (Apple Music's living background)
            let (w, h) = (r.w, r.h);
            let size = w.max(h) * 0.75;
            for (i, col) in pal.iter().cycle().take(5).enumerate() {
                let f = i as f32;
                let cx = r.x + w * (0.5 + 0.38 * (t * (0.05 + f * 0.013) + f * 1.9).sin());
                let cy = r.y + h * (0.5 + 0.36 * (t * (0.043 + f * 0.017) + f * 2.7).cos());
                let rad = size * (0.75 + 0.18 * (t * 0.07 + f).sin());
                draw::glow(c, cx, cy, rad, col.fade(0.62));
            }
        }
        // darker toward the bottom and the edges: white text stays readable
        // a GIF can be bright anywhere: a deeper veil
        let veil = if self.set.background == "gif" { 0.5 } else { 0.28 };
        c.fill(r, Color::hex(0).fade(veil));
        c.gradient_v(rect(r.x, r.bottom() - 260.0, r.w, 260.0), Color::hex(0).fade(0.0), Color::hex(0).fade(0.55));
    }

    pub fn paint_now(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let full = rect(0.0, 0.0, w, h);
        self.paint_backdrop(c, full);
        let white = Color::hex(0xffffff);
        let dim = white.fade(0.66);
        // close (and the tabs)
        self.icon_btn(c, &ic::COLLAPSE, rect(18.0, 14.0, 36.0, 36.0), Hit::NowClose, white);
        let Some(t) = self.current().cloned() else {
            draw::text(c, tr("Nothing is playing"), rect(0.0, h / 2.0 - 20.0, w, 40.0), 24.0, 700, white, Align::Center);
            return;
        };
        let wide = w > 1000.0;
        let side = if wide { (w * 0.42).min(560.0) } else { 0.0 };
        let left = rect(0.0, 0.0, w - side, h);
        // the cover
        let size = (left.w * 0.62).min(h - 390.0).clamp(140.0, 560.0);
        let cr = rect(left.x + (left.w - size) / 2.0, (h - size - 230.0).max(70.0) / 2.0 + 30.0, size, size);
        c.fill_round(cr.offset(0.0, 18.0).inset(10.0), 22.0, Color::hex(0).fade(0.35));
        let mut animated = false;
        if let Some((_, m)) = &self.motion
            && !self.set.reduce_motion
            && !m.frames.is_empty()
        {
            let i = ((self.t0.elapsed().as_secs_f32() * m.fps) as usize) % m.frames.len();
            let key = 0x4d00_0000_0000_0000u64 + i as u64;
            if let Some(old) = self.motion_last
                && old != key
            {
                c.forget_image(old);
            }
            self.motion_last = Some(key);
            let wic = self.wic.clone();
            let data = m.frames[i].as_slice();
            draw::image_fill(c, key, cr, 14.0, 1.0, || art::decode_bytes(&wic, data, 600));
            animated = true;
        }
        if !animated {
            let src = art::source(&t);
            self.cover(c, src.as_deref(), cr, 14.0, 640);
        }
        // title, artist, like
        let ty = cr.bottom() + 28.0;
        let tw = size.max(420.0).min(left.w - 60.0);
        let tx = left.x + (left.w - tw) / 2.0;
        draw::text(c, &t.title, rect(tx, ty, tw - 50.0, 36.0), 26.0, 800, white, Align::Left);
        self.hits.add(rect(tx, ty, tw - 50.0, 36.0), Hit::NowTitle);
        c.text(&t.artist, rect(tx, ty + 36.0, tw - 50.0, 24.0), Font::Title, dim, Align::Left);
        self.hits.add(rect(tx, ty + 36.0, tw - 50.0, 24.0), Hit::NowArtist);
        let heart = rect(tx + tw - 40.0, ty + 10.0, 40.0, 40.0);
        if t.liked {
            self.icon_fill_btn(c, &ic::HEART, heart, Hit::LikeNow, white);
        } else {
            self.icon_btn(c, &ic::HEART, heart, Hit::LikeNow, dim);
        }
        // progress
        let py = ty + 78.0;
        let (pos, dur) = self.shown_position();
        let v = if dur > 0.0 { (pos / dur) as f32 } else { 0.0 };
        self.slider(c, rect(tx, py, tw, 20.0), v, Hit::Seek, white);
        c.text(&clock(pos), rect(tx, py + 18.0, 80.0, 20.0), Font::Small, dim, Align::Left);
        c.text(&format!("-{}", clock((dur - pos).max(0.0))), rect(tx + tw - 80.0, py + 18.0, 80.0, 20.0), Font::Small, dim, Align::Right);
        // controls
        let cy = py + 52.0;
        let mid = tx + tw / 2.0;
        let on = |b: bool| if b { white } else { dim };
        self.icon_btn(c, &ic::SHUFFLE, rect(mid - 150.0, cy + 8.0, 40.0, 40.0), Hit::Shuffle, on(self.set.shuffle));
        self.icon_fill_btn(c, &ic::PREV, rect(mid - 92.0, cy + 4.0, 48.0, 48.0), Hit::Prev, white);
        let pr = rect(mid - 32.0, cy - 4.0, 64.0, 64.0);
        let hot = self.hovered(&Hit::PlayPause);
        c.fill_round(if hot { pr.inset(-2.0) } else { pr }, 32.0, white);
        let icon = if self.st.playing && self.loading.is_none() { &ic::PAUSE } else { &ic::PLAY };
        c.icon_fill(icon, rect(pr.x + 19.0, pr.y + 19.0, 26.0, 26.0), Color::hex(0x111111));
        self.hits.add(pr, Hit::PlayPause);
        self.icon_fill_btn(c, &ic::NEXT, rect(mid + 44.0, cy + 4.0, 48.0, 48.0), Hit::Next, white);
        self.icon_btn(c, &ic::REPEAT, rect(mid + 110.0, cy + 8.0, 40.0, 40.0), Hit::Repeat, on(self.set.repeat > 0));
        if self.set.repeat == 2 {
            c.text("1", rect(mid + 135.0, cy + 8.0, 12.0, 12.0), Font::SmallBold, white, Align::Center);
        }
        // volume and the lyrics/queue switch under the controls
        let vy = cy + 76.0;
        let vol = self.set.volume;
        c.icon(if vol <= 0.001 { &ic::MUTE } else { &ic::VOLUME }, rect(mid - 110.0, vy + 2.0, 16.0, 16.0), dim);
        self.hits.add(rect(mid - 116.0, vy - 4.0, 28.0, 28.0), Hit::Mute);
        self.slider(c, rect(mid - 84.0, vy, 168.0, 20.0), vol, Hit::Volume, white);
        if !wide {
            let lon = self.now_tab == Panel::Lyrics;
            self.icon_btn(c, &ic::LYRICS, rect(mid + 100.0, vy - 6.0, 32.0, 32.0), Hit::LyricsBtn, on(lon));
        }
        // lyrics or queue on the side
        if wide {
            let sr = rect(w - side, 60.0, side - 30.0, h - 90.0);
            let tabs = [(Panel::Lyrics, tr("Lyrics")), (Panel::Queue, tr("Queue"))];
            let mut x = sr.x;
            for (tab, label) in tabs {
                let tw = c.text_width(label, Font::BodyBold) + 30.0;
                let r = rect(x, 14.0, tw, 34.0);
                let active = self.now_tab == tab;
                c.fill_round(r, 17.0, if active { white } else { white.fade(0.14) });
                c.text(label, r, Font::BodyBold, if active { Color::hex(0x111111) } else { white }, Align::Center);
                self.hits.add(r, Hit::NowTab(tab));
                x += tw + 8.0;
            }
            match self.now_tab {
                Panel::Lyrics => self.paint_lyrics(c, sr, true),
                Panel::Queue => self.paint_queue(c, sr, true),
            }
        }
        let _ = (fill, icons::MORE);
    }

    /// The lyrics: the line sung now in full light, the others dimmed, scrolling smoothly with the song.
    pub fn paint_lyrics(&mut self, c: &mut Canvas, r: Rect, big: bool) {
        let p = self.pal.clone();
        let (ink, dim) = if big { (Color::hex(0xffffff), Color::hex(0xffffff).fade(0.38)) } else { (p.t1, p.t3) };
        let key = self.current().map(|t| t.key.clone());
        let lyr = match (&self.lyrics, &key) {
            (Some((k, l)), Some(cur)) if k == cur => l.clone(),
            _ => {
                if key.is_some() {
                    let text = if self.lyrics_loading.is_some() { tr("Looking for lyrics…") } else { tr("No lyrics for this song.") };
                    c.text(text, rect(r.x + 20.0, r.y + 20.0, r.w - 40.0, 30.0), Font::Body, dim, Align::Left);
                }
                return;
            }
        };
        let Some(l) = lyr else {
            c.text(tr("No lyrics for this song."), rect(r.x + 20.0, r.y + 20.0, r.w - 40.0, 30.0), Font::Body, dim, Align::Left);
            return;
        };
        let size = [if big { 26.0 } else { 18.0 }, if big { 32.0 } else { 21.0 }, if big { 40.0 } else { 25.0 }][self.set.lyrics_size.min(2) as usize];
        // room for the source line at the bottom of the side panel
        let r = if big { r } else { rect(r.x, r.y, r.w, r.h - 34.0) };
        let pos = self.st.pos + 0.25; // a little ahead: the line lights up as it is sung
        let current = l.current(pos);
        let x = r.x + 20.0;
        let w = r.w - 40.0;
        // heights of the lines (cached by layout per frame: lyrics are short)
        let gap = size * 0.55;
        let mut ys = Vec::with_capacity(l.lines.len());
        let mut y = 0.0;
        for (_, line) in &l.lines {
            ys.push(y);
            let text = if line.is_empty() { "♪" } else { line.as_str() };
            y += draw::text_wrap(c, text, rect(x, 0.0, w, 0.0), size, 800, ink, Align::Left, false) + gap;
        }
        let total = y;
        // the current line a third of the way down
        let target = match current {
            Some(i) if l.synced => (ys[i] - r.h * 0.33).max(0.0),
            _ => self.lyric_y.min((total - r.h * 0.6).max(0.0)),
        };
        if l.synced {
            let k = if self.set.reduce_motion { 1.0 } else { ease(0.16) };
            self.lyric_y += (target - self.lyric_y) * k;
        }
        c.clip(r);
        for (i, (_, line)) in l.lines.iter().enumerate() {
            let ly = r.y + 10.0 + ys[i] - self.lyric_y;
            let lh = ys.get(i + 1).copied().unwrap_or(total) - ys[i];
            if ly + lh < r.y || ly > r.bottom() {
                continue;
            }
            let active = current == Some(i) || !l.synced;
            let past = current.is_some_and(|c_| i < c_);
            let col = if active {
                ink
            } else if past {
                dim.fade(0.8)
            } else {
                dim
            };
            let text = if line.is_empty() { "♪" } else { line.as_str() };
            draw::text_wrap(c, text, rect(x, ly, w, lh), size, 800, col, Align::Left, true);
            if l.synced {
                self.hits.add(rect(x, ly, w, lh - gap * 0.5), Hit::LyricLine(i));
            }
        }
        c.unclip();
        if !l.source.is_empty() && !big {
            c.text(&fill(tr("Lyrics from {s}"), &[&l.source]), rect(r.x + 20.0, r.bottom() + 8.0, r.w - 40.0, 20.0), Font::Small, p.t3, Align::Left);
        }
    }

    pub fn paint_queue(&mut self, c: &mut Canvas, r: Rect, big: bool) {
        let p = self.pal.clone();
        let (ink, dim) = if big { (Color::hex(0xffffff), Color::hex(0xffffff).fade(0.6)) } else { (p.t1, p.t3) };
        if self.queue.is_empty() {
            c.text(tr("The queue is empty."), rect(r.x + 20.0, r.y + 20.0, r.w - 40.0, 30.0), Font::Body, dim, Align::Left);
            return;
        }
        c.clip(r);
        let mut y = r.y + 4.0;
        c.text(tr("Now playing"), rect(r.x + 20.0, y, r.w - 120.0, 24.0), Font::SmallBold, dim, Align::Left);
        let clear = rect(r.right() - 110.0, y - 2.0, 96.0, 26.0);
        c.text(tr("Clear queue"), clear, Font::Small, if self.hovered(&Hit::QueueClear) { ink } else { dim }, Align::Right);
        self.hits.add(clear, Hit::QueueClear);
        y += 30.0;
        let keys = self.queue.clone();
        let start = self.qpos;
        let mut first = true;
        for (i, k) in keys.iter().enumerate().skip(start) {
            if y > r.bottom() {
                break;
            }
            let Some(t) = self.lib.tracks.get(k).cloned() else { continue };
            let row = rect(r.x + 10.0, y, r.w - 20.0, 56.0);
            let hit = Hit::QueueRow(i);
            if self.hovered(&hit) || self.hovered(&Hit::QueueRemove(i)) {
                c.fill_round(row, 8.0, ink.fade(0.08));
            }
            let src = art::source(&t);
            self.cover(c, src.as_deref(), rect(row.x + 8.0, row.y + 8.0, 40.0, 40.0), 5.0, 96);
            let current = i == self.qpos;
            c.text(
                &t.title,
                rect(row.x + 60.0, row.y + 8.0, row.w - 110.0, 22.0),
                Font::BodyBold,
                if current { p.accent.mix(Color::hex(0xffffff), if big { 0.4 } else { 0.0 }) } else { ink },
                Align::Left,
            );
            c.text(&t.artist, rect(row.x + 60.0, row.y + 29.0, row.w - 110.0, 20.0), Font::Small, dim, Align::Left);
            self.hits.add(row, hit);
            if !current && (self.hovered(&Hit::QueueRow(i)) || self.hovered(&Hit::QueueRemove(i))) {
                self.icon_btn(c, &icons::CLOSE, rect(row.right() - 38.0, row.y + 14.0, 28.0, 28.0), Hit::QueueRemove(i), dim);
            }
            y += 58.0;
            if first {
                first = false;
                y += 8.0;
                c.text(tr("Next up"), rect(r.x + 20.0, y, r.w - 40.0, 24.0), Font::SmallBold, dim, Align::Left);
                y += 30.0;
            }
        }
        c.unclip();
    }
}
