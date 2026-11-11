//! Les applications de la suite, le dossier où elles sont installées, et ce que Windows en sait : raccourcis du menu
//! Démarrer et entrée « Plume Suite » dans Paramètres › Applications.
//!
//! Tout est installé pour l'utilisateur courant, sans droits administrateur, dans %LOCALAPPDATA%\Programs\Plume (le
//! dossier de Plume Web depuis toujours). Les fichiers eux-mêmes sont écrits par PlumeSetup.exe, dont une copie reste
//! dans ce dossier : Plume Suite s'en sert pour installer une application sans rien télécharger.

use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};
use windows::{
    Win32::{
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IPersistFile},
            Registry::{HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RegDeleteTreeW, RegSetKeyValueW},
        },
        UI::Shell::{FOLDERID_Desktop, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellLink},
    },
    core::{HSTRING, Interface},
};

/// Une application de la suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum App {
    Web,
    Fex,
    Mail,
}

pub const ALL: [App; 3] = [App::Web, App::Fex, App::Mail];

/// Plume Mail fait-il partie de cette version (`PLUME_WITH_MAIL=1` à la compilation, comme pour l'installeur) ?
pub fn mail_ready() -> bool {
    option_env!("PLUME_WITH_MAIL") == Some("1")
}

impl App {
    /// L'application peut être installée par cette version (sinon : « Bientôt »).
    pub fn available(self) -> bool {
        self != App::Mail || mail_ready()
    }
}

impl App {
    /// Nom court, celui de la ligne de commande de PlumeSetup.exe (`--install fex`).
    pub fn key(self) -> &'static str {
        match self {
            App::Web => "web",
            App::Fex => "fex",
            App::Mail => "mail",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            App::Web => "Plume Web",
            App::Fex => "Plume FEx",
            App::Mail => "Plume Mail",
        }
    }
    pub fn exe_name(self) -> &'static str {
        match self {
            App::Web => "plume.exe",
            App::Fex => "plume-fex.exe",
            App::Mail => "plume-mail.exe",
        }
    }
    pub fn exe(self) -> PathBuf {
        install_dir().join(self.exe_name())
    }
    pub fn installed(self) -> bool {
        self.exe().is_file()
    }
    /// Raccourci du menu Démarrer. Celui de Plume Web (« Plume.lnk ») est géré par le navigateur lui-même.
    fn shortcut(self) -> &'static str {
        match self {
            App::Web => "Plume.lnk",
            App::Fex => "Plume FEx.lnk",
            App::Mail => "Plume Mail.lnk",
        }
    }
}

fn local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

pub fn install_dir() -> PathBuf {
    local_appdata().join("Programs").join("Plume")
}

/// Données de la suite (téléchargements de mise à jour) : %LOCALAPPDATA%\Plume\Suite.
pub fn data_dir() -> PathBuf {
    local_appdata().join("Plume").join("Suite")
}

/// La copie de l'installeur gardée à côté des applications.
pub fn setup() -> PathBuf {
    install_dir().join("PlumeSetup.exe")
}

pub fn suite_exe() -> PathBuf {
    install_dir().join("plume-suite.exe")
}

/// Plume Suite tourne depuis son dossier d'installation (et non depuis un build de développement).
pub fn running_installed() -> bool {
    let key = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    std::env::current_exe().ok().and_then(|e| e.parent().map(key)).is_some_and(|d| d == key(&install_dir()))
}

const NO_WINDOW: u32 = 0x0800_0000;

/// Lance l'installeur gardé dans le dossier avec ces arguments et attend qu'il ait fini. Rend son code de sortie.
pub fn run_setup(args: &[&str]) -> Result<i32, String> {
    let setup = setup();
    if !setup.is_file() {
        return Err("missing-setup".into());
    }
    let st = Command::new(&setup).args(args).current_dir(install_dir()).creation_flags(NO_WINDOW).status().map_err(|e| e.to_string())?;
    Ok(st.code().unwrap_or(-1))
}

/// Ouvre une application installée.
pub fn open(app: App) -> bool {
    Command::new(app.exe()).current_dir(install_dir()).spawn().is_ok()
}

// ───────────────────────── Windows : raccourcis et Paramètres › Applications ─────────────────────────

/// Clé de l'entrée dans Paramètres › Applications. Celle de Plume Web depuis toujours : la suite la reprend, pour
/// qu'une seule entrée existe. PLUME_SUITE_TEST_KEY (essais) écrit dans une clé à part.
pub fn uninstall_key() -> String {
    let name = if std::env::var_os("PLUME_SUITE_TEST_KEY").is_some() { "PlumeSuiteTest" } else { "Plume" };
    format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{name}")
}

fn start_menu() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(local_appdata).join(r"Microsoft\Windows\Start Menu\Programs")
}

fn desktop() -> Option<PathBuf> {
    // PLUME_SUITE_DESKTOP (essais) : un autre dossier que le vrai bureau
    if let Some(d) = std::env::var_os("PLUME_SUITE_DESKTOP") {
        return Some(PathBuf::from(d));
    }
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.map(PathBuf::from)
    }
}

fn shortcut(lnk: &Path, exe: &Path, description: &str) -> windows::core::Result<()> {
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
        sl.SetDescription(&HSTRING::from(description))?;
        sl.cast::<IPersistFile>()?.Save(&HSTRING::from(lnk), true)
    }
}

fn reg_sz(key: &str, name: &str, value: &str) -> bool {
    let w: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name), REG_SZ.0, Some(w.as_ptr() as _), (w.len() * 2) as u32).is_ok() }
}

fn reg_dword(key: &str, name: &str, value: u32) -> bool {
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name), REG_DWORD.0, Some(&value as *const u32 as _), 4).is_ok() }
}

fn dir_kb(dir: &Path) -> u32 {
    std::fs::read_dir(dir).map(|r| r.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum::<u64>() / 1024).unwrap_or(0) as u32
}

/// `plume-suite.exe --register [--desktop]` (lancé par PlumeSetup.exe après chaque installation) : raccourcis de la
/// suite et des applications installées (ceux des applications retirées disparaissent), et l'entrée de la suite dans
/// Paramètres › Applications. Plume Web crée le sien, et ce qui fait de lui un navigateur, avec `plume.exe --register`.
pub fn register(with_desktop: bool) -> bool {
    let (dir, exe) = (install_dir(), suite_exe());
    let menu = start_menu();
    let mut ok = shortcut(&menu.join("Plume Suite.lnk"), &exe, "Plume Suite").is_ok();
    for app in [App::Fex, App::Mail] {
        let lnk = menu.join(app.shortcut());
        if app.installed() {
            ok &= shortcut(&lnk, &app.exe(), app.name()).is_ok();
        } else {
            let _ = std::fs::remove_file(lnk);
        }
    }
    if with_desktop && let Some(d) = desktop() {
        ok &= shortcut(&d.join("Plume Suite.lnk"), &exe, "Plume Suite").is_ok();
    }
    let key = uninstall_key();
    let setup = format!("\"{}\"", setup().display());
    let repo = plume_update::REPO;
    ok &= reg_sz(&key, "DisplayName", "Plume Suite")
        && reg_sz(&key, "DisplayVersion", plume_update::VERSION)
        && reg_sz(&key, "Publisher", repo.split('/').next().unwrap_or("Plume"))
        && reg_sz(&key, "DisplayIcon", &format!("{},0", exe.display()))
        && reg_sz(&key, "InstallLocation", &dir.display().to_string())
        && reg_sz(&key, "UninstallString", &format!("{setup} --uninstall-all"))
        && reg_sz(&key, "QuietUninstallString", &format!("{setup} --uninstall-all --quiet"))
        && reg_sz(&key, "URLInfoAbout", &format!("https://github.com/{repo}"))
        && reg_sz(&key, "URLUpdateInfo", &format!("https://github.com/{repo}/releases"))
        && reg_sz(&key, "HelpLink", &format!("https://github.com/{repo}/issues"))
        && reg_dword(&key, "EstimatedSize", dir_kb(&dir))
        && reg_dword(&key, "NoModify", 1)
        && reg_dword(&key, "NoRepair", 1);
    ok
}

/// `plume-suite.exe --unregister` (désinstallation de toute la suite) : raccourcis et entrée de Paramètres › Applications.
pub fn unregister() {
    let menu = start_menu();
    for lnk in ["Plume Suite.lnk", App::Fex.shortcut(), App::Mail.shortcut()] {
        let _ = std::fs::remove_file(menu.join(lnk));
    }
    if let Some(d) = desktop() {
        let _ = std::fs::remove_file(d.join("Plume Suite.lnk"));
    }
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(uninstall_key()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        assert!(install_dir().ends_with(r"Programs\Plume"));
        assert_eq!(App::Fex.exe().file_name().unwrap(), "plume-fex.exe");
        assert!(setup().ends_with(r"Programs\Plume\PlumeSetup.exe"));
        assert_eq!(ALL.iter().map(|a| a.key()).collect::<Vec<_>>(), ["web", "fex", "mail"]);
        assert!(!running_installed()); // les essais tournent depuis target\
        assert!(uninstall_key().ends_with(r"\Uninstall\Plume") || std::env::var_os("PLUME_SUITE_TEST_KEY").is_some());
    }
}
