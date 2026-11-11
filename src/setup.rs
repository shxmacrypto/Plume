// Plume installé : raccourcis, entrée dans Paramètres › Applications, désinstallation, ménage après une mise à jour.
// PlumeSetup.exe copie les fichiers dans %LOCALAPPDATA%\Programs\Plume (sans droits administrateur),
// puis lance `plume.exe --register` pour tout le reste.
use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
use windows::{
    Win32::{
        Foundation::CloseHandle,
        Globalization::GetUserDefaultUILanguage,
        System::{
            Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile},
            Registry::{HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RegDeleteTreeW, RegSetKeyValueW},
            Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE},
        },
        UI::{
            Shell::{FOLDERID_Desktop, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellLink},
            WindowsAndMessaging::{IDYES, MB_DEFBUTTON2, MB_ICONINFORMATION, MB_ICONQUESTION, MB_ICONWARNING, MB_YESNO, MESSAGEBOX_STYLE, MessageBoxW},
        },
    },
    core::{HSTRING, Interface},
};

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Plume";
/// Présent tant qu'un Plume tourne (PlumeSetup.exe et la désinstallation le vérifient).
pub const MUTEX: &str = r"Local\PlumeBrowser";
const NO_WINDOW: u32 = 0x0800_0000;

fn local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// Programme : %LOCALAPPDATA%\Programs\Plume (dossier standard des applications par utilisateur).
pub fn install_dir() -> PathBuf {
    local_appdata().join("Programs").join("Plume")
}

/// Données de l'utilisateur (favoris, historique, coffre, extensions) : elles survivent aux mises à jour.
pub fn data_dir() -> PathBuf {
    local_appdata().join("Plume")
}

pub fn exe() -> PathBuf {
    install_dir().join("plume.exe")
}

/// Plume Suite est installée à côté : c'est elle qui tient l'entrée de Paramètres › Applications, et le dossier
/// contient d'autres applications. Plume Web ne s'occupe alors que de ses propres fichiers.
pub fn in_suite() -> bool {
    install_dir().join("plume-suite.exe").is_file()
}

fn start_menu() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(local_appdata).join(r"Microsoft\Windows\Start Menu\Programs")
}

fn desktop() -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.map(PathBuf::from)
    }
}

/// Plume tourne depuis son dossier d'installation (et non depuis un build de développement).
pub fn installed() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let key = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
        std::env::current_exe().ok().and_then(|e| e.parent().map(key)).is_some_and(|d| d == key(&install_dir()))
    })
}

pub fn mark_running() {
    // la poignée reste ouverte jusqu'à la fermeture de Plume
    unsafe {
        let _ = CreateMutexW(None, false, &HSTRING::from(MUTEX));
    }
}

pub fn running() -> bool {
    unsafe {
        match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &HSTRING::from(MUTEX)) {
            Ok(h) => {
                let _ = CloseHandle(h);
                true
            }
            Err(_) => false,
        }
    }
}

fn reg_sz(name: &str, value: &str) -> bool {
    let w: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL_KEY), &HSTRING::from(name), REG_SZ.0, Some(w.as_ptr() as _), (w.len() * 2) as u32).is_ok()
    }
}

fn reg_dword(name: &str, value: u32) -> bool {
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL_KEY), &HSTRING::from(name), REG_DWORD.0, Some(&value as *const u32 as _), 4).is_ok() }
}

fn shortcut(lnk: &Path, exe: &Path) -> windows::core::Result<()> {
    if let Some(d) = lnk.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    unsafe {
        let sl: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        sl.SetPath(&HSTRING::from(exe))?;
        if let Some(d) = exe.parent() {
            sl.SetWorkingDirectory(&HSTRING::from(d))?;
        }
        sl.SetIconLocation(&HSTRING::from(exe), 0)?;
        sl.SetDescription(&HSTRING::from("Plume"))?;
        sl.cast::<IPersistFile>()?.Save(&HSTRING::from(lnk), true)
    }
}

fn dir_kb(dir: &Path) -> u32 {
    std::fs::read_dir(dir).map(|r| r.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum::<u64>() / 1024).unwrap_or(0) as u32
}

/// `plume.exe --register [--desktop]` : raccourci du menu Démarrer (+ bureau à la première installation)
/// et entrée « Plume » dans Paramètres › Applications, avec sa version et sa désinstallation.
pub fn register(with_desktop: bool) -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let (dir, exe) = (install_dir(), exe());
    let mut ok = shortcut(&start_menu().join("Plume.lnk"), &exe).is_ok();
    if with_desktop && let Some(d) = desktop() {
        ok &= shortcut(&d.join("Plume.lnk"), &exe).is_ok();
    }
    let q = format!("\"{}\"", exe.display());
    let owner = crate::REPO.split('/').next().unwrap_or("Plume");
    ok &= in_suite()
        || reg_sz("DisplayName", "Plume")
            && reg_sz("DisplayVersion", crate::VERSION)
            && reg_sz("Publisher", owner)
            && reg_sz("DisplayIcon", &format!("{},0", exe.display()))
            && reg_sz("InstallLocation", &dir.display().to_string())
            && reg_sz("UninstallString", &format!("{q} --uninstall"))
            && reg_sz("QuietUninstallString", &format!("{q} --uninstall --quiet"))
            && reg_sz("URLInfoAbout", &format!("https://github.com/{}", crate::REPO))
            && reg_sz("URLUpdateInfo", &format!("https://github.com/{}/releases", crate::REPO))
            && reg_sz("HelpLink", &format!("https://github.com/{}/issues", crate::REPO))
            && reg_dword("EstimatedSize", dir_kb(&dir))
            && reg_dword("NoModify", 1)
            && reg_dword("NoRepair", 1);
    // proposé comme navigateur dans Applications par défaut (l'utilisateur choisit lui-même)
    crate::defbrowser::register(&exe);
    ok
}

/// Langue des boîtes de dialogue : celle choisie dans Plume, sinon celle de Windows.
fn french() -> bool {
    let cfg = std::fs::read_to_string(data_dir().join("plume.json")).unwrap_or_default();
    if cfg.contains(r#""lang":"fr""#) {
        return true;
    }
    if cfg.contains(r#""lang":"en""#) {
        return false;
    }
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x0c }
}

fn dialog(text: &str, style: MESSAGEBOX_STYLE) -> bool {
    unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from("Plume"), style) == IDYES }
}

/// `plume.exe --uninstall [--quiet]` (lancé depuis Paramètres › Applications).
pub fn uninstall(quiet: bool) -> i32 {
    let fr = french();
    let tr = |en: &'static str, f: &'static str| if fr { f } else { en };
    if !quiet
        && !dialog(
            tr(
                "Uninstall Plume?\n\nYour favorites, history and passwords are kept unless you choose to delete them in the next step.",
                "Désinstaller Plume ?\n\nVos favoris, votre historique et vos mots de passe sont conservés, sauf si vous choisissez de les supprimer à l'étape suivante.",
            ),
            MB_YESNO | MB_ICONQUESTION,
        )
    {
        return 1;
    }
    if running() {
        if !quiet {
            dialog(tr("Close Plume, then uninstall it again.", "Fermez Plume, puis relancez la désinstallation."), MB_ICONWARNING);
        }
        return 2;
    }
    let wipe = !quiet
        && dialog(
            tr(
                "Also delete your Plume data (favorites, history, passwords, extensions)?\n\nThis can't be undone.",
                "Supprimer aussi vos données Plume (favoris, historique, mots de passe, extensions) ?\n\nC'est définitif.",
            ),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
        );

    let _ = std::fs::remove_file(start_menu().join("Plume.lnk"));
    if let Some(d) = desktop() {
        let _ = std::fs::remove_file(d.join("Plume.lnk"));
    }
    let suite = in_suite();
    if !suite {
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL_KEY));
        }
    }
    crate::defbrowser::unregister();
    let data = data_dir();
    if wipe {
        let _ = std::fs::remove_dir_all(&data);
    } else {
        // composants du programme rangés à côté des données : outils vidéo, anciennes copies, installeurs
        for d in ["tools", "app", "updates"] {
            let _ = std::fs::remove_dir_all(data.join(d));
        }
    }
    // Un exe en cours d'exécution ne peut pas s'effacer : cmd le fera une fois Plume fermé. Avec Plume Suite, seuls
    // les fichiers du navigateur partent ; le dossier garde les autres applications.
    let dir = install_dir();
    let clean = if suite {
        format!("del /f /q \"{}\" \"{}\"", dir.join("plume.exe").display(), dir.join("WebView2Loader.dll").display())
    } else {
        format!("rmdir /s /q \"{}\"", dir.display())
    };
    let _ = Command::new("cmd").raw_arg(format!("/c ping -n 3 127.0.0.1 >nul & {clean}")).current_dir(std::env::temp_dir()).creation_flags(NO_WINDOW).spawn();
    if !quiet {
        dialog(tr("Plume has been uninstalled.", "Plume a été désinstallé."), MB_ICONINFORMATION);
    }
    0
}

/// Au démarrage de la version installée : efface les fichiers mis de côté par la mise à jour précédente,
/// les copies décompressées par l'ancien Plume.exe autonome (≤ 0.5) et les installeurs téléchargés.
pub fn cleanup() {
    // après un redémarrage, l'ancienne version met un instant à se fermer
    std::thread::sleep(std::time::Duration::from_secs(5));
    if let Ok(r) = std::fs::read_dir(install_dir()) {
        for e in r.flatten() {
            let p = e.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
            if ext.starts_with("old") || ext == "new" {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    for d in ["app", "updates"] {
        let _ = std::fs::remove_dir_all(data_dir().join(d));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn folders() {
        assert!(super::install_dir().ends_with(r"Programs\Plume"));
        assert!(super::desktop().is_some_and(|d| d.is_dir()));
        assert!(!super::installed()); // les tests tournent depuis target\
    }
}
