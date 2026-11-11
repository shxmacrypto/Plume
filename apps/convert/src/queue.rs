//! One conversion, start to finish, on a worker thread: read what the file holds, take a free output name, run
//! FFmpeg (or WIC for pictures) while reporting the progress, fall back to software when the graphics card's
//! encoder fails, try again a little smaller when a size target was missed, and clean up after a cancel or an error.

use crate::{
    ffmpeg::{self, Ctx, Hw, Info, Progress},
    image, naming,
    opts::{Opts, Preset},
    tools,
};
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// What a job tells the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// 0..1.
    Progress(f64),
    /// A short line under the file's name ("Pass 1 of 2", "Graphics card failed: software"…).
    Note(String),
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub src: PathBuf,
    pub preset: Preset,
    pub opts: Opts,
    pub out_dir: Option<PathBuf>,
    pub keep_meta: bool,
    pub hw: bool,
}

/// Cancels a running job: the flag, and FFmpeg killed.
#[derive(Default)]
pub struct Ctl {
    cancel: AtomicBool,
    child: Mutex<Option<Child>>,
}

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

pub const CANCELLED: &str = "cancelled";

/// The finished file (or folder) and its size.
pub fn run(spec: &Spec, ctl: &Ctl, report: &mut dyn FnMut(Msg)) -> Result<(PathBuf, u64), String> {
    if spec.preset.is_image() {
        return run_picture(spec, ctl, report);
    }
    if ffmpeg::is_picture(&spec.src) {
        return Err("This is a picture: use the Images preset".into());
    }
    let info = ffmpeg::probe(&tools::ffprobe(), &spec.src)?;
    if ctl.cancelled() {
        return Err(CANCELLED.into());
    }
    let dir = naming::out_dir(&spec.src, spec.out_dir.as_deref());
    let (output, folder) = match ffmpeg::out_ext(spec.preset, &spec.opts, &spec.src, Some(&info)) {
        Some(ext) => (naming::reserve_file(&spec.src, &dir, &ext).map_err(|e| e.to_string())?, false),
        None => (naming::reserve_folder(&spec.src, &dir, "frames").map_err(|e| e.to_string())?, true),
    };
    let passlog = passlog_path();
    let r = encode(spec, ctl, report, &info, &output, &passlog);
    clean_passlog(&passlog);
    match r {
        Ok(()) => {
            let size = if folder { folder_size(&output) } else { std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0) };
            Ok((output, size))
        }
        Err(e) => {
            // only what this job created
            if folder {
                let _ = std::fs::remove_dir_all(&output);
            } else {
                let _ = std::fs::remove_file(&output);
            }
            Err(e)
        }
    }
}

fn encode(spec: &Spec, ctl: &Ctl, report: &mut dyn FnMut(Msg), info: &Info, output: &Path, passlog: &Path) -> Result<(), String> {
    let mut hw: Option<Hw> = None;
    if spec.hw && ffmpeg::uses_hw(spec.preset, &spec.opts, info) {
        hw = ffmpeg::hw_cached(&tools::ffmpeg());
        if let Some(h) = hw {
            report(Msg::Note(h.label().to_string()));
        }
    }
    let mut squeeze = 1.0;
    let mut tries = 0;
    loop {
        let c = Ctx { input: &spec.src, output, info, hw, keep_meta: spec.keep_meta, passlog, squeeze };
        let passes = ffmpeg::plan(spec.preset, &spec.opts, &c)?;
        match run_passes(&passes, ctl, report) {
            Ok(()) => {}
            Err(_) if ctl.cancelled() => return Err(CANCELLED.into()),
            Err(_) if hw.is_some() => {
                hw = None;
                report(Msg::Note("Graphics card encoder failed: using the processor".into()));
                report(Msg::Progress(0.0));
                continue;
            }
            Err(e) => return Err(e),
        }
        if spec.preset == Preset::Discord {
            let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            let target = ffmpeg::target_bytes(spec.opts.discord_mb);
            if size > target {
                tries += 1;
                if tries > 2 {
                    return Err(format!("Still {} after 3 tries", naming::size_text(size)));
                }
                squeeze *= 0.85 * target as f64 / size as f64;
                report(Msg::Note("A little too big: trying again smaller".into()));
                report(Msg::Progress(0.0));
                continue;
            }
        }
        report(Msg::Progress(1.0));
        return Ok(());
    }
}

fn run_passes(passes: &[ffmpeg::Pass], ctl: &Ctl, report: &mut dyn FnMut(Msg)) -> Result<(), String> {
    let shares: Vec<(f64, f64)> = passes.iter().map(|p| (p.weight, p.span)).collect();
    for (i, pass) in passes.iter().enumerate() {
        if passes.len() > 1 {
            report(Msg::Note(format!("Pass {} of {}", i + 1, passes.len())));
        }
        run_ffmpeg(&pass.args, ctl, &mut |t| report(Msg::Progress(ffmpeg::fraction(&shares, i, t))))?;
    }
    Ok(())
}

/// Runs FFmpeg hidden, at low priority; `at`: seconds of output written so far.
pub fn run_ffmpeg(args: &[String], ctl: &Ctl, at: &mut dyn FnMut(f64)) -> Result<(), String> {
    if ctl.cancelled() {
        return Err(CANCELLED.into());
    }
    let mut child = ffmpeg::command(&tools::ffmpeg()).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
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
        let mut p = Progress::default();
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            if ffmpeg::progress_line(&line, &mut p) {
                at(p.time);
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

/// The last meaningful line FFmpeg wrote.
pub fn last_error(stderr: &str) -> String {
    stderr.lines().map(str::trim).rfind(|l| !l.is_empty()).map(|l| l.chars().take(200).collect()).unwrap_or_else(|| "FFmpeg stopped with an error".into())
}

fn run_picture(spec: &Spec, ctl: &Ctl, report: &mut dyn FnMut(Msg)) -> Result<(PathBuf, u64), String> {
    let o = &spec.opts;
    let fmt = image::Fmt::from_name(&o.img_fmt);
    let ext = ffmpeg::out_ext(Preset::Images, o, &spec.src, None).unwrap_or_else(|| "webp".into());
    if fmt == image::Fmt::Webp && !tools::ready() {
        return Err("FFmpeg is needed for WebP".into());
    }
    let dir = naming::out_dir(&spec.src, spec.out_dir.as_deref());
    let output = naming::reserve_file(&spec.src, &dir, &ext).map_err(|e| e.to_string())?;
    report(Msg::Progress(0.3));
    let webp = |png: &Path, dst: &Path, q: u32| -> Result<(), String> { run_ffmpeg(&ffmpeg::webp_args(png, dst, q), ctl, &mut |_| {}) };
    match image::convert(&spec.src, &output, fmt, o.img_quality, o.img_max, &webp) {
        Ok(()) => {
            report(Msg::Progress(1.0));
            let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
            Ok((output, size))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&output);
            Err(if ctl.cancelled() { CANCELLED.into() } else { e })
        }
    }
}

fn passlog_path() -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!("plume-convert-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)))
}

/// x264 writes "<log>-0.log" and "<log>-0.log.mbtree".
fn clean_passlog(log: &Path) {
    let (Some(dir), Some(name)) = (log.parent(), log.file_name().map(|n| n.to_string_lossy().into_owned())) else { return };
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with(&format!("{name}-")) && n.contains(".log") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

fn folder_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir).map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0)
}

/// How many files a frames folder holds.
pub fn count_files(dir: &Path) -> usize {
    std::fs::read_dir(dir).map(|rd| rd.flatten().count()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_lines() {
        assert_eq!(last_error("a\nUnknown encoder 'h264_nvenc'\n\n"), "Unknown encoder 'h264_nvenc'");
        assert_eq!(last_error(""), "FFmpeg stopped with an error");
    }

    #[test]
    fn passlog_cleanup_only_takes_its_own() {
        let log = passlog_path();
        let dir = log.parent().unwrap();
        let name = log.file_name().unwrap().to_string_lossy().into_owned();
        let mine = dir.join(format!("{name}-0.log"));
        let other = dir.join(format!("{name}x-keep.log"));
        std::fs::write(&mine, b"x").unwrap();
        std::fs::write(&other, b"x").unwrap();
        clean_passlog(&log);
        assert!(!mine.exists() && other.exists());
        let _ = std::fs::remove_file(other);
    }

    #[test]
    fn cancel_before_start() {
        let ctl = Ctl::default();
        ctl.cancel();
        assert!(ctl.cancelled());
        assert_eq!(run_ffmpeg(&[], &ctl, &mut |_| {}), Err(CANCELLED.to_string()));
    }
}
