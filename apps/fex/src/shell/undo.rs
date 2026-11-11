//! Ce qu'il faut pour annuler (Ctrl+Z) : envoyer à la Corbeille, déplacer, et sortir un élément de la Corbeille.
//! Envoyer et déplacer passent par le shell (mêmes boîtes de dialogue que l'Explorateur). Pour la Corbeille, Windows
//! garde pour chaque élément supprimé un fichier « $I… » qui dit d'où il vient, à côté de l'élément lui-même « $R… » :
//! le remettre en place, c'est renommer « $R… » vers son ancien chemin et effacer « $I… », ce que fait « Restaurer ».

use super::wide;
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{CLSCTX_ALL, CoCreateInstance, IBindCtx},
        UI::Shell::{FOF_ALLOWUNDO, FOFX_SHOWELEVATIONPROMPT, FileOperation, IFileOperation, IFileOperationProgressSink, IShellItem, SHCreateItemFromParsingName},
    },
    core::PCWSTR,
};

fn item(p: &Path) -> windows::core::Result<IShellItem> {
    let w = wide(p);
    unsafe { SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None::<&IBindCtx>) }
}

/// Une opération du shell : `fill` y ajoute ses éléments. Faux si elle a échoué ou a été abandonnée.
fn run(owner: HWND, fill: impl FnOnce(&IFileOperation) -> windows::core::Result<()>) -> bool {
    unsafe {
        let go = || -> windows::core::Result<bool> {
            let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
            op.SetOperationFlags(FOF_ALLOWUNDO | FOFX_SHOWELEVATIONPROMPT)?;
            op.SetOwnerWindow(owner)?;
            fill(&op)?;
            op.PerformOperations()?;
            Ok(!op.GetAnyOperationsAborted()?.as_bool())
        };
        go().unwrap_or(false)
    }
}

/// Envoie des éléments à la Corbeille.
pub fn recycle(owner: HWND, paths: &[PathBuf]) -> bool {
    run(owner, |op| unsafe {
        for p in paths {
            op.DeleteItem(&item(p)?, None::<&IFileOperationProgressSink>)?;
        }
        Ok(())
    })
}

/// Déplace des éléments : (élément, dossier d'arrivée, nom à l'arrivée).
pub fn move_items(owner: HWND, moves: &[(PathBuf, PathBuf, String)]) -> bool {
    run(owner, |op| unsafe {
        for (from, dir, name) in moves {
            let to: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            op.MoveItem(&item(from)?, &item(dir)?, PCWSTR(to.as_ptr()), None::<&IFileOperationProgressSink>)?;
        }
        Ok(())
    })
}

/// Le chemin d'origine noté dans un fichier « $I… » de la Corbeille, et l'heure de la suppression.
pub fn read_info(bytes: &[u8]) -> Option<(PathBuf, u64)> {
    let num = |at: usize| bytes.get(at..at + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()));
    let (version, when) = (num(0)?, num(16)?);
    let units: Vec<u16> = match version {
        // Windows 10 et 11 : longueur (en caractères, zéro final compris), puis le chemin
        2 => {
            let n = u32::from_le_bytes(bytes.get(24..28)?.try_into().ok()?) as usize;
            bytes.get(28..28 + n * 2)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect()
        }
        // Vista à 8.1 : 260 caractères
        1 => bytes.get(24..24 + 520)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect(),
        _ => return None,
    };
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    Some((PathBuf::from(String::from_utf16_lossy(&units[..end])), when))
}

/// Les dossiers de Corbeille lisibles d'un disque : celui de l'utilisateur (« X:\$Recycle.Bin\<SID> »).
fn bins(drive: &Path) -> Vec<PathBuf> {
    // PLUME_FEX_RECYCLE_ROOT (essais) : une Corbeille factice
    let root = std::env::var_os("PLUME_FEX_RECYCLE_ROOT").map(PathBuf::from).unwrap_or_else(|| drive.join("$Recycle.Bin"));
    std::fs::read_dir(&root).map(|r| r.flatten().map(|e| e.path()).filter(|p| std::fs::read_dir(p).is_ok()).collect()).unwrap_or_default()
}

/// Remet en place des éléments envoyés à la Corbeille (le plus récent, pour chaque chemin). Rend ceux remis en place.
pub fn restore(originals: &[PathBuf]) -> Vec<PathBuf> {
    let same = |a: &Path, b: &Path| a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy());
    let mut done = vec![];
    for original in originals {
        if original.exists() {
            continue;
        }
        let Some(drive) = original.ancestors().last().map(Path::to_path_buf) else { continue };
        // la suppression la plus récente de ce chemin
        let mut best: Option<(u64, PathBuf, PathBuf)> = None;
        for bin in bins(&drive) {
            let Ok(list) = std::fs::read_dir(&bin) else { continue };
            for e in list.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let Some(rest) = name.strip_prefix("$I") else { continue };
                let Some((from, when)) = std::fs::read(e.path()).ok().and_then(|b| read_info(&b)) else { continue };
                if same(&from, original) && best.as_ref().is_none_or(|b| when > b.0) {
                    best = Some((when, e.path(), bin.join(format!("$R{rest}"))));
                }
            }
        }
        if let Some((_, info, data)) = best
            && data.exists()
            && original.parent().is_some_and(Path::is_dir)
            && std::fs::rename(&data, original).is_ok()
        {
            let _ = std::fs::remove_file(info);
            done.push(original.clone());
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info_v2(path: &str, when: u64) -> Vec<u8> {
        let units: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let mut b = vec![];
        b.extend(2u64.to_le_bytes());
        b.extend(123u64.to_le_bytes());
        b.extend(when.to_le_bytes());
        b.extend((units.len() as u32).to_le_bytes());
        units.iter().for_each(|u| b.extend(u.to_le_bytes()));
        b
    }

    #[test]
    fn reads_recycle_bin_records() {
        let (p, w) = read_info(&info_v2(r"C:\Users\Sam\Été\notes.txt", 77)).unwrap();
        assert_eq!((p, w), (PathBuf::from(r"C:\Users\Sam\Été\notes.txt"), 77));
        let mut v1 = vec![];
        v1.extend(1u64.to_le_bytes());
        v1.extend(5u64.to_le_bytes());
        v1.extend(9u64.to_le_bytes());
        let units: Vec<u16> = r"D:\old.txt".encode_utf16().collect();
        units.iter().for_each(|u| v1.extend(u.to_le_bytes()));
        v1.resize(24 + 520, 0);
        assert_eq!(read_info(&v1).unwrap().0, PathBuf::from(r"D:\old.txt"));
        assert!(read_info(&[1, 2, 3]).is_none());
        assert!(read_info(&info_v2("x", 1)[..20]).is_none());
    }

    #[test]
    fn restores_the_latest_copy_from_a_bin() {
        let root = std::env::temp_dir().join(format!("fex-undo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (bin, home) = (root.join("bin").join("S-1-5-21-test"), root.join("home"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let original = home.join("notes.txt");
        // deux suppressions du même fichier : la plus récente revient
        std::fs::write(bin.join("$IAAA.txt"), info_v2(&original.to_string_lossy(), 10)).unwrap();
        std::fs::write(bin.join("$RAAA.txt"), b"old").unwrap();
        std::fs::write(bin.join("$IBBB.txt"), info_v2(&original.to_string_lossy(), 20)).unwrap();
        std::fs::write(bin.join("$RBBB.txt"), b"new").unwrap();
        unsafe { std::env::set_var("PLUME_FEX_RECYCLE_ROOT", root.join("bin")) };
        assert_eq!(restore(std::slice::from_ref(&original)), std::slice::from_ref(&original));
        assert_eq!(std::fs::read(&original).unwrap(), b"new");
        assert!(!bin.join("$IBBB.txt").exists() && bin.join("$IAAA.txt").exists());
        // déjà là : rien n'est touché
        assert!(restore(std::slice::from_ref(&original)).is_empty());
        unsafe { std::env::remove_var("PLUME_FEX_RECYCLE_ROOT") };
        let _ = std::fs::remove_dir_all(&root);
    }
}
