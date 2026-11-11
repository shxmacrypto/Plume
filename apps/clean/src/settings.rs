//! Plume Clean's settings, in %LOCALAPPDATA%\Plume\Clean\settings.json (PLUME_CLEAN_DIR replaces the folder).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_CLEAN_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Clean")
}

pub const OLD_DAYS: [u32; 4] = [30, 90, 180, 365];
pub const LARGE_MB: [u64; 4] = [100, 500, 1000, 4000];
pub const DUP_MB: [u64; 4] = [1, 10, 100, 1000];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// Removed files go to the Recycle Bin.
    pub recycle: bool,
    /// Downloads untouched for this many days are "old".
    pub old_days: u32,
    /// Files from this size (MB) are "large".
    pub large_mb: u64,
    /// Duplicates are looked for among files from this size (MB).
    pub dup_mb: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: "system".into(), lang: "system".into(), recycle: true, old_days: 90, large_mb: 500, dup_mb: 10 }
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
    fn defaults_and_partial_files() {
        let s: Settings = serde_json::from_str(r#"{"old_days":30}"#).unwrap();
        assert_eq!(s.old_days, 30);
        assert!(s.recycle);
        assert_eq!(s.large_mb, 500);
    }
}
