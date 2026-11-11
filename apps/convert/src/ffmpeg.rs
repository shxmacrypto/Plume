//! FFmpeg: what a file holds (ffprobe), the command lines of every preset, the progress FFmpeg reports
//! (`-progress pipe:1`), the bitrate that makes a video fit a size, and the hardware encoders of this PC.
//! FFmpeg itself runs as a hidden child process (see `queue`).

use crate::opts::{Opts, Preset};
use serde_json::Value;
use std::{
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::OnceLock,
};

/// CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS: no console, and the PC stays smooth while it works.
pub const SPAWN_FLAGS: u32 = 0x0800_0000 | 0x0000_4000;

pub fn command(exe: &Path) -> Command {
    let mut c = Command::new(exe);
    c.creation_flags(SPAWN_FLAGS).stdin(Stdio::null());
    c
}

// ───────────────────────── probe ─────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    /// Seconds (0 when unknown).
    pub duration: f64,
    /// As shown (a phone video's rotation already applied).
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_video: bool,
    pub has_audio: bool,
    pub vcodec: String,
    pub acodec: String,
    pub size: u64,
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// "30000/1001" → 29.97.
fn rate(s: &str) -> f64 {
    match s.split_once('/') {
        Some((a, b)) => {
            let (a, b) = (a.parse::<f64>().unwrap_or(0.0), b.parse::<f64>().unwrap_or(0.0));
            if b > 0.0 { a / b } else { 0.0 }
        }
        None => s.parse().unwrap_or(0.0),
    }
}

/// ffprobe's JSON (`-show_format -show_streams`) → Info.
pub fn parse_probe(json: &str) -> Result<Info, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut info = Info { duration: num(&v["format"]["duration"]).unwrap_or(0.0), size: num(&v["format"]["size"]).unwrap_or(0.0) as u64, ..Info::default() };
    for s in v["streams"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        match s["codec_type"].as_str() {
            // a cover inside an audio file is a "video" stream too: not a picture that moves
            Some("video") if !info.has_video && s["disposition"]["attached_pic"].as_i64() != Some(1) => {
                info.has_video = true;
                info.vcodec = s["codec_name"].as_str().unwrap_or("").to_string();
                info.width = s["width"].as_u64().unwrap_or(0) as u32;
                info.height = s["height"].as_u64().unwrap_or(0) as u32;
                info.fps = rate(s["avg_frame_rate"].as_str().unwrap_or("0/0"));
                if info.fps <= 0.0 {
                    info.fps = rate(s["r_frame_rate"].as_str().unwrap_or("0/0"));
                }
                let mut rot = num(&s["tags"]["rotate"]).unwrap_or(0.0);
                if let Some(list) = s["side_data_list"].as_array() {
                    for d in list {
                        if let Some(r) = num(&d["rotation"]) {
                            rot = r;
                        }
                    }
                }
                if (rot.abs() - 90.0).abs() < 1.0 || (rot.abs() - 270.0).abs() < 1.0 {
                    std::mem::swap(&mut info.width, &mut info.height);
                }
                if info.duration <= 0.0 {
                    info.duration = num(&s["duration"]).unwrap_or(0.0);
                }
            }
            Some("audio") if !info.has_audio => {
                info.has_audio = true;
                info.acodec = s["codec_name"].as_str().unwrap_or("").to_string();
                if info.duration <= 0.0 {
                    info.duration = num(&s["duration"]).unwrap_or(0.0);
                }
            }
            _ => {}
        }
    }
    if !info.has_video && !info.has_audio {
        return Err("No picture or sound found in this file".into());
    }
    Ok(info)
}

pub fn probe(ffprobe: &Path, file: &Path) -> Result<Info, String> {
    let out = command(ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let e = String::from_utf8_lossy(&out.stderr).trim().lines().last().unwrap_or("").to_string();
        return Err(if e.is_empty() { "Not a video or audio file".into() } else { e });
    }
    parse_probe(&String::from_utf8_lossy(&out.stdout))
}

/// Pictures go through WIC; everything else through FFmpeg (an animated GIF counts as a video).
pub fn is_picture(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "jpe" | "jfif" | "bmp" | "dib" | "tif" | "tiff" | "webp" | "heic" | "heif" | "avif" | "ico" | "jxr" | "wdp" | "dds"
    )
}

// ───────────────────────── sizes ─────────────────────────

fn even(v: f64) -> u32 {
    ((v / 2.0).round() as u32 * 2).max(2)
}

/// The shorter side at most `max` (1080 → "1080p", portrait or landscape), even sides for the encoders.
pub fn fit_short(w: u32, h: u32, max: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (w, h);
    }
    let short = w.min(h);
    if max == 0 || short <= max {
        return (even(w as f64), even(h as f64));
    }
    let k = max as f64 / short as f64;
    (even(w as f64 * k), even(h as f64 * k))
}

/// The longer side at most `max` (any parity: pictures and GIFs).
pub fn fit_long(w: u32, h: u32, max: u32) -> (u32, u32) {
    let long = w.max(h);
    if max == 0 || long <= max || w == 0 || h == 0 {
        return (w, h);
    }
    let k = max as f64 / long as f64;
    (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1))
}

// ───────────────────────── "For Discord": a size target ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    pub video_k: u32,
    pub audio_k: u32,
    /// The shorter side for that bitrate.
    pub short: u32,
}

/// Bytes allowed for "N MB" (decimal megabytes: what upload limits count, a little under the binary ones).
pub fn target_bytes(mb: u32) -> u64 {
    mb as u64 * 1_000_000
}

/// The bitrates that make `duration` seconds fit in `bytes`, with 5 % kept for the container. `squeeze` < 1
/// asks for less (a second try when the first was a little too big).
pub fn budget(bytes: u64, duration: f64, has_audio: bool, squeeze: f64) -> Result<Budget, String> {
    if duration <= 0.0 {
        return Err("The video's length is unknown".into());
    }
    let total = (bytes as f64 * 8.0 / duration / 1000.0 * 0.95 * squeeze).floor();
    let audio_k = if !has_audio {
        0
    } else if total >= 1500.0 {
        128
    } else if total >= 500.0 {
        96
    } else {
        64
    };
    let video = total - audio_k as f64;
    if video < 80.0 {
        return Err("Too long to fit in that size".into());
    }
    let video_k = (video as u32).min(12_000);
    let short = if video_k < 700 {
        480
    } else if video_k < 1800 {
        720
    } else {
        1080
    };
    Ok(Budget { video_k, audio_k, short })
}

// ───────────────────────── hardware ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hw {
    Nvenc,
    Qsv,
    Amf,
}

impl Hw {
    fn suffix(self) -> &'static str {
        match self {
            Hw::Nvenc => "nvenc",
            Hw::Qsv => "qsv",
            Hw::Amf => "amf",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Hw::Nvenc => "NVIDIA NVENC",
            Hw::Qsv => "Intel Quick Sync",
            Hw::Amf => "AMD AMF",
        }
    }
}

/// A half-second black clip encoded with each family: the first that works is this PC's.
pub fn detect_hw(ffmpeg: &Path) -> Option<Hw> {
    for hw in [Hw::Nvenc, Hw::Qsv, Hw::Amf] {
        let ok = command(ffmpeg)
            .args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i", "color=c=black:s=320x240:r=30:d=0.5", "-frames:v", "8"])
            .args(["-c:v", &format!("h264_{}", hw.suffix()), "-f", "null", "-"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return Some(hw);
        }
    }
    None
}

static HW: OnceLock<Option<Hw>> = OnceLock::new();

/// Detected once per run (on a worker thread: it takes about a second).
pub fn hw_cached(ffmpeg: &Path) -> Option<Hw> {
    *HW.get_or_init(|| detect_hw(ffmpeg))
}

pub fn hw_known() -> Option<Option<Hw>> {
    HW.get().copied()
}

// ───────────────────────── command lines ─────────────────────────

/// One run of FFmpeg: its arguments and its share of the progress bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub args: Vec<String>,
    /// Share of the whole job (the shares add up to 1).
    pub weight: f64,
    /// Seconds of media this pass goes through (for the percentage).
    pub span: f64,
}

/// Everything a plan needs to know.
pub struct Ctx<'a> {
    pub input: &'a Path,
    /// The file (or, for frames, the folder) to write.
    pub output: &'a Path,
    pub info: &'a Info,
    pub hw: Option<Hw>,
    pub keep_meta: bool,
    /// Two-pass log, without extension (a temp path).
    pub passlog: &'a Path,
    /// 1.0, or less on a second try at a size target.
    pub squeeze: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Q {
    Crf(u32),
    Kbps(u32),
}

fn s(v: &str) -> String {
    v.to_string()
}

fn push(a: &mut Vec<String>, items: &[&str]) {
    a.extend(items.iter().map(|x| x.to_string()));
}

fn head(input: &Path) -> Vec<String> {
    vec![
        s("-hide_banner"),
        s("-nostdin"),
        s("-y"),
        s("-v"),
        s("error"),
        s("-progress"),
        s("pipe:1"),
        s("-nostats"),
        s("-i"),
        input.to_string_lossy().into_owned(),
    ]
}

fn meta(a: &mut Vec<String>, keep: bool) {
    if keep {
        push(a, &["-map_metadata", "0"]);
    } else {
        push(a, &["-map_metadata", "-1", "-map_chapters", "-1"]);
    }
}

fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// The video encoder: software, or the PC's hardware one for H.264, HEVC and AV1.
fn venc(a: &mut Vec<String>, codec: &str, q: Q, hw: Option<Hw>, mp4_like: bool) {
    let hw = hw.filter(|_| matches!(codec, "h264" | "hevc" | "av1"));
    match (codec, hw) {
        ("copy", _) => push(a, &["-c:v", "copy"]),
        (_, Some(h)) => {
            let name = format!("{codec}_{}", h.suffix());
            push(a, &["-c:v", &name]);
            match (h, q) {
                (Hw::Nvenc, Q::Crf(n)) => push(a, &["-preset", "p5", "-rc", "vbr", "-cq", &n.to_string(), "-b:v", "0"]),
                (Hw::Qsv, Q::Crf(n)) => push(a, &["-preset", "medium", "-global_quality", &n.to_string()]),
                (Hw::Amf, Q::Crf(n)) => {
                    let n = n.to_string();
                    push(a, &["-quality", "balanced", "-rc", "cqp", "-qp_i", &n, "-qp_p", &n, "-qp_b", &n]);
                }
                (_, Q::Kbps(k)) => push(a, &["-b:v", &format!("{k}k")]),
            }
            push(a, &["-pix_fmt", if h == Hw::Nvenc { "yuv420p" } else { "nv12" }]);
            if codec == "hevc" && mp4_like {
                push(a, &["-tag:v", "hvc1"]);
            }
        }
        ("hevc", None) => {
            push(a, &["-c:v", "libx265", "-preset", "medium", "-x265-params", "log-level=error"]);
            match q {
                Q::Crf(n) => push(a, &["-crf", &n.to_string()]),
                Q::Kbps(k) => push(a, &["-b:v", &format!("{k}k")]),
            }
            push(a, &["-pix_fmt", "yuv420p"]);
            if mp4_like {
                push(a, &["-tag:v", "hvc1"]);
            }
        }
        ("vp9", None) | ("av1", None) => {
            if codec == "vp9" {
                push(a, &["-c:v", "libvpx-vp9", "-deadline", "good", "-cpu-used", "4", "-row-mt", "1"]);
            } else {
                push(a, &["-c:v", "libaom-av1", "-cpu-used", "6", "-row-mt", "1"]);
            }
            match q {
                // their scale runs to 63: about 10 above x264's for the same look
                Q::Crf(n) => push(a, &["-crf", &(n + 10).min(63).to_string(), "-b:v", "0"]),
                Q::Kbps(k) => push(a, &["-b:v", &format!("{k}k")]),
            }
            push(a, &["-pix_fmt", "yuv420p"]);
        }
        _ => {
            push(a, &["-c:v", "libx264", "-preset", "medium"]);
            match q {
                Q::Crf(n) => push(a, &["-crf", &n.to_string()]),
                Q::Kbps(k) => push(a, &["-b:v", &format!("{k}k")]),
            }
            push(a, &["-pix_fmt", "yuv420p"]);
        }
    }
}

fn aenc(a: &mut Vec<String>, codec: &str, kbps: u32, rate: u32) {
    let k = format!("{kbps}k");
    match codec {
        "none" => {
            push(a, &["-an"]);
            return;
        }
        "copy" => push(a, &["-c:a", "copy"]),
        "mp3" => push(a, &["-c:a", "libmp3lame", "-b:a", &k]),
        "opus" => push(a, &["-c:a", "libopus", "-b:a", &k]),
        "vorbis" => push(a, &["-c:a", "libvorbis", "-b:a", &k]),
        "flac" => push(a, &["-c:a", "flac"]),
        "pcm" => push(a, &["-c:a", "pcm_s16le"]),
        _ => push(a, &["-c:a", "aac", "-b:a", &k]),
    }
    if rate > 0 {
        push(a, &["-ar", &rate.to_string()]);
    }
}

/// The sound for an MP4: copied when MP4 takes it as it is, else AAC.
fn audio_for_mp4(a: &mut Vec<String>, info: &Info, kbps: u32) {
    if !info.has_audio {
        push(a, &["-an"]);
    } else if matches!(info.acodec.as_str(), "aac" | "mp3" | "alac" | "ac3") {
        push(a, &["-c:a", "copy"]);
    } else {
        aenc(a, "aac", kbps, 0);
    }
}

fn maps(a: &mut Vec<String>, info: &Info) {
    if info.has_video {
        push(a, &["-map", "0:v:0"]);
    }
    if info.has_audio {
        push(a, &["-map", "0:a:0?"]);
    }
}

fn scale_filter(info: &Info, w: u32, h: u32) -> Option<String> {
    (w != info.width || h != info.height).then(|| format!("scale={w}:{h}"))
}

fn vf(a: &mut Vec<String>, filters: Vec<String>) {
    if !filters.is_empty() {
        a.push(s("-vf"));
        a.push(filters.join(","));
    }
}

fn out(a: &mut Vec<String>, p: &Path) {
    a.push(p.to_string_lossy().into_owned());
}

fn need_video(info: &Info) -> Result<(), String> {
    if info.has_video { Ok(()) } else { Err("This needs a video".into()) }
}

/// The audio codec that goes with an audio file extension.
fn audio_codec_for_ext(ext: &str) -> &'static str {
    match ext {
        "mp3" => "mp3",
        "wav" => "pcm",
        "flac" => "flac",
        "ogg" | "oga" => "vorbis",
        "opus" => "opus",
        _ => "aac",
    }
}

/// The extension of the result (None: frames go in a folder).
pub fn out_ext(preset: Preset, o: &Opts, input: &Path, info: Option<&Info>) -> Option<String> {
    let src = ext_of(input);
    let audio_only = info.is_some_and(|i| !i.has_video);
    Some(match preset {
        Preset::Phone | Preset::Discord | Preset::Smaller => s("mp4"),
        Preset::Rotate => {
            if matches!(src.as_str(), "mkv" | "mov") {
                src
            } else {
                s("mp4")
            }
        }
        Preset::Audio => match o.audio_fmt.as_str() {
            f @ ("mp3" | "m4a" | "wav" | "flac" | "ogg") => s(f),
            _ => s("mp3"),
        },
        Preset::Gif => s("gif"),
        Preset::Images => match o.img_fmt.as_str() {
            "jpeg" | "jpg" => s("jpg"),
            "tiff" => s("tif"),
            f @ ("png" | "bmp" | "ico" | "webp") => s(f),
            _ => s("webp"),
        },
        Preset::Frames => return None,
        Preset::Trim if !o.trim_exact => src,
        Preset::Trim if audio_only => {
            if matches!(src.as_str(), "mp3" | "m4a" | "wav" | "flac" | "ogg" | "opus") {
                src
            } else {
                s("m4a")
            }
        }
        Preset::Trim => {
            if matches!(src.as_str(), "mp4" | "mov" | "m4v" | "mkv" | "webm") {
                src
            } else {
                s("mp4")
            }
        }
        Preset::Mute => src,
        Preset::Custom => match o.container.as_str() {
            c @ ("mp4" | "mkv" | "webm" | "mov") => s(c),
            _ => s("mp4"),
        },
    })
}

/// The FFmpeg runs of a preset (one, or two for a size target). Pictures ("Images") are not FFmpeg's.
pub fn plan(preset: Preset, o: &Opts, c: &Ctx) -> Result<Vec<Pass>, String> {
    let info = c.info;
    let dur = info.duration.max(0.0);
    let one = |args: Vec<String>, span: f64| Ok(vec![Pass { args, weight: 1.0, span }]);
    let mut a = head(c.input);
    let out_e = ext_of(c.output);
    let mp4_like = matches!(out_e.as_str(), "mp4" | "mov" | "m4v");
    match preset {
        Preset::Phone | Preset::Smaller => {
            need_video(info)?;
            let (short, crf, ak) = if preset == Preset::Phone { (1080, 23, 160) } else { (1080, 28, 128) };
            let (w, h) = fit_short(info.width, info.height, short);
            maps(&mut a, info);
            let mut f: Vec<String> = scale_filter(info, w, h).into_iter().collect();
            if info.fps > 60.5 {
                f.push(s("fps=60"));
            }
            vf(&mut a, f);
            let crf = if c.hw.is_some() { crf + 2 } else { crf };
            venc(&mut a, "h264", Q::Crf(crf), c.hw, true);
            if preset == Preset::Phone {
                push(&mut a, &["-profile:v", "high"]);
            }
            if info.has_audio {
                aenc(&mut a, "aac", ak, 0);
            }
            meta(&mut a, c.keep_meta);
            push(&mut a, &["-movflags", "+faststart"]);
            out(&mut a, c.output);
            one(a, dur)
        }
        Preset::Discord => {
            need_video(info)?;
            let b = budget(target_bytes(o.discord_mb), dur, info.has_audio, c.squeeze)?;
            let (w, h) = fit_short(info.width, info.height, b.short);
            let mut f: Vec<String> = scale_filter(info, w, h).into_iter().collect();
            if info.fps > 30.5 && b.video_k < 2500 {
                f.push(s("fps=30"));
            }
            let log = c.passlog.to_string_lossy().into_owned();
            let mut p1 = head(c.input);
            push(&mut p1, &["-map", "0:v:0"]);
            vf(&mut p1, f.clone());
            venc(&mut p1, "h264", Q::Kbps(b.video_k), None, true);
            push(&mut p1, &["-pass", "1", "-passlogfile", &log, "-an", "-f", "null", "NUL"]);
            let mut p2 = a;
            maps(&mut p2, info);
            vf(&mut p2, f);
            venc(&mut p2, "h264", Q::Kbps(b.video_k), None, true);
            push(&mut p2, &["-pass", "2", "-passlogfile", &log]);
            if info.has_audio {
                aenc(&mut p2, "aac", b.audio_k, 0);
            }
            meta(&mut p2, c.keep_meta);
            push(&mut p2, &["-movflags", "+faststart"]);
            out(&mut p2, c.output);
            Ok(vec![Pass { args: p1, weight: 0.4, span: dur }, Pass { args: p2, weight: 0.6, span: dur }])
        }
        Preset::Audio => {
            if !info.has_audio {
                return Err("This file has no sound".into());
            }
            push(&mut a, &["-map", "0:a:0", "-vn"]);
            match o.audio_fmt.as_str() {
                "m4a" => aenc(&mut a, "aac", 192, 0),
                "wav" => aenc(&mut a, "pcm", 0, 0),
                "flac" => aenc(&mut a, "flac", 0, 0),
                "ogg" => push(&mut a, &["-c:a", "libvorbis", "-q:a", "5"]),
                _ => push(&mut a, &["-c:a", "libmp3lame", "-q:a", "2", "-id3v2_version", "3"]),
            }
            meta(&mut a, c.keep_meta);
            out(&mut a, c.output);
            one(a, dur)
        }
        Preset::Gif => {
            need_video(info)?;
            let (w, h) = fit_long(info.width, info.height, o.gif_width.max(16));
            let fps = o.gif_fps.clamp(1, 50);
            let graph = format!(
                "[0:v:0]fps={fps},scale={w}:{h}:flags=lanczos,split[a][b];[a]palettegen=max_colors=256:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle[gif]"
            );
            push(&mut a, &["-filter_complex", &graph, "-map", "[gif]", "-loop", "0"]);
            out(&mut a, c.output);
            one(a, dur)
        }
        Preset::Frames => {
            need_video(info)?;
            push(&mut a, &["-map", "0:v:0"]);
            if o.frames_every > 0.0 {
                push(&mut a, &["-vf", &format!("fps=1/{}", trim_num(o.frames_every))]);
            }
            push(&mut a, &["-fps_mode", "vfr"]);
            let jpeg = o.frames_fmt == "jpeg";
            if jpeg {
                push(&mut a, &["-q:v", "2"]);
            }
            let pattern = c.output.join(if jpeg { "frame_%05d.jpg" } else { "frame_%05d.png" });
            out(&mut a, &pattern);
            one(a, dur)
        }
        Preset::Trim => {
            let start = o.trim_start.max(0.0);
            if dur > 0.0 && start >= dur {
                return Err("The start is after the end of the file".into());
            }
            let end = if o.trim_end > 0.0 { o.trim_end.min(if dur > 0.0 { dur } else { f64::MAX }) } else { dur };
            if o.trim_end > 0.0 && end <= start {
                return Err("The end must come after the start".into());
            }
            // the seek goes before the input: fast, and exact when encoding again
            let at = a.iter().position(|x| x == "-i").unwrap_or(a.len());
            a.splice(at..at, [s("-ss"), trim_num(start)]);
            let span = (end - start).max(0.0);
            if o.trim_end > 0.0 {
                push(&mut a, &["-t", &trim_num(span)]);
            }
            maps(&mut a, info);
            if !o.trim_exact {
                push(&mut a, &["-c", "copy", "-avoid_negative_ts", "make_zero"]);
            } else if !info.has_video {
                aenc(&mut a, audio_codec_for_ext(&out_e), 192, 0);
            } else if out_e == "webm" {
                venc(&mut a, "vp9", Q::Crf(20), None, false);
                if info.has_audio {
                    aenc(&mut a, "opus", 160, 0);
                }
            } else {
                venc(&mut a, "h264", Q::Crf(18), c.hw, mp4_like);
                if info.has_audio {
                    aenc(&mut a, "aac", 192, 0);
                }
            }
            meta(&mut a, c.keep_meta);
            if mp4_like {
                push(&mut a, &["-movflags", "+faststart"]);
            }
            out(&mut a, c.output);
            one(a, span)
        }
        Preset::Mute => {
            need_video(info)?;
            push(&mut a, &["-map", "0:v:0", "-c:v", "copy", "-an"]);
            meta(&mut a, c.keep_meta);
            out(&mut a, c.output);
            one(a, dur)
        }
        Preset::Rotate => {
            need_video(info)?;
            let f = match o.rotate {
                180 => "hflip,vflip",
                270 => "transpose=2",
                _ => "transpose=1",
            };
            maps(&mut a, info);
            push(&mut a, &["-vf", f]);
            venc(&mut a, "h264", Q::Crf(18), c.hw, mp4_like);
            if out_e == "mkv" {
                if info.has_audio {
                    push(&mut a, &["-c:a", "copy"]);
                }
            } else {
                audio_for_mp4(&mut a, info, 192);
            }
            meta(&mut a, c.keep_meta);
            if mp4_like {
                push(&mut a, &["-movflags", "+faststart"]);
            }
            out(&mut a, c.output);
            one(a, dur)
        }
        Preset::Custom => {
            let webm = out_e == "webm";
            if webm && info.has_video && !matches!(o.vcodec.as_str(), "vp9" | "av1") {
                return Err("WebM needs VP9 or AV1".into());
            }
            if webm && info.has_audio && !matches!(o.acodec.as_str(), "opus" | "none") {
                return Err("WebM needs Opus sound".into());
            }
            maps(&mut a, info);
            if info.has_video && o.vcodec != "copy" {
                let (w, h) = fit_short(info.width, info.height, o.height);
                let mut f: Vec<String> = scale_filter(info, w, h).into_iter().collect();
                if o.fps > 0 && (info.fps - o.fps as f64).abs() > 0.01 {
                    f.push(format!("fps={}", o.fps));
                }
                vf(&mut a, f);
                let q = if o.vbitrate > 0 { Q::Kbps(o.vbitrate) } else { Q::Crf(o.crf.clamp(0, 51)) };
                venc(&mut a, &o.vcodec, q, c.hw, mp4_like);
            } else if info.has_video {
                push(&mut a, &["-c:v", "copy"]);
            }
            if info.has_audio {
                aenc(&mut a, &o.acodec, o.abitrate.max(32), o.sample_rate);
            }
            meta(&mut a, c.keep_meta);
            if mp4_like {
                push(&mut a, &["-movflags", "+faststart"]);
            }
            out(&mut a, c.output);
            one(a, dur)
        }
        Preset::Images => Err("Pictures are not converted by FFmpeg".into()),
    }
}

/// True when the plan encodes video with the PC's hardware (so a failure can be retried in software).
pub fn uses_hw(preset: Preset, o: &Opts, info: &Info) -> bool {
    info.has_video
        && match preset {
            Preset::Phone | Preset::Smaller | Preset::Rotate => true,
            Preset::Trim => o.trim_exact,
            Preset::Custom => matches!(o.vcodec.as_str(), "h264" | "hevc" | "av1"),
            _ => false,
        }
}

/// A PNG (WIC wrote it) to WebP.
pub fn webp_args(png: &Path, output: &Path, quality: u32) -> Vec<String> {
    let mut a = head(png);
    a.retain(|x| x != "-progress" && x != "pipe:1");
    push(&mut a, &["-frames:v", "1", "-c:v", "libwebp"]);
    if quality >= 100 {
        push(&mut a, &["-lossless", "1"]);
    } else {
        push(&mut a, &["-quality", &quality.clamp(1, 100).to_string()]);
    }
    out(&mut a, output);
    a
}

/// 1.0 → "1", 2.5 → "2.5", 0.125 → "0.125".
fn trim_num(v: f64) -> String {
    let t = format!("{v:.3}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ───────────────────────── progress ─────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    /// Seconds of output written.
    pub time: f64,
    /// "1.5x" → 1.5 (0: unknown).
    pub speed: f64,
    pub end: bool,
}

/// One line of `-progress pipe:1`. True at the end of a block ("progress=continue" / "progress=end").
pub fn progress_line(line: &str, p: &mut Progress) -> bool {
    let Some((k, v)) = line.trim().split_once('=') else { return false };
    let v = v.trim();
    match k {
        // both are microseconds (FFmpeg's "ms" is a historical misnomer)
        "out_time_us" | "out_time_ms" => {
            if let Ok(us) = v.parse::<i64>() {
                p.time = (us.max(0) as f64) / 1_000_000.0;
            }
        }
        "out_time" => {
            if let Some(t) = crate::opts::parse_time(v) {
                p.time = t;
            }
        }
        "speed" => {
            if let Ok(x) = v.trim_end_matches('x').trim().parse::<f64>() {
                p.speed = x;
            }
        }
        "progress" => {
            p.end = v == "end";
            return true;
        }
        _ => {}
    }
    false
}

/// The part done, 0..1, of a job made of passes: `pass` runs and is at `time` seconds of its `span`.
pub fn fraction(passes: &[(f64, f64)], pass: usize, time: f64) -> f64 {
    let before: f64 = passes.iter().take(pass).map(|p| p.0).sum();
    let (weight, span) = passes.get(pass).copied().unwrap_or((0.0, 0.0));
    let inner = if span > 0.0 { (time / span).clamp(0.0, 1.0) } else { 0.0 };
    (before + weight * inner).clamp(0.0, 1.0)
}

/// Seconds left, from the time spent and the part done (None until it can be told).
pub fn time_left(elapsed: f64, frac: f64) -> Option<f64> {
    (frac > 0.02 && elapsed > 0.8).then(|| elapsed * (1.0 - frac) / frac)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn video() -> Info {
        Info {
            duration: 60.0,
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_video: true,
            has_audio: true,
            vcodec: "h264".into(),
            acodec: "aac".into(),
            size: 50_000_000,
        }
    }

    fn run(preset: Preset, o: &Opts, info: &Info, hw: Option<Hw>, out_name: &str) -> Result<Vec<Pass>, String> {
        let input = PathBuf::from(r"C:\in\clip.mov");
        let output = PathBuf::from(format!(r"C:\out\{out_name}"));
        let log = PathBuf::from(r"C:\tmp\pass");
        plan(preset, o, &Ctx { input: &input, output: &output, info, hw, keep_meta: true, passlog: &log, squeeze: 1.0 })
    }

    fn joined(p: &[Pass]) -> Vec<String> {
        p.iter().map(|x| x.args.join(" ")).collect()
    }

    fn has(args: &str, part: &str) -> bool {
        args.contains(part)
    }

    #[test]
    fn probe_parsing() {
        let j = r#"{"streams":[
            {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"avg_frame_rate":"30000/1001","side_data_list":[{"rotation":-90}]},
            {"codec_type":"audio","codec_name":"aac"},
            {"codec_type":"video","codec_name":"mjpeg","disposition":{"attached_pic":1}}],
            "format":{"duration":"12.5","size":"1000"}}"#;
        let i = parse_probe(j).unwrap();
        assert_eq!((i.width, i.height), (1080, 1920));
        assert!((i.fps - 29.97).abs() < 0.01);
        assert!(i.has_video && i.has_audio);
        assert_eq!(i.duration, 12.5);
        assert_eq!(i.size, 1000);
        // a song with its cover is not a video
        let j = r#"{"streams":[{"codec_type":"audio","codec_name":"mp3"},{"codec_type":"video","codec_name":"png","disposition":{"attached_pic":1}}],"format":{"duration":"200"}}"#;
        let i = parse_probe(j).unwrap();
        assert!(!i.has_video && i.has_audio);
        assert!(parse_probe(r#"{"streams":[],"format":{}}"#).is_err());
        assert!(parse_probe("not json").is_err());
    }

    #[test]
    fn sizes() {
        assert_eq!(fit_short(1920, 1080, 1080), (1920, 1080));
        assert_eq!(fit_short(3840, 2160, 1080), (1920, 1080));
        assert_eq!(fit_short(1080, 1920, 720), (720, 1280));
        assert_eq!(fit_short(1281, 721, 0), (1282, 722));
        assert_eq!(fit_short(1920, 1080, 480), (854, 480));
        assert_eq!(fit_long(1920, 1080, 480), (480, 270));
        assert_eq!(fit_long(300, 200, 480), (300, 200));
        assert_eq!(fit_long(1000, 4000, 1000), (250, 1000));
    }

    #[test]
    fn size_target_math() {
        // 10 MB for 60 s: about 1267 kbit/s in all
        let b = budget(target_bytes(10), 60.0, true, 1.0).unwrap();
        assert_eq!(b.audio_k, 96);
        assert_eq!(b.video_k, 1266 - 96);
        assert_eq!(b.short, 720);
        let total = (b.video_k + b.audio_k) as f64 * 1000.0 / 8.0 * 60.0;
        assert!(total <= 10_000_000.0 * 0.95);
        // a short clip: capped, full HD
        let b = budget(target_bytes(50), 5.0, true, 1.0).unwrap();
        assert_eq!((b.video_k, b.audio_k, b.short), (12_000, 128, 1080));
        // no sound: all for the picture
        assert_eq!(budget(target_bytes(10), 60.0, false, 1.0).unwrap().audio_k, 0);
        // a second try asks for less
        assert!(budget(target_bytes(10), 60.0, true, 0.85).unwrap().video_k < 1170);
        // an hour does not fit in 10 MB
        assert!(budget(target_bytes(10), 3600.0, true, 1.0).is_err());
        assert!(budget(target_bytes(10), 0.0, true, 1.0).is_err());
        // low bitrates make smaller pictures
        assert_eq!(budget(target_bytes(10), 200.0, true, 1.0).unwrap().short, 480);
    }

    #[test]
    fn phone() {
        let mut big = video();
        big.width = 3840;
        big.height = 2160;
        let p = run(Preset::Phone, &Opts::default(), &big, None, "clip (converted).mp4").unwrap();
        assert_eq!(p.len(), 1);
        let a = &joined(&p)[0];
        for part in [
            "-progress pipe:1",
            "-i C:\\in\\clip.mov",
            "-vf scale=1920:1080",
            "-c:v libx264",
            "-crf 23",
            "-pix_fmt yuv420p",
            "-c:a aac -b:a 160k",
            "-movflags +faststart",
            "-map_metadata 0",
        ] {
            assert!(has(a, part), "{part} missing in {a}");
        }
        assert!(a.ends_with(r"C:\out\clip (converted).mp4"));
        // already 1080p: no scaling; with NVENC
        let a = &joined(&run(Preset::Phone, &Opts::default(), &video(), Some(Hw::Nvenc), "x.mp4").unwrap())[0];
        assert!(!has(a, "scale="));
        assert!(has(a, "-c:v h264_nvenc") && has(a, "-cq 25"));
        assert!(run(Preset::Phone, &Opts::default(), &Info { has_video: false, ..video() }, None, "x.mp4").is_err());
    }

    #[test]
    fn discord_two_pass() {
        let o = Opts { discord_mb: 10, ..Opts::default() };
        let p = run(Preset::Discord, &o, &video(), Some(Hw::Nvenc), "x.mp4").unwrap();
        assert_eq!(p.len(), 2);
        assert!((p[0].weight + p[1].weight - 1.0).abs() < 1e-9);
        let j = joined(&p);
        assert!(has(&j[0], "-pass 1") && has(&j[0], "-an -f null NUL") && has(&j[0], r"-passlogfile C:\tmp\pass"));
        assert!(has(&j[1], "-pass 2") && has(&j[1], "-b:v 1170k") && has(&j[1], "-c:a aac -b:a 96k"));
        // size targets stay in software (exact bitrate)
        assert!(has(&j[1], "libx264") && !has(&j[1], "nvenc"));
        assert!(has(&j[1], "scale=1280:720"));
    }

    #[test]
    fn smaller_audio_gif() {
        let a = &joined(&run(Preset::Smaller, &Opts::default(), &video(), None, "x.mp4").unwrap())[0];
        assert!(has(a, "-crf 28") && has(a, "-b:a 128k"));
        for (fmt, part) in
            [("mp3", "-c:a libmp3lame -q:a 2"), ("m4a", "-c:a aac -b:a 192k"), ("wav", "-c:a pcm_s16le"), ("flac", "-c:a flac"), ("ogg", "-c:a libvorbis")]
        {
            let o = Opts { audio_fmt: fmt.into(), ..Opts::default() };
            let a = &joined(&run(Preset::Audio, &o, &video(), None, "x").unwrap())[0];
            assert!(has(a, part) && has(a, "-vn"), "{fmt}: {a}");
            assert_eq!(out_ext(Preset::Audio, &o, Path::new("a.mov"), None).as_deref(), Some(fmt));
        }
        assert!(run(Preset::Audio, &Opts::default(), &Info { has_audio: false, ..video() }, None, "x").is_err());
        let o = Opts { gif_fps: 12, gif_width: 480, ..Opts::default() };
        let a = &joined(&run(Preset::Gif, &o, &video(), None, "x.gif").unwrap())[0];
        assert!(
            has(a, "fps=12,scale=480:270:flags=lanczos") && has(a, "palettegen") && has(a, "paletteuse") && has(a, "-map [gif] -loop 0") && !has(a, "-map 0:v")
        );
    }

    #[test]
    fn frames_trim_mute_rotate() {
        let o = Opts { frames_every: 5.0, ..Opts::default() };
        let a = &joined(&run(Preset::Frames, &o, &video(), None, "clip (frames)").unwrap())[0];
        assert!(has(a, "-vf fps=1/5") && a.ends_with(r"C:\out\clip (frames)\frame_%05d.png"));
        let o = Opts { frames_every: 0.0, frames_fmt: "jpeg".into(), ..Opts::default() };
        let a = &joined(&run(Preset::Frames, &o, &video(), None, "f").unwrap())[0];
        assert!(!has(a, "fps=1/") && a.ends_with("frame_%05d.jpg"));

        let o = Opts { trim_start: 2.5, trim_end: 10.0, ..Opts::default() };
        let p = run(Preset::Trim, &o, &video(), None, "x.mov").unwrap();
        let a = &joined(&p)[0];
        assert!(has(a, r"-ss 2.5 -i C:\in\clip.mov") && has(a, "-t 7.5") && has(a, "-crf 18"));
        assert_eq!(p[0].span, 7.5);
        let fast = Opts { trim_exact: false, ..o.clone() };
        assert!(has(&joined(&run(Preset::Trim, &fast, &video(), None, "x.mov").unwrap())[0], "-c copy"));
        assert!(run(Preset::Trim, &Opts { trim_start: 9.0, trim_end: 3.0, ..Opts::default() }, &video(), None, "x").is_err());
        assert!(run(Preset::Trim, &Opts { trim_start: 99.0, ..Opts::default() }, &video(), None, "x").is_err());
        // to the end
        let p = run(Preset::Trim, &Opts { trim_start: 20.0, trim_end: 0.0, ..Opts::default() }, &video(), None, "x.mp4").unwrap();
        assert!(!joined(&p)[0].contains(" -t "));
        assert_eq!(p[0].span, 40.0);
        // sound only: the codec of the extension
        let song = Info { has_video: false, width: 0, height: 0, ..video() };
        assert!(has(&joined(&run(Preset::Trim, &o, &song, None, "x.mp3").unwrap())[0], "libmp3lame"));
        assert_eq!(out_ext(Preset::Trim, &o, Path::new("a.wma"), Some(&song)).as_deref(), Some("m4a"));
        assert_eq!(out_ext(Preset::Trim, &o, Path::new("a.avi"), Some(&video())).as_deref(), Some("mp4"));

        let a = &joined(&run(Preset::Mute, &Opts::default(), &video(), None, "x.mov").unwrap())[0];
        assert!(has(a, "-c:v copy -an"));
        assert_eq!(out_ext(Preset::Mute, &Opts::default(), Path::new("a.MKV"), None).as_deref(), Some("mkv"));

        for (deg, f) in [(90, "transpose=1"), (180, "hflip,vflip"), (270, "transpose=2")] {
            let o = Opts { rotate: deg, ..Opts::default() };
            let a = &joined(&run(Preset::Rotate, &o, &video(), None, "x.mp4").unwrap())[0];
            assert!(has(a, f) && has(a, "-c:a copy"), "{deg}: {a}");
        }
        let opus = Info { acodec: "opus".into(), ..video() };
        assert!(has(&joined(&run(Preset::Rotate, &Opts::default(), &opus, None, "x.mp4").unwrap())[0], "-c:a aac"));
    }

    #[test]
    fn custom() {
        let o = Opts {
            container: "mkv".into(),
            vcodec: "hevc".into(),
            acodec: "opus".into(),
            height: 720,
            fps: 24,
            crf: 26,
            vbitrate: 0,
            abitrate: 128,
            sample_rate: 48000,
            ..Opts::default()
        };
        let a = &joined(&run(Preset::Custom, &o, &video(), None, "x.mkv").unwrap())[0];
        for part in ["scale=1280:720,fps=24", "-c:v libx265", "-crf 26", "-c:a libopus -b:a 128k -ar 48000"] {
            assert!(has(a, part), "{part} missing in {a}");
        }
        assert!(!has(a, "hvc1"));
        let o2 = Opts { vbitrate: 5000, container: "mp4".into(), ..o.clone() };
        let a = &joined(&run(Preset::Custom, &o2, &video(), Some(Hw::Qsv), "x.mp4").unwrap())[0];
        assert!(has(a, "-c:v hevc_qsv") && has(a, "-b:v 5000k") && has(a, "-tag:v hvc1"));
        let amf = &joined(&run(Preset::Custom, &Opts { vbitrate: 0, ..o2.clone() }, &video(), Some(Hw::Amf), "x.mp4").unwrap())[0];
        assert!(has(amf, "-c:v hevc_amf") && has(amf, "-qp_i 26"));
        // VP9 never uses the hardware path
        let vp9 = Opts { container: "webm".into(), vcodec: "vp9".into(), acodec: "opus".into(), ..Opts::default() };
        let a = &joined(&run(Preset::Custom, &vp9, &video(), Some(Hw::Nvenc), "x.webm").unwrap())[0];
        assert!(has(a, "libvpx-vp9") && has(a, "-crf 33 -b:v 0"));
        // WebM with H.264 is refused
        assert!(run(Preset::Custom, &Opts { container: "webm".into(), ..Opts::default() }, &video(), None, "x.webm").is_err());
        let copy = Opts { vcodec: "copy".into(), acodec: "none".into(), ..Opts::default() };
        let a = &joined(&run(Preset::Custom, &copy, &video(), None, "x.mp4").unwrap())[0];
        assert!(has(a, "-c:v copy") && has(a, "-an") && !has(a, "scale"));
        assert!(uses_hw(Preset::Custom, &Opts::default(), &video()) && !uses_hw(Preset::Custom, &vp9, &video()));
        assert!(!uses_hw(Preset::Discord, &Opts::default(), &video()));
    }

    #[test]
    fn metadata_strip() {
        let input = PathBuf::from("a.mp4");
        let output = PathBuf::from("b.mp4");
        let log = PathBuf::from("l");
        let info = video();
        let p = plan(
            Preset::Phone,
            &Opts::default(),
            &Ctx { input: &input, output: &output, info: &info, hw: None, keep_meta: false, passlog: &log, squeeze: 1.0 },
        )
        .unwrap();
        assert!(p[0].args.join(" ").contains("-map_metadata -1 -map_chapters -1"));
    }

    #[test]
    fn extensions() {
        let o = Opts::default();
        let i = Path::new("a.webm");
        assert_eq!(out_ext(Preset::Phone, &o, i, None).as_deref(), Some("mp4"));
        assert_eq!(out_ext(Preset::Gif, &o, i, None).as_deref(), Some("gif"));
        assert_eq!(out_ext(Preset::Frames, &o, i, None), None);
        assert_eq!(out_ext(Preset::Rotate, &o, Path::new("a.mkv"), None).as_deref(), Some("mkv"));
        for (f, e) in [("jpeg", "jpg"), ("png", "png"), ("webp", "webp"), ("bmp", "bmp"), ("tiff", "tif"), ("ico", "ico")] {
            assert_eq!(out_ext(Preset::Images, &Opts { img_fmt: f.into(), ..o.clone() }, Path::new("a.heic"), None).as_deref(), Some(e));
        }
        assert!(is_picture(Path::new("x.HEIC")) && is_picture(Path::new("x.jpg")) && !is_picture(Path::new("x.gif")) && !is_picture(Path::new("x.mp4")));
        let w = webp_args(Path::new("t.png"), Path::new("o.webp"), 80).join(" ");
        assert!(w.contains("-c:v libwebp -quality 80") && !w.contains("-progress"));
        assert!(webp_args(Path::new("t.png"), Path::new("o.webp"), 100).join(" ").contains("-lossless 1"));
    }

    #[test]
    fn progress_parsing() {
        let mut p = Progress::default();
        let block = "frame=10\nfps=0.0\nout_time_us=2500000\nout_time_ms=2500000\nout_time=00:00:02.500000\nspeed=1.25x\nprogress=continue\n";
        let mut ends = 0;
        for l in block.lines() {
            if progress_line(l, &mut p) {
                ends += 1;
            }
        }
        assert_eq!(ends, 1);
        assert_eq!(p.time, 2.5);
        assert_eq!(p.speed, 1.25);
        assert!(!p.end);
        progress_line("out_time_us=N/A", &mut p);
        assert_eq!(p.time, 2.5);
        progress_line("speed=N/A", &mut p);
        assert!(progress_line("progress=end", &mut p) && p.end);
        progress_line("out_time=00:01:02.5", &mut p);
        assert_eq!(p.time, 62.5);
        progress_line("out_time_us=-5", &mut p);
        assert_eq!(p.time, 0.0);
        // two passes: 40 % then 60 %
        let passes = [(0.4, 10.0), (0.6, 10.0)];
        assert!((fraction(&passes, 0, 5.0) - 0.2).abs() < 1e-9);
        assert!((fraction(&passes, 1, 5.0) - 0.7).abs() < 1e-9);
        assert_eq!(fraction(&passes, 1, 50.0), 1.0);
        assert_eq!(fraction(&[(1.0, 0.0)], 0, 3.0), 0.0);
        assert_eq!(time_left(10.0, 0.5), Some(10.0));
        assert_eq!(time_left(10.0, 0.01), None);
    }
}
