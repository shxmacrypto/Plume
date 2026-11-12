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

pub const MONTHS_EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
pub const MONTHS_FR: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];

/// "November 2026" / "Novembre 2026".
pub fn month_year(year: i64, month: u32) -> String {
    let i = (month.clamp(1, 12) - 1) as usize;
    if french() {
        let m = MONTHS_FR[i];
        let mut c = m.chars();
        let cap: String = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
        format!("{cap} {year}")
    } else {
        format!("{} {year}", MONTHS_EN[i])
    }
}

/// "14 November 2026, 18:05" / "14 novembre 2026, 18:05".
pub fn date_time(year: i64, month: u32, day: u32, h: u32, m: u32) -> String {
    let i = (month.clamp(1, 12) - 1) as usize;
    let name = if french() { MONTHS_FR[i] } else { MONTHS_EN[i] };
    format!("{day} {name} {year}, {h:02}:{m:02}")
}

pub fn tr(en: &'static str) -> &'static str {
    if !french() {
        return en;
    }
    match en {
        "Timeline" => "Chronologie",
        "Videos" => "Vidéos",
        "Folders" => "Dossiers",
        "Favorites" => "Favoris",
        "Settings" => "Paramètres",
        "Add a folder" => "Ajouter un dossier",
        "Library" => "Bibliothèque",
        "Photos and videos" => "Photos et vidéos",
        "Looking for photos and videos…" => "Recherche des photos et vidéos…",
        "No photos or videos yet" => "Pas encore de photos ni de vidéos",
        "Add a folder to see its photos and videos here." => "Ajoutez un dossier pour voir ici ses photos et vidéos.",
        "No favorites yet" => "Pas encore de favoris",
        "Press the star on a photo or a video to find it here." => "Appuyez sur l'étoile d'une photo ou d'une vidéo pour la retrouver ici.",
        "No videos yet" => "Pas encore de vidéos",
        "{n} items" => "{n} éléments",
        "1 item" => "1 élément",
        "{n} folders" => "{n} dossiers",
        "1 folder" => "1 dossier",
        "Back" => "Retour",
        "Previous" => "Précédent",
        "Next" => "Suivant",
        "Zoom in" => "Agrandir",
        "Zoom out" => "Réduire",
        "Rotate left" => "Pivoter à gauche",
        "Rotate right" => "Pivoter à droite",
        "Favorite" => "Favori",
        "Info" => "Infos",
        "Slideshow" => "Diaporama",
        "Full screen" => "Plein écran",
        "Exit full screen" => "Quitter le plein écran",
        "Play" => "Lecture",
        "Pause" => "Pause",
        "Mute" => "Couper le son",
        "Volume" => "Volume",
        "Speed" => "Vitesse",
        "Normal" => "Normale",
        "Subtitles" => "Sous-titres",
        "Off" => "Désactivés",
        "Audio track" => "Piste audio",
        "Track" => "Piste",
        "Loading…" => "Chargement…",
        "This picture can't be opened." => "Impossible d'ouvrir cette image.",
        "This video can't be played." => "Impossible de lire cette vidéo.",
        "Windows has no codec for this format. HEIC and RAW need the free extensions from the Microsoft Store." => {
            "Windows n'a pas de codec pour ce format. HEIC et RAW demandent les extensions gratuites du Microsoft Store."
        }
        "File" => "Fichier",
        "Folder" => "Dossier",
        "Dimensions" => "Dimensions",
        "Size" => "Taille",
        "Taken" => "Prise le",
        "Modified" => "Modifié le",
        "Camera" => "Appareil",
        "Lens" => "Objectif",
        "Exposure" => "Exposition",
        "Location" => "Lieu",
        "Duration" => "Durée",
        "Audio tracks" => "Pistes audio",
        "Flash fired" => "Flash déclenché",
        "No flash" => "Sans flash",
        "Rotated" => "Pivotée",
        "Rotated in Plume Media only: this format can't be rewritten without loss." => {
            "Pivotée dans Plume Media seulement : ce format ne peut pas être réécrit sans perte."
        }
        "Couldn't rotate the file." => "Impossible de pivoter le fichier.",
        "Added to favorites" => "Ajoutée aux favoris",
        "Removed from favorites" => "Retirée des favoris",
        "Resuming at {t}" => "Reprise à {t}",
        "Start over" => "Recommencer",
        "Open" => "Ouvrir",
        "Show in File Explorer" => "Afficher dans l'Explorateur",
        "Add to favorites" => "Ajouter aux favoris",
        "Remove from favorites" => "Retirer des favoris",
        "Appearance" => "Apparence",
        "Theme" => "Thème",
        "System" => "Système",
        "Dark" => "Sombre",
        "Light" => "Clair",
        "Language" => "Langue",
        "Library folders" => "Dossiers de la bibliothèque",
        "Their subfolders are included." => "Leurs sous-dossiers sont inclus.",
        "Remove" => "Retirer",
        "Photos" => "Photos",
        "Opening zoom" => "Zoom à l'ouverture",
        "Fit" => "Ajuster",
        "Fill" => "Remplir",
        "Actual size" => "Taille réelle",
        "Fit: whole picture, small ones are not enlarged. Fill: whole picture, as large as the window." => {
            "Ajuster : l'image entière, les petites ne sont pas agrandies. Remplir : l'image entière, aussi grande que la fenêtre."
        }
        "Slideshow speed" => "Vitesse du diaporama",
        "{n} s per photo" => "{n} s par photo",
        "Thumbnail size" => "Taille des vignettes",
        "Small" => "Petites",
        "Medium" => "Moyennes",
        "Large" => "Grandes",
        "Open with" => "Ouvrir avec",
        "Offer Plume Media in Windows' \"Open with\" menu for photos and videos." => {
            "Proposer Plume Media dans le menu « Ouvrir avec » de Windows pour les photos et vidéos."
        }
        "Add to \"Open with\"" => "Ajouter à « Ouvrir avec »",
        "Remove from \"Open with\"" => "Retirer de « Ouvrir avec »",
        "Added to \"Open with\"" => "Ajouté à « Ouvrir avec »",
        "Removed from \"Open with\"" => "Retiré de « Ouvrir avec »",
        "Thumbnails" => "Vignettes",
        "{n} on disk" => "{n} sur le disque",
        "Clear" => "Vider",
        "Thumbnails cleared" => "Vignettes effacées",
        "Rescan" => "Actualiser",
        "Keyboard shortcuts" => "Raccourcis clavier",
        "Photos: ← → previous / next, + − zoom, 0 fit, 1 actual size, R rotate, I info, F full screen, S slideshow, Ctrl+D favorite." => {
            "Photos : ← → précédente / suivante, + − zoom, 0 ajuster, 1 taille réelle, R pivoter, I infos, F plein écran, S diaporama, Ctrl+D favori."
        }
        "Videos: Space play / pause, ← → 5 s, ↑ ↓ volume, M mute, F full screen, C subtitles, [ ] speed, Page Up / Down previous / next." => {
            "Vidéos : Espace lecture / pause, ← → 5 s, ↑ ↓ volume, M couper le son, F plein écran, C sous-titres, [ ] vitesse, Page préc. / suiv. précédente / suivante."
        }
        "Nothing goes online: your photos and videos stay on this PC." => "Rien ne passe par Internet : vos photos et vidéos restent sur ce PC.",
        "About" => "À propos",
        "{a} of {b}" => "{a} sur {b}",
        "Slideshow: Esc to stop" => "Diaporama : Échap pour arrêter",
        "Settings saved" => "Paramètres enregistrés",
        "Folder added" => "Dossier ajouté",
        "Already in the library" => "Déjà dans la bibliothèque",
        "No library folder yet" => "Aucun dossier pour l'instant",
        "Pinned" => "Épinglés",
        "Pin a folder" => "Épingler un dossier",
        "Drop to pin" => "Déposer pour épingler",
        "Pin to sidebar" => "Épingler dans la barre latérale",
        "Unpin from sidebar" => "Retirer de la barre latérale",
        "Pin its folder to the sidebar" => "Épingler son dossier dans la barre latérale",
        "Folder pinned" => "Dossier épinglé",
        "{n} folders pinned" => "{n} dossiers épinglés",
        "Already pinned" => "Déjà épinglé",
        "Unpinned" => "Retiré de la barre latérale",
        "Rename" => "Renommer",
        "Move up" => "Monter",
        "Move down" => "Descendre",
        "All" => "Tout",
        "Date taken" => "Date de prise de vue",
        "Date modified" => "Date de modification",
        "Include subfolders" => "Inclure les sous-dossiers",
        "Show all" => "Tout afficher",
        "Nothing to show here" => "Rien à afficher ici",
        "This folder has no photos or videos." => "Ce dossier ne contient ni photo ni vidéo.",
        "Nothing matches this filter in this folder." => "Rien ne correspond à ce filtre dans ce dossier.",
        "Open in Plume Cut" => "Ouvrir dans Plume Cut",
        _ => en,
    }
}

/// Tests that change the language take this first (the language is global).
#[cfg(test)]
pub static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_and_months() {
        let _l = TEST_LOCK.lock();
        assert_eq!(fill("{a} of {b}", &["3", "10"]), "3 of 10");
        set("fr");
        assert_eq!(month_year(2026, 8), "Août 2026");
        assert_eq!(tr("Videos"), "Vidéos");
        set("en");
        assert_eq!(month_year(2026, 11), "November 2026");
    }
}
