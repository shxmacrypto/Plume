//! An export, start to finish, on a worker thread: take a free file name (never an existing file), prepare FFmpeg's
//! folder (graph, texts, fonts), encode with the graphics card when asked and fall back to the processor if it
//! fails, report the progress, and remove what was made after a cancel or an error.

use crate::{
    ffmpeg::{self, Ctl, Hw},
    model::Project,
    render::{self, Codec, Target},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// The export's choices.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    /// Shorter side: 720, 1080, 2160.
    pub short: u32,
    /// Frame shape; None: the project's.
    pub aspect: Option<[u32; 2]>,
    pub codec: Codec,
    pub hw: bool,
    pub dir: PathBuf,
}

/// The aspect presets: key, frame, label.
pub const ASPECTS: [(&str, Option<[u32; 2]>); 4] = [("project", None), ("16:9", Some([16, 9])), ("9:16", Some([9, 16])), ("1:1", Some([1, 1]))];

pub fn aspect_of(key: &str) -> Option<[u32; 2]> {
    ASPECTS.iter().find(|(k, _)| *k == key).and_then(|(_, a)| *a)
}

/// What the job tells the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// 0..1.
    Progress(f64),
    Note(String),
}

/// "Trip.mp4", then "Trip (2).mp4"… taken by creating the file.
pub fn reserve(dir: &Path, name: &str, ext: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let clean: String = name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c }).collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    let base = if clean.is_empty() { "Plume Cut".to_string() } else { clean };
    for k in 1..10_000 {
        let p = if k == 1 { dir.join(format!("{base}.{ext}")) } else { dir.join(format!("{base} ({k}).{ext}")) };
        match fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(_) => return Ok(p),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no free name"))
}

/// A fresh folder for one FFmpeg run, with the graph, the texts and the fonts of the plan.
pub fn prepare(plan: &render::Plan, tag: &str) -> io::Result<PathBuf> {
    let root = std::env::temp_dir().join("plume-cut");
    let dir = root.join(format!("{tag}-{}-{}", std::process::id(), ffmpeg_seq()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(render::GRAPH), &plan.graph)?;
    for (name, text) in &plan.texts {
        fs::write(dir.join(name), text)?;
    }
    for (name, src) in &plan.fonts {
        fs::copy(src, dir.join(name))?;
    }
    Ok(dir)
}

fn ffmpeg_seq() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Exports the project. Returns the file and its size.
pub fn run(p: &Project, spec: &Spec, ctl: &Ctl, report: &mut dyn FnMut(Msg)) -> Result<(PathBuf, u64), String> {
    if p.duration() <= 0 {
        return Err("The timeline is empty".into());
    }
    let aspect = spec.aspect.unwrap_or(p.aspect);
    let (w, h) = render::out_size(aspect, spec.short);
    let out = reserve(&spec.dir, &p.name, "mp4").map_err(|e| e.to_string())?;
    let mut hw: Option<Hw> = if spec.hw { ffmpeg::hw_cached() } else { None };
    if let Some(h) = hw {
        report(Msg::Note(h.label().to_string()));
    }
    let result = loop {
        let plan = render::plan(p, w, h, &Target::File { path: out.clone(), codec: spec.codec, hw });
        let dir = match prepare(&plan, "export") {
            Ok(d) => d,
            Err(e) => break Err(e.to_string()),
        };
        let total = plan.duration.max(0.01);
        let r = ffmpeg::run(&plan.args, &dir, ctl, &mut |t| report(Msg::Progress((t / total).clamp(0.0, 1.0))));
        let _ = fs::remove_dir_all(&dir);
        match r {
            Err(_) if ctl.cancelled() => break Err(ffmpeg::CANCELLED.into()),
            Err(_) if hw.is_some() => {
                hw = None;
                report(Msg::Note("Graphics card encoder failed: using the processor".into()));
                report(Msg::Progress(0.0));
            }
            r => break r,
        }
    };
    match result {
        Ok(()) => {
            report(Msg::Progress(1.0));
            let size = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            Ok((out, size))
        }
        Err(e) => {
            let _ = fs::remove_file(&out);
            Err(e)
        }
    }
}

/// "12.4 MB".
pub fn size_text(b: u64) -> String {
    let b = b as f64;
    if b >= 1e9 {
        format!("{:.2} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else {
        format!("{:.0} KB", (b / 1e3).max(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_overwrites() {
        let d = std::env::temp_dir().join(format!("plume-cut-names-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let a = reserve(&d, "Trip: day 1?", "mp4").unwrap();
        let b = reserve(&d, "Trip: day 1?", "mp4").unwrap();
        assert_eq!(a.file_name().unwrap(), "Trip_ day 1_.mp4");
        assert_eq!(b.file_name().unwrap(), "Trip_ day 1_ (2).mp4");
        assert_eq!(reserve(&d, "  ", "mp4").unwrap().file_name().unwrap(), "Plume Cut.mp4");
        let _ = fs::remove_dir_all(&d);
        assert_eq!(aspect_of("9:16"), Some([9, 16]));
        assert_eq!(aspect_of("project"), None);
        assert_eq!(size_text(12_400_000), "12.4 MB");
    }
}
