//! The command line other Plume apps use to keep a copy of their own files with Plume Backup, without a window:
//!
//!   plume-backup.exe --ensure-plan "<name>" --source "<dir>" --dest "<dir>" [--keep <n>]
//!       Creates the plan (versions mode, "only when asked") or, when a plan of that name exists, makes sure it backs
//!       up that folder. An existing plan keeps the destination, schedule and number of versions chosen in Plume
//!       Backup: the user may have moved it. Running it twice changes nothing.
//!   plume-backup.exe --run-plan "<name>"
//!       Backs up that plan now and waits for the end. Exit code 0 when it worked, 1 with a message otherwise.
//!
//! When a Plume Backup window is running, the request is handed to it (it owns plans.json and saves it after every
//! change) and this process waits for its answer, written to `<data>\cli\<id>.json`. Otherwise the work is done here
//! and plans.json is written the usual way (a temporary file renamed over the old one), never half-written.
//! Every safety rule of a plan made in the window applies: the destination is never inside the source, nor the source
//! inside the backups.

use crate::{
    engine::{self, Fail, Mode, Plan, Report, Schedule},
    i18n::french,
    settings::{Last, Store, fail_code},
    sys,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ensure {
    pub name: String,
    pub source: PathBuf,
    pub dest: PathBuf,
    pub keep: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cli {
    Ensure(Ensure),
    Run(String),
}

/// What a request handed to the window answers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Answer {
    pub ok: bool,
    pub message: String,
}

fn tf(en: &'static str, fr: &'static str) -> &'static str {
    if french() { fr } else { en }
}

/// Reads `--ensure-plan` / `--run-plan` and their values. `Err`: those options, but incomplete (exit code 2).
pub fn parse(args: &[String]) -> Option<Result<Cli, String>> {
    let pos = |flag: &str| args.iter().position(|a| a.eq_ignore_ascii_case(flag));
    let value = |flag: &str| pos(flag).and_then(|i| args.get(i + 1)).filter(|v| !v.starts_with("--")).cloned();
    if pos("--ensure-plan").is_some() {
        let (Some(name), Some(source), Some(dest)) = (value("--ensure-plan"), value("--source"), value("--dest")) else {
            return Some(Err("usage: --ensure-plan \"<name>\" --source \"<dir>\" --dest \"<dir>\" [--keep <n>]".into()));
        };
        let keep = match value("--keep") {
            Some(k) => match k.parse::<u32>() {
                Ok(n) => Some(n),
                Err(_) => return Some(Err("--keep needs a number".into())),
            },
            None => None,
        };
        return Some(Ok(Cli::Ensure(Ensure { name, source: PathBuf::from(source), dest: PathBuf::from(dest), keep })));
    }
    if pos("--run-plan").is_some() {
        return Some(match value("--run-plan") {
            Some(n) => Ok(Cli::Run(n)),
            None => Err("usage: --run-plan \"<name>\"".into()),
        });
    }
    None
}

pub fn fail_text(f: &Fail) -> String {
    match f {
        Fail::Overlap => tf("The backup can't be inside the folder it backs up.", "La sauvegarde ne peut pas être dans le dossier qu'elle sauvegarde.").into(),
        Fail::SourceMissing(p) => format!("{} {}", tf("The folder to back up is missing:", "Le dossier à sauvegarder est absent :"), p.display()),
        Fail::DestMissing => tf("The destination is not available.", "La destination n'est pas disponible.").into(),
        Fail::DestFull => tf("The destination is full.", "La destination est pleine.").into(),
        Fail::NoSources => tf("No folder to back up.", "Aucun dossier à sauvegarder.").into(),
        Fail::NoDest => tf("No destination.", "Aucune destination.").into(),
        Fail::Stopped => tf("Stopped.", "Arrêtée.").into(),
        Fail::Io(e) => e.clone(),
    }
}

fn find<'a>(store: &'a Store, name: &str) -> Option<&'a Plan> {
    store.plans.iter().find(|p| p.name.trim().eq_ignore_ascii_case(name.trim()))
}

/// Creates or updates the plan in `store` (not saved). Returns its id and whether anything changed. The source must
/// exist; the destination folder is created when missing (never inside the source: that is checked first).
pub fn ensure(store: &mut Store, a: &Ensure, new_id: u64) -> Result<(u64, bool), String> {
    let name = a.name.trim();
    if name.is_empty() {
        return Err(tf("The plan needs a name.", "Le plan doit avoir un nom.").into());
    }
    if !a.source.is_absolute() || !a.dest.is_absolute() {
        return Err(tf("The folders must be full paths.", "Les dossiers doivent être des chemins complets.").into());
    }
    if !a.source.is_dir() {
        return Err(fail_text(&Fail::SourceMissing(a.source.clone())));
    }
    if let Some(i) = store.plans.iter().position(|p| p.name.trim().eq_ignore_ascii_case(name)) {
        let mut plan = store.plans[i].clone();
        plan.sources = vec![a.source.clone()];
        // the safety rules hold for the destination the plan really has (perhaps moved by the user)
        if let Err(f @ (Fail::Overlap | Fail::NoDest)) = engine::check(&plan, &plan.dest) {
            return Err(fail_text(&f));
        }
        let changed = plan != store.plans[i];
        store.plans[i] = plan;
        return Ok((store.plans[i].id, changed));
    }
    let mut plan = Plan {
        id: new_id,
        name: name.to_string(),
        sources: vec![a.source.clone()],
        dest: a.dest.clone(),
        schedule: Schedule::Manual,
        mode: Mode::Versions,
        ..Default::default()
    };
    if let Some(k) = a.keep {
        plan.keep_versions = k;
    }
    match engine::check(&plan, &plan.dest) {
        Ok(()) => {}
        Err(Fail::DestMissing) => {
            std::fs::create_dir_all(&plan.dest).map_err(|e| e.to_string())?;
        }
        Err(f) => return Err(fail_text(&f)),
    }
    let base = engine::safe_name(&plan.name);
    let mut folder = base.clone();
    let mut n = 2;
    while store.plans.iter().any(|p| p.folder.eq_ignore_ascii_case(&folder)) {
        folder = format!("{base} ({n})");
        n += 1;
    }
    plan.folder = folder;
    plan.dest_serial = sys::serial_of(&plan.dest);
    let id = plan.id;
    store.plans.push(plan);
    Ok((id, true))
}

/// The plan a `--run-plan` names, ready to run: its id and copy. Paused plans are refused (the user paused them).
pub fn runnable(store: &Store, name: &str) -> Result<Plan, String> {
    let Some(plan) = find(store, name) else {
        return Err(format!("{} \"{name}\"", tf("No plan named", "Aucun plan nommé")));
    };
    if plan.paused {
        return Err(tf("This plan is paused in Plume Backup.", "Ce plan est en pause dans Plume Backup.").into());
    }
    Ok(plan.clone())
}

/// Runs a plan once, here, and returns how it went (nothing saved).
pub fn run_once(plan: &Plan, now: u64) -> Result<Report, Fail> {
    let dest = sys::resolve(&plan.dest, plan.dest_serial).ok_or(Fail::DestMissing)?;
    let stop = AtomicBool::new(false);
    engine::run(plan, &dest, &sys::stamp(now), now, &stop, &mut |_| {})
}

/// Writes how a run went into the store (the window does the same in `on_done`).
pub fn record(store: &mut Store, id: u64, now: u64, result: &Result<Report, Fail>) {
    let st = store.states.entry(id).or_default();
    st.last_try = now;
    match result {
        Ok(r) => {
            st.last_ok = now;
            st.last = Some(Last { time: now, ok: true, report: r.clone(), fail: String::new() });
        }
        Err(f) => st.last = Some(Last { time: now, ok: false, report: Report::default(), fail: fail_code(f) }),
    }
}

pub fn report_text(r: &Report) -> String {
    if french() {
        format!("{} fichiers, {} copiés, {} inchangés, {} erreurs", r.files, r.copied, r.linked + r.unchanged, r.errors.len())
    } else {
        format!("{} files, {} copied, {} unchanged, {} errors", r.files, r.copied, r.linked + r.unchanged, r.errors.len())
    }
}

// ───────────────────────── the window's side ─────────────────────────

/// Where a request handed to the window gets its answer.
pub fn answer_file(nonce: &str) -> PathBuf {
    sys::data_dir().join("cli").join(format!("{}.json", engine::safe_name(nonce)))
}

pub fn write_answer(nonce: &str, a: &Answer) {
    let f = answer_file(nonce);
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = f.with_extension("tmp");
    if let Ok(b) = serde_json::to_vec(a)
        && std::fs::write(&tmp, b).is_ok()
    {
        let _ = std::fs::rename(&tmp, &f);
    }
}

/// The text sent to the window: "ensure-plan <nonce>\n<json>" or "run-plan <nonce>\n<name>".
pub fn message(cli: &Cli, nonce: &str) -> String {
    match cli {
        Cli::Ensure(e) => format!("ensure-plan {nonce}\n{}", serde_json::to_string(e).unwrap_or_default()),
        Cli::Run(name) => format!("run-plan {nonce}\n{name}"),
    }
}

/// Reads a message sent by [`message`].
pub fn read_message(text: &str) -> Option<(Cli, String)> {
    let (head, body) = text.split_once('\n')?;
    let (kind, nonce) = head.split_once(' ')?;
    match kind {
        "ensure-plan" => Some((Cli::Ensure(serde_json::from_str(body).ok()?), nonce.to_string())),
        "run-plan" => Some((Cli::Run(body.to_string()), nonce.to_string())),
        _ => None,
    }
}

// ───────────────────────── this process's side ─────────────────────────

/// Runs a command-line request and returns the exit code (0 worked, 1 failed, 2 bad arguments).
pub fn main(cli: Result<Cli, String>, window: Option<plume_ui::Window>) -> i32 {
    let cli = match cli {
        Ok(c) => c,
        Err(usage) => {
            eprintln!("{usage}");
            return 2;
        }
    };
    let res = match window {
        Some(w) => handed(&cli, &w),
        None => here(&cli),
    };
    match res {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(msg) => {
            eprintln!("{msg}");
            1
        }
    }
}

fn here(cli: &Cli) -> Result<String, String> {
    match cli {
        Cli::Ensure(e) => {
            let mut store = Store::load();
            let (_, changed) = ensure(&mut store, e, new_id())?;
            if changed {
                store.save();
            }
            Ok(if changed { tf("Plan saved.", "Plan enregistré.") } else { tf("Plan already up to date.", "Plan déjà à jour.") }.into())
        }
        Cli::Run(name) => {
            let plan = runnable(&Store::load(), name)?;
            let now = engine::now();
            let result = run_once(&plan, now);
            // read again: the file may have changed during the run; only this plan's state is written
            let mut store = Store::load();
            if store.plan(plan.id).is_some() {
                record(&mut store, plan.id, now, &result);
                store.save();
            }
            result.map(|r| report_text(&r)).map_err(|f| fail_text(&f))
        }
    }
}

fn handed(cli: &Cli, w: &plume_ui::Window) -> Result<String, String> {
    let nonce = format!("{}-{}", std::process::id(), new_id());
    let file = answer_file(&nonce);
    let _ = std::fs::remove_file(&file);
    if !w.send_text(&message(cli, &nonce)) {
        return Err(tf("Plume Backup is running but did not answer.", "Plume Backup tourne mais n'a pas répondu.").into());
    }
    // a backup can be queued behind another one: wait as long as the window is there (30 minutes at most)
    let limit = if matches!(cli, Cli::Ensure(_)) { Duration::from_secs(20) } else { Duration::from_secs(30 * 60) };
    let start = Instant::now();
    while start.elapsed() < limit {
        if let Some(a) = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Answer>(&b).ok()) {
            let _ = std::fs::remove_file(&file);
            return if a.ok { Ok(a.message) } else { Err(a.message) };
        }
        if !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(w.hwnd)) }.as_bool() {
            return Err(tf("Plume Backup closed before answering.", "Plume Backup s'est fermé avant de répondre.").into());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Err(tf("Plume Backup did not finish in time.", "Plume Backup n'a pas fini à temps.").into())
}

pub fn new_id() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)
}

/// The window's mark: Plume Backup's own, or one per data folder when PLUME_BACKUP_DIR points elsewhere (tests),
/// so a test copy and the real one never answer each other.
pub fn mark() -> String {
    match std::env::var("PLUME_BACKUP_DIR") {
        Ok(d) if !d.is_empty() => {
            let mut h: u32 = 2166136261;
            for b in d.to_lowercase().bytes() {
                h = (h ^ b as u32).wrapping_mul(16777619);
            }
            format!("PlumeBackupWindow-{h:08x}")
        }
        _ => "PlumeBackupWindow".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("plume-backup-cli-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_the_options() {
        let a = parse(&args(&["--ensure-plan", "Plume Boost settings", "--source", r"C:\a", "--dest", r"D:\b", "--keep", "12"])).unwrap().unwrap();
        assert_eq!(a, Cli::Ensure(Ensure { name: "Plume Boost settings".into(), source: r"C:\a".into(), dest: r"D:\b".into(), keep: Some(12) }));
        assert_eq!(parse(&args(&["--run-plan", "X"])).unwrap().unwrap(), Cli::Run("X".into()));
        assert!(parse(&args(&["--ensure-plan", "X", "--source", r"C:\a"])).unwrap().is_err(), "no --dest");
        assert!(parse(&args(&["--run-plan"])).unwrap().is_err());
        assert!(parse(&args(&["--ensure-plan", "X", "--source", "a", "--dest", "b", "--keep", "many"])).unwrap().is_err());
        assert!(parse(&args(&["--run"])).is_none());
    }

    #[test]
    fn messages_round_trip() {
        let e = Cli::Ensure(Ensure { name: "N".into(), source: r"C:\s".into(), dest: r"C:\d".into(), keep: None });
        assert_eq!(read_message(&message(&e, "42-1")), Some((e, "42-1".into())));
        let r = Cli::Run("Plume Boost settings".into());
        assert_eq!(read_message(&message(&r, "7")), Some((r, "7".into())));
        assert_eq!(read_message("open"), None);
    }

    #[test]
    fn ensure_creates_once_and_is_idempotent() {
        let d = tmp("ensure");
        let (src, dest) = (d.join("Snapshots"), d.join("Backups"));
        std::fs::create_dir_all(&src).unwrap();
        let mut store = Store::default();
        let e = Ensure { name: "Plume Boost settings".into(), source: src.clone(), dest: dest.clone(), keep: Some(20) };
        let (id, changed) = ensure(&mut store, &e, 5).unwrap();
        assert!(changed && id == 5);
        assert!(dest.is_dir(), "the destination folder is created");
        let p = &store.plans[0];
        assert_eq!((p.mode, p.schedule, p.keep_versions, p.sources.clone()), (Mode::Versions, Schedule::Manual, 20, vec![src.clone()]));
        assert_eq!(p.folder, "Plume Boost settings");
        let before = store.clone();
        let (id2, changed) = ensure(&mut store, &e, 9).unwrap();
        assert_eq!((id2, changed), (5, false), "twice: nothing changes");
        assert_eq!(store, before);
        // the user moved the plan's destination and made it daily: those choices stay
        let moved = d.join("Elsewhere");
        std::fs::create_dir_all(&moved).unwrap();
        store.plans[0].dest = moved.clone();
        store.plans[0].schedule = Schedule::Daily;
        let (_, changed) = ensure(&mut store, &e, 9).unwrap();
        assert!(!changed);
        assert_eq!((store.plans[0].dest.clone(), store.plans[0].schedule), (moved, Schedule::Daily));
        // the source moved: it is updated
        let src2 = d.join("Snapshots2");
        std::fs::create_dir_all(&src2).unwrap();
        let (_, changed) = ensure(&mut store, &Ensure { source: src2.clone(), ..e.clone() }, 9).unwrap();
        assert!(changed && store.plans[0].sources == vec![src2]);
        assert_eq!(store.plans.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ensure_keeps_the_safety_rules() {
        let d = tmp("rules");
        let src = d.join("Snapshots");
        std::fs::create_dir_all(&src).unwrap();
        let mut store = Store::default();
        // the destination inside the source: refused, and nothing is created in the source
        let inside = src.join("Backups");
        let r = ensure(&mut store, &Ensure { name: "A".into(), source: src.clone(), dest: inside.clone(), keep: None }, 1);
        assert!(r.is_err());
        assert!(!inside.exists(), "nothing written inside the source");
        assert!(store.plans.is_empty());
        // a missing source
        assert!(ensure(&mut store, &Ensure { name: "A".into(), source: d.join("nope"), dest: d.join("B"), keep: None }, 1).is_err());
        // relative paths
        assert!(ensure(&mut store, &Ensure { name: "A".into(), source: "rel".into(), dest: d.join("B"), keep: None }, 1).is_err());
        // two plans with the same folder name get their own folders
        store.plans.push(Plan { id: 3, name: "Other".into(), folder: "Plume Boost settings".into(), ..Default::default() });
        let (_, _) = ensure(&mut store, &Ensure { name: "Plume Boost settings".into(), source: src.clone(), dest: d.join("B"), keep: None }, 4).unwrap();
        assert_eq!(store.plans[1].folder, "Plume Boost settings (2)");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn run_plan_backs_up_and_records() {
        let d = tmp("run");
        let src = d.join("Snapshots");
        std::fs::create_dir_all(src.join("2026-11-11 10-00-00")).unwrap();
        std::fs::write(src.join("2026-11-11 10-00-00").join("registry.reg"), "Windows Registry Editor Version 5.00\r\n").unwrap();
        let mut store = Store::default();
        let e = Ensure { name: "Plume Boost settings".into(), source: src.clone(), dest: d.join("Backups"), keep: None };
        let (id, _) = ensure(&mut store, &e, 11).unwrap();
        assert!(runnable(&store, "nope").is_err());
        let plan = runnable(&store, "plume boost SETTINGS").unwrap();
        let now = engine::now();
        let r = run_once(&plan, now);
        let rep = r.clone().unwrap();
        assert_eq!((rep.files, rep.copied), (1, 1));
        record(&mut store, id, now, &r);
        assert!(store.state(id).last.unwrap().ok);
        let snaps = engine::snapshots(&plan.root(&plan.dest));
        assert_eq!(snaps.len(), 1, "one dated version at the destination");
        // a run whose destination vanished fails with a message, recorded as such
        std::fs::remove_dir_all(d.join("Backups")).unwrap();
        let r = run_once(&plan, now + 1);
        assert_eq!(r, Err(Fail::DestMissing));
        record(&mut store, id, now + 1, &r);
        assert!(!store.state(id).last.unwrap().ok);
        // paused: refused
        store.plans[0].paused = true;
        assert!(runnable(&store, "Plume Boost settings").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
