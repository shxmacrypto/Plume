//! English and French. The interface is written in English; `tr` gives the French when Plume speaks French
//! (Plume Notepad's setting, else Plume's, else Windows').

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
        "(empty pad)" => "(pad vide)",
        "1 pad" => "1 pad",
        "1 replacement" => "1 remplacement",
        "1 result" => "1 résultat",
        "All files" => "Tous les fichiers",
        "Anywhere in Windows: Ctrl+Alt+N" => "Partout dans Windows : Ctrl+Alt+N",
        "Appearance" => "Apparence",
        "Cancel" => "Annuler",
        "Choose Plume Notepad for .txt in Windows Settings" => "Choisissez Plume Notepad pour .txt dans les Paramètres de Windows",
        "Choose…" => "Choisir…",
        "Couldn't change it" => "Impossible de le changer",
        "Couldn't open the file: {e}" => "Impossible d'ouvrir le fichier : {e}",
        "Couldn't save: {e}" => "Impossible d'enregistrer : {e}",
        "Dark" => "Sombre",
        "Default app for text files" => "App par défaut des fichiers texte",
        "Delete" => "Supprimer",
        "Delete pad" => "Supprimer le pad",
        "Delete this pad?" => "Supprimer ce pad ?",
        "Don't save" => "Ne pas enregistrer",
        "Editor" => "Éditeur",
        "English" => "English",
        "Find" => "Rechercher",
        "Fixed width" => "Chasse fixe",
        "Font" => "Police",
        "For notepad.exe, the Run box and apps that open Notepad. Windows asks for an administrator's permission." => {
            "Pour notepad.exe, la boîte Exécuter et les apps qui ouvrent le Bloc-notes. Windows demande l'accord d'un administrateur."
        }
        "Français" => "Français",
        "Go" => "Aller",
        "Go to line" => "Atteindre la ligne",
        "Go to line…" => "Atteindre la ligne…",
        "Hide all pads" => "Masquer tous les pads",
        "Insert time and date" => "Insérer l'heure et la date",
        "Language" => "Langue",
        "Light" => "Clair",
        "Line number (1 to {n})" => "Numéro de ligne (1 à {n})",
        "Line numbers" => "Numéros de ligne",
        "Ln {l}, Col {c}" => "Ligne {l}, col. {c}",
        "More" => "Plus",
        "New pad" => "Nouveau pad",
        "New sticky pad" => "Nouveau pad",
        "New tab" => "Nouvel onglet",
        "No results" => "Aucun résultat",
        "Open" => "Ouvrir",
        "Open Plume Notepad" => "Ouvrir Plume Notepad",
        "Open Plume Notepad instead of Windows Notepad" => "Ouvrir Plume Notepad à la place du Bloc-notes",
        "Open in Plume Notepad" => "Ouvrir dans Plume Notepad",
        "Open my pads when I sign in" => "Ouvrir mes pads à la connexion",
        "Plume Notepad stays near the clock when its window is closed." => "Plume Notepad reste près de l'horloge quand sa fenêtre est fermée.",
        "Plume Notepad: 1 pad" => "Plume Notepad : 1 pad",
        "Plume Notepad: plain text, tabs, and sticky pads." => "Plume Notepad : texte brut, onglets et pads repositionnables.",
        "Plume Notepad: {n} pads" => "Plume Notepad : {n} pads",
        "Quit" => "Quitter",
        "Redo" => "Rétablir",
        "Reopen as {enc}" => "Rouvrir en {enc}",
        "Replace" => "Remplacer",
        "Replace all" => "Tout remplacer",
        "Replace with" => "Remplacer par",
        "Reset zoom" => "Zoom par défaut",
        "Sans serif" => "Sans empattement",
        "Save" => "Enregistrer",
        "Save all" => "Tout enregistrer",
        "Save anyway" => "Enregistrer quand même",
        "Save as" => "Enregistrer sous",
        "Save as UTF-8" => "Enregistrer en UTF-8",
        "Save as {enc}" => "Enregistrer en {enc}",
        "Save as…" => "Enregistrer sous…",
        "Save changes to {name}?" => "Enregistrer les modifications de {name} ?",
        "Settings" => "Réglages",
        "Show all" => "Tout afficher",
        "Show all pads" => "Afficher tous les pads",
        "Some bytes are not valid {enc}: they show as \u{FFFD}" => "Certains octets ne sont pas du {enc} valide : ils s'affichent en \u{FFFD}",
        "Some characters can't be saved in ANSI" => "Certains caractères n'existent pas en ANSI",
        "Status bar" => "Barre d'état",
        "Sticky pad" => "Pad",
        "Sticky pads" => "Pads",
        "System" => "Système",
        "Text files" => "Fichiers texte",
        "The file is too big (more than 512 MB)" => "Le fichier est trop gros (plus de 512 Mo)",
        "Theme" => "Thème",
        "They would become \"?\". Save in UTF-8 instead?" => "Ils deviendraient « ? ». Enregistrer plutôt en UTF-8 ?",
        "This is a folder" => "C'est un dossier",
        "Type a line number" => "Tapez un numéro de ligne",
        "Undo" => "Annuler",
        "Untitled" => "Sans titre",
        "Untitled {n}" => "Sans titre {n}",
        "Waiting for Windows…" => "En attente de Windows…",
        "Windows Notepad" => "Bloc-notes de Windows",
        "Windows Notepad is back" => "Le Bloc-notes de Windows est de retour",
        "Windows did not allow it" => "Windows ne l'a pas permis",
        "Windows lets only you choose it: Plume Notepad is listed for .txt, .log, .ini, .md…" => {
            "Windows vous laisse seul le choisir : Plume Notepad est proposé pour .txt, .log, .ini, .md…"
        }
        "Windows now opens Plume Notepad instead of Notepad" => "Windows ouvre désormais Plume Notepad au lieu du Bloc-notes",
        "Word wrap" => "Retour à la ligne",
        "Write something…" => "Écrivez quelque chose…",
        "Your changes will be lost if you don't save them." => "Vos modifications seront perdues si vous ne les enregistrez pas.",
        "Zoom in" => "Zoom avant",
        "Zoom out" => "Zoom arrière",
        "\"{q}\" not found" => "« {q} » introuvable",
        "{i} of {n}" => "{i} sur {n}",
        "{name} will be created when you save" => "{name} sera créé à l'enregistrement",
        "{n} characters" => "{n} caractères",
        "{n} characters ({s} selected)" => "{n} caractères ({s} sélectionnés)",
        "{n} pads" => "{n} pads",
        "{n} replacements" => "{n} remplacements",
        "{n} results" => "{n} résultats",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes() {
        assert_eq!(fill("{i} of {n}", &["3", "12"]), "3 of 12");
        assert_eq!(fill("no holes", &["x"]), "no holes");
        assert!(french_of("Save").is_some());
    }
}
