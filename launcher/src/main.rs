// Plume.exe : un seul fichier. Contient le navigateur + WebView2Loader.dll,
// les décompresse dans %LOCALAPPDATA%\Plume\app\<version>\ au premier lancement, puis démarre le navigateur.
// Ce lanceur se ferme aussitôt : il ne consomme rien pendant la navigation.
#![windows_subsystem = "windows"]

use std::{env, fs, path::PathBuf, process::Command};

static CORE: &[u8] = include_bytes!(env!("PLUME_CORE_EXE"));
static DLL: &[u8] = include_bytes!(env!("PLUME_LOADER_DLL"));
const ID: &str = env!("PLUME_BUILD_ID");

fn same_size(p: &PathBuf, n: usize) -> bool {
    fs::metadata(p).is_ok_and(|m| m.len() == n as u64)
}

fn main() {
    let base = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Plume")
        .join("app");
    let dir = base.join(ID);
    let exe = dir.join("plume.exe");
    let dll = dir.join("WebView2Loader.dll");

    if !(same_size(&exe, CORE.len()) && same_size(&dll, DLL.len())) {
        let _ = fs::create_dir_all(&dir);
        let tmp = dir.join("plume.exe.tmp");
        let written = fs::write(&dll, DLL).is_ok() && fs::write(&tmp, CORE).is_ok() && fs::rename(&tmp, &exe).is_ok();
        if !written {
            return;
        }
        // Supprime les anciennes versions (ignorées si encore utilisées)
        if let Ok(entries) = fs::read_dir(&base) {
            for e in entries.flatten() {
                if e.file_name() != ID {
                    let _ = fs::remove_dir_all(e.path());
                }
            }
        }
    }

    let _ = Command::new(&exe).args(env::args_os().skip(1)).current_dir(&dir).spawn();
}
