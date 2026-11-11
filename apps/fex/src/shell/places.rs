//! Les lieux de Windows : dossiers connus (Bureau, Téléchargements…) et lecteurs.

use super::{icons, wide};
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Storage::FileSystem::{GetDiskFreeSpaceExW, GetLogicalDrives},
        System::Com::CoTaskMemFree,
        UI::Shell::{
            FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Videos, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
        },
    },
    core::{GUID, PCWSTR},
};

#[derive(Clone, Debug)]
pub struct Place {
    /// Nom affiché par Windows (traduit : « Téléchargements »).
    pub name: String,
    pub path: PathBuf,
    pub icon: i32,
}

#[derive(Clone, Debug)]
pub struct Drive {
    /// Racine : « C:\ ».
    pub root: PathBuf,
    /// « Windows (C:) ».
    pub label: String,
    /// « Disque local », « Lecteur USB »…
    pub type_name: String,
    pub total: u64,
    pub free: u64,
    pub icon: i32,
}

fn known(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.map(PathBuf::from)
    }
}

fn place(path: PathBuf) -> Place {
    let i = icons::exact(&path);
    let name = i.as_ref().map(|i| i.display.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| last_name(&path));
    Place { name, icon: i.map_or(-1, |i| i.icon), path }
}

/// Un dossier quelconque comme lieu de la barre latérale (dossier épinglé). Lit le disque.
pub fn place_of(path: &Path) -> Option<Place> {
    path.is_dir().then(|| place(path.to_path_buf()))
}

/// Dernier élément d'un chemin, ou le chemin entier pour une racine.
pub fn last_name(p: &Path) -> String {
    p.file_name().map_or_else(|| p.display().to_string().trim_end_matches('\\').to_string(), |n| n.to_string_lossy().into_owned())
}

/// Les dossiers personnels, dans l'ordre de l'Explorateur. Lit le disque : à appeler hors du fil de l'interface.
pub fn personal() -> Vec<Place> {
    [FOLDERID_Desktop, FOLDERID_Downloads, FOLDERID_Documents, FOLDERID_Pictures, FOLDERID_Music, FOLDERID_Videos]
        .iter()
        .filter_map(known)
        .filter(|p| p.is_dir())
        .map(place)
        .collect()
}

/// Les lecteurs prêts à l'emploi (un lecteur sans disque est ignoré). Peut attendre un lecteur réseau :
/// à appeler hors du fil de l'interface.
pub fn drives() -> Vec<Drive> {
    let mask = unsafe { GetLogicalDrives() };
    let mut out = vec![];
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let root = PathBuf::from(format!("{}:\\", (b'A' + i as u8) as char));
        let w = wide(&root);
        let (mut free, mut total) = (0u64, 0u64);
        // pas de disque dans le lecteur : rien à montrer
        if unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), None, Some(&mut total), Some(&mut free)) }.is_err() {
            continue;
        }
        let info = icons::exact(&root);
        let letter = root.display().to_string().trim_end_matches('\\').to_string();
        out.push(Drive {
            label: info.as_ref().map(|i| i.display.clone()).filter(|n| !n.is_empty()).unwrap_or(letter),
            type_name: info.as_ref().map(|i| i.type_name.clone()).unwrap_or_default(),
            icon: info.map_or(-1, |i| i.icon),
            total,
            free,
            root,
        });
    }
    out
}
