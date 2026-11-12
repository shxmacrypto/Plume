//! Plume Boost keeps a copy of its snapshots with Plume Backup: a plan "Plume Boost settings" (the Snapshots folder,
//! dated versions, run only when asked), made or updated with `plume-backup.exe --ensure-plan`, then run with
//! `--run-plan` after each snapshot. Plume Backup is found next to plume-boost.exe (the Suite installs its apps in one
//! folder); when it is missing, `PlumeSetup.exe --install backup` from the same folder installs it.
//!
//! Tests: PLUME_BOOST_BACKUP_EXE (where Plume Backup is), PLUME_BOOST_SETUP_EXE (the installer), PLUME_BOOST_BACKUP_DEST
//! (the destination of a new plan). A test build always gives Plume Backup its own data folder (PLUME_BACKUP_DIR,
//! inside the imaginary PC's folder) so the PC's real Plume Backup plans are never touched.

use std::{
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const PLAN: &str = "Plume Boost settings";
const NO_WINDOW: u32 = 0x0800_0000;

fn install_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default()
}

/// plume-backup.exe, if it is there.
pub fn exe() -> Option<PathBuf> {
    let p = match std::env::var_os("PLUME_BOOST_BACKUP_EXE") {
        Some(p) => PathBuf::from(p),
        None => install_dir().join("plume-backup.exe"),
    };
    p.is_file().then_some(p)
}

pub fn setup_exe() -> PathBuf {
    match std::env::var_os("PLUME_BOOST_SETUP_EXE") {
        Some(p) => PathBuf::from(p),
        None => install_dir().join("PlumeSetup.exe"),
    }
}

/// Where a new plan saves: Documents\Plume Backups (Plume Backup lets the user move it).
pub fn default_dest() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_BOOST_BACKUP_DEST") {
        return PathBuf::from(d);
    }
    if cfg!(feature = "test-hook") || cfg!(test) {
        return crate::settings::data_dir().join("Plume Backups");
    }
    documents().unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join("Documents")).join("Plume Backups")
}

fn documents() -> Option<PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

fn command(exe: &PathBuf) -> Command {
    let mut c = Command::new(exe);
    c.creation_flags(NO_WINDOW).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // a test build never reaches the user's real Plume Backup data
    if (cfg!(feature = "test-hook") || cfg!(test)) && std::env::var_os("PLUME_BACKUP_DIR").is_none() {
        c.env("PLUME_BACKUP_DIR", crate::settings::data_dir().join("backup-data"));
    }
    c
}

/// Runs Plume Backup and waits at most `limit`. Ok(message) when it exits with 0.
fn run(exe: &PathBuf, args: &[&std::ffi::OsStr], limit: Duration) -> Result<String, Outcome> {
    let mut child = command(exe).args(args).spawn().map_err(|e| Outcome::Failed(e.to_string()))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(100)),
            // still copying: it carries on by itself
            Ok(None) => return Err(Outcome::Late),
            Err(e) => return Err(Outcome::Failed(e.to_string())),
        }
    }
    let out = child.wait_with_output().map_err(|e| Outcome::Failed(e.to_string()))?;
    let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_string();
    if out.status.success() {
        Ok(text(&out.stdout))
    } else {
        let e = text(&out.stderr);
        Err(Outcome::Failed(if e.is_empty() { format!("exit code {}", out.status.code().unwrap_or(-1)) } else { e }))
    }
}

/// How keeping the copy went.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Copied: Plume Backup's summary.
    Copied(String),
    /// Plume Backup isn't installed.
    Missing,
    /// Still copying after the time Plume Boost waits (it finishes by itself).
    Late,
    Failed(String),
}

/// Makes sure the plan exists, then backs up the snapshots now (waits up to `limit`).
pub fn sync(limit: Duration) -> Outcome {
    let Some(exe) = exe() else { return Outcome::Missing };
    let src = crate::snapshot::root();
    let _ = std::fs::create_dir_all(&src);
    let dest = default_dest();
    let ensure =
        ["--ensure-plan".as_ref(), PLAN.as_ref(), "--source".as_ref(), src.as_os_str(), "--dest".as_ref(), dest.as_os_str(), "--keep".as_ref(), "60".as_ref()];
    if let Err(o) = run(&exe, &ensure, Duration::from_secs(30)) {
        return o;
    }
    match run(&exe, &["--run-plan".as_ref(), PLAN.as_ref()], limit) {
        Ok(m) => Outcome::Copied(m),
        Err(o) => o,
    }
}

/// `PlumeSetup.exe --install backup`, waited for. Ok when Plume Backup is there afterwards.
pub fn install() -> Result<(), String> {
    let setup = setup_exe();
    if !setup.is_file() {
        return Err(crate::i18n::tf(
            "The Plume Suite installer (PlumeSetup.exe) is not next to Plume Boost. Install Plume Backup from Plume Suite.",
            "L'installateur de Plume Suite (PlumeSetup.exe) n'est pas à côté de Plume Boost. Installez Plume Backup depuis Plume Suite.",
        )
        .into());
    }
    let st = Command::new(&setup).args(["--install", "backup"]).current_dir(install_dir()).creation_flags(NO_WINDOW).status().map_err(|e| e.to_string())?;
    if exe().is_some() {
        Ok(())
    } else {
        Err(format!("{} {}", crate::i18n::tf("The installer ended with code", "L'installateur s'est terminé avec le code"), st.code().unwrap_or(-1)))
    }
}
