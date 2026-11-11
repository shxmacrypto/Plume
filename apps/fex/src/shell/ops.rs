//! Opérations sur les fichiers. Tout ce qui déplace, copie ou supprime passe par le shell de Windows
//! (mêmes boîtes de dialogue, même corbeille, même « Annuler » que l'Explorateur).

use super::wide;
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::{HANDLE, HWND},
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance, IBindCtx},
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        },
        UI::Shell::{
            FOF_ALLOWUNDO, FOFX_SHOWELEVATIONPROMPT, FileOperation, IFileOperation, IFileOperationProgressSink, IShellItem, SHCreateItemFromParsingName,
        },
    },
    core::PCWSTR,
};

const CF_UNICODETEXT: u32 = 13;

/// Caractères interdits dans un nom de fichier sous Windows.
pub const FORBIDDEN: &str = "\\ / : * ? \" < > |";

/// Le nom est-il acceptable pour un fichier ou un dossier ?
pub fn valid_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n != "."
        && n != ".."
        && !n.chars().any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || (c as u32) < 32)
        && !n.ends_with(['.', ' '])
}

/// Renomme un fichier ou un dossier par le shell : annulable, avec les questions de Windows (nom déjà pris,
/// droits d'administrateur). Faux si l'opération a échoué ou a été abandonnée. Peut ouvrir des boîtes de
/// dialogue : à appeler depuis `Cx::after`.
pub fn rename(owner: HWND, path: &Path, new_name: &str) -> bool {
    unsafe {
        let run = || -> windows::core::Result<bool> {
            let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
            op.SetOperationFlags(FOF_ALLOWUNDO | FOFX_SHOWELEVATIONPROMPT)?;
            op.SetOwnerWindow(owner)?;
            let from = wide(path);
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(from.as_ptr()), None::<&IBindCtx>)?;
            let to: Vec<u16> = new_name.encode_utf16().chain(Some(0)).collect();
            op.RenameItem(&item, PCWSTR(to.as_ptr()), None::<&IFileOperationProgressSink>)?;
            op.PerformOperations()?;
            Ok(!op.GetAnyOperationsAborted()?.as_bool())
        };
        run().unwrap_or(false)
    }
}

/// Premier nom libre dans `dir` : « Nouveau dossier », « Nouveau dossier (2) »…
pub fn free_name(dir: &Path, base: &str) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{base} ({n})");
        n += 1;
    }
    name
}

/// Crée un dossier au premier nom libre et renvoie son nom.
pub fn new_folder(dir: &Path, base: &str) -> std::io::Result<String> {
    let name = free_name(dir, base);
    std::fs::create_dir(dir.join(&name))?;
    Ok(name)
}

/// Texte « Copier le chemin d'accès » : un chemin par ligne, entre guillemets comme dans l'Explorateur.
pub fn paths_text(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| format!("\"{}\"", p.display())).collect::<Vec<_>>().join("\r\n")
}

/// Les chemins d'une liste de fichiers déposés ou copiés (le format CF_HDROP de Windows).
pub fn dropped_files(list: windows::Win32::UI::Shell::HDROP) -> Vec<PathBuf> {
    use windows::Win32::UI::Shell::DragQueryFileW;
    unsafe {
        (0..DragQueryFileW(list, u32::MAX, None))
            .map(|i| {
                let mut name = vec![0u16; DragQueryFileW(list, i, None) as usize + 1];
                DragQueryFileW(list, i, Some(&mut name));
                PathBuf::from(super::from_wide(&name))
            })
            .collect()
    }
}

/// Les fichiers copiés dans le presse-papiers (vide : il contient autre chose, ou rien).
pub fn clipboard_files(owner: HWND) -> Vec<PathBuf> {
    use windows::Win32::{
        System::{DataExchange::GetClipboardData, Ole::CF_HDROP},
        UI::Shell::HDROP,
    };
    unsafe {
        if OpenClipboard(Some(owner)).is_err() {
            return vec![];
        }
        let files = GetClipboardData(CF_HDROP.0 as u32).map(|h| dropped_files(HDROP(h.0))).unwrap_or_default();
        let _ = CloseClipboard();
        files
    }
}

/// Les fichiers du presse-papiers ont-ils été coupés (et non copiés) ? Le format « Preferred DropEffect » le dit.
pub fn clipboard_cut(owner: HWND) -> bool {
    use windows::Win32::System::DataExchange::{GetClipboardData, RegisterClipboardFormatW};
    unsafe {
        let format = RegisterClipboardFormatW(windows::core::w!("Preferred DropEffect"));
        if format == 0 || OpenClipboard(Some(owner)).is_err() {
            return false;
        }
        let mut cut = false;
        if let Ok(h) = GetClipboardData(format) {
            let mem = windows::Win32::Foundation::HGLOBAL(h.0);
            let p = GlobalLock(mem) as *const u32;
            if !p.is_null() {
                cut = *p & 2 != 0; // DROPEFFECT_MOVE
                let _ = GlobalUnlock(mem);
            }
        }
        let _ = CloseClipboard();
        cut
    }
}

/// Place un texte dans le presse-papiers.
pub fn copy_text(owner: HWND, text: &str) -> bool {
    let data: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        if OpenClipboard(Some(owner)).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let mut ok = false;
        if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2) {
            let p = GlobalLock(mem) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
                let _ = GlobalUnlock(mem);
                // le presse-papiers devient propriétaire de la mémoire
                ok = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(mem.0))).is_ok();
            }
        }
        let _ = CloseClipboard();
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_name("notes.txt"));
        assert!(valid_name("  Nouveau dossier "));
        assert!(valid_name(".gitignore"));
        // les espaces autour du nom sont retirés avant le test, comme le fait Windows
        assert!(valid_name("space "));
        for bad in ["", "  ", ".", "..", "a/b", "a\\b", "c:d", "what?", "x*", "\"q\"", "a<b", "a|b", "tab\there", "dot."] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn free_names_and_new_folder() {
        let dir = std::env::temp_dir().join(format!("fex-ops-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_name(&dir, "New folder"), "New folder");
        assert_eq!(new_folder(&dir, "New folder").unwrap(), "New folder");
        assert_eq!(new_folder(&dir, "New folder").unwrap(), "New folder (2)");
        assert_eq!(new_folder(&dir, "New folder").unwrap(), "New folder (3)");
        assert!(dir.join("New folder (3)").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_as_path_text() {
        let t = paths_text(&[PathBuf::from(r"C:\a b\c.txt"), PathBuf::from(r"D:\x")]);
        assert_eq!(t, "\"C:\\a b\\c.txt\"\r\n\"D:\\x\"");
    }
}
