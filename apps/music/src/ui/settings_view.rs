//! The settings page.

use super::{App, Hit, draw};
use crate::{
    audio::{Cmd, EQ_FREQS},
    i18n::{fill, tr},
    settings::PRESETS,
    yt,
};
use plume_ui::{Align, Canvas, Color, Cx, Font, Rect, icons, rect};

impl App {
    fn group(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        let p = self.pal.clone();
        draw::text(c, title, rect(x, y, w, 30.0), 19.0, 700, p.t1, Align::Left);
        y + 40.0
    }

    /// A row: label (and a line under it) on the left; returns the row's rect for the control on the right.
    fn row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, sub: &str) -> Rect {
        let p = self.pal.clone();
        let h = if sub.is_empty() { 48.0 } else { 62.0 };
        let r = rect(x, y, w, h);
        c.text(label, rect(x, y + if sub.is_empty() { 0.0 } else { 8.0 }, w * 0.55, if sub.is_empty() { h } else { 22.0 }), Font::Body, p.t1, Align::Left);
        if !sub.is_empty() {
            c.text(sub, rect(x, y + 32.0, w * 0.6, 20.0), Font::Small, p.t3, Align::Left);
        }
        c.hline(x, y + h, w, p.line);
        r
    }

    fn toggle(&mut self, c: &mut Canvas, r: Rect, on: bool, name: &'static str) {
        let p = self.pal.clone();
        let sw = rect(r.right() - 48.0, r.y + (r.h - 26.0) / 2.0, 48.0, 26.0);
        c.fill_round(sw, 13.0, if on { p.accent } else { p.t1.fade(0.22) });
        let k = if on { sw.right() - 23.0 } else { sw.x + 3.0 };
        c.fill_round(rect(k, sw.y + 3.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
        self.hits.add(r, Hit::Toggle(name));
    }

    /// Segmented choice on the right of a row.
    fn choices(&mut self, c: &mut Canvas, r: Rect, labels: &[&str], cur: usize, name: &'static str) {
        let p = self.pal.clone();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::Small) + 26.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 8.0;
        let mut x = r.right() - total;
        let bg = rect(x - 4.0, r.y + (r.h - 34.0) / 2.0, total, 34.0);
        c.fill_round(bg, 17.0, p.t1.fade(0.07));
        x += 0.0;
        for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
            let b = rect(x, bg.y + 3.0, w, 28.0);
            let hit = Hit::Choice(name, i);
            if i == cur {
                c.fill_round(b, 14.0, p.accent);
            } else if self.hovered(&hit) {
                c.fill_round(b, 14.0, p.t1.fade(0.08));
            }
            c.text(l, b, Font::Small, if i == cur { p.on_accent } else { p.t1 }, Align::Center);
            self.hits.add(b, hit);
            x += w;
        }
    }

    pub fn paint_settings(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, _page: Rect) -> f32 {
        let p = self.pal.clone();
        let w = w.min(860.0);
        let mut y = y;
        draw::text(c, tr("Settings"), rect(x, y, w, 50.0), 34.0, 800, p.t1, Align::Left);
        y += 72.0;

        // appearance
        y = self.group(c, x, y, w, tr("Appearance"));
        let r = self.row(c, x, y, w, tr("Theme"), "");
        let cur = ["system", "dark", "light"].iter().position(|t| *t == self.set.theme).unwrap_or(0);
        self.choices(c, r, &[tr("System"), tr("Dark"), tr("Light")], cur, "theme");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Accent color"), "");
        let accents = ["cover", "rose", "teal", "blue", "green", "orange", "violet"];
        let mut sx = r.right() - accents.len() as f32 * 34.0;
        for (i, a) in accents.iter().enumerate() {
            let s = rect(sx, r.y + 11.0, 26.0, 26.0);
            let col = if *a == "cover" { p.accent } else { super::accent_named(a, p.dark) };
            if *a == "cover" {
                c.fill_round(s, 13.0, Color::hex(0xfb3e6e));
                c.fill_round(rect(s.x + 13.0, s.y, 13.0, 26.0), 6.0, Color::hex(0x8b5cf6));
                c.fill_round(rect(s.x + 6.0, s.y + 6.0, 14.0, 14.0), 7.0, Color::hex(0x14b8a6));
            } else {
                c.fill_round(s, 13.0, col);
            }
            if self.set.accent == *a {
                c.stroke_round(s.inset(-3.0), 16.0, p.t1, 2.0);
            }
            self.hits.add(s, Hit::Choice("accent", i));
            sx += 34.0;
        }
        c.text(tr("From the cover"), rect(r.x + w * 0.3, r.y, 200.0, r.h), Font::Small, p.t3, Align::Left);
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Now playing background"), "");
        let bgs = ["cover", "blur", "gif", "plain"];
        let cur = bgs.iter().position(|b| *b == self.set.background).unwrap_or(0);
        self.choices(c, r, &[tr("Moving colors of the cover"), tr("Blurred cover"), tr("Animated GIF"), tr("Plain")], cur, "background");
        y += r.h + 1.0;
        let gif_name =
            self.set.gif.as_ref().and_then(|g| g.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| tr("Choose a GIF…").to_string());
        let r = self.row(c, x, y, w, tr("Animated GIF"), &gif_name);
        let bw = c.text_width(tr("Choose a GIF…"), Font::BodyBold) + 32.0;
        self.pill(c, rect(r.right() - bw, r.y + 13.0, bw, 36.0), tr("Choose a GIF…"), false, Hit::Action("gif"));
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Also behind the whole window"), "");
        let on = self.set.gif_everywhere;
        self.toggle(c, r, on, "gif_everywhere");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Animated album covers"), tr("When Apple Music has a motion cover for the album (needs FFmpeg)."));
        let on = self.set.animated_covers;
        self.toggle(c, r, on, "animated_covers");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Reduce motion"), "");
        let on = self.set.reduce_motion;
        self.toggle(c, r, on, "reduce_motion");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Lyrics size"), "");
        let cur = self.set.lyrics_size as usize;
        self.choices(c, r, &[tr("Small"), tr("Medium"), tr("Large")], cur, "lyrics_size");
        y += r.h + 28.0;

        // playback
        y = self.group(c, x, y, w, tr("Playback"));
        let label = if self.set.crossfade < 0.5 { tr("Off").to_string() } else { format!("{:.0} s", self.set.crossfade) };
        let r = self.row(c, x, y, w, tr("Crossfade"), "");
        c.text(&label, rect(r.right() - 270.0, r.y, 50.0, r.h), Font::Small, p.t2, Align::Right);
        let v = self.set.crossfade / 12.0;
        let accent = p.accent;
        self.slider(c, rect(r.right() - 210.0, r.y + 14.0, 210.0, 20.0), v, Hit::Slider("crossfade"), accent);
        y += r.h + 1.0;
        for (name, label, sub) in [
            ("gapless", tr("Gapless playback"), ""),
            ("normalize", tr("Normalize volume"), tr("Uses the songs' ReplayGain when they have one.")),
            ("autoplay", tr("Autoplay"), tr("When the queue ends, keep playing similar songs.")),
            ("explicit", tr("Allow explicit songs"), ""),
            ("notify", tr("Notifications"), tr("Show the song in Windows' media overlay and use the media keys.")),
        ] {
            let r = self.row(c, x, y, w, label, sub);
            let on = match name {
                "gapless" => self.set.gapless,
                "normalize" => self.set.normalize,
                "autoplay" => self.set.autoplay,
                "explicit" => self.set.explicit,
                _ => self.set.notify,
            };
            self.toggle(c, r, on, name);
            y += r.h + 1.0;
        }
        y += 28.0;

        // equalizer
        y = self.group(c, x, y, w, tr("Equalizer"));
        let r = self.row(c, x, y, w, tr("Equalizer"), "");
        let on = self.set.eq_on;
        self.toggle(c, r, on, "eq_on");
        y += r.h + 12.0;
        // presets
        let mut px = x;
        let mut py = y;
        for (i, (name, _, _)) in PRESETS.iter().enumerate() {
            let tw = c.text_width(name, Font::Small) + 26.0;
            if px + tw > x + w {
                px = x;
                py += 38.0;
            }
            let b = rect(px, py, tw, 30.0);
            let active = self.set.preset == *name;
            let hit = Hit::Preset(i);
            c.fill_round(
                b,
                15.0,
                if active {
                    p.accent
                } else if self.hovered(&hit) {
                    p.t1.fade(0.14)
                } else {
                    p.t1.fade(0.07)
                },
            );
            c.text(name, b, Font::Small, if active { p.on_accent } else { p.t1 }, Align::Center);
            self.hits.add(b, hit);
            px += tw + 8.0;
        }
        y = py + 50.0;
        // the bands (and the preamp), vertical sliders
        let bh = 160.0;
        let n = 11;
        let col_w = (w / n as f32).min(70.0);
        #[allow(clippy::needless_range_loop)] // 10 bands and the preamp, side by side
        for i in 0..n {
            let cxm = x + col_w * i as f32 + col_w / 2.0;
            let db = if i == 10 { self.set.preamp } else { self.set.eq[i] };
            let track = rect(cxm - 2.0, y, 4.0, bh);
            c.fill_round(track, 2.0, p.t1.fade(0.15));
            let zero = y + bh / 2.0;
            let ky = y + bh * (1.0 - (db + 12.0) / 24.0);
            let (top, h) = if ky < zero { (ky, zero - ky) } else { (zero, ky - zero) };
            let col = if self.set.eq_on { p.accent } else { p.t3 };
            c.fill_round(rect(cxm - 2.0, top, 4.0, h.max(1.0)), 2.0, col);
            c.fill_round(rect(cxm - 8.0, ky - 8.0, 16.0, 16.0), 8.0, if self.set.eq_on { Color::hex(0xffffff) } else { p.t2 });
            self.hits.add(rect(cxm - col_w / 2.0, y - 8.0, col_w, bh + 16.0), Hit::EqBand(i));
            let label = if i == 10 {
                tr("Preamp").to_string()
            } else {
                let f = EQ_FREQS[i];
                if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") }
            };
            c.text(&label, rect(cxm - col_w / 2.0, y + bh + 12.0, col_w, 18.0), Font::Small, p.t3, Align::Center);
            c.text(&format!("{:+.0}", db), rect(cxm - col_w / 2.0, y - 28.0, col_w, 18.0), Font::Small, p.t2, Align::Center);
        }
        y += bh + 64.0;

        // downloads and storage
        y = self.group(c, x, y, w, tr("Downloads and storage"));
        let r = self.row(c, x, y, w, tr("Format"), tr("Only download music you have the right to keep."));
        let cur = if self.set.format == "mp3" { 1 } else { 0 };
        self.choices(c, r, &[tr("M4A (original, no loss)"), tr("MP3 320 kbps (FFmpeg)")], cur, "format");
        y += r.h + 1.0;
        let folder = self.set.download_dir.display().to_string();
        let r = self.row(c, x, y, w, tr("Download folder"), &folder);
        let bw = c.text_width(tr("Change…"), Font::BodyBold) + 32.0;
        self.pill(c, rect(r.right() - bw, r.y + 13.0, bw, 36.0), tr("Change…"), false, Hit::Action("download_dir"));
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Music folders"), "");
        let bw = c.text_width(tr("Add a folder…"), Font::BodyBold) + 32.0;
        self.pill(c, rect(r.right() - bw, r.y + 6.0, bw, 36.0), tr("Add a folder…"), false, Hit::Action("add_folder"));
        let rw = c.text_width(tr("Rescan"), Font::BodyBold) + 32.0;
        self.pill(
            c,
            rect(r.right() - bw - rw - 8.0, r.y + 6.0, rw, 36.0),
            if self.scanning { tr("Scanning your music…") } else { tr("Rescan") },
            false,
            Hit::Action("rescan"),
        );
        y += r.h + 1.0;
        let folders = self.lib.folders.clone();
        for (i, f) in folders.iter().enumerate() {
            let rr = rect(x + 16.0, y, w - 16.0, 38.0);
            c.icon(&icons::FOLDER, rect(rr.x, rr.y + 11.0, 16.0, 16.0), p.t3);
            c.text(&f.display().to_string(), rect(rr.x + 26.0, rr.y, rr.w - 70.0, rr.h), Font::Small, p.t2, Align::Left);
            self.icon_btn(c, &icons::CLOSE, rect(rr.right() - 34.0, rr.y + 5.0, 28.0, 28.0), Hit::FolderRemove(i), p.t3);
            y += 38.0;
        }
        let size_mb = yt::cache_size() / (1024 * 1024);
        let r = self.row(c, x, y, w, tr("Cache for streamed songs"), &format!("{size_mb} MB / {} MB", self.set.cache_mb));
        let cw = c.text_width(tr("Clear"), Font::BodyBold) + 32.0;
        self.pill(c, rect(r.right() - cw, r.y + 13.0, cw, 36.0), tr("Clear"), false, Hit::Action("clear_cache"));
        let v = (self.set.cache_mb as f32 - 256.0) / (16384.0 - 256.0);
        self.slider(c, rect(r.right() - cw - 220.0, r.y + 21.0, 200.0, 20.0), v, Hit::Slider("cache"), accent);
        y += r.h + 28.0;

        // tools
        y = self.group(c, x, y, w, tr("Tools"));
        for (name, have, act) in [("yt-dlp", yt::have_ytdlp(), "install_ytdlp"), ("FFmpeg", yt::have_ffmpeg(), "install_ffmpeg")] {
            let r = self.row(c, x, y, w, name, if have { tr("Installed") } else { tr("Not installed") });
            if !have {
                let label = if self.tools_busy {
                    tr("Installing…")
                } else if name == "FFmpeg" {
                    tr("Install FFmpeg")
                } else {
                    tr("Install yt-dlp")
                };
                let bw = c.text_width(label, Font::BodyBold) + 32.0;
                self.pill(c, rect(r.right() - bw, r.y + 13.0, bw, 36.0), label, true, Hit::Action(act));
            }
            y += r.h + 1.0;
        }
        y += 28.0;

        // language
        y = self.group(c, x, y, w, tr("Language"));
        let r = self.row(c, x, y, w, tr("Language"), "");
        let cur = ["system", "en", "fr"].iter().position(|l| *l == self.set.lang).unwrap_or(0);
        self.choices(c, r, &[tr("System"), "English", "Français"], cur, "lang");
        y += r.h + 28.0;

        // shortcuts
        y = self.group(c, x, y, w, tr("Keyboard shortcuts"));
        let keys: [(&str, &str); 10] = [
            ("Space", tr("Play")),
            ("Ctrl + →  /  Ctrl + ←", "Next / Previous"),
            ("Shift + →  /  Shift + ←", "+10 s / -10 s"),
            ("Ctrl + ↑  /  Ctrl + ↓", tr("Volume")),
            ("Ctrl + F", tr("Search")),
            ("Ctrl + L", tr("Lyrics")),
            ("Ctrl + S  /  Ctrl + R", "Shuffle / Repeat"),
            ("Ctrl + I", tr("Import a playlist")),
            ("Ctrl + N", tr("New playlist")),
            ("F11", tr("Full screen")),
        ];
        for (k, what) in keys {
            c.text(k, rect(x, y, 260.0, 30.0), Font::SmallBold, p.t2, Align::Left);
            c.text(what, rect(x + 270.0, y, w - 270.0, 30.0), Font::Small, p.t3, Align::Left);
            y += 30.0;
        }
        y += 28.0;
        c.text(&format!("Plume Music {}", plume_update::VERSION), rect(x, y, w, 24.0), Font::Small, p.t3, Align::Left);
        y + 40.0 + self.scroll
    }

    pub fn set_slider(&mut self, name: &'static str, v: f32) {
        match name {
            "crossfade" => {
                self.set.crossfade = (v * 12.0).round();
                self.engine.send(Cmd::Crossfade(self.set.crossfade));
            }
            "cache" => {
                let mb = 256.0 + v * (16384.0 - 256.0);
                self.set.cache_mb = ((mb / 256.0).round() * 256.0) as u64;
                yt::CACHE_MB.store(self.set.cache_mb, std::sync::atomic::Ordering::Relaxed);
            }
            _ => {}
        }
    }

    pub fn settings_act(&mut self, hit: Hit, cx: &mut Cx) {
        match hit {
            Hit::Toggle(name) => {
                let s = &mut self.set;
                match name {
                    "gif_everywhere" => s.gif_everywhere = !s.gif_everywhere,
                    "animated_covers" => s.animated_covers = !s.animated_covers,
                    "reduce_motion" => s.reduce_motion = !s.reduce_motion,
                    "gapless" => s.gapless = !s.gapless,
                    "normalize" => s.normalize = !s.normalize,
                    "autoplay" => s.autoplay = !s.autoplay,
                    "explicit" => s.explicit = !s.explicit,
                    "notify" => {
                        s.notify = !s.notify;
                        if s.notify && self.smtc.is_none() {
                            self.smtc = crate::smtc::Smtc::new(&self.win);
                        }
                    }
                    "eq_on" => s.eq_on = !s.eq_on,
                    _ => {}
                }
                if name == "animated_covers" && self.set.animated_covers && !yt::have_ffmpeg() {
                    self.install_tools(true, cx);
                }
                self.settings_changed(cx);
            }
            Hit::Choice(name, i) => {
                match name {
                    "theme" => self.set.theme = ["system", "dark", "light"][i.min(2)].into(),
                    "accent" => self.set.accent = ["cover", "rose", "teal", "blue", "green", "orange", "violet"][i.min(6)].into(),
                    "background" => {
                        self.set.background = ["cover", "blur", "gif", "plain"][i.min(3)].into();
                        if self.set.background == "gif" && self.set.gif.is_none() {
                            self.pick_gif();
                        }
                    }
                    "lyrics_size" => self.set.lyrics_size = i.min(2) as u8,
                    "format" => {
                        self.set.format = if i == 1 { "mp3" } else { "m4a" }.into();
                        if i == 1 && !yt::have_ffmpeg() {
                            self.install_tools(true, cx);
                        }
                    }
                    "lang" => self.set.lang = ["system", "en", "fr"][i.min(2)].into(),
                    _ => {}
                }
                self.settings_changed(cx);
            }
            Hit::Preset(i) => {
                if let Some((name, pre, bands)) = PRESETS.get(i) {
                    self.set.preset = name.to_string();
                    self.set.preamp = *pre;
                    self.set.eq = *bands;
                    self.set.eq_on = true;
                    self.settings_changed(cx);
                }
            }
            Hit::FolderRemove(i) => {
                if i < self.lib.folders.len() {
                    self.lib.folders.remove(i);
                    self.lib.dirty = true;
                    self.rescan();
                }
            }
            Hit::Action(a) => match a {
                "gif" => {
                    self.pick_gif();
                    self.settings_changed(cx);
                }
                "download_dir" => {
                    if let Some(d) = plume_ui::dialogs::pick_folder(self.win.hwnd, tr("Download folder")) {
                        self.set.download_dir = d;
                        self.settings_changed(cx);
                    }
                }
                "add_folder" => {
                    if let Some(d) = plume_ui::dialogs::pick_folder(self.win.hwnd, tr("Music folders"))
                        && !self.lib.folders.contains(&d)
                    {
                        self.lib.folders.push(d);
                        self.lib.dirty = true;
                        self.rescan();
                    }
                }
                "rescan" => self.rescan(),
                "clear_cache" => {
                    yt::clear_cache();
                    self.say(tr("Done"), cx);
                }
                "install_ytdlp" => self.install_tools(false, cx),
                "install_ffmpeg" => self.install_tools(true, cx),
                "paste_songs" => self.import_clipboard(cx),
                _ => {}
            },
            _ => {}
        }
        let _ = fill;
        cx.repaint();
    }

    fn pick_gif(&mut self) {
        let files = plume_ui::dialogs::pick_files(self.win.hwnd, tr("Choose a GIF…"));
        if let Some(f) = files.into_iter().find(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"))) {
            self.set.gif = Some(f);
            self.gif = None;
            self.gif_failed = false;
        }
    }
}
