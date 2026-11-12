//! English and French. The interface is written in English; `tr` gives the French when Plume speaks French
//! (Plume Clipboard's setting, else Plume's, else Windows').

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
    french_of(en).unwrap_or(en)
}

fn french_of(en: &str) -> Option<&'static str> {
    Some(match en {
        "1 day" => "1 jour",
        "1 year" => "1 an",
        "30 days" => "30 jours",
        "7 days" => "7 jours",
        "Add" => "Ajouter",
        "All" => "Tout",
        "Always" => "Toujours",
        "Another app uses it: try other keys." => "Une autre app l'utilise : essayez d'autres touches.",
        "Cancel" => "Annuler",
        "Clear the history" => "Effacer l'historique",
        "Clear" => "Effacer",
        "Clear the search" => "Effacer la recherche",
        "Click again to clear" => "Cliquez encore pour effacer",
        "Copied" => "Copié",
        "Copy something, then press {k} to paste it again." => "Copiez quelque chose, puis {k} pour le recoller.",
        "Could not copy" => "Copie impossible",
        "Ctrl+Enter saves." => "Ctrl+Entrée enregistre.",
        "Dark" => "Sombre",
        "Delete" => "Supprimer",
        "Encrypt the history" => "Chiffrer l'historique",
        "Enter paste · Shift+Enter plain text · Del remove · Esc close" => "Entrée colle · Maj+Entrée texte brut · Suppr retire · Échap ferme",
        "Everything stays on this PC." => "Tout reste sur ce PC.",
        "Files" => "Fichiers",
        "General" => "Général",
        "History cleared" => "Historique effacé",
        "History" => "Historique",
        "Ignored apps" => "Apps ignorées",
        "In the background, with its icon near the clock." => "En arrière-plan, avec son icône près de l'horloge.",
        "Keep copied files" => "Garder les fichiers copiés",
        "Keep for" => "Garder pendant",
        "Keep pictures" => "Garder les images",
        "Keep up to" => "Garder jusqu'à",
        "Kept forever. The arrows change their order." => "Gardés pour toujours. Les flèches changent leur ordre.",
        "Language" => "Langue",
        "Largest picture" => "Image la plus grande",
        "Light" => "Clair",
        "Links" => "Liens",
        "Name" => "Nom",
        "New snippet" => "Nouvel extrait",
        "No pins yet" => "Rien d'épinglé",
        "No snippets yet" => "Aucun extrait",
        "Nothing copied yet" => "Rien de copié pour l'instant",
        "Nothing found" => "Rien trouvé",
        "Nothing new is kept until you turn this off." => "Rien de nouveau n'est gardé tant que c'est activé.",
        "Off: the item is only put back on the clipboard." => "Désactivé : l'élément est seulement remis dans le presse-papiers.",
        "Open Plume Clipboard" => "Ouvrir Plume Clipboard",
        "Open the panel" => "Ouvrir le panneau",
        "Opens the panel in any app" => "Ouvre le panneau dans n'importe quelle app",
        "Paste after picking" => "Coller après le choix",
        "Pause" => "Pause",
        "Paused" => "En pause",
        "Paused: nothing new is kept" => "En pause : rien de nouveau n'est gardé",
        "Picture" => "Image",
        "Pictures" => "Images",
        "Pin an item to keep it forever: right-click it, or its pin button." => {
            "Épinglez un élément pour le garder toujours : clic droit, ou son bouton épingle."
        }
        "Pinned items and snippets stay." => "Les éléments épinglés et les extraits restent.",
        "Pinned items do not count." => "Les éléments épinglés ne comptent pas.",
        "Pinned" => "Épinglés",
        "Pinned: kept forever" => "Épinglé : gardé pour toujours",
        "Press the new keys…" => "Appuyez sur les touches…",
        "Privacy" => "Confidentialité",
        "Quit" => "Quitter",
        "Save a text you type often, with a trigger like ;sig." => "Enregistrez un texte que vous tapez souvent, avec un déclencheur comme ;sig.",
        "Save" => "Enregistrer",
        "Search it, then Enter pastes into the app you were in." => "Cherchez, puis Entrée colle dans l'app où vous étiez.",
        "Search the history" => "Chercher dans l'historique",
        "Search, or type a snippet's ;trigger" => "Cherchez, ou tapez le ;déclencheur d'un extrait",
        "Settings" => "Paramètres",
        "Shortcut unavailable: pick another one in Settings" => "Raccourci indisponible : choisissez-en un autre dans les paramètres",
        "Shortcut" => "Raccourci",
        "Show all" => "Tout afficher",
        "Signature" => "Signature",
        "Snippet saved" => "Extrait enregistré",
        "Snippets" => "Extraits",
        "Start with Windows" => "Démarrer avec Windows",
        "System" => "Système",
        "Text" => "Texte",
        "Texts you type often. Type their trigger in the panel." => "Les textes que vous tapez souvent. Tapez leur déclencheur dans le panneau.",
        "Their paths: the files themselves stay where they are." => "Leurs chemins : les fichiers restent où ils sont.",
        "Theme" => "Thème",
        "Trigger" => "Déclencheur",
        "Try other words." => "Essayez d'autres mots.",
        "Type it in the panel to find this snippet." => "Tapez-le dans le panneau pour trouver cet extrait.",
        "Unpinned" => "Désépinglé",
        "What apps mark as private is never kept: password managers mark the passwords they copy. Everything stays on this PC, nothing goes online." => {
            "Ce que les apps marquent comme privé n'est jamais gardé : les gestionnaires de mots de passe marquent les mots de passe qu'ils copient. Tout reste sur ce PC, rien ne part en ligne."
        }
        "What the snippet pastes" => "Ce que l'extrait colle",
        "What you copy in these apps is never kept." => "Ce que vous copiez dans ces apps n'est jamais gardé.",
        "Windows locks it to your account (DPAPI)." => "Windows le verrouille sur votre compte (DPAPI).",
        "copied {n} times" => "copié {n} fois",
        "now" => "à l'instant",
        "yesterday" => "hier",
        "{n} characters" => "{n} caractères",
        "{n} days ago" => "il y a {n} jours",
        "{n} files" => "{n} fichiers",
        "{n} found" => "{n} trouvés",
        "{n} h ago" => "il y a {n} h",
        "{n} items" => "{n} éléments",
        "{n} items, newest first" => "{n} éléments, les plus récents d'abord",
        "{n} min ago" => "il y a {n} min",
        "Password" => "Mot de passe",
        "IP address" => "Adresse IP",
        "Key" => "Clé",
        "hidden" => "masqué",
        "hidden, removed in {n} min" => "masqué, retiré dans {n} min",
        "{n} sensitive items cleared" => "{n} éléments sensibles effacés",
        "Pin this sensitive item?" => "Épingler cet élément sensible ?",
        "This item ({kind}) will be kept forever instead of a short time. It stays hidden and sealed." => {
            "Cet élément ({kind}) sera gardé pour toujours au lieu d'un court moment. Il reste masqué et scellé."
        }
        "Pin it" => "Épingler",
        "Sensitive items are pinned from the main window." => "Les éléments sensibles s'épinglent depuis la fenêtre principale.",
        "Clear sensitive items" => "Effacer les éléments sensibles",
        "Sensitive copies" => "Copies sensibles",
        "Shown hidden, sealed on this PC even if the history is not encrypted, and kept a short time." => {
            "Affichées masquées, scellées sur ce PC même si l'historique n'est pas chiffré, et gardées peu de temps."
        }
        "Hide passwords" => "Masquer les mots de passe",
        "One word mixing capitals, digits and symbols." => "Un mot qui mélange majuscules, chiffres et symboles.",
        "Hide IP addresses" => "Masquer les adresses IP",
        "Hide keys and tokens" => "Masquer les clés et jetons",
        "API keys, tokens, JWTs and private keys." => "Clés d'API, jetons, JWT et clés privées.",
        "Keep them for" => "Les garder pendant",
        "Unless pinned." => "Sauf si épinglées.",
        "5 min" => "5 min",
        "15 min" => "15 min",
        "1 hour" => "1 heure",
        "8 hours" => "8 heures",
        "Never keep sensitive items" => "Ne jamais garder les éléments sensibles",
        "Skipped like private content." => "Ignorées comme le contenu privé.",
        "Pinned ones too." => "Les épinglés aussi.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes() {
        assert_eq!(fill("{a} of {b}", &["3", "10"]), "3 of 10");
        assert_eq!(fill("no holes", &["x"]), "no holes");
    }

    /// Every text given to `tr` in the code has its French.
    #[test]
    fn all_translated() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        let mut stack = vec![dir];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let src = std::fs::read_to_string(&p).unwrap();
                for part in src.split("tr(\"").skip(1) {
                    let Some(end) = part.find("\")") else { continue };
                    let s = &part[..end];
                    if !s.contains('"') && s.trim() != "—" && french_of(s).is_none() && !p.ends_with("i18n.rs") && !["English", "Français"].contains(&s) {
                        missing.push(format!("{}: {s}", p.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "{missing:#?}");
    }
}
