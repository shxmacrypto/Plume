//! English and French. The interface is written in English; `tr` gives the French when Plume speaks French
//! (Plume Boost's choice, otherwise the Suite's, otherwise Windows'). The tweaks carry both languages themselves
//! (tweaks.rs).

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

/// A text written in both languages where it is used.
pub fn tf(en: &'static str, fr: &'static str) -> &'static str {
    if french() { fr } else { en }
}

pub fn tr(en: &'static str) -> &'static str {
    if !french() {
        return en;
    }
    fr(en).unwrap_or(en)
}

fn fr(en: &str) -> Option<&'static str> {
    Some(match en {
        "(not set)" => "(absent)",
        "1 change applied" => "1 changement appliqué",
        "1 change to apply" => "1 changement à appliquer",
        "99 % (no turbo)" => "99 % (sans turbo)",
        "99 % turns turbo boost off on most Intel processors: cooler and quieter." => {
            "99 % coupe le turbo sur la plupart des processeurs Intel : plus frais et plus silencieux."
        }
        "A profile only ticks boxes: change any of them before applying." => {
            "Un profil ne fait que cocher des cases : modifiez-les à votre guise avant d'appliquer."
        }
        "About" => "À propos",
        "About overclocking" => "À propos de l'overclocking",
        "Active power plan" => "Mode d'alimentation actif",
        "Admin" => "Admin",
        "Administrator" => "Administrateur",
        "Administrator rights needed" => "Droits d'administrateur nécessaires",
        "After" => "Après",
        "Aggressive" => "Agressif",
        "Appearance" => "Apparence",
        "Applied tweaks show up here, ready to undo." => "Les réglages appliqués apparaissent ici, prêts à être annulés.",
        "Apply" => "Appliquer",
        "Apply 1 change?" => "Appliquer 1 changement ?",
        "Apply anyway" => "Appliquer quand même",
        "Apply {}" => "Appliquer {}",
        "Apply {} changes?" => "Appliquer {} changements ?",
        "Applying…" => "Application…",
        "Automatic" => "Automatique",
        "Base clock" => "Fréquence de base",
        "Before" => "Avant",
        "Before and after the last Apply" => "Avant et après la dernière application",
        "Cancel" => "Annuler",
        "Change one thing at a time, test for stability for hours (games, OCCT or Prime95, memory tests), watch temperatures, and keep a way back (the BIOS's default settings). Undervolting — less voltage at the same clock — is the gentler path: cooler and quieter, sometimes faster because the chip boosts longer." => {
            "Changez une chose à la fois, testez la stabilité pendant des heures (jeux, OCCT ou Prime95, tests mémoire), surveillez les températures et gardez un retour possible (les réglages par défaut du BIOS). Le sous-voltage — moins de tension à la même fréquence — est la voie la plus douce : plus frais, plus silencieux, parfois plus rapide car la puce reste plus longtemps en boost."
        }
        "Changed. Undo it from Changes whenever you like." => "Modifié. Annulez-le depuis Changements quand vous voulez.",
        "Changes" => "Changements",
        "Changes to the whole PC (marked Admin) will ask Windows for administrator rights first." => {
            "Les changements pour tout le PC (marqués Admin) demanderont d'abord les droits d'administrateur à Windows."
        }
        "Changes to the whole PC can be applied directly." => "Les changements pour tout le PC peuvent être appliqués directement.",
        "Choose tweaks" => "Choisir des réglages",
        "Clocks as Windows reports them, and the limits your power plan sets." => {
            "Les fréquences telles que Windows les indique, et les limites fixées par votre mode d'alimentation."
        }
        "Create a System Restore point first" => "Créer d'abord un point de restauration système",
        "Creating a System Restore point needs administrator rights." => "Créer un point de restauration système demande les droits d'administrateur.",
        "Creating a restore point…" => "Création d'un point de restauration…",
        "Dark" => "Sombre",
        "Disabled" => "Désactivé",
        "Efficient" => "Efficace",
        "Efficient aggressive" => "Efficace agressif",
        "Everything Plume Boost changed is back as it was." => "Tout ce que Plume Boost avait changé est revenu comme avant.",
        "Everything Plume Boost changed, and the value each setting gets back on Undo." => {
            "Tout ce que Plume Boost a changé, et la valeur que chaque réglage retrouve en cas d'annulation."
        }
        "Everything is in use already." => "Tout est déjà en service.",
        "Everything stays on this PC: Plume Boost sends nothing online." => "Tout reste sur ce PC : Plume Boost n'envoie rien en ligne.",
        "Everything ticked is already on." => "Tout ce qui est coché est déjà actif.",
        "Everything ticked is on" => "Tout ce qui est coché est actif",
        "Evidence" => "Sources",
        "Graphics" => "Graphiques",
        "Highest seen since opening" => "Maximum vu depuis l'ouverture",
        "How eagerly the processor goes above its base clock." => "À quel point le processeur monte volontiers au-dessus de sa fréquence de base.",
        "How late a 1 ms timer really wakes a program. Deep idle states and power saving make it later; High performance plans make it shorter." => {
            "Le retard réel avec lequel un minuteur de 1 ms réveille un programme. Les états de repos profonds et les économies d'énergie l'allongent ; les modes Performances élevées le raccourcissent."
        }
        "How slow the processor may go when idle. Higher wakes faster, uses more power." => {
            "Jusqu'où le processeur peut ralentir au repos. Plus haut, il se réveille plus vite et consomme plus."
        }
        "If you try it" => "Si vous essayez",
        "It sets the power plan and its processor limits above, all through Windows, all undoable. That decides how fast the processor reacts and how high it may boost, within what its maker allows." => {
            "Il règle le mode d'alimentation et ses limites processeur ci-dessus, toujours via Windows, toujours annulable. Cela décide de la vitesse de réaction du processeur et de la hauteur de son boost, dans ce que son fabricant autorise."
        }
        "Language" => "Langue",
        "Left out on purpose: anything that turns off Defender, UAC, memory integrity or Windows Update, and tweaks without measurable gains (see the README)." => {
            "Écartés exprès : tout ce qui coupe Defender, le contrôle de compte, l'intégrité de la mémoire ou Windows Update, et les réglages sans gain mesurable (voir le README)."
        }
        "Light" => "Clair",
        "Manual" => "Manuel",
        "Maximum processor state" => "État maximal du processeur",
        "Measure" => "Mesurer",
        "Measured just before and just after. Clocks and memory move on their own; the wake-up delay shows what a power plan changes." => {
            "Mesuré juste avant et juste après. Fréquences et mémoire bougent d'elles-mêmes ; le délai de réveil montre ce qu'un mode d'alimentation change."
        }
        "Test mode: the plan comes from the imaginary PC; clocks, timer, memory and delay are this PC's real numbers, which Plume Boost did not change." => {
            "Mode test : le mode vient du PC imaginaire ; fréquences, minuteur, mémoire et délai sont les vrais chiffres de ce PC, que Plume Boost n'a pas changés."
        }
        "Measuring…" => "Mesure…",
        "Memory in use" => "Mémoire utilisée",
        "Minimum processor state" => "État minimal du processeur",
        "Needs administrator rights: Plume Boost will ask Windows first." => {
            "Demande les droits d'administrateur : Plume Boost les demandera d'abord à Windows."
        }
        "No restore point was created" => "Aucun point de restauration n'a été créé",
        "Not on this PC" => "Absent de ce PC",
        "Nothing changed yet" => "Rien n'a encore changé",
        "Nothing ticked" => "Rien de coché",
        "Nothing was changed" => "Rien n'a été changé",
        "Now (all cores)" => "Maintenant (tous les cœurs)",
        "OK" => "OK",
        "Off" => "Inactif",
        "Offer a System Restore point before applying" => "Proposer un point de restauration avant d'appliquer",
        "On" => "Actif",
        "On battery" => "Sur batterie",
        "Only Windows' own settings, each one explained and reversible. Nothing is applied until you tick it and press Apply." => {
            "Uniquement des réglages de Windows, chacun expliqué et réversible. Rien n'est appliqué tant que vous ne l'avez pas coché et n'avez pas appuyé sur Appliquer."
        }
        "Other" => "Autre",
        "Overview" => "Aperçu",
        "Overview shows the numbers before and after. Changes lists every change, ready to undo." => {
            "Aperçu montre les chiffres avant et après. Changements liste chaque changement, prêt à être annulé."
        }
        "Partly" => "En partie",
        "Pick a profile, or tick tweaks one by one." => "Choisissez un profil, ou cochez les réglages un par un.",
        "Plugged in" => "Sur secteur",
        "Plume Boost opens again as administrator with the same ticks. Press Apply there." => {
            "Plume Boost se rouvre en administrateur avec les mêmes cases cochées. Appuyez sur Appliquer là-bas."
        }
        "Plume Boost opens again as administrator. Undo from there." => "Plume Boost se rouvre en administrateur. Annulez depuis là.",
        "Plume Boost puts back the {} settings it changed, newest first, to the values they had before." => {
            "Plume Boost remet les {} réglages qu'il a changés, du plus récent au plus ancien, aux valeurs qu'ils avaient avant."
        }
        "Plume Boost was not started as administrator." => "Plume Boost n'a pas été lancé en administrateur.",
        "Power plan" => "Mode d'alimentation",
        "Power setting" => "Réglage d'alimentation",
        "Processor" => "Processeur",
        "Processor & graphics" => "Processeur et graphiques",
        "Processor & GPU" => "Processeur et GPU",
        "Processor boost mode" => "Mode de boost du processeur",
        "Processor clock" => "Fréquence du processeur",
        "Processor limits" => "Limites du processeur",
        "Processor limits of the plan \"{}\"" => "Limites du processeur du mode « {} »",
        "Processor: the BIOS/UEFI of your motherboard (Precision Boost Overdrive and Curve Optimizer on AMD, power limits on Intel), AMD Ryzen Master, Intel Extreme Tuning Utility on unlocked chips. Graphics card: MSI Afterburner, AMD Software (Performance › Tuning), the NVIDIA app's automatic tuning. Laptops: usually the maker's own app (performance modes)." => {
            "Processeur : le BIOS/UEFI de la carte mère (Precision Boost Overdrive et Curve Optimizer chez AMD, limites de puissance chez Intel), AMD Ryzen Master, Intel Extreme Tuning Utility sur les puces débloquées. Carte graphique : MSI Afterburner, AMD Software (Performances › Réglage), le réglage automatique de l'appli NVIDIA. Portables : en général l'appli du fabricant (modes de performance)."
        }
        "Real numbers from this PC, read every second. Nothing changes until you tick it and press Apply." => {
            "De vrais chiffres de ce PC, relus chaque seconde. Rien ne change tant que vous ne l'avez pas coché et n'avez pas appuyé sur Appliquer."
        }
        "Real overclocking changes multipliers and voltages. On a processor they live in the BIOS/UEFI; on a graphics card they go through the maker's driver. Done wrong, it crashes games, corrupts files silently, or wears the chip out — the 2024 Intel 13th/14th-gen instability came from voltages that were too high. No app can do it safely for every PC, so Plume Boost doesn't try." => {
            "Le vrai overclocking change multiplicateurs et tensions. Pour un processeur, ils se règlent dans le BIOS/UEFI ; pour une carte graphique, via le pilote du fabricant. Mal fait, il plante les jeux, corrompt des fichiers sans prévenir ou use la puce — l'instabilité des Intel de 13e/14e génération en 2024 venait de tensions trop élevées. Aucune appli ne peut le faire sans risque pour tous les PC : Plume Boost ne s'y essaie pas."
        }
        "Removing Plume Boost puts back everything it changed." => "Désinstaller Plume Boost remet tout ce qu'il a changé.",
        "Restart" => "Redémarrage",
        "Restart Plume Boost as administrator, or turn off the restore point switch: every change can be undone from Changes anyway." => {
            "Relancez Plume Boost en administrateur, ou coupez l'option du point de restauration : chaque changement reste annulable depuis Changements."
        }
        "Restart Windows to finish: some changes are used only after a restart." => {
            "Redémarrez Windows pour terminer : certains changements ne servent qu'après un redémarrage."
        }
        "Restart as administrator" => "Relancer en administrateur",
        "Restart the game" => "Relancer le jeu",
        "Restore points made by Plume Boost" => "Points de restauration créés par Plume Boost",
        "Running as administrator." => "Lancé en administrateur.",
        "Running with standard rights." => "Lancé avec des droits standard.",
        "Safe and reversible: these are Windows' own power settings. A change applies at once, and Changes can undo it." => {
            "Sûr et réversible : ce sont les réglages d'alimentation de Windows. Un changement s'applique tout de suite, et Changements peut l'annuler."
        }
        "Safety" => "Sécurité",
        "Same as Windows" => "Comme Windows",
        "Service" => "Service",
        "Settings" => "Paramètres",
        "Sign out" => "Déconnexion",
        "Sign out and back in to finish." => "Déconnectez-vous puis reconnectez-vous pour terminer.",
        "Some may need a restart or a sign-out to be fully back." => "Certains peuvent demander un redémarrage ou une déconnexion pour revenir entièrement.",
        "Some of the settings to put back belong to the whole PC, so Windows asks for administrator rights." => {
            "Certains réglages à remettre concernent tout le PC : Windows demande donc les droits d'administrateur."
        }
        "Standard rights" => "Droits standard",
        "Start from a profile" => "Partir d'un profil",
        "Start your games again to use the new settings." => "Relancez vos jeux pour profiter des nouveaux réglages.",
        "System Restore is turned off on this PC." => "La restauration du système est désactivée sur ce PC.",
        "Test mode: an imaginary PC kept in a file. This PC is not changed." => "Mode test : un PC imaginaire gardé dans un fichier. Ce PC n'est pas modifié.",
        "Test mode: imaginary PC" => "Mode test : PC imaginaire",
        "Test mode: the imaginary PC now has administrator rights." => "Mode test : le PC imaginaire a maintenant les droits d'administrateur.",
        "The Apply dialog starts with its switch on. A restore point needs administrator rights." => {
            "La fenêtre Appliquer s'ouvre avec l'option activée. Un point de restauration demande les droits d'administrateur."
        }
        "The current value of each setting is saved first." => "La valeur actuelle de chaque réglage est d'abord enregistrée.",
        "The value each setting has now is saved first: Changes can put it back at any time." => {
            "La valeur actuelle de chaque réglage est d'abord enregistrée : Changements peut la remettre à tout moment."
        }
        "Theme" => "Thème",
        "These change the whole PC, so Windows asks for administrator rights:" => {
            "Ces changements concernent tout le PC, Windows demande donc les droits d'administrateur :"
        }
        "This PC doesn't have it." => "Ce PC ne l'a pas.",
        "Tick at least one tweak first." => "Cochez d'abord au moins un réglage.",
        "Tick what you want, then Apply. Every change can be undone from Changes." => {
            "Cochez ce que vous voulez, puis Appliquer. Chaque changement s'annule depuis Changements."
        }
        "Timer" => "Minuteur",
        "Timer resolution" => "Résolution du minuteur",
        "Turbo boost mode" => "Mode turbo",
        "Tweaks" => "Réglages",
        "Undo" => "Annuler",
        "Undo everything" => "Tout annuler",
        "Undo everything?" => "Tout annuler ?",
        "Undoing…" => "Annulation…",
        "Wake-up delay" => "Délai de réveil",
        "Wake-up delay (typical)" => "Délai de réveil (typique)",
        "What Plume Boost does instead" => "Ce que Plume Boost fait à la place",
        "What Plume Boost writes" => "Ce que Plume Boost écrit",
        "What it costs" => "Ce que ça coûte",
        "What it does" => "Ce que ça fait",
        "What you gain" => "Ce que vous gagnez",
        "Where it is done" => "Où ça se fait",
        "Why Plume Boost doesn't overclock" => "Pourquoi Plume Boost n'overclocke pas",
        "Windows also skips a new point when one was made in the last 24 hours. Apply without one? Every change can still be undone from Changes." => {
            "Windows saute aussi un nouveau point si un autre a été créé dans les dernières 24 heures. Appliquer sans ? Chaque changement reste annulable depuis Changements."
        }
        "Windows asks for administrator rights for this." => "Windows demande les droits d'administrateur pour cela.",
        "Windows has no common way to read graphics clocks. NVIDIA cards show them here; for Intel and AMD, their own apps do." => {
            "Windows n'a pas de moyen commun de lire les fréquences graphiques. Les cartes NVIDIA les affichent ici ; pour Intel et AMD, leurs propres applis le font."
        }
        "Windows keeps it in System › System protection. Takes up to a minute." => {
            "Windows le garde dans Système › Protection du système. Jusqu'à une minute."
        }
        "Windows refused to change this power setting without administrator rights." => {
            "Windows a refusé de changer ce réglage d'alimentation sans droits d'administrateur."
        }
        "Windows restores them from System › System protection › System Restore." => {
            "Windows les restaure depuis Système › Protection du système › Restauration du système."
        }
        "Windows' active plan" => "Le mode actif de Windows",
        "Your own choice: a profile was changed by hand." => "Votre propre choix : un profil a été modifié à la main.",
        "after a restart" => "après un redémarrage",
        "after signing out" => "après une déconnexion",
        "average of all cores" => "moyenne de tous les cœurs",
        "base {}" => "base {}",
        "base {} · peak {}" => "base {} · pointe {}",
        "finest {} · coarsest {}" => "de {} à {}",
        "just now" => "à l'instant",
        "needs administrator rights to undo" => "droits d'administrateur nécessaires pour annuler",
        "of {} · {} %" => "sur {} · {} %",
        "on battery" => "sur batterie",
        "plugged in" => "sur secteur",
        "typical · {} µs at worst 1 in 100" => "typique · {} µs au pire 1 fois sur 100",
        "unknown" => "inconnu",
        "when games start again" => "au prochain lancement des jeux",
        "{} MHz now · {} MHz max · memory {} MHz" => "{} MHz maintenant · {} MHz max · mémoire {} MHz",
        "{} changes applied" => "{} changements appliqués",
        "{} changes could not be put back: {}" => "{} changements n'ont pas pu être remis : {}",
        "{} changes to apply" => "{} changements à appliquer",
        "{} days ago" => "il y a {} jours",
        "{} h ago" => "il y a {} h",
        "{} is back as it was." => "{} est revenu comme avant.",
        "{} min ago" => "il y a {} min",
        "{} need a restart or sign-out" => "{} demandent un redémarrage ou une déconnexion",
        "{} need administrator rights" => "{} demandent les droits d'administrateur",
        "{} settings changed in all. Removing Plume Boost puts them back too." => "{} réglages changés en tout. Désinstaller Plume Boost les remet aussi.",
        "{} ticked were already on." => "{} cochés étaient déjà actifs.",
        _ => return None,
    })
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
