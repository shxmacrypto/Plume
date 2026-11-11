//! Playing: the queue, shuffle and repeat, the next song prepared ahead (gapless and crossfade, and no wait between
//! two YouTube songs), autoplay of similar songs when the queue ends, lyrics and animated cover of the song playing.

use super::{App, Panel, with_app};
use crate::{
    art,
    audio::{Cmd, Note, Spec},
    i18n::{fill, tr},
    library::{Track, data_dir},
    yt,
};
use plume_ui::Cx;
use serde_json::json;

impl App {
    /// Plays `keys` from `start` (the page's songs, an album, a playlist…).
    pub fn play_list(&mut self, keys: Vec<String>, start: usize, context: String, cx: &mut Cx) {
        if keys.is_empty() {
            return;
        }
        let first = keys.get(start).cloned().unwrap_or_else(|| keys[0].clone());
        self.queue = keys;
        self.qpos = start.min(self.queue.len() - 1);
        self.unshuffled = None;
        self.context = context;
        if self.set.shuffle {
            self.shuffle_rest(true);
            self.qpos = self.queue.iter().position(|k| *k == first).unwrap_or(0);
        }
        self.play_at(self.qpos, cx);
    }

    /// Double-click (or play button) on a row of the current page.
    pub fn play_rows(&mut self, i: usize, cx: &mut Cx) {
        let rows = self.page_keys();
        if rows.is_empty() {
            return;
        }
        let ctx = self.page_title();
        self.play_list(rows, i, ctx, cx);
    }

    /// Mixes what comes after the current song (Fisher–Yates on the indexes, with Windows' random numbers).
    fn shuffle_rest(&mut self, all: bool) {
        if self.unshuffled.is_none() {
            self.unshuffled = Some(self.queue.clone());
        }
        let from = if all { 0 } else { self.qpos + 1 };
        let n = self.queue.len();
        if n <= from + 1 {
            return;
        }
        let mut seed = [0u8; 8];
        let _ = getrandom_fill(&mut seed);
        let mut x = u64::from_le_bytes(seed) | 1;
        for i in (from + 1..n).rev() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let j = from + (x % (i - from + 1) as u64) as usize;
            self.queue.swap(i, j);
        }
    }

    pub fn toggle_shuffle(&mut self, cx: &mut Cx) {
        self.set.shuffle = !self.set.shuffle;
        let cur = self.queue.get(self.qpos).cloned();
        if self.set.shuffle {
            // the current song stays first, the rest is mixed
            if let Some(c) = &cur {
                let i = self.qpos;
                self.queue.swap(0, i);
                self.qpos = 0;
                let _ = c;
            }
            self.shuffle_rest(false);
        } else if let Some(orig) = self.unshuffled.take() {
            self.queue = orig;
            self.qpos = cur.and_then(|c| self.queue.iter().position(|k| *k == c)).unwrap_or(0);
        }
        self.set.save();
        self.prepare_next();
        cx.repaint();
    }

    pub fn cycle_repeat(&mut self, cx: &mut Cx) {
        self.set.repeat = (self.set.repeat + 1) % 3;
        self.set.save();
        self.prepare_next();
        cx.repaint();
    }

    /// Starts the song at `i` of the queue (fetching its sound first if it comes from YouTube or SoundCloud).
    pub fn play_at(&mut self, i: usize, cx: &mut Cx) {
        let Some(key) = self.queue.get(i).cloned() else { return };
        self.qpos = i;
        let Some(t) = self.lib.tracks.get(&key).cloned() else { return };
        self.load_gen += 1;
        let g = self.load_gen;
        self.loading = Some(key.clone());
        self.prepared = None;
        self.engine.send(Cmd::Next(None));
        self.on_track_change(&t, cx);
        if t.local() {
            let id = self.new_id();
            let spec = Spec { id, path: t.path.clone().unwrap(), growth: None, gain_db: self.gain(&t), start_at: 0.0 };
            self.cur = Some((id, key));
            self.loading = None;
            self.engine.send(Cmd::Load(spec, true));
            self.after_start(cx);
            return;
        }
        if !yt::have_ytdlp() {
            self.loading = None;
            self.modal = Some(super::Modal::Tools);
            cx.repaint();
            return;
        }
        let sender = self.win.sender();
        let gain = self.gain(&t);
        std::thread::spawn(move || {
            let r = yt::fetch(&t);
            sender.send(move |h, cx| {
                let Some(app) = with_app(h) else { return };
                if app.load_gen != g {
                    return; // another song was asked in the meantime
                }
                match r {
                    Ok((path, growth)) => {
                        let id = app.new_id();
                        let done = growth.done.load(std::sync::atomic::Ordering::Acquire);
                        let spec = Spec { id, path, growth: (!done).then_some(growth), gain_db: gain, start_at: 0.0 };
                        app.cur = Some((id, key));
                        app.loading = None;
                        app.engine.send(Cmd::Load(spec, true));
                        app.after_start(cx);
                    }
                    Err(e) => {
                        app.loading = None;
                        app.note(format!("play failed {key}: {e}"));
                        app.say(fill(tr("Could not play this song: {s}"), &[&e]), cx);
                        // the next one, unless every song fails
                        if app.qpos + 1 < app.queue.len() {
                            let n = app.qpos + 1;
                            app.play_at(n, cx);
                        }
                    }
                }
                cx.repaint();
            });
        });
        cx.repaint();
    }

    fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn gain(&self, t: &Track) -> f32 {
        if self.set.normalize { t.gain.unwrap_or(0.0).clamp(-12.0, 6.0) } else { 0.0 }
    }

    fn after_start(&mut self, cx: &mut Cx) {
        if let Some((_, k)) = self.cur.clone() {
            self.lib.played(&k);
        }
        self.prepare_next();
        self.save_queue();
        self.update_smtc();
        cx.repaint();
    }

    /// The song after this one, by the queue and repeat (None: the end).
    fn next_index(&self) -> Option<usize> {
        if self.set.repeat == 2 {
            return Some(self.qpos);
        }
        if self.qpos + 1 < self.queue.len() {
            Some(self.qpos + 1)
        } else if self.set.repeat == 1 && !self.queue.is_empty() {
            Some(0)
        } else {
            None
        }
    }

    /// Prepares the next song in the engine (its sound fetched ahead when it comes from the network).
    pub fn prepare_next(&mut self) {
        let Some(i) = self.next_index() else {
            self.prepared = None;
            self.engine.send(Cmd::Next(None));
            // the end of the queue is near: similar songs, if wanted
            self.autoplay_more();
            return;
        };
        let Some(key) = self.queue.get(i).cloned() else { return };
        if self.prepared.as_ref().is_some_and(|(_, k)| *k == key) {
            return;
        }
        let Some(t) = self.lib.tracks.get(&key).cloned() else { return };
        let gain = self.gain(&t);
        if t.local() {
            let id = self.new_id();
            self.prepared = Some((id, key));
            self.engine.send(Cmd::Next(Some(Spec { id, path: t.path.clone().unwrap(), growth: None, gain_db: gain, start_at: 0.0 })));
            return;
        }
        if !yt::have_ytdlp() || self.resolving.contains(&key) {
            return;
        }
        self.resolving.insert(key.clone());
        let sender = self.win.sender();
        let g = self.load_gen;
        std::thread::spawn(move || {
            // fetched now: the next song starts at once, without the few seconds of yt-dlp
            let r = yt::fetch(&t);
            sender.send(move |h, _cx| {
                let Some(app) = with_app(h) else { return };
                app.resolving.remove(&key);
                if app.load_gen != g || app.next_index().and_then(|i| app.queue.get(i)) != Some(&key) {
                    return;
                }
                if let Ok((path, growth)) = r {
                    let id = app.new_id();
                    let done = growth.done.load(std::sync::atomic::Ordering::Acquire);
                    app.prepared = Some((id, key));
                    app.engine.send(Cmd::Next(Some(Spec { id, path, growth: (!done).then_some(growth), gain_db: gain, start_at: 0.0 })));
                }
            });
        });
    }

    /// The queue is ending: similar songs from YouTube Music's radio of the last one.
    fn autoplay_more(&mut self) {
        if !self.set.autoplay || self.set.repeat != 0 || self.queue.is_empty() {
            return;
        }
        let Some(seed) = self.queue.last().and_then(|k| self.lib.tracks.get(k)).and_then(|t| t.yt.clone()) else { return };
        if self.resolving.contains("radio") {
            return;
        }
        self.resolving.insert("radio".into());
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let r = yt::radio(&seed).unwrap_or_default();
            sender.send(move |h, _cx| {
                let Some(app) = with_app(h) else { return };
                app.resolving.remove("radio");
                let explicit = app.set.explicit;
                let mut added = 0;
                for t in r.into_iter().skip(1).filter(|t| explicit || !t.explicit) {
                    if app.queue.contains(&t.key) || added >= 15 {
                        continue;
                    }
                    let k = t.key.clone();
                    if !app.lib.tracks.contains_key(&k) {
                        app.lib.tracks.insert(k.clone(), Track { added: 0, ..t });
                    }
                    app.queue.push(k);
                    added += 1;
                }
                if added > 0 {
                    app.note(format!("autoplay +{added}"));
                    app.prepare_next();
                }
            });
        });
    }

    /// News from the engine.
    pub fn on_note(&mut self, n: Note, cx: &mut Cx) {
        match n {
            Note::Started(id) => {
                // the prepared song took over (gapless, crossfade)
                if let Some((pid, key)) = self.prepared.clone()
                    && pid == id
                {
                    if let Some(i) = self.next_index() {
                        self.qpos = i;
                    }
                    self.cur = Some((id, key.clone()));
                    self.prepared = None;
                    if let Some(t) = self.lib.tracks.get(&key).cloned() {
                        self.on_track_change(&t, cx);
                    }
                    self.after_start(cx);
                    if self.sleep_end_of_song {
                        self.sleep_end_of_song = false;
                        self.engine.send(Cmd::Pause);
                    }
                }
            }
            Note::Ended(id) => {
                if self.cur.as_ref().is_some_and(|(c, _)| *c == id) && self.prepared.is_none() {
                    // nothing was ready to take over
                    if self.sleep_end_of_song {
                        self.sleep_end_of_song = false;
                    } else if let Some(i) = self.next_index() {
                        self.play_at(i, cx);
                    }
                }
            }
            Note::Failed(id, e) => {
                if self.cur.as_ref().is_some_and(|(c, _)| *c == id) {
                    self.note(format!("engine failed: {e}"));
                    // a damaged cache file: forget it (the network next time)
                    if let Some(t) = self.current().cloned()
                        && let Some(p) = yt::cached(&t)
                    {
                        let _ = std::fs::remove_file(p);
                    }
                    self.say(fill(tr("Could not play this song: {s}"), &[&e]), cx);
                    self.cur = None;
                    if self.qpos + 1 < self.queue.len() {
                        let n = self.qpos + 1;
                        self.play_at(n, cx);
                    }
                }
            }
        }
        cx.repaint();
    }

    /// A new song is heard: its lyrics, its colors, its animated cover.
    fn on_track_change(&mut self, t: &Track, cx: &mut Cx) {
        if let Some(src) = art::source(t) {
            self.ensure_palette(&src);
        }
        self.refresh_accent();
        self.lyric_y = 0.0;
        self.load_lyrics(t);
        self.load_motion(t);
        self.blur = None;
        cx.repaint();
    }

    pub fn load_lyrics(&mut self, t: &Track) {
        if self.lyrics.as_ref().is_some_and(|(k, _)| *k == t.key) || self.lyrics_loading.as_deref() == Some(t.key.as_str()) {
            return;
        }
        self.lyrics = None;
        self.lyrics_loading = Some(t.key.clone());
        let t = t.clone();
        let sender = self.win.sender();
        self.pool.run(move || {
            let l = crate::lyrics::find(&t);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h)
                    && app.lyrics_loading.as_deref() == Some(t.key.as_str())
                {
                    app.lyrics_loading = None;
                    app.lyrics = Some((t.key.clone(), l));
                    cx.repaint();
                }
            });
        });
    }

    fn load_motion(&mut self, t: &Track) {
        let akey = format!("{}\u{1}{}", t.artist, t.album);
        if self.motion.as_ref().is_some_and(|(k, _)| *k == akey) {
            return;
        }
        self.motion = None;
        if !self.set.animated_covers || !yt::have_ffmpeg() || self.motion_loading.as_deref() == Some(akey.as_str()) {
            return;
        }
        self.motion_loading = Some(akey.clone());
        let t = t.clone();
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let m = art::motion(&t);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h)
                    && app.motion_loading.as_deref() == Some(akey.as_str())
                {
                    app.motion_loading = None;
                    if let Some(m) = m {
                        app.note(format!("motion {} frames", m.frames.len()));
                        app.motion = Some((akey, m));
                    }
                    cx.repaint();
                }
            });
        });
    }

    pub fn toggle_play(&mut self, cx: &mut Cx) {
        if self.cur.is_none() {
            if !self.queue.is_empty() {
                let i = self.qpos;
                self.play_at(i, cx);
            } else {
                let rows = self.page_keys();
                if !rows.is_empty() {
                    let ctx = self.page_title();
                    self.play_list(rows, 0, ctx, cx);
                }
            }
            return;
        }
        self.engine.send(if self.st.playing { Cmd::Pause } else { Cmd::Play });
        self.st.playing = !self.st.playing;
        self.update_smtc();
        cx.repaint();
    }

    /// Next (1) or previous (-1). Previous restarts the song after its first 3 seconds.
    pub fn skip(&mut self, dir: i32, cx: &mut Cx) {
        if self.queue.is_empty() {
            return;
        }
        if dir < 0 && self.st.pos > 3.0 {
            self.engine.send(Cmd::Seek(0.0));
            return;
        }
        let n = self.queue.len() as i32;
        let mut i = self.qpos as i32 + dir;
        if i >= n {
            if self.set.repeat == 1 {
                i = 0;
            } else {
                return;
            }
        }
        if i < 0 {
            i = if self.set.repeat == 1 { n - 1 } else { 0 };
        }
        self.play_at(i as usize, cx);
    }

    pub fn seek_by(&mut self, d: f64) {
        let t = (self.st.pos + d).clamp(0.0, (self.st.dur - 1.0).max(0.0));
        self.engine.send(Cmd::Seek(t));
    }

    pub fn change_volume(&mut self, d: f32) {
        self.set.volume = (self.set.volume + d).clamp(0.0, 1.0);
        self.muted_volume = None;
        self.engine.send(Cmd::Volume(self.set.volume));
    }

    pub fn toggle_panel(&mut self, p: Panel, cx: &mut Cx) {
        self.panel = if self.panel == Some(p) { None } else { Some(p) };
        self.set.show_lyrics_panel = self.panel == Some(Panel::Lyrics);
        self.set.save();
        cx.repaint();
    }

    pub fn play_next(&mut self, key: &str) {
        if self.queue.is_empty() {
            self.queue.push(key.to_string());
            return;
        }
        let at = (self.qpos + 1).min(self.queue.len());
        self.queue.insert(at, key.to_string());
        self.prepare_next();
    }

    pub fn add_queue(&mut self, key: &str) {
        self.queue.push(key.to_string());
        self.prepare_next();
    }

    pub fn start_radio(&mut self, key: &str, cx: &mut Cx) {
        let Some(id) = self.lib.tracks.get(key).and_then(|t| t.yt.clone()) else { return };
        let sender = self.win.sender();
        let k = key.to_string();
        std::thread::spawn(move || {
            let r = yt::radio(&id).unwrap_or_default();
            sender.send(move |h, cx| {
                let Some(app) = with_app(h) else { return };
                let mut keys = vec![k.clone()];
                for t in r {
                    if keys.contains(&t.key) {
                        continue;
                    }
                    let tk = t.key.clone();
                    if !app.lib.tracks.contains_key(&tk) {
                        app.lib.tracks.insert(tk.clone(), Track { added: 0, ..t });
                    }
                    keys.push(tk);
                }
                let name = app.lib.tracks.get(&k).map(|t| t.title.clone()).unwrap_or_default();
                app.play_list(keys, 0, format!("{} · {}", tr("Radio"), name), cx);
            });
        });
        cx.repaint();
    }

    // ───────────────────────── remembered between launches ─────────────────────────

    pub fn save_queue(&self) {
        let v = json!({ "queue": self.queue, "pos": self.qpos, "context": self.context, "at": self.st.pos });
        let _ = std::fs::create_dir_all(data_dir());
        let _ = std::fs::write(data_dir().join("queue.json"), v.to_string());
    }

    pub fn restore_queue(&mut self) {
        let Ok(b) = std::fs::read(data_dir().join("queue.json")) else { return };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&b) else { return };
        self.queue = v["queue"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).filter(|k| self.lib.tracks.contains_key(k)).collect())
            .unwrap_or_default();
        self.qpos = (v["pos"].as_u64().unwrap_or(0) as usize).min(self.queue.len().saturating_sub(1));
        self.context = v["context"].as_str().unwrap_or("").to_string();
        if let Some(t) = self.queue.get(self.qpos).and_then(|k| self.lib.tracks.get(k)).cloned() {
            // shown in the player bar, ready to play
            if let Some(src) = art::source(&t) {
                self.ensure_palette(&src);
            }
            self.refresh_accent();
            self.load_lyrics(&t);
        }
    }

    // ───────────────────────── Windows' media overlay ─────────────────────────

    pub fn update_smtc(&mut self) {
        let Some(t) = self.current().cloned() else { return };
        let cover = art::source(&t).and_then(|s| art::cached(&s));
        if let Some(s) = &mut self.smtc {
            s.update(&t.title, &t.artist, &t.album, self.st.playing, cover.as_deref());
        }
    }
}

fn getrandom_fill(b: &mut [u8]) -> bool {
    use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};
    unsafe { BCryptGenRandom(None, b, BCRYPT_USE_SYSTEM_PREFERRED_RNG).is_ok() }
}
