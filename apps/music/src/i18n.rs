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
    match en {
        "Home" => "Accueil",
        "Search" => "Rechercher",
        "Library" => "Bibliothèque",
        "Songs" => "Titres",
        "Albums" => "Albums",
        "Artists" => "Artistes",
        "Liked songs" => "Titres likés",
        "Downloads" => "Téléchargements",
        "Playlists" => "Playlists",
        "New playlist" => "Nouvelle playlist",
        "Import a playlist" => "Importer une playlist",
        "Import" => "Importer",
        "Settings" => "Paramètres",
        "Good morning" => "Bonjour",
        "Good afternoon" => "Bon après-midi",
        "Good evening" => "Bonsoir",
        "Recently played" => "Écoutés récemment",
        "Recently added" => "Ajoutés récemment",
        "Your playlists" => "Vos playlists",
        "Made for you" => "Pour vous",
        "Start listening" => "Commencez à écouter",
        "Search for a song, an artist or an album, or import a playlist from Spotify, Apple Music, Deezer or SoundCloud." => {
            "Cherchez un titre, un artiste ou un album, ou importez une playlist de Spotify, Apple Music, Deezer ou SoundCloud."
        }
        "What do you want to listen to?" => "Que voulez-vous écouter ?",
        "On YouTube Music" => "Sur YouTube Music",
        "In your library" => "Dans votre bibliothèque",
        "Recent searches" => "Recherches récentes",
        "No results" => "Aucun résultat",
        "Searching…" => "Recherche…",
        "Play" => "Lecture",
        "Pause" => "Pause",
        "Shuffle" => "Aléatoire",
        "Repeat" => "Répéter",
        "Next" => "Suivant",
        "Previous" => "Précédent",
        "Lyrics" => "Paroles",
        "Queue" => "File d'attente",
        "Now playing" => "En cours",
        "Next up" => "À suivre",
        "Download" => "Télécharger",
        "Downloaded" => "Téléchargé",
        "Downloading…" => "Téléchargement…",
        "Download all" => "Tout télécharger",
        "Like" => "J'aime",
        "Play next" => "Lire ensuite",
        "Add to queue" => "Ajouter à la file",
        "Add to playlist" => "Ajouter à une playlist",
        "Remove from playlist" => "Retirer de la playlist",
        "Go to album" => "Voir l'album",
        "Go to artist" => "Voir l'artiste",
        "Rename" => "Renommer",
        "Delete playlist" => "Supprimer la playlist",
        "Show in folder" => "Afficher dans le dossier",
        "Copy link" => "Copier le lien",
        "Song radio" => "Radio du titre",
        "{n} songs" => "{n} titres",
        "1 song" => "1 titre",
        "Playlist" => "Playlist",
        "Album" => "Album",
        "Artist" => "Artiste",
        "Title" => "Titre",
        "Duration" => "Durée",
        "No lyrics for this song." => "Pas de paroles pour ce titre.",
        "Looking for lyrics…" => "Recherche des paroles…",
        "Lyrics from {s}" => "Paroles : {s}",
        "Nothing is playing" => "Rien en cours",
        "The queue is empty." => "La file d'attente est vide.",
        "Paste a link to a playlist, an album or a song from Spotify, Apple Music, SoundCloud or YouTube." => {
            "Collez le lien d'une playlist, d'un album ou d'un titre de Spotify, Apple Music, SoundCloud ou YouTube."
        }
        "Reading the link…" => "Lecture du lien…",
        "Finding the songs on YouTube Music: {a} of {b}" => "Recherche des titres sur YouTube Music : {a} sur {b}",
        "Imported: {a} songs, {b} not found" => "Importée : {a} titres, {b} introuvables",
        "Imported: {a} songs" => "Importée : {a} titres",
        "Paste copied songs" => "Coller les titres copiés",
        "Link: 100 songs max" => "Lien : 100 titres max",
        "Copy & paste: no limit" => "Copier-coller : sans limite",
        "Link: every song" => "Lien : tous les titres",
        "Spotify, every song: open the playlist in Spotify, press Ctrl+A then Ctrl+C, then click Paste copied songs." => {
            "Spotify, tous les titres : ouvrez la playlist dans Spotify, faites Ctrl+A puis Ctrl+C, puis cliquez sur Coller les titres copiés."
        }
        "Paste a link to a playlist, an album or a song from Spotify, Apple Music, Deezer, SoundCloud or YouTube." => {
            "Collez le lien d'une playlist, d'un album ou d'un titre de Spotify, Apple Music, Deezer, SoundCloud ou YouTube."
        }
        "A whole Spotify playlist: open it in Spotify, press Ctrl+A then Ctrl+C, then Paste copied songs." => {
            "Une playlist Spotify entière : ouvrez-la dans Spotify, faites Ctrl+A puis Ctrl+C, puis Coller les titres copiés."
        }
        "Songs from Spotify" => "Titres de Spotify",
        "For all of them: in Spotify, open the playlist, press Ctrl+A then Ctrl+C, then click Paste copied songs." => {
            "Pour tous les avoir : dans Spotify, ouvrez la playlist, faites Ctrl+A puis Ctrl+C, puis cliquez sur Coller les titres copiés."
        }
        "No Spotify songs were copied. In Spotify, open the playlist, press Ctrl+A then Ctrl+C, and try again." => {
            "Aucun titre Spotify copié. Dans Spotify, ouvrez la playlist, faites Ctrl+A puis Ctrl+C, et réessayez."
        }
        "Spotify only shows the first 100 songs of a shared link." => "Spotify ne montre que les 100 premiers titres d'un lien partagé.",
        "Cancel" => "Annuler",
        "Close" => "Fermer",
        "Done" => "Terminé",
        "Create" => "Créer",
        "Save" => "Enregistrer",
        "Name" => "Nom",
        "My playlist #{n}" => "Ma playlist n°{n}",
        "Plume Music needs yt-dlp to play songs from YouTube and SoundCloud (a free tool, 20 MB, shared with Plume Web)." => {
            "Plume Music a besoin de yt-dlp pour lire les titres de YouTube et SoundCloud (un outil libre, 20 Mo, partagé avec Plume Web)."
        }
        "Install yt-dlp" => "Installer yt-dlp",
        "Installing…" => "Installation…",
        "Only download music you have the right to keep." => "Ne téléchargez que la musique que vous avez le droit de garder.",
        "Could not play this song: {s}" => "Impossible de lire ce titre : {s}",
        "Could not find this song." => "Titre introuvable.",
        "Added to {s}" => "Ajouté à {s}",
        "Added to the queue" => "Ajouté à la file",
        "Saved to Downloads" => "Enregistré dans Téléchargements",
        "Playlist created" => "Playlist créée",
        "Copied" => "Copié",
        "Scanning your music…" => "Analyse de votre musique…",
        // settings
        "Appearance" => "Apparence",
        "Theme" => "Thème",
        "System" => "Système",
        "Dark" => "Sombre",
        "Light" => "Clair",
        "Accent color" => "Couleur d'accent",
        "From the cover" => "Celle de la pochette",
        "Now playing background" => "Fond de « En cours »",
        "Moving colors of the cover" => "Couleurs animées de la pochette",
        "Blurred cover" => "Pochette floutée",
        "Animated GIF" => "GIF animé",
        "Plain" => "Uni",
        "Choose a GIF…" => "Choisir un GIF…",
        "Also behind the whole window" => "Aussi derrière toute la fenêtre",
        "Animated album covers" => "Pochettes animées",
        "When Apple Music has a motion cover for the album (needs FFmpeg)." => "Quand Apple Music a une pochette animée pour l'album (nécessite FFmpeg).",
        "Reduce motion" => "Réduire les animations",
        "Playback" => "Lecture audio",
        "Crossfade" => "Fondu enchaîné",
        "Off" => "Désactivé",
        "Gapless playback" => "Lecture sans blanc",
        "Normalize volume" => "Normaliser le volume",
        "Uses the songs' ReplayGain when they have one." => "Utilise le ReplayGain des titres quand ils en ont un.",
        "Autoplay" => "Lecture automatique",
        "When the queue ends, keep playing similar songs." => "À la fin de la file, continuer avec des titres similaires.",
        "Allow explicit songs" => "Autoriser les titres explicites",
        "Equalizer" => "Égaliseur",
        "Preset" => "Préréglage",
        "Preamp" => "Préampli",
        "Downloads and storage" => "Téléchargements et stockage",
        "Format" => "Format",
        "M4A (original, no loss)" => "M4A (original, sans perte)",
        "MP3 320 kbps (FFmpeg)" => "MP3 320 kbit/s (FFmpeg)",
        "Download folder" => "Dossier de téléchargement",
        "Change…" => "Changer…",
        "Music folders" => "Dossiers de musique",
        "Add a folder…" => "Ajouter un dossier…",
        "Rescan" => "Réanalyser",
        "Cache for streamed songs" => "Cache des titres écoutés",
        "Clear" => "Vider",
        "Lyrics size" => "Taille des paroles",
        "Small" => "Petite",
        "Medium" => "Moyenne",
        "Large" => "Grande",
        "Language" => "Langue",
        "Tools" => "Outils",
        "Installed" => "Installé",
        "Not installed" => "Non installé",
        "Install FFmpeg" => "Installer FFmpeg",
        "Keyboard shortcuts" => "Raccourcis clavier",
        "Sleep timer" => "Minuteur de mise en veille",
        "Stop in {n} min" => "Arrêt dans {n} min",
        "End of song" => "Fin du titre",
        "Notifications" => "Notifications",
        "Show the song in Windows' media overlay and use the media keys." => {
            "Afficher le titre dans le panneau multimédia de Windows et utiliser les touches multimédia."
        }
        "About" => "À propos",
        "Update" => "Mettre à jour",
        "Explicit" => "Explicite",
        "Matching…" => "Correspondance…",
        "Not found on YouTube Music" => "Introuvable sur YouTube Music",
        "Volume" => "Volume",
        "Full screen" => "Plein écran",
        "Mini player" => "Mini lecteur",
        "Back" => "Retour",
        "Clear queue" => "Vider la file",
        "Radio" => "Radio",
        "Similar songs" => "Titres similaires",
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
}
