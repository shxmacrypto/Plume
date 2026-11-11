//! Files and folders: a light index of the user's own folders (Desktop, Documents, Downloads, Pictures, Music,
//! Videos, OneDrive), built with Plume FEx's index and searched with its search, without admin rights.
//! Plume FEx's own index covers whole disks and needs its service to stay current: the launcher keeps its own,
//! smaller one, rebuilt in the background every half hour and kept on disk for a quick start.
//!
//! Developer folders full of generated files (node_modules, Rust's target, dot folders) are left out.

use crate::apps::known_folder;
use fex_core::{
    index::{F_DELETED, F_DIR, F_HIDDEN, F_LINK, F_SYSTEM, Index, NO_PARENT},
    listing::{ATTR_DIRECTORY, ATTR_HIDDEN, ATTR_REPARSE, ATTR_SYSTEM, read_dir},
    query::{Clock, Order, Query},
};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use windows::Win32::{
    System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL},
    UI::Shell::{FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Videos},
};

/// At most this many files and folders in the index (about 55 bytes each).
const MAX_ENTRIES: usize = 1_500_000;
const REBUILD_EVERY: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub path: PathBuf,
    pub name: String,
    pub dir: bool,
    /// Seconds since 1970.
    pub mtime: u64,
}

pub type Deliver = Box<dyn Fn(u64, Vec<Hit>) + Send>;

enum Req {
    Search(u64, String),
    Rebuild,
}

pub struct Files {
    index: RwLock<Index>,
    tx: Mutex<mpsc::Sender<Req>>,
    pub building: AtomicBool,
    pub count: AtomicUsize,
    /// When the index was last built (seconds since 1970; 0: never).
    pub built: AtomicU64,
    stop: AtomicBool,
}

impl Files {
    /// Asks for the files matching `q`; the answer comes through `deliver` with the same `generation`.
    pub fn search(&self, generation: u64, q: &str) {
        let _ = self.tx.lock().unwrap().send(Req::Search(generation, q.to_string()));
    }
    pub fn rebuild(&self) {
        let _ = self.tx.lock().unwrap().send(Req::Rebuild);
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The folders to index (tests: PLUME_LAUNCHER_FILE_ROOTS, folders separated by ";"; empty: none).
pub fn roots() -> Vec<PathBuf> {
    if let Some(v) = std::env::var_os("PLUME_LAUNCHER_FILE_ROOTS") {
        return std::env::split_paths(&v).filter(|p| p.is_dir()).collect();
    }
    let mut all: Vec<PathBuf> = [FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Pictures, FOLDERID_Music, FOLDERID_Videos]
        .iter()
        .filter_map(known_folder)
        .chain(std::env::var_os("OneDrive").map(PathBuf::from))
        .filter(|p| p.is_dir())
        .collect();
    nest(&mut all);
    all
}

/// Keeps only the outermost folders (Documents inside OneDrive is indexed once, with OneDrive).
fn nest(all: &mut Vec<PathBuf>) {
    let low = |p: &Path| p.display().to_string().to_lowercase().trim_end_matches('\\').to_string() + "\\";
    let keys: Vec<String> = all.iter().map(|p| low(p)).collect();
    let mut keep = vec![];
    for (i, p) in all.iter().enumerate() {
        let inside = keys.iter().enumerate().any(|(j, k)| j != i && keys[i].starts_with(k.as_str()) && (keys[i] != *k || j < i));
        if !inside {
            keep.push(p.clone());
        }
    }
    *all = keep;
}

/// Folders that are left out: generated, hidden-by-convention or system ones.
fn skip_dir(name: &str, rust_project: bool) -> bool {
    name.starts_with('.')
        || name.starts_with('$')
        || name.eq_ignore_ascii_case("node_modules")
        || name.eq_ignore_ascii_case("__pycache__")
        || name.eq_ignore_ascii_case("AppData")
        || (rust_project && name.eq_ignore_ascii_case("target"))
}

/// Reads the folders `roots` into an index (breadth first: a folder is always before what it contains).
pub fn build(roots: &[PathBuf], cancel: &AtomicBool) -> Index {
    let mut ix = Index::default();
    let mut queue = VecDeque::new();
    for r in roots {
        let name = r.display().to_string();
        let id = ix.push(name.trim_end_matches('\\'), NO_PARENT, 0, 0, F_DIR);
        queue.push_back((id, r.clone(), 0u32));
    }
    let mut kids: Vec<(String, u32, u64, u64)> = vec![];
    while let Some((id, path, depth)) = queue.pop_front() {
        if cancel.load(Ordering::Relaxed) || ix.len() >= MAX_ENTRIES {
            break;
        }
        kids.clear();
        let _ = read_dir(&path, Some(cancel), &mut |e| kids.push((String::from_utf16_lossy(e.name), e.attrs, e.size, e.mtime)));
        let rust_project = kids.iter().any(|k| k.0.eq_ignore_ascii_case("Cargo.toml"));
        for (name, attrs, size, mtime) in kids.drain(..) {
            if attrs & (ATTR_HIDDEN | ATTR_SYSTEM) != 0 {
                continue;
            }
            let dir = attrs & ATTR_DIRECTORY != 0;
            if dir && skip_dir(&name, rust_project) {
                continue;
            }
            let link = attrs & ATTR_REPARSE != 0;
            let flags = if dir { F_DIR } else { 0 } | if link { F_LINK } else { 0 };
            let child = ix.push(&name, id, size, mtime, flags);
            if dir && !link && depth < 24 {
                queue.push_back((child, path.join(&name), depth + 1));
            }
        }
    }
    ix.compute_rank(std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4));
    ix
}

/// Keeps only letters, digits and plain punctuation: Plume FEx's search syntax (`ext:`, `|`, `!`, quotes, wildcards)
/// is not something a launcher user means to type.
fn sanitize(q: &str) -> String {
    q.chars().map(|c| if matches!(c, '"' | '|' | '!' | '<' | '>' | '*' | '?' | ':' | '(' | ')' | '\\' | '/') { ' ' } else { c }).collect()
}

/// The files and folders matching `q`, at most `limit`, those whose name starts with it first.
pub fn search_index(ix: &Index, q: &str, limit: usize) -> Vec<Hit> {
    let s = sanitize(q);
    if s.trim().chars().count() < 2 {
        return vec![];
    }
    let query = Query::parse(&s, &Clock::system());
    let found = ix.search(&query, None, Order::Name, false, limit * 3, 2, &AtomicBool::new(false));
    found
        .ids
        .into_iter()
        .filter(|&i| ix.flags[i as usize] & (F_HIDDEN | F_SYSTEM | F_DELETED) == 0 && ix.parent[i as usize] != NO_PARENT)
        .take(limit)
        .map(|i| Hit { path: ix.path(i), name: ix.name(i).to_string(), dir: ix.is_dir(i), mtime: (ix.mtime_of(i) / 10_000_000).saturating_sub(11_644_473_600) })
        .collect()
}

fn now() -> u64 {
    crate::rank::now()
}

/// Starts the index service. `dir`: the launcher's data folder (the index is kept there).
pub fn start(dir: PathBuf, deliver: Deliver) -> Arc<Files> {
    let (tx, rx) = mpsc::channel();
    let files = Arc::new(Files {
        index: RwLock::new(Index::default()),
        tx: Mutex::new(tx),
        building: AtomicBool::new(false),
        count: AtomicUsize::new(0),
        built: AtomicU64::new(0),
        stop: AtomicBool::new(false),
    });
    let f = files.clone();
    std::thread::spawn(move || service(f, dir, rx, deliver));
    files
}

fn rebuild(files: &Arc<Files>, file: &Path) {
    if files.building.swap(true, Ordering::Relaxed) {
        return;
    }
    let f = files.clone();
    let file = file.to_path_buf();
    std::thread::spawn(move || {
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        }
        let roots = roots();
        let ix = build(&roots, &f.stop);
        if !f.stop.load(Ordering::Relaxed) {
            let volumes: Vec<(String, u64, i64)> = roots.iter().map(|r| (r.display().to_string(), 0, 0)).collect();
            if let Some(parent) = file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = ix.save(&file, &volumes);
            f.count.store(ix.len(), Ordering::Relaxed);
            *f.index.write().unwrap() = ix;
            f.built.store(now(), Ordering::Relaxed);
        }
        f.building.store(false, Ordering::Relaxed);
    });
}

fn service(files: Arc<Files>, dir: PathBuf, rx: mpsc::Receiver<Req>, deliver: Deliver) {
    let file = dir.join("files.bin");
    // the index of last time, if it covers the same folders: searches work at once
    let roots = roots();
    let names: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
    let mut fresh = false;
    if let Ok((ix, saved)) = Index::load(&file)
        && saved.iter().map(|v| &v.0).eq(names.iter())
    {
        files.count.store(ix.len(), Ordering::Relaxed);
        *files.index.write().unwrap() = ix;
        let age = std::fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).unwrap_or(Duration::MAX);
        files.built.store(now().saturating_sub(age.as_secs().min(now())), Ordering::Relaxed);
        fresh = age < Duration::from_secs(10 * 60);
    }
    if !fresh {
        rebuild(&files, &file);
    }
    let mut last = Instant::now();
    while !files.stop.load(Ordering::Relaxed) {
        let req = match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if last.elapsed() >= REBUILD_EVERY {
                    last = Instant::now();
                    rebuild(&files, &file);
                }
                continue;
            }
            Err(_) => return,
        };
        // only the latest search counts: the ones typed over are dropped
        let mut latest = None;
        let mut again = false;
        for r in std::iter::once(req).chain(rx.try_iter()) {
            match r {
                Req::Search(g, q) => latest = Some((g, q)),
                Req::Rebuild => again = true,
            }
        }
        if again {
            last = Instant::now();
            rebuild(&files, &file);
        }
        if let Some((g, q)) = latest {
            let hits = search_index(&files.index.read().unwrap(), &q, 60);
            deliver(g, hits);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outermost_roots() {
        let mut v = vec![PathBuf::from(r"C:\Users\a\OneDrive\Documents"), PathBuf::from(r"C:\Users\a\Downloads"), PathBuf::from(r"C:\Users\a\OneDrive")];
        nest(&mut v);
        assert_eq!(v, [PathBuf::from(r"C:\Users\a\Downloads"), PathBuf::from(r"C:\Users\a\OneDrive")]);
        let mut same = vec![PathBuf::from(r"C:\x"), PathBuf::from(r"c:\X\")];
        nest(&mut same);
        assert_eq!(same.len(), 1);
    }

    #[test]
    fn index_and_search() {
        let dir = std::env::temp_dir().join(format!("plume-launcher-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let proj = dir.join("project");
        std::fs::create_dir_all(proj.join("target").join("debug")).unwrap();
        std::fs::create_dir_all(proj.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules").join("left")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join("Holiday photos")).unwrap();
        std::fs::write(proj.join("Cargo.toml"), b"x").unwrap();
        std::fs::write(proj.join("target").join("debug").join("budget-build.txt"), b"x").unwrap();
        std::fs::write(proj.join("src").join("budget.rs"), b"x").unwrap();
        std::fs::write(dir.join("Budget 2026.xlsx"), b"x").unwrap();
        std::fs::write(dir.join("node_modules").join("left").join("budget.js"), b"x").unwrap();
        std::fs::write(dir.join("Holiday photos").join("beach.jpg"), b"x").unwrap();
        let ix = build(std::slice::from_ref(&dir), &AtomicBool::new(false));
        let hits = search_index(&ix, "budget", 10);
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"Budget 2026.xlsx"), "{names:?}");
        assert!(names.contains(&"budget.rs"), "{names:?}");
        // generated folders are not indexed
        assert!(!names.contains(&"budget-build.txt") && !names.contains(&"budget.js"), "{names:?}");
        let h = search_index(&ix, "holiday", 10);
        assert_eq!(h.len(), 1);
        assert!(h[0].dir && h[0].path.ends_with("Holiday photos"));
        // the search syntax of Plume FEx is not interpreted, and one letter finds nothing
        assert!(search_index(&ix, "b", 10).is_empty());
        assert!(!search_index(&ix, "beach!", 10).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
