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
    /// Folders pinned to the sidebar, in their order.
    pub pins: Vec<Pin>,
}

/// A folder pinned to the sidebar, and how its timeline shows.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Pin {
    pub path: PathBuf,
    /// Its own name in the sidebar (empty: the folder's name).
    pub name: String,
    /// "taken" (date taken) or "modified" (the file's date).
    pub sort: String,
    /// "all", "photos", "videos", "favorites".
    pub show: String,
    /// The pictures of its subfolders too.
    pub subfolders: bool,
}

impl Default for Pin {
    fn default() -> Self {
        Pin { path: PathBuf::new(), name: String::new(), sort: "taken".into(), show: "all".into(), subfolders: true }
    }
}

impl Pin {
    pub fn new(path: PathBuf) -> Pin {
        Pin { path, ..Default::default() }
    }
    /// What the sidebar shows.
    pub fn label(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.trim().to_string();
        }
        self.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| self.path.display().to_string())
    }
}

/// The same folder (case and a final separator aside).
pub fn same(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.to_string_lossy().trim_end_matches(['\\', '/']).eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['\\', '/']))
}

/// Where a folder is among the pins.
pub fn pin_index(pins: &[Pin], path: &std::path::Path) -> Option<usize> {
    pins.iter().position(|p| same(&p.path, path))
}

/// Pins a folder at the end; false when it already is.
pub fn add_pin(pins: &mut Vec<Pin>, path: PathBuf) -> bool {
    if pin_index(pins, &path).is_some() {
        return false;
    }
    pins.push(Pin::new(path));
    true
}

/// Moves the pin at `from` so it ends at `to` (dragged in the sidebar).
pub fn move_pin(pins: &mut [Pin], from: usize, to: usize) {
    if from >= pins.len() {
        return;
    }
    let to = to.min(pins.len() - 1);
    if from < to {
        pins[from..=to].rotate_left(1);
    } else {
        pins[to..=from].rotate_right(1);
    }
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
            pins: vec![],
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
    fn pins_saved_reordered_restored() {
        let mut s = Settings::default();
        assert!(add_pin(&mut s.pins, PathBuf::from(r"C:\Photos\Trips")));
        assert!(add_pin(&mut s.pins, PathBuf::from(r"D:\Family")));
        assert!(add_pin(&mut s.pins, PathBuf::from(r"E:\Work\Shoots")));
        // the same folder twice (case, trailing slash): once
        assert!(!add_pin(&mut s.pins, PathBuf::from(r"c:\photos\trips\")));
        s.pins[1].name = "Home".into();
        s.pins[2].sort = "modified".into();
        s.pins[2].show = "videos".into();
        s.pins[2].subfolders = false;
        move_pin(&mut s.pins, 2, 0);
        let order: Vec<String> = s.pins.iter().map(Pin::label).collect();
        assert_eq!(order, ["Shoots", "Trips", "Home"]);
        move_pin(&mut s.pins, 0, 2);
        assert_eq!(s.pins.iter().map(Pin::label).collect::<Vec<_>>(), ["Trips", "Home", "Shoots"]);
        move_pin(&mut s.pins, 9, 0);
        // written and read back as the settings file is
        let back: Settings = serde_json::from_slice(&serde_json::to_vec_pretty(&s).unwrap()).unwrap();
        assert_eq!(back.pins, s.pins);
        assert_eq!(back.pins[2].show, "videos");
        assert!(!back.pins[2].subfolders);
        assert_eq!(pin_index(&back.pins, std::path::Path::new(r"D:\FAMILY")), Some(1));
        // a settings file from before pins: none
        let old: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert!(old.pins.is_empty());
    }

    #[test]
    fn unknown_fields_and_defaults() {
        let s: Settings = serde_json::from_str(r#"{"theme":"dark","whatever":1}"#).unwrap();
        assert_eq!(s.theme, "dark");
        assert_eq!(s.slideshow, 5);
        assert!(!sound_allowed() || std::env::var_os("PLUME_MEDIA_MUTE").is_none());
    }
}
