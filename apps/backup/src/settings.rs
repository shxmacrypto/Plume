//! What Plume Backup keeps between runs, in %LOCALAPPDATA%\Plume\Backup (PLUME_BACKUP_DIR for tests):
//! settings.json (the app's settings) and plans.json (the plans and how their last run went).

use crate::{
    engine::{Fail, Plan, Report},
    sys::data_dir,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "en", "fr".
    pub lang: String,
    /// "system", "dark", "light".
    pub theme: String,
    /// Closing the window leaves Plume Backup running near the clock, so the schedules keep working.
    pub background: bool,
    pub notify_ok: bool,
    pub notify_fail: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { lang: "system".into(), theme: "system".into(), background: true, notify_ok: true, notify_fail: true }
    }
}

fn save_json<T: Serialize>(name: &str, v: &T) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(name);
    let tmp = file.with_extension("tmp");
    if let Ok(b) = serde_json::to_vec_pretty(v)
        && std::fs::write(&tmp, b).is_ok()
    {
        let _ = std::fs::rename(&tmp, file);
    }
}

fn load_json<T: for<'a> Deserialize<'a> + Default>(name: &str) -> T {
    std::fs::read(data_dir().join(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

impl Settings {
    pub fn load() -> Settings {
        load_json("settings.json")
    }
    pub fn save(&self) {
        save_json("settings.json", self);
    }
}

/// How the last run of a plan went.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Last {
    pub time: u64,
    pub ok: bool,
    pub report: Report,
    /// Why it failed (see `fail_code`), empty when it worked.
    pub fail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlanState {
    /// Unix time of the last run that worked.
    pub last_ok: u64,
    /// Unix time of the last attempt.
    pub last_try: u64,
    pub last: Option<Last>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Store {
    pub plans: Vec<Plan>,
    pub states: BTreeMap<u64, PlanState>,
}

impl Store {
    pub fn load() -> Store {
        load_json("plans.json")
    }
    pub fn save(&self) {
        save_json("plans.json", self);
    }
    pub fn state(&self, id: u64) -> PlanState {
        self.states.get(&id).cloned().unwrap_or_default()
    }
    pub fn plan(&self, id: u64) -> Option<&Plan> {
        self.plans.iter().find(|p| p.id == id)
    }
}

/// A failure, as kept in plans.json.
pub fn fail_code(f: &Fail) -> String {
    match f {
        Fail::Stopped => "stopped".into(),
        Fail::NoSources => "no_sources".into(),
        Fail::NoDest => "no_dest".into(),
        Fail::DestMissing => "dest_missing".into(),
        Fail::SourceMissing(p) => format!("source_missing:{}", p.display()),
        Fail::Overlap => "overlap".into(),
        Fail::DestFull => "dest_full".into(),
        Fail::Io(e) => format!("io:{e}"),
    }
}

pub fn fail_from(code: &str) -> Fail {
    match code.split_once(':') {
        Some(("source_missing", p)) => Fail::SourceMissing(PathBuf::from(p)),
        Some(("io", e)) => Fail::Io(e.to_string()),
        _ => match code {
            "stopped" => Fail::Stopped,
            "no_sources" => Fail::NoSources,
            "no_dest" => Fail::NoDest,
            "dest_missing" => Fail::DestMissing,
            "overlap" => Fail::Overlap,
            "dest_full" => Fail::DestFull,
            other => Fail::Io(other.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_round_trip() {
        for f in [Fail::Stopped, Fail::DestMissing, Fail::SourceMissing(PathBuf::from(r"C:\a:b")), Fail::Io("x: y".into()), Fail::DestFull, Fail::Overlap] {
            assert_eq!(fail_from(&fail_code(&f)), f);
        }
    }

    #[test]
    fn plans_survive_a_round_trip() {
        let mut s = Store::default();
        s.plans.push(Plan { id: 7, name: "Docs".into(), sources: vec![PathBuf::from(r"C:\x")], ..Default::default() });
        s.states.insert(7, PlanState { last_ok: 5, last_try: 6, last: Some(Last { time: 6, ok: true, ..Default::default() }) });
        let back: Store = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        // a file from an older version, with missing fields, still loads
        let old: Store = serde_json::from_str(r#"{"plans":[{"id":1,"name":"A"}]}"#).unwrap();
        assert_eq!(old.plans[0].keep_versions, 30);
    }
}
