//! « Afficher dans le dossier » pour toutes les applications Plume : quand Plume FEx ouvre les dossiers (son option
//! « Ouvrir les dossiers et Win+E avec Plume FEx »), c'est lui qui montre le fichier, sélectionné dans son dossier ;
//! sinon l'Explorateur de fichiers de Windows.

use std::path::{Path, PathBuf};
use windows::{
    Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW},
    core::HSTRING,
};

/// Montre `path` : un dossier s'ouvre, un fichier apparaît sélectionné dans son dossier.
pub fn show(path: &Path) {
    use std::os::windows::process::CommandExt;
    if let Some(fex) = fex()
        && std::process::Command::new(fex).arg(path).spawn().is_ok()
    {
        return;
    }
    let arg = if path.is_dir() { format!("\"{}\"", path.display()) } else { format!("/select,\"{}\"", path.display()) };
    // explorer.exe veut ses arguments tels quels (les guillemets après « /select, »)
    let _ = std::process::Command::new("explorer.exe").raw_arg(arg).spawn();
}

/// Le programme de Plume FEx quand c'est lui qui ouvre les dossiers. PLUME_FEX_CLASSES_ROOT désigne la clé d'essai
/// des tests, comme dans Plume FEx.
pub fn fex() -> Option<PathBuf> {
    let base = std::env::var("PLUME_FEX_CLASSES_ROOT").ok().filter(|b| !b.trim().is_empty()).unwrap_or_else(|| r"Software\Classes".into());
    fex_exe(get(&format!(r"{base}\Directory\shell")).as_deref(), get(&format!(r"{base}\Directory\shell\PlumeFEx\command")).as_deref())
        .filter(|exe| exe.is_file())
}

/// Le programme de la commande `"C:\…\plume-fex.exe" "%1"`, si le verbe par défaut des dossiers est celui de Plume FEx.
fn fex_exe(default_verb: Option<&str>, command: Option<&str>) -> Option<PathBuf> {
    if !default_verb?.eq_ignore_ascii_case("PlumeFEx") {
        return None;
    }
    let exe = command?.trim().strip_prefix('"')?.split('"').next()?;
    exe.to_ascii_lowercase().ends_with(".exe").then(|| exe.into())
}

/// La valeur par défaut d'une clé de HKEY_CURRENT_USER.
fn get(key: &str) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let ok = unsafe { RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(key), None, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as _), Some(&mut len)).is_ok() };
    ok.then(|| String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())]))
}

#[cfg(test)]
mod tests {
    use super::fex_exe;

    #[test]
    fn only_when_fex_opens_folders() {
        let cmd = Some(r#""C:\Users\a\AppData\Local\Programs\Plume\plume-fex.exe" "%1""#);
        assert_eq!(fex_exe(Some("PlumeFEx"), cmd), Some(r"C:\Users\a\AppData\Local\Programs\Plume\plume-fex.exe".into()));
        assert!(fex_exe(Some("plumefex"), cmd).is_some());
        assert_eq!(fex_exe(Some("OtherManager"), cmd), None);
        assert_eq!(fex_exe(None, cmd), None);
        assert_eq!(fex_exe(Some("PlumeFEx"), None), None);
        assert_eq!(fex_exe(Some("PlumeFEx"), Some("plume-fex.exe %1")), None);
    }
}
