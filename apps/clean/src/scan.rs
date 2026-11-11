//! The scan: a drive or a folder read by several threads at once, into one flat tree.
//!
//! Each thread takes a folder from a shared queue, reads it in one go with fex-core's fast listing (one handle, big
//! buffers), adds its entries to the tree under a short lock and queues its subfolders. Entries of one folder sit
//! next to each other in the tree, and always after their folder, so folder sizes are summed in a single pass from
//! the end. Links and junctions are listed but never followed; files that live only in the cloud count for nothing.

use fex_core::{
    listing::{ATTR_DIRECTORY, ATTR_HIDDEN, ATTR_OFFLINE, ATTR_RECALL_ON_DATA_ACCESS, ATTR_REPARSE, ATTR_SYSTEM, read_dir_buf},
    wide::long_path,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

pub const DIR: u8 = 1;
pub const HIDDEN: u8 = 2;
pub const SYSTEM: u8 = 4;
/// Link or junction: shown, never followed.
pub const LINK: u8 = 8;
/// The folder could not be read.
pub const DENIED: u8 = 16;
/// Only in the cloud (OneDrive "online only"): no space on this disk.
pub const CLOUD: u8 = 32;
/// Somewhere inside, a folder could not be read or a link was not followed: its content is not fully known.
pub const UNSURE: u8 = 64;

const TAG_MOUNT_POINT: u32 = 0xA000_0003;
const TAG_SYMLINK: u32 = 0xA000_000C;

#[derive(Clone, Copy, Debug, Default)]
pub struct Node {
    name_off: u32,
    name_len: u32,
    pub flags: u8,
    pub parent: u32,
    /// Children: ids `first .. first + count`.
    pub first: u32,
    pub count: u32,
    /// Files inside (for a file: 1).
    pub files: u64,
    pub size: u64,
    /// Last change (FILETIME); for a folder, the newest of everything inside.
    pub mtime: u64,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.flags & DIR != 0
    }
}

/// What a scan found. Node 0 is the folder scanned.
#[derive(Default)]
pub struct Tree {
    pub root: PathBuf,
    pub nodes: Vec<Node>,
    names: String,
    pub millis: u64,
    pub cancelled: bool,
}

#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub cancel: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Size,
    Name,
    Date,
}

impl Tree {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn node(&self, i: u32) -> &Node {
        &self.nodes[i as usize]
    }
    pub fn name(&self, i: u32) -> &str {
        let n = &self.nodes[i as usize];
        &self.names[n.name_off as usize..(n.name_off + n.name_len) as usize]
    }
    pub fn children(&self, i: u32) -> std::ops::Range<u32> {
        let n = &self.nodes[i as usize];
        n.first..n.first + n.count
    }
    pub fn path(&self, i: u32) -> PathBuf {
        let mut parts = Vec::new();
        let mut at = i;
        while at != 0 {
            parts.push(self.name(at));
            at = self.nodes[at as usize].parent;
        }
        let mut p = self.root.clone();
        for part in parts.iter().rev() {
            p.push(part);
        }
        p
    }
    /// The chain of folders from the root down to `i` (both included).
    pub fn chain(&self, i: u32) -> Vec<u32> {
        let mut out = vec![i];
        let mut at = i;
        while at != 0 {
            at = self.nodes[at as usize].parent;
            out.push(at);
        }
        out.reverse();
        out
    }
    #[cfg(test)]
    pub fn find(&self, path: &Path) -> Option<u32> {
        let rel = path.strip_prefix(&self.root).ok()?;
        let mut at = 0u32;
        for part in rel.components() {
            let want = part.as_os_str().to_string_lossy();
            at = self.children(at).find(|&c| self.name(c).eq_ignore_ascii_case(&want))?;
        }
        Some(at)
    }
    pub fn sorted_children(&self, i: u32, by: Order, desc: bool) -> Vec<u32> {
        let mut v: Vec<u32> = self.children(i).collect();
        self.sort(&mut v, by, desc);
        v
    }
    pub fn sort(&self, v: &mut [u32], by: Order, desc: bool) {
        match by {
            Order::Size => v.sort_by_key(|&c| self.node(c).size),
            Order::Date => v.sort_by_key(|&c| self.node(c).mtime),
            Order::Name => v.sort_by_cached_key(|&c| self.name(c).to_lowercase()),
        }
        if desc {
            v.reverse();
        }
    }
    /// Every file under `under`, folder by folder.
    pub fn files_under(&self, under: u32, mut f: impl FnMut(u32)) {
        let mut stack = vec![under];
        while let Some(d) = stack.pop() {
            for c in self.children(d) {
                if self.node(c).is_dir() {
                    stack.push(c);
                } else {
                    f(c);
                }
            }
        }
    }
    /// The `n` biggest files under `under`, biggest first.
    pub fn biggest_files(&self, under: u32, n: usize) -> Vec<u32> {
        let mut all = Vec::new();
        self.files_under(under, |c| {
            if self.node(c).size > 0 {
                all.push(c)
            }
        });
        if all.len() > n {
            all.select_nth_unstable_by(n, |a, b| self.node(*b).size.cmp(&self.node(*a).size));
            all.truncate(n);
        }
        all.sort_by_key(|&a| std::cmp::Reverse(self.node(a).size));
        all
    }
    /// Folders with no file at all inside (only empty folders, or nothing), keeping only the outermost ones.
    /// Folders not fully read, links, hidden and system folders are left out, and so are those `keep` names and
    /// every folder holding one of them.
    pub fn empty_dirs(&self, keep: impl Fn(u32) -> bool) -> Vec<u32> {
        let n = self.nodes.len();
        let mut ok = vec![false; n];
        for (i, nd) in self.nodes.iter().enumerate().skip(1) {
            ok[i] = nd.is_dir() && nd.files == 0 && nd.flags & (LINK | DENIED | UNSURE | HIDDEN | SYSTEM) == 0 && !keep(i as u32);
        }
        // children come after their folder: from the end, a folder that must stay keeps its parent too
        for i in (1..n).rev() {
            if self.nodes[i].is_dir() && !ok[i] {
                ok[self.nodes[i].parent as usize] = false;
            }
        }
        (1..n).filter(|&i| ok[i] && (self.nodes[i].parent == 0 || !ok[self.nodes[i].parent as usize])).map(|i| i as u32).collect()
    }
    /// Every file of at least `min` bytes.
    pub fn files_at_least(&self, min: u64) -> Vec<u32> {
        (1..self.nodes.len() as u32).filter(|&i| !self.node(i).is_dir() && self.node(i).size >= min).collect()
    }
}

/// One entry read: offset in `names`, length, attributes, size, mtime, reparse tag, UTF-16 range in `wide`.
type RawRead = (u32, u32, u32, u64, u64, u32, (usize, usize));

/// One folder's entries, read outside the lock.
struct Read {
    names: String,
    entries: Vec<RawRead>,
    wide: Vec<u16>,
}

struct Shared<'a> {
    tree: Mutex<(Vec<Node>, String)>,
    queue: Mutex<Vec<(u32, Vec<u16>)>>,
    cv: Condvar,
    pending: AtomicUsize,
    progress: &'a Progress,
}

/// A folder path as Windows wants it ("C:" becomes "C:\").
pub fn normalize(root: &Path) -> PathBuf {
    let s = root.to_string_lossy();
    if s.len() == 2 && s.ends_with(':') { PathBuf::from(format!("{s}\\")) } else { root.to_path_buf() }
}

/// Reads everything under `root` with `threads` threads. Stops early (with what was found) when `progress.cancel`
/// becomes true.
pub fn scan(root: &Path, threads: usize, progress: &Progress) -> Tree {
    let start = Instant::now();
    let root = normalize(root);
    let mut name = String::new();
    name.push_str(&root.to_string_lossy());
    let top = Node { name_off: 0, name_len: name.len() as u32, flags: DIR, ..Default::default() };
    let mut w = long_path(&root);
    w.pop();
    let shared = Shared { tree: Mutex::new((vec![top], name)), queue: Mutex::new(vec![(0, w)]), cv: Condvar::new(), pending: AtomicUsize::new(1), progress };
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| worker(&shared));
        }
    });
    let (mut nodes, names) = shared.tree.into_inner().unwrap_or_else(|e| e.into_inner());
    // sizes, file counts and dates, from the deepest entries up
    for i in (1..nodes.len()).rev() {
        let n = nodes[i];
        let p = &mut nodes[n.parent as usize];
        p.size += n.size;
        p.files += n.files;
        p.mtime = p.mtime.max(n.mtime);
        if n.flags & (DENIED | LINK | UNSURE) != 0 && n.is_dir() {
            p.flags |= UNSURE;
        }
    }
    Tree { root, nodes, names, millis: start.elapsed().as_millis() as u64, cancelled: progress.cancel.load(Ordering::Relaxed) }
}

fn worker(sh: &Shared) {
    let mut buf = vec![0u64; 128 * 1024 / 8];
    let mut read = Read { names: String::new(), entries: Vec::new(), wide: Vec::new() };
    loop {
        let job = {
            let mut q = sh.queue.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(j) = q.pop() {
                    break Some(j);
                }
                if sh.pending.load(Ordering::SeqCst) == 0 || sh.progress.cancel.load(Ordering::Relaxed) {
                    break None;
                }
                q = sh.cv.wait_timeout(q, Duration::from_millis(50)).unwrap_or_else(|e| e.into_inner()).0;
            }
        };
        let Some((id, mut path)) = job else {
            sh.cv.notify_all();
            return;
        };
        read.names.clear();
        read.entries.clear();
        read.wide.clear();
        path.push(0);
        let ok = read_dir_buf(&path, Some(&sh.progress.cancel), &mut buf, &mut |e| {
            let off = read.names.len() as u32;
            for ch in char::decode_utf16(e.name.iter().copied()) {
                read.names.push(ch.unwrap_or('\u{FFFD}'));
            }
            let len = read.names.len() as u32 - off;
            let ws = read.wide.len();
            read.wide.extend_from_slice(e.name);
            read.entries.push((off, len, e.attrs, e.size, e.mtime, e.reparse_tag, (ws, e.name.len())));
        })
        .is_ok();
        path.pop();
        let (mut nfiles, mut nbytes, mut ndirs) = (0u64, 0u64, 0u64);
        let mut subdirs: Vec<(u32, usize)> = Vec::new();
        {
            let mut t = sh.tree.lock().unwrap_or_else(|e| e.into_inner());
            let (nodes, names) = &mut *t;
            if !ok {
                nodes[id as usize].flags |= DENIED;
            }
            let base = names.len() as u32;
            names.push_str(&read.names);
            let first = nodes.len() as u32;
            for (k, &(off, len, attrs, size, mtime, tag, _)) in read.entries.iter().enumerate() {
                let mut flags = 0u8;
                if attrs & ATTR_HIDDEN != 0 {
                    flags |= HIDDEN;
                }
                if attrs & ATTR_SYSTEM != 0 {
                    flags |= SYSTEM;
                }
                let link = attrs & ATTR_REPARSE != 0 && matches!(tag, TAG_MOUNT_POINT | TAG_SYMLINK);
                if link {
                    flags |= LINK;
                }
                let dir = attrs & ATTR_DIRECTORY != 0;
                let cloud = !dir && attrs & (ATTR_RECALL_ON_DATA_ACCESS | ATTR_OFFLINE) != 0;
                if cloud {
                    flags |= CLOUD;
                }
                let (size, files) = if dir {
                    flags |= DIR;
                    ndirs += 1;
                    (0, 0)
                } else if link || cloud {
                    nfiles += 1;
                    (0, 1)
                } else {
                    nfiles += 1;
                    nbytes += size;
                    (size, 1)
                };
                let idx = nodes.len() as u32;
                nodes.push(Node { name_off: base + off, name_len: len, flags, parent: id, first: 0, count: 0, files, size, mtime });
                if dir && !link {
                    subdirs.push((idx, k));
                }
            }
            let n = nodes.len() as u32 - first;
            nodes[id as usize].first = first;
            nodes[id as usize].count = n;
        }
        sh.progress.files.fetch_add(nfiles, Ordering::Relaxed);
        sh.progress.bytes.fetch_add(nbytes, Ordering::Relaxed);
        sh.progress.dirs.fetch_add(ndirs, Ordering::Relaxed);
        if !subdirs.is_empty() {
            let jobs: Vec<(u32, Vec<u16>)> = subdirs
                .iter()
                .map(|&(idx, k)| {
                    let (ws, wl) = read.entries[k].6;
                    let mut p = Vec::with_capacity(path.len() + wl + 2);
                    p.extend_from_slice(&path);
                    if p.last() != Some(&(b'\\' as u16)) {
                        p.push(b'\\' as u16);
                    }
                    p.extend_from_slice(&read.wide[ws..ws + wl]);
                    (idx, p)
                })
                .collect();
            sh.pending.fetch_add(jobs.len(), Ordering::SeqCst);
            sh.queue.lock().unwrap_or_else(|e| e.into_inner()).extend(jobs);
        }
        sh.pending.fetch_sub(1, Ordering::SeqCst);
        sh.cv.notify_all();
    }
}

/// Threads for a scan: enough to keep a fast SSD busy, not more.
pub fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(4, 16)
}

/// Size and file count of a folder (or a file), read directly (links not followed).
pub fn size_of(path: &Path) -> (u64, u64) {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return (0, 0) };
    if !meta.is_dir() {
        return (meta.len(), 1);
    }
    let mut buf = vec![0u64; 64 * 1024 / 8];
    let mut stack = vec![path.to_path_buf()];
    let (mut bytes, mut files) = (0u64, 0u64);
    while let Some(d) = stack.pop() {
        let mut subs = Vec::new();
        let _ = read_dir_buf(&long_path(&d), None, &mut buf, &mut |e| {
            let link = e.attrs & ATTR_REPARSE != 0 && matches!(e.reparse_tag, TAG_MOUNT_POINT | TAG_SYMLINK);
            if e.attrs & ATTR_DIRECTORY != 0 {
                if !link {
                    subs.push(String::from_utf16_lossy(e.name));
                }
            } else {
                files += 1;
                if !link && e.attrs & (ATTR_RECALL_ON_DATA_ACCESS | ATTR_OFFLINE) == 0 {
                    bytes += e.size;
                }
            }
        });
        stack.extend(subs.into_iter().map(|s| d.join(s)));
    }
    (bytes, files)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A folder of its own for a test, removed at the end.
    pub struct TempDir(pub PathBuf);
    impl TempDir {
        pub fn new(tag: &str) -> TempDir {
            let p = std::env::temp_dir().join(format!("plume-clean-test-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        pub fn file(&self, rel: &str, bytes: usize) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, vec![b'x'; bytes]).unwrap();
            p
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sizes_add_up() {
        let t = TempDir::new("sizes");
        t.file("a/one.bin", 1000);
        t.file("a/b/two.bin", 2500);
        t.file("a/b/c/three.bin", 10);
        t.file("top.txt", 7);
        std::fs::create_dir_all(t.0.join("empty/inner")).unwrap();
        std::fs::create_dir_all(t.0.join("a/b/emptytoo")).unwrap();
        let p = Progress::default();
        let tree = scan(&t.0, 4, &p);
        assert_eq!(tree.node(0).size, 3517);
        assert_eq!(tree.node(0).files, 4);
        assert_eq!(p.files.load(Ordering::Relaxed), 4);
        let a = tree.find(&t.0.join("a")).unwrap();
        assert_eq!(tree.node(a).size, 3510);
        let b = tree.find(&t.0.join("a").join("b")).unwrap();
        assert_eq!((tree.node(b).size, tree.node(b).files), (2510, 2));
        assert_eq!(tree.path(b), t.0.join("a").join("b"));
        let biggest = tree.biggest_files(0, 2);
        assert_eq!(biggest.iter().map(|&i| tree.name(i)).collect::<Vec<_>>(), ["two.bin", "one.bin"]);
        let kids = tree.sorted_children(0, Order::Size, true);
        assert_eq!(tree.name(kids[0]), "a");
        let mut empty: Vec<String> = tree.empty_dirs(|_| false).iter().map(|&i| tree.name(i).to_string()).collect();
        empty.sort();
        assert_eq!(empty, ["empty", "emptytoo"]);
        assert_eq!(size_of(&t.0.join("a")), (3510, 3));
        assert_eq!(tree.chain(b).len(), 3);
    }

    #[test]
    fn wide_tree_with_many_threads() {
        let t = TempDir::new("wide");
        for i in 0..40 {
            for j in 0..5 {
                t.file(&format!("d{i}/s{j}/f.bin"), i + j);
            }
        }
        let tree = scan(&t.0, 8, &Progress::default());
        let want: u64 = (0..40).flat_map(|i| (0..5).map(move |j| (i + j) as u64)).sum();
        assert_eq!(tree.node(0).size, want);
        assert_eq!(tree.node(0).files, 200);
        // every child comes after its folder
        assert!(tree.nodes.iter().enumerate().skip(1).all(|(i, n)| (n.parent as usize) < i));
    }

    #[test]
    fn missing_folder_is_marked() {
        let tree = scan(Path::new(r"C:\this-folder-does-not-exist-plume"), 2, &Progress::default());
        assert_eq!(tree.len(), 1);
        assert!(tree.node(0).flags & DENIED != 0);
        assert_eq!(normalize(Path::new("D:")), PathBuf::from("D:\\"));
    }
}
