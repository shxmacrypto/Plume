//! The backup engine: files only, no window and no Windows API, so every rule here is covered by the tests below.
//!
//! At the destination, each plan has its own folder:
//!
//! ```text
//! <destination>\Plume Backup\<plan>\
//!     versions\2026-11-11 14-30-05\<source>\…   one complete copy per run (versions mode); unchanged files are hard
//!                                              links to the previous run, so they take no extra space
//!     versions\2026-11-11 15-30-00.partial\     a run that was interrupted: the next run carries on from it
//!     current\<source>\…                        the copy kept identical to the sources (mirror mode)
//!     deleted\2026-11-11 15-30-00\<source>\…    files that disappeared from the sources, kept before any cleanup
//! ```
//!
//! The rules that make it safe: the sources are only ever read; a source folder that is missing stops the run
//! (it is never read as "everything was deleted"); a folder that cannot be read keeps its previous copy; a file
//! removed from a source moves to `deleted` before the mirror is cleaned; files are written under a temporary name
//! and renamed once complete, so an interruption never leaves a half-written file under its real name.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// The folder created at the destination.
pub const ROOT: &str = "Plume Backup";
/// What a snapshot knows about itself (hidden from the restore view).
pub const META: &str = ".plume-backup.json";
/// A file being written: renamed to its real name once complete.
const PART: &str = ".plume-part";
const PARTIAL: &str = ".partial";
const VERSIONS: &str = "versions";
const CURRENT: &str = "current";
const DELETED: &str = "deleted";

/// Temporary files skipped when "Skip temporary files" is on.
pub const TEMP: &[&str] = &[
    "*.tmp",
    "*.temp",
    "~$*",
    "*.crdownload",
    "*.part",
    "*.partial",
    "*.swp",
    "Thumbs.db",
    "desktop.ini",
    ".DS_Store",
    "$RECYCLE.BIN",
    "System Volume Information",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// A dated copy per run, unchanged files shared between them.
    #[default]
    Versions,
    /// One copy kept identical to the sources.
    Mirror,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Schedule {
    /// Only with "Back up now".
    Manual,
    Hourly,
    #[default]
    Daily,
    /// When the destination drive is plugged in.
    Plug,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plan {
    pub id: u64,
    pub name: String,
    /// The plan's folder at the destination (set once, so renaming the plan keeps its backups).
    pub folder: String,
    pub sources: Vec<PathBuf>,
    pub dest: PathBuf,
    /// Serial number of the destination's volume: a USB drive is found again when Windows gives it another letter.
    pub dest_serial: Option<u32>,
    pub schedule: Schedule,
    /// Daily: from this hour (0 to 23).
    pub hour: u8,
    pub mode: Mode,
    /// Versions kept (0: no limit by number).
    pub keep_versions: u32,
    /// Days kept (0: no limit by age). The newest version is always kept.
    pub keep_days: u32,
    /// Days the files removed from the sources are kept (0: forever).
    pub deleted_days: u32,
    /// Patterns to leave out: `*.iso`, `node_modules`, `Downloads/Big/*`.
    pub excludes: Vec<String>,
    pub skip_temp: bool,
    /// Read each copied file back and compare it with the source.
    pub verify: bool,
    pub paused: bool,
}

impl Default for Plan {
    fn default() -> Self {
        Plan {
            id: 0,
            name: String::new(),
            folder: String::new(),
            sources: Vec::new(),
            dest: PathBuf::new(),
            dest_serial: None,
            schedule: Schedule::Daily,
            hour: 12,
            mode: Mode::Versions,
            keep_versions: 30,
            keep_days: 0,
            deleted_days: 90,
            excludes: Vec::new(),
            skip_temp: true,
            verify: false,
            paused: false,
        }
    }
}

impl Plan {
    /// The plan's folder at a destination.
    pub fn root(&self, dest: &Path) -> PathBuf {
        let folder = if self.folder.is_empty() { safe_name(&self.name) } else { self.folder.clone() };
        dest.join(ROOT).join(folder)
    }
}

/// A name usable as a folder name.
pub fn safe_name(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let out = out.trim().trim_end_matches('.').trim().to_string();
    if out.is_empty() { "Backup".into() } else { out }
}

/// The folder name of each source inside a snapshot: the source's own name, numbered when two share it.
pub fn source_keys(sources: &[PathBuf]) -> Vec<String> {
    let mut used: Vec<String> = Vec::new();
    sources
        .iter()
        .map(|p| {
            let base = match p.file_name() {
                Some(n) => safe_name(&n.to_string_lossy()),
                // a whole drive: "C"
                None => safe_name(p.to_string_lossy().trim_end_matches(['\\', '/', ':'])),
            };
            let mut key = base.clone();
            let mut n = 2;
            while used.iter().any(|u| u.eq_ignore_ascii_case(&key)) {
                key = format!("{base} ({n})");
                n += 1;
            }
            used.push(key.clone());
            key
        })
        .collect()
}

// ───────────────────────── problems, progress, report ─────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fail {
    /// Asked to stop: what was done is kept and the next run carries on.
    Stopped,
    NoSources,
    NoDest,
    /// The destination is not there (drive unplugged, share offline).
    DestMissing,
    /// A source folder is missing: nothing is touched.
    SourceMissing(PathBuf),
    /// The destination is inside a source, or a source inside the destination.
    Overlap,
    DestFull,
    Io(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Scanning,
    Copying,
    Cleaning,
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    pub stamp: String,
    pub files: u64,
    /// Written from the sources.
    pub copied: u64,
    /// Shared with the previous version (hard link) or copied from it.
    pub linked: u64,
    pub unchanged: u64,
    /// Removed from the sources since last time: kept in `deleted`.
    pub removed: u64,
    pub bytes: u64,
    pub verified: u64,
    /// Old versions removed by the retention rules.
    pub pruned: u64,
    /// The run carried on from an interrupted one.
    pub resumed: bool,
    /// Files that could not be backed up: (path, reason).
    pub errors: Vec<(String, String)>,
}

/// What a snapshot folder knows about itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub plan: String,
    pub stamp: String,
    /// Unix time of the run.
    pub time: u64,
    /// Each source's folder in the snapshot and where it came from.
    pub sources: Vec<(String, PathBuf)>,
    pub files: u64,
    pub bytes: u64,
    pub complete: bool,
}

pub fn unix(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn now() -> u64 {
    unix(SystemTime::now())
}

fn is_full(e: &io::Error) -> bool {
    // 112 ERROR_DISK_FULL, 39 ERROR_HANDLE_DISK_FULL
    matches!(e.raw_os_error(), Some(112 | 39)) || e.kind() == io::ErrorKind::StorageFull
}

fn io_fail(e: io::Error) -> Fail {
    if is_full(&e) { Fail::DestFull } else { Fail::Io(e.to_string()) }
}

// ───────────────────────── patterns ─────────────────────────

/// `*` and `?` wildcards, without regard to case.
pub fn glob(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.to_lowercase().chars().collect();
    let s: Vec<char> = s.to_lowercase().chars().collect();
    let (mut i, mut j) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while j < s.len() {
        if i < p.len() && (p[i] == '?' || p[i] == s[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(st) = star {
            i = st + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

/// Splits what the user typed (commas, semicolons or new lines) into patterns.
pub fn parse_patterns(s: &str) -> Vec<String> {
    s.split([',', ';', '\n']).map(|p| p.trim().replace('\\', "/")).filter(|p| !p.is_empty()).collect()
}

struct Excludes {
    names: Vec<String>,
    paths: Vec<String>,
}

impl Excludes {
    fn new(plan: &Plan) -> Excludes {
        let mut names = Vec::new();
        let mut paths = Vec::new();
        let temp = if plan.skip_temp { TEMP } else { &[] };
        for p in plan.excludes.iter().map(|p| p.trim().replace('\\', "/")).chain(temp.iter().map(|s| s.to_string())) {
            let p = p.trim_end_matches('/').to_string();
            if p.is_empty() {
                continue;
            }
            if p.contains('/') { paths.push(p.trim_start_matches('/').to_string()) } else { names.push(p) }
        }
        Excludes { names, paths }
    }
    /// `rel`: the path inside the source, with `/`.
    fn hit(&self, name: &str, rel: &str) -> bool {
        name.ends_with(PART) || name == META || self.names.iter().any(|p| glob(p, name)) || self.paths.iter().any(|p| glob(p, rel))
    }
}

// ───────────────────────── reading the sources ─────────────────────────

#[derive(Clone, Debug)]
struct Item {
    /// Path inside the snapshot (`<source key>\…`).
    rel: PathBuf,
    src: PathBuf,
    size: u64,
    mtime: SystemTime,
}

#[derive(Default)]
struct Scan {
    files: Vec<Item>,
    dirs: Vec<PathBuf>,
    /// Folders that could not be read (lower case, `/`): what they held is kept as it was.
    unreadable: Vec<String>,
    errors: Vec<(String, String)>,
}

fn norm(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn under(n: &str, dirs: &[String]) -> bool {
    dirs.iter().any(|d| n.starts_with(d.as_str()) && n.as_bytes().get(d.len()) == Some(&b'/'))
}

fn scan(plan: &Plan, keys: &[String], stop: &AtomicBool) -> Result<Scan, Fail> {
    let ex = Excludes::new(plan);
    let mut out = Scan::default();
    for (src, key) in plan.sources.iter().zip(keys) {
        if !src.is_dir() {
            return Err(Fail::SourceMissing(src.clone()));
        }
        out.dirs.push(PathBuf::from(key));
        let mut stack = vec![(src.clone(), PathBuf::from(key), String::new())];
        while let Some((dir, rel, inner)) = stack.pop() {
            if stop.load(Ordering::Relaxed) {
                return Err(Fail::Stopped);
            }
            let rd = match fs::read_dir(&dir) {
                Ok(rd) => rd,
                Err(e) => {
                    if dir == *src {
                        return Err(Fail::SourceMissing(src.clone()));
                    }
                    out.unreadable.push(norm(&rel));
                    out.errors.push((dir.display().to_string(), e.to_string()));
                    continue;
                }
            };
            for entry in rd {
                let Ok(entry) = entry else {
                    out.unreadable.push(norm(&rel));
                    continue;
                };
                let name = entry.file_name().to_string_lossy().into_owned();
                let inner_rel = if inner.is_empty() { name.clone() } else { format!("{inner}/{name}") };
                if ex.hit(&name, &inner_rel) {
                    continue;
                }
                let Ok(ft) = entry.file_type() else { continue };
                // links and junctions are not followed: they would loop or leave the source
                if ft.is_symlink() {
                    continue;
                }
                let r = rel.join(&name);
                if ft.is_dir() {
                    out.dirs.push(r.clone());
                    stack.push((entry.path(), r, inner_rel));
                } else if ft.is_file() {
                    match entry.metadata() {
                        Ok(md) => out.files.push(Item { rel: r, src: entry.path(), size: md.len(), mtime: md.modified().unwrap_or(UNIX_EPOCH) }),
                        Err(e) => out.errors.push((entry.path().display().to_string(), e.to_string())),
                    }
                }
            }
        }
    }
    out.files.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

/// Files under a snapshot folder: (path inside it, size).
fn files_under(root: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), PathBuf::new())];
    while let Some((dir, rel)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() || (rel.as_os_str().is_empty() && name == META) {
                continue;
            }
            if ft.is_dir() {
                stack.push((e.path(), rel.join(&name)));
            } else if ft.is_file() {
                if name.to_string_lossy().ends_with(PART) {
                    let _ = fs::remove_file(e.path());
                    continue;
                }
                out.push((rel.join(&name), e.metadata().map(|m| m.len()).unwrap_or(0)));
            }
        }
    }
    out
}

// ───────────────────────── files ─────────────────────────

/// SHA-256 of a file.
pub fn hash_file(p: &Path, stop: &AtomicBool) -> io::Result<[u8; 32]> {
    let mut f = fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"));
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().into())
}

enum Same {
    Yes,
    No,
    /// Same size and content, another date: the copy only needs the date.
    YesButDate,
}

/// Is the copy `dst` the same as the source? Size and date; when only the date differs, the content decides.
fn same(src: &Path, size: u64, mtime: SystemTime, dst: &Path, stop: &AtomicBool) -> Same {
    let Ok(md) = fs::metadata(dst) else { return Same::No };
    if !md.is_file() || md.len() != size {
        return Same::No;
    }
    if md.modified().ok() == Some(mtime) {
        return Same::Yes;
    }
    match (hash_file(src, stop), hash_file(dst, stop)) {
        (Ok(a), Ok(b)) if a == b => Same::YesButDate,
        _ => Same::No,
    }
}

fn part_of(dst: &Path) -> PathBuf {
    let mut name = dst.file_name().unwrap_or_default().to_os_string();
    name.push(PART);
    dst.with_file_name(name)
}

/// Copies `src` to `dst` through a temporary name, with the source's date. Returns the bytes and the content's hash.
fn copy_file(src: &Path, dst: &Path, mtime: Option<SystemTime>, stop: &AtomicBool) -> Result<(u64, [u8; 32]), CopyErr> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(CopyErr::Dest)?;
    }
    let part = part_of(dst);
    let mut from = fs::File::open(src).map_err(CopyErr::Source)?;
    let mut to = fs::File::create(&part).map_err(CopyErr::Dest)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    let result = (|| {
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err(CopyErr::Stopped);
            }
            let n = from.read(&mut buf).map_err(CopyErr::Source)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            to.write_all(&buf[..n]).map_err(CopyErr::Dest)?;
            total += n as u64;
        }
        to.flush().map_err(CopyErr::Dest)?;
        if let Some(t) = mtime {
            let _ = to.set_modified(t);
        }
        Ok(())
    })();
    drop(to);
    if let Err(e) = result {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    // a folder of the same name in the way (it was a folder last time): it is not ours to remove here
    if dst.is_dir() {
        let _ = fs::remove_file(&part);
        return Err(CopyErr::Dest(io::Error::new(io::ErrorKind::AlreadyExists, "a folder has this name")));
    }
    fs::rename(&part, dst).map_err(|e| {
        let _ = fs::remove_file(&part);
        CopyErr::Dest(e)
    })?;
    Ok((total, h.finalize().into()))
}

enum CopyErr {
    Stopped,
    /// The source could not be read (in use, no access): this file is skipped.
    Source(io::Error),
    /// The destination could not be written.
    Dest(io::Error),
}

/// A name for a file kept aside: `report.docx` → `report (before restore 2026-11-11 14-30-05).docx`.
fn aside_name(p: &Path, label: &str) -> PathBuf {
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut n = 1;
    loop {
        let extra = if n == 1 { String::new() } else { format!(" {n}") };
        let cand = p.with_file_name(format!("{stem} ({label}{extra}){ext}"));
        if !cand.exists() {
            return cand;
        }
        n += 1;
    }
}

fn write_meta(dir: &Path, m: &Meta) {
    if let Ok(b) = serde_json::to_vec_pretty(m) {
        let _ = fs::write(dir.join(META), b);
    }
}

pub fn read_meta(dir: &Path) -> Option<Meta> {
    fs::read(dir.join(META)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

// ───────────────────────── safety checks ─────────────────────────

fn parts(p: &Path) -> Vec<String> {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    abs.components()
        .filter_map(|c| match c {
            Component::Prefix(x) => Some(x.as_os_str().to_string_lossy().to_lowercase()),
            Component::Normal(x) => Some(x.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect()
}

fn inside(a: &[String], b: &[String]) -> bool {
    a.len() >= b.len() && a[..b.len()] == *b
}

/// What would make a plan unsafe or impossible, before anything is written.
pub fn check(plan: &Plan, dest: &Path) -> Result<(), Fail> {
    if plan.sources.is_empty() {
        return Err(Fail::NoSources);
    }
    if dest.as_os_str().is_empty() {
        return Err(Fail::NoDest);
    }
    let root = parts(&plan.root(dest));
    let backups = parts(&dest.join(ROOT));
    for s in &plan.sources {
        let s = parts(s);
        // the backup inside a source would back itself up; a source inside the backup could be cleaned by it
        if inside(&root, &s) || inside(&s, &backups) {
            return Err(Fail::Overlap);
        }
    }
    if !dest.is_dir() {
        return Err(Fail::DestMissing);
    }
    Ok(())
}

// ───────────────────────── a run ─────────────────────────

struct Ctx<'a> {
    stop: &'a AtomicBool,
    verify: bool,
    report: Report,
    progress: Progress,
}

impl Ctx<'_> {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Copies a source file; a source that cannot be read is noted and skipped (false). Fatal problems stop the run.
    fn copy(&mut self, it: &Item, dst: &Path) -> Result<bool, Fail> {
        match copy_file(&it.src, dst, Some(it.mtime), self.stop) {
            Ok((n, hash)) => {
                self.report.copied += 1;
                self.report.bytes += n;
                if self.verify {
                    match hash_file(dst, self.stop) {
                        Ok(h) if h == hash => self.report.verified += 1,
                        Ok(_) => {
                            self.report.errors.push((it.src.display().to_string(), "the copy does not match the original".into()));
                            let _ = fs::remove_file(dst);
                            return Ok(false);
                        }
                        Err(_) if self.stopped() => return Err(Fail::Stopped),
                        Err(e) => self.report.errors.push((it.src.display().to_string(), e.to_string())),
                    }
                }
                Ok(true)
            }
            Err(CopyErr::Stopped) => Err(Fail::Stopped),
            Err(CopyErr::Source(e)) => {
                self.report.errors.push((it.src.display().to_string(), e.to_string()));
                Ok(false)
            }
            Err(CopyErr::Dest(e)) => {
                let f = io_fail(e);
                if f == Fail::DestFull {
                    return Err(f);
                }
                self.report.errors.push((
                    it.src.display().to_string(),
                    match f {
                        Fail::Io(s) => s,
                        _ => String::new(),
                    },
                ));
                Ok(false)
            }
        }
    }

    fn tick(&mut self, it: &Item, progress: &mut dyn FnMut(&Progress)) {
        self.progress.done += 1;
        self.progress.bytes_done += it.size;
        self.progress.current = it.src.display().to_string();
        progress(&self.progress);
    }
}

/// Runs a plan once. `dest`: where the destination is now (the drive's letter may have changed); `stamp`: the name
/// of this run ("2026-11-11 14-30-05", local time); `now`: unix time.
pub fn run(plan: &Plan, dest: &Path, stamp: &str, now: u64, stop: &AtomicBool, progress: &mut dyn FnMut(&Progress)) -> Result<Report, Fail> {
    check(plan, dest)?;
    let keys = source_keys(&plan.sources);
    progress(&Progress::default());
    let scan = scan(plan, &keys, stop)?;
    let root = plan.root(dest);
    fs::create_dir_all(&root).map_err(io_fail)?;
    let mut cx = Ctx {
        stop,
        verify: plan.verify,
        report: Report { stamp: stamp.to_string(), files: scan.files.len() as u64, errors: scan.errors.clone(), ..Default::default() },
        progress: Progress {
            phase: Phase::Copying,
            total: scan.files.len() as u64,
            bytes_total: scan.files.iter().map(|f| f.size).sum(),
            ..Default::default()
        },
    };
    let meta = Meta {
        plan: plan.name.clone(),
        stamp: stamp.to_string(),
        time: now,
        sources: keys.iter().cloned().zip(plan.sources.iter().cloned()).collect(),
        files: scan.files.len() as u64,
        bytes: cx.progress.bytes_total,
        complete: false,
    };
    match plan.mode {
        Mode::Versions => run_versions(&root, &scan, &meta, &mut cx, progress)?,
        Mode::Mirror => run_mirror(&root, &scan, &meta, &mut cx, progress)?,
    }
    cx.progress.phase = Phase::Cleaning;
    progress(&cx.progress);
    cx.report.pruned = prune(plan, &root, now);
    Ok(cx.report)
}

/// Complete versions, oldest first: (folder name, path).
fn versions(root: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = fs::read_dir(root.join(VERSIONS))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .filter(|(n, _)| !n.ends_with(PARTIAL))
        .collect();
    v.sort();
    v
}

/// A folder name not taken yet in `dir`.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let mut p = dir.join(name);
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{name} ({n})"));
        n += 1;
    }
    p
}

fn run_versions(root: &Path, scan: &Scan, meta: &Meta, cx: &mut Ctx, progress: &mut dyn FnMut(&Progress)) -> Result<(), Fail> {
    let vdir = root.join(VERSIONS);
    fs::create_dir_all(&vdir).map_err(io_fail)?;
    let prev = versions(root).pop().map(|(_, p)| p);
    // an interrupted run: carry on in its folder
    let partial = fs::read_dir(&vdir).into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.is_dir() && p.to_string_lossy().ends_with(PARTIAL));
    let work = match partial {
        Some(p) => {
            cx.report.resumed = true;
            p
        }
        None => free_name(&vdir, &format!("{}{PARTIAL}", meta.stamp)),
    };
    fs::create_dir_all(&work).map_err(io_fail)?;
    write_meta(&work, meta);
    for d in &scan.dirs {
        let _ = fs::create_dir_all(work.join(d));
    }
    let mut seen: HashSet<String> = HashSet::with_capacity(scan.files.len());
    for it in &scan.files {
        if cx.stopped() {
            return Err(Fail::Stopped);
        }
        seen.insert(norm(&it.rel));
        let dst = work.join(&it.rel);
        // already done by the interrupted run
        if cx.report.resumed {
            match same(&it.src, it.size, it.mtime, &dst, cx.stop) {
                Same::Yes | Same::YesButDate => {
                    cx.report.unchanged += 1;
                    cx.tick(it, progress);
                    continue;
                }
                Same::No => {
                    if dst.is_file() {
                        let _ = fs::remove_file(&dst);
                    }
                }
            }
        }
        let old = prev.as_ref().map(|p| p.join(&it.rel));
        let unchanged = match &old {
            Some(o) => match same(&it.src, it.size, it.mtime, o, cx.stop) {
                Same::Yes => true,
                Same::YesButDate => {
                    // same content: the shared copy takes the source's date (it is the same content in every version)
                    if let Ok(f) = fs::File::options().write(true).open(o) {
                        let _ = f.set_modified(it.mtime);
                    }
                    true
                }
                Same::No => false,
            },
            None => false,
        };
        if cx.stopped() {
            return Err(Fail::Stopped);
        }
        if unchanged && let Some(o) = &old {
            if link_or_copy(o, &dst, cx.stop)? {
                cx.report.unchanged += 1;
                cx.report.linked += 1;
            } else {
                cx.copy(it, &dst)?;
            }
        } else if !cx.copy(it, &dst)? {
            // not readable now: the previous copy, if any, stays in this version
            if let Some(o) = &old
                && o.is_file()
            {
                let _ = link_or_copy(o, &dst, cx.stop);
            }
        }
        cx.tick(it, progress);
    }
    // what disappeared from the sources since the previous version
    if let Some(prev) = &prev {
        let mut removed = Vec::new();
        for (rel, _) in files_under(prev) {
            let n = norm(&rel);
            if seen.contains(&n) {
                continue;
            }
            if under(&n, &scan.unreadable) {
                // a folder that could not be read: its last copy carries on
                let _ = link_or_copy(&prev.join(&rel), &work.join(&rel), cx.stop);
                continue;
            }
            removed.push(rel);
        }
        if !removed.is_empty() {
            let ddir = free_name(&root.join(DELETED), &meta.stamp);
            for rel in &removed {
                if cx.stopped() {
                    return Err(Fail::Stopped);
                }
                if link_or_copy(&prev.join(rel), &ddir.join(rel), cx.stop)? {
                    cx.report.removed += 1;
                }
            }
            write_meta(&ddir, &Meta { complete: true, files: removed.len() as u64, ..meta.clone() });
        }
    }
    write_meta(&work, &Meta { complete: true, ..meta.clone() });
    let fin = free_name(&vdir, &meta.stamp);
    fs::rename(&work, &fin).map_err(io_fail)?;
    if let Some(n) = fin.file_name() {
        cx.report.stamp = n.to_string_lossy().into_owned();
    }
    Ok(())
}

/// A file shared with an older version: a hard link when the drive allows it, otherwise a copy. False if neither
/// worked (the caller copies from the source instead).
fn link_or_copy(from: &Path, to: &Path, stop: &AtomicBool) -> Result<bool, Fail> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(io_fail)?;
    }
    if to.exists() {
        let _ = fs::remove_file(to);
    }
    if fs::hard_link(from, to).is_ok() {
        return Ok(true);
    }
    let mtime = fs::metadata(from).and_then(|m| m.modified()).ok();
    match copy_file(from, to, mtime, stop) {
        Ok(_) => Ok(true),
        Err(CopyErr::Stopped) => Err(Fail::Stopped),
        Err(CopyErr::Dest(e)) if is_full(&e) => Err(Fail::DestFull),
        Err(_) => Ok(false),
    }
}

fn run_mirror(root: &Path, scan: &Scan, meta: &Meta, cx: &mut Ctx, progress: &mut dyn FnMut(&Progress)) -> Result<(), Fail> {
    let cur = root.join(CURRENT);
    fs::create_dir_all(&cur).map_err(io_fail)?;
    cx.report.resumed = read_meta(&cur).is_some_and(|m| !m.complete);
    write_meta(&cur, meta);
    let mut seen: HashSet<String> = HashSet::with_capacity(scan.files.len());
    let mut deleted: Option<PathBuf> = None;
    let mut removed = 0u64;
    for it in &scan.files {
        if cx.stopped() {
            return Err(Fail::Stopped);
        }
        seen.insert(norm(&it.rel));
        let dst = cur.join(&it.rel);
        match same(&it.src, it.size, it.mtime, &dst, cx.stop) {
            Same::Yes => cx.report.unchanged += 1,
            Same::YesButDate => {
                if let Ok(f) = fs::File::options().write(true).open(&dst) {
                    let _ = f.set_modified(it.mtime);
                }
                cx.report.unchanged += 1;
            }
            Same::No => {
                if cx.stopped() {
                    return Err(Fail::Stopped);
                }
                // a folder where the source now has a file: it goes to "deleted" first
                if dst.is_dir() {
                    let d = deleted.get_or_insert_with(|| free_name(&root.join(DELETED), &meta.stamp)).clone();
                    if move_aside(&dst, &d.join(&it.rel)) {
                        removed += files_under(&d.join(&it.rel)).len() as u64;
                    }
                }
                cx.copy(it, &dst)?;
            }
        }
        cx.tick(it, progress);
    }
    let dirs: HashSet<String> = scan.dirs.iter().map(|d| norm(d)).collect();
    // files gone from the sources: moved to "deleted" (same drive: a rename), never simply removed
    for (rel, _) in files_under(&cur) {
        if cx.stopped() {
            return Err(Fail::Stopped);
        }
        let n = norm(&rel);
        if seen.contains(&n) || under(&n, &scan.unreadable) {
            continue;
        }
        let d = deleted.get_or_insert_with(|| free_name(&root.join(DELETED), &meta.stamp)).clone();
        if move_aside(&cur.join(&rel), &d.join(&rel)) {
            removed += 1;
        }
    }
    // then the folders left empty that no source has any more (only empty folders can be removed this way)
    let mut all_dirs = Vec::new();
    let mut stack = vec![(cur.clone(), PathBuf::new())];
    while let Some((dir, rel)) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                let r = rel.join(e.file_name());
                all_dirs.push(r.clone());
                stack.push((e.path(), r));
            }
        }
    }
    all_dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in all_dirs {
        let n = norm(&d);
        if !dirs.contains(&n) && !under(&n, &scan.unreadable) && !scan.unreadable.contains(&n) {
            let _ = fs::remove_dir(cur.join(&d));
        }
    }
    if let Some(d) = &deleted {
        write_meta(d, &Meta { complete: true, files: removed, ..meta.clone() });
    }
    cx.report.removed = removed;
    write_meta(&cur, &Meta { complete: true, ..meta.clone() });
    Ok(())
}

fn move_aside(from: &Path, to: &Path) -> bool {
    if let Some(parent) = to.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::rename(from, to).is_ok()
}

/// Applies the retention rules; returns how many folders were removed. Only folders inside the plan's own folder at
/// the destination are ever removed, and the newest version always stays.
fn prune(plan: &Plan, root: &Path, now: u64) -> u64 {
    let mut n = 0;
    if plan.mode == Mode::Versions {
        let v = versions(root);
        let count = v.len();
        for (i, (name, path)) in v.iter().enumerate() {
            if i + 1 == count {
                break;
            }
            let newer = count - i - 1;
            let time = read_meta(path).map(|m| m.time).unwrap_or(now);
            let too_many = plan.keep_versions > 0 && newer >= plan.keep_versions as usize;
            let too_old = plan.keep_days > 0 && now.saturating_sub(time) > plan.keep_days as u64 * 86_400;
            if (too_many || too_old) && path.starts_with(root.join(VERSIONS)) && !name.is_empty() && fs::remove_dir_all(path).is_ok() {
                n += 1;
            }
        }
    }
    if plan.deleted_days > 0 {
        for e in fs::read_dir(root.join(DELETED)).into_iter().flatten().flatten() {
            let p = e.path();
            let time = read_meta(&p).map(|m| m.time).unwrap_or(now);
            if p.is_dir() && now.saturating_sub(time) > plan.deleted_days as u64 * 86_400 && fs::remove_dir_all(&p).is_ok() {
                n += 1;
            }
        }
    }
    n
}

// ───────────────────────── browsing and restoring ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Version,
    Mirror,
    Deleted,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub kind: Kind,
    pub stamp: String,
    pub time: u64,
    pub path: PathBuf,
    pub files: u64,
}

/// What can be restored from a plan's folder, newest first.
pub fn snapshots(root: &Path) -> Vec<Snapshot> {
    let mut out = Vec::new();
    let time_of = |p: &Path| -> (u64, u64) {
        match read_meta(p) {
            Some(m) => (m.time, m.files),
            None => (fs::metadata(p).and_then(|m| m.modified()).map(unix).unwrap_or(0), 0),
        }
    };
    for (name, path) in versions(root) {
        let (time, files) = time_of(&path);
        out.push(Snapshot { kind: Kind::Version, stamp: name, time, path, files });
    }
    let cur = root.join(CURRENT);
    if cur.is_dir() {
        let (time, files) = time_of(&cur);
        out.push(Snapshot { kind: Kind::Mirror, stamp: CURRENT.into(), time, path: cur, files });
    }
    for e in fs::read_dir(root.join(DELETED)).into_iter().flatten().flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            let (time, files) = time_of(&e.path());
            out.push(Snapshot { kind: Kind::Deleted, stamp: e.file_name().to_string_lossy().into_owned(), time, path: e.path(), files });
        }
    }
    out.sort_by(|a, b| b.time.cmp(&a.time).then(b.stamp.cmp(&a.stamp)));
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub modified: u64,
}

/// A folder of a snapshot: folders first, then files, by name.
pub fn list(dir: &Path) -> Vec<Entry> {
    let mut out: Vec<Entry> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == META || name.ends_with(PART) {
                return None;
            }
            let md = e.metadata().ok()?;
            Some(Entry { name, dir: md.is_dir(), size: if md.is_dir() { 0 } else { md.len() }, modified: md.modified().map(unix).unwrap_or(0) })
        })
        .collect();
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Where it came from.
    Original,
    /// Into this folder.
    Folder(PathBuf),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Restored {
    pub files: u64,
    /// Already identical where it goes.
    pub same: u64,
    /// Different files that were there: kept under another name, next to it.
    pub kept: u64,
    pub to: PathBuf,
    pub errors: Vec<(String, String)>,
}

/// Where a path of a snapshot goes back to. `rel`: `<source key>\…`.
pub fn restore_target(snap: &Path, rel: &Path, to: &Target) -> Option<PathBuf> {
    match to {
        Target::Folder(d) => Some(d.join(rel.file_name()?)),
        Target::Original => {
            let mut comps = rel.components();
            let key = comps.next()?.as_os_str().to_string_lossy().into_owned();
            let rest: PathBuf = comps.collect();
            let meta = read_meta(snap)?;
            let src = meta.sources.iter().find(|(k, _)| k.eq_ignore_ascii_case(&key)).map(|(_, p)| p.clone())?;
            Some(if rest.as_os_str().is_empty() { src } else { src.join(rest) })
        }
    }
}

/// Puts a file or a folder of a snapshot back. Nothing is overwritten: a different file already there is renamed
/// first ("… (before restore <label>)"), an identical one is left alone.
pub fn restore(snap: &Path, rel: &Path, to: &Target, label: &str, stop: &AtomicBool) -> Result<Restored, Fail> {
    let from = snap.join(rel);
    let target = restore_target(snap, rel, to).ok_or(Fail::Io("unknown source".into()))?;
    if !from.exists() {
        return Err(Fail::Io("not in the backup".into()));
    }
    let mut out = Restored { to: target.clone(), ..Default::default() };
    let list: Vec<(PathBuf, u64)> = if from.is_dir() { files_under(&from) } else { vec![(PathBuf::new(), 0)] };
    if from.is_dir() {
        let _ = fs::create_dir_all(&target);
    }
    let aside = format!("before restore {label}");
    for (r, _) in list {
        if stop.load(Ordering::Relaxed) {
            return Err(Fail::Stopped);
        }
        let (src, dst) = if r.as_os_str().is_empty() { (from.clone(), target.clone()) } else { (from.join(&r), target.join(&r)) };
        let md = match fs::metadata(&src) {
            Ok(m) => m,
            Err(e) => {
                out.errors.push((src.display().to_string(), e.to_string()));
                continue;
            }
        };
        let mtime = md.modified().ok();
        if dst.exists() {
            match same(&src, md.len(), mtime.unwrap_or(UNIX_EPOCH), &dst, stop) {
                Same::Yes | Same::YesButDate => {
                    out.same += 1;
                    continue;
                }
                Same::No => {
                    if dst.is_dir() || fs::rename(&dst, aside_name(&dst, &aside)).is_err() {
                        out.errors.push((dst.display().to_string(), "could not keep the file that is there".into()));
                        continue;
                    }
                    out.kept += 1;
                }
            }
        }
        match copy_file(&src, &dst, mtime, stop) {
            Ok(_) => out.files += 1,
            Err(CopyErr::Stopped) => return Err(Fail::Stopped),
            Err(CopyErr::Source(e) | CopyErr::Dest(e)) => out.errors.push((dst.display().to_string(), e.to_string())),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::atomic::AtomicUsize, time::Duration};

    static N: AtomicUsize = AtomicUsize::new(0);

    /// A fresh folder in the system's temp folder (removed at the end of the test).
    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Tmp {
            let p = std::env::temp_dir().join(format!("plume-backup-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Tmp(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn put(p: &Path, text: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap_or_else(|_| panic!("missing {}", p.display()))
    }
    fn set_time(p: &Path, secs: u64) {
        fs::File::options().write(true).open(p).unwrap().set_modified(UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
    }

    struct Setup {
        _t: Tmp,
        src: PathBuf,
        dest: PathBuf,
        plan: Plan,
    }

    fn setup(mode: Mode) -> Setup {
        let t = Tmp::new();
        let src = t.0.join("Documents");
        let dest = t.0.join("USB");
        put(&src.join("a.txt"), "alpha");
        put(&src.join("sub/b.txt"), "bravo");
        put(&src.join("sub/deep/c.txt"), "charlie");
        fs::create_dir_all(src.join("empty")).unwrap();
        fs::create_dir_all(&dest).unwrap();
        let plan = Plan { id: 1, name: "Docs".into(), sources: vec![src.clone()], dest: dest.clone(), mode, ..Default::default() };
        Setup { _t: t, src, dest, plan }
    }

    fn go(s: &Setup, stamp: &str, now: u64) -> Report {
        let stop = AtomicBool::new(false);
        run(&s.plan, &s.dest, stamp, now, &stop, &mut |_| {}).unwrap()
    }

    fn vdir(s: &Setup, stamp: &str) -> PathBuf {
        s.plan.root(&s.dest).join(VERSIONS).join(stamp).join("Documents")
    }

    #[test]
    fn first_run_copies_everything_and_the_next_only_what_changed() {
        let s = setup(Mode::Versions);
        let r = go(&s, "2026-01-01 10-00-00", 1000);
        assert_eq!((r.files, r.copied, r.linked), (3, 3, 0));
        let v1 = vdir(&s, "2026-01-01 10-00-00");
        assert_eq!(read(&v1.join("sub/deep/c.txt")), "charlie");
        assert!(v1.join("empty").is_dir(), "empty folders are kept");

        let r = go(&s, "2026-01-01 11-00-00", 2000);
        assert_eq!((r.copied, r.linked, r.unchanged), (0, 3, 3), "nothing changed: everything shared");

        put(&s.src.join("a.txt"), "alpha 2");
        put(&s.src.join("new.txt"), "new");
        let r = go(&s, "2026-01-01 12-00-00", 3000);
        assert_eq!((r.copied, r.linked), (2, 2));
        // the old version still has the old content, the new one the new
        assert_eq!(read(&v1.join("a.txt")), "alpha");
        let v3 = vdir(&s, "2026-01-01 12-00-00");
        assert_eq!(read(&v3.join("a.txt")), "alpha 2");
        assert_eq!(read(&v3.join("new.txt")), "new");
        assert_eq!(read(&v3.join("sub/b.txt")), "bravo");
        // the copy has the source's date
        let t = |p: &Path| fs::metadata(p).unwrap().modified().unwrap();
        assert_eq!(t(&v3.join("a.txt")), t(&s.src.join("a.txt")));
        assert_eq!(snapshots(&s.plan.root(&s.dest)).len(), 3);
    }

    #[test]
    fn unchanged_files_are_shared_not_copied() {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
        };
        let links = |p: &Path| {
            let f = fs::File::open(p).unwrap();
            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            unsafe { GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut info).unwrap() };
            info.nNumberOfLinks
        };
        let s = setup(Mode::Versions);
        go(&s, "v1", 1000);
        go(&s, "v2", 2000);
        // the temp folder is on NTFS: one file, two names
        assert_eq!(links(&vdir(&s, "v2").join("a.txt")), 2);
        assert_eq!(links(&s.src.join("a.txt")), 1, "the source is never linked");
    }

    #[test]
    fn a_hash_decides_when_only_the_date_differs() {
        let s = setup(Mode::Versions);
        go(&s, "v1", 1000);
        // same size, same content, another date: not copied again
        set_time(&s.src.join("a.txt"), 1_600_000_000);
        // same size, other content, another date: copied
        put(&s.src.join("sub/b.txt"), "BRAVO");
        set_time(&s.src.join("sub/b.txt"), 1_600_000_000);
        let r = go(&s, "v2", 2000);
        assert_eq!(r.copied, 1);
        assert_eq!(read(&vdir(&s, "v2").join("sub/b.txt")), "BRAVO");
        assert_eq!(read(&vdir(&s, "v1").join("sub/b.txt")), "bravo");
    }

    #[test]
    fn mirror_keeps_removed_files_in_deleted_and_never_touches_the_source() {
        let s = setup(Mode::Mirror);
        let r = go(&s, "m1", 1000);
        assert_eq!(r.copied, 3);
        let cur = s.plan.root(&s.dest).join(CURRENT).join("Documents");
        assert_eq!(read(&cur.join("sub/b.txt")), "bravo");

        fs::remove_file(s.src.join("sub/b.txt")).unwrap();
        fs::remove_dir_all(s.src.join("sub/deep")).unwrap();
        put(&s.src.join("a.txt"), "alpha 2");
        let r = go(&s, "m2", 2000);
        assert_eq!((r.copied, r.removed), (1, 2));
        assert!(!cur.join("sub/b.txt").exists());
        assert!(!cur.join("sub/deep").exists(), "an empty folder gone from the source goes too");
        assert_eq!(read(&cur.join("a.txt")), "alpha 2");
        let del = s.plan.root(&s.dest).join(DELETED).join("m2").join("Documents");
        assert_eq!(read(&del.join("sub/b.txt")), "bravo");
        assert_eq!(read(&del.join("sub/deep/c.txt")), "charlie");
        // the source is as the test left it
        assert_eq!(read(&s.src.join("a.txt")), "alpha 2");
        assert!(s.src.join("empty").is_dir());
        let kinds: Vec<Kind> = snapshots(&s.plan.root(&s.dest)).iter().map(|x| x.kind).collect();
        assert_eq!(kinds, vec![Kind::Deleted, Kind::Mirror]);
    }

    #[test]
    fn versions_keep_removed_files_too() {
        let s = setup(Mode::Versions);
        go(&s, "v1", 1000);
        fs::remove_file(s.src.join("a.txt")).unwrap();
        let r = go(&s, "v2", 2000);
        assert_eq!(r.removed, 1);
        assert!(!vdir(&s, "v2").join("a.txt").exists());
        assert_eq!(read(&s.plan.root(&s.dest).join(DELETED).join("v2").join("Documents").join("a.txt")), "alpha");
    }

    #[test]
    fn retention_by_number_and_by_age() {
        let mut s = setup(Mode::Versions);
        s.plan.keep_versions = 2;
        for (i, st) in ["v1", "v2", "v3", "v4"].iter().enumerate() {
            put(&s.src.join("a.txt"), &format!("alpha {i}"));
            go(&s, st, 1000 + i as u64);
        }
        let names: Vec<String> = versions(&s.plan.root(&s.dest)).into_iter().map(|v| v.0).collect();
        assert_eq!(names, vec!["v3", "v4"]);
        assert_eq!(read(&vdir(&s, "v4").join("a.txt")), "alpha 3");
        assert_eq!(read(&vdir(&s, "v3").join("sub/b.txt")), "bravo", "shared files survive the removal of an old version");

        s.plan.keep_versions = 0;
        s.plan.keep_days = 10;
        let day = 86_400;
        go(&s, "v5", 1000 + 20 * day);
        // v3 and v4 are 20 days old: removed; v5 stays
        let names: Vec<String> = versions(&s.plan.root(&s.dest)).into_iter().map(|v| v.0).collect();
        assert_eq!(names, vec!["v5"]);
        // the newest version stays even when it is old
        go(&s, "v6", 1000 + 21 * day);
        let r = prune(&s.plan, &s.plan.root(&s.dest), 1000 + 400 * day);
        assert_eq!(r, 1);
        assert_eq!(versions(&s.plan.root(&s.dest)).len(), 1);
    }

    #[test]
    fn deleted_files_expire_after_their_days() {
        let mut s = setup(Mode::Mirror);
        s.plan.deleted_days = 5;
        go(&s, "m1", 1000);
        fs::remove_file(s.src.join("a.txt")).unwrap();
        go(&s, "m2", 2000);
        assert!(s.plan.root(&s.dest).join(DELETED).join("m2").is_dir());
        go(&s, "m3", 2000 + 6 * 86_400);
        assert!(!s.plan.root(&s.dest).join(DELETED).join("m2").exists());
    }

    #[test]
    fn an_interrupted_run_resumes_where_it_stopped() {
        let s = setup(Mode::Versions);
        for i in 0..20 {
            put(&s.src.join(format!("many/f{i:02}.txt")), &format!("file {i}"));
        }
        let stop = AtomicBool::new(false);
        let mut seen = 0;
        let r = run(&s.plan, &s.dest, "v1", 1000, &stop, &mut |p| {
            if p.phase == Phase::Copying && p.done > 0 {
                seen += 1;
                if seen == 10 {
                    stop.store(true, Ordering::Relaxed);
                }
            }
        });
        assert_eq!(r, Err(Fail::Stopped));
        let vd = s.plan.root(&s.dest).join(VERSIONS);
        assert!(vd.join(format!("v1{PARTIAL}")).is_dir(), "the work so far is kept");
        assert!(versions(&s.plan.root(&s.dest)).is_empty());

        let r = go(&s, "v2", 2000);
        assert!(r.resumed);
        assert_eq!(r.files, 23);
        assert!(r.copied <= 13, "files already done are not copied again ({})", r.copied);
        assert_eq!(r.copied + r.unchanged, 23);
        assert!(!vd.join(format!("v1{PARTIAL}")).exists());
        assert_eq!(read(&vdir(&s, "v2").join("many/f19.txt")), "file 19");
        assert!(read_meta(&vd.join("v2")).unwrap().complete);
    }

    #[test]
    fn a_half_written_file_is_never_left_under_its_name() {
        let s = setup(Mode::Mirror);
        let stop = AtomicBool::new(true);
        assert_eq!(run(&s.plan, &s.dest, "m1", 1000, &stop, &mut |_| {}), Err(Fail::Stopped));
        let cur = s.plan.root(&s.dest).join(CURRENT).join("Documents");
        assert!(!cur.join("a.txt").exists());
        // a leftover temporary file is cleaned at the next run
        put(&cur.join(format!("a.txt{PART}")), "half");
        let r = go(&s, "m2", 2000);
        assert_eq!(r.copied, 3);
        assert!(!cur.join(format!("a.txt{PART}")).exists());
    }

    #[test]
    fn excludes_and_temporary_files() {
        let mut s = setup(Mode::Mirror);
        put(&s.src.join("x.tmp"), "t");
        put(&s.src.join("~$report.docx"), "lock");
        put(&s.src.join("Thumbs.db"), "t");
        put(&s.src.join("big.iso"), "iso");
        put(&s.src.join("node_modules/pkg/i.js"), "js");
        put(&s.src.join("sub/cache/c.bin"), "c");
        put(&s.src.join("keep/cache/c.bin"), "c");
        s.plan.excludes = parse_patterns("*.ISO; node_modules, sub/cache");
        let r = go(&s, "m1", 1000);
        assert_eq!(r.files, 4, "a.txt, b.txt, c.txt, keep/cache/c.bin");
        let cur = s.plan.root(&s.dest).join(CURRENT).join("Documents");
        for gone in ["x.tmp", "~$report.docx", "Thumbs.db", "big.iso", "node_modules", "sub/cache"] {
            assert!(!cur.join(gone).exists(), "{gone} should be left out");
        }
        assert!(cur.join("keep/cache/c.bin").exists());
        // temporary files are kept when asked
        s.plan.skip_temp = false;
        s.plan.excludes.clear();
        let r = go(&s, "m2", 2000);
        assert_eq!(r.files, 10);
    }

    #[test]
    fn patterns() {
        assert!(glob("*.tmp", "A.TMP"));
        assert!(glob("~$*", "~$doc.docx"));
        assert!(glob("a?c", "abc") && !glob("a?c", "abbc"));
        assert!(glob("photos/*/raw", "photos/2024/raw"));
        assert!(!glob("*.tmp", "tmp.txt"));
        assert!(glob("*", ""));
        assert_eq!(parse_patterns(" *.iso ;\nnode_modules, a\\b "), vec!["*.iso", "node_modules", "a/b"]);
    }

    #[test]
    fn a_missing_source_stops_everything() {
        let s = setup(Mode::Mirror);
        go(&s, "m1", 1000);
        let moved = s.src.with_file_name("Documents-away");
        fs::rename(&s.src, &moved).unwrap();
        let stop = AtomicBool::new(false);
        let r = run(&s.plan, &s.dest, "m2", 2000, &stop, &mut |_| {});
        assert_eq!(r, Err(Fail::SourceMissing(s.src.clone())));
        // the mirror is untouched, nothing went to "deleted"
        assert_eq!(read(&s.plan.root(&s.dest).join(CURRENT).join("Documents").join("a.txt")), "alpha");
        assert!(!s.plan.root(&s.dest).join(DELETED).exists());
    }

    #[test]
    fn unsafe_plans_are_refused() {
        let s = setup(Mode::Mirror);
        let mut p = s.plan.clone();
        p.sources.clear();
        assert_eq!(check(&p, &s.dest), Err(Fail::NoSources));
        // the destination inside the source
        let inner = s.src.join("backup");
        fs::create_dir_all(&inner).unwrap();
        assert_eq!(check(&s.plan, &inner), Err(Fail::Overlap));
        // a source inside the backup folder
        let mut p = s.plan.clone();
        p.sources = vec![s.plan.root(&s.dest).join("x")];
        assert_eq!(check(&p, &s.dest), Err(Fail::Overlap));
        // the destination is gone (drive unplugged)
        assert_eq!(check(&s.plan, &s.dest.join("nope")), Err(Fail::DestMissing));
        assert_eq!(check(&s.plan, &s.dest), Ok(()));
    }

    #[test]
    fn restore_to_its_place_keeps_what_is_there() {
        let s = setup(Mode::Versions);
        go(&s, "v1", 1000);
        put(&s.src.join("a.txt"), "alpha changed");
        fs::remove_file(s.src.join("sub/b.txt")).unwrap();
        go(&s, "v2", 2000);
        let snap = s.plan.root(&s.dest).join(VERSIONS).join("v1");
        let stop = AtomicBool::new(false);
        // one file
        let r = restore(&snap, Path::new("Documents/a.txt"), &Target::Original, "v1", &stop).unwrap();
        assert_eq!((r.files, r.kept), (1, 1));
        assert_eq!(read(&s.src.join("a.txt")), "alpha");
        assert_eq!(read(&s.src.join("a (before restore v1).txt")), "alpha changed");
        // a folder: the missing file comes back, the identical ones are left alone
        let r = restore(&snap, Path::new("Documents/sub"), &Target::Original, "v1", &stop).unwrap();
        assert_eq!((r.files, r.same, r.kept), (1, 1, 0));
        assert_eq!(read(&s.src.join("sub/b.txt")), "bravo");
        // again: nothing to do
        let r = restore(&snap, Path::new("Documents/a.txt"), &Target::Original, "v1", &stop).unwrap();
        assert_eq!((r.files, r.same), (0, 1));
    }

    #[test]
    fn restore_elsewhere() {
        let s = setup(Mode::Mirror);
        go(&s, "m1", 1000);
        let other = s.dest.with_file_name("Restored");
        let snap = s.plan.root(&s.dest).join(CURRENT);
        let stop = AtomicBool::new(false);
        let r = restore(&snap, Path::new("Documents/sub"), &Target::Folder(other.clone()), "x", &stop).unwrap();
        assert_eq!(r.files, 2);
        assert_eq!(r.to, other.join("sub"));
        assert_eq!(read(&other.join("sub/deep/c.txt")), "charlie");
        let r = restore(&snap, Path::new("Documents"), &Target::Folder(other.clone()), "x", &stop).unwrap();
        assert_eq!(r.files, 3);
        assert_eq!(read(&other.join("Documents/a.txt")), "alpha");
    }

    #[test]
    fn browsing_a_snapshot() {
        let s = setup(Mode::Versions);
        go(&s, "v1", 1000);
        let snap = &snapshots(&s.plan.root(&s.dest))[0];
        assert_eq!((snap.kind, snap.time, snap.files), (Kind::Version, 1000, 3));
        let top = list(&snap.path);
        assert_eq!(top.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["Documents"], "the meta file is hidden");
        let docs = list(&snap.path.join("Documents"));
        assert_eq!(docs.iter().map(|e| (e.name.as_str(), e.dir)).collect::<Vec<_>>(), vec![("empty", true), ("sub", true), ("a.txt", false)]);
        assert_eq!(docs[2].size, 5);
    }

    #[test]
    fn two_sources_with_the_same_name() {
        let t = Tmp::new();
        let a = t.0.join("one").join("Photos");
        let b = t.0.join("two").join("Photos");
        put(&a.join("x.jpg"), "a");
        put(&b.join("x.jpg"), "b");
        assert_eq!(source_keys(&[a.clone(), b.clone()]), vec!["Photos", "Photos (2)"]);
        assert_eq!(source_keys(&[PathBuf::from(r"D:\")]), vec!["D"]);
        let dest = t.0.join("dest");
        fs::create_dir_all(&dest).unwrap();
        let plan = Plan { name: "P".into(), sources: vec![a, b.clone()], dest: dest.clone(), ..Default::default() };
        let stop = AtomicBool::new(false);
        run(&plan, &dest, "v1", 1, &stop, &mut |_| {}).unwrap();
        let snap = plan.root(&dest).join(VERSIONS).join("v1");
        assert_eq!(read(&snap.join("Photos (2)/x.jpg")), "b");
        assert_eq!(restore_target(&snap, Path::new("Photos (2)/x.jpg"), &Target::Original), Some(b.join("x.jpg")));
    }

    #[test]
    fn verification_reads_the_copy_back() {
        let mut s = setup(Mode::Mirror);
        s.plan.verify = true;
        let r = go(&s, "m1", 1000);
        assert_eq!((r.copied, r.verified), (3, 3));
        assert!(r.errors.is_empty());
    }

    #[test]
    fn names() {
        assert_eq!(safe_name("My: docs?"), "My_ docs_");
        assert_eq!(safe_name("  ..  "), "Backup");
        assert_eq!(aside_name(Path::new("C:/x/r.docx"), "t").file_name().unwrap(), "r (t).docx");
    }
}
