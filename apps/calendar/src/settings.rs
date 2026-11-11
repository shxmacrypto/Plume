//! Plume Calendar's settings, in the data folder (settings.json).

use crate::model::data_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use windows::{
    Win32::Globalization::{GetLocaleInfoEx, LOCALE_IFIRSTDAYOFWEEK, LOCALE_STIMEFORMAT},
    core::PCWSTR,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// "auto" (the language's habit, or Windows'), "mon", "sun", "sat".
    pub first_day: String,
    /// "auto", "24", "12".
    pub clock: String,
    /// Reminder given to new events, in minutes before (-1: none).
    pub default_reminder: i64,
    pub week_numbers: bool,
    /// Closing the window keeps Plume Calendar near the clock, for reminders.
    pub background: bool,
    /// "popup" (Plume's own window) or "toast" (a Windows notification).
    pub reminder_style: String,
    /// Last view: "month", "week", "day", "agenda".
    pub view: String,
    /// Hours shown from (the week view opens scrolled there).
    pub day_start: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            lang: "system".into(),
            first_day: "auto".into(),
            clock: "auto".into(),
            default_reminder: 10,
            week_numbers: false,
            background: true,
            reminder_style: "popup".into(),
            view: "month".into(),
            day_start: 7,
        }
    }
}

fn locale_info(kind: u32) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetLocaleInfoEx(PCWSTR::null(), kind, Some(&mut buf)) };
    String::from_utf16_lossy(&buf[..(n.max(1) as usize - 1).min(buf.len())])
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

    /// First day of the week, 0 = Monday … 6 = Sunday.
    pub fn week_start(&self) -> u32 {
        match self.first_day.as_str() {
            "mon" => 0,
            "sun" => 6,
            "sat" => 5,
            _ if crate::i18n::french() => 0,
            _ => locale_info(LOCALE_IFIRSTDAYOFWEEK).trim().parse::<u32>().ok().filter(|d| *d < 7).unwrap_or(6),
        }
    }

    pub fn h24(&self) -> bool {
        match self.clock.as_str() {
            "24" => true,
            "12" => false,
            _ if crate::i18n::french() => true,
            _ => locale_info(LOCALE_STIMEFORMAT).contains('H'),
        }
    }
}
