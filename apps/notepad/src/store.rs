//! What Plume Notepad keeps on disk, in %LOCALAPPDATA%\Plume\Notepad (PLUME_NOTEPAD_DIR in tests):
//! settings.json, session.json (the open tabs), tabs\<id>.txt (the text of tabs not saved yet, written a moment
//! after each change, so nothing is lost on close or crash) and pads.json (the sticky pads).

use crate::codec::{Encoding, Eol};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_NOTEPAD_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Notepad")
}

/// Writes a file whole or not at all (a temporary file renamed over it).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> bool {
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_ok()
}

fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save<T: Serialize>(path: &Path, v: &T) -> bool {
    serde_json::to_vec_pretty(v).is_ok_and(|b| write_atomic(path, &b))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    pub wrap: bool,
    pub numbers: bool,
    /// Text size, 1.0 = 14 px.
    pub zoom: f32,
    /// "mono" or "sans".
    pub font: String,
    pub status: bool,
    /// The window's place: x, y, w, h, maximized.
    pub placement: Option<(i32, i32, i32, i32, bool)>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: "system".into(), lang: "system".into(), wrap: true, numbers: false, zoom: 1.0, font: "mono".into(), status: true, placement: None }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Settings {
        let mut s: Settings = load(&dir.join("settings.json"));
        s.zoom = s.zoom.clamp(0.5, 4.0);
        s
    }
    pub fn save(&self, dir: &Path) -> bool {
        save(&dir.join("settings.json"), self)
    }
}

/// A tab as it is remembered.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TabInfo {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub enc: Encoding,
    pub eol: Eol,
    pub caret: (usize, usize),
    pub top: (usize, usize),
    /// The text is in tabs\<id>.txt, not (or not only) in the file.
    pub unsaved: bool,
    /// "Untitled 2".
    pub number: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Session {
    pub tabs: Vec<TabInfo>,
    pub active: usize,
}

impl Session {
    pub fn load(dir: &Path) -> Session {
        load(&dir.join("session.json"))
    }
    pub fn save(&self, dir: &Path) -> bool {
        save(&dir.join("session.json"), self)
    }
}

pub fn backup_path(dir: &Path, id: u64) -> PathBuf {
    dir.join("tabs").join(format!("{id}.txt"))
}

pub fn write_backup(dir: &Path, id: u64, text: &str) -> bool {
    write_atomic(&backup_path(dir, id), text.as_bytes())
}

pub fn read_backup(dir: &Path, id: u64) -> Option<String> {
    std::fs::read(backup_path(dir, id)).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub fn drop_backup(dir: &Path, id: u64) {
    let _ = std::fs::remove_file(backup_path(dir, id));
}

/// Backups no tab refers to any more (left by a crash between two writes).
pub fn sweep_backups(dir: &Path, keep: &[u64]) {
    let Ok(list) = std::fs::read_dir(dir.join("tabs")) else { return };
    for e in list.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let id = name.strip_suffix(".txt").and_then(|n| n.parse::<u64>().ok());
        if id.is_some_and(|id| !keep.contains(&id)) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// A sticky pad.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Pad {
    pub id: u64,
    pub text: String,
    /// Index in the pad colours.
    pub color: usize,
    /// Place on screen, in pixels.
    pub rect: (i32, i32, i32, i32),
    /// Stays over the other windows.
    pub on_top: bool,
    /// Shown (closed pads stay listed near the clock).
    pub open: bool,
}

impl Pad {
    /// The first line, for the list near the clock.
    pub fn title(&self) -> String {
        let first = self.text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
        let mut t: String = first.chars().take(40).collect();
        if first.chars().count() > 40 {
            t.push('…');
        }
        t
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Pads {
    pub list: Vec<Pad>,
    pub next_id: u64,
}

impl Pads {
    pub fn load(dir: &Path) -> Pads {
        let mut p: Pads = load(&dir.join("pads.json"));
        p.next_id = p.next_id.max(p.list.iter().map(|p| p.id + 1).max().unwrap_or(1));
        p
    }
    pub fn save(&self, dir: &Path) -> bool {
        save(&dir.join("pads.json"), self)
    }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Pad> {
        self.list.iter_mut().find(|p| p.id == id)
    }
    pub fn get(&self, id: u64) -> Option<&Pad> {
        self.list.iter().find(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("plume-notepad-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn session_and_backups_round_trip() {
        let d = tmp("session");
        let s = Session {
            tabs: vec![TabInfo {
                id: 3,
                path: Some("C:\\x.txt".into()),
                enc: Encoding::Utf16Le,
                eol: Eol::Lf,
                caret: (2, 4),
                unsaved: true,
                ..Default::default()
            }],
            active: 0,
        };
        assert!(s.save(&d));
        assert_eq!(Session::load(&d), s);
        assert!(write_backup(&d, 3, "unsaved\ntext"));
        assert!(write_backup(&d, 9, "left over"));
        assert_eq!(read_backup(&d, 3).as_deref(), Some("unsaved\ntext"));
        sweep_backups(&d, &[3]);
        assert!(read_backup(&d, 9).is_none());
        assert!(read_backup(&d, 3).is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn settings_defaults_and_old_files() {
        let d = tmp("settings");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("settings.json"), r#"{"wrap":false,"zoom":99}"#).unwrap();
        let s = Settings::load(&d);
        assert!(!s.wrap && s.status && s.zoom == 4.0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn pads_ids_and_titles() {
        let d = tmp("pads");
        let mut p = Pads::default();
        p.list.push(Pad { id: 7, text: "\n  Groceries \nmilk".into(), ..Default::default() });
        assert!(p.save(&d));
        let p = Pads::load(&d);
        assert_eq!(p.next_id, 8);
        assert_eq!(p.list[0].title(), "Groceries");
        let _ = std::fs::remove_dir_all(&d);
    }
}
