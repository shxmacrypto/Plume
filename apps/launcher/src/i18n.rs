//! English and French. The interface is written in English; `tr` gives the French when Plume Launcher speaks
//! French (its own choice, otherwise the Plume Suite's or Windows').

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
        if cfg.contains(r#""lang":"fr""#) {
            return true;
        }
        if cfg.contains(r#""lang":"en""#) {
            return false;
        }
    }
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x0c }
}

/// `{n}`-style holes filled in order.
pub fn fill(s: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = s;
    let mut i = 0;
    while let Some(a) = rest.find('{') {
        let Some(b) = rest[a..].find('}') else { break };
        out.push_str(&rest[..a]);
        out.push_str(args.get(i).copied().unwrap_or(""));
        i += 1;
        rest = &rest[a + b + 1..];
    }
    out.push_str(rest);
    out
}

pub fn tr(en: &'static str) -> &'static str {
    if !french() {
        return en;
    }
    fr(en)
}

fn fr(en: &'static str) -> &'static str {
    match en {
        "Add Ctrl, Alt or Win to the key" => "Ajoutez Ctrl, Alt ou Win à la touche",
        "Another app already uses this shortcut: choose another one" => "Une autre application utilise déjà ce raccourci : choisissez-en un autre",
        "App" => "Application",
        "Apps" => "Applications",
        "Calculator" => "Calculatrice",
        "Clear" => "Effacer",
        "Copy" => "Copier",
        "Dark" => "Sombre",
        "Desktop, Documents, Downloads, Pictures, Music, Videos, OneDrive" => "Bureau, Documents, Téléchargements, Images, Musique, Vidéos, OneDrive",
        "Enter to copy the result" => "Entrée pour copier le résultat",
        "Enter to open, Ctrl+Enter to open its folder" => "Entrée pour ouvrir, Ctrl+Entrée pour ouvrir son dossier",
        "Enter to search the web" => "Entrée pour chercher sur le web",
        "File index" => "Index des fichiers",
        "File" => "Fichier",
        "Files and folders" => "Fichiers et dossiers",
        "Folder" => "Dossier",
        "General" => "Général",
        "History" => "Historique",
        "Indexing files…" => "Indexation des fichiers…",
        "Indexing your folders…" => "Indexation de vos dossiers…",
        "Language" => "Langue",
        "Light" => "Clair",
        "Nothing found" => "Aucun résultat",
        "Open Plume Launcher" => "Ouvrir Plume Launcher",
        "Open" => "Ouvrir",
        "Opens the search box from anywhere" => "Ouvre la recherche depuis n'importe où",
        "Plume Launcher settings" => "Paramètres de Plume Launcher",
        "Press keys…" => "Appuyez…",
        "Press the new shortcut, or Esc to cancel" => "Appuyez sur le nouveau raccourci, ou Échap pour annuler",
        "Quit" => "Quitter",
        "Rebuild" => "Reconstruire",
        "Recent" => "Récents",
        "Runs in the background, with an icon near the clock" => "Tourne en arrière-plan, avec une icône près de l'horloge",
        "Search apps, files, settings…" => "Rechercher applications, fichiers, paramètres…",
        "Search engine" => "Moteur de recherche",
        "Search in" => "Rechercher dans",
        "Search the web for \u{201c}{q}\u{201d}" => "Rechercher \u{ab}\u{a0}{q}\u{a0}\u{bb} sur le web",
        "Search" => "Rechercher",
        "Setting" => "Paramètre",
        "Settings" => "Paramètres",
        "Shortcut" => "Raccourci",
        "Start menu and Store apps" => "Menu Démarrer et applications du Store",
        "Start with Windows" => "Lancer avec Windows",
        "Sums, percentages and units, like 15% of 80 or 5 km in mi" => "Calculs, pourcentages et unités, comme 15% de 80 ou 5 km en mi",
        "System" => "Système",
        "Theme" => "Thème",
        "Type to search apps, files and settings, or to calculate" => "Tapez pour chercher applications, fichiers et paramètres, ou pour calculer",
        "Up and down to move, Enter to open" => "Haut et bas pour choisir, Entrée pour ouvrir",
        "Web search" => "Recherche web",
        "Web" => "Web",
        "When nothing else matches, search the web" => "Quand rien d'autre ne correspond, chercher sur le web",
        "Windows settings" => "Paramètres Windows",
        "updated just now" => "mis à jour à l'instant",
        "updated {n} h ago" => "mis à jour il y a {n} h",
        "updated {n} min ago" => "mis à jour il y a {n} min",
        "{n} items" => "{n} éléments",
        "{n} results remembered, to put your picks first" => "{n} résultats retenus, pour mettre vos choix en premier",
        _ => en,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes() {
        assert_eq!(fill("{a} of {b}", &["3", "10"]), "3 of 10");
        assert_eq!(fill("no holes", &["x"]), "no holes");
    }

    #[test]
    fn every_string_is_translated() {
        // the English strings of the interface, as written in the code
        let sources = [include_str!("ui.rs"), include_str!("settings_ui.rs"), include_str!("main.rs")];
        let mut missing = vec![];
        for src in sources {
            let mut rest = src;
            while let Some(i) = rest.find("tr(\"") {
                rest = &rest[i + 4..];
                let Some(end) = rest.find("\")") else { break };
                let raw = &rest[..end];
                // the one string written with escapes
                let s = raw.replace("\\u{201c}", "\u{201c}").replace("\\u{201d}", "\u{201d}");
                let s: &'static str = Box::leak(s.into_boxed_str());
                if fr(s) == s && !matches!(s, "Web") {
                    missing.push(s.to_string());
                }
            }
        }
        assert!(missing.is_empty(), "not translated: {missing:?}");
    }
}
