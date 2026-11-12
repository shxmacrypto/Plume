//! FFmpeg as Plume Cut uses it: what a file holds (ffprobe), single frames and filmstrips for the preview and the
//! timeline, the outline of a sound, the hardware encoders of this PC, and a run with its progress and a way to
//! stop it. FFmpeg always runs hidden, at low priority. Small parts are copies of Plume Convert's `ffmpeg.rs`.

use crate::{
    model::{Media, MediaKind},
    tools,
};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read},
    os::windows::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS: no console, and the PC stays smooth while it works.
pub const SPAWN_FLAGS: u32 = 0x0800_0000 | 0x0000_4000;

pub fn command(exe: &Path) -> Command {
    let mut c = Command::new(exe);
    c.creation_flags(SPAWN_FLAGS).stdin(Stdio::null());
    c
}

// ───────────────────────── probe ─────────────────────────

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

pub fn is_picture(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "jpe" | "jfif" | "bmp" | "tif" | "tiff" | "webp" | "heic" | "heif" | "avif")
}

/// ffprobe's JSON → what Plume Cut keeps of a file. `picture`: the file is a still image.
pub fn parse_probe(json: &str, picture: bool) -> Result<Media, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut m = Media { duration: num(&v["format"]["duration"]).unwrap_or(0.0), ..Media::default() };
    let (mut video, mut audio) = (false, false);
    for s in v["streams"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        match s["codec_type"].as_str() {
            Some("video") if !video && s["disposition"]["attached_pic"].as_i64() != Some(1) => {
                video = true;
                m.width = s["width"].as_u64().unwrap_or(0) as u32;
                m.height = s["height"].as_u64().unwrap_or(0) as u32;
                m.fps = rate(s["avg_frame_rate"].as_str().unwrap_or("0/0"));
                if m.fps <= 0.0 {
                    m.fps = rate(s["r_frame_rate"].as_str().unwrap_or("0/0"));
                }
                let mut rot = num(&s["tags"]["rotate"]).unwrap_or(0.0);
                for d in s["side_data_list"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                    if let Some(r) = num(&d["rotation"]) {
                        rot = r;
                    }
                }
                if (rot.abs() - 90.0).abs() < 1.0 || (rot.abs() - 270.0).abs() < 1.0 {
                    std::mem::swap(&mut m.width, &mut m.height);
                }
                if m.duration <= 0.0 {
                    m.duration = num(&s["duration"]).unwrap_or(0.0);
                }
            }
            Some("audio") if !audio => {
                audio = true;
                if m.duration <= 0.0 {
                    m.duration = num(&s["duration"]).unwrap_or(0.0);
                }
            }
            _ => {}
        }
    }
    m.has_audio = audio;
    m.kind = if video && (picture || m.duration <= 0.0 && !audio) {
        MediaKind::Image
    } else if video {
        MediaKind::Video
    } else if audio {
        MediaKind::Audio
    } else {
        return Err("No picture or sound in this file".into());
    };
    if m.kind == MediaKind::Image {
        m.duration = 0.0;
        m.has_audio = false;
    }
    if m.kind != MediaKind::Image && m.duration <= 0.0 {
        return Err("The length of this file is unknown".into());
    }
    Ok(m)
}

pub fn probe(file: &Path) -> Result<Media, String> {
    let out = command(&tools::ffprobe())
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let e = String::from_utf8_lossy(&out.stderr).trim().lines().last().unwrap_or("").to_string();
        return Err(if e.is_empty() { "Not a video, picture or sound".into() } else { e });
    }
    let mut m = parse_probe(&String::from_utf8_lossy(&out.stdout), is_picture(file))?;
    m.path = file.to_path_buf();
    Ok(m)
}

// ───────────────────────── frames ─────────────────────────

/// A picture in memory: BGRA, `w` × `h`.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

/// The size a picture of `sw` × `sh` takes inside `max` × `max` (even sides).
pub fn fit_box(sw: u32, sh: u32, max: u32) -> (u32, u32) {
    let (sw, sh) = (sw.max(1) as f64, sh.max(1) as f64);
    let k = (max as f64 / sw).min(max as f64 / sh).min(1.0);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    (even(sw * k), even(sh * k))
}

/// One frame of a file at `secs`, `w` × `h` (exact: FFmpeg decodes up to it).
pub fn grab(file: &Path, secs: f64, w: u32, h: u32) -> Option<Frame> {
    let mut c = command(&tools::ffmpeg());
    c.args(["-hide_banner", "-v", "error"]);
    if !is_picture(file) {
        c.args(["-ss", &crate::render::n(secs.max(0.0))]);
    }
    let out = c
        .arg("-i")
        .arg(file)
        .args(["-frames:v", "1", "-an", "-vf", &format!("scale={w}:{h}:flags=bilinear"), "-f", "rawvideo", "-pix_fmt", "bgra", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let need = (w * h * 4) as usize;
    (out.stdout.len() >= need).then(|| Frame { w, h, data: out.stdout[..need].to_vec() })
}

/// `count` small frames spread over a video (`h` high), for the timeline. One run of FFmpeg.
pub fn filmstrip(file: &Path, duration: f64, picture: bool, count: usize, w: u32, h: u32) -> Vec<Frame> {
    let need = (w * h * 4) as usize;
    let mut c = command(&tools::ffmpeg());
    c.args(["-hide_banner", "-v", "error"]);
    let vf = if picture || duration <= 0.0 {
        format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}")
    } else {
        let every = (duration / count.max(1) as f64).max(0.05);
        format!("fps=1/{},scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}", crate::render::n(every))
    };
    let out = c
        .arg("-i")
        .arg(file)
        .args(["-an", "-vf", &vf, "-frames:v", &count.max(1).to_string(), "-f", "rawvideo", "-pix_fmt", "bgra", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    match out {
        Ok(o) => o.stdout.chunks_exact(need).map(|d| Frame { w, h, data: d.to_vec() }).collect(),
        Err(_) => Vec::new(),
    }
}

/// The loudness outline of a file's sound: `per_sec` peaks (0..1) a second.
pub fn waveform(file: &Path, per_sec: u32) -> Vec<f32> {
    let rate = 4000u32;
    let out = command(&tools::ffmpeg())
        .args(["-hide_banner", "-v", "error", "-i"])
        .arg(file)
        .args(["-vn", "-ac", "1", "-ar", &rate.to_string(), "-f", "s16le", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(o) = out else { return Vec::new() };
    peaks(&o.stdout, (rate / per_sec.max(1)) as usize)
}

/// The highest level of each `chunk` samples of 16-bit sound.
pub fn peaks(pcm: &[u8], chunk: usize) -> Vec<f32> {
    let samples: Vec<i16> = pcm.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
    samples.chunks(chunk.max(1)).map(|c| c.iter().map(|v| (*v as f32).abs()).fold(0.0, f32::max) / 32768.0).collect()
}

// ───────────────────────── hardware ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hw {
    Nvenc,
    Qsv,
    Amf,
}

impl Hw {
    pub fn suffix(self) -> &'static str {
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
pub fn detect_hw() -> Option<Hw> {
    for hw in [Hw::Nvenc, Hw::Qsv, Hw::Amf] {
        let ok = command(&tools::ffmpeg())
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
pub fn hw_cached() -> Option<Hw> {
    *HW.get_or_init(detect_hw)
}

pub fn hw_known() -> Option<Option<Hw>> {
    HW.get().copied()
}

// ───────────────────────── a run ─────────────────────────

/// Stops a running FFmpeg: the flag, and the process killed.
#[derive(Default)]
pub struct Ctl {
    cancel: AtomicBool,
    child: Mutex<Option<Child>>,
}

pub const CANCELLED: &str = "cancelled";

impl Ctl {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// One line of `-progress pipe:1`: the seconds written so far, at the end of each block.
pub fn progress_line(line: &str, time: &mut f64) -> bool {
    let Some((k, v)) = line.trim().split_once('=') else { return false };
    match k {
        "out_time_us" | "out_time_ms" => {
            if let Ok(us) = v.trim().parse::<i64>() {
                *time = us.max(0) as f64 / 1_000_000.0;
            }
            false
        }
        "progress" => true,
        _ => false,
    }
}

/// The last meaningful line FFmpeg wrote.
pub fn last_error(stderr: &str) -> String {
    stderr.lines().map(str::trim).rfind(|l| !l.is_empty()).map(|l| l.chars().take(240).collect()).unwrap_or_else(|| "FFmpeg stopped with an error".into())
}

/// Runs FFmpeg in `dir`; `at` gets the seconds written.
pub fn run(args: &[String], dir: &Path, ctl: &Ctl, at: &mut dyn FnMut(f64)) -> Result<(), String> {
    if ctl.cancelled() {
        return Err(CANCELLED.into());
    }
    let mut child = command(&tools::ffmpeg()).args(args).current_dir(dir).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let out = child.stdout.take();
    let err = child.stderr.take();
    let errors = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = err {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    *ctl.child.lock().unwrap() = Some(child);
    if ctl.cancelled() {
        ctl.cancel();
    }
    if let Some(out) = out {
        let mut t = 0.0;
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            if progress_line(&line, &mut t) {
                at(t);
            }
        }
    }
    let status = ctl.child.lock().unwrap().take().map(|mut c| c.wait());
    let stderr = errors.join().unwrap_or_default();
    if ctl.cancelled() {
        return Err(CANCELLED.into());
    }
    match status {
        Some(Ok(s)) if s.success() => Ok(()),
        _ => Err(last_error(&stderr)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_parsing() {
        let j = r#"{"streams":[
            {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"avg_frame_rate":"30000/1001","side_data_list":[{"rotation":-90}]},
            {"codec_type":"audio","codec_name":"aac"}],
            "format":{"duration":"12.5"}}"#;
        let m = parse_probe(j, false).unwrap();
        assert_eq!((m.width, m.height, m.kind), (1080, 1920, MediaKind::Video));
        assert!((m.fps - 29.97).abs() < 0.01 && m.has_audio && m.duration == 12.5);
        let song = r#"{"streams":[{"codec_type":"audio"},{"codec_type":"video","disposition":{"attached_pic":1}}],"format":{"duration":"200"}}"#;
        assert_eq!(parse_probe(song, false).unwrap().kind, MediaKind::Audio);
        let png = r#"{"streams":[{"codec_type":"video","width":800,"height":600}],"format":{}}"#;
        let m = parse_probe(png, true).unwrap();
        assert_eq!((m.kind, m.duration, m.has_audio), (MediaKind::Image, 0.0, false));
        assert!(parse_probe(r#"{"streams":[],"format":{}}"#, false).is_err());
    }

    #[test]
    fn helpers() {
        assert_eq!(fit_box(1920, 1080, 640), (640, 360));
        assert_eq!(fit_box(1080, 1920, 640), (360, 640));
        assert_eq!(fit_box(100, 50, 640), (100, 50));
        assert_eq!(peaks(&[0, 0, 0, 64, 0, 0, 0, 0xc0], 2), vec![0.5, 0.5]);
        let mut t = 0.0;
        assert!(!progress_line("out_time_us=1500000", &mut t));
        assert!(progress_line("progress=continue", &mut t));
        assert_eq!(t, 1.5);
        assert_eq!(last_error("a\nb\n\n"), "b");
    }
}
