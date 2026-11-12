//! Applying and undoing. Before anything changes, the value it had is written to the undo log (undo.json, saved after
//! every change), so every change can be put back: one tweak at a time, or all of them at once in reverse order.
//!
//! The log keeps one entry per change, in order. "Undo everything" walks it backwards, which restores exactly what the
//! PC had before Plume Boost, whatever was applied in between. Undoing a single tweak restores its own entries, unless
//! a later change touched the same setting: then the older value is handed to that later entry instead, so a later
//! "Undo everything" still ends at the original.

use crate::backend::{Fail, Hive, Res, Shared, Val, guids};
use crate::tweaks::{After, Needs, Op, TWEAKS, Tweak, by_id};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DX_PATH: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
const DX_NAME: &str = "DirectXUserGlobalSettings";

/// What a setting was before a change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Before {
    /// `old` None: the value didn't exist (undo deletes it).
    Reg {
        hive: Hive,
        path: String,
        name: String,
        old: Option<Val>,
    },
    DxFlag {
        name: String,
        old: Option<String>,
    },
    /// The plan that was active; `created`: a plan Plume Boost made (deleted again on undo).
    Plan {
        old: String,
        created: Option<String>,
    },
    Power {
        plan: String,
        setting: String,
        ac: bool,
        old: u32,
    },
    Service {
        name: String,
        old: u32,
    },
    /// A graphics card's settings (all of them, as read before the change).
    Gpu {
        card: String,
        old: crate::gpu::Tuning,
    },
    /// Whether a power setting is shown in Windows' advanced power options.
    Attrib {
        setting: String,
        old: u32,
    },
}

impl Before {
    /// Two entries about the same setting have the same key.
    pub fn key(&self) -> String {
        match self {
            Before::Reg { hive, path, name, .. } => format!("reg|{}|{}|{}", hive.short(), path.to_lowercase(), name.to_lowercase()),
            Before::DxFlag { name, .. } => format!("dx|{}", name.to_lowercase()),
            Before::Plan { .. } => "plan".into(),
            Before::Power { plan, setting, ac, .. } => format!("power|{plan}|{setting}|{ac}"),
            Before::Service { name, .. } => format!("svc|{}", name.to_lowercase()),
            Before::Gpu { card, .. } => format!("gpu|{card}"),
            Before::Attrib { setting, .. } => format!("attrib|{setting}"),
        }
    }
    pub fn needs_admin(&self) -> bool {
        matches!(self, Before::Reg { hive: Hive::Machine, .. } | Before::Service { .. } | Before::Attrib { .. })
    }
    /// A short line for the Changes page: the setting and the value it will get back.
    pub fn describe(&self) -> String {
        let none = crate::i18n::tr("(not set)");
        match self {
            Before::Reg { hive, path, name, old } => format!("{}\\{}\\{} ← {}", hive.short(), path, name, old.as_ref().map(Val::show).unwrap_or(none.into())),
            Before::DxFlag { name, old } => format!("DirectX {name} ← {}", old.clone().unwrap_or(none.into())),
            Before::Plan { old, .. } => format!("{} ← {old}", crate::i18n::tr("Power plan")),
            Before::Power { setting, ac, old, .. } => {
                format!("{} ({}) ← {old}", setting_name(setting), if *ac { crate::i18n::tr("plugged in") } else { crate::i18n::tr("on battery") })
            }
            Before::Service { name, old } => format!("{} {name} ← {}", crate::i18n::tr("Service"), start_name(*old)),
            Before::Gpu { card, old } => format!("{card} ← {}", old.describe()),
            Before::Attrib { setting, old } => format!(
                "{} ← {}",
                setting_name(setting),
                if *old == 0 {
                    crate::i18n::tf("shown in Windows' power options", "visible dans les options d'alimentation")
                } else {
                    crate::i18n::tf("hidden from Windows' power options", "masqué des options d'alimentation")
                }
            ),
        }
    }
}

pub fn setting_name(guid: &str) -> &'static str {
    use crate::i18n::tr;
    match guid {
        guids::PROC_MIN => tr("Minimum processor state"),
        guids::PROC_MAX => tr("Maximum processor state"),
        guids::BOOST_MODE => tr("Processor boost mode"),
        guids::EPP => crate::i18n::tf("Energy performance preference", "Préférence performances / énergie"),
        guids::PARK_MIN => crate::i18n::tf("Core parking: minimum cores", "Parcage des cœurs : cœurs minimum"),
        _ => tr("Power setting"),
    }
}

pub fn start_name(s: u32) -> &'static str {
    use crate::i18n::tr;
    match s {
        2 => tr("Automatic"),
        3 => tr("Manual"),
        4 => tr("Disabled"),
        _ => tr("Other"),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The tweak's id, or "cpu" for the Processor page's own choices.
    pub tweak: String,
    /// Seconds since 1970.
    pub when: u64,
    pub before: Before,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UndoLog {
    pub entries: Vec<Entry>,
    /// Restore points Plume Boost created (time, name).
    pub restore_points: Vec<(u64, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    On,
    Off,
    /// Some of its settings are as Plume Boost would set them, not all.
    Partly,
    /// The PC doesn't have what it needs.
    Unavailable,
}

/// What a batch did.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub applied: Vec<&'static str>,
    pub already: Vec<&'static str>,
    pub failed: Vec<(&'static str, Fail)>,
    pub after: Option<After>,
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn now_f64() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// The `name=value;` pairs of the DirectX settings string.
pub fn dx_get(s: &str, name: &str) -> Option<String> {
    s.split(';').filter_map(|p| p.split_once('=')).find(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string())
}

/// The string with `name` set to `val` (or removed when None), other pairs kept in their order.
pub fn dx_put(s: &str, name: &str, val: Option<&str>) -> String {
    let mut out = String::new();
    let mut done = false;
    for p in s.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        match p.split_once('=') {
            Some((k, _)) if k.trim().eq_ignore_ascii_case(name) => {
                if let Some(v) = val
                    && !done
                {
                    out.push_str(&format!("{name}={v};"));
                }
                done = true;
            }
            _ => out.push_str(&format!("{p};")),
        }
    }
    if !done && let Some(v) = val {
        out.push_str(&format!("{name}={v};"));
    }
    out
}

pub struct Engine {
    pub be: Shared,
    pub log: UndoLog,
    file: Option<PathBuf>,
    /// The user accepted Intel's overclocking waiver (Settings): needed before an Arc card is changed.
    pub waiver_ok: bool,
}

/// The error when an Intel card is changed before its waiver was accepted.
pub const NO_WAIVER: &str = "waiver";

impl Engine {
    /// The engine with the log saved in `dir` (None: kept in memory only, for unit tests).
    pub fn new(be: Shared, dir: Option<PathBuf>) -> Engine {
        let file = dir.map(|d| d.join("undo.json"));
        let log = file.as_ref().and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Engine { be, log, file, waiver_ok: false }
    }

    fn save(&self) {
        let Some(f) = &self.file else { return };
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(b) = serde_json::to_vec_pretty(&self.log) {
            let tmp = f.with_extension("tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, f);
            }
        }
    }

    // ── reading ──────────────────────────────────────────────────────────────────────────────────────────────────

    fn dx_string(&self) -> String {
        match self.be.reg_get(Hive::User, DX_PATH, DX_NAME) {
            Some(Val::Sz(s)) => s,
            _ => String::new(),
        }
    }

    /// The plan `src` is active (the GUID itself, or Plume Boost's copy of a hidden template, found by name).
    fn plan_is_active(&self, src: &str) -> bool {
        let Some(active) = self.be.active_plan() else { return false };
        if active == src {
            return true;
        }
        let builtin = [guids::BALANCED, guids::HIGH, guids::POWER_SAVER].contains(&src);
        !builtin && self.be.plan_name(src).is_some_and(|n| self.be.plan_name(&active).as_deref() == Some(n.as_str()))
    }

    /// An existing plan that is `src` or a copy of it.
    fn find_plan(&self, src: &str) -> Option<String> {
        let plans = self.be.plans();
        if plans.iter().any(|p| p.guid == src) {
            return Some(src.to_string());
        }
        let name = self.be.plan_name(src)?;
        plans.into_iter().find(|p| p.name == name).map(|p| p.guid)
    }

    pub fn available(&self, t: &Tweak) -> bool {
        match t.needs {
            Needs::Nothing => true,
            Needs::Battery => self.be.has_battery(),
            Needs::Hibernation => self.be.reg_get(Hive::Machine, r"SYSTEM\CurrentControlSet\Control\Power", "HibernateEnabled") != Some(Val::Dword(0)),
            Needs::Plan(g) => self.find_plan(g).is_some() || self.be.plan_name(g).is_some(),
        }
    }

    fn op_done(&self, op: &Op) -> bool {
        match *op {
            Op::Reg { hive, path, name, val, absent } => {
                let cur = self.be.reg_get(hive, path, name).or(absent.map(|a| a.val()));
                cur == Some(val.val())
            }
            Op::DxFlag { name, val } => dx_get(&self.dx_string(), name).as_deref() == Some(val),
            Op::Plan(src) => self.plan_is_active(src),
            Op::Power { setting, ac, val } => self.be.active_plan().and_then(|p| self.be.power_get(&p, guids::SUB_PROCESSOR, setting, ac)) == Some(val),
            Op::Service { name, start } => self.be.service_start(name) == Some(start),
        }
    }

    pub fn status(&self, t: &Tweak) -> Status {
        if !self.available(t) {
            return Status::Unavailable;
        }
        let done = t.ops.iter().filter(|o| self.op_done(o)).count();
        if done == t.ops.len() {
            Status::On
        } else if done == 0 {
            Status::Off
        } else {
            Status::Partly
        }
    }

    // ── changing ─────────────────────────────────────────────────────────────────────────────────────────────────

    /// Logs the value a setting had. False when it was already logged for this tweak (nothing added).
    fn record(&mut self, tweak: &str, before: Before) -> bool {
        // the same tweak changing the same setting again (a second click on the Processor page): the first value
        // is the one to restore
        let key = before.key();
        if let Some(last) = self.log.entries.iter().rev().find(|e| e.before.key() == key)
            && last.tweak == tweak
        {
            return false;
        }
        self.log.entries.push(Entry { tweak: tweak.into(), when: now(), before });
        self.save();
        true
    }

    fn apply_op(&mut self, tweak: &str, op: &Op) -> Res {
        let be = self.be.clone();
        match *op {
            Op::Reg { hive, path, name, val, .. } => {
                let old = be.reg_get(hive, path, name);
                let new = val.val();
                if old.as_ref() == Some(&new) {
                    return Ok(());
                }
                be.reg_set(hive, path, name, &new)?;
                self.record(tweak, Before::Reg { hive, path: path.into(), name: name.into(), old });
            }
            Op::DxFlag { name, val } => {
                let cur = self.dx_string();
                let old = dx_get(&cur, name);
                if old.as_deref() == Some(val) {
                    return Ok(());
                }
                be.reg_set(Hive::User, DX_PATH, DX_NAME, &Val::Sz(dx_put(&cur, name, Some(val))))?;
                self.record(tweak, Before::DxFlag { name: name.into(), old });
            }
            Op::Plan(src) => {
                if self.plan_is_active(src) {
                    return Ok(());
                }
                let old = be.active_plan().ok_or_else(|| Fail::Other("no active power plan".into()))?;
                let (guid, created) = match self.find_plan(src) {
                    Some(g) => (g, None),
                    None => {
                        let g = be.duplicate_plan(src)?;
                        (g.clone(), Some(g))
                    }
                };
                if let Err(e) = be.set_active_plan(&guid) {
                    if let Some(c) = &created {
                        let _ = be.delete_plan(c);
                    }
                    return Err(e);
                }
                self.record(tweak, Before::Plan { old, created });
            }
            Op::Power { setting, ac, val } => {
                let plan = be.active_plan().ok_or_else(|| Fail::Other("no active power plan".into()))?;
                self.set_power_on(tweak, &plan, setting, ac, val)?;
            }
            Op::Service { name, start } => {
                let old = be.service_start(name).ok_or(Fail::Missing)?;
                if old == start {
                    return Ok(());
                }
                be.set_service_start(name, start)?;
                self.record(tweak, Before::Service { name: name.into(), old });
            }
        }
        Ok(())
    }

    fn set_power_on(&mut self, tweak: &str, plan: &str, setting: &str, ac: bool, val: u32) -> Res {
        let old = self.be.power_get(plan, guids::SUB_PROCESSOR, setting, ac).ok_or(Fail::Missing)?;
        if old == val {
            return Ok(());
        }
        self.be.power_set(plan, guids::SUB_PROCESSOR, setting, ac, val)?;
        self.record(tweak, Before::Power { plan: plan.into(), setting: setting.into(), ac, old });
        Ok(())
    }

    /// A processor setting of the active plan, chosen on the Processor page (kept in the log as "cpu").
    pub fn set_power(&mut self, setting: &str, ac: bool, val: u32) -> Res {
        let plan = self.be.active_plan().ok_or_else(|| Fail::Other("no active power plan".into()))?;
        self.set_power_on("cpu", &plan, setting, ac, val)
    }

    /// The tweaks among `ids` that need administrator rights and are not on yet.
    pub fn admin_needed(&self, ids: &[&str]) -> Vec<&'static str> {
        ids.iter().filter_map(|id| by_id(id)).filter(|t| t.needs_admin() && self.status(t) != Status::On).map(|t| t.id).collect()
    }

    /// Applies the ticked tweaks: power plans first (other power settings then go to the new plan), skipping those
    /// already on or unavailable. A tweak that fails halfway keeps what it changed in the log, so it can be undone.
    pub fn apply(&mut self, ids: &[&str]) -> Report {
        let mut list: Vec<&'static Tweak> = TWEAKS.iter().filter(|t| ids.contains(&t.id)).collect();
        // one plan only: the last ticked of the group wins
        let plans: Vec<&'static str> = list.iter().filter(|t| t.group == Some("plan")).map(|t| t.id).collect();
        if plans.len() > 1 {
            let keep = plans[plans.len() - 1];
            list.retain(|t| t.group != Some("plan") || t.id == keep);
        }
        list.sort_by_key(|t| if t.group == Some("plan") { 0 } else { 1 });
        let mut r = Report::default();
        for t in list {
            match self.status(t) {
                Status::On => {
                    r.already.push(t.id);
                    continue;
                }
                Status::Unavailable => {
                    r.failed.push((t.id, Fail::Missing));
                    continue;
                }
                _ => {}
            }
            let mut err = None;
            for op in t.ops {
                if let Err(e) = self.apply_op(t.id, op) {
                    err = Some(e);
                    break;
                }
            }
            match err {
                Some(e) => r.failed.push((t.id, e)),
                None => {
                    r.applied.push(t.id);
                    r.after = r.after.max(Some(t.after));
                }
            }
        }
        if r.after == Some(After::Now) {
            r.after = None;
        }
        r
    }

    fn restore(&self, b: &Before) -> Res {
        let be = &self.be;
        match b {
            Before::Reg { hive, path, name, old } => match old {
                Some(v) => be.reg_set(*hive, path, name, v),
                None => be.reg_del(*hive, path, name),
            },
            Before::DxFlag { name, old } => {
                let s = dx_put(&self.dx_string(), name, old.as_deref());
                if s.is_empty() { be.reg_del(Hive::User, DX_PATH, DX_NAME) } else { be.reg_set(Hive::User, DX_PATH, DX_NAME, &Val::Sz(s)) }
            }
            Before::Plan { old, created } => {
                be.set_active_plan(old)?;
                if let Some(c) = created {
                    be.delete_plan(c)?;
                }
                Ok(())
            }
            Before::Power { plan, setting, ac, old } => {
                if !be.plans().iter().any(|p| &p.guid == plan) {
                    return Ok(()); // the plan is gone (deleted with its copy): nothing left to restore
                }
                be.power_set(plan, guids::SUB_PROCESSOR, setting, *ac, *old)
            }
            Before::Service { name, old } => be.set_service_start(name, *old),
            Before::Gpu { card, old } => be.gpu_set(card, old),
            Before::Attrib { setting, old } => be.set_power_attrib(guids::SUB_PROCESSOR, setting, *old),
        }
    }

    /// Undoes one tweak (or "cpu"). Entries that fail stay in the log.
    pub fn undo(&mut self, tweak: &str) -> Vec<Fail> {
        let mut fails = Vec::new();
        let mut i = self.log.entries.len();
        while i > 0 {
            i -= 1;
            if self.log.entries[i].tweak != tweak {
                continue;
            }
            let key = self.log.entries[i].before.key();
            // a later change of the same setting: it inherits the older value instead
            if let Some(j) = (i + 1..self.log.entries.len()).find(|&j| self.log.entries[j].before.key() == key) {
                let older = self.log.entries[i].before.clone();
                let later = &mut self.log.entries[j].before;
                *later = match (older, later.clone()) {
                    (Before::Plan { old, created: c1 }, Before::Plan { created: c2, .. }) => Before::Plan { old, created: c2.or(c1) },
                    (o, _) => o,
                };
                self.log.entries.remove(i);
                continue;
            }
            let b = self.log.entries[i].before.clone();
            match self.restore(&b) {
                Ok(()) => {
                    self.log.entries.remove(i);
                }
                Err(e) => fails.push(e),
            }
        }
        self.save();
        fails
    }

    /// Undoes every change, newest first. Returns what could not be put back (those entries stay in the log).
    pub fn undo_all(&mut self) -> Vec<(String, Fail)> {
        let mut fails = Vec::new();
        let mut i = self.log.entries.len();
        while i > 0 {
            i -= 1;
            let b = self.log.entries[i].before.clone();
            match self.restore(&b) {
                Ok(()) => {
                    self.log.entries.remove(i);
                }
                Err(e) => fails.push((self.log.entries[i].tweak.clone(), e)),
            }
        }
        self.save();
        fails
    }

    /// The log holds changes that only an administrator can put back.
    pub fn undo_needs_admin(&self) -> bool {
        self.log.entries.iter().any(|e| e.before.needs_admin())
    }

    // ── graphics cards, power setting visibility, the sign-in value ─────────────────────────────────────────────

    /// Sets a graphics card, every value brought inside the ranges its driver reports. Logged as "gpu" (the card's
    /// settings before the first change are the ones Undo puts back). Returns what the card had just before and
    /// whether a log entry was added (for "Revert" after a try).
    pub fn set_gpu(&mut self, card: &str, t: &crate::gpu::Tuning) -> Res<(crate::gpu::Tuning, bool)> {
        let c = self.be.gpus().into_iter().find(|c| c.id == card).ok_or(Fail::Missing)?;
        let want = c.caps.clamp(t);
        let prev = self.be.gpu_get(card).ok_or(Fail::Missing)?;
        if c.needs_waiver {
            if !self.waiver_ok {
                return Err(Fail::Other(NO_WAIVER.into()));
            }
            self.be.gpu_waiver(card)?;
        }
        self.be.gpu_set(card, &want)?;
        let logged = self.record("gpu", Before::Gpu { card: card.into(), old: prev.clone() });
        Ok((prev, logged))
    }

    /// Back to the card's stock settings (logged like any change).
    pub fn reset_gpu(&mut self, card: &str) -> Res<crate::gpu::Tuning> {
        let prev = self.be.gpu_get(card).ok_or(Fail::Missing)?;
        self.be.gpu_reset(card)?;
        self.record("gpu", Before::Gpu { card: card.into(), old: prev.clone() });
        Ok(prev)
    }

    /// Undoes a try: the card gets `prev` back, and the log entry the try added (if any) goes.
    pub fn revert_gpu(&mut self, card: &str, prev: &crate::gpu::Tuning, logged: bool) -> Res {
        self.be.gpu_set(card, prev)?;
        if logged {
            let key = format!("gpu|{card}");
            if let Some(i) = self.log.entries.iter().rposition(|e| e.before.key() == key) {
                self.log.entries.remove(i);
                self.save();
            }
        }
        Ok(())
    }

    /// Shows (0) or hides (1) a power setting in Windows' advanced power options; logged as "cpu".
    pub fn set_attrib(&mut self, setting: &str, attrib: u32) -> Res {
        let old = self.be.power_attrib(guids::SUB_PROCESSOR, setting).ok_or(Fail::Missing)?;
        if old == attrib {
            return Ok(());
        }
        self.be.set_power_attrib(guids::SUB_PROCESSOR, setting, attrib)?;
        self.record("cpu", Before::Attrib { setting: setting.into(), old });
        Ok(())
    }

    /// Sets (Some) or removes (None) a registry value, logged under `tweak` (the sign-in value of "gpu_signin").
    pub fn set_reg(&mut self, tweak: &str, hive: Hive, path: &str, name: &str, v: Option<Val>) -> Res {
        let old = self.be.reg_get(hive, path, name);
        if old == v {
            return Ok(());
        }
        match &v {
            Some(v) => self.be.reg_set(hive, path, name, v)?,
            None => self.be.reg_del(hive, path, name)?,
        }
        self.record(tweak, Before::Reg { hive, path: path.into(), name: name.into(), old });
        Ok(())
    }

    /// The log of a snapshot that was restored completely replaces this one (the PC is as it was then).
    pub fn replace_log(&mut self, log: UndoLog) {
        self.log = log;
        self.save();
    }

    pub fn restore_point(&mut self) -> Res {
        let name = "Plume Boost";
        self.be.restore_point(name)?;
        self.log.restore_points.push((now(), name.into()));
        self.save();
        Ok(())
    }
}

impl UndoLog {
    /// The tweaks that have entries in the log, newest first.
    pub fn changed(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in self.entries.iter().rev() {
            if !out.contains(&e.tweak) {
                out.push(e.tweak.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Backend, fake::Fake, fake::State};
    use crate::tweaks::Profile;
    use std::sync::Arc;

    fn engine(admin: bool) -> (Engine, Arc<Fake>) {
        let state = State { admin, ..State::default() };
        let fake = Arc::new(Fake::memory(state));
        (Engine::new(fake.clone(), None), fake)
    }

    #[test]
    fn dx_string_pairs() {
        assert_eq!(dx_put("AutoHDREnable=1;", "SwapEffectUpgradeEnable", Some("1")), "AutoHDREnable=1;SwapEffectUpgradeEnable=1;");
        assert_eq!(dx_put("A=1;VRROptimizeEnable=0;B=2;", "VRROptimizeEnable", Some("1")), "A=1;VRROptimizeEnable=1;B=2;");
        assert_eq!(dx_put("A=1;VRROptimizeEnable=1;", "VRROptimizeEnable", None), "A=1;");
        assert_eq!(dx_get("A=1; SwapEffectUpgradeEnable=1;", "swapeffectupgradeenable").as_deref(), Some("1"));
        assert_eq!(dx_get("", "A"), None);
    }

    /// Every tweak, alone: off → apply → on → undo → exactly the imaginary PC it started from.
    #[test]
    fn each_tweak_applies_and_undoes() {
        for t in TWEAKS {
            let (mut e, fake) = engine(true);
            let start = fake.snapshot();
            assert_ne!(e.status(t), Status::Unavailable, "{} is available on the fake PC", t.id);
            if e.status(t) == Status::On {
                // on by default (Game Mode, Fast Startup): nothing to do, nothing logged
                let r = e.apply(&[t.id]);
                assert_eq!(r.already, vec![t.id]);
                assert!(e.log.entries.is_empty());
                continue;
            }
            let r = e.apply(&[t.id]);
            assert_eq!(r.applied, vec![t.id], "{} applied: {:?}", t.id, r.failed);
            assert_eq!(e.status(t), Status::On, "{} is on after apply", t.id);
            assert!(!e.log.entries.is_empty(), "{} logged its old values", t.id);
            assert!(e.undo(t.id).is_empty(), "{} undone without error", t.id);
            assert!(e.log.entries.is_empty());
            let mut end = fake.snapshot();
            end.writes.clear();
            end.next_id = start.next_id;
            let mut start2 = start.clone();
            start2.writes.clear();
            assert_eq!(end, start2, "{}: the PC is back as it was", t.id);
        }
    }

    #[test]
    fn needs_admin_and_refuses_without() {
        let (mut e, fake) = engine(false);
        assert_eq!(e.admin_needed(&["hags", "game_mode", "sysmain_off"]), vec!["hags", "sysmain_off"]);
        let r = e.apply(&["hags"]);
        assert_eq!(r.failed, vec![("hags", Fail::NeedsAdmin)]);
        assert!(e.log.entries.is_empty(), "nothing logged when nothing changed");
        assert!(fake.snapshot().writes.is_empty());
    }

    #[test]
    fn ultimate_is_created_then_removed() {
        let (mut e, fake) = engine(false);
        let t = by_id("plan_ultimate").unwrap();
        assert_eq!(e.status(t), Status::Off);
        let r = e.apply(&["plan_ultimate"]);
        assert_eq!(r.applied, vec!["plan_ultimate"]);
        let s = fake.snapshot();
        assert_eq!(s.plans.len(), 3, "a copy of the template was made");
        assert!(s.plans.iter().any(|p| p.name == "Ultimate Performance" && p.guid == s.active));
        // applying again finds the copy instead of making another one
        assert_eq!(e.apply(&["plan_ultimate"]).already, vec!["plan_ultimate"]);
        assert!(e.undo_all().is_empty());
        let s = fake.snapshot();
        assert_eq!(s.active, guids::BALANCED);
        assert_eq!(s.plans.len(), 2, "the copy is deleted again");
    }

    #[test]
    fn plans_exclude_each_other_and_power_goes_to_the_new_plan() {
        let (mut e, fake) = engine(true);
        // ticked in the "wrong" order: the plan still comes first, and only the last plan ticked is used
        let r = e.apply(&["battery_boost_efficient", "plan_ultimate", "plan_high"]);
        assert_eq!(r.applied, vec!["plan_high", "battery_boost_efficient"]);
        let s = fake.snapshot();
        assert_eq!(s.active, guids::HIGH);
        assert_eq!(e.be.power_get(guids::HIGH, guids::SUB_PROCESSOR, guids::BOOST_MODE, false), Some(3));
        assert_eq!(e.be.power_get(guids::BALANCED, guids::SUB_PROCESSOR, guids::BOOST_MODE, false), Some(2), "the old plan untouched");
        e.undo_all();
        assert_eq!(fake.snapshot().active, guids::BALANCED);
        assert_eq!(e.be.power_get(guids::HIGH, guids::SUB_PROCESSOR, guids::BOOST_MODE, false), Some(2));
    }

    #[test]
    fn profiles_apply_and_undo_everything() {
        for p in Profile::ALL {
            let (mut e, fake) = engine(true);
            let start = fake.snapshot();
            let ids = p.ticks(|id| by_id(id).is_some_and(|t| e.available(t)));
            let r = e.apply(&ids);
            assert!(r.failed.is_empty(), "{p:?}: {:?}", r.failed);
            for id in &ids {
                assert_eq!(e.status(by_id(id).unwrap()), Status::On, "{p:?}: {id}");
            }
            assert!(e.undo_all().is_empty());
            assert!(e.log.entries.is_empty());
            let (mut a, mut b) = (fake.snapshot(), start);
            a.writes.clear();
            b.writes.clear();
            a.next_id = b.next_id;
            assert_eq!(a, b, "{p:?}: everything is back");
        }
    }

    #[test]
    fn undo_one_keeps_the_other_and_the_original() {
        let (mut e, fake) = engine(true);
        // two tweaks share the DirectX string; two changes of the same plan setting
        e.apply(&["windowed_opt", "vrr"]);
        assert_eq!(e.dx_string(), "AutoHDREnable=1;SwapEffectUpgradeEnable=1;VRROptimizeEnable=1;");
        e.undo("windowed_opt");
        assert_eq!(e.dx_string(), "AutoHDREnable=1;VRROptimizeEnable=1;");
        assert_eq!(e.status(by_id("vrr").unwrap()), Status::On);
        e.undo_all();
        assert_eq!(e.dx_string(), "AutoHDREnable=1;");

        // a plan changed by one tweak, then by another; undoing the first hands the original to the second
        e.apply(&["plan_high"]);
        e.apply(&["plan_ultimate"]);
        assert!(e.undo("plan_high").is_empty());
        assert!(fake.snapshot().plans.iter().any(|p| p.guid == fake.snapshot().active && p.name == "Ultimate Performance"), "still on Ultimate");
        e.undo_all();
        let s = fake.snapshot();
        assert_eq!((s.active.as_str(), s.plans.len()), (guids::BALANCED, 2), "back to the original plan, copy deleted");
    }

    #[test]
    fn cpu_choices_are_logged_once_per_setting() {
        let (mut e, _fake) = engine(false);
        e.set_power(guids::PROC_MAX, true, 99).unwrap();
        e.set_power(guids::PROC_MAX, true, 80).unwrap();
        assert_eq!(e.log.entries.len(), 1, "the first value is the one kept");
        assert_eq!(e.be.power_get(guids::BALANCED, guids::SUB_PROCESSOR, guids::PROC_MAX, true), Some(80));
        e.undo("cpu");
        assert_eq!(e.be.power_get(guids::BALANCED, guids::SUB_PROCESSOR, guids::PROC_MAX, true), Some(100));
    }

    #[test]
    fn the_log_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("plume-boost-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let fake = Arc::new(Fake::open(dir.join("pc")));
        {
            let mut e = Engine::new(fake.clone(), Some(dir.clone()));
            e.apply(&["transparency_off", "startup_delay_off"]);
            assert_eq!(e.log.entries.len(), 2);
        }
        let fake2 = Arc::new(Fake::open(dir.join("pc")));
        let mut e = Engine::new(fake2.clone(), Some(dir.clone()));
        assert_eq!(e.log.changed(), vec!["startup_delay_off".to_string(), "transparency_off".to_string()]);
        assert!(e.undo_all().is_empty());
        assert_eq!(fake2.reg_get(Hive::User, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency"), Some(Val::Dword(1)));
        assert_eq!(
            fake2.reg_get(Hive::User, r"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize", "StartupDelayInMSec"),
            None,
            "a value that didn't exist is deleted again"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_point_needs_admin() {
        let (mut e, fake) = engine(false);
        assert_eq!(e.restore_point(), Err(Fail::NeedsAdmin));
        fake.set_admin(true);
        assert!(e.restore_point().is_ok());
        assert_eq!(fake.snapshot().restore_points, vec!["Plume Boost".to_string()]);
        assert_eq!(e.log.restore_points.len(), 1);
    }

    #[test]
    fn unavailable_tweaks_are_not_applied() {
        let state = State { admin: true, battery: false, ..State::default() };
        let fake = Arc::new(Fake::memory(state));
        let mut e = Engine::new(fake.clone(), None);
        let t = by_id("battery_boost_efficient").unwrap();
        assert_eq!(e.status(t), Status::Unavailable);
        assert_eq!(e.apply(&[t.id]).failed, vec![(t.id, Fail::Missing)]);
        assert!(fake.snapshot().writes.is_empty());
    }
    #[test]
    fn graphics_cards_are_clamped_logged_tried_and_undone() {
        use crate::backend::fake::fake_gpu;
        use crate::gpu::{Fan, Knob};
        let state = State { gpus: ["nvidia", "amd", "arc", "iris"].iter().map(|k| fake_gpu(k).unwrap()).collect(), ..State::default() };
        let fake = Arc::new(Fake::memory(state));
        let mut e = Engine::new(fake.clone(), None);
        let start = fake.snapshot().gpus;
        // NVIDIA: far outside the ranges -> the driver's limits
        let mut t = e.be.gpu_get("nvidia-1-rtx4070").unwrap();
        t.knobs.insert(Knob::CoreOffset, 5000.0);
        t.knobs.insert(Knob::Power, 10.0);
        t.fan = Fan::Fixed(10);
        let (prev, logged) = e.set_gpu("nvidia-1-rtx4070", &t).unwrap();
        assert!(logged && prev == start[0].now);
        let now = e.be.gpu_get("nvidia-1-rtx4070").unwrap();
        assert_eq!((now.get(Knob::CoreOffset), now.get(Knob::Power), now.fan.clone()), (Some(1000.0), Some(100.0), Fan::Fixed(30)));
        // a second try is not logged again; reverting it gives back the first try's values
        let mut t2 = now.clone();
        t2.knobs.insert(Knob::CoreOffset, 100.0);
        let (prev2, logged2) = e.set_gpu("nvidia-1-rtx4070", &t2).unwrap();
        assert!(!logged2);
        e.revert_gpu("nvidia-1-rtx4070", &prev2, logged2).unwrap();
        assert_eq!(e.be.gpu_get("nvidia-1-rtx4070").unwrap(), now);
        assert_eq!(e.log.entries.len(), 1);
        // AMD: a fixed speed becomes a flat curve; Intel Arc: refused until the waiver is accepted
        let mut a = e.be.gpu_get("amd-3-rx7800xt").unwrap();
        a.knobs.insert(Knob::Voltage, 1050.0);
        a.fan = Fan::Fixed(55);
        e.set_gpu("amd-3-rx7800xt", &a).unwrap();
        assert!(matches!(e.be.gpu_get("amd-3-rx7800xt").unwrap().fan, Fan::Curve(ref p) if p.iter().all(|x| x.1 == 55)));
        let mut i = e.be.gpu_get("intel-3-arca770").unwrap();
        i.knobs.insert(Knob::CoreOffset, 100.0);
        assert_eq!(e.set_gpu("intel-3-arca770", &i), Err(Fail::Other(NO_WAIVER.into())));
        e.waiver_ok = true;
        e.set_gpu("intel-3-arca770", &i).unwrap();
        // integrated graphics: nothing to change
        assert!(e.set_gpu("intel-0-irisxe", &crate::gpu::Tuning::default()).is_err());
        // undo everything: every card as it was
        assert!(e.undo_all().is_empty());
        let end = fake.snapshot().gpus;
        for (a, b) in end.iter().zip(&start) {
            assert_eq!(a.now, b.now, "{} back", a.card.id);
        }
    }

    #[test]
    fn processor_visibility_and_the_signin_value_are_undone() {
        let (mut e, fake) = engine(true);
        let start = fake.snapshot();
        e.set_attrib(guids::EPP, 0).unwrap();
        e.set_power(guids::EPP, true, 60).unwrap();
        e.set_power(guids::PARK_MIN, true, 100).unwrap();
        e.set_reg("gpu_signin", Hive::User, crate::backend::RUN_KEY, crate::backend::RUN_NAME, Some(Val::Sz("x --apply-gpu-profile".into()))).unwrap();
        assert!(e.undo_needs_admin(), "showing a hidden setting is machine-wide");
        assert!(e.undo_all().is_empty());
        let end = fake.snapshot();
        assert_eq!((end.attribs, end.power, end.reg), (start.attribs, start.power, start.reg));
    }
}
