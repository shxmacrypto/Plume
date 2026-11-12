//! Plume Cut's settings, in %LOCALAPPDATA%\Plume\Cut\settings.json (PLUME_CUT_DIR moves the folder, for tests).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_CUT_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(r"Plume\Cut")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// Use the graphics card's encoder when there is one.
    pub hw: bool,
    /// Where exports go; None: Videos\Plume Cut.
    pub out_dir: Option<PathBuf>,
    /// Projects opened lately, newest first.
    pub recent: Vec<PathBuf>,
    pub snap: bool,
    /// The export choices last used.
    pub export_res: u32,
    pub export_aspect: String,
    pub export_codec: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            lang: "system".into(),
            hw: true,
            out_dir: None,
            recent: Vec::new(),
            snap: true,
            export_res: 1080,
            export_aspect: "project".into(),
            export_codec: "h264".into(),
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
    /// A project goes to the top of the recent list (12 kept).
    pub fn opened(&mut self, file: &std::path::Path) {
        self.recent.retain(|p| !p.as_os_str().eq_ignore_ascii_case(file.as_os_str()));
        self.recent.insert(0, file.to_path_buf());
        self.recent.truncate(12);
        self.save();
    }
    pub fn out_dir(&self) -> PathBuf {
        match &self.out_dir {
            Some(d) if d.is_dir() => d.clone(),
            _ => {
                if let Some(d) = std::env::var_os("PLUME_CUT_OUT") {
                    return PathBuf::from(d);
                }
                let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
                home.join("Videos").join("Plume Cut")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_files_load() {
        let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(s.theme, "dark");
        assert!(s.hw && s.snap);
        assert_eq!(s.export_res, 1080);
    }
}
