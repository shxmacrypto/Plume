//! Duplicate files: same size first (free, from the scan), then the same first 64 KB, then the same whole content
//! (SHA-256, pure Rust). Hard links of one file are not duplicates (removing one frees nothing), so each group keeps
//! one name per real file.

use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

/// A group of files with the same content, oldest first.
pub type Group = Vec<(PathBuf, u64, u64)>;

#[derive(Default)]
pub struct Progress {
    /// Bytes read so far.
    pub read: AtomicU64,
    pub cancel: AtomicBool,
}

/// The real file behind a name (volume and file index), to see hard links.
fn identity(f: &File) -> Option<(u32, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut info) }.ok()?;
    Some((info.dwVolumeSerialNumber, ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64))
}

fn hash(path: &PathBuf, limit: Option<usize>, p: &Progress) -> Option<[u8; 32]> {
    let mut f = File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut left = limit.unwrap_or(usize::MAX);
    while left > 0 {
        if p.cancel.load(Ordering::Relaxed) {
            return None;
        }
        let want = buf.len().min(left);
        let n = f.read(&mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        p.read.fetch_add(n as u64, Ordering::Relaxed);
        left -= n;
    }
    Some(h.finalize().into())
}

/// Splits `files` (path, size, mtime) into groups by `key`, keeping groups of two or more.
fn split<K: std::hash::Hash + Eq>(files: Vec<(PathBuf, u64, u64)>, key: impl Fn(&(PathBuf, u64, u64)) -> Option<K>) -> Vec<Group> {
    let mut map: HashMap<K, Group> = HashMap::new();
    for f in files {
        if let Some(k) = key(&f) {
            map.entry(k).or_default().push(f);
        }
    }
    map.into_values().filter(|g| g.len() > 1).collect()
}

/// Groups of identical files among `files` (path, size, mtime). Biggest waste first; in each group, the oldest file
/// first (the one to keep).
pub fn find(files: Vec<(PathBuf, u64, u64)>, threads: usize, p: &Progress) -> Vec<Group> {
    let by_size = split(files.into_iter().filter(|f| f.1 > 0).collect(), |f| Some(f.1));
    let next = AtomicUsize::new(0);
    let out = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(group) = by_size.get(i) else { break };
                    if p.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    // one name per real file
                    let mut seen = std::collections::HashSet::new();
                    let real: Group = group
                        .iter()
                        .filter(|f| match File::open(&f.0).ok().and_then(|h| identity(&h)) {
                            Some(id) => seen.insert(id),
                            None => false,
                        })
                        .cloned()
                        .collect();
                    if real.len() < 2 {
                        continue;
                    }
                    let size = real[0].1;
                    for g in split(real, |f| hash(&f.0, Some(64 * 1024), p)) {
                        let full = if size <= 64 * 1024 { vec![g] } else { split(g, |f| hash(&f.0, None, p)) };
                        out.lock().unwrap_or_else(|e| e.into_inner()).extend(full);
                    }
                }
            });
        }
    });
    let mut groups = out.into_inner().unwrap_or_else(|e| e.into_inner());
    for g in groups.iter_mut() {
        g.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
    }
    groups.sort_by(|a, b| (b[0].1 * (b.len() as u64 - 1)).cmp(&(a[0].1 * (a.len() as u64 - 1))).then_with(|| a[0].0.cmp(&b[0].0)));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tests::TempDir;

    fn entry(p: PathBuf) -> (PathBuf, u64, u64) {
        let len = std::fs::metadata(&p).unwrap().len();
        (p, len, 0)
    }

    #[test]
    fn finds_same_content_only() {
        let t = TempDir::new("dupes");
        let a = t.0.join("a.bin");
        let b = t.0.join("sub").join("b.bin");
        let c = t.0.join("c.bin");
        let d = t.0.join("d.bin");
        std::fs::create_dir_all(t.0.join("sub")).unwrap();
        // 200 KB: past the first 64 KB check; c differs only at the end
        let mut data = vec![7u8; 200 * 1024];
        std::fs::write(&a, &data).unwrap();
        std::fs::write(&b, &data).unwrap();
        data[200 * 1024 - 1] = 8;
        std::fs::write(&c, &data).unwrap();
        // same size as the others, different start
        data[0] = 1;
        std::fs::write(&d, &data).unwrap();
        let small1 = t.0.join("s1.txt");
        let small2 = t.0.join("s2.txt");
        std::fs::write(&small1, b"hello").unwrap();
        std::fs::write(&small2, b"hello").unwrap();
        let link = t.0.join("hardlink.bin");
        std::fs::hard_link(&a, &link).unwrap();
        let files = [&a, &b, &c, &d, &small1, &small2, &link].iter().map(|p| entry((*p).clone())).collect();
        let p = Progress::default();
        let groups = find(files, 3, &p);
        assert_eq!(groups.len(), 2, "{groups:?}");
        // the big group first, and it holds a, b and only one of a / its hard link
        assert_eq!(groups[0].len(), 2);
        assert!(groups[0].iter().any(|f| f.0 == b));
        assert_eq!(groups[1].len(), 2);
        assert!(groups[1].iter().all(|f| f.1 == 5));
        assert!(p.read.load(Ordering::Relaxed) > 0);
    }
}
