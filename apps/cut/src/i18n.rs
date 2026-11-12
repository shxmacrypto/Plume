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

/// `{}` holes filled in order.
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
    if french() { french_of(en).unwrap_or(en) } else { en }
}

/// A message made elsewhere (in English), translated when it is known.
pub fn tr_dyn(s: &str) -> &str {
    if french() { french_of(s).unwrap_or(s) } else { s }
}

fn french_of(en: &str) -> Option<&'static str> {
    Some(match en {
        "Project" => "Projet",
        "Cannot create the project" => "Impossible de créer le projet",
        "{} file(s) of this project cannot be found" => "{} fichier(s) de ce projet sont introuvables",
        "Cannot open the project" => "Impossible d'ouvrir le projet",
        "Open a project" => "Ouvrir un projet",
        "Nothing to split under the playhead" => "Rien à couper sous la tête de lecture",
        "Gap closed" => "Espace refermé",
        "Select the clip a transition leads into" => "Sélectionnez le clip où mène la transition",
        "Plume Cut needs FFmpeg to read videos: install it first" => "Plume Cut a besoin de FFmpeg pour lire les vidéos : installez-le d'abord",
        "Import media" => "Importer des médias",
        "Nothing to play" => "Rien à lire",
        "The timeline is empty" => "La timeline est vide",
        "None found: the processor encodes" => "Aucune trouvée : le processeur encode",
        "Looking…" => "Recherche…",
        "Saved" => "Enregistré",
        "Your text" => "Votre texte",
        "Snapping on" => "Magnétisme activé",
        "Snapping off" => "Magnétisme désactivé",
        "Title" => "Titre",
        "Subtitle" => "Sous-titre",
        "Caption on a box" => "Légende sur un fond",
        "Your caption" => "Votre légende",
        "Big and bold" => "Grand et gras",
        "WOW" => "WOW",
        "Main" => "Vidéo",
        "Overlay" => "Calque",
        "Text" => "Texte",
        "Sound" => "Son",
        "Music" => "Musique",
        "Export" => "Exporter",
        "Cut, arrange and export your videos, on this PC." => "Coupez, assemblez et exportez vos vidéos, sur ce PC.",
        "New project" => "Nouveau projet",
        "Open project" => "Ouvrir un projet",
        "Recent projects" => "Projets récents",
        "No projects yet" => "Aucun projet pour l'instant",
        "Start a new one, or drop videos on this window." => "Créez-en un, ou déposez des vidéos sur cette fenêtre.",
        "Plume Cut reads and exports videos with FFmpeg (about 100 MB, downloaded once)." => {
            "Plume Cut lit et exporte les vidéos avec FFmpeg (environ 100 Mo, téléchargé une fois)."
        }
        "Downloading FFmpeg… {}%" => "Téléchargement de FFmpeg… {} %",
        "FFmpeg could not be installed" => "FFmpeg n'a pas pu être installé",
        "Install FFmpeg" => "Installer FFmpeg",
        "Media" => "Médias",
        "Transitions" => "Transitions",
        "No media yet" => "Aucun média pour l'instant",
        "Import videos, pictures and music, or drop them here." => "Importez des vidéos, des photos et de la musique, ou déposez-les ici.",
        "Import" => "Importer",
        "Reading {} file(s)…" => "Lecture de {} fichier(s)…",
        "missing" => "introuvable",
        "Click to add a text at the playhead." => "Cliquez pour ajouter un texte à la tête de lecture.",
        "Select a clip that follows another, then pick how it begins." => "Sélectionnez un clip qui en suit un autre, puis choisissez comment il commence.",
        "Cut" => "Cut",
        "Crossfade" => "Fondu enchaîné",
        "Dip to black" => "Passage au noir",
        "Add clips to the timeline to see them here" => "Ajoutez des clips à la timeline pour les voir ici",
        "Exporting…" => "Exportation…",
        "about {} left" => "encore {} environ",
        "Cancel" => "Annuler",
        "Your video is ready" => "Votre vidéo est prête",
        "Done" => "Terminé",
        "Show in folder" => "Afficher dans le dossier",
        "The export failed" => "L'exportation a échoué",
        "Resolution" => "Résolution",
        "Format" => "Format",
        "16:9 YouTube" => "16:9 YouTube",
        "9:16 TikTok, Reels" => "9:16 TikTok, Reels",
        "1:1 Square" => "1:1 Carré",
        "Video codec" => "Codec vidéo",
        "H.264 (everywhere)" => "H.264 (partout)",
        "Use the graphics card" => "Utiliser la carte graphique",
        "Change" => "Changer",
        "Keyboard shortcuts" => "Raccourcis clavier",
        "Play / pause" => "Lecture / pause",
        "Backwards, stop, forwards (again: faster)" => "Arrière, stop, avant (encore : plus vite)",
        "One frame (Shift: one second)" => "Une image (Maj : une seconde)",
        "Previous / next cut" => "Coupe précédente / suivante",
        "Start / end" => "Début / fin",
        "Split at the playhead" => "Couper à la tête de lecture",
        "Delete the selected clips" => "Supprimer les clips sélectionnés",
        "Delete and close the gap" => "Supprimer et refermer l'espace",
        "Select several clips" => "Sélectionner plusieurs clips",
        "Select everything" => "Tout sélectionner",
        "Undo" => "Annuler",
        "Redo" => "Rétablir",
        "Add a text" => "Ajouter un texte",
        "Snapping on or off" => "Magnétisme activé ou non",
        "Zoom the timeline" => "Zoomer la timeline",
        "Show the whole timeline" => "Voir toute la timeline",
        "Zoom around the mouse" => "Zoomer autour de la souris",
        "Save now (saving is automatic)" => "Enregistrer (l'enregistrement est automatique)",
        "New project / open a project" => "Nouveau projet / ouvrir un projet",
        "Clear the selection, close a dialog" => "Désélectionner, fermer une fenêtre",
        "Close" => "Fermer",
        "Settings" => "Réglages",
        "Appearance" => "Apparence",
        "Like Windows" => "Comme Windows",
        "Dark" => "Sombre",
        "Light" => "Clair",
        "Language" => "Langue",
        "Export with the graphics card" => "Exporter avec la carte graphique",
        "FFmpeg is not installed yet" => "FFmpeg n'est pas encore installé",
        "Exports go to" => "Les exports vont dans",
        "just now" => "à l'instant",
        "{} min ago" => "il y a {} min",
        "{} h ago" => "il y a {} h",
        "{} days ago" => "il y a {} jours",
        "Size" => "Taille",
        "Left / right" => "Gauche / droite",
        "Up / down" => "Haut / bas",
        "Rotation" => "Rotation",
        "Opacity" => "Opacité",
        "Crop left" => "Rogner à gauche",
        "Crop top" => "Rogner en haut",
        "Crop right" => "Rogner à droite",
        "Crop bottom" => "Rogner en bas",
        "Volume" => "Volume",
        "Fade in" => "Fondu d'entrée",
        "Fade out" => "Fondu de sortie",
        "Speed" => "Vitesse",
        "Brightness" => "Luminosité",
        "Contrast" => "Contraste",
        "Saturation" => "Saturation",
        "Box opacity" => "Opacité du fond",
        "Animation length" => "Durée de l'animation",
        "Transition length" => "Durée de la transition",
        "Picture" => "Image",
        "Colour" => "Couleur",
        "Style" => "Style",
        "Motion" => "Animation",
        "{} clips selected" => "{} clips sélectionnés",
        "Drag them together on the timeline, or delete them." => "Déplacez-les ensemble sur la timeline, ou supprimez-les.",
        "Delete" => "Supprimer",
        "Delete and close the gaps" => "Supprimer et refermer les espaces",
        "Fit" => "Ajuster",
        "Fill" => "Remplir",
        "Reset" => "Réinitialiser",
        "Crop" => "Rogner",
        "Lower the music under voices" => "Baisser la musique sous les voix",
        "While other sounds play, the music gets quieter by itself." => "Pendant que d'autres sons jouent, la musique baisse d'elle-même.",
        "The clip keeps the same moment of the video: faster makes it shorter." => "Le clip garde le même moment de la vidéo : plus vite le rend plus court.",
        "Font" => "Police",
        "Bold" => "Gras",
        "Box behind the text" => "Fond derrière le texte",
        "Position" => "Position",
        "Centre" => "Centrer",
        "None" => "Aucune",
        "Fade" => "Fondu",
        "Rise" => "Montée",
        "Slide" => "Glissement",
        "Coming in" => "À l'entrée",
        "Going out" => "À la sortie",
        "Transition in" => "Transition d'entrée",
        "Place this clip right after another one to add a transition." => "Placez ce clip juste après un autre pour ajouter une transition.",
        "{} clips" => "{} clips",
        "Name" => "Nom",
        "Frame" => "Cadre",
        "Frames per second" => "Images par seconde",
        "Select a clip on the timeline to change it. Projects save by themselves." => {
            "Sélectionnez un clip sur la timeline pour le modifier. Les projets s'enregistrent tout seuls."
        }
        "Drag media here, or click + on a clip of the bin" => "Glissez des médias ici, ou cliquez sur + d'un média de la bibliothèque",
        "Save exports in" => "Enregistrer les exports dans",
        "No room there" => "Pas de place ici",
        "Unknown media" => "Média inconnu",
        "Unknown track" => "Piste inconnue",
        "That track does not take this kind of file" => "Cette piste ne prend pas ce type de fichier",
        "No track for it" => "Aucune piste pour lui",
        "That is not a sound file" => "Ce n'est pas un fichier son",
        "That track does not take this kind of clip" => "Cette piste ne prend pas ce type de clip",
        "A transition needs a clip right before this one" => "Une transition a besoin d'un clip juste avant celui-ci",
        "Graphics card encoder failed: using the processor" => "L'encodeur de la carte graphique a échoué : le processeur prend le relais",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_has_its_french() {
        let mut missing = Vec::new();
        for src in [
            include_str!("ui/mod.rs"),
            include_str!("ui/panels.rs"),
            include_str!("ui/props.rs"),
            include_str!("ui/timeline.rs"),
            include_str!("model.rs"),
            include_str!("export.rs"),
        ] {
            let mut rest = src;
            while let Some(i) = rest.find("tr(\"") {
                let word = i > 0 && (rest.as_bytes()[i - 1].is_ascii_alphanumeric() || rest.as_bytes()[i - 1] == b'_');
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
        // the refusals of the model and the notes of the export reach the screen through tr / tr_dyn
        for s in [
            crate::model::NO_ROOM,
            "Graphics card encoder failed: using the processor",
            "Unknown media",
            "Unknown track",
            "That track does not take this kind of file",
            "No track for it",
            "That is not a sound file",
            "That track does not take this kind of clip",
            "A transition needs a clip right before this one",
        ] {
            if french_of(s).is_none() {
                missing.push(s.to_string());
            }
        }
        assert!(missing.is_empty(), "no French for {missing:?}");
        assert_eq!(fill("{} of {}", &["1", "2"]), "1 of 2");
    }
}
