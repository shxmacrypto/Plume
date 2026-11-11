//! English and French. The interface is written in English; `tr` gives the French when Plume speaks French
//! (Plume Task's choice, otherwise the Suite's, otherwise Windows').

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
    if !french() {
        return en;
    }
    match en {
        "Apps" => "Applications",
        "Performance" => "Performances",
        "Startup apps" => "Démarrage",
        "Settings" => "Paramètres",
        "Background processes" => "Processus en arrière-plan",
        "Name" => "Nom",
        "Processor" => "Processeur",
        "Memory" => "Mémoire",
        "Disk" => "Disque",
        "Graphics" => "Graphique",
        "Network" => "Réseau",
        "Battery impact" => "Impact batterie",
        "Very low" => "Très faible",
        "Low" => "Faible",
        "Moderate" => "Modéré",
        "High" => "Élevé",
        "Very high" => "Très élevé",
        "Search a program, a process or a PID" => "Rechercher un programme, un processus ou un PID",
        "End task" => "Fin de tâche",
        "Details" => "Détails",
        "Show" => "Voir",
        "Open file location" => "Ouvrir l'emplacement",
        "Show its processes" => "Voir ses processus",
        "Collapse" => "Replier",
        "{} programs, {} processes" => "{} programmes, {} processus",
        "Reading the programs…" => "Lecture des programmes…",
        "No program matches this search." => "Aucun programme ne correspond à cette recherche.",
        "The list is paused." => "La liste est en pause.",
        // the summary
        "Your PC is running smoothly" => "Votre PC tourne sans souci",
        "Nothing is slowing it down right now." => "Rien ne le ralentit en ce moment.",
        "Nothing is slowing it down. The busiest program is {} ({} of the processor)." => {
            "Rien ne le ralentit. Le programme le plus actif est {} ({} du processeur)."
        }
        "Memory is almost full" => "La mémoire est presque pleine",
        "Memory is filling up" => "La mémoire se remplit",
        "{} uses {}. Closing programs you don't need frees memory." => "{} utilise {}. Fermer les programmes inutiles libère de la mémoire.",
        "The processor is working hard" => "Le processeur est très sollicité",
        "The processor is busy" => "Le processeur est occupé",
        "{} uses {} of it. End it if you don't need it now." => "{} en utilise {}. Arrêtez-le si vous n'en avez pas besoin maintenant.",
        "The disk is very busy" => "Le disque est très sollicité",
        "The disk is busy" => "Le disque est occupé",
        "{} reads and writes the most ({})." => "{} lit et écrit le plus ({}).",
        "The graphics card is working hard" => "La carte graphique est très sollicitée",
        "The graphics card is busy" => "La carte graphique est occupée",
        "{} uses {} of it." => "{} en utilise {}.",
        // end task
        "End {}?" => "Arrêter {} ?",
        "Plume Task can't end {}" => "Plume Task ne peut pas arrêter {}",
        "It will close. Anything not saved in it will be lost." => "Il va se fermer. Ce qui n'y est pas enregistré sera perdu.",
        "Its {} processes will close. Anything not saved in it will be lost." => "Ses {} processus vont se fermer. Ce qui n'y est pas enregistré sera perdu.",
        "{} of its {} processes will close; the others can't be ended. {}" => {
            "{} de ses {} processus vont se fermer ; les autres ne peuvent pas être arrêtés. {}"
        }
        "It runs as administrator. To end it, open Plume Task as administrator." => {
            "Il tourne en administrateur. Pour l'arrêter, ouvrez Plume Task en administrateur."
        }
        "It is part of Windows, which protects it." => "Il fait partie de Windows, qui le protège.",
        "It is a Windows service. Services are stopped from Windows' Services app." => {
            "C'est un service de Windows. Les services s'arrêtent depuis l'application Services de Windows."
        }
        "It has already closed." => "Il s'est déjà fermé.",
        "Cancel" => "Annuler",
        "Close" => "Fermer",
        "{} was ended." => "{} a été arrêté.",
        "Windows refused to end part of {}." => "Windows a refusé d'arrêter une partie de {}.",
        "Windows doesn't show where this one is." => "Windows ne montre pas où se trouve celui-ci.",
        // details
        "Program" => "Programme",
        "Process" => "Processus",
        "Processes" => "Processus",
        "Publisher" => "Éditeur",
        "PID" => "PID",
        "Parent PID" => "PID parent",
        "Started" => "Démarré",
        "Location" => "Emplacement",
        "Command line" => "Ligne de commande",
        "Threads" => "Threads",
        "Handles" => "Handles",
        "Not available" => "Non disponible",
        "Where" => "Où",
        "Entry" => "Entrée",
        "State" => "État",
        // performance
        "Windows has been running for {}  ·  {} processes  ·  {} threads" => "Windows tourne depuis {}  ·  {} processus  ·  {} threads",
        "Logical processors" => "Processeurs logiques",
        "Up time" => "Temps de fonctionnement",
        "{} of {} in use" => "{} sur {} utilisés",
        "{} of {}" => "{} sur {}",
        "{} processes" => "{} processus",
        "Available" => "Disponible",
        "Committed" => "Validée",
        "Cached" => "En cache",
        "Share of the time the disks are busy" => "Part du temps où les disques travaillent",
        "Read" => "Lecture",
        "Write" => "Écriture",
        "All connected adapters" => "Toutes les cartes connectées",
        "Receive" => "Réception",
        "Send" => "Envoi",
        "Graphics card" => "Carte graphique",
        "Dedicated memory" => "Mémoire dédiée",
        "60 seconds" => "60 secondes",
        // startup
        "Programs that open by themselves when you sign in. Turning one off keeps it installed." => {
            "Les programmes qui s'ouvrent seuls à la connexion. En désactiver un le laisse installé."
        }
        "Refresh" => "Actualiser",
        "Reading the startup list…" => "Lecture de la liste de démarrage…",
        "Test mode: a test registry key and folder." => "Mode test : une clé de registre et un dossier de test.",
        "on" => "activés",
        "On" => "Activé",
        "Off" => "Désactivé",
        "Nothing starts with Windows." => "Rien ne démarre avec Windows.",
        "running, {}" => "ouvert, {}",
        "Registry, your account" => "Registre, votre compte",
        "Registry, all accounts" => "Registre, tous les comptes",
        "Startup folder" => "Dossier Démarrage",
        "Startup folder, all accounts" => "Dossier Démarrage, tous les comptes",
        "Scheduled task at sign-in" => "Tâche planifiée à la connexion",
        "This entry can't be changed here." => "Cette entrée ne peut pas être modifiée ici.",
        "Windows refused: this one needs administrator rights." => "Windows a refusé : celle-ci demande les droits d'administrateur.",
        // settings
        "Language" => "Langue",
        "Theme" => "Thème",
        "Same as Windows" => "Comme Windows",
        "Dark" => "Sombre",
        "Light" => "Clair",
        "How often the numbers are read again. Slower uses even less of the processor." => {
            "À quelle fréquence les chiffres sont relus. Plus lent sollicite encore moins le processeur."
        }
        "Fast (½ s)" => "Rapide (½ s)",
        "Normal (1 s)" => "Normale (1 s)",
        "Slow (2 s)" => "Lente (2 s)",
        "Paused" => "En pause",
        "Apps apart from background processes" => "Applications séparées des processus en arrière-plan",
        "Programs with a window come first, in their own section." => "Les programmes avec une fenêtre viennent d'abord, dans leur propre section.",
        "Plume Task itself uses {} of the processor (average of the last minute)." => {
            "Plume Task lui-même utilise {} du processeur (moyenne de la dernière minute)."
        }
        "Everything stays on this PC: Plume Task sends nothing online." => "Tout reste sur ce PC : Plume Task n'envoie rien en ligne.",
        _ => en,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes() {
        assert_eq!(fill("{} of {}", &["3", "10"]), "3 of 10");
        assert_eq!(fill("no holes", &["x"]), "no holes");
    }
}
