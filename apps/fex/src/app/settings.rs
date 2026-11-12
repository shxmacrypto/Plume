//! Réglages de Plume FEx : `%LOCALAPPDATA%\Plume\FEx\settings.json`.
//! Ce qui n'y est pas encore réglé vient de Plume (langue, thème et couleur d'accent du navigateur)
//! et de l'Explorateur de Windows (fichiers masqués, extensions).

use super::i18n::Lang;
use crate::{shell::integration::Backup, ui::Color};
use serde_json::{Value, json};
use std::path::PathBuf;
use windows::{
    Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
    core::PCWSTR,
};

pub use crate::ui::window::Placement;

/// Une catégorie de la barre latérale : un groupe de dossiers choisi par l'utilisateur (« Travail », « Jeux »…).
#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    pub name: String,
    /// Pastille de couleur : 0 aucune, sinon une couleur de `CAT_COLORS` (à partir de 1).
    pub color: u8,
    /// Ses dossiers sont affichés (sinon la catégorie est repliée).
    pub open: bool,
    pub folders: Vec<PathBuf>,
}

/// Les couleurs des pastilles de catégorie (orange, rouge, vert, bleu, violet, rose, gris).
pub const CAT_COLORS: [u32; 7] = [0xff9f0a, 0xff453a, 0x30d158, 0x0a84ff, 0xbf5af2, 0xff375f, 0x8e8e93];

/// Combien de dossiers récents sont gardés et montrés.
pub const RECENT_MAX: usize = 8;

#[derive(Clone, Debug)]
pub struct Settings {
    /// Dossier des données de Plume FEx.
    pub dir: PathBuf,
    /// « auto », « en » ou « fr ».
    pub lang: String,
    /// « system », « dark » ou « light ».
    pub theme: String,
    pub accent: Option<Color>,
    /// Réglé dans Plume FEx même (sinon la valeur suit celle du navigateur Plume).
    pub own_lang: bool,
    pub own_theme: bool,
    pub own_accent: bool,
    pub show_hidden: bool,
    /// Fichiers protégés du système (masqués et système à la fois).
    pub show_system: bool,
    pub hide_ext: bool,
    pub nav_w: f32,
    pub nav_open: bool,
    /// Largeur des colonnes Date, Type et Taille.
    pub cols: [f32; 3],
    /// Largeur de la colonne Dossier des résultats de recherche (0 : un tiers de la liste).
    pub col_folder: f32,
    /// Afficher la taille des dossiers (calculée par l'index, qui reste alors en mémoire pendant qu'on navigue).
    pub folder_sizes: bool,
    /// Volet d'aperçu à droite de la liste (Alt+P).
    pub preview: bool,
    /// Vue en icônes (sinon « Détails »), et leur taille : 0 très grandes, 1 grandes, 2 moyennes, 3 petites.
    pub thumbs: bool,
    pub icon_size: u8,
    pub win: Option<Placement>,
    pub pinned: Vec<PathBuf>,
    /// L'ordre choisi des dossiers du haut de la barre latérale (dossiers personnels et épinglés).
    pub quick_order: Vec<PathBuf>,
    /// Les derniers dossiers ouverts, le plus récent d'abord.
    pub recent: Vec<PathBuf>,
    /// La section « Récents » de la barre latérale : affichée, et dépliée.
    pub show_recent: bool,
    pub recent_open: bool,
    pub categories: Vec<Category>,
    /// Les serveurs SFTP déjà visités (« utilisateur@hôte ») : jamais leur mot de passe.
    pub servers: Vec<String>,
    /// La recherche porte sur tout le PC (sinon sur le dossier ouvert et ses sous-dossiers).
    pub search_everywhere: bool,
    /// Plume FEx est l'explorateur par défaut : ce qui ouvrait les dossiers avant, pour le remettre en place.
    pub explorer_backup: Option<Backup>,
}

fn wz(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn reg_dword(sub: &str, name: &str) -> Option<u32> {
    let (s, n) = (wz(sub), wz(name));
    let (mut data, mut size) = (0u32, 4u32);
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, PCWSTR(s.as_ptr()), PCWSTR(n.as_ptr()), RRF_RT_REG_DWORD, None, Some(&mut data as *mut u32 as *mut _), Some(&mut size))
    };
    r.is_ok().then_some(data)
}

/// Largeur de départ des colonnes Date, Type et Taille.
pub const COLS: [f32; 3] = [150.0, 170.0, 96.0];

const EXPLORER: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

/// Windows est-il réglé sur le thème sombre pour les applications ?
pub fn system_dark() -> bool {
    reg_dword(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "AppsUseLightTheme") == Some(0)
}

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

fn read_json(path: &std::path::Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

impl Settings {
    /// Réglages par défaut, puis ceux de Plume et de l'Explorateur, puis le fichier de Plume FEx.
    pub fn load() -> Settings {
        // PLUME_FEX_PROFILE : dossier de données à part, pour les essais (rien n'est alors repris de Plume)
        let test_dir = std::env::var_os("PLUME_FEX_PROFILE").map(PathBuf::from);
        let dir = test_dir.clone().unwrap_or_else(|| local_app_data().join("Plume").join("FEx"));
        let mut s = Settings {
            dir,
            lang: "auto".into(),
            theme: "system".into(),
            accent: None,
            own_lang: false,
            own_theme: false,
            own_accent: false,
            show_hidden: reg_dword(EXPLORER, "Hidden") == Some(1),
            show_system: reg_dword(EXPLORER, "ShowSuperHidden") == Some(1),
            hide_ext: reg_dword(EXPLORER, "HideFileExt") == Some(1),
            nav_w: 216.0,
            nav_open: true,
            cols: COLS,
            col_folder: 0.0,
            folder_sizes: false,
            preview: false,
            thumbs: false,
            icon_size: 1,
            win: None,
            pinned: vec![],
            quick_order: vec![],
            recent: vec![],
            show_recent: true,
            recent_open: true,
            categories: vec![],
            servers: vec![],
            search_everywhere: false,
            explorer_backup: None,
        };
        if test_dir.is_none()
            && let Some(plume) = read_json(&local_app_data().join("Plume").join("plume.json"))
            && let Some(cfg) = plume.get("cfg")
        {
            s.apply_look(cfg);
            // la langue et le thème suivent Plume ; l'accent reste celui de Plume FEx (la couleur de son icône)
            s.accent = None;
        }
        if let Some(v) = read_json(&s.file()) {
            s.apply_look(&v);
            s.own_lang = v.get("lang").is_some();
            s.own_theme = v.get("theme").is_some();
            s.own_accent = v.get("accent").is_some();
            let b = |k: &str, d: bool| v.get(k).and_then(Value::as_bool).unwrap_or(d);
            let f = |k: &str, d: f32| v.get(k).and_then(Value::as_f64).map_or(d, |x| x as f32);
            s.show_hidden = b("show_hidden", s.show_hidden);
            s.show_system = b("show_system", s.show_system);
            s.hide_ext = b("hide_ext", s.hide_ext);
            s.nav_open = b("nav_open", s.nav_open);
            s.search_everywhere = b("search_everywhere", s.search_everywhere);
            s.folder_sizes = b("folder_sizes", s.folder_sizes);
            s.preview = b("preview", s.preview);
            s.thumbs = b("thumbs", s.thumbs);
            s.icon_size = v.get("icon_size").and_then(Value::as_u64).map_or(s.icon_size, |k| k.min(3) as u8);
            s.col_folder = Some(f("col_folder", 0.0)).filter(|w| *w > 0.0).map_or(0.0, |w| w.clamp(60.0, 900.0));
            s.nav_w = f("nav_w", s.nav_w).clamp(140.0, 480.0);
            if let Some(cols) = v.get("cols").and_then(Value::as_array) {
                for (i, c) in cols.iter().take(3).enumerate() {
                    s.cols[i] = c.as_f64().map_or(s.cols[i], |x| (x as f32).clamp(60.0, 600.0));
                }
            }
            if let Some(w) = v.get("win") {
                let i = |k: &str| w.get(k).and_then(Value::as_i64).map(|x| x as i32);
                if let (Some(x), Some(y), Some(ww), Some(h)) = (i("x"), i("y"), i("w"), i("h")) {
                    s.win = Some(Placement { x, y, w: ww, h, maximized: w.get("max").and_then(Value::as_bool).unwrap_or(false) });
                }
            }
            let paths = |k: &str| -> Vec<PathBuf> {
                v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(PathBuf::from).collect()).unwrap_or_default()
            };
            s.pinned = paths("pinned");
            s.quick_order = paths("quick_order");
            s.recent = paths("recent");
            s.recent.truncate(RECENT_MAX);
            s.show_recent = b("show_recent", s.show_recent);
            s.recent_open = b("recent_open", s.recent_open);
            if let Some(list) = v.get("categories").and_then(Value::as_array) {
                s.categories = list
                    .iter()
                    .filter_map(|c| {
                        let name = c.get("name").and_then(Value::as_str)?.trim().to_string();
                        Some(Category {
                            color: c.get("color").and_then(Value::as_u64).map_or(0, |k| if k as usize <= CAT_COLORS.len() { k as u8 } else { 0 }),
                            open: c.get("open").and_then(Value::as_bool).unwrap_or(true),
                            folders: c
                                .get("folders")
                                .and_then(Value::as_array)
                                .map(|a| a.iter().filter_map(Value::as_str).map(PathBuf::from).collect())
                                .unwrap_or_default(),
                            name: if name.is_empty() { "?".into() } else { name },
                        })
                    })
                    .collect();
            }
            if let Some(list) = v.get("servers").and_then(Value::as_array) {
                // relus par le même contrôle que ce qui est tapé : rien d'autre qu'une adresse n'ira vers sftp.exe
                s.servers = list.iter().filter_map(Value::as_str).filter_map(crate::shell::sftp::Server::parse).map(|server| server.label()).collect();
            }
            if let Some(b) = v.get("explorer_backup").filter(|b| b.is_object()) {
                let text = |k: &str| b.get(k).and_then(Value::as_str).map(str::to_string);
                s.explorer_backup =
                    Some(Backup { directory: text("directory"), drive: text("drive"), win_e: text("win_e"), win_e_delegate: text("win_e_delegate") });
            }
        }
        s
    }

    /// Langue, thème et accent : mêmes clés dans les réglages de Plume et dans ceux de Plume FEx.
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

    /// L'accent : l'orange de l'icône de Plume FEx (plus soutenu sur fond clair), sauf s'il est choisi dans ses réglages.
    pub fn accent_color(&self) -> Color {
        self.accent.unwrap_or_else(|| Color::hex(if self.dark() { 0xff9f0a } else { 0xd97a00 }))
    }

    pub fn dark(&self) -> bool {
        match self.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => system_dark(),
        }
    }

    /// Écrit le fichier (par un fichier temporaire : jamais de réglages à moitié écrits).
    pub fn save(&self) {
        let mut v = json!({
            "show_hidden": self.show_hidden,
            "show_system": self.show_system,
            "hide_ext": self.hide_ext,
            "nav_w": self.nav_w,
            "nav_open": self.nav_open,
            "cols": self.cols,
            "col_folder": self.col_folder,
            "folder_sizes": self.folder_sizes,
            "preview": self.preview,
            "thumbs": self.thumbs,
            "icon_size": self.icon_size,
            "search_everywhere": self.search_everywhere,
            "pinned": self.pinned.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "quick_order": self.quick_order.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "recent": self.recent.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "show_recent": self.show_recent,
            "recent_open": self.recent_open,
            "categories": self.categories.iter().map(|c| json!({
                "name": c.name,
                "color": c.color,
                "open": c.open,
                "folders": c.folders.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "servers": self.servers,
        });
        if self.own_lang {
            v["lang"] = json!(self.lang);
        }
        if self.own_theme {
            v["theme"] = json!(self.theme);
        }
        if let Some(a) = self.accent.filter(|_| self.own_accent) {
            v["accent"] = json!(format!("#{:02x}{:02x}{:02x}", (a.r * 255.0).round() as u8, (a.g * 255.0).round() as u8, (a.b * 255.0).round() as u8));
        }
        if let Some(w) = self.win {
            v["win"] = json!({ "x": w.x, "y": w.y, "w": w.w, "h": w.h, "max": w.maximized });
        }
        if let Some(b) = &self.explorer_backup {
            v["explorer_backup"] = json!({ "directory": b.directory, "drive": b.drive, "win_e": b.win_e, "win_e_delegate": b.win_e_delegate });
        }
        let _ = std::fs::create_dir_all(&self.dir);
        let tmp = self.dir.join("settings.json.tmp");
        if std::fs::write(&tmp, serde_json::to_string_pretty(&v).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, self.file());
        }
    }
}
