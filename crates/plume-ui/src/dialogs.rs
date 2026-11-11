//! Les boîtes de dialogue de Windows dont l'application a besoin : où enregistrer, quel dossier, quels fichiers.
//!
//! Pendant les essais automatiques, aucune ne s'ouvre : la réponse vient d'une variable d'environnement
//! (PLUME_TEST_SAVE, PLUME_TEST_FOLDER, PLUME_TEST_FILES), ou c'est comme si l'utilisateur annulait.

use std::path::PathBuf;
use windows::Win32::Foundation::HWND;

#[cfg(not(feature = "test-hook"))]
mod real {
    use std::path::PathBuf;
    use windows::{
        Win32::{
            Foundation::HWND,
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree},
            UI::Shell::{
                Common::COMDLG_FILTERSPEC, FOS_ALLOWMULTISELECT, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, FileSaveDialog, IFileOpenDialog,
                IFileSaveDialog, IShellItem, SIGDN_FILESYSPATH,
            },
        },
        core::PCWSTR,
    };

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    unsafe fn path_of(item: &IShellItem) -> Option<PathBuf> {
        unsafe {
            let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let path = PathBuf::from(String::from_utf16_lossy(name.as_wide()));
            CoTaskMemFree(Some(name.0 as _));
            Some(path)
        }
    }

    pub fn save_as(owner: HWND, title: &str, name: &str, ext: &str, kinds: &[(&str, &str)]) -> Option<PathBuf> {
        unsafe {
            let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            // les textes doivent vivre aussi longtemps que la liste qui les désigne
            let texts: Vec<(Vec<u16>, Vec<u16>)> = kinds.iter().map(|(label, pattern)| (w(label), w(pattern))).collect();
            let specs: Vec<COMDLG_FILTERSPEC> =
                texts.iter().map(|(l, p)| COMDLG_FILTERSPEC { pszName: PCWSTR(l.as_ptr()), pszSpec: PCWSTR(p.as_ptr()) }).collect();
            dialog.SetFileTypes(&specs).ok()?;
            dialog.SetTitle(PCWSTR(w(title).as_ptr())).ok()?;
            dialog.SetFileName(PCWSTR(w(name).as_ptr())).ok()?;
            dialog.SetDefaultExtension(PCWSTR(w(ext).as_ptr())).ok()?;
            // « Annuler » est une erreur pour Windows ; pour nous, simplement pas de fichier
            dialog.Show(Some(owner)).ok()?;
            path_of(&dialog.GetResult().ok()?)
        }
    }

    /// La boîte « Ouvrir » : un dossier (`folders`), ou un ou plusieurs fichiers.
    pub fn open(owner: HWND, title: &str, folders: bool) -> Vec<PathBuf> {
        unsafe {
            let pick = || -> Option<Vec<PathBuf>> {
                let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
                let options = dialog.GetOptions().ok()?;
                dialog.SetOptions(options | FOS_FORCEFILESYSTEM | if folders { FOS_PICKFOLDERS } else { FOS_ALLOWMULTISELECT }).ok()?;
                dialog.SetTitle(PCWSTR(w(title).as_ptr())).ok()?;
                dialog.Show(Some(owner)).ok()?;
                let items = dialog.GetResults().ok()?;
                Some((0..items.GetCount().ok()?).filter_map(|i| path_of(&items.GetItemAt(i).ok()?)).collect())
            };
            pick().unwrap_or_default()
        }
    }
}

/// Demande où enregistrer un fichier. `name` : le nom proposé ; `ext` : l'extension ajoutée si l'utilisateur n'en
/// tape pas ; `kinds` : les types proposés (libellé, motif comme « *.csv »). None : annulé.
#[cfg(not(feature = "test-hook"))]
pub fn save_as(owner: HWND, title: &str, name: &str, ext: &str, kinds: &[(&str, &str)]) -> Option<PathBuf> {
    real::save_as(owner, title, name, ext, kinds)
}

/// Demande un dossier. None : annulé.
#[cfg(not(feature = "test-hook"))]
pub fn pick_folder(owner: HWND, title: &str) -> Option<PathBuf> {
    real::open(owner, title, true).into_iter().next()
}

/// Demande un ou plusieurs fichiers. Vide : annulé.
#[cfg(not(feature = "test-hook"))]
pub fn pick_files(owner: HWND, title: &str) -> Vec<PathBuf> {
    real::open(owner, title, false)
}

#[cfg(feature = "test-hook")]
pub fn save_as(_owner: HWND, _title: &str, _name: &str, _ext: &str, _kinds: &[(&str, &str)]) -> Option<PathBuf> {
    std::env::var_os("PLUME_TEST_SAVE").map(PathBuf::from)
}

#[cfg(feature = "test-hook")]
pub fn pick_folder(_owner: HWND, _title: &str) -> Option<PathBuf> {
    std::env::var_os("PLUME_TEST_FOLDER").map(PathBuf::from)
}

#[cfg(feature = "test-hook")]
pub fn pick_files(_owner: HWND, _title: &str) -> Vec<PathBuf> {
    std::env::var_os("PLUME_TEST_FILES").map(|v| std::env::split_paths(&v).collect()).unwrap_or_default()
}
