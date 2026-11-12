//! Plume Create's settings, in %LOCALAPPDATA%\Plume\Create\settings.json (PLUME_CREATE_DIR moves the data folder:
//! tests use it so they never touch the real one).

use crate::wic::Format;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_CREATE_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Create")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// Projects and photos opened lately, newest first.
    pub recent: Vec<PathBuf>,
    pub export_format: Format,
    pub export_quality: u32,
    pub grid: bool,
    pub rulers: bool,
    pub snap: bool,
    /// Grid spacing in document pixels.
    pub grid_size: u32,
    /// Colours picked lately (hex), newest first.
    pub colours: Vec<String>,
    pub brush_size: f32,
    pub brush_hardness: f32,
    pub brush_opacity: f32,
    pub custom_w: u32,
    pub custom_h: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: "system".into(),
            lang: "system".into(),
            recent: vec![],
            export_format: Format::Png,
            export_quality: 90,
            grid: false,
            rulers: true,
            snap: true,
            grid_size: 50,
            colours: vec![],
            brush_size: 40.0,
            brush_hardness: 0.7,
            brush_opacity: 1.0,
            custom_w: 1600,
            custom_h: 1200,
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(data_dir().join("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let tmp = dir.join("settings.json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, dir.join("settings.json"));
            }
        }
    }
    pub fn add_recent(&mut self, p: PathBuf) {
        self.recent.retain(|r| r != &p);
        self.recent.insert(0, p);
        self.recent.truncate(8);
    }
    pub fn add_colour(&mut self, hex: String) {
        self.colours.retain(|c| c != &hex);
        self.colours.insert(0, hex);
        self.colours.truncate(12);
    }
}
