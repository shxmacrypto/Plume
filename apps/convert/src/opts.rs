//! What a conversion should make: the preset (one click) and its few choices, plus the advanced settings used by
//! "Custom". Saved with the settings, so the last choices come back.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Preset {
    Phone,
    Discord,
    Smaller,
    Audio,
    Gif,
    Images,
    Frames,
    Trim,
    Mute,
    Rotate,
    Custom,
}

impl Preset {
    pub const ALL: [Preset; 11] = [
        Preset::Phone,
        Preset::Discord,
        Preset::Smaller,
        Preset::Audio,
        Preset::Gif,
        Preset::Images,
        Preset::Frames,
        Preset::Trim,
        Preset::Mute,
        Preset::Rotate,
        Preset::Custom,
    ];

    /// Its name in the test socket and the settings file.
    #[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
    pub fn key(self) -> &'static str {
        match self {
            Preset::Phone => "phone",
            Preset::Discord => "discord",
            Preset::Smaller => "smaller",
            Preset::Audio => "audio",
            Preset::Gif => "gif",
            Preset::Images => "images",
            Preset::Frames => "frames",
            Preset::Trim => "trim",
            Preset::Mute => "mute",
            Preset::Rotate => "rotate",
            Preset::Custom => "custom",
        }
    }

    #[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
    pub fn from_key(k: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.key() == k)
    }

    /// The name shown on its tile (English; the window translates it).
    pub fn name(self) -> &'static str {
        match self {
            Preset::Phone => "For phone",
            Preset::Discord => "For Discord",
            Preset::Smaller => "Smaller file",
            Preset::Audio => "Audio only",
            Preset::Gif => "GIF",
            Preset::Images => "Images",
            Preset::Frames => "Extract frames",
            Preset::Trim => "Trim",
            Preset::Mute => "Mute",
            Preset::Rotate => "Rotate",
            Preset::Custom => "Advanced",
        }
    }

    pub fn sub(self) -> &'static str {
        match self {
            Preset::Phone => "Plays everywhere",
            Preset::Discord => "Under the size limit",
            Preset::Smaller => "Same look, fewer MB",
            Preset::Audio => "MP3, M4A, FLAC…",
            Preset::Gif => "Loop from a video",
            Preset::Images => "WebP, JPEG, PNG…",
            Preset::Frames => "Pictures from a video",
            Preset::Trim => "Keep a part",
            Preset::Mute => "Remove the sound",
            Preset::Rotate => "Turn the picture",
            Preset::Custom => "Pick every setting",
        }
    }

    /// True when the result is a picture made by WIC (not FFmpeg).
    pub fn is_image(self) -> bool {
        self == Preset::Images
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Opts {
    /// "For Discord": the size to stay under, in MB.
    pub discord_mb: u32,
    /// "Audio only": mp3, m4a, wav, flac, ogg.
    pub audio_fmt: String,
    pub gif_fps: u32,
    /// The GIF's longer side, in pixels.
    pub gif_width: u32,
    /// "Images": webp, jpeg, png, bmp, tiff, ico.
    pub img_fmt: String,
    /// 1..100 (JPEG, WebP).
    pub img_quality: u32,
    /// Longer side at most this (0: as it is).
    pub img_max: u32,
    /// One picture every this many seconds (0: every frame).
    pub frames_every: f64,
    /// png or jpeg.
    pub frames_fmt: String,
    pub trim_start: f64,
    /// 0: to the end.
    pub trim_end: f64,
    /// Exact cut (encodes again) or fast (copies, cuts at the nearest key frame).
    pub trim_exact: bool,
    /// Clockwise: 90, 180, 270.
    pub rotate: u32,
    // ── Custom ──
    /// mp4, mkv, webm, mov.
    pub container: String,
    /// h264, hevc, vp9, av1, copy.
    pub vcodec: String,
    /// aac, mp3, opus, flac, copy, none.
    pub acodec: String,
    /// The shorter side at most this (0: as it is), "1080" meaning 1080p.
    pub height: u32,
    /// 0: as it is.
    pub fps: u32,
    /// Constant quality (lower: better, bigger).
    pub crf: u32,
    /// kbit/s; 0: constant quality instead.
    pub vbitrate: u32,
    pub abitrate: u32,
    /// 0: as it is.
    pub sample_rate: u32,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            discord_mb: 10,
            audio_fmt: "mp3".into(),
            gif_fps: 15,
            gif_width: 480,
            img_fmt: "webp".into(),
            img_quality: 85,
            img_max: 0,
            frames_every: 1.0,
            frames_fmt: "png".into(),
            trim_start: 0.0,
            trim_end: 0.0,
            trim_exact: true,
            rotate: 90,
            container: "mp4".into(),
            vcodec: "h264".into(),
            acodec: "aac".into(),
            height: 0,
            fps: 0,
            crf: 23,
            vbitrate: 0,
            abitrate: 160,
            sample_rate: 0,
        }
    }
}

/// "1:23", "1:02:03.5", "75", "75.5" → seconds.
pub fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    if s.is_empty() {
        return None;
    }
    let mut total = 0.0;
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() > 3 {
        return None;
    }
    for (i, p) in parts.iter().enumerate() {
        let v: f64 = p.trim().parse().ok()?;
        if v < 0.0 || (i > 0 && v >= 60.0) {
            return None;
        }
        total = total * 60.0 + v;
    }
    Some(total)
}

/// Seconds → "1:05", "1:02:03", with tenths when asked ("1:05.5").
pub fn clock(t: f64, tenths: bool) -> String {
    let t = t.max(0.0);
    let whole = t.floor() as u64;
    let (h, m, s) = (whole / 3600, whole / 60 % 60, whole % 60);
    let mut out = if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") };
    let frac = ((t - t.floor()) * 10.0).round() as u64;
    if tenths && frac > 0 && frac < 10 {
        out.push_str(&format!(".{frac}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for p in Preset::ALL {
            assert_eq!(Preset::from_key(p.key()), Some(p));
        }
        assert_eq!(Preset::from_key("nope"), None);
    }

    #[test]
    fn times() {
        assert_eq!(parse_time("75"), Some(75.0));
        assert_eq!(parse_time("1:15"), Some(75.0));
        assert_eq!(parse_time("1:02:03.5"), Some(3723.5));
        assert_eq!(parse_time("0:07,5"), Some(7.5));
        assert_eq!(parse_time("1:75"), None);
        assert_eq!(parse_time("abc"), None);
        assert_eq!(parse_time(""), None);
        assert_eq!(clock(75.0, false), "1:15");
        assert_eq!(clock(3723.5, true), "1:02:03.5");
        assert_eq!(clock(7.0, true), "0:07");
    }
}
