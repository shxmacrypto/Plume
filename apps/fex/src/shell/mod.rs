//! Ce que Windows sait faire mieux que nous : icônes des fichiers, dossiers connus, lecteurs, ouverture d'un fichier
//! avec son application. Tout passe par les API du shell, pour se comporter comme l'Explorateur.

pub mod archive;
pub mod dnd;
pub mod icons;
pub mod integration;
pub mod menu;
pub mod ops;
pub mod places;
pub mod sftp;
pub mod undo;
/// Les boîtes de dialogue de Windows (où enregistrer, quel dossier, quels fichiers) : celles du kit commun.
pub use crate::ui::dialogs;

use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ},
        UI::{
            Shell::{IShellLinkW, ShellExecuteW, ShellLink},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{Interface, PCWSTR, w},
};

/// Chaîne UTF-16 terminée par zéro, pour un chemin.
pub fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Chaîne d'un tampon UTF-16 terminé par zéro.
pub fn from_wide(buf: &[u16]) -> String {
    String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())])
}

/// Ouvre un fichier avec son application par défaut (comme un double-clic dans l'Explorateur).
/// Faux si Windows n'a pas pu l'ouvrir.
pub fn open(owner: HWND, path: &Path) -> bool {
    let file = wide(path);
    let dir = path.parent().map(wide);
    let r = unsafe {
        ShellExecuteW(Some(owner), w!("open"), PCWSTR(file.as_ptr()), None, dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())), SW_SHOWNORMAL)
    };
    r.0 as usize > 32
}

/// Ouvre une adresse web ou tout ce que Windows sait lancer d'après son nom (« https://… », « mailto:… »).
pub fn open_text(owner: HWND, text: &str) -> bool {
    let t: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let r = unsafe { ShellExecuteW(Some(owner), w!("open"), PCWSTR(t.as_ptr()), None, None, SW_SHOWNORMAL) };
    r.0 as usize > 32
}

/// Ouvre un terminal dans un dossier : Windows Terminal s'il est installé, PowerShell sinon.
pub fn open_terminal(owner: HWND, dir: &Path) -> bool {
    let d = wide(dir);
    let args: Vec<u16> = format!("-d \"{}\"", dir.display()).encode_utf16().chain(Some(0)).collect();
    unsafe {
        let r = ShellExecuteW(Some(owner), w!("open"), w!("wt.exe"), PCWSTR(args.as_ptr()), PCWSTR(d.as_ptr()), SW_SHOWNORMAL);
        if r.0 as usize > 32 {
            return true;
        }
        ShellExecuteW(Some(owner), w!("open"), w!("powershell.exe"), None, PCWSTR(d.as_ptr()), SW_SHOWNORMAL).0 as usize > 32
    }
}

/// Cible d'un raccourci `.lnk` (None si ce n'en est pas un, ou s'il ne pointe pas vers un fichier).
pub fn link_target(lnk: &Path) -> Option<PathBuf> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        let name = wide(lnk);
        file.Load(PCWSTR(name.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let s = from_wide(&buf);
        (!s.is_empty()).then(|| PathBuf::from(s))
    }
}
