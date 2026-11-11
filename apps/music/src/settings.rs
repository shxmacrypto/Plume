//! Plume Music's settings, in %LOCALAPPDATA%\Plume\Music\settings.json.

use crate::library::{data_dir, music_folder};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Equalizer presets: name, preamp, 10 bands (31 Hz … 16 kHz).
pub const PRESETS: [(&str, f32, [f32; 10]); 9] = [
    ("Flat", 0.0, [0.0; 10]),
    ("Bass boost", -4.0, [6.0, 5.0, 4.0, 2.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("Treble boost", -3.0, [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.5, 4.0, 5.0, 5.5]),
    ("Vocal", -2.0, [-2.0, -1.5, -1.0, 1.0, 3.0, 3.5, 3.0, 1.5, 0.0, -1.0]),
    ("Electronic", -4.0, [5.0, 4.0, 1.5, 0.0, -1.5, 1.0, 0.5, 1.5, 4.0, 5.0]),
    ("Rock", -3.5, [4.5, 3.5, 2.0, 0.5, -1.0, -0.5, 1.0, 2.5, 3.5, 4.0]),
    ("Hip-hop", -4.0, [5.5, 4.5, 1.5, 2.5, -1.0, -1.0, 1.0, -0.5, 2.0, 3.0]),
    ("Acoustic", -2.0, [3.5, 3.0, 2.5, 1.0, 1.5, 1.5, 3.0, 3.5, 3.0, 2.0]),
    ("Loudness", -4.0, [5.0, 3.5, 0.0, 0.0, -1.5, 0.0, -0.5, -4.0, 4.0, 1.5]),
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "cover" (the playing song's color), or a fixed color: "rose", "teal", "blue", "green", "orange", "violet".
    pub accent: String,
    /// Behind "Now playing": "cover" (moving colors of the cover), "blur" (the cover, blurred), "gif", "plain".
    pub background: String,
    pub gif: Option<PathBuf>,
    /// The GIF also behind the whole window, softly.
    pub gif_everywhere: bool,
    /// Apple Music's motion covers when the album has one (needs FFmpeg).
    pub animated_covers: bool,
    /// Fewer animations (also follows Windows' setting).
    pub reduce_motion: bool,
    pub crossfade: f32,
    pub gapless: bool,
    pub normalize: bool,
    pub eq_on: bool,
    pub eq: [f32; 10],
    pub preamp: f32,
    pub preset: String,
    pub volume: f32,
    /// When the queue ends, keep playing similar songs (YouTube Music radio).
    pub autoplay: bool,
    pub explicit: bool,
    /// "m4a" (as YouTube serves it, no loss) or "mp3" (320 kbps, needs FFmpeg).
    pub format: String,
    pub download_dir: PathBuf,
    pub cache_mb: u64,
    /// 0 small, 1 medium, 2 large.
    pub lyrics_size: u8,
    pub lang: String,
    pub sidebar_small: bool,
    pub shuffle: bool,
    /// 0 off, 1 all, 2 one.
    pub repeat: u8,
    pub show_lyrics_panel: bool,
    pub notify: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            accent: "cover".into(),
            background: "cover".into(),
            gif: None,
            gif_everywhere: false,
            animated_covers: true,
            reduce_motion: false,
            crossfade: 0.0,
            gapless: true,
            normalize: true,
            eq_on: false,
            eq: [0.0; 10],
            preamp: 0.0,
            preset: "Flat".into(),
            volume: 0.8,
            autoplay: true,
            explicit: true,
            format: "m4a".into(),
            download_dir: music_folder().join("Plume Music"),
            cache_mb: 2048,
            lyrics_size: 1,
            lang: "system".into(),
            sidebar_small: false,
            shuffle: false,
            repeat: 0,
            show_lyrics_panel: false,
            notify: true,
        }
    }
}

impl Settings {
    fn file() -> PathBuf {
        data_dir().join("settings.json")
    }
    pub fn load() -> Settings {
        std::fs::read(Self::file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(data_dir());
        let tmp = Self::file().with_extension("tmp");
        if let Ok(b) = serde_json::to_vec_pretty(self)
            && std::fs::write(&tmp, b).is_ok()
        {
            let _ = std::fs::rename(&tmp, Self::file());
        }
    }
    pub fn eq_cmd(&self) -> Option<([f32; 10], f32)> {
        self.eq_on.then_some((self.eq, self.preamp))
    }
}
