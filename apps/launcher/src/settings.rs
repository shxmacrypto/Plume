//! Plume Launcher's settings, in %LOCALAPPDATA%\Plume\Launcher\settings.json (PLUME_LAUNCHER_DIR in tests).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The launcher's data folder: settings, history, the file index.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_LAUNCHER_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Launcher")
}

/// Web search engines: (id, name, address before the query).
pub const ENGINES: [(&str, &str, &str); 4] = [
    ("google", "Google", "https://www.google.com/search?q="),
    ("bing", "Bing", "https://www.bing.com/search?q="),
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q="),
    ("ecosia", "Ecosia", "https://www.ecosia.org/search?q="),
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "Alt+Space", "Ctrl+Space"…
    pub hotkey: String,
    pub apps: bool,
    pub files: bool,
    pub settings: bool,
    pub calc: bool,
    pub web: bool,
    pub engine: String,
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkey: "Alt+Space".into(),
            apps: true,
            files: true,
            settings: true,
            calc: true,
            web: true,
            engine: "google".into(),
            theme: "system".into(),
            lang: "system".into(),
        }
    }
}

impl Settings {
    fn file() -> PathBuf {
        data_dir().join("settings.json")
    }
    /// The saved settings, and whether there were any (false: first start).
    pub fn load() -> (Settings, bool) {
        match std::fs::read(Self::file()) {
            Ok(b) => (serde_json::from_slice(&b).unwrap_or_default(), true),
            Err(_) => (Settings::default(), false),
        }
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
    /// The web address that searches `q` with the chosen engine.
    pub fn search_url(&self, q: &str) -> String {
        let base = ENGINES.iter().find(|e| e.0 == self.engine).unwrap_or(&ENGINES[0]).2;
        format!("{base}{}", encode(q))
    }
    pub fn engine_name(&self) -> &'static str {
        ENGINES.iter().find(|e| e.0 == self.engine).unwrap_or(&ENGINES[0]).1
    }
}

/// Percent-encoding of a query (spaces as "+").
pub fn encode(q: &str) -> String {
    let mut out = String::new();
    for b in q.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_address() {
        let mut s = Settings::default();
        assert_eq!(s.search_url("café & co"), "https://www.google.com/search?q=caf%C3%A9+%26+co");
        s.engine = "duckduckgo".into();
        assert_eq!(s.search_url(" rust "), "https://duckduckgo.com/?q=rust");
        s.engine = "unknown".into();
        assert!(s.search_url("x").starts_with("https://www.google.com/"));
    }

    #[test]
    fn old_files_keep_working() {
        // a settings file from an older version (missing keys) takes the defaults for them
        let s: Settings = serde_json::from_str(r#"{"hotkey":"Ctrl+Space","files":false}"#).unwrap();
        assert_eq!(s.hotkey, "Ctrl+Space");
        assert!(!s.files && s.apps && s.web);
    }
}
