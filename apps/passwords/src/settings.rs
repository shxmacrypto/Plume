//! Plume Passwords' settings (nothing secret), in %LOCALAPPDATA%\Plume\Passwords\settings.json, next to the vault.
//! PLUME_PASSWORDS_DIR puts all of it elsewhere (tests).

use crate::generator::GenOptions;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_PASSWORDS_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Passwords")
}

pub fn vault_path() -> PathBuf {
    data_dir().join("vault.ppw")
}

pub fn hello_path() -> PathBuf {
    data_dir().join("hello.bin")
}

/// Auto-lock choices, in minutes (0: never).
pub const AUTOLOCK: [u32; 6] = [1, 5, 10, 30, 60, 0];
/// Clipboard clearing choices, in seconds.
pub const CLEAR: [u32; 4] = [15, 30, 60, 120];
/// Auto-type shortcuts: label, modifiers (MOD_ALT 1, MOD_CONTROL 2, MOD_SHIFT 4), key.
pub const HOTKEYS: [(&str, u32, u32); 3] = [("Ctrl+Alt+A", 3, b'A' as u32), ("Ctrl+Shift+L", 6, b'L' as u32), ("Ctrl+Alt+P", 3, b'P' as u32)];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    pub autolock_min: u32,
    pub lock_on_windows_lock: bool,
    pub clear_clipboard_s: u32,
    pub autotype: bool,
    /// Index in HOTKEYS.
    pub hotkey: usize,
    /// Press Enter after the password.
    pub autotype_enter: bool,
    /// Closing the window keeps Plume Passwords near the clock (auto-type keeps working).
    pub keep_running: bool,
    pub generator: GenOptions,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            lang: "system".into(),
            autolock_min: 10,
            lock_on_windows_lock: true,
            clear_clipboard_s: 30,
            autotype: true,
            hotkey: 0,
            autotype_enter: false,
            keep_running: true,
            generator: GenOptions::default(),
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
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let _ = crate::vault::write_atomic(&Self::file(), &b);
        }
    }
    pub fn hotkey(&self) -> (&'static str, u32, u32) {
        HOTKEYS[self.hotkey.min(HOTKEYS.len() - 1)]
    }
}
