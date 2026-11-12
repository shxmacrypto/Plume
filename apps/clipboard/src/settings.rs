//! Plume Clipboard's settings, in settings.json next to the history. The start with Windows lives in the registry.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// The shortcut that opens the panel.
    pub hotkey: String,
    /// Items kept, pins aside.
    pub max_items: u32,
    /// Days kept (0: no limit), pins aside.
    pub days: u32,
    /// Largest picture kept, in MB.
    pub max_image_mb: u32,
    pub images: bool,
    pub files: bool,
    /// The history sealed by Windows for this user (DPAPI).
    pub encrypt: bool,
    /// Programs whose copies are never kept (exe names, lowercase).
    pub ignored: Vec<String>,
    /// Picking an item also pastes it (otherwise it is only put on the clipboard).
    pub paste: bool,
    /// Nothing new is kept for now.
    pub paused: bool,
    /// Sensitive copies shown masked and kept sealed: passwords, IP addresses, keys and tokens.
    pub hide_passwords: bool,
    pub hide_ips: bool,
    pub hide_keys: bool,
    /// How long a sensitive copy is kept, in minutes (unless pinned).
    pub secret_minutes: u32,
    /// Sensitive copies are not kept at all (like private content).
    pub never_keep_secrets: bool,
}

pub const MAX_ITEMS: [u32; 4] = [100, 500, 1000, 5000];
pub const DAYS: [u32; 5] = [1, 7, 30, 365, 0];
pub const IMAGE_MB: [u32; 4] = [2, 10, 25, 50];
pub const SECRET_MINUTES: [u32; 5] = [5, 15, 60, 480, 1440];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            lang: "system".into(),
            hotkey: "Ctrl+Shift+V".into(),
            max_items: 500,
            days: 30,
            max_image_mb: 10,
            images: true,
            files: true,
            encrypt: true,
            // they already mark their copies as private; listed in case an older version does not
            ignored: vec!["keepass.exe".into(), "keepassxc.exe".into(), "1password.exe".into(), "bitwarden.exe".into(), "plume-passwords.exe".into()],
            paste: true,
            paused: false,
            hide_passwords: true,
            hide_ips: true,
            hide_keys: true,
            secret_minutes: 60,
            never_keep_secrets: false,
        }
    }
}

/// An exe name as kept in the list: "KeePass" → "keepass.exe".
pub fn clean_app(s: &str) -> Option<String> {
    let name = s.trim().trim_matches('"').rsplit(['\\', '/']).next()?.trim().to_lowercase();
    if name.is_empty() || name.contains(['*', '?', '<', '>', '|', ':']) {
        return None;
    }
    Some(if name.ends_with(".exe") { name } else { format!("{name}.exe") })
}

impl Settings {
    pub fn load(dir: &Path) -> Settings {
        std::fs::read(dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::create_dir_all(dir);
        let tmp = dir.join("settings.json.tmp");
        if let Ok(b) = serde_json::to_vec_pretty(self)
            && std::fs::write(&tmp, b).is_ok()
        {
            let _ = std::fs::rename(&tmp, dir.join("settings.json"));
        }
    }

    /// The kinds of sensitive copies looked for.
    pub fn wanted(&self) -> crate::detect::Wanted {
        crate::detect::Wanted { passwords: self.hide_passwords, ips: self.hide_ips, keys: self.hide_keys }
    }

    pub fn secret_secs(&self) -> u64 {
        self.secret_minutes.max(1) as u64 * 60
    }

    pub fn ignores(&self, app: &str) -> bool {
        !app.is_empty() && self.ignored.iter().any(|a| a.eq_ignore_ascii_case(app))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apps() {
        assert_eq!(clean_app("KeePass").as_deref(), Some("keepass.exe"));
        assert_eq!(clean_app(r#""C:\Program Files\App\Thing.EXE""#).as_deref(), Some("thing.exe"));
        assert_eq!(clean_app("  "), None);
        assert_eq!(clean_app("a*b"), None);
        let s = Settings::default();
        assert!(s.ignores("KeePassXC.exe"));
        assert!(!s.ignores("notepad.exe"));
        assert!(!s.ignores(""));
    }

    #[test]
    fn old_files_get_defaults() {
        let s: Settings = serde_json::from_str(r#"{"max_items":100}"#).unwrap();
        assert_eq!(s.max_items, 100);
        assert_eq!(s.hotkey, "Ctrl+Shift+V");
        assert!(s.encrypt);
    }
}
