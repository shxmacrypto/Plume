//! Snapshots: before every Apply, every processor or graphics card change, Undo everything and every restore, Plume
//! Boost writes what the settings it manages are at that moment into
//! `%LOCALAPPDATA%\Plume\Boost\Snapshots\<yyyy-mm-dd hh-mm-ss>\`:
//!
//! - `registry.reg`: every registry value Plume Boost can change, under its key ("Name"=- when it didn't exist),
//!   readable by regedit (a double click restores it);
//! - `power-<GUID>.pow`: each power plan, exported by Windows itself (`powercfg /export`), and `power.json`: the
//!   active plan and the processor settings Plume Boost manages in every plan;
//! - `services.json`: the start type of the services Plume Boost manages;
//! - `gpu.json`: each graphics card's settings;
//! - `undo.json`: a copy of Plume Boost's undo log;
//! - `snapshot.json`: when and why, and what it holds; `README.txt`: how to restore it (English and French).
//!
//! All of it goes through the backend, so in tests the imaginary PC writes imaginary exports. Restoring compares each
//! saved value with the PC and changes only what differs, after taking a snapshot of its own.

use crate::{
    backend::{Fail, Hive, Raw, Shared, guids},
    gpu::Tuning,
    regfile::{self, Key, Line},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The processor settings saved for every plan.
pub const POWER_SETTINGS: [&str; 5] = [guids::PROC_MIN, guids::PROC_MAX, guids::BOOST_MODE, guids::EPP, guids::PARK_MIN];
/// The power settings whose visibility Plume Boost can change.
pub const ATTRIB_SETTINGS: [&str; 3] = [guids::BOOST_MODE, guids::EPP, guids::PARK_MIN];
pub const SERVICES: [&str; 2] = ["SysMain", "WSearch"];
/// Snapshots kept on this PC (Plume Backup keeps its own copies).
pub const KEEP: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Why {
    Apply,
    Processor,
    Gpu,
    UndoAll,
    Restore,
}

impl Why {
    pub fn text(self) -> &'static str {
        use crate::i18n::tf;
        match self {
            Why::Apply => tf("Before applying tweaks", "Avant d'appliquer des réglages"),
            Why::Processor => tf("Before a processor change", "Avant un changement du processeur"),
            Why::Gpu => tf("Before a graphics card change", "Avant un changement de la carte graphique"),
            Why::UndoAll => tf("Before Undo everything", "Avant Tout annuler"),
            Why::Restore => tf("Before restoring a snapshot", "Avant de restaurer un instantané"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub time: u64,
    pub why: Why,
    /// A few words on what was about to change ("3 tweaks", "Maximum processor state"…).
    pub detail: String,
    pub reg_values: usize,
    pub plans: usize,
    pub services: usize,
    pub gpus: usize,
    /// Plans Windows couldn't export (still in power.json).
    pub export_failures: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanSave {
    pub guid: String,
    pub name: String,
    /// setting GUID → (plugged in, on battery)
    pub settings: BTreeMap<String, (Option<u32>, Option<u32>)>,
    /// The .pow file, when Windows exported it.
    pub file: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PowerSave {
    pub active: String,
    pub plans: Vec<PlanSave>,
    pub attribs: BTreeMap<String, u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GpuSave {
    pub card: String,
    pub name: String,
    pub tuning: Tuning,
}

pub fn root() -> PathBuf {
    crate::settings::data_dir().join("Snapshots")
}

/// "2026-11-11 14-30-05" in local time.
pub fn stamp() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02} {:02}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// Every registry value Plume Boost can change, grouped by key.
pub fn managed_values() -> Vec<(Hive, String, Vec<String>)> {
    let mut out: Vec<(Hive, String, Vec<String>)> = Vec::new();
    for (hive, path, name) in crate::tweaks::registry_values() {
        match out.iter_mut().find(|(h, p, _)| *h == hive && p.eq_ignore_ascii_case(&path)) {
            Some(k) => {
                if !k.2.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    k.2.push(name);
                }
            }
            None => out.push((hive, path, vec![name])),
        }
    }
    out
}

fn write_json<T: Serialize>(p: &Path, v: &T) -> std::io::Result<()> {
    std::fs::write(p, serde_json::to_vec_pretty(v).unwrap_or_default())
}

fn read_json<T: for<'a> Deserialize<'a>>(p: &Path) -> Option<T> {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

/// Takes a snapshot now. Returns its folder.
pub fn take(be: &Shared, why: Why, detail: &str) -> std::io::Result<PathBuf> {
    take_in(be, &root(), why, detail)
}

pub fn take_in(be: &Shared, root: &Path, why: Why, detail: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let base = stamp();
    let mut dir = root.join(&base);
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{base} ({n})"));
        n += 1;
    }
    let tmp = root.join(format!(".{}.partial", dir.file_name().unwrap_or_default().to_string_lossy()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;

    // registry
    let mut keys = Vec::new();
    let mut reg_values = 0;
    for (hive, path, names) in managed_values() {
        let lines: Vec<Line> = names
            .iter()
            .map(|n| match be.reg_values(hive, &path).and_then(|vs| vs.into_iter().find(|v| v.name.eq_ignore_ascii_case(n))) {
                Some(raw) => Line::Set(Raw { name: n.clone(), ..raw }),
                None => Line::Delete(n.clone()),
            })
            .collect();
        reg_values += lines.len();
        keys.push(Key { hive, path, lines });
    }
    std::fs::write(tmp.join("registry.reg"), regfile::bytes(&keys))?;

    // power plans
    let mut power = PowerSave { active: be.active_plan().unwrap_or_default(), ..Default::default() };
    let mut export_failures = 0;
    for p in be.plans() {
        let file = format!("power-{}.pow", p.guid);
        let exported = be.power_export(&p.guid, &tmp.join(&file)).is_ok();
        if !exported {
            export_failures += 1;
        }
        let settings = POWER_SETTINGS
            .iter()
            .map(|s| (s.to_string(), (be.power_get(&p.guid, guids::SUB_PROCESSOR, s, true), be.power_get(&p.guid, guids::SUB_PROCESSOR, s, false))))
            .collect();
        power.plans.push(PlanSave { guid: p.guid, name: p.name, settings, file: exported.then_some(file) });
    }
    for s in ATTRIB_SETTINGS {
        if let Some(a) = be.power_attrib(guids::SUB_PROCESSOR, s) {
            power.attribs.insert(s.to_string(), a);
        }
    }
    write_json(&tmp.join("power.json"), &power)?;

    // services
    let services: BTreeMap<String, u32> = SERVICES.iter().filter_map(|s| be.service_start(s).map(|v| (s.to_string(), v))).collect();
    write_json(&tmp.join("services.json"), &services)?;

    // graphics cards
    let gpus: Vec<GpuSave> =
        be.gpus().into_iter().filter(|c| c.caps.tunable()).filter_map(|c| be.gpu_get(&c.id).map(|t| GpuSave { card: c.id, name: c.name, tuning: t })).collect();
    write_json(&tmp.join("gpu.json"), &gpus)?;

    let undo = crate::settings::data_dir().join("undo.json");
    if undo.is_file() {
        std::fs::copy(&undo, tmp.join("undo.json"))?;
    }
    let m = Manifest {
        time: crate::engine::now(),
        why,
        detail: detail.to_string(),
        reg_values,
        plans: power.plans.len(),
        services: services.len(),
        gpus: gpus.len(),
        export_failures,
    };
    write_json(&tmp.join("snapshot.json"), &m)?;
    std::fs::write(tmp.join("README.txt"), readme(&m, dir.file_name().unwrap_or_default().to_string_lossy().as_ref()))?;
    std::fs::rename(&tmp, &dir)?;
    prune(root);
    Ok(dir)
}

fn readme(m: &Manifest, name: &str) -> String {
    format!(
        "Plume Boost snapshot {name}\r\n\
         {why_en}{detail}\r\n\
         \r\n\
         What it holds: {r} registry values (registry.reg), {p} power plans (power-*.pow and power.json),\r\n\
         {s} service start types (services.json), {g} graphics card settings (gpu.json), and Plume Boost's undo log\r\n\
         (undo.json).\r\n\
         \r\n\
         How to restore it:\r\n\
         - Easiest: Plume Boost > Changes > Snapshots > Restore this snapshot. It takes a snapshot of the current\r\n\
           settings first and changes only what differs.\r\n\
         - By hand: double-click registry.reg (values marked =- are removed); for a power plan,\r\n\
           powercfg /import \"power-<GUID>.pow\" <GUID> in a command prompt run as administrator; services in\r\n\
           services.msc (2 = Automatic, 3 = Manual, 4 = Disabled); graphics card values from gpu.json in Plume Boost\r\n\
           or the maker's app.\r\n\
         \r\n\
         ----\r\n\
         \r\n\
         Instantané de Plume Boost {name}\r\n\
         {why_fr}{detail}\r\n\
         \r\n\
         Contenu : {r} valeurs du Registre (registry.reg), {p} modes d'alimentation (power-*.pow et power.json),\r\n\
         {s} types de démarrage de services (services.json), {g} réglages de carte graphique (gpu.json) et le journal\r\n\
         d'annulation de Plume Boost (undo.json).\r\n\
         \r\n\
         Pour le restaurer :\r\n\
         - Le plus simple : Plume Boost > Changements > Instantanés > Restaurer cet instantané. Un instantané des\r\n\
           réglages actuels est pris d'abord, et seul ce qui diffère est changé.\r\n\
         - À la main : double-cliquez sur registry.reg (les valeurs marquées =- sont supprimées) ; pour un mode\r\n\
           d'alimentation, powercfg /import \"power-<GUID>.pow\" <GUID> dans une invite de commandes en administrateur ;\r\n\
           les services dans services.msc (2 = Automatique, 3 = Manuel, 4 = Désactivé) ; les valeurs de la carte\r\n\
           graphique de gpu.json dans Plume Boost ou l'application du fabricant.\r\n",
        why_en = match m.why {
            Why::Apply => "Taken before applying tweaks",
            Why::Processor => "Taken before a processor change",
            Why::Gpu => "Taken before a graphics card change",
            Why::UndoAll => "Taken before Undo everything",
            Why::Restore => "Taken before restoring another snapshot",
        },
        why_fr = match m.why {
            Why::Apply => "Pris avant d'appliquer des réglages",
            Why::Processor => "Pris avant un changement du processeur",
            Why::Gpu => "Pris avant un changement de la carte graphique",
            Why::UndoAll => "Pris avant Tout annuler",
            Why::Restore => "Pris avant de restaurer un autre instantané",
        },
        detail = if m.detail.is_empty() { String::new() } else { format!(": {}", m.detail) },
        r = m.reg_values,
        p = m.plans,
        s = m.services,
        g = m.gpus,
    )
}

/// Only the newest KEEP snapshots stay on this PC.
fn prune(root: &Path) {
    let list = list_in(root);
    for (dir, _) in list.into_iter().skip(KEEP) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The snapshots, newest first.
pub fn list() -> Vec<(PathBuf, Manifest)> {
    list_in(&root())
}

pub fn list_in(root: &Path) -> Vec<(PathBuf, Manifest)> {
    let mut v: Vec<(PathBuf, Manifest)> = std::fs::read_dir(root)
        .map(|rd| {
            rd.flatten().filter(|e| e.path().is_dir()).filter_map(|e| read_json::<Manifest>(&e.path().join("snapshot.json")).map(|m| (e.path(), m))).collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.1.time.cmp(&a.1.time).then(b.0.cmp(&a.0)));
    v
}

// ───────────────────────── restoring ─────────────────────────

/// One difference between a snapshot and the PC now.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// `raw` None: remove the value.
    Reg {
        hive: Hive,
        path: String,
        name: String,
        raw: Option<Raw>,
    },
    /// A plan that no longer exists, imported back from its .pow file.
    Import {
        plan: String,
        name: String,
        file: PathBuf,
    },
    Power {
        plan: String,
        setting: String,
        ac: bool,
        val: u32,
    },
    Active(String),
    Attrib {
        setting: String,
        val: u32,
    },
    Service {
        name: String,
        start: u32,
    },
    Gpu {
        card: String,
        name: String,
        tuning: Tuning,
    },
}

impl Change {
    pub fn needs_admin(&self) -> bool {
        matches!(self, Change::Reg { hive: Hive::Machine, .. } | Change::Service { .. } | Change::Attrib { .. } | Change::Import { .. })
    }
    pub fn describe(&self) -> String {
        use crate::i18n::tf;
        match self {
            Change::Reg { hive, path, name, raw } => match raw.as_ref().and_then(Raw::to_val) {
                Some(v) => format!("{}\\{path}\\{name} = {}", hive.short(), v.show()),
                None if raw.is_some() => format!("{}\\{path}\\{name}", hive.short()),
                None => format!("{}\\{path}\\{name} {}", hive.short(), tf("removed", "supprimée")),
            },
            Change::Import { name, .. } => format!("{} \"{name}\"", tf("Power plan brought back:", "Mode d'alimentation recréé :")),
            Change::Power { setting, ac, val, .. } => format!(
                "{} ({}) = {val}",
                crate::engine::setting_name(setting),
                if *ac { crate::i18n::tr("plugged in") } else { crate::i18n::tr("on battery") }
            ),
            Change::Active(g) => format!("{}: {g}", crate::i18n::tr("Active power plan")),
            Change::Attrib { setting, val } => format!(
                "{}: {}",
                crate::engine::setting_name(setting),
                if *val == 0 { tf("shown in Windows' power options", "visible dans les options d'alimentation") } else { tf("hidden", "masqué") }
            ),
            Change::Service { name, start } => format!("{} {name}: {}", crate::i18n::tr("Service"), crate::engine::start_name(*start)),
            Change::Gpu { name, tuning, .. } => format!("{name}: {}", tuning.describe()),
        }
    }
}

fn same_value(now: Option<&Raw>, want: &Raw) -> bool {
    match now {
        Some(n) => n.ty == want.ty && n.data == want.data || (n.to_val().is_some() && n.to_val() == want.to_val()),
        None => false,
    }
}

/// What restoring this snapshot would change on the PC now (nothing is changed).
pub fn plan(be: &Shared, dir: &Path) -> Result<Vec<Change>, String> {
    let mut out = Vec::new();
    let reg = std::fs::read(dir.join("registry.reg")).map_err(|e| e.to_string())?;
    for k in regfile::parse_bytes(&reg) {
        let now = be.reg_values(k.hive, &k.path).unwrap_or_default();
        for l in k.lines {
            let cur = now.iter().find(|v| v.name.eq_ignore_ascii_case(l.name()));
            match l {
                Line::Set(raw) if !same_value(cur, &raw) => {
                    out.push(Change::Reg { hive: k.hive, path: k.path.clone(), name: raw.name.clone(), raw: Some(raw) })
                }
                Line::Delete(name) if cur.is_some() => out.push(Change::Reg { hive: k.hive, path: k.path.clone(), name, raw: None }),
                _ => {}
            }
        }
    }
    if let Some(p) = read_json::<PowerSave>(&dir.join("power.json")) {
        let plans_now = be.plans();
        for ps in &p.plans {
            let exists = plans_now.iter().any(|x| x.guid == ps.guid);
            if !exists {
                match &ps.file {
                    Some(f) => out.push(Change::Import { plan: ps.guid.clone(), name: ps.name.clone(), file: dir.join(f) }),
                    None => continue,
                }
            }
            for (setting, (ac, dc)) in &ps.settings {
                for (is_ac, want) in [(true, ac), (false, dc)] {
                    if let Some(v) = want
                        && (!exists || be.power_get(&ps.guid, guids::SUB_PROCESSOR, setting, is_ac) != Some(*v))
                    {
                        out.push(Change::Power { plan: ps.guid.clone(), setting: setting.clone(), ac: is_ac, val: *v });
                    }
                }
            }
        }
        if !p.active.is_empty() && be.active_plan().as_deref() != Some(p.active.as_str()) {
            out.push(Change::Active(p.active.clone()));
        }
        for (s, a) in &p.attribs {
            if be.power_attrib(guids::SUB_PROCESSOR, s) != Some(*a) {
                out.push(Change::Attrib { setting: s.clone(), val: *a });
            }
        }
    }
    if let Some(sv) = read_json::<BTreeMap<String, u32>>(&dir.join("services.json")) {
        for (n, v) in sv {
            if be.service_start(&n) != Some(v) {
                out.push(Change::Service { name: n, start: v });
            }
        }
    }
    if let Some(gs) = read_json::<Vec<GpuSave>>(&dir.join("gpu.json")) {
        let cards = be.gpus();
        for g in gs {
            if let Some(c) = cards.iter().find(|c| c.id == g.card) {
                let want = c.caps.clamp(&g.tuning);
                if be.gpu_get(&g.card).is_some_and(|now| now != want) {
                    out.push(Change::Gpu { card: g.card, name: g.name, tuning: want });
                }
            }
        }
    }
    Ok(out)
}

/// Makes the changes; returns those that failed. A plan is imported before its settings, the active plan is set
/// after them.
pub fn apply(be: &Shared, changes: &[Change]) -> Vec<(Change, Fail)> {
    let mut fails = Vec::new();
    let mut order: Vec<&Change> = changes.iter().collect();
    order.sort_by_key(|c| match c {
        Change::Import { .. } => 0,
        Change::Active(_) => 2,
        _ => 1,
    });
    for c in order {
        let r = match c {
            Change::Reg { hive, path, name, raw: Some(raw) } => be.reg_set_raw(*hive, path, &Raw { name: name.clone(), ..raw.clone() }),
            Change::Reg { hive, path, name, raw: None } => be.reg_del(*hive, path, name),
            Change::Import { plan, file, .. } => be.power_import(plan, file),
            Change::Power { plan, setting, ac, val } => be.power_set(plan, guids::SUB_PROCESSOR, setting, *ac, *val),
            Change::Active(g) => be.set_active_plan(g),
            Change::Attrib { setting, val } => be.set_power_attrib(guids::SUB_PROCESSOR, setting, *val),
            Change::Service { name, start } => be.set_service_start(name, *start),
            Change::Gpu { card, tuning, .. } => be.gpu_set(card, tuning),
        };
        if let Err(f) = r {
            fails.push((c.clone(), f));
        }
    }
    fails
}

/// The undo log saved in a snapshot.
pub fn saved_log(dir: &Path) -> crate::engine::UndoLog {
    read_json(&dir.join("undo.json")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{fake::Fake, fake::State, fake::fake_gpu};
    use crate::engine::Engine;
    use std::sync::Arc;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("plume-boost-snap-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_snapshot_holds_everything_and_restores_it() {
        let state = State { admin: true, gpus: vec![fake_gpu("nvidia").unwrap(), fake_gpu("iris").unwrap()], ..State::default() };
        let fake = Arc::new(Fake::memory(state));
        let be: Shared = fake.clone();
        let root = dir("restore");
        let snap = take_in(&be, &root, Why::Apply, "test").unwrap();
        for f in ["registry.reg", "power.json", "services.json", "gpu.json", "snapshot.json", "README.txt"] {
            assert!(snap.join(f).is_file(), "{f} written");
        }
        assert!(snap.join(format!("power-{}.pow", guids::BALANCED)).is_file(), "each plan exported");
        let m = &list_in(&root)[0].1;
        assert_eq!((m.why, m.plans, m.services, m.gpus), (Why::Apply, 2, 2, 1), "the integrated card has nothing to save");
        let reg = String::from_utf16_lossy(
            &std::fs::read(snap.join("registry.reg")).unwrap()[2..].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect::<Vec<_>>(),
        );
        assert!(reg.contains("[HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\Dwm]") && reg.contains("=-"), "absent values are written as removals");
        assert!(plan(&be, &snap).unwrap().is_empty(), "nothing differs right after");

        // change things through the engine (tweaks, processor, card), then restore
        let mut e = Engine::new(be.clone(), None);
        let start = fake.snapshot();
        assert!(e.apply(&["plan_ultimate", "hags", "mpo_off", "sysmain_off", "visual_fx"]).failed.is_empty());
        e.set_power(guids::EPP, true, 80).unwrap();
        let mut t = be.gpu_get("nvidia-1-rtx4070").unwrap();
        t.knobs.insert(crate::gpu::Knob::CoreOffset, 150.0);
        t.fan = crate::gpu::Fan::Fixed(60);
        e.set_gpu("nvidia-1-rtx4070", &t).unwrap();
        be.set_power_attrib(guids::SUB_PROCESSOR, guids::EPP, 0).unwrap();
        let changes = plan(&be, &snap).unwrap();
        assert!(changes.iter().any(|c| matches!(c, Change::Active(_))));
        assert!(changes.iter().any(|c| matches!(c, Change::Gpu { .. })));
        assert!(changes.iter().any(|c| matches!(c, Change::Service { .. })));
        assert!(changes.iter().any(|c| c.needs_admin()));
        assert!(apply(&be, &changes).is_empty());
        assert!(plan(&be, &snap).unwrap().is_empty(), "everything is as in the snapshot");
        let end = fake.snapshot();
        assert_eq!((end.active.clone(), end.reg.clone(), end.services.clone(), end.attribs.clone()), (start.active, start.reg, start.services, start.attribs));
        assert_eq!(end.gpus[0].now, start.gpus[0].now);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_deleted_plan_is_imported_back() {
        let fake = Arc::new(Fake::memory(State { admin: true, ..State::default() }));
        let be: Shared = fake.clone();
        let root = dir("import");
        // a plan that exists at snapshot time, then is deleted
        let g = be.duplicate_plan(guids::HIGH).unwrap();
        be.power_set(&g, guids::SUB_PROCESSOR, guids::PROC_MAX, true, 90).unwrap();
        let snap = take_in(&be, &root, Why::Processor, "").unwrap();
        be.delete_plan(&g).unwrap();
        let changes = plan(&be, &snap).unwrap();
        assert!(matches!(changes[0], Change::Import { .. }) || changes.iter().any(|c| matches!(c, Change::Import { .. })));
        assert!(apply(&be, &changes).is_empty());
        assert_eq!(be.power_get(&g, guids::SUB_PROCESSOR, guids::PROC_MAX, true), Some(90));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_the_newest_are_kept() {
        let fake = Arc::new(Fake::memory(State::default()));
        let be: Shared = fake;
        let root = dir("prune");
        for _ in 0..KEEP + 3 {
            take_in(&be, &root, Why::Gpu, "").unwrap();
        }
        assert_eq!(list_in(&root).len(), KEEP);
        let _ = std::fs::remove_dir_all(&root);
    }
}
