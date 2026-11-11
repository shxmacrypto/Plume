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

/// 123,456 (English) or 123 456 (French).
pub fn count(n: u64) -> String {
    let s = n.to_string();
    let sep = if french() { '\u{202F}' } else { ',' };
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}

/// How long ago a FILETIME was: "today", "3 days ago", "5 months ago", "2 years ago".
pub fn ago(ft: u64, now: u64) -> String {
    if ft == 0 {
        return String::new();
    }
    let days = now.saturating_sub(ft) / (86_400 * 10_000_000);
    let fr = french();
    let (n, unit_en, unit_fr) = match days {
        0 => return (if fr { "aujourd’hui" } else { "today" }).into(),
        1 => return (if fr { "hier" } else { "yesterday" }).into(),
        2..=60 => (days, "days", "jours"),
        61..=730 => (days / 30, "months", "mois"),
        _ => (days / 365, "years", "ans"),
    };
    if fr { format!("il y a {n} {unit_fr}") } else { format!("{n} {unit_en} ago") }
}

/// "1 item", "12 items".
pub fn items(n: u64) -> String {
    if n == 1 { tr("1 item").to_string() } else { fill(tr("{n} items"), &[&count(n)]) }
}

pub fn tr(en: &'static str) -> &'static str {
    if !french() {
        return en;
    }
    match en {
        "Drives" => "Lecteurs",
        "Map" => "Carte",
        "Cleanup" => "Nettoyage",
        "Settings" => "Réglages",
        "Local Disk" => "Disque local",
        "USB drive" => "Clé USB",
        "Test folder" => "Dossier d’essai",
        " · read in {t}" => " · lu en {t}",
        " · some folders could not be read" => " · certains dossiers n’ont pas pu être lus",
        "A folder" => "Un dossier",
        "A test build does not restart as administrator." => "Une version d’essai ne redémarre pas en administrateur.",
        "Biggest files" => "Plus gros fichiers",
        "Cancel" => "Annuler",
        "Changed" => "Modifié",
        "Choose a drive" => "Choisir un lecteur",
        "Choose a folder to scan" => "Choisir un dossier à analyser",
        "Choose a folder…" => "Choisir un dossier…",
        "Clean now?" => "Nettoyer maintenant ?",
        "Clean selected" => "Nettoyer la sélection",
        "Clean" => "Nettoyer",
        "Cleaning…" => "Nettoyage…",
        "Close {name} first" => "Fermez d’abord {name}",
        "Comparing files… {size} read" => "Comparaison des fichiers… {size} lus",
        "Copy path" => "Copier le chemin",
        "Dark" => "Sombre",
        "Done" => "Terminé",
        "Empty folder" => "Dossier vide",
        "Empty the Recycle Bin to get the space back." => "Videz la Corbeille pour récupérer la place.",
        "Everything else goes to the Recycle Bin: you can still get it back." => "Tout le reste va dans la Corbeille : vous pouvez encore le récupérer.",
        "Explorer" => "Explorateur",
        "Files are removed for good (the Recycle Bin is off in the settings)." => {
            "Les fichiers sont supprimés définitivement (la Corbeille est désactivée dans les réglages)."
        }
        "Find duplicates" => "Chercher les doublons",
        "Free up space" => "Libérer de la place",
        "In the Recycle Bin you can still get them back until you empty it." => {
            "Dans la Corbeille, vous pouvez encore les récupérer tant que vous ne la videz pas."
        }
        "In this folder" => "Dans ce dossier",
        "Language" => "Langue",
        "Large files and duplicates" => "Gros fichiers et doublons",
        "Large files from, and duplicates looked for from:" => "Gros fichiers à partir de, et doublons cherchés à partir de :",
        "Last scan: {path}, {size}" => "Dernière analyse : {path}, {size}",
        "Light" => "Clair",
        "Looking at temporary files, caches and downloads…" => "Examen des fichiers temporaires, des caches et des téléchargements…",
        "Measuring…" => "Mesure…",
        "Name" => "Nom",
        "Needs administrator" => "Droits d’administrateur requis",
        "Not touched for at least:" => "Pas ouverts depuis au moins :",
        "Nothing goes online. Scans stay on this PC and are not saved." => {
            "Rien ne passe par Internet. Les analyses restent sur ce PC et ne sont pas enregistrées."
        }
        "Nothing here." => "Rien ici.",
        "Nothing to clean" => "Rien à nettoyer",
        "OK" => "OK",
        "Old downloads" => "Anciens téléchargements",
        "Open here" => "Ouvrir ici",
        "Open in Plume FEx" => "Ouvrir dans Plume FEx",
        "Open the map" => "Ouvrir la carte",
        "Open" => "Ouvrir",
        "Path copied." => "Chemin copié.",
        "Pick a drive to see what fills it. Nothing is changed by a scan." => "Choisissez un lecteur pour voir ce qui le remplit. Une analyse ne change rien.",
        "Plume Clean closes and opens again; Windows asks you first." => "Plume Clean se ferme et se rouvre ; Windows vous demande d’abord.",
        "Plume FEx is not installed. Install it from Plume Suite." => "Plume FEx n’est pas installé. Installez-le depuis Plume Suite.",
        "Plume FEx" => "Plume FEx",
        "Refresh" => "Actualiser",
        "Removed files" => "Fichiers supprimés",
        "Rescan" => "Analyser à nouveau",
        "Restart as administrator" => "Redémarrer en administrateur",
        "Restart as administrator?" => "Redémarrer en administrateur ?",
        "Restart" => "Redémarrer",
        "Review" => "Choisir",
        "Scan a drive first" => "Analysez d’abord un lecteur",
        "Scan a drive or a folder to see what fills it." => "Analysez un lecteur ou un dossier pour voir ce qui le remplit.",
        "Scan only one folder, faster." => "N’analyser qu’un dossier, plus vite.",
        "Scan stopped." => "Analyse arrêtée.",
        "Scan" => "Analyser",
        "Scanning…" => "Analyse…",
        "Selected: {size}" => "Sélection : {size}",
        "Send to the Recycle Bin" => "Envoyer dans la Corbeille",
        "Show in Explorer" => "Afficher dans l’Explorateur",
        "Size" => "Taille",
        "Some of Windows' own files need administrator rights." => "Certains fichiers de Windows demandent des droits d’administrateur.",
        "Stop" => "Arrêter",
        "System" => "Système",
        "Test mode: files in the test folder are removed for good." => "Mode d’essai : les fichiers du dossier d’essai sont supprimés définitivement.",
        "The Recycle Bin is emptied: what is in it is gone for good." => "La Corbeille est vidée : ce qu’elle contient disparaît définitivement.",
        "Theme" => "Thème",
        "This folder holds nothing that takes space." => "Ce dossier ne contient rien qui prenne de la place.",
        "Tick all copies but the oldest" => "Cocher toutes les copies sauf la plus ancienne",
        "Tick all" => "Tout cocher",
        "Tick what you want to clean." => "Cochez ce que vous voulez nettoyer.",
        "Untick all" => "Tout décocher",
        "Windows could not open it." => "Windows n’a pas pu l’ouvrir.",
        "Windows did not give administrator rights." => "Windows n’a pas donné les droits d’administrateur.",
        "Windows' own files can only be cleaned with administrator rights." => {
            "Les fichiers propres à Windows ne se nettoient qu’avec les droits d’administrateur."
        }
        "Your drives" => "Vos lecteurs",
        "empty" => "vide",
        "{a} of {b} ticked" => "{a} sur {b} cochés",
        "{a} of {b} ticked, {size}" => "{a} sur {b} cochés, {size}",
        "{free} free of {total}" => "{free} libres sur {total}",
        "{n} days" => "{n} jours",
        "{n} files so far" => "{n} fichiers pour l’instant",
        "{n} in use, left in place." => "{n} en cours d’utilisation, laissés en place.",
        "{n} items" => "{n} éléments",
        "1 item" => "1 élément",
        "{n} not touched: Plume Clean never removes them there." => "{n} non touchés : Plume Clean n’y supprime jamais rien.",
        "{size} can be freed safely. Nothing is ticked until you choose." => {
            "{size} peuvent être libérés sans risque. Rien n’est coché tant que vous ne choisissez pas."
        }
        "{size} freed." => "{size} libérés.",
        "{size} in {n} files" => "{size} dans {n} fichiers",
        "{used} used" => "{used} utilisés",
        // the suggestions
        "Temporary files" => "Fichiers temporaires",
        "Windows temporary files" => "Fichiers temporaires de Windows",
        "Recycle Bin" => "Corbeille",
        "Edge cache" => "Cache d’Edge",
        "Chrome cache" => "Cache de Chrome",
        "Firefox cache" => "Cache de Firefox",
        "Plume Web cache" => "Cache de Plume Web",
        "Thumbnail cache" => "Cache des miniatures",
        "Crash reports" => "Rapports de plantage",
        "Windows crash dumps" => "Vidages mémoire de Windows",
        "Windows Update leftovers" => "Restes de Windows Update",
        "Large files" => "Gros fichiers",
        "Duplicate files" => "Fichiers en double",
        "Empty folders" => "Dossiers vides",
        "Files apps left behind while they worked. Apps make new ones when they need them; files still in use are skipped." => {
            "Des fichiers laissés par les applications pendant leur travail. Elles en refont quand il le faut ; ceux encore utilisés sont ignorés."
        }
        "The same for Windows itself and its services." => "La même chose pour Windows et ses services.",
        "Files you already deleted. Emptying it removes them for good." => "Des fichiers que vous avez déjà supprimés. La vider les efface définitivement.",
        "Copies of web pages and pictures kept to load sites faster. Sites load a bit slower the first time again. Your history, passwords and sign-ins stay." => {
            "Des copies de pages et d’images gardées pour charger les sites plus vite. Les sites seront un peu plus lents la première fois. L’historique, les mots de passe et les connexions restent."
        }
        "Small previews of your pictures and videos. Windows draws them again when you open a folder." => {
            "Les petits aperçus de vos images et vidéos. Windows les redessine quand vous ouvrez un dossier."
        }
        "Reports written when an app crashed. Only useful to send to its makers." => {
            "Des rapports écrits quand une application a planté. Utiles seulement pour ses créateurs."
        }
        "Memory copies Windows wrote after a crash or an error report." => {
            "Des copies de la mémoire écrites par Windows après un plantage ou un rapport d’erreur."
        }
        "Update files Windows already installed. Windows downloads them again if it needs them." => {
            "Des fichiers de mise à jour déjà installés. Windows les retélécharge s’il en a besoin."
        }
        "Things in your Downloads folder you have not touched for a long time. Look before you tick." => {
            "Ce que vous n’avez pas ouvert depuis longtemps dans Téléchargements. Regardez avant de cocher."
        }
        "The biggest files of the last scan. Nothing is ticked: choose what you no longer need." => {
            "Les plus gros fichiers de la dernière analyse. Rien n’est coché : choisissez ce dont vous n’avez plus besoin."
        }
        "Files with exactly the same content in several places. Keeping one copy is enough." => {
            "Des fichiers au contenu identique à plusieurs endroits. Une seule copie suffit."
        }
        "Folders with nothing inside, from the last scan." => "Des dossiers sans rien dedans, trouvés par la dernière analyse.",
        _ => en,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_and_numbers() {
        assert_eq!(fill("{a} of {b}", &["1", "2"]), "1 of 2");
        assert_eq!(count(1234567), "1,234,567");
        assert_eq!(count(12), "12");
        let day = 86_400 * 10_000_000u64;
        let now = 1000 * day;
        assert_eq!(ago(now, now), "today");
        assert_eq!(ago(now - 5 * day, now), "5 days ago");
        assert_eq!(ago(now - 400 * day, now), "13 months ago");
        assert_eq!(ago(now - 800 * day, now), "2 years ago");
    }
}
