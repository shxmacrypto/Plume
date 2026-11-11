//! Plume Media's settings, in %LOCALAPPDATA%\Plume\Media\settings.json (PLUME_MEDIA_DIR moves the whole data
//! folder: tests use it so they never touch the real one).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use windows::Win32::{
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_Pictures, FOLDERID_Videos, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
};

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_MEDIA_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Media")
}

fn known(id: &windows::core::GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

/// The folders a new library starts with: Pictures and Videos. None when the data folder is moved (tests), so a
/// test never reads the real ones.
pub fn default_folders() -> Vec<PathBuf> {
    if let Ok(list) = std::env::var("PLUME_MEDIA_LIBRARY") {
        return list.split(';').filter(|s| !s.trim().is_empty()).map(PathBuf::from).collect();
    }
    if std::env::var_os("PLUME_MEDIA_DIR").is_some() {
        return vec![];
    }
    [known(&FOLDERID_Pictures), known(&FOLDERID_Videos)].into_iter().flatten().collect()
}

pub const SPEEDS: [u32; 5] = [3, 5, 8, 12, 20];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// Library folders (searched with their subfolders).
    pub folders: Vec<PathBuf>,
    /// How a photo opens: "fit" (whole, never enlarged), "fill" (whole, enlarged to the window), "actual" (100 %).
    pub zoom: String,
    /// Seconds per photo in a slideshow.
    pub slideshow: u32,
    /// 0 small, 1 medium, 2 large thumbnails.
    pub tiles: u8,
    pub volume: f32,
    pub muted: bool,
    pub info_open: bool,
    /// Subtitles found next to a video are shown.
    pub subtitles: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            lang: "system".into(),
            folders: default_folders(),
            zoom: "fit".into(),
            slideshow: 5,
            tiles: 1,
            volume: 0.8,
            muted: false,
            info_open: false,
            subtitles: true,
        }
    }
}

impl Settings {
    fn file() -> PathBuf {
        data_dir().join("settings.json")
    }
    pub fn load() -> Settings {
        let mut s: Settings = std::fs::read(Self::file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if let Ok(list) = std::env::var("PLUME_MEDIA_LIBRARY") {
            s.folders = list.split(';').filter(|x| !x.trim().is_empty()).map(PathBuf::from).collect();
        }
        if !SPEEDS.contains(&s.slideshow) {
            s.slideshow = 5;
        }
        s.tiles = s.tiles.min(2);
        s.volume = s.volume.clamp(0.0, 1.0);
        s
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

/// Sound is never heard from a test build, nor when PLUME_MEDIA_MUTE is set.
pub fn sound_allowed() -> bool {
    !cfg!(feature = "test-hook") && std::env::var_os("PLUME_MEDIA_MUTE").is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_fields_and_defaults() {
        let s: Settings = serde_json::from_str(r#"{"theme":"dark","whatever":1}"#).unwrap();
        assert_eq!(s.theme, "dark");
        assert_eq!(s.slideshow, 5);
        assert!(!sound_allowed() || std::env::var_os("PLUME_MEDIA_MUTE").is_none());
    }
}
