//! Plume Task's settings, in %LOCALAPPDATA%\Plume\Task\settings.json (PLUME_TASK_DIR replaces the folder in tests).

use crate::model::Col;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_TASK_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Task")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// Milliseconds between two readings: 500, 1000, 2000, or 0 (paused).
    pub interval: u32,
    pub sort: Col,
    pub desc: bool,
    /// "apps", "perf", "startup", "settings".
    pub page: String,
    /// Background processes listed apart from apps with a window.
    pub sections: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: "system".into(), lang: "system".into(), interval: 1000, sort: Col::Cpu, desc: true, page: "apps".into(), sections: true }
    }
}

impl Settings {
    fn file() -> PathBuf {
        data_dir().join("settings.json")
    }
    pub fn load() -> Settings {
        std::fs::read(Self::file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(data_dir());
        let tmp = Self::file().with_extension("tmp");
        if let Ok(b) = serde_json::to_vec_pretty(self)
            && std::fs::write(&tmp, b).is_ok()
        {
            let _ = std::fs::rename(&tmp, Self::file());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_or_partial_files_still_load() {
        let s: Settings = serde_json::from_str(r#"{"interval":2000,"sort":"Mem"}"#).unwrap();
        assert_eq!((s.interval, s.sort, s.theme.as_str()), (2000, Col::Mem, "system"));
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
