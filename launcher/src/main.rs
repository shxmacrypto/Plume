// PlumeSetup.exe : installe Plume pour l'utilisateur courant (sans droits administrateur) dans
// %LOCALAPPDATA%\Programs\Plume, puis le démarre. Plume s'en sert aussi pour ses mises à jour (--update).
//
//   PlumeSetup.exe                 installe (ou met à jour), crée les raccourcis, ouvre Plume
//   PlumeSetup.exe --update        utilisé par Plume : remplace les fichiers, sans fenêtre ni raccourci bureau
//   PlumeSetup.exe --quiet         installation silencieuse (sans ouvrir Plume) ; --no-desktop : pas de raccourci bureau
#![windows_subsystem = "windows"]

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

static CORE: &[u8] = include_bytes!(env!("PLUME_CORE_EXE"));
static DLL: &[u8] = include_bytes!(env!("PLUME_LOADER_DLL"));
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[link(name = "user32", kind = "raw-dylib")]
unsafe extern "system" {
    fn MessageBoxW(hwnd: *mut u8, text: *const u16, caption: *const u16, kind: u32) -> i32;
}
#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" {
    fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> *mut u8;
    fn CloseHandle(h: *mut u8) -> i32;
    fn GetUserDefaultUILanguage() -> u16;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn alert(text: &str, icon: u32) {
    unsafe {
        MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("Plume").as_ptr(), icon);
    }
}

/// Un Plume est ouvert (il garde ce mutex nommé tant qu'il tourne).
fn plume_running() -> bool {
    unsafe {
        let h = OpenMutexW(0x0010_0000, 0, wide(r"Local\PlumeBrowser").as_ptr());
        !h.is_null() && CloseHandle(h) != 0
    }
}

fn local_appdata() -> PathBuf {
    env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir)
}

/// Langue choisie dans Plume, sinon celle de Windows.
fn french() -> bool {
    let cfg = fs::read_to_string(local_appdata().join(r"Plume\plume.json")).unwrap_or_default();
    if cfg.contains(r#""lang":"fr""#) || cfg.contains(r#""lang":"en""#) {
        return cfg.contains(r#""lang":"fr""#);
    }
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x0c }
}

/// Écrit un fichier même s'il est en cours d'utilisation : un exe ou une DLL chargés ne peuvent pas être
/// remplacés, mais peuvent être renommés. L'ancien est mis de côté (*.old), Plume l'efface au démarrage suivant.
fn put(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if fs::metadata(path).is_ok_and(|m| m.len() == bytes.len() as u64) && fs::read(path).is_ok_and(|b| b == bytes) {
        return Ok(());
    }
    let new = path.with_extension("new");
    fs::write(&new, bytes)?;
    if path.exists() {
        let moved = (0..10).any(|n| {
            let old = path.with_extension(if n == 0 { "old".into() } else { format!("old{n}") });
            let _ = fs::remove_file(&old);
            if fs::rename(path, &old).is_ok() {
                let _ = fs::remove_file(&old); // réussit si ce fichier n'était pas utilisé
                true
            } else {
                false
            }
        });
        if !moved {
            let _ = fs::remove_file(&new);
            return Err(io::Error::other("file in use"));
        }
    }
    fs::rename(&new, path)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let update = has("--update");
    let quiet = update || has("--quiet") || has("/s");
    let fr = french();
    let was_running = plume_running();

    let dir = local_appdata().join(r"Programs\Plume");
    let exe = dir.join("plume.exe");
    let written = fs::create_dir_all(&dir).and_then(|_| put(&dir.join("WebView2Loader.dll"), DLL)).and_then(|_| put(&exe, CORE));
    if let Err(e) = written {
        if !quiet {
            let msg = if fr { format!("Impossible d'installer Plume :\n{e}") } else { format!("Plume could not be installed:\n{e}") };
            alert(&msg, 0x10);
        }
        std::process::exit(2);
    }

    // Raccourcis + Paramètres › Applications : c'est la nouvelle version de Plume qui s'en charge
    let mut reg = Command::new(&exe);
    reg.arg("--register").current_dir(&dir);
    if !update && !has("--no-desktop") {
        reg.arg("--desktop");
    }
    let registered = reg.status().is_ok_and(|s| s.success());
    if update {
        std::process::exit(if registered { 0 } else { 3 });
    }
    if was_running {
        // l'ancienne version tourne encore : la nouvelle servira au prochain lancement
        if !quiet {
            let msg = if fr {
                format!("Plume {VERSION} est installé.\n\nFermez Plume puis rouvrez-le pour utiliser la nouvelle version.")
            } else {
                format!("Plume {VERSION} is installed.\n\nClose Plume and open it again to use the new version.")
            };
            alert(&msg, 0x40);
        }
    } else if !quiet {
        let _ = Command::new(&exe).current_dir(&dir).spawn();
    }
}
