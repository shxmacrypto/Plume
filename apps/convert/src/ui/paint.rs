//! The main page: title bar, presets and their choices (left), drop zone, queue and the Convert bar (right).

use super::{App, BOTTOM_H, Hit, Item, LEFT_W, ROW_H, State, TOP_H, Tools, View, draw, draw::ic};
use crate::{
    ffmpeg,
    i18n::{fill, tr},
    naming::size_text,
    opts::{Preset, clock},
};
use plume_ui::{Align, Canvas, Color, EditStyle, Font, PaintCx, Rect, icons, icons::Icon, m, rect};

/// Segmented controls: the space around a label inside its segment, and between the track and the segments.
const SEG_PAD: f32 = 24.0;
const SEG_INSET: f32 = 3.0;

pub fn preset_icon(p: Preset) -> &'static Icon {
    match p {
        Preset::Phone => &ic::PHONE,
        Preset::Discord => &ic::CHAT,
        Preset::Smaller => &ic::SHRINK,
        Preset::Audio => &ic::NOTE,
        Preset::Gif => &ic::GIF,
        Preset::Images => &ic::IMAGE,
        Preset::Frames => &ic::FRAMES,
        Preset::Trim => &ic::SCISSORS,
        Preset::Mute => &ic::MUTE,
        Preset::Rotate => &ic::ROTATE,
        Preset::Custom => &ic::SLIDERS,
    }
}

fn kind_of(p: &std::path::Path) -> &'static Icon {
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ffmpeg::is_picture(p) {
        &ic::IMAGE
    } else if matches!(ext.as_str(), "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "oga" | "opus" | "wma" | "aiff" | "aif" | "alac" | "amr") {
        &ic::NOTE
    } else if ext == "gif" {
        &ic::GIF
    } else {
        &ic::VIDEO
    }
}

impl App {
    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, primary: bool, hit: Hit, enabled: bool) {
        let p = self.pal.clone();
        let hot = enabled && self.hovered(&hit);
        let down = hot && self.pressed.as_ref() == Some(&hit);
        let fill = if primary && enabled {
            if down {
                p.accent.mix(Color::hex(0x000000), 0.1)
            } else if hot {
                p.accent.mix(Color::hex(0xffffff), 0.12)
            } else {
                p.accent
            }
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

    pub fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, color: Color) {
        let hot = self.hovered(&hit);
        if hot {
            c.fill_round(r, r.h / 2.0, self.pal.hover);
        }
        let s = (r.w * 0.5).round();
        c.icon(icon, rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s), if hot { self.pal.t1 } else { color });
        self.hits.add(r, hit);
    }

    /// A segmented control under a small label, as wide as `w`. Returns the y below it.
    pub fn chip_row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, key: &'static str) -> f32 {
        let p = self.pal.clone();
        let mut y = y;
        if !label.is_empty() {
            draw::text(c, label, rect(x, y, w, 18.0), 12.0, 600, p.t2, Align::Left);
            y += 24.0;
        }
        let r = self.segmented(c, x, y, Some(w), key);
        r.bottom() + 16.0
    }

    /// The natural width of a segmented control (each choice as wide as its label).
    pub fn segmented_width(&self, c: &mut Canvas, key: &str) -> f32 {
        let (labels, _) = self.chips_of(key);
        labels.iter().map(|l| draw::text_width(c, l, 12.0, 600) + SEG_PAD).sum::<f32>() + 2.0 * SEG_INSET
    }

    /// A segmented control: a pill track holding the choices, the chosen one raised (like the suite's settings). With `w`, the
    /// choices share that width (on two lines or more when their labels do not fit); without, each is as wide as
    /// its label.
    pub fn segmented(&mut self, c: &mut Canvas, x: f32, y: f32, w: Option<f32>, key: &'static str) -> Rect {
        let p = self.pal.clone();
        let (labels, cur) = self.chips_of(key);
        let n = labels.len().max(1);
        let nat: Vec<f32> = labels.iter().map(|l| draw::text_width(c, l, 12.0, 600) + SEG_PAD).collect();
        let (sh, gap) = (28.0, 2.0);
        let equal = w.is_some();
        let (w, per) = match w {
            None => (nat.iter().sum::<f32>() + 2.0 * SEG_INSET, n),
            Some(w) => {
                let widest = nat.iter().copied().fold(0.0, f32::max);
                let mut per = n;
                while per > 1 && widest > (w - 2.0 * SEG_INSET) / per as f32 {
                    per -= 1;
                }
                (w, n.div_ceil(n.div_ceil(per)))
            }
        };
        let lines = n.div_ceil(per);
        let h = lines as f32 * sh + (lines - 1) as f32 * gap + 2.0 * SEG_INSET;
        let track = rect(x, y, w, h);
        c.fill_round(track, if lines == 1 { h / 2.0 } else { 17.0 }, p.fill);
        let mut sx = x + SEG_INSET;
        for (i, l) in labels.iter().enumerate() {
            let (line, col) = (i / per, i % per);
            if col == 0 {
                sx = x + SEG_INSET;
            }
            // one column width for every line, so a wrapped line sits under the first
            let sw = if equal { (w - 2.0 * SEG_INSET) / per as f32 } else { nat[i] };
            let r = rect(sx, y + SEG_INSET + line as f32 * (sh + gap), sw, sh);
            let hit = Hit::Chip(key, i);
            let on = i == cur;
            if on {
                c.fill_round(r, sh / 2.0, if p.dark { p.solid2 } else { p.solid });
                c.stroke_round(r, sh / 2.0, p.ring, 1.0);
            } else if self.hovered(&hit) {
                c.fill_round(r, sh / 2.0, p.hover);
            }
            draw::text(c, l, r, 12.0, if on { 600 } else { 400 }, if on { p.t1 } else { p.t2 }, Align::Center);
            self.hits.add(r, hit);
            sx += sw;
        }
        track
    }

    /// A text field drawn by the app (the real edit control is placed on it when clicked).
    fn field(&mut self, c: &mut Canvas, r: Rect, key: &'static str, text: &str, placeholder: &str, fields: &mut Vec<(&'static str, Rect)>) {
        let p = self.pal.clone();
        let on = self.edit_for == Some(key);
        c.fill_round(r, 10.0, if on { p.fill2 } else { p.fill });
        if on {
            c.stroke_round(r, 10.0, p.accent, 1.5);
        }
        let inner = rect(r.x + 12.0, r.y, r.w - 24.0, r.h);
        if text.is_empty() {
            draw::text(c, placeholder, inner, 13.0, 400, p.t3, Align::Left);
        } else if !on {
            draw::text(c, text, inner, 13.0, 400, p.t1, Align::Left);
        }
        self.hits.add(r, Hit::Field(key));
        fields.push((key, r));
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let p = self.pal.clone();
        self.hits.clear();
        c.clear(p.bg);
        let (w, h) = (cx.w, cx.h);
        let mut fields = Vec::new();
        match self.view {
            View::Main => {
                self.paint_right(c, rect(LEFT_W, TOP_H, w - LEFT_W, h - TOP_H));
                self.paint_left(c, rect(0.0, TOP_H, LEFT_W, h - TOP_H), &mut fields);
            }
            View::Settings => self.paint_settings(c, rect(0.0, TOP_H, w, h - TOP_H)),
        }
        self.paint_top(c, w);
        self.paint_caption(c, w, cx);
        self.paint_toast(c, w, h);
        self.place_edit(&fields);
    }

    fn paint_top(&mut self, c: &mut Canvas, w: f32) {
        let p = self.pal.clone();
        if self.view == View::Main {
            c.fill(rect(0.0, 0.0, LEFT_W, TOP_H), p.side);
        }
        let logo = rect(20.0, (TOP_H - 18.0) / 2.0, 18.0, 18.0);
        let wic = self.wic.clone();
        c.image(7, logo, 1.0, || crate::image::decode_bytes(&wic, crate::LOGO, 64));
        draw::text(c, "Plume Convert", rect(46.0, 0.0, 240.0, TOP_H), 13.0, 600, p.t1, Align::Left);
        let gx = w - 3.0 * m::CAPTION_W - 50.0;
        let hit = if self.view == View::Settings { Hit::Back } else { Hit::Gear };
        let icon = if self.view == View::Settings { &icons::CLOSE } else { &icons::GEAR };
        self.icon_btn(c, icon, rect(gx, 10.0, 36.0, 36.0), hit, p.t2);
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

    // ───────────────────────── left: presets ─────────────────────────

    fn paint_left(&mut self, c: &mut Canvas, r: Rect, fields: &mut Vec<(&'static str, Rect)>) {
        let p = self.pal.clone();
        c.fill(r, p.side);
        c.clip(r);
        let pad = 18.0;
        let (x, w) = (r.x + pad, r.w - 2.0 * pad);
        let top = r.y + 6.0 - self.left_scroll;
        let mut y = top;
        draw::text(c, tr("Presets"), rect(x, y, w, 20.0), 12.0, 600, p.t3, Align::Left);
        y += 26.0;
        let gap = 8.0;
        let tw = (w - gap) / 2.0;
        let th = 60.0;
        for (i, preset) in Preset::ALL.into_iter().enumerate() {
            let tr_ = rect(x + (i % 2) as f32 * (tw + gap), y + (i / 2) as f32 * (th + gap), tw, th);
            let on = self.set.preset == preset;
            let hit = Hit::Preset(preset);
            let hot = self.hovered(&hit);
            let bg = if on {
                p.accent.fade(0.16).over(p.side)
            } else if hot {
                p.hover.over(p.card.over(p.side))
            } else {
                p.card
            };
            c.fill_round(tr_, 14.0, bg);
            c.stroke_round(tr_.inset(0.5), 13.5, if on { p.accent.fade(0.3) } else { p.line }, 1.0);
            // the accent only on the chosen one; the others stay quiet
            let badge = rect(tr_.x + 10.0, tr_.y + 14.0, 32.0, 32.0);
            c.fill_round(badge, 9.0, if on { p.accent } else { p.fill });
            c.icon(preset_icon(preset), badge.inset(8.0), if on { p.on_accent } else { p.t2 });
            let tx = badge.right() + 10.0;
            draw::text(c, tr(preset.name()), rect(tx, tr_.y + 11.0, tr_.right() - tx - 8.0, 20.0), 13.0, 600, p.t1, Align::Left);
            draw::text(c, tr(preset.sub()), rect(tx, tr_.y + 31.0, tr_.right() - tx - 8.0, 18.0), 12.0, 400, p.t3, Align::Left);
            self.hits.add(tr_, hit);
        }
        y += (Preset::ALL.len().div_ceil(2)) as f32 * (th + gap) + 14.0;
        c.hline(x, y, w, p.line);
        y += 18.0;
        y = self.paint_options(c, x, y, w, fields);
        // the settings that matter for every video
        let label = if self.set.out_dir.is_some() { tr("Saved in the chosen folder") } else { tr("Saved next to the original") };
        draw::text(c, label, rect(x, y, w, 18.0), 12.0, 400, p.t3, Align::Left);
        y += 30.0;
        self.left_h = (y - top) - r.h;
        c.unclip();
        if self.left_h > 0.0 {
            self.left_scroll = self.left_scroll.min(self.left_h);
            let k = r.h / (r.h + self.left_h);
            let bar = rect(r.right() - 6.0, r.y + self.left_scroll * k + 4.0, 3.0, r.h * k - 8.0);
            c.fill_round(bar, 1.5, p.t4);
        } else {
            self.left_scroll = 0.0;
        }
    }

    /// The choices of the chosen preset.
    fn paint_options(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, fields: &mut Vec<(&'static str, Rect)>) -> f32 {
        let p = self.pal.clone();
        let preset = self.set.preset;
        let mut y = y;
        draw::text(c, tr(preset.name()), rect(x, y, w, 24.0), 17.0, 600, p.t1, Align::Left);
        y += 30.0;
        let line = |c: &mut Canvas, y: &mut f32, s: &str| {
            let h = draw::text_wrap(c, s, rect(x, *y, w, 100.0), 12.0, 400, p.t2, Align::Left);
            *y += h + 14.0;
        };
        match preset {
            Preset::Phone => line(c, &mut y, tr("MP4 with H.264 and AAC, at most 1080p: plays on every phone, TV and browser.")),
            Preset::Discord => {
                line(c, &mut y, tr("Two passes find the quality that fills the size without going over."));
                y = self.chip_row(c, x, y, w, tr("Stay under"), "discord");
            }
            Preset::Smaller => line(c, &mut y, tr("H.264 at a lower quality and at most 1080p. Often a third of the size.")),
            Preset::Audio => y = self.chip_row(c, x, y, w, tr("Format"), "audio_fmt"),
            Preset::Gif => {
                y = self.chip_row(c, x, y, w, tr("Longest side"), "gif_width");
                let head = |c: &mut Canvas, y: &mut f32, s: &str| {
                    draw::text(c, s, rect(x, *y + 4.0, w, 20.0), 13.0, 600, p.t1, Align::Left);
                    *y += 30.0;
                };
                head(c, &mut y, tr("From a video"));
                y = self.chip_row(c, x, y, w, tr("Frames per second"), "gif_fps");
                line(c, &mut y, tr("Trim the video first: GIFs grow fast."));
                head(c, &mut y, tr("From photos"));
                y = self.chip_row(c, x, y, w, tr("Each photo stays"), "gif_secs");
                y = self.chip_row(c, x, y, w, tr("Several photos"), "gif_together");
                y = self.chip_row(c, x, y, w, tr("Plays"), "gif_loop");
                line(c, &mut y, tr("One photo makes a still GIF. Several play one after the other, in name order."));
            }
            Preset::Images => {
                y = self.chip_row(c, x, y, w, tr("Format"), "img_fmt");
                let f = self.set.opts.img_fmt.clone();
                if f == "jpeg" || f == "webp" {
                    y = self.chip_row(c, x, y, w, tr("Quality"), "img_quality");
                }
                y = self.chip_row(c, x, y, w, tr("Longest side"), "img_max");
            }
            Preset::Frames => {
                y = self.chip_row(c, x, y, w, tr("Take"), "frames_every");
                y = self.chip_row(c, x, y, w, tr("As"), "frames_fmt");
                line(c, &mut y, tr("The pictures go in a new folder next to the video."));
            }
            Preset::Trim => {
                let half = (w - 10.0) / 2.0;
                draw::text(c, tr("Start"), rect(x, y, half, 18.0), 12.0, 600, p.t2, Align::Left);
                draw::text(c, tr("End"), rect(x + half + 10.0, y, half, 18.0), 12.0, 600, p.t2, Align::Left);
                y += 24.0;
                let (a, b) = self.trim_text.clone();
                self.field(c, rect(x, y, half, 36.0), "trim_start", &a, "0:00", fields);
                self.field(c, rect(x + half + 10.0, y, half, 36.0), "trim_end", &b, tr("The end"), fields);
                y += 46.0;
                let o = &self.set.opts;
                let span = if o.trim_end > o.trim_start {
                    fill(tr("Keeps {t}"), &[&clock(o.trim_end - o.trim_start, true)])
                } else {
                    tr("Keeps everything after the start").to_string()
                };
                line(c, &mut y, &span);
                y = self.chip_row(c, x, y, w, tr("Cut"), "trim_mode");
            }
            Preset::Mute => line(c, &mut y, tr("Same picture, copied as it is, without the sound. Takes a second.")),
            Preset::Rotate => y = self.chip_row(c, x, y, w, tr("Turn"), "rotate"),
            Preset::Custom => {
                y = self.chip_row(c, x, y, w, tr("Container"), "container");
                y = self.chip_row(c, x, y, w, tr("Video codec"), "vcodec");
                y = self.chip_row(c, x, y, w, tr("Audio codec"), "acodec");
                y = self.chip_row(c, x, y, w, tr("Resolution"), "height");
                y = self.chip_row(c, x, y, w, tr("Frame rate"), "fps");
                y = self.chip_row(c, x, y, w, tr("Video size by"), "rate_mode");
                if self.set.opts.vbitrate > 0 {
                    y = self.chip_row(c, x, y, w, tr("Video bitrate"), "vbitrate");
                } else {
                    y = self.chip_row(c, x, y, w, tr("Quality"), "crf");
                }
                y = self.chip_row(c, x, y, w, tr("Audio bitrate"), "abitrate");
                y = self.chip_row(c, x, y, w, tr("Sample rate"), "sample_rate");
                // hardware encoding, also in the settings
                let r = rect(x, y, w, 40.0);
                draw::text(c, tr("Graphics card encoding"), rect(x, y, w - 60.0, 20.0), 13.0, 600, p.t1, Align::Left);
                let sub = match &self.hw_label {
                    Some(l) if !l.is_empty() => l.clone(),
                    Some(_) => tr("None found: the processor does it").to_string(),
                    None => tr("H.264, H.265 and AV1, when the PC has one").to_string(),
                };
                draw::text(c, &sub, rect(x, y + 20.0, w - 60.0, 18.0), 12.0, 400, p.t3, Align::Left);
                let on = self.set.hw;
                self.switch(c, r, on, "hw");
                y += 56.0;
            }
        }
        y
    }

    pub fn switch(&mut self, c: &mut Canvas, r: Rect, on: bool, name: &'static str) {
        let p = self.pal.clone();
        let sw = rect(r.right() - 48.0, r.y + (r.h - 26.0) / 2.0, 48.0, 26.0);
        c.fill_round(sw, 13.0, if on { p.accent } else { p.fill2 });
        let k = if on { sw.right() - 23.0 } else { sw.x + 3.0 };
        c.fill_round(rect(k, sw.y + 3.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
        self.hits.add(r, Hit::Toggle(name));
    }

    // ───────────────────────── right: drop zone, queue, Convert ─────────────────────────

    fn paint_right(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        let pad = 24.0;
        let x = r.x + pad;
        let w = r.w - 2.0 * pad;
        let empty = self.items.is_empty();
        let bottom = rect(r.x, r.bottom() - BOTTOM_H, r.w, BOTTOM_H);
        let dz_h = if empty { (bottom.y - r.y - 2.0 * pad).max(160.0) } else { 96.0 };
        let dz = rect(x, r.y + 12.0, w, dz_h);
        self.paint_drop(c, dz, empty);
        if !empty {
            let list = rect(x, dz.bottom() + 18.0, w, bottom.y - dz.bottom() - 30.0);
            self.paint_queue(c, list);
        }
        self.paint_bottom(c, rect(x, bottom.y, w, BOTTOM_H));
        let _ = p;
    }

    fn paint_drop(&mut self, c: &mut Canvas, r: Rect, big: bool) {
        let p = self.pal.clone();
        let hot = self.hovered(&Hit::DropZone);
        c.fill_round(r, 18.0, if hot { p.accent.fade(0.08) } else { p.card });
        // a dashed edge
        let dash: f32 = 10.0;
        let ink = if hot { p.accent } else { p.t4 };
        let mut xx = r.x + 18.0;
        while xx < r.right() - 18.0 {
            let len = dash.min(r.right() - 18.0 - xx);
            c.fill(rect(xx, r.y, len, 1.5), ink);
            c.fill(rect(xx, r.bottom() - 1.5, len, 1.5), ink);
            xx += dash * 1.8;
        }
        let mut yy = r.y + 18.0;
        while yy < r.bottom() - 18.0 {
            let len = dash.min(r.bottom() - 18.0 - yy);
            c.fill(rect(r.x, yy, 1.5, len), ink);
            c.fill(rect(r.right() - 1.5, yy, 1.5, len), ink);
            yy += dash * 1.8;
        }
        self.hits.add(r, Hit::DropZone);
        let add = tr("Add files");
        let bw = draw::text_width(c, add, 13.0, 600) + 36.0;
        if big {
            let cy = r.y + r.h / 2.0;
            let badge = rect(r.x + (r.w - 72.0) / 2.0, cy - 110.0, 72.0, 72.0);
            c.fill_round(badge, 20.0, p.fill);
            c.icon(&ic::DROP, badge.inset(20.0), if hot { p.accent } else { p.t2 });
            draw::text(c, tr("Drop files here"), rect(r.x, cy - 24.0, r.w, 32.0), 24.0, 700, p.t1, Align::Center);
            draw::text(
                c,
                tr("Videos, sounds and pictures. Nothing leaves this PC."),
                rect(r.x + 20.0, cy + 10.0, r.w - 40.0, 22.0),
                13.0,
                400,
                p.t2,
                Align::Center,
            );
            self.pill(c, rect(r.x + (r.w - bw) / 2.0, cy + 46.0, bw, 38.0), add, true, Hit::AddFiles, true);
        } else {
            let badge = rect(r.x + 22.0, r.y + (r.h - 48.0) / 2.0, 48.0, 48.0);
            c.fill_round(badge, 14.0, p.fill);
            c.icon(&ic::DROP, badge.inset(13.0), if hot { p.accent } else { p.t2 });
            draw::text(c, tr("Drop more files here"), rect(badge.right() + 16.0, r.y + 24.0, r.w - 260.0, 24.0), 15.0, 600, p.t1, Align::Left);
            draw::text(c, tr("or click to choose them"), rect(badge.right() + 16.0, r.y + 48.0, r.w - 260.0, 20.0), 12.0, 400, p.t3, Align::Left);
            self.pill(c, rect(r.right() - bw - 24.0, r.y + (r.h - 36.0) / 2.0, bw, 36.0), add, false, Hit::AddFiles, true);
        }
    }

    fn state_line(&self, it: &Item) -> (String, Color) {
        let p = &self.pal;
        match &it.state {
            State::Ready => (format!("{} · {}", size_text(it.size), tr("Ready")), p.t3),
            State::Waiting if matches!(self.tools, Tools::Installing(_)) => (tr("Waiting for FFmpeg").to_string(), p.t3),
            State::Waiting => (tr("Waiting").to_string(), p.t3),
            State::Running => {
                let pct = (it.frac * 100.0).floor() as u32;
                let mut s = format!("{pct} %");
                if let Some(left) = it.started.and_then(|t| ffmpeg::time_left(t.elapsed().as_secs_f64(), it.frac)) {
                    s.push_str(" · ");
                    s.push_str(&fill(tr("{t} left"), &[&clock(left.ceil(), false)]));
                }
                if !it.note.is_empty() {
                    s.push_str(" · ");
                    s.push_str(tr_dyn(&it.note));
                }
                (s, p.t2)
            }
            State::Done => {
                let name = it.out.as_ref().and_then(|o| o.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let size = if it.out.as_ref().is_some_and(|o| o.is_dir()) {
                    fill(tr("{n} pictures"), &[&it.out.as_ref().map(|o| crate::queue::count_files(o)).unwrap_or(0).to_string()])
                } else {
                    size_text(it.out_size)
                };
                (format!("{} · {name} · {size}", tr("Done")), p.green)
            }
            State::Failed(e) => (crate::i18n::colon(tr("Failed"), tr_dyn(e)), p.red),
            State::Cancelled => (tr("Cancelled").to_string(), p.t3),
        }
    }

    fn paint_queue(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        let n = self.items.len();
        let done = self.items.iter().filter(|i| matches!(i.state, State::Done | State::Failed(_) | State::Cancelled)).count();
        draw::text(c, &fill(tr("Queue · {n}"), &[&n.to_string()]), rect(r.x, r.y, 300.0, 22.0), 15.0, 600, p.t1, Align::Left);
        if done > 0 {
            let l = tr("Clear finished");
            let bw = draw::text_width(c, l, 13.0, 600) + 28.0;
            self.pill(c, rect(r.right() - bw, r.y - 4.0, bw, 30.0), l, false, Hit::ClearDone, true);
        }
        let list = rect(r.x, r.y + 34.0, r.w, r.h - 34.0);
        let total = n as f32 * (ROW_H + 8.0);
        self.queue_h = total - list.h;
        if self.queue_h <= 0.0 {
            self.queue_scroll = 0.0;
        }
        self.queue_scroll = self.queue_scroll.clamp(0.0, self.queue_h.max(0.0));
        c.clip(list);
        let items = self.items.clone();
        for (i, it) in items.iter().enumerate() {
            let row = rect(list.x, list.y + i as f32 * (ROW_H + 8.0) - self.queue_scroll, list.w - 8.0, ROW_H);
            if row.bottom() < list.y || row.y > list.bottom() {
                continue;
            }
            self.paint_item(c, row, it);
        }
        c.unclip();
        if self.queue_h > 0.0 {
            let k = list.h / total;
            let bar = rect(list.right() - 3.0, list.y + self.queue_scroll * k, 3.0, list.h * k);
            c.fill_round(bar, 1.5, p.t4);
        }
    }

    fn paint_item(&mut self, c: &mut Canvas, row: Rect, it: &Item) {
        let p = self.pal.clone();
        c.fill_round(row, 16.0, p.card);
        c.stroke_round(row.inset(0.5), 15.5, p.line, 1.0);
        let icon = kind_of(&it.src);
        let badge = rect(row.x + 14.0, row.y + 14.0, 40.0, 40.0);
        c.fill_round(badge, 11.0, p.fill);
        c.icon(icon, badge.inset(11.0), p.t2);
        // buttons on the right
        let mut bx = row.right() - 12.0;
        let mut btn = |app: &mut App, c: &mut Canvas, icon: &Icon, hit: Hit, color: Color| {
            bx -= 34.0;
            app.icon_btn(c, icon, rect(bx, row.y + (row.h - 34.0) / 2.0, 34.0, 34.0), hit, color);
        };
        match it.state {
            State::Running | State::Waiting => btn(self, c, &icons::CLOSE, Hit::ItemCancel(it.id), p.t2),
            State::Done => {
                btn(self, c, &icons::CLOSE, Hit::ItemRemove(it.id), p.t3);
                btn(self, c, &ic::OPEN, Hit::ItemOpen(it.id), p.t1);
            }
            State::Failed(_) | State::Cancelled => {
                btn(self, c, &icons::CLOSE, Hit::ItemRemove(it.id), p.t3);
                if it.spec.is_some() {
                    btn(self, c, &icons::RELOAD, Hit::ItemRetry(it.id), p.t1);
                }
            }
            State::Ready => btn(self, c, &icons::CLOSE, Hit::ItemRemove(it.id), p.t3),
        }
        // the preset it was given
        if let Some(spec) = &it.spec {
            let tag = tr(spec.preset.name());
            let tw = draw::text_width(c, tag, 12.0, 600) + 20.0;
            bx -= tw + 10.0;
            let t = rect(bx, row.y + (row.h - 24.0) / 2.0, tw, 24.0);
            c.fill_round(t, 12.0, p.fill);
            draw::text(c, tag, t, 12.0, 600, p.t2, Align::Center);
        }
        let tx = badge.right() + 14.0;
        let tw = bx - tx - 12.0;
        let name = match &it.spec {
            Some(s) if !s.extra.is_empty() => fill(tr("Slideshow of {n} photos"), &[&(s.extra.len() + 1).to_string()]),
            _ => it.src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        };
        draw::text(c, &name, rect(tx, row.y + 12.0, tw, 22.0), 13.0, 600, p.t1, Align::Left);
        let (line, color) = self.state_line(it);
        draw::text(c, &line, rect(tx, row.y + 34.0, tw, 18.0), 12.0, 400, color, Align::Left);
        if it.state == State::Running {
            let bar = rect(tx, row.bottom() - 11.0, tw, 4.0);
            c.fill_round(bar, 2.0, p.fill);
            c.fill_round(rect(bar.x, bar.y, (bar.w * it.frac as f32).max(4.0), 4.0), 2.0, p.accent);
        }
    }

    fn paint_bottom(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.hline(r.x, r.y, r.w, p.line);
        let cy = r.y + r.h / 2.0;
        draw::text(c, tr("Save to"), rect(r.x, cy - 10.0, 70.0, 20.0), 12.0, 600, p.t3, Align::Left);
        let dest = match &self.set.out_dir {
            Some(d) => d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| d.display().to_string()),
            None => tr("Next to the original").to_string(),
        };
        let dw = (draw::text_width(c, &dest, 13.0, 600) + 50.0).min(r.w * 0.4);
        let d = rect(r.x + 64.0, cy - 17.0, dw, 34.0);
        self.pill(c, d, "", false, Hit::OutFolder, true);
        c.icon(&ic::OPEN, rect(d.x + 13.0, d.y + 10.0, 14.0, 14.0), p.t2);
        draw::text(c, &dest, rect(d.x + 34.0, d.y, d.w - 44.0, d.h), 13.0, 600, p.t1, Align::Left);
        if self.set.out_dir.is_some() {
            self.icon_btn(c, &icons::CLOSE, rect(d.right() + 4.0, cy - 15.0, 30.0, 30.0), Hit::OutReset, p.t3);
        }
        let ready = self.items.iter().filter(|i| i.state == State::Ready).count();
        let label = if ready > 1 { fill(tr("Convert {n} files"), &[&ready.to_string()]) } else { tr("Convert").to_string() };
        let bw = draw::text_width(c, &label, 13.0, 600) + 64.0;
        let b = rect(r.right() - bw, cy - 21.0, bw, 42.0);
        let enabled = ready > 0 || self.items.is_empty();
        self.pill(c, b, "", true, Hit::Convert, enabled);
        let ink = if enabled { p.on_accent } else { p.t3 };
        c.icon(&ic::CONVERT, rect(b.x + 20.0, b.y + 13.0, 16.0, 16.0), ink);
        draw::text(c, &label, rect(b.x + 42.0, b.y, b.w - 58.0, b.h), 13.0, 600, ink, Align::Left);
        if self.busy() {
            let l = tr("Cancel all");
            let cw = draw::text_width(c, l, 13.0, 600) + 32.0;
            self.pill(c, rect(b.x - cw - 10.0, cy - 18.0, cw, 36.0), l, false, Hit::CancelAll, true);
        }
        if let Tools::Installing(f) = self.tools {
            let s = fill(tr("Downloading FFmpeg… {n} %"), &[&((f * 100.0) as u32).to_string()]);
            draw::text(c, &s, rect(d.right() + 40.0, cy - 10.0, 260.0, 20.0), 12.0, 400, p.t2, Align::Left);
        }
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((text, _)) = self.toast.clone() else { return };
        let p = self.pal.clone();
        let tw = (draw::text_width(c, &text, 13.0, 600) + 40.0).min(w - 40.0);
        let r = rect((w - tw) / 2.0, h - BOTTOM_H - 60.0, tw, 40.0);
        c.fill_round(r, 20.0, p.solid2);
        c.stroke_round(r, 20.0, p.ring, 1.0);
        draw::text(c, &text, r, 13.0, 600, p.t1, Align::Center);
        self.hits.add(r, Hit::Toast);
    }

    /// The window's single edit control, over the field being typed in.
    fn place_edit(&mut self, fields: &[(&'static str, Rect)]) {
        let p = self.pal.clone();
        let want = self.edit_for.and_then(|k| fields.iter().find(|(f, _)| *f == k).map(|(_, r)| (k, *r)));
        match want {
            Some((k, r)) => {
                let inner = rect(r.x + 10.0, r.y + 7.0, r.w - 20.0, r.h - 14.0);
                if self.edit_shown != Some(k) {
                    let text = if k == "trim_start" { self.trim_text.0.clone() } else { self.trim_text.1.clone() };
                    let style = EditStyle {
                        fg: p.t1,
                        bg: p.fill2.over(p.side),
                        size: 13.0,
                        select: Some((0, text.encode_utf16().count())),
                        placeholder: String::new(),
                        password: false,
                        multiline: false,
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
            }
        }
    }
}

/// Messages made by the worker (English) shown translated when they are known.
pub fn tr_dyn(s: &str) -> &str {
    crate::i18n::tr_known(s).unwrap_or(s)
}

#[allow(dead_code)]
fn unused(_: Font) {}
