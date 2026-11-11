//! Plume Convert's settings, in %LOCALAPPDATA%\Plume\Convert\settings.json (PLUME_CONVERT_DIR moves the folder,
//! for tests).

use crate::opts::{Opts, Preset};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_CONVERT_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(r"Plume\Convert")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// Where results go; None: next to the original.
    pub out_dir: Option<PathBuf>,
    /// Keep the title, date, GPS… of videos and sounds.
    pub keep_meta: bool,
    /// Use the graphics card's encoder when there is one.
    pub hw: bool,
    /// "Convert with Plume" in Explorer's right-click menu.
    pub context_menu: bool,
    pub preset: Preset,
    pub opts: Opts,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            lang: "system".into(),
            out_dir: None,
            keep_meta: true,
            hw: true,
            context_menu: false,
            preset: Preset::Phone,
            opts: Opts::default(),
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
        let s: Settings = serde_json::from_str(r#"{"theme":"dark","opts":{"discord_mb":25}}"#).unwrap();
        assert_eq!(s.theme, "dark");
        assert_eq!(s.opts.discord_mb, 25);
        assert_eq!(s.opts.audio_fmt, "mp3");
        assert!(s.keep_meta && s.hw);
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
