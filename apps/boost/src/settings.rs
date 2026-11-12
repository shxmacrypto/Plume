//! Plume Boost's settings, in %LOCALAPPDATA%\Plume\Boost\settings.json, next to the undo log (undo.json).
//! PLUME_BOOST_DIR replaces the folder; with the imaginary PC of the tests, both files live in its folder instead, so
//! a test never mixes with the real log.

use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::OnceLock};

static DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init_dir(test_dir: Option<PathBuf>) {
    let d = if let Some(d) = std::env::var_os("PLUME_BOOST_DIR") {
        PathBuf::from(d)
    } else if let Some(t) = test_dir {
        t
    } else {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Boost")
    };
    let _ = DIR.set(d);
}

pub fn data_dir() -> PathBuf {
    DIR.get().cloned().unwrap_or_else(|| std::env::temp_dir().join("plume-boost"))
}

/// Writes a JSON file through a temporary file renamed over the old one (never half-written).
pub fn save_json<T: Serialize>(file: &std::path::Path, v: &T) {
    if let Some(d) = file.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = file.with_extension("tmp");
    if let Ok(b) = serde_json::to_vec_pretty(v)
        && std::fs::write(&tmp, b).is_ok()
    {
        let _ = std::fs::rename(&tmp, file);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// "overview", "tweaks", "cpu", "changes", "settings".
    pub page: String,
    /// "gaming", "balanced", "battery", or "" when the ticks were changed by hand.
    pub profile: String,
    /// The ticked tweaks (kept across a restart as administrator).
    pub ticked: Vec<String>,
    /// Offer a System Restore point before applying (the switch of the Apply dialog starts on).
    pub restore_point: bool,
    /// Keep a copy of each snapshot with Plume Backup.
    pub backup: bool,
    /// The graphics card shown on the Graphics page (its id).
    pub gpu_card: String,
    /// The always-on-top monitor is open.
    pub monitor: bool,
    /// Applying the graphics profile at sign-in is paused (after a crash or a driver reset blamed on it).
    pub gpu_signin_blocked: bool,
    /// Why it was paused, shown once on the Graphics page.
    pub gpu_notice: String,
    /// The user accepted Intel's overclocking waiver.
    pub intel_waiver: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            lang: "system".into(),
            page: "overview".into(),
            profile: String::new(),
            ticked: Vec::new(),
            restore_point: true,
            backup: true,
            gpu_card: String::new(),
            monitor: false,
            gpu_signin_blocked: false,
            gpu_notice: String::new(),
            intel_waiver: false,
        }
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
        let s: Settings = serde_json::from_str(r#"{"profile":"gaming","ticked":["hags"]}"#).unwrap();
        assert_eq!((s.profile.as_str(), s.ticked.len(), s.restore_point, s.theme.as_str()), ("gaming", 1, true, "system"));
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
