//! Cleanup suggestions and the cleanup itself.
//!
//! Each suggestion says what it is in plain words, how much space it holds, and is never ticked by default. Files go
//! to the Recycle Bin (IFileOperation with FOF_ALLOWUNDO) unless the user turned that off. Places that belong to
//! Windows need administrator rights: the app says so and offers to restart elevated, it never asks silently.
//!
//! Sandbox: with PLUME_CLEAN_SANDBOX=<folder>, every place below lives inside that folder, the real Recycle Bin is
//! neither filled nor emptied (removals there are permanent), and nothing outside the folder can be removed.

use crate::scan::{self, Tree};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Browser {
    Edge,
    Chrome,
    Firefox,
    PlumeWeb,
}

impl Browser {
    pub fn name(self) -> &'static str {
        match self {
            Browser::Edge => "Microsoft Edge",
            Browser::Chrome => "Google Chrome",
            Browser::Firefox => "Firefox",
            Browser::PlumeWeb => "Plume Web",
        }
    }
    pub fn exe(self) -> &'static str {
        match self {
            Browser::Edge => "msedge.exe",
            Browser::Chrome => "chrome.exe",
            Browser::Firefox => "firefox.exe",
            Browser::PlumeWeb => "plume.exe",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Temp,
    SystemTemp,
    RecycleBin,
    Browser(Browser),
    Thumbnails,
    CrashDumps,
    SystemDumps,
    UpdateLeftovers,
    OldDownloads,
    Large,
    Duplicates,
    EmptyFolders,
}

pub const ALL: [Kind; 15] = [
    Kind::Temp,
    Kind::RecycleBin,
    Kind::Browser(Browser::Edge),
    Kind::Browser(Browser::Chrome),
    Kind::Browser(Browser::Firefox),
    Kind::Browser(Browser::PlumeWeb),
    Kind::Thumbnails,
    Kind::CrashDumps,
    Kind::SystemTemp,
    Kind::SystemDumps,
    Kind::UpdateLeftovers,
    Kind::OldDownloads,
    Kind::Large,
    Kind::Duplicates,
    Kind::EmptyFolders,
];

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Temp => "temp",
            Kind::SystemTemp => "system_temp",
            Kind::RecycleBin => "recycle_bin",
            Kind::Browser(Browser::Edge) => "edge",
            Kind::Browser(Browser::Chrome) => "chrome",
            Kind::Browser(Browser::Firefox) => "firefox",
            Kind::Browser(Browser::PlumeWeb) => "plume_web",
            Kind::Thumbnails => "thumbnails",
            Kind::CrashDumps => "crash_dumps",
            Kind::SystemDumps => "system_dumps",
            Kind::UpdateLeftovers => "update",
            Kind::OldDownloads => "old_downloads",
            Kind::Large => "large",
            Kind::Duplicates => "duplicates",
            Kind::EmptyFolders => "empty_folders",
        }
    }
    /// The title, in English (the interface translates it).
    pub fn title(self) -> &'static str {
        match self {
            Kind::Temp => "Temporary files",
            Kind::SystemTemp => "Windows temporary files",
            Kind::RecycleBin => "Recycle Bin",
            Kind::Browser(Browser::Edge) => "Edge cache",
            Kind::Browser(Browser::Chrome) => "Chrome cache",
            Kind::Browser(Browser::Firefox) => "Firefox cache",
            Kind::Browser(Browser::PlumeWeb) => "Plume Web cache",
            Kind::Thumbnails => "Thumbnail cache",
            Kind::CrashDumps => "Crash reports",
            Kind::SystemDumps => "Windows crash dumps",
            Kind::UpdateLeftovers => "Windows Update leftovers",
            Kind::OldDownloads => "Old downloads",
            Kind::Large => "Large files",
            Kind::Duplicates => "Duplicate files",
            Kind::EmptyFolders => "Empty folders",
        }
    }
    pub fn explain(self) -> &'static str {
        match self {
            Kind::Temp => "Files apps left behind while they worked. Apps make new ones when they need them; files still in use are skipped.",
            Kind::SystemTemp => "The same for Windows itself and its services.",
            Kind::RecycleBin => "Files you already deleted. Emptying it removes them for good.",
            Kind::Browser(_) => {
                "Copies of web pages and pictures kept to load sites faster. Sites load a bit slower the first time again. Your history, passwords and sign-ins stay."
            }
            Kind::Thumbnails => "Small previews of your pictures and videos. Windows draws them again when you open a folder.",
            Kind::CrashDumps => "Reports written when an app crashed. Only useful to send to its makers.",
            Kind::SystemDumps => "Memory copies Windows wrote after a crash or an error report.",
            Kind::UpdateLeftovers => "Update files Windows already installed. Windows downloads them again if it needs them.",
            Kind::OldDownloads => "Things in your Downloads folder you have not touched for a long time. Look before you tick.",
            Kind::Large => "The biggest files of the last scan. Nothing is ticked: choose what you no longer need.",
            Kind::Duplicates => "Files with exactly the same content in several places. Keeping one copy is enough.",
            Kind::EmptyFolders => "Folders with nothing inside, from the last scan.",
        }
    }
    pub fn needs_admin(self) -> bool {
        matches!(self, Kind::SystemTemp | Kind::SystemDumps | Kind::UpdateLeftovers)
    }
    /// The user picks the files one by one.
    pub fn review(self) -> bool {
        matches!(self, Kind::OldDownloads | Kind::Large | Kind::Duplicates | Kind::EmptyFolders)
    }
    /// Found from the last scan.
    pub fn scan_based(self) -> bool {
        matches!(self, Kind::Large | Kind::Duplicates | Kind::EmptyFolders)
    }
}

/// Where things are. Real places, or all inside the sandbox folder.
#[derive(Clone, Debug)]
pub struct Places {
    pub local: PathBuf,
    pub temp: PathBuf,
    pub windows: PathBuf,
    pub program_data: PathBuf,
    pub downloads: PathBuf,
    /// The sandbox folder, when testing.
    pub sandbox: Option<PathBuf>,
}

fn env_path(k: &str) -> Option<PathBuf> {
    std::env::var_os(k).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

fn downloads_folder() -> PathBuf {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    };
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None) {
            let s = String::from_utf16_lossy(p.as_wide());
            CoTaskMemFree(Some(p.0 as _));
            return PathBuf::from(s);
        }
    }
    env_path("USERPROFILE").unwrap_or_default().join("Downloads")
}

impl Places {
    pub fn real() -> Places {
        Places {
            local: env_path("LOCALAPPDATA").unwrap_or_default(),
            temp: std::env::temp_dir(),
            windows: env_path("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows")),
            program_data: env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")),
            downloads: downloads_folder(),
            sandbox: None,
        }
    }
    pub fn sandbox(dir: &Path) -> Places {
        Places {
            local: dir.join("LocalAppData"),
            temp: dir.join("LocalAppData").join("Temp"),
            windows: dir.join("Windows"),
            program_data: dir.join("ProgramData"),
            downloads: dir.join("Downloads"),
            sandbox: Some(dir.to_path_buf()),
        }
    }
    /// The sandbox when PLUME_CLEAN_SANDBOX is set, the real places otherwise.
    pub fn current() -> Places {
        match env_path("PLUME_CLEAN_SANDBOX") {
            Some(d) => Places::sandbox(&d),
            None => Places::real(),
        }
    }
    /// The stand-in for the Recycle Bin in the sandbox.
    pub fn fake_bin(&self) -> Option<PathBuf> {
        self.sandbox.as_ref().map(|d| d.join("RecycleBin"))
    }

    /// Folders whose content a suggestion removes (the folders themselves stay).
    pub fn folders(&self, kind: Kind) -> Vec<PathBuf> {
        let l = &self.local;
        let wer = |base: &Path| vec![base.join(r"Microsoft\Windows\WER\ReportArchive"), base.join(r"Microsoft\Windows\WER\ReportQueue")];
        match kind {
            Kind::Temp => vec![self.temp.clone()],
            Kind::SystemTemp => vec![self.windows.join("Temp")],
            Kind::Browser(b) => browser_folders(l, b),
            Kind::CrashDumps => {
                let mut v = vec![l.join("CrashDumps")];
                v.extend(wer(l));
                v
            }
            Kind::SystemDumps => {
                let mut v = vec![self.windows.join("Minidump"), self.windows.join("LiveKernelReports")];
                v.extend(wer(&self.program_data));
                v
            }
            Kind::UpdateLeftovers => vec![self.windows.join(r"SoftwareDistribution\Download")],
            _ => vec![],
        }
    }

    /// Single files a suggestion removes.
    pub fn files(&self, kind: Kind) -> Vec<PathBuf> {
        match kind {
            Kind::Thumbnails => {
                let dir = self.local.join(r"Microsoft\Windows\Explorer");
                let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                        n.starts_with("thumbcache_") && n.ends_with(".db")
                    })
                    .collect();
                v.sort();
                v
            }
            Kind::SystemDumps => vec![self.windows.join("MEMORY.DMP")].into_iter().filter(|p| p.is_file()).collect(),
            _ => vec![],
        }
    }
}

fn subdirs(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        std::fs::read_dir(p).into_iter().flatten().flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect();
    v.sort();
    v
}

/// The cache folders of a browser, in every profile it has.
fn browser_folders(local: &Path, b: Browser) -> Vec<PathBuf> {
    let chromium = |data: PathBuf| {
        let mut v = vec![data.join("ShaderCache"), data.join("GrShaderCache")];
        for profile in subdirs(&data) {
            if profile.join("Cache").is_dir() || profile.join("Code Cache").is_dir() {
                v.extend(["Cache", "Code Cache", "GPUCache"].iter().map(|c| profile.join(c)));
            }
        }
        v
    };
    let mut v = match b {
        Browser::Edge => chromium(local.join(r"Microsoft\Edge\User Data")),
        Browser::Chrome => chromium(local.join(r"Google\Chrome\User Data")),
        Browser::PlumeWeb => chromium(local.join(r"Plume\EBWebView")),
        Browser::Firefox => subdirs(&local.join(r"Mozilla\Firefox\Profiles")).into_iter().flat_map(|p| [p.join("cache2"), p.join("startupCache")]).collect(),
    };
    v.retain(|p| p.is_dir());
    v
}

/// One thing a suggestion would remove.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: u64,
    pub dir: bool,
    pub selected: bool,
    /// Duplicates: the group, and whether this is the copy kept by "keep the oldest".
    pub group: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// Found from a scan: none yet.
    NeedScan,
    /// The browser is open.
    Close(&'static str),
    /// Windows' own files.
    Admin,
}

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub kind: Kind,
    pub items: Vec<Item>,
    /// Everything it holds (for the Recycle Bin: what Windows reports).
    pub size: u64,
    pub count: u64,
    pub blocked: Option<Blocked>,
    /// Ticked for the next cleanup.
    pub selected: bool,
}

impl Suggestion {
    /// What a cleanup would free now.
    pub fn selected_size(&self) -> u64 {
        if !self.selected || self.blocked.is_some() {
            return 0;
        }
        if self.kind.review() { self.items.iter().filter(|i| i.selected).map(|i| i.size).sum() } else { self.size }
    }
    pub fn to_remove(&self) -> Vec<(PathBuf, u64)> {
        if self.kind == Kind::RecycleBin {
            return vec![];
        }
        self.items.iter().filter(|i| !self.kind.review() || i.selected).map(|i| (i.path.clone(), i.size)).collect()
    }
}

/// Now, as a FILETIME.
pub fn now_filetime() -> u64 {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (secs + 11_644_473_600) * 10_000_000
}

pub fn filetime_of(t: SystemTime) -> u64 {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    (d.as_secs() + 11_644_473_600) * 10_000_000 + d.subsec_nanos() as u64 / 100
}

/// Folders that belong to Windows or to apps, or are hidden on purpose (".git"…): never offered as empty folders,
/// large files or duplicates.
pub fn protected(path: &Path, root: &Path) -> bool {
    const SYSTEM: [&str; 10] = [
        "windows",
        "program files",
        "program files (x86)",
        "programdata",
        "appdata",
        "$recycle.bin",
        "system volume information",
        "recovery",
        "$windows.~bt",
        "$windows.~ws",
    ];
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy().to_lowercase();
        s.starts_with('.') || SYSTEM.contains(&s.as_str())
    })
}

/// Entries directly inside `folders`, with their sizes.
fn contents(folders: &[PathBuf]) -> Vec<Item> {
    let mut v = Vec::new();
    for f in folders {
        for e in std::fs::read_dir(f).into_iter().flatten().flatten() {
            let path = e.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            let dir = meta.is_dir();
            let (size, _) = if dir { scan::size_of(&path) } else { (meta.len(), 1) };
            let mtime = meta.modified().map(filetime_of).unwrap_or(0);
            v.push(Item { path, size, mtime, dir, selected: true, group: 0 });
        }
    }
    v
}

/// Names of the programs running now (lowercase). In the sandbox, PLUME_CLEAN_SANDBOX_RUNNING (comma list) can
/// stand for it.
pub fn running() -> Vec<String> {
    if std::env::var_os("PLUME_CLEAN_SANDBOX").is_some()
        && let Ok(list) = std::env::var("PLUME_CLEAN_SANDBOX_RUNNING")
    {
        return list.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
    }
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS},
    };
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            out.push(String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase());
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out.sort();
    out.dedup();
    out
}

/// Running with administrator rights. In the sandbox, PLUME_CLEAN_SANDBOX_ADMIN=1 can stand for it.
pub fn elevated() -> bool {
    if std::env::var_os("PLUME_CLEAN_SANDBOX").is_some() {
        return std::env::var("PLUME_CLEAN_SANDBOX_ADMIN").is_ok_and(|v| v == "1");
    }
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() }
}

/// The Recycle Bin's size and item count, as Windows reports them (all drives).
pub fn bin_size(places: &Places) -> (u64, u64) {
    if let Some(fake) = places.fake_bin() {
        return scan::size_of(&fake);
    }
    use windows::Win32::UI::Shell::{SHQUERYRBINFO, SHQueryRecycleBinW};
    let mut info = SHQUERYRBINFO { cbSize: size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    match unsafe { SHQueryRecycleBinW(windows::core::PCWSTR::null(), &mut info) } {
        Ok(()) => (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64),
        Err(_) => (0, 0),
    }
}

/// What the suggestion rules need besides the places.
pub struct Rules {
    pub old_days: u32,
    pub large_bytes: u64,
    pub running: Vec<String>,
    pub elevated: bool,
    pub now: u64,
}

/// One suggestion, measured now. Duplicates come from [`with_duplicates`] (they need reading the files).
pub fn suggest(kind: Kind, places: &Places, tree: Option<&Tree>, rules: &Rules) -> Suggestion {
    let mut items = match kind {
        Kind::RecycleBin => vec![],
        Kind::OldDownloads => old_downloads(&places.downloads, rules.old_days, rules.now),
        Kind::Large => tree.map(|t| large_files(t, rules.large_bytes)).unwrap_or_default(),
        Kind::EmptyFolders => tree.map(empty_folders).unwrap_or_default(),
        Kind::Duplicates => vec![],
        _ => {
            let mut v = contents(&places.folders(kind));
            for f in places.files(kind) {
                let meta = std::fs::metadata(&f).ok();
                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let mtime = meta.and_then(|m| m.modified().ok()).map(filetime_of).unwrap_or(0);
                v.push(Item { path: f, size, mtime, dir: false, selected: true, group: 0 });
            }
            v
        }
    };
    if kind.review() {
        for i in items.iter_mut() {
            i.selected = false;
        }
    }
    items.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    let (size, count) = if kind == Kind::RecycleBin { bin_size(places) } else { (items.iter().map(|i| i.size).sum(), items.len() as u64) };
    let blocked = if kind.scan_based() && tree.is_none() {
        Some(Blocked::NeedScan)
    } else if let Kind::Browser(b) = kind
        && rules.running.iter().any(|r| r == b.exe())
        && count > 0
    {
        Some(Blocked::Close(b.name()))
    } else if kind.needs_admin() && !rules.elevated && count > 0 {
        Some(Blocked::Admin)
    } else {
        None
    };
    Suggestion { kind, items, size, count, blocked, selected: false }
}

/// Duplicates from the groups found by [`crate::dupes::find`]: every copy listed, none ticked.
pub fn with_duplicates(groups: &[crate::dupes::Group]) -> Suggestion {
    let mut items = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        for (path, size, mtime) in group {
            items.push(Item { path: path.clone(), size: *size, mtime: *mtime, dir: false, selected: false, group: g as u32 });
        }
    }
    // what could be freed: all copies but one
    let size = groups.iter().map(|g| g[0].1 * (g.len() as u64 - 1)).sum();
    let count = items.len() as u64;
    Suggestion { kind: Kind::Duplicates, items, size, count, blocked: None, selected: false }
}

/// Ticks every copy but the oldest of each group.
pub fn keep_oldest(items: &mut [Item]) {
    let mut seen = std::collections::HashSet::new();
    for i in items.iter_mut() {
        i.selected = !seen.insert(i.group);
    }
}

/// Top-level things in Downloads whose newest file is older than `days`.
pub fn old_downloads(dir: &Path, days: u32, now: u64) -> Vec<Item> {
    let limit = now.saturating_sub(days as u64 * 86_400 * 10_000_000);
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = e.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if name == "desktop.ini" {
            continue;
        }
        let dir = meta.is_dir();
        let (size, newest) = if dir {
            let tree = scan::scan(&path, 2, &scan::Progress::default());
            // the newest file inside tells (an unpacked folder gets today's date itself); an empty one: its own date
            let inside = tree.node(0).mtime;
            (tree.node(0).size, if inside > 0 { inside } else { meta.modified().map(filetime_of).unwrap_or(0) })
        } else {
            (meta.len(), meta.modified().map(filetime_of).unwrap_or(0))
        };
        if newest > 0 && newest < limit {
            v.push(Item { path, size, mtime: newest, dir, selected: false, group: 0 });
        }
    }
    v
}

pub fn large_files(tree: &Tree, min: u64) -> Vec<Item> {
    let mut v: Vec<Item> = tree
        .files_at_least(min)
        .into_iter()
        .filter(|&i| tree.node(i).flags & (scan::SYSTEM | scan::LINK | scan::CLOUD) == 0)
        .map(|i| (i, tree.path(i)))
        .filter(|(_, p)| !protected(p, &tree.root))
        .map(|(i, path)| Item { path, size: tree.node(i).size, mtime: tree.node(i).mtime, dir: false, selected: false, group: 0 })
        .collect();
    v.sort_by_key(|i| std::cmp::Reverse(i.size));
    v.truncate(500);
    v
}

/// Files of the scan worth comparing for duplicates (path, size, mtime).
pub fn duplicate_candidates(tree: &Tree, min: u64) -> Vec<(PathBuf, u64, u64)> {
    tree.files_at_least(min.max(1))
        .into_iter()
        .filter(|&i| tree.node(i).flags & (scan::SYSTEM | scan::LINK | scan::CLOUD) == 0)
        .map(|i| (tree.path(i), tree.node(i).size, tree.node(i).mtime))
        .filter(|(p, _, _)| !protected(p, &tree.root))
        .collect()
}

pub fn empty_folders(tree: &Tree) -> Vec<Item> {
    tree.empty_dirs(|i| protected(&tree.path(i), &tree.root))
        .into_iter()
        .map(|i| (i, tree.path(i)))
        .map(|(i, path)| Item { path, size: 0, mtime: tree.node(i).mtime, dir: true, selected: false, group: 0 })
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub freed: u64,
    pub removed: u64,
    /// Still there (in use, refused).
    pub failed: u64,
    /// Not touched because the path is not allowed.
    pub refused: u64,
}

impl Outcome {
    pub fn add(&mut self, o: &Outcome) {
        self.freed += o.freed;
        self.removed += o.removed;
        self.failed += o.failed;
        self.refused += o.refused;
    }
}

/// Paths the app never removes, whatever a list says: drive roots, Windows, the user folder itself, and in the
/// sandbox anything outside the sandbox folder.
pub fn allowed(path: &Path, places: &Places) -> bool {
    if !path.is_absolute() || path.parent().is_none() || path.components().count() < 3 {
        return false;
    }
    if let Some(sb) = &places.sandbox {
        return path.starts_with(sb) && path != sb;
    }
    let same = |a: &Path, b: &Path| a.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches('\\'));
    let home = env_path("USERPROFILE").unwrap_or_default();
    let mut never = vec![places.windows.clone(), places.program_data.clone(), places.local.clone(), places.downloads.clone(), home.join("AppData")];
    never.extend(["ProgramFiles", "ProgramFiles(x86)", "APPDATA", "USERPROFILE"].iter().filter_map(|k| env_path(k)));
    !never.iter().any(|n| !n.as_os_str().is_empty() && same(path, n))
}

/// Removes `paths` (path, size known before), to the Recycle Bin when `recycle`. Runs IFileOperation on the calling
/// thread (initializes COM there). In the sandbox, never to the Recycle Bin.
pub fn remove(paths: &[(PathBuf, u64)], recycle: bool, places: &Places) -> Outcome {
    use windows::Win32::{
        System::Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx},
        UI::Shell::{
            FOF_ALLOWUNDO, FOF_NO_UI, FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName,
        },
    };
    let recycle = recycle && places.sandbox.is_none();
    let mut out = Outcome::default();
    let mut todo = Vec::new();
    for (p, size) in paths {
        if allowed(p, places) && std::fs::symlink_metadata(p).is_ok() {
            todo.push((p.clone(), *size));
        } else if !allowed(p, places) {
            out.refused += 1;
        }
    }
    if todo.is_empty() {
        return out;
    }
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(op) = CoCreateInstance::<_, IFileOperation>(&FileOperation, None, CLSCTX_ALL) {
            let mut flags = FOF_NO_UI.0;
            if recycle {
                flags |= FOF_ALLOWUNDO.0 | FOFX_RECYCLEONDELETE.0;
            }
            // keep going past a file in use
            flags &= !FOFX_EARLYFAILURE.0;
            let _ = op.SetOperationFlags(windows::Win32::UI::Shell::FILEOPERATION_FLAGS(flags));
            for (p, _) in &todo {
                let w: Vec<u16> = p.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect();
                if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(windows::core::PCWSTR(w.as_ptr()), None) {
                    let _ = op.DeleteItem(&item, None);
                }
            }
            let _ = op.PerformOperations();
        }
    }
    for (p, size) in &todo {
        if std::fs::symlink_metadata(p).is_err() {
            out.freed += size;
            out.removed += 1;
        } else {
            // a folder partly emptied: what left it counts
            let (left, _) = scan::size_of(p);
            out.freed += size.saturating_sub(left);
            out.failed += 1;
        }
    }
    out
}

/// Empties the Recycle Bin (the sandbox's stand-in when testing: the real one is never touched there).
pub fn empty_bin(places: &Places) -> Outcome {
    let (size, count) = bin_size(places);
    if let Some(fake) = places.fake_bin() {
        let items: Vec<(PathBuf, u64)> = contents(&[fake]).into_iter().map(|i| (i.path, i.size)).collect();
        return remove(&items, false, places);
    }
    use windows::Win32::UI::Shell::{SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW};
    let ok = unsafe { SHEmptyRecycleBinW(None, windows::core::PCWSTR::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) }.is_ok();
    let (after, left) = bin_size(places);
    Outcome { freed: size.saturating_sub(after), removed: count.saturating_sub(left), failed: if ok { 0 } else { left }, refused: 0 }
}

/// "1.2 GB", "340 MB", "12 KB", "0 B" (decimal units, like Windows' drive bars).
pub fn human(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1000.0 && u < U.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    let s = if u == 0 || v >= 100.0 {
        format!("{v:.0}")
    } else if v >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    };
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    let s = if crate::i18n::french() { s.replace('.', ",") } else { s };
    format!("{s} {}", if crate::i18n::french() { U_FR[u] } else { U[u] })
}

const U_FR: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tests::TempDir;

    fn rules(old_days: u32) -> Rules {
        Rules { old_days, large_bytes: 1000, running: vec![], elevated: false, now: now_filetime() }
    }

    fn set_old(p: &Path, days: u64) {
        let t = SystemTime::now() - std::time::Duration::from_secs(days * 86_400);
        let f = std::fs::File::options().write(true).open(p).unwrap();
        f.set_modified(t).unwrap();
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(999), "999 B");
        assert_eq!(human(1_500), "1.5 KB");
        assert_eq!(human(12_340_000), "12.3 MB");
        assert_eq!(human(2_000_000_000), "2 GB");
        assert_eq!(human(476_000_000_000), "476 GB");
    }

    #[test]
    fn protected_paths() {
        let root = Path::new(r"C:\");
        assert!(protected(Path::new(r"C:\Windows\Temp\x"), root));
        assert!(protected(Path::new(r"C:\Users\a\AppData\Local\x"), root));
        assert!(protected(Path::new(r"C:\code\proj\.git\refs\tags"), root));
        assert!(!protected(Path::new(r"C:\Users\a\Videos\big.mkv"), root));
        // relative to the scanned folder: a scan of AppData itself still protects
        assert!(protected(Path::new(r"D:\scan\Program Files\x"), Path::new(r"D:\scan")));
    }

    #[test]
    fn allowed_paths() {
        let real = Places::real();
        assert!(!allowed(Path::new(r"C:\"), &real));
        assert!(!allowed(Path::new(r"C:\Windows"), &real));
        assert!(!allowed(Path::new("relative"), &real));
        assert!(!allowed(&real.local, &real));
        if let Some(pf) = env_path("ProgramFiles") {
            assert!(!allowed(&pf, &real));
        }
        let sb = Places::sandbox(Path::new(r"C:\sandbox-x"));
        assert!(allowed(Path::new(r"C:\sandbox-x\Temp\a.tmp"), &sb));
        assert!(!allowed(Path::new(r"C:\sandbox-x"), &sb));
        assert!(!allowed(Path::new(r"C:\Users\someone\file.txt"), &sb));
    }

    #[test]
    fn sandbox_cleanup_and_rules() {
        let t = TempDir::new("sandbox");
        let places = Places::sandbox(&t.0);
        t.file(r"LocalAppData\Temp\a.tmp", 1000);
        t.file(r"LocalAppData\Temp\sub\b.tmp", 3000);
        t.file(r"LocalAppData\Microsoft\Edge\User Data\Default\Cache\Cache_Data\f_0001", 5000);
        t.file(r"LocalAppData\Microsoft\Windows\Explorer\thumbcache_256.db", 700);
        t.file(r"LocalAppData\Microsoft\Windows\Explorer\iconcache_16.db", 99);
        t.file(r"Windows\SoftwareDistribution\Download\kb.cab", 4000);
        t.file(r"RecycleBin\deleted.txt", 300);
        let old = t.file(r"Downloads\old-setup.exe", 2000);
        t.file(r"Downloads\new.pdf", 10);
        set_old(&old, 400);
        let unpacked = t.file(r"Downloads\unpacked\readme.txt", 30);
        set_old(&unpacked, 200);
        t.file(r"Downloads\fresh\a.txt", 30);
        let r = rules(90);

        let temp = suggest(Kind::Temp, &places, None, &r);
        assert_eq!((temp.size, temp.count, temp.blocked.clone()), (4000, 2, None));
        assert!(!temp.selected);

        let edge = suggest(Kind::Browser(Browser::Edge), &places, None, &r);
        assert_eq!(edge.size, 5000);
        let open = Rules { running: vec!["msedge.exe".into()], ..rules(90) };
        assert_eq!(suggest(Kind::Browser(Browser::Edge), &places, None, &open).blocked, Some(Blocked::Close("Microsoft Edge")));

        let thumbs = suggest(Kind::Thumbnails, &places, None, &r);
        assert_eq!((thumbs.size, thumbs.count), (700, 1));

        let update = suggest(Kind::UpdateLeftovers, &places, None, &r);
        assert_eq!((update.size, update.blocked.clone()), (4000, Some(Blocked::Admin)));

        let old_dl = suggest(Kind::OldDownloads, &places, None, &r);
        assert_eq!(old_dl.items.len(), 2);
        assert!(old_dl.items[0].path.ends_with("old-setup.exe") && !old_dl.items[0].selected);
        assert!(old_dl.items[1].path.ends_with("unpacked") && old_dl.items[1].dir);

        assert_eq!(suggest(Kind::Large, &places, None, &r).blocked, Some(Blocked::NeedScan));
        assert_eq!(suggest(Kind::RecycleBin, &places, None, &r).size, 300);

        // the cleanup: everything in the temp folder goes, the folder stays
        let mut temp = temp;
        temp.selected = true;
        assert_eq!(temp.selected_size(), 4000);
        let out = remove(&temp.to_remove(), true, &places);
        assert_eq!((out.freed, out.removed, out.failed), (4000, 2, 0));
        assert!(places.temp.is_dir() && std::fs::read_dir(&places.temp).unwrap().next().is_none());

        let bin = empty_bin(&places);
        assert_eq!(bin.freed, 300);

        // nothing outside the sandbox, even if asked
        let outside = std::env::temp_dir().join(format!("plume-clean-outside-{}", std::process::id()));
        std::fs::write(&outside, b"keep").unwrap();
        let out = remove(&[(outside.clone(), 4)], false, &places);
        assert_eq!(out.refused, 1);
        assert!(outside.exists());
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn scan_based_rules() {
        let t = TempDir::new("scanrules");
        t.file(r"Videos\big.mkv", 5000);
        t.file(r"Videos\small.txt", 10);
        t.file(r"AppData\Local\huge.bin", 9000);
        std::fs::create_dir_all(t.0.join(r"Projects\empty\deeper")).unwrap();
        std::fs::create_dir_all(t.0.join(r"Projects\.git\refs")).unwrap();
        let tree = scan::scan(&t.0, 4, &scan::Progress::default());
        let r = rules(90);
        let large = suggest(Kind::Large, &PathsOnly::places(&t.0), Some(&tree), &r);
        assert_eq!(large.items.len(), 1, "AppData is left out");
        assert!(large.items[0].path.ends_with("big.mkv"));
        let empty = suggest(Kind::EmptyFolders, &PathsOnly::places(&t.0), Some(&tree), &r);
        assert_eq!(empty.items.len(), 1);
        assert!(empty.items[0].path.ends_with("empty"));

        let mut d = with_duplicates(&[vec![(t.0.join("a"), 10, 1), (t.0.join("b"), 10, 2), (t.0.join("c"), 10, 3)]]);
        assert_eq!(d.size, 20);
        keep_oldest(&mut d.items);
        assert_eq!(d.items.iter().map(|i| i.selected).collect::<Vec<_>>(), [false, true, true]);
        d.selected = true;
        assert_eq!(d.selected_size(), 20);
    }

    struct PathsOnly;
    impl PathsOnly {
        fn places(dir: &Path) -> Places {
            Places::sandbox(dir)
        }
    }
}
