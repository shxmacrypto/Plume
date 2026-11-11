//! English and French. The interface is written in English; `tr` gives the French when Plume speaks French
//! (Plume's choice, otherwise Windows').

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
    fr(en).unwrap_or(en)
}

fn fr(en: &str) -> Option<&'static str> {
    Some(match en {
        "Could not open “{name}”." => "Impossible d'ouvrir « {name} ».",
        "This file can be read but not changed ({why})." => "Ce fichier peut être lu mais pas modifié ({why}).",
        "The file is still opening." => "Le fichier s'ouvre encore.",
        "Page deleted." => "Page supprimée.",
        "{n} pages deleted." => "{n} pages supprimées.",
        "This file cannot be changed." => "Ce fichier ne peut pas être modifié.",
        "Saved “{name}”." => "« {name} » enregistré.",
        "Could not save: {e}" => "Enregistrement impossible : {e}",
        " (edited)" => " (modifié)",
        "Save a copy" => "Enregistrer une copie",
        "No changes to save." => "Aucune modification à enregistrer.",
        "Open PDF files" => "Ouvrir des fichiers PDF",
        "PDF files to add" => "Fichiers PDF à ajouter",
        "it is protected by a password" => "il est protégé par un mot de passe",
        "Could not add “{name}”: {e}" => "Impossible d'ajouter « {name} » : {e}",
        "{n} files added at the end." => "{n} fichiers ajoutés à la fin.",
        "Merged.pdf" => "Fusion.pdf",
        "{n} files merged. Save to keep the result." => "{n} fichiers fusionnés. Enregistrez pour garder le résultat.",
        "pages" => "pages",
        "Save the pages as" => "Enregistrer les pages sous",
        "Pages not understood: {part}" => "Pages non comprises : {part}",
        "{n} files written." => "{n} fichiers écrits.",
        "{n} pages sent to the printer." => "{n} pages envoyées à l'imprimante.",
        "Could not print: {e}" => "Impression impossible : {e}",
        "Signature saved. Click where it goes." => "Signature enregistrée. Cliquez là où elle va.",
        "Draw your signature in the box first." => "Dessinez d'abord votre signature dans le cadre.",
        "Folder for the new files" => "Dossier des nouveaux fichiers",
        "Merge PDFs…" => "Fusionner des PDF…",
        "Split into files…" => "Découper en fichiers…",
        "Save pages as a new file…" => "Enregistrer des pages dans un nouveau fichier…",
        "Rotate left" => "Tourner à gauche",
        "Rotate right" => "Tourner à droite",
        "Delete pages" => "Supprimer les pages",
        "Save a copy…" => "Enregistrer une copie…",
        "Print…" => "Imprimer…",
        "New signature…" => "Nouvelle signature…",
        "Delete" => "Supprimer",
        "Edit text" => "Modifier le texte",
        "Drag over text to highlight it" => "Glissez sur le texte pour le surligner",
        "Click to add a note" => "Cliquez pour ajouter une note",
        "Draw on the page" => "Dessinez sur la page",
        "Click or drag to add text" => "Cliquez ou glissez pour ajouter du texte",
        "Click to place your signature" => "Cliquez pour placer votre signature",
        "Pages" => "Pages",
        "Outline" => "Sommaire",
        "1 page" => "1 page",
        "{n} pages" => "{n} pages",
        "No bookmarks in this file." => "Pas de signets dans ce fichier.",
        "Opening…" => "Ouverture…",
        "This file is protected" => "Ce fichier est protégé",
        "It needs a password to open." => "Il faut un mot de passe pour l'ouvrir.",
        "Enter the password" => "Saisir le mot de passe",
        "This file could not be opened" => "Ce fichier n'a pas pu être ouvert",
        "Find in the document" => "Rechercher dans le document",
        "Reading…" => "Lecture…",
        "No text" => "Pas de texte",
        "No results" => "Aucun résultat",
        "{i} of {n}" => "{i} sur {n}",
        "Read, sign, annotate and merge PDF files." => "Lisez, signez, annotez et fusionnez vos PDF.",
        "Open a PDF" => "Ouvrir un PDF",
        "Or drop PDF files on this window." => "Ou déposez des PDF sur cette fenêtre.",
        "Recent" => "Récents",
        "Password needed" => "Mot de passe nécessaire",
        "That password is not right. Try again." => "Ce mot de passe n'est pas le bon. Réessayez.",
        "“{name}” is protected. Enter its password to open it." => "« {name} » est protégé. Saisissez son mot de passe pour l'ouvrir.",
        "Open" => "Ouvrir",
        "Replace the original file?" => "Remplacer le fichier d'origine ?",
        "“{name}” will be overwritten with your changes. You can save a copy instead." => {
            "« {name} » sera remplacé par la version modifiée. Vous pouvez plutôt enregistrer une copie."
        }
        "Replace" => "Remplacer",
        "Close without saving?" => "Fermer sans enregistrer ?",
        "Changes to “{name}” will be lost." => "Les modifications de « {name} » seront perdues.",
        "Close without saving" => "Fermer sans enregistrer",
        "Quit without saving?" => "Quitter sans enregistrer ?",
        "Some files have changes that are not saved." => "Certains fichiers ont des modifications non enregistrées.",
        "Quit" => "Quitter",
        "Split into files" => "Découper en fichiers",
        "Pages for each new file, separated by commas. Example: 1-3, 4-6, 7" => {
            "Les pages de chaque nouveau fichier, séparées par des virgules. Exemple : 1-3, 4-6, 7"
        }
        "Choose a folder…" => "Choisir un dossier…",
        "Note" => "Note",
        "Ctrl+Enter to finish." => "Ctrl+Entrée pour terminer.",
        "Done" => "OK",
        "Text" => "Texte",
        "Draw your signature" => "Dessinez votre signature",
        "It is kept on this computer only, ready to place on any page." => "Elle reste sur cet ordinateur, prête à être placée sur n'importe quelle page.",
        "Save signature" => "Enregistrer la signature",
        "Clear" => "Effacer",
        "Sign here with the mouse or a pen" => "Signez ici avec la souris ou un stylet",
        "Cancel" => "Annuler",
        "Settings" => "Paramètres",
        "Appearance" => "Apparence",
        "Theme" => "Thème",
        "System" => "Système",
        "Dark" => "Sombre",
        "Light" => "Clair",
        "Language" => "Langue",
        "Dark pages" => "Pages sombres",
        "Shows pages in dark colors (the file is not changed)." => "Affiche les pages en couleurs sombres (le fichier n'est pas modifié).",
        "Reading" => "Lecture",
        "Zoom when a file opens" => "Zoom à l'ouverture",
        "Fit width" => "Largeur",
        "Whole page" => "Page entière",
        "Reopen files where I left them" => "Rouvrir les fichiers là où je les ai laissés",
        "Remembers the last page read in each file." => "Retient la dernière page lue de chaque fichier.",
        "Signature and files" => "Signature et fichiers",
        "Your signature" => "Votre signature",
        "Saved on this computer only." => "Enregistrée sur cet ordinateur uniquement.",
        "None yet." => "Aucune pour l'instant.",
        "Forget" => "Oublier",
        "Draw…" => "Dessiner…",
        "Open PDF files with Plume PDF" => "Ouvrir les PDF avec Plume PDF",
        "Choose it in Windows' default apps." => "Choisissez-le dans les applications par défaut de Windows.",
        "Default apps…" => "Applications par défaut…",
        "Nothing leaves your computer." => "Rien ne quitte votre ordinateur.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_are_filled_in_order() {
        assert_eq!(fill("{i} of {n}", &["2", "9"]), "2 of 9");
        assert_eq!(fill("no holes", &["x"]), "no holes");
    }

    /// Every `tr("…")` of the interface has its French.
    #[test]
    fn every_text_has_its_french() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        let mut files = vec![dir.join("ui").join("mod.rs"), dir.join("ui").join("paint.rs"), dir.join("ui").join("settings_view.rs")];
        files.retain(|f| f.exists());
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            for part in src.split("tr(\"").skip(1) {
                let Some(end) = part.find("\")") else { continue };
                let s = &part[..end];
                if !s.contains('\\') && fr(s).is_none() {
                    missing.push(s.to_string());
                }
            }
        }
        assert!(missing.is_empty(), "no French for: {missing:#?}");
    }
}
