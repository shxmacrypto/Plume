//! The rows of chips: their labels, which one is chosen, and what a click changes.

use super::App;
use crate::i18n::tr;

const DISCORD: [u32; 3] = [10, 25, 50];
const AUDIO_FMT: [&str; 5] = ["mp3", "m4a", "wav", "flac", "ogg"];
const GIF_FPS: [u32; 3] = [10, 15, 24];
const GIF_W: [u32; 4] = [320, 480, 640, 800];
const IMG_FMT: [&str; 6] = ["webp", "jpeg", "png", "bmp", "tiff", "ico"];
const IMG_Q: [u32; 5] = [60, 75, 85, 95, 100];
const IMG_MAX: [u32; 6] = [0, 3840, 2560, 1920, 1280, 640];
const FR_EVERY: [f64; 5] = [0.0, 0.1, 1.0, 5.0, 10.0];
const FR_FMT: [&str; 2] = ["png", "jpeg"];
const ROTATE: [u32; 3] = [90, 180, 270];
const CONTAINER: [&str; 4] = ["mp4", "mkv", "webm", "mov"];
const VCODEC: [&str; 5] = ["h264", "hevc", "vp9", "av1", "copy"];
const ACODEC: [&str; 6] = ["aac", "mp3", "opus", "flac", "copy", "none"];
const HEIGHT: [u32; 7] = [0, 2160, 1440, 1080, 720, 480, 360];
const FPS: [u32; 5] = [0, 60, 30, 24, 15];
const CRF: [u32; 4] = [18, 23, 28, 32];
const VBITRATE: [u32; 5] = [1000, 2500, 5000, 8000, 16000];
const ABITRATE: [u32; 6] = [96, 128, 160, 192, 256, 320];
const SRATE: [u32; 3] = [0, 44100, 48000];
const THEME: [&str; 3] = ["system", "dark", "light"];
const LANG: [&str; 3] = ["system", "en", "fr"];

fn pos<T: PartialEq>(list: &[T], v: &T) -> usize {
    list.iter().position(|x| x == v).unwrap_or(0)
}

fn upper(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_ascii_uppercase()).collect()
}

impl App {
    /// The labels of a row and the chosen one.
    pub fn chips_of(&self, key: &str) -> (Vec<String>, usize) {
        let o = &self.set.opts;
        match key {
            "discord" => (DISCORD.iter().map(|m| format!("{m} MB")).collect(), pos(&DISCORD, &o.discord_mb)),
            "audio_fmt" => (upper(&AUDIO_FMT), pos(&AUDIO_FMT, &o.audio_fmt.as_str())),
            "gif_fps" => (GIF_FPS.iter().map(|f| format!("{f} fps")).collect(), pos(&GIF_FPS, &o.gif_fps)),
            "gif_width" => (GIF_W.iter().map(|w| format!("{w} px")).collect(), pos(&GIF_W, &o.gif_width)),
            "img_fmt" => (upper(&IMG_FMT), pos(&IMG_FMT, &o.img_fmt.as_str())),
            "img_quality" => (IMG_Q.iter().map(|q| q.to_string()).collect(), pos(&IMG_Q, &o.img_quality)),
            "img_max" => (IMG_MAX.iter().map(|m| if *m == 0 { tr("Same").to_string() } else { format!("{m} px") }).collect(), pos(&IMG_MAX, &o.img_max)),
            "frames_every" => (
                vec![tr("Every frame").into(), tr("10 a second").into(), tr("1 a second").into(), tr("Every 5 s").into(), tr("Every 10 s").into()],
                FR_EVERY.iter().position(|v| (v - o.frames_every).abs() < 1e-6).unwrap_or(2),
            ),
            "frames_fmt" => (upper(&FR_FMT), pos(&FR_FMT, &o.frames_fmt.as_str())),
            "trim_mode" => (vec![tr("Exact").into(), tr("Fast, no re-encode").into()], if o.trim_exact { 0 } else { 1 }),
            "rotate" => (vec![tr("90° right").into(), "180°".into(), tr("90° left").into()], pos(&ROTATE, &o.rotate)),
            "container" => (upper(&CONTAINER), pos(&CONTAINER, &o.container.as_str())),
            "vcodec" => (vec!["H.264".into(), "H.265".into(), "VP9".into(), "AV1".into(), tr("Copy").into()], pos(&VCODEC, &o.vcodec.as_str())),
            "acodec" => {
                (vec!["AAC".into(), "MP3".into(), "Opus".into(), "FLAC".into(), tr("Copy").into(), tr("None").into()], pos(&ACODEC, &o.acodec.as_str()))
            }
            "height" => (
                HEIGHT
                    .iter()
                    .map(|h| {
                        if *h == 0 {
                            tr("Same").to_string()
                        } else if *h == 2160 {
                            "4K".into()
                        } else {
                            format!("{h}p")
                        }
                    })
                    .collect(),
                pos(&HEIGHT, &o.height),
            ),
            "fps" => (FPS.iter().map(|f| if *f == 0 { tr("Same").to_string() } else { format!("{f}") }).collect(), pos(&FPS, &o.fps)),
            "rate_mode" => (vec![tr("Quality").into(), tr("Bitrate").into()], if o.vbitrate > 0 { 1 } else { 0 }),
            "crf" => {
                (vec![tr("High").into(), tr("Good").into(), tr("Smaller").into(), tr("Smallest").into()], CRF.iter().position(|c| *c == o.crf).unwrap_or(1))
            }
            "vbitrate" => (VBITRATE.iter().map(|k| format!("{} Mbps", *k as f32 / 1000.0)).collect(), pos(&VBITRATE, &o.vbitrate)),
            "abitrate" => (ABITRATE.iter().map(|k| format!("{k} kbps")).collect(), pos(&ABITRATE, &o.abitrate)),
            "sample_rate" => (vec![tr("Same").into(), "44.1 kHz".into(), "48 kHz".into()], pos(&SRATE, &o.sample_rate)),
            "theme" => (vec![tr("System").into(), tr("Dark").into(), tr("Light").into()], pos(&THEME, &self.set.theme.as_str())),
            "lang" => (vec![tr("System").into(), "English".into(), "Français".into()], pos(&LANG, &self.set.lang.as_str())),
            _ => (Vec::new(), 0),
        }
    }

    /// A click on the `i`-th chip of a row.
    pub fn chip(&mut self, key: &str, i: usize) {
        let o = &mut self.set.opts;
        match key {
            "discord" => o.discord_mb = DISCORD[i.min(2)],
            "audio_fmt" => o.audio_fmt = AUDIO_FMT[i.min(4)].into(),
            "gif_fps" => o.gif_fps = GIF_FPS[i.min(2)],
            "gif_width" => o.gif_width = GIF_W[i.min(3)],
            "img_fmt" => o.img_fmt = IMG_FMT[i.min(5)].into(),
            "img_quality" => o.img_quality = IMG_Q[i.min(4)],
            "img_max" => o.img_max = IMG_MAX[i.min(5)],
            "frames_every" => o.frames_every = FR_EVERY[i.min(4)],
            "frames_fmt" => o.frames_fmt = FR_FMT[i.min(1)].into(),
            "trim_mode" => o.trim_exact = i == 0,
            "rotate" => o.rotate = ROTATE[i.min(2)],
            "container" => {
                o.container = CONTAINER[i.min(3)].into();
                // WebM takes VP9 or AV1 with Opus
                if o.container == "webm" {
                    if !matches!(o.vcodec.as_str(), "vp9" | "av1") {
                        o.vcodec = "vp9".into();
                    }
                    if !matches!(o.acodec.as_str(), "opus" | "none") {
                        o.acodec = "opus".into();
                    }
                }
            }
            "vcodec" => o.vcodec = VCODEC[i.min(4)].into(),
            "acodec" => o.acodec = ACODEC[i.min(5)].into(),
            "height" => o.height = HEIGHT[i.min(6)],
            "fps" => o.fps = FPS[i.min(4)],
            "rate_mode" => o.vbitrate = if i == 1 { if o.vbitrate > 0 { o.vbitrate } else { 5000 } } else { 0 },
            "crf" => o.crf = CRF[i.min(3)],
            "vbitrate" => o.vbitrate = VBITRATE[i.min(4)],
            "abitrate" => o.abitrate = ABITRATE[i.min(5)],
            "sample_rate" => o.sample_rate = SRATE[i.min(2)],
            "theme" => self.set.theme = THEME[i.min(2)].into(),
            "lang" => self.set.lang = LANG[i.min(2)].into(),
            _ => {}
        }
    }
}
