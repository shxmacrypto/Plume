//! Lecture d'un dossier, le plus vite possible : un seul handle sur le dossier et de gros tampons
//! (`GetFileInformationByHandleEx`), donc très peu d'appels système même pour 100 000 fichiers.
//! Chaque entrée arrive avec sa taille, ses dates, ses attributs et son identifiant NTFS (utile à l'index).

use crate::wide::long_path;
use std::{
    io,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, ERROR_NOT_SUPPORTED, HANDLE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            FIND_FIRST_EX_LARGE_FETCH, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, FindClose, FindExInfoBasic, FindExSearchNameMatch,
            FindFirstFileExW, FindNextFileW, GetFileInformationByHandleEx, OPEN_EXISTING, SYNCHRONIZE, WIN32_FIND_DATAW,
        },
    },
    core::PCWSTR,
};

pub const ATTR_READONLY: u32 = 0x1;
pub const ATTR_HIDDEN: u32 = 0x2;
pub const ATTR_SYSTEM: u32 = 0x4;
pub const ATTR_DIRECTORY: u32 = 0x10;
pub const ATTR_REPARSE: u32 = 0x400;
pub const ATTR_OFFLINE: u32 = 0x1000;
/// Le contenu n'est pas sur le disque (fichier OneDrive « en ligne seulement »).
pub const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
const TAG_MOUNT_POINT: u32 = 0xA000_0003;
const TAG_SYMLINK: u32 = 0xA000_000C;

/// Taille du tampon de lecture : ~2 000 entrées par appel système.
const BUF_BYTES: usize = 256 * 1024;

/// Une entrée telle que Windows la donne, nom en UTF-16 emprunté au tampon (aucune allocation).
pub struct RawEntry<'a> {
    pub name: &'a [u16],
    pub attrs: u32,
    pub size: u64,
    /// Dates au format FILETIME (pas de 100 ns depuis 1601, UTC).
    pub mtime: u64,
    pub ctime: u64,
    /// Identifiant NTFS du fichier ; 0 quand le système de fichiers n'en donne pas.
    pub file_id: u64,
    pub reparse_tag: u32,
}

/// Une entrée de dossier, prête pour l'affichage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub name: String,
    pub attrs: u32,
    pub size: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub file_id: u64,
    pub reparse_tag: u32,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.attrs & ATTR_DIRECTORY != 0
    }
    pub fn is_hidden(&self) -> bool {
        self.attrs & ATTR_HIDDEN != 0
    }
    pub fn is_system(&self) -> bool {
        self.attrs & ATTR_SYSTEM != 0
    }
    /// Lien symbolique ou jonction.
    pub fn is_link(&self) -> bool {
        self.attrs & ATTR_REPARSE != 0 && matches!(self.reparse_tag, TAG_MOUNT_POINT | TAG_SYMLINK)
    }
    pub fn is_cloud_only(&self) -> bool {
        self.attrs & (ATTR_RECALL_ON_DATA_ACCESS | ATTR_OFFLINE) != 0
    }
    /// Extension sans le point, vide pour un dossier ou un nom sans extension (« .gitignore » n'en a pas).
    pub fn ext(&self) -> &str {
        if self.is_dir() {
            return "";
        }
        match self.name.rfind('.') {
            Some(i) if i > 0 && i + 1 < self.name.len() => &self.name[i + 1..],
            _ => "",
        }
    }
}

impl From<&RawEntry<'_>> for Entry {
    fn from(r: &RawEntry<'_>) -> Self {
        Entry {
            name: String::from_utf16_lossy(r.name),
            attrs: r.attrs,
            size: r.size,
            mtime: r.mtime,
            ctime: r.ctime,
            file_id: r.file_id,
            reparse_tag: r.reparse_tag,
        }
    }
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Erreur Windows → `io::Error` avec le vrai code (ERROR_ACCESS_DENIED → PermissionDenied…).
pub fn to_io(e: windows::core::Error) -> io::Error {
    let hr = e.code().0 as u32;
    if hr & 0xFFFF_0000 == 0x8007_0000 { io::Error::from_raw_os_error((hr & 0xFFFF) as i32) } else { io::Error::other(e.message()) }
}

fn is(e: &windows::core::Error, code: windows::Win32::Foundation::WIN32_ERROR) -> bool {
    e.code() == code.to_hresult()
}

fn cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|c| c.load(Ordering::Relaxed))
}

/// Identifiant NTFS d'un dossier (celui que le journal des modifications cite), ou None s'il est illisible.
pub fn dir_id(path: &Path) -> Option<u64> {
    use windows::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
    let w = long_path(path);
    let h = unsafe {
        CreateFileW(
            PCWSTR(w.as_ptr()),
            SYNCHRONIZE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .ok()?;
    let h = Handle(h);
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(h.0, &mut info) }.ok()?;
    Some(((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64)
}

/// Lit le dossier `path` et appelle `f` pour chaque entrée (sans « . » ni « .. »).
/// `cancel` passé à vrai arrête la lecture (erreur `Interrupted`).
pub fn read_dir(path: &Path, cancel: Option<&AtomicBool>, f: &mut dyn FnMut(&RawEntry)) -> io::Result<()> {
    read_dir_w(&long_path(path), cancel, f)
}

/// Comme [`read_dir`], avec un chemin UTF-16 déjà préfixé `\\?\` et terminé par zéro.
pub fn read_dir_w(path_w: &[u16], cancel: Option<&AtomicBool>, f: &mut dyn FnMut(&RawEntry)) -> io::Result<()> {
    // tampon aligné sur 8 octets, comme les enregistrements renvoyés
    let mut buf = vec![0u64; BUF_BYTES / 8];
    read_dir_buf(path_w, cancel, &mut buf, f)
}

/// Comme [`read_dir_w`], avec un tampon fourni par l'appelant : l'indexation réutilise le sien d'un dossier à
/// l'autre au lieu d'en préparer un neuf pour chacun des centaines de milliers de dossiers d'un disque.
pub fn read_dir_buf(path_w: &[u16], cancel: Option<&AtomicBool>, buf: &mut [u64], f: &mut dyn FnMut(&RawEntry)) -> io::Result<()> {
    debug_assert_eq!(path_w.last(), Some(&0));
    let bytes = (buf.len() * 8) as u32;
    let h = unsafe {
        CreateFileW(
            PCWSTR(path_w.as_ptr()),
            (FILE_LIST_DIRECTORY | SYNCHRONIZE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .map_err(to_io)?;
    let h = Handle(h);
    let mut class = FileIdBothDirectoryRestartInfo;
    let mut first = true;
    loop {
        if cancelled(cancel) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if let Err(e) = unsafe { GetFileInformationByHandleEx(h.0, class, buf.as_mut_ptr() as _, bytes) } {
            if is(&e, ERROR_NO_MORE_FILES) {
                return Ok(());
            }
            // système de fichiers sans cette classe d'information (certains partages, lecteurs exotiques)
            if first && (is(&e, ERROR_INVALID_PARAMETER) || is(&e, ERROR_NOT_SUPPORTED) || is(&e, ERROR_INVALID_FUNCTION)) {
                drop(h);
                return read_dir_find(path_w, cancel, f);
            }
            return Err(to_io(e));
        }
        first = false;
        class = FileIdBothDirectoryInfo;
        let base = buf.as_ptr() as *const u8;
        let mut off = 0usize;
        loop {
            // SAFETY: Windows garantit des enregistrements complets, alignés sur 8 octets, chaînés par NextEntryOffset
            let (next, entry) = unsafe {
                let p = base.add(off) as *const FILE_ID_BOTH_DIR_INFO;
                let info = &*p;
                let name = std::slice::from_raw_parts(std::ptr::addr_of!((*p).FileName) as *const u16, info.FileNameLength as usize / 2);
                (
                    info.NextEntryOffset as usize,
                    RawEntry {
                        name,
                        attrs: info.FileAttributes,
                        size: info.EndOfFile as u64,
                        mtime: info.LastWriteTime as u64,
                        ctime: info.CreationTime as u64,
                        file_id: info.FileId as u64,
                        // pour un point d'analyse, EaSize contient son étiquette (lien, jonction, OneDrive…)
                        reparse_tag: if info.FileAttributes & ATTR_REPARSE != 0 { info.EaSize } else { 0 },
                    },
                )
            };
            if !is_dot(entry.name) {
                f(&entry);
            }
            if next == 0 {
                break;
            }
            off += next;
        }
    }
}

fn is_dot(name: &[u16]) -> bool {
    const DOT: u16 = b'.' as u16;
    matches!(name, [DOT] | [DOT, DOT])
}

/// Lecture classique (FindFirstFile) pour les systèmes de fichiers qui refusent la lecture par handle.
fn read_dir_find(path_w: &[u16], cancel: Option<&AtomicBool>, f: &mut dyn FnMut(&RawEntry)) -> io::Result<()> {
    let mut pattern: Vec<u16> = path_w[..path_w.len() - 1].to_vec();
    if pattern.last() != Some(&(b'\\' as u16)) {
        pattern.push(b'\\' as u16);
    }
    pattern.extend_from_slice(&[b'*' as u16, 0]);
    let mut d = WIN32_FIND_DATAW::default();
    let h =
        unsafe { FindFirstFileExW(PCWSTR(pattern.as_ptr()), FindExInfoBasic, &mut d as *mut _ as _, FindExSearchNameMatch, None, FIND_FIRST_EX_LARGE_FETCH) }
            .map_err(to_io)?;
    struct Find(HANDLE);
    impl Drop for Find {
        fn drop(&mut self) {
            unsafe {
                let _ = FindClose(self.0);
            }
        }
    }
    let h = Find(h);
    let ft = |t: windows::Win32::Foundation::FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
    loop {
        if cancelled(cancel) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let len = d.cFileName.iter().position(|&c| c == 0).unwrap_or(d.cFileName.len());
        let name = &d.cFileName[..len];
        if !is_dot(name) {
            f(&RawEntry {
                name,
                attrs: d.dwFileAttributes,
                size: ((d.nFileSizeHigh as u64) << 32) | d.nFileSizeLow as u64,
                mtime: ft(d.ftLastWriteTime),
                ctime: ft(d.ftCreationTime),
                file_id: 0,
                reparse_tag: if d.dwFileAttributes & ATTR_REPARSE != 0 { d.dwReserved0 } else { 0 },
            });
        }
        if let Err(e) = unsafe { FindNextFileW(h.0, &mut d) } {
            return if is(&e, ERROR_NO_MORE_FILES) { Ok(()) } else { Err(to_io(e)) };
        }
    }
}

/// Toutes les entrées de `path`, dans l'ordre du disque (voir [`crate::sort`] pour les trier).
pub fn list_dir(path: &Path, cancel: Option<&AtomicBool>) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    read_dir(path, cancel, &mut |r| out.push(Entry::from(r)))?;
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Dossier temporaire propre à un test, supprimé à la fin.
    pub struct TempDir(pub PathBuf);
    impl TempDir {
        pub fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("fex-core-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn lists_files_and_folders() {
        let t = TempDir::new("list");
        std::fs::write(t.0.join("a.txt"), b"hello").unwrap();
        std::fs::write(t.0.join("été à Paris.md"), b"").unwrap();
        std::fs::write(t.0.join(".gitignore"), b"x").unwrap();
        std::fs::create_dir(t.0.join("sub")).unwrap();
        let mut l = list_dir(&t.0, None).unwrap();
        l.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<&str> = l.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, [".gitignore", "a.txt", "sub", "été à Paris.md"]);
        let a = &l[1];
        assert_eq!((a.size, a.is_dir(), a.ext()), (5, false, "txt"));
        assert!(a.mtime > 0 && a.ctime > 0);
        assert!(a.file_id != 0, "NTFS donne un identifiant de fichier");
        assert!(l[2].is_dir());
        assert_eq!(l[2].ext(), "");
        assert_eq!(l[0].ext(), "", "« .gitignore » n'a pas d'extension");
        assert_eq!(l[3].ext(), "md");
    }

    #[test]
    fn big_folder_spans_several_buffers() {
        let t = TempDir::new("big");
        // des noms longs pour dépasser un tampon de 256 Ko (≈ 130 octets par entrée)
        for i in 0..3000 {
            std::fs::write(t.0.join(format!("fichier-numero-{i:05}-avec-un-nom-assez-long.txt")), b"").unwrap();
        }
        let l = list_dir(&t.0, None).unwrap();
        assert_eq!(l.len(), 3000);
        let mut names: Vec<&str> = l.iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 3000, "aucune entrée en double ni perdue entre deux tampons");
    }

    #[test]
    fn errors_and_cancel() {
        let t = TempDir::new("err");
        let missing = list_dir(&t.0.join("nope"), None).unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        let stop = AtomicBool::new(true);
        assert_eq!(list_dir(&t.0, Some(&stop)).unwrap_err().kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn find_fallback_gives_the_same_entries() {
        let t = TempDir::new("find");
        std::fs::write(t.0.join("x.bin"), [0u8; 10]).unwrap();
        std::fs::create_dir(t.0.join("d")).unwrap();
        let mut a = vec![];
        read_dir_find(&long_path(&t.0), None, &mut |r| a.push(Entry::from(r))).unwrap();
        a.sort_by(|x, y| x.name.cmp(&y.name));
        assert_eq!(a.iter().map(|e| (e.name.as_str(), e.size, e.is_dir())).collect::<Vec<_>>(), [("d", 0, true), ("x.bin", 10, false)]);
    }
}
