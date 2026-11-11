//! English and French. The interface is written in English; `tr` gives the French when Plume Backup speaks French
//! (its setting, otherwise the suite's, otherwise Windows').

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
    while let Some(a) = rest.find("{}") {
        out.push_str(&rest[..a]);
        out.push_str(args.get(i).copied().unwrap_or(""));
        i += 1;
        rest = &rest[a + 2..];
    }
    out.push_str(rest);
    out
}

/// 1204 → "1,204" (English), "1 204" (French).
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let sep = if french() { '\u{202f}' } else { ',' };
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}

/// "1 file", "12 files".
pub fn files(n: u64) -> String {
    if n == 1 { tr("1 file").into() } else { fill(tr("{} files"), &[&count(n)]) }
}

/// 1_500_000 → "1.4 MB" (English), "1,4 Mo" (French).
pub fn size(b: u64) -> String {
    let fr = french();
    let units: [&str; 5] = if fr { ["o", "Ko", "Mo", "Go", "To"] } else { ["B", "KB", "MB", "GB", "TB"] };
    let mut v = b as f64;
    let mut u = 0;
    while v >= 1024.0 && u < units.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    let s = if u == 0 || v >= 100.0 { format!("{v:.0}") } else { format!("{v:.1}") };
    let s = if fr { s.replace('.', ",") } else { s };
    format!("{s} {}", units[u])
}

/// A moment, near `now`: "Today 14:30", "Yesterday 09:05", "Nov 3, 14:30", "Nov 3, 2025".
pub fn when(t: u64, now: u64) -> String {
    if t == 0 {
        return String::new();
    }
    let fr = french();
    let l = crate::sys::local(t);
    let hm = format!("{:02}:{:02}", l.hour, l.minute);
    let today = crate::sys::day_start(now);
    if t >= today && t < today + 86_400 {
        return format!("{} {hm}", if fr { "Aujourd'hui" } else { "Today" });
    }
    if t < today && t + 86_400 >= today {
        return format!("{} {hm}", if fr { "Hier" } else { "Yesterday" });
    }
    const EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const FR_M: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    let m = (l.month.clamp(1, 12) - 1) as usize;
    let tail = if crate::sys::local(now).year == l.year { hm } else { l.year.to_string() };
    if fr { format!("{} {} {tail}", l.day, FR_M[m]) } else { format!("{} {}, {tail}", EN[m], l.day) }
}

pub fn tr(en: &'static str) -> &'static str {
    if !french() {
        return en;
    }
    match en {
        "Stopped. The next backup carries on from there." => "Arrêtée. La prochaine sauvegarde reprendra là où elle s'est arrêtée.",
        "Add at least one folder to back up." => "Ajoutez au moins un dossier à sauvegarder.",
        "Choose where to save the backup." => "Choisissez où enregistrer la sauvegarde.",
        "The destination is not available: plug in the drive or check the network share." => {
            "La destination n'est pas disponible : branchez le disque ou vérifiez le partage réseau."
        }
        "A folder to back up is missing: {}. Nothing was changed." => "Un dossier à sauvegarder est introuvable : {}. Rien n'a été modifié.",
        "The backup can't be inside a folder it backs up." => "La sauvegarde ne peut pas se trouver dans un dossier qu'elle sauvegarde.",
        "The destination is full." => "La destination est pleine.",
        "Error: {}" => "Erreur : {}",
        "{}: {}" => "{} : {}",
        "Plume Backup: backing up {}… {} %" => "Plume Backup : sauvegarde de {}… {} %",
        "Plume Backup: last backup {}" => "Plume Backup : dernière sauvegarde {}",
        "Backup done" => "Sauvegarde terminée",
        "{}: {} copied, {} in all." => "{} : {} copiés, {} en tout.",
        "1 file" => "1 fichier",
        "Restored: {}" => "Restauré : {}",
        "The different file that was there is kept, with “before restore” in its name." => {
            "Le fichier différent qui s'y trouvait est gardé, avec « before restore » dans son nom."
        }
        "Backup done, with problems" => "Sauvegarde terminée, avec des problèmes",
        "{}: {} files could not be backed up." => "{} : {} fichiers n'ont pas pu être sauvegardés.",
        "Backup failed" => "La sauvegarde a échoué",
        "Backup {}" => "Sauvegarde {}",
        "Backup" => "Sauvegarde",
        "Plan “{}” saved" => "Plan « {} » enregistré",
        "Plan “{}” deleted. Its backups are still on the destination." => "Plan « {} » supprimé. Ses sauvegardes restent sur la destination.",
        "Choose a file or a folder first." => "Choisissez d'abord un fichier ou un dossier.",
        "Already there and identical: nothing to restore." => "Déjà en place et identique : rien à restaurer.",
        "Restored {} to {}" => "{} restaurés dans {}",
        "{} different files that were there are kept, with “before restore” in their name." => {
            "{} fichiers différents qui s'y trouvaient sont gardés, avec « before restore » dans leur nom."
        }
        "{} could not be restored." => "{} n'ont pas pu être restaurés.",
        "Folder to back up" => "Dossier à sauvegarder",
        "Where to save the backup" => "Où enregistrer la sauvegarde",
        "Restore to this folder" => "Restaurer dans ce dossier",
        "Only when you ask" => "Seulement quand vous le demandez",
        "Every hour" => "Toutes les heures",
        "Every day from {}" => "Chaque jour à partir de {}",
        "When the drive is plugged in" => "Quand le disque est branché",
        "Mirror" => "Miroir",
        "Versions: all kept" => "Versions : toutes gardées",
        "Versions: {} kept" => "Versions : {} gardées",
        ", {} days at most" => ", {} jours au plus",
        "{} days" => "{} jours",
        "Backups" => "Sauvegardes",
        "Restore" => "Restaurer",
        "Settings" => "Réglages",
        "Backing up {}" => "Sauvegarde de {}",
        "Your folders, copied automatically to a drive, another folder or a PC on your network." => {
            "Vos dossiers, copiés automatiquement sur un disque, dans un autre dossier ou sur un PC de votre réseau."
        }
        "New plan" => "Nouveau plan",
        "No backup plan yet" => "Aucun plan de sauvegarde",
        "Choose the folders to protect and where to copy them." => "Choisissez les dossiers à protéger et où les copier.",
        "Create a plan" => "Créer un plan",
        "Edit" => "Modifier",
        "Stop" => "Arrêter",
        "Back up now" => "Sauvegarder",
        "Looking at the files…" => "Examen des fichiers…",
        "Tidying up old versions…" => "Rangement des anciennes versions…",
        "{} of {} files · {} of {} · {} %" => "{} fichiers sur {} · {} sur {} · {} %",
        "Waiting for its turn" => "En attente de son tour",
        "Paused: the schedule is off" => "En pause : la planification est arrêtée",
        "Never backed up" => "Jamais sauvegardé",
        "Last backup {}: {} copied, {} unchanged" => "Dernière sauvegarde {} : {} copiés, {} inchangés",
        ", {} removed from the sources (kept)" => ", {} retirés des sources (gardés)",
        ", {} skipped" => ", {} ignorés",
        "Waiting for the destination" => "En attente de la destination",
        "Edit the plan" => "Modifier le plan",
        "Name" => "Nom",
        "Name of the plan" => "Nom du plan",
        "Folders to back up" => "Dossiers à sauvegarder",
        "No folder yet." => "Aucun dossier pour l'instant.",
        "Add a folder…" => "Ajouter un dossier…",
        "Save to" => "Enregistrer sur",
        "A USB drive, another disk or folder, or a network share such as \\\\PC\\Backups. A USB drive is found again even if Windows gives it another letter." => {
            "Une clé ou un disque USB, un autre disque ou dossier, ou un partage réseau comme \\\\PC\\Sauvegardes. Un disque USB est retrouvé même si Windows lui donne une autre lettre."
        }
        "Removable drive" => "Disque amovible",
        "Network share" => "Partage réseau",
        "Disk of this PC" => "Disque de ce PC",
        "Choose…" => "Choisir…",
        "When" => "Quand",
        "Every day" => "Chaque jour",
        "Only when I ask" => "Quand je le demande",
        "From" => "À partir de",
        "How" => "Comment",
        "Each backup is a complete dated copy you can go back to. Files that did not change are shared between copies, so they take no extra space." => {
            "Chaque sauvegarde est une copie complète et datée où revenir. Les fichiers qui n'ont pas changé sont partagés entre les copies : ils ne prennent pas plus de place."
        }
        "One copy, kept identical to your folders. Files you delete are moved to a dated “deleted” folder at the destination, never simply erased." => {
            "Une seule copie, gardée identique à vos dossiers. Les fichiers que vous supprimez vont dans un dossier daté « deleted » sur la destination, jamais simplement effacés."
        }
        "Versions" => "Versions",
        "All" => "Toutes",
        "Versions kept" => "Versions gardées",
        "Oldest version kept" => "Version la plus ancienne gardée",
        "Removed files kept" => "Fichiers retirés gardés",
        "Leave out" => "Laisser de côté",
        "Patterns, for example *.iso, node_modules, Videos/Raw" => "Motifs, par exemple *.iso, node_modules, Vidéos/Brut",
        "Skip temporary files" => "Ignorer les fichiers temporaires",
        "*.tmp, ~$ files of Office, Thumbs.db, unfinished downloads…" => "*.tmp, fichiers ~$ d'Office, Thumbs.db, téléchargements inachevés…",
        "Check each copy" => "Vérifier chaque copie",
        "Reads every copied file back and compares it with the original. Slower." => "Relit chaque fichier copié et le compare à l'original. Plus lent.",
        "Paused" => "En pause",
        "While paused, the schedule does not start this backup." => "En pause, la planification ne lance pas cette sauvegarde.",
        "Save" => "Enregistrer",
        "Cancel" => "Annuler",
        "Delete the plan" => "Supprimer le plan",
        "Pick a backup by its date, then a file or a folder to put back." => "Choisissez une sauvegarde par sa date, puis un fichier ou un dossier à remettre.",
        "Create a plan first: its backups will show here." => "Créez d'abord un plan : ses sauvegardes apparaîtront ici.",
        "Looking for the backups…" => "Recherche des sauvegardes…",
        "No backup yet." => "Aucune sauvegarde pour l'instant.",
        "Version" => "Version",
        "Mirror (latest)" => "Miroir (la plus récente)",
        "Removed from the sources" => "Retirés des sources",
        "{} files" => "{} fichiers",
        "Size" => "Taille",
        "Modified" => "Modifié",
        "This folder is empty." => "Ce dossier est vide.",
        "Restore to its place" => "Restaurer à sa place",
        "Restore to…" => "Restaurer vers…",
        "Restoring…" => "Restauration…",
        "Choose a file or a folder." => "Choisissez un fichier ou un dossier.",
        "In the background" => "En arrière-plan",
        "Start with Windows" => "Démarrer avec Windows",
        "In the background, so scheduled backups run without opening the window." => {
            "En arrière-plan, pour que les sauvegardes planifiées tournent sans ouvrir la fenêtre."
        }
        "Keep running near the clock" => "Rester actif près de l'horloge",
        "When the window is closed, backups keep their schedule." => "Quand la fenêtre est fermée, les sauvegardes gardent leur planification.",
        "Notifications" => "Notifications",
        "When a backup is done" => "Quand une sauvegarde est terminée",
        "When a backup fails or skips files" => "Quand une sauvegarde échoue ou ignore des fichiers",
        "Appearance" => "Apparence",
        "Language" => "Langue",
        "System" => "Système",
        "Theme" => "Thème",
        "Dark" => "Sombre",
        "Light" => "Clair",
        "About" => "À propos",
        "Your backups go only where you choose: a drive, a folder, or a share on your network. Plume Backup never sends anything online, and it never changes or deletes anything in the folders it backs up." => {
            "Vos sauvegardes vont seulement où vous le choisissez : un disque, un dossier ou un partage de votre réseau. Plume Backup n'envoie jamais rien en ligne, et ne modifie ni ne supprime jamais rien dans les dossiers qu'il sauvegarde."
        }
        "The plan goes away. Its backups stay on the destination, where you can still open them." => {
            "Le plan disparaît. Ses sauvegardes restent sur la destination, où vous pouvez toujours les ouvrir."
        }
        "Delete the plan “{}”?" => "Supprimer le plan « {} » ?",
        "Delete" => "Supprimer",
        "No limit" => "Sans limite",
        "Forever" => "Toujours",
        "Open Plume Backup" => "Ouvrir Plume Backup",
        "Back up all now" => "Tout sauvegarder maintenant",
        "Quit" => "Quitter",
        _ => en,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_counts_and_sizes() {
        set("en");
        assert_eq!(fill("{} of {}", &["3", "10"]), "3 of 10");
        assert_eq!(fill("no holes", &["x"]), "no holes");
        assert_eq!(count(1_234_567), "1,234,567");
        assert_eq!(count(999), "999");
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(300 << 20), "300 MB");
        let now = crate::engine::now();
        assert!(when(now, now).starts_with("Today "));
        assert!(when(now - 86_400, now).starts_with("Yesterday "));
        assert_eq!(when(0, now), "");
    }
}
