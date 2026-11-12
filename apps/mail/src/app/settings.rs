//! Les réglages de Plume Mail : thème, langue, place de la fenêtre. Sans réglage propre, le thème et la langue
//! sont ceux du navigateur Plume, pour que toute la suite ait le même visage.

use super::i18n::Lang;
use plume_ui::{Color, window::Placement};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use windows::{
    Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
    core::PCWSTR,
};

#[derive(Clone, Debug)]
pub struct Settings {
    /// Le dossier de Plume Mail (celui de la boîte gardée).
    pub dir: PathBuf,
    /// « auto », « en » ou « fr ».
    pub lang: String,
    /// « system », « dark » ou « light ».
    pub theme: String,
    pub accent: Option<Color>,
    /// Réglé dans Plume Mail même (sinon la valeur suit celle du navigateur Plume).
    pub own_lang: bool,
    pub own_theme: bool,
    pub win: Option<Placement>,
    /// Les messages d'une même conversation sont regroupés dans la liste.
    pub threads: bool,
}

/// Windows est-il réglé sur le thème sombre pour les applications ?
pub fn system_dark() -> bool {
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() };
    let (key, name) = (wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"), wide("AppsUseLightTheme"));
    let (mut data, mut size) = (1u32, 4u32);
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    read.is_ok() && data == 0
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(std::fs::read_to_string(path).ok()?.trim_start_matches('\u{feff}')).ok()
}

impl Settings {
    /// `dir` : le dossier de Plume Mail ; `test` : dossier d'essai, où rien n'est repris du navigateur Plume.
    pub fn load(dir: PathBuf, test: bool) -> Settings {
        let mut s = Settings { dir, lang: "auto".into(), theme: "system".into(), accent: None, own_lang: false, own_theme: false, win: None, threads: true };
        if !test
            && let Some(base) = std::env::var_os("LOCALAPPDATA")
            && let Some(plume) = read_json(&Path::new(&base).join("Plume").join("plume.json"))
            && let Some(cfg) = plume.get("cfg")
        {
            s.apply_look(cfg);
            // la langue et le thème suivent Plume ; l'accent reste celui de Plume Mail (la couleur de son icône)
            s.accent = None;
        }
        if let Some(v) = read_json(&s.file()) {
            s.apply_look(&v);
            s.own_lang = v.get("lang").is_some();
            s.own_theme = v.get("theme").is_some();
            s.threads = v.get("threads").and_then(Value::as_bool).unwrap_or(true);
            if let Some(w) = v.get("win") {
                let i = |k: &str| w.get(k).and_then(Value::as_i64).map(|x| x as i32);
                if let (Some(x), Some(y), Some(ww), Some(h)) = (i("x"), i("y"), i("w"), i("h")) {
                    s.win = Some(Placement { x, y, w: ww, h, maximized: w.get("max").and_then(Value::as_bool).unwrap_or(false) });
                }
            }
        }
        s
    }

    fn apply_look(&mut self, v: &Value) {
        if let Some(l) = v.get("lang").and_then(Value::as_str) {
            self.lang = l.to_string();
        }
        if let Some(t) = v.get("theme").and_then(Value::as_str)
            && matches!(t, "system" | "dark" | "light")
        {
            self.theme = t.to_string();
        }
        if let Some(a) = v.get("accent").and_then(Value::as_str) {
            self.accent = Color::parse(a);
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    pub fn language(&self) -> Lang {
        Lang::parse(&self.lang)
    }

    /// L'accent : le rose de l'icône de Plume Mail, sauf s'il est choisi dans ses réglages.
    pub fn accent_color(&self) -> Color {
        self.accent.unwrap_or_else(|| Color::hex(if self.dark() { 0xff375f } else { 0xff2d55 }))
    }

    pub fn dark(&self) -> bool {
        match self.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => system_dark(),
        }
    }

    pub fn save(&self) {
        let mut v = json!({});
        if self.own_lang {
            v["lang"] = json!(self.lang);
        }
        if self.own_theme {
            v["theme"] = json!(self.theme);
        }
        if !self.threads {
            v["threads"] = json!(false);
        }
        if let Some(w) = self.win {
            v["win"] = json!({ "x": w.x, "y": w.y, "w": w.w, "h": w.h, "max": w.maximized });
        }
        let _ = std::fs::create_dir_all(&self.dir);
        let tmp = self.dir.join("settings.json.tmp");
        if std::fs::write(&tmp, serde_json::to_string_pretty(&v).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, self.file());
        }
    }
}
