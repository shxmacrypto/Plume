//! The library: every photo and video under the chosen folders, newest first, with the date each was taken
//! (EXIF for photos, otherwise the file's date). The list is kept in the data folder so the window opens on it
//! at once; a scan in the background then brings it up to date, reading again only the files that changed.
//! Also the small things remembered about files: favorites, where each video stopped, view-only rotations.

use crate::{exif_info, image, settings::data_dir};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering as AtOrd},
    },
    time::UNIX_EPOCH,
};
use windows::Win32::{
    Foundation::{FILETIME, SYSTEMTIME},
    System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    pub video: bool,
    /// When it was taken: local wall-clock seconds since 1970.
    pub taken: i64,
    /// The file's modification time (seconds since 1970, UTC) and size: they tell when it changed.
    pub mtime: i64,
    pub size: u64,
}

impl Item {
    pub fn key(&self) -> u64 {
        image::thumb_key(&self.path, self.mtime, self.size)
    }
    pub fn folder(&self) -> PathBuf {
        self.path.parent().map(Path::to_path_buf).unwrap_or_default()
    }
}

/// Days since 1970-01-01 of a calendar date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The calendar date of a day number.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// (year, month, day, hour, minute) of local wall-clock seconds.
pub fn parts(t: i64) -> (i64, u32, u32, u32, u32) {
    let (y, m, d) = civil_from_days(t.div_euclid(86400));
    let s = t.rem_euclid(86400);
    (y, m, d, (s / 3600) as u32, ((s / 60) % 60) as u32)
}

/// UTC seconds since 1970 → local wall-clock seconds (with the daylight saving time of that date).
pub fn local(utc: i64) -> i64 {
    let ft100 = (utc + 11_644_473_600) * 10_000_000;
    let ft = FILETIME { dwLowDateTime: ft100 as u32, dwHighDateTime: (ft100 >> 32) as u32 };
    let (mut st, mut lt) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        if FileTimeToSystemTime(&ft, &mut st).is_err() || SystemTimeToTzSpecificLocalTime(None, &st, &mut lt).is_err() {
            return utc;
        }
    }
    days_from_civil(lt.wYear as i64, lt.wMonth as u32, lt.wDay as u32) * 86400 + lt.wHour as i64 * 3600 + lt.wMinute as i64 * 60 + lt.wSecond as i64
}

fn mtime_of(m: &std::fs::Metadata) -> i64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64)
}

/// One file, read from scratch.
pub fn read_item(path: &Path) -> Option<Item> {
    let m = std::fs::metadata(path).ok()?;
    let video = image::is_video(path);
    if !video && !image::is_photo(path) {
        return None;
    }
    let mtime = mtime_of(&m);
    let taken = if video { None } else { exif_info::taken(path) }.unwrap_or_else(|| local(mtime));
    Some(Item { path: path.to_path_buf(), video, taken, mtime, size: m.len() })
}

fn skip_dir(name: &str, attrs: u32) -> bool {
    // hidden or system folders, and the usual places that are not anyone's photos
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    attrs & (HIDDEN | SYSTEM) != 0 || name.starts_with('.') || name.starts_with('$') || name.eq_ignore_ascii_case("node_modules")
}

/// Progress of a scan, read by the window.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicUsize,
    pub stop: AtomicBool,
}

/// Every photo and video under `folders`, newest first. Files unchanged since `old` are not read again.
pub fn scan(folders: &[PathBuf], old: &HashMap<PathBuf, Item>, progress: &Arc<Progress>) -> Vec<Item> {
    scan_depth(folders, old, progress, 32)
}

/// Like [`scan`], `max_depth` levels of subfolders down (0: the folders themselves only).
pub fn scan_depth(folders: &[PathBuf], old: &HashMap<PathBuf, Item>, progress: &Arc<Progress>, max_depth: u32) -> Vec<Item> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<(PathBuf, u32)> = folders.iter().map(|f| (f.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        if progress.stop.load(AtOrd::Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            let path = e.path();
            if m.is_dir() {
                let name = e.file_name().to_string_lossy().to_string();
                if depth < max_depth && !skip_dir(&name, m.file_attributes()) {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !image::is_media(&path) || !seen.insert(path.to_string_lossy().to_lowercase()) {
                continue;
            }
            let (mtime, size) = (mtime_of(&m), m.len());
            let item = match old.get(&path) {
                Some(o) if o.mtime == mtime && o.size == size => Some(o.clone()),
                _ => read_item(&path),
            };
            if let Some(it) = item {
                out.push(it);
                progress.files.fetch_add(1, AtOrd::Relaxed);
            }
        }
    }
    sort(&mut out);
    out
}

pub fn sort(items: &mut [Item]) {
    items.sort_by(|a, b| b.taken.cmp(&a.taken).then_with(|| natural_cmp(&a.path.to_string_lossy(), &b.path.to_string_lossy())));
}

fn index_file() -> PathBuf {
    data_dir().join("library.tsv")
}

/// The list kept from last time.
pub fn load_index() -> Vec<Item> {
    let Ok(text) = std::fs::read_to_string(index_file()) else { return vec![] };
    let mut items: Vec<Item> = text
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let path = PathBuf::from(f.next()?);
            let video = f.next()? == "v";
            let taken = f.next()?.parse().ok()?;
            let mtime = f.next()?.parse().ok()?;
            let size = f.next()?.parse().ok()?;
            Some(Item { path, video, taken, mtime, size })
        })
        .collect();
    sort(&mut items);
    items
}

pub fn save_index(items: &[Item]) {
    let mut s = String::with_capacity(items.len() * 80);
    for it in items {
        let p = it.path.to_string_lossy();
        if p.contains(['\t', '\n']) {
            continue;
        }
        s.push_str(&format!("{p}\t{}\t{}\t{}\t{}\n", if it.video { "v" } else { "p" }, it.taken, it.mtime, it.size));
    }
    let _ = std::fs::create_dir_all(data_dir());
    let tmp = index_file().with_extension("tmp");
    if std::fs::write(&tmp, s).is_ok() {
        let _ = std::fs::rename(&tmp, index_file());
    }
}

/// Compares names the way File Explorer orders them: "IMG_2" before "IMG_10", case aside.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    x.next();
                }
                let mut nb = String::new();
                while let Some(c) = y.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    y.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// The photos and videos of a folder, in File Explorer's order (for ← → after opening a file from Explorer).
pub fn folder_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && image::is_media(p)).collect()).unwrap_or_default();
    v.sort_by(|a, b| {
        let n = |p: &PathBuf| p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        natural_cmp(&n(a), &n(b))
    });
    v
}

/// What Plume Media remembers about files.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Store {
    /// Paths, lowercase.
    pub favorites: HashSet<String>,
    /// Where each video stopped, in seconds.
    pub resume: HashMap<String, f64>,
    /// Quarter turns of pictures that cannot be turned in their file.
    pub rotations: HashMap<String, u32>,
}

pub fn norm(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

impl Store {
    fn file() -> PathBuf {
        data_dir().join("store.json")
    }
    pub fn load() -> Store {
        std::fs::read(Self::file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(data_dir());
        let tmp = Self::file().with_extension("tmp");
        if let Ok(b) = serde_json::to_vec(self)
            && std::fs::write(&tmp, b).is_ok()
        {
            let _ = std::fs::rename(&tmp, Self::file());
        }
    }
    pub fn is_favorite(&self, p: &Path) -> bool {
        self.favorites.contains(&norm(p))
    }
    /// Toggles; true when it became a favorite.
    pub fn toggle_favorite(&mut self, p: &Path) -> bool {
        let k = norm(p);
        if !self.favorites.remove(&k) {
            self.favorites.insert(k);
            return true;
        }
        false
    }
    pub fn rotation(&self, p: &Path) -> u32 {
        self.rotations.get(&norm(p)).copied().unwrap_or(0)
    }
    /// Remembers where a video stopped; forgets it near the start or the end.
    pub fn set_resume(&mut self, p: &Path, pos: f64, dur: f64) {
        let k = norm(p);
        if pos > 10.0 && dur > 60.0 && pos < dur - 15.0 {
            self.resume.insert(k, pos);
        } else {
            self.resume.remove(&k);
        }
    }
    pub fn resume_at(&self, p: &Path) -> Option<f64> {
        self.resume.get(&norm(p)).copied()
    }
}

/// "1:05", "1:02:03".
pub fn clock(t: f64) -> String {
    let s = if t.is_finite() { t.max(0.0) as u64 } else { 0 };
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// "3.4 MB".
pub fn bytes(n: u64) -> String {
    let fr = crate::i18n::french();
    let (v, unit) = match n {
        0..1024 => return format!("{n} {}", if fr { "o" } else { "B" }),
        1024..1_048_576 => (n as f64 / 1024.0, if fr { "Ko" } else { "KB" }),
        1_048_576..1_073_741_824 => (n as f64 / 1_048_576.0, if fr { "Mo" } else { "MB" }),
        _ => (n as f64 / 1_073_741_824.0, if fr { "Go" } else { "GB" }),
    };
    let s = if v < 10.0 { format!("{v:.1}") } else { format!("{v:.0}") };
    format!("{} {unit}", if fr { s.replace('.', ",") } else { s })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        for d in [-1000, 0, 365, 10_957, 20_000, 30_000] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        let t = days_from_civil(2026, 11, 11) * 86400 + 18 * 3600 + 5 * 60;
        assert_eq!(parts(t), (2026, 11, 11, 18, 5));
    }

    #[test]
    fn natural_order() {
        assert_eq!(natural_cmp("IMG_2.jpg", "IMG_10.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("a.jpg", "B.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("x01", "x1"), Ordering::Equal);
    }

    #[test]
    fn resume_rules() {
        let mut s = Store::default();
        let p = Path::new("C:/v/a.mp4");
        s.set_resume(p, 125.0, 600.0);
        assert_eq!(s.resume_at(p), Some(125.0));
        s.set_resume(p, 595.0, 600.0);
        assert_eq!(s.resume_at(p), None);
        assert!(s.toggle_favorite(Path::new("C:/A.jpg")));
        assert!(s.is_favorite(Path::new("c:/a.jpg")));
    }

    #[test]
    fn formats() {
        let _l = crate::i18n::TEST_LOCK.lock();
        crate::i18n::set("en");
        assert_eq!(clock(65.0), "1:05");
        assert_eq!(clock(3723.0), "1:02:03");
        assert_eq!(bytes(3_565_158), "3.4 MB");
    }

    #[test]
    fn scans_a_folder() {
        let dir = std::env::temp_dir().join(format!("plume-media-scan-{}", std::process::id()));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.jpg"), b"x").unwrap();
        std::fs::write(sub.join("b.mp4"), b"x").unwrap();
        std::fs::write(dir.join("c.txt"), b"x").unwrap();
        let items = scan(std::slice::from_ref(&dir), &HashMap::new(), &Arc::new(Progress::default()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|i| i.video));
    }
}
