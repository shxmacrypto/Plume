//! English and French. The interface is written in English; `tr` gives the French when Plume speaks French
//! (Plume Create's own choice, else the suite's, else Windows').

use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Globalization::GetUserDefaultUILanguage;

static FR: AtomicBool = AtomicBool::new(false);

pub fn set(lang: &str) {
    let fr = match lang {
        "fr" => true,
        "en" => false,
        _ => system_french(),
    };
    FR.store(fr, Ordering::Relaxed);
}

pub fn french() -> bool {
    FR.load(Ordering::Relaxed)
}

fn system_french() -> bool {
    if let Ok(l) = std::env::var("PLUME_SUITE_LANG") {
        return l == "fr";
    }
    let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_default().join("Plume");
    for f in [base.join("Suite").join("settings.json"), base.join("plume.json")] {
        let cfg = std::fs::read_to_string(f).unwrap_or_default();
        if cfg.contains(r#""lang":"fr""#) || cfg.contains(r#""lang": "fr""#) {
            return true;
        }
        if cfg.contains(r#""lang":"en""#) || cfg.contains(r#""lang": "en""#) {
            return false;
        }
    }
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x0c }
}

/// `{a}`, `{b}`… filled in order.
pub fn fill(s: &str, args: &[&str]) -> String {
    let mut out = s.to_string();
    for (i, a) in args.iter().enumerate() {
        let key = format!("{{{}}}", (b'a' + i as u8) as char);
        out = out.replace(&key, a);
    }
    out
}

pub fn tr(en: &'static str) -> &'static str {
    if !french() {
        return en;
    }
    fr(en)
}

include!("i18n_fr.rs");

/// Tests that change the language take this first (the language is global).
#[cfg(test)]
pub static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_and_holes() {
        let _l = TEST_LOCK.lock();
        assert_eq!(fill("{a} × {b}", &["1080", "1920"]), "1080 × 1920");
        set("fr");
        assert_eq!(tr("Layers"), "Calques");
        assert_eq!(tr("Export"), "Exporter");
        set("en");
        assert_eq!(tr("Layers"), "Layers");
    }
}
