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

/// "Label: text", with the space French puts before the colon.
pub fn colon(a: &str, b: &str) -> String {
    if french() { format!("{a} : {b}") } else { format!("{a}: {b}") }
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

/// A message made at run time (by the worker): its French when it is a known one. "Pass 2 of 2" is built here.
pub fn tr_known(en: &str) -> Option<&'static str> {
    if !french() {
        return None;
    }
    if let Some(rest) = en.strip_prefix("Pass ") {
        return match rest {
            "1 of 2" => Some("Passe 1 sur 2"),
            "2 of 2" => Some("Passe 2 sur 2"),
            _ => None,
        };
    }
    french_of(en)
}

fn french_of(en: &str) -> Option<&'static str> {
    Some(match en {
        // presets
        "For phone" => "Pour téléphone",
        "For Discord" => "Pour Discord",
        "Smaller file" => "Fichier plus léger",
        "Audio only" => "Son seul",
        "GIF" => "GIF",
        "Images" => "Images",
        "Extract frames" => "Captures",
        "Trim" => "Couper",
        "Mute" => "Sans le son",
        "Rotate" => "Pivoter",
        "Advanced" => "Avancé",
        "Plays everywhere" => "Se lit partout",
        "Under the size limit" => "Sous la limite",
        "Fewer MB" => "Moins de Mo",
        "MP3, M4A, FLAC…" => "MP3, M4A, FLAC…",
        "Video or photos" => "Vidéo ou photos",
        "WebP, JPEG, PNG…" => "WebP, JPEG, PNG…",
        "Stills from a video" => "Images d'une vidéo",
        "Keep a part" => "Garder un passage",
        "Remove the sound" => "Retirer le son",
        "Turn the picture" => "Tourner l'image",
        "Pick every setting" => "Tous les réglages",
        // preset choices
        "Presets" => "Préréglages",
        "MP4 with H.264 and AAC, at most 1080p: plays on every phone, TV and browser." => {
            "MP4 en H.264 et AAC, 1080p au plus : se lit sur tous les téléphones, téléviseurs et navigateurs."
        }
        "Two passes find the quality that fills the size without going over." => "Deux passes trouvent la qualité qui remplit la taille sans la dépasser.",
        "Stay under" => "Rester sous",
        "H.264 at a lower quality and at most 1080p. Often a third of the size." => {
            "H.264 en qualité un peu plus basse, 1080p au plus. Souvent trois fois plus léger."
        }
        "Format" => "Format",
        "Frames per second" => "Images par seconde",
        "From a video" => "D'une vidéo",
        "From photos" => "De photos",
        "Trim the video first: GIFs grow fast." => "Coupez d'abord la vidéo : un GIF grossit vite.",
        "Each photo stays" => "Chaque photo dure",
        "Several photos" => "Plusieurs photos",
        "Plays" => "Lecture",
        "One animated GIF" => "Un GIF animé",
        "One GIF each" => "Un GIF par photo",
        "In a loop" => "En boucle",
        "Once" => "Une fois",
        "One photo makes a still GIF. Several play one after the other, in name order." => {
            "Une photo donne un GIF fixe. Plusieurs défilent l'une après l'autre, dans l'ordre des noms."
        }
        "Slideshow of {n} photos" => "Diaporama de {n} photos",
        "Quality" => "Qualité",
        "Longest side" => "Plus grand côté",
        "Take" => "Prendre",
        "As" => "En",
        "The pictures go in a new folder next to the video." => "Les images vont dans un nouveau dossier à côté de la vidéo.",
        "Start" => "Début",
        "End" => "Fin",
        "The end" => "La fin",
        "Keeps {t}" => "Garde {t}",
        "Keeps everything after the start" => "Garde tout après le début",
        "Cut" => "Coupe",
        "Exact" => "Exacte",
        "Fast, no re-encode" => "Rapide, sans réencoder",
        "Same picture, copied as it is, without the sound. Takes a second." => "La même image, copiée telle quelle, sans le son. Une seconde suffit.",
        "Turn" => "Tourner",
        "90° right" => "90° à droite",
        "90° left" => "90° à gauche",
        "Container" => "Conteneur",
        "Video codec" => "Codec vidéo",
        "Audio codec" => "Codec audio",
        "Resolution" => "Résolution",
        "Frame rate" => "Images par seconde",
        "Video size by" => "Taille de la vidéo par",
        "Bitrate" => "Débit",
        "Video bitrate" => "Débit vidéo",
        "Audio bitrate" => "Débit audio",
        "Sample rate" => "Fréquence",
        "High" => "Haute",
        "Good" => "Bonne",
        "Smaller" => "Plus léger",
        "Smallest" => "Le plus léger",
        "Same" => "Identique",
        "Copy" => "Copier",
        "None" => "Aucun",
        "Every frame" => "Chaque image",
        "10 a second" => "10 par seconde",
        "1 a second" => "1 par seconde",
        "Every 5 s" => "Toutes les 5 s",
        "Every 10 s" => "Toutes les 10 s",
        "Graphics card encoding" => "Encodage par la carte graphique",
        "H.264, H.265 and AV1, when the PC has one" => "H.264, H.265 et AV1, si le PC en a une",
        "None found: the processor does it" => "Aucune trouvée : le processeur s'en charge",
        "Saved in the chosen folder" => "Enregistré dans le dossier choisi",
        "Saved next to the original" => "Enregistré à côté de l'original",
        // drop zone and queue
        "Add files" => "Ajouter des fichiers",
        "Drop files here" => "Déposez vos fichiers ici",
        "Videos, sounds and pictures. Nothing leaves this PC." => "Vidéos, sons et images. Rien ne quitte ce PC.",
        "Drop more files here" => "Déposez d'autres fichiers ici",
        "or click to choose them" => "ou cliquez pour les choisir",
        "Queue · {n}" => "File d'attente · {n}",
        "Clear finished" => "Effacer les terminés",
        "Ready" => "Prêt",
        "Waiting" => "En attente",
        "Waiting for FFmpeg" => "En attente de FFmpeg",
        "{t} left" => "encore {t}",
        "Done" => "Terminé",
        "{n} pictures" => "{n} images",
        "Failed" => "Échec",
        "Cancelled" => "Annulé",
        "Save to" => "Vers",
        "Next to the original" => "À côté de l'original",
        "Convert" => "Convertir",
        "Convert {n} files" => "Convertir {n} fichiers",
        "Cancel all" => "Tout annuler",
        "Downloading FFmpeg… {n} %" => "Téléchargement de FFmpeg… {n} %",
        "FFmpeg is ready" => "FFmpeg est prêt",
        "FFmpeg could not be downloaded" => "FFmpeg n'a pas pu être téléchargé",
        "Save results in" => "Enregistrer les résultats dans",
        "Convert with Plume" => "Convertir avec Plume",
        // settings
        "Settings" => "Paramètres",
        "Appearance" => "Apparence",
        "Theme" => "Thème",
        "System" => "Système",
        "Dark" => "Sombre",
        "Light" => "Clair",
        "Language" => "Langue",
        "Converting" => "Conversion",
        "Save results" => "Enregistrer les résultats",
        "Choose a folder…" => "Choisir un dossier…",
        "Keep metadata" => "Garder les métadonnées",
        "Title, date, place and chapters of videos and sounds." => "Titre, date, lieu et chapitres des vidéos et des sons.",
        "Off: the processor does everything." => "Désactivé : le processeur fait tout.",
        "Looking for the graphics card's encoder…" => "Recherche de l'encodeur de la carte graphique…",
        "Found: {name}. Falls back to the processor if it fails." => "Trouvé : {name}. Le processeur prend le relais en cas d'échec.",
        "Faster H.264, H.265 and AV1 with NVIDIA, Intel or AMD." => "H.264, H.265 et AV1 plus rapides avec NVIDIA, Intel ou AMD.",
        "Explorer menu" => "Menu de l'Explorateur",
        "Adds \"Convert with Plume\" when you right-click a file." => "Ajoute « Convertir avec Plume » au clic droit sur un fichier.",
        "Installed in {path}" => "Installé dans {path}",
        "Downloaded the first time a video or a sound is converted (about 100 MB)." => {
            "Téléchargé à la première conversion d'une vidéo ou d'un son (environ 100 Mo)."
        }
        "Video and sound engine" => "Moteur vidéo et son",
        "Download now" => "Télécharger maintenant",
        "Plume Convert works on this PC: your files are never uploaded. The only download is FFmpeg, from gyan.dev." => {
            "Plume Convert travaille sur ce PC : vos fichiers ne sont jamais envoyés. Le seul téléchargement est FFmpeg, depuis gyan.dev."
        }
        // messages of the worker
        "A little too big: trying again smaller" => "Un peu trop gros : nouvel essai plus léger",
        "Graphics card encoder failed: using the processor" => "L'encodeur de la carte graphique a échoué : le processeur prend le relais",
        "FFmpeg is needed for WebP" => "FFmpeg est nécessaire pour le WebP",
        "No picture or sound found in this file" => "Ni image ni son dans ce fichier",
        "Not a video or audio file" => "Ce n'est ni une vidéo ni un son",
        "The end must come after the start" => "La fin doit venir après le début",
        "The start is after the end of the file" => "Le début est après la fin du fichier",
        "The video's length is unknown" => "La durée de la vidéo est inconnue",
        "This file has no sound" => "Ce fichier n'a pas de son",
        "This is a picture: use the Images or GIF preset" => "C'est une image : utilisez le préréglage Images ou GIF",
        "This needs a video" => "Il faut une vidéo",
        "Too long to fit in that size" => "Trop long pour tenir dans cette taille",
        "WebM needs Opus sound" => "Le WebM demande un son en Opus",
        "WebM needs VP9 or AV1" => "Le WebM demande du VP9 ou de l'AV1",
        "Windows cannot read this picture (its codec may be missing)" => "Windows ne sait pas lire cette image (son codec manque peut-être)",
        "FFmpeg stopped with an error" => "FFmpeg s'est arrêté sur une erreur",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes() {
        assert_eq!(fill("Convert {n} files", &["3"]), "Convert 3 files");
        assert_eq!(fill("{a} and {b}", &["x"]), "x and ");
    }

    #[test]
    fn every_ui_string_has_its_french() {
        // all the English the window shows through tr(), read from the sources
        let mut missing = Vec::new();
        for src in [
            include_str!("ui/paint.rs"),
            include_str!("ui/settings_view.rs"),
            include_str!("ui/choices.rs"),
            include_str!("ui/mod.rs"),
            include_str!("opts.rs"),
        ] {
            let mut rest = src;
            while let Some(i) = rest.find("tr(\"") {
                // not push_str("…")
                let word = i > 0 && rest.as_bytes()[i - 1].is_ascii_alphanumeric() || i > 0 && rest.as_bytes()[i - 1] == b'_';
                rest = &rest[i + 4..];
                if word {
                    continue;
                }
                let end = rest.find("\")").unwrap_or(0);
                let s = rest[..end].replace("\\\"", "\"");
                if french_of(&s).is_none() {
                    missing.push(s);
                }
            }
        }
        for p in crate::opts::Preset::ALL {
            for s in [p.name(), p.sub()] {
                if french_of(s).is_none() {
                    missing.push(s.to_string());
                }
            }
        }
        assert!(missing.is_empty(), "no French for {missing:?}");
    }
}
