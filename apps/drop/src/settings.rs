//! Plume Drop's settings and history, in %LOCALAPPDATA%\Plume\Drop (PLUME_DROP_DIR in tests).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_DROP_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Drop")
}

/// Windows' Downloads folder (moved or not).
pub fn downloads() -> PathBuf {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    };
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None) {
            let path = PathBuf::from(String::from_utf16_lossy(p.as_wide()));
            CoTaskMemFree(Some(p.0 as _));
            return path;
        }
    }
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join("Downloads")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Where received files go.
    pub folder: PathBuf,
    /// Received text goes to the clipboard.
    pub copy_text: bool,
    /// Received links open in the browser.
    pub open_links: bool,
    /// "system", "en", "fr".
    pub lang: String,
    /// The code in the QR code. It stays the same between launches, so a phone that scanned it once can come back;
    /// "New code" changes it.
    pub token: String,
    pub port: u16,
}

pub const PORT: u16 = 53917;

impl Default for Settings {
    fn default() -> Self {
        Settings { folder: downloads().join("Plume Drop"), copy_text: true, open_links: false, lang: "system".into(), token: String::new(), port: PORT }
    }
}

/// Writes through a temporary file: a crash never leaves half a file.
pub fn write_json<T: Serialize>(path: &Path, v: &T) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if let Ok(b) = serde_json::to_vec_pretty(v)
        && std::fs::write(&tmp, b).is_ok()
    {
        let _ = std::fs::rename(&tmp, path);
    }
}

pub fn read_json<T: for<'a> Deserialize<'a> + Default>(path: &Path) -> T {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

impl Settings {
    fn file() -> PathBuf {
        data_dir().join("settings.json")
    }
    pub fn load() -> Settings {
        let mut s: Settings = read_json(&Self::file());
        if s.token.len() < 16 {
            s.token = crate::util::new_token();
            s.save();
        }
        if s.port == 0 {
            s.port = PORT;
        }
        s
    }
    pub fn save(&self) {
        write_json(&Self::file(), self);
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Dir {
    /// From a phone to this PC.
    In,
    /// From this PC to the phones.
    Out,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Kind {
    File,
    Text,
    Link,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum State {
    Running,
    Done,
    Failed,
    /// Offered to the phones (sent items, while Plume Drop runs).
    Shared,
}

/// One line of the history.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub dir: Dir,
    pub kind: Kind,
    /// File name, or the text itself.
    pub name: String,
    pub path: Option<PathBuf>,
    pub size: u64,
    #[serde(skip)]
    pub done: u64,
    pub state: State,
    /// The phone it came from, or the last one that saved it.
    pub device: String,
    pub at: u64,
    /// Sent items: how many times a phone saved it.
    #[serde(default)]
    pub saves: u32,
}

pub const HISTORY_MAX: usize = 300;

pub fn history_file() -> PathBuf {
    data_dir().join("history.json")
}

pub fn load_history() -> Vec<Entry> {
    let mut h: Vec<Entry> = read_json(&history_file());
    for e in &mut h {
        match e.state {
            // the app stopped during the transfer
            State::Running => e.state = State::Failed,
            // offers end with the app
            State::Shared => e.state = State::Done,
            _ => {}
        }
    }
    h
}

pub fn save_history(h: &[Entry]) {
    let keep: Vec<&Entry> = h.iter().filter(|e| e.state != State::Running).take(HISTORY_MAX).collect();
    write_json(&history_file(), &keep);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_fill_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"copy_text":false}"#).unwrap();
        assert!(!s.copy_text);
        assert_eq!(s.port, PORT);
        assert!(s.folder.ends_with("Plume Drop"));
    }

    #[test]
    fn history_round_trip() {
        let e = Entry {
            id: 7,
            dir: Dir::In,
            kind: Kind::File,
            name: "a.jpg".into(),
            path: Some("C:\\x\\a.jpg".into()),
            size: 10,
            done: 10,
            state: State::Shared,
            device: "Pixel".into(),
            at: 1,
            saves: 2,
        };
        let text = serde_json::to_string(&vec![e.clone()]).unwrap();
        let back: Vec<Entry> = serde_json::from_str(&text).unwrap();
        assert_eq!(back[0].name, "a.jpg");
        assert_eq!(back[0].done, 0);
        assert_eq!(back[0].saves, 2);
    }
}
