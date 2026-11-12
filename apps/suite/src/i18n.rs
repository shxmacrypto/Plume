//! Les textes de Plume Suite, en anglais et en français. La langue est celle choisie dans Plume, sinon celle de Windows.

use windows::Win32::Globalization::GetUserDefaultUILanguage;

pub struct Strings {
    pub title: &'static str,
    pub welcome: &'static str,
    pub sub_none: &'static str,
    pub sub_some: &'static str,
    pub desc: [&'static str; crate::apps::COUNT],
    pub installed: &'static str,
    pub not_installed: &'static str,
    pub coming_soon: &'static str,
    pub fex_default: &'static str,
    pub install: &'static str,
    pub open: &'static str,
    pub remove: &'static str,
    pub cancel: &'static str,
    pub installing: &'static str,
    pub removing: &'static str,
    pub checking: &'static str,
    pub up_to_date: &'static str,
    pub dev_build: &'static str,
    pub offline: &'static str,
    pub available: &'static str,
    pub available_sub: &'static str,
    pub update_all: &'static str,
    pub downloading: &'static str,
    pub updating: &'static str,
    pub updated: &'static str,
    pub update_failed: &'static str,
    pub retry: &'static str,
    pub check_now: &'static str,
    pub missing_setup: &'static str,
    pub failed: &'static str,
    pub confirm_remove: &'static str,
    pub confirm_remove_needed: &'static str,
    pub installs_too: &'static str,
    pub website: &'static str,
    pub notes: &'static str,
    pub set_default: &'static str,
    pub is_default: &'static str,
    pub opening: &'static str,
    pub default_explain: &'static str,
    pub continue_: &'static str,
    pub default_opened: &'static str,
}

const EN: Strings = Strings {
    title: "Plume Suite",
    welcome: "Your Plume apps",
    sub_none: "Pick the apps you want. Each one installs in a second, without administrator rights.",
    sub_some: "Open, add or remove them here. One update brings all of them up to date.",
    desc: [
        "The light browser: sleeping tabs, a built-in blocker, encrypted passwords.",
        "The file explorer that finds any file on your PC while you type.",
        "Your mail, shown as safe text: no tracking pixels, no scripts, no ads.",
        "Control this PC from your phone: scan a QR code, and the screen is yours.",
        "Your music and YouTube Music, playlists from Spotify and Apple Music, synced lyrics.",
        "Send files, photos and links between this PC and your phone with a QR code.",
        "Alt+Space, then type: apps, files, settings and quick sums, instantly.",
        "An encrypted vault for your passwords and 2FA codes, with auto-type.",
        "Your photos and videos: an instant viewer, a timeline and a player.",
        "See what slows your PC down, and stop it in one click.",
        "See what fills your drives and free space safely, step by step.",
        "Automatic backups of your folders to a drive or another PC.",
        "Clipboard history with Ctrl+Shift+V, pinned items and snippets.",
        "Read, annotate, sign, merge and split PDF files.",
        "Your agenda with reminders, recurring events and subscribed calendars.",
        "Drop a video, song or picture and get it in the format you need.",
        "Replaces Windows Notepad: tabs, find and replace, and sticky pads that stay on top.",
        "Edit your videos on a timeline: cuts, text, transitions and music, then export.",
        "Photo editing and graphic design in one app: layers, looks, text and templates.",
        "Measured performance tweaks for Windows, each one undoable in one click.",
    ],
    installed: "Installed",
    not_installed: "Not installed",
    coming_soon: "Coming soon",
    fex_default: "Folders and Win+E now open in Plume FEx. To undo it: Plume FEx, … menu.",
    install: "Install",
    open: "Open",
    remove: "Remove",
    cancel: "Cancel",
    installing: "Installing…",
    removing: "Removing…",
    checking: "Checking for updates…",
    up_to_date: "Plume Suite {v} · up to date",
    dev_build: "Plume Suite {v} · development build: updates are checked once installed",
    offline: "Plume Suite {v} · could not check for updates",
    available: "Plume Suite {v} is available",
    available_sub: "Every installed app is updated at once. Open apps take the new version when you restart them.",
    update_all: "Update everything",
    downloading: "Downloading {v}… {p} %",
    updating: "Installing the update…",
    updated: "Everything is up to date. Restart the apps that were open to use the new version.",
    update_failed: "The update did not work: {e}",
    retry: "Try again",
    check_now: "Check now",
    missing_setup: "The installer is missing from the Plume folder. Download Plume again from the website.",
    failed: "That did not work (error {e}).",
    confirm_remove: "Remove {app}?\n\nYour data stays on this PC: installing it again brings everything back.",
    confirm_remove_needed: "Remove {app}?\n\n{users} uses it and will ask to install it again. Your data stays on this PC.",
    installs_too: "Also installs {apps}, which it needs.",
    website: "Website",
    notes: "Release notes",
    set_default: "Set as default",
    is_default: "Default ✓",
    opening: "Opening…",
    default_explain: "Make {app} your default app\n\nWindows asks you to confirm it. In the page that opens, click Set default at the top. On older versions of Windows, choose {app} for each file type instead.",
    continue_: "Continue",
    default_opened: "Windows Settings is open: click Set default to finish.",
};

const FR: Strings = Strings {
    title: "Plume Suite",
    welcome: "Vos applis Plume",
    sub_none: "Choisissez les applis que vous voulez. Chacune s'installe en une seconde, sans droits administrateur.",
    sub_some: "Ouvrez-les, ajoutez-les ou retirez-les ici. Une seule mise à jour les met toutes à jour.",
    desc: [
        "Le navigateur léger : onglets en veille, bloqueur intégré, mots de passe chiffrés.",
        "L'explorateur de fichiers qui trouve n'importe quel fichier de votre PC pendant que vous tapez.",
        "Vos e-mails, affichés en texte sûr : pas de pixels espions, pas de scripts, pas de pubs.",
        "Contrôlez ce PC depuis votre téléphone : un QR code, et l'écran est à vous.",
        "Votre musique et YouTube Music, vos playlists Spotify et Apple Music, les paroles synchronisées.",
        "Envoyez fichiers, photos et liens entre ce PC et votre téléphone avec un QR code.",
        "Alt+Espace, puis tapez : applis, fichiers, paramètres et calculs, instantanément.",
        "Un coffre chiffré pour vos mots de passe et codes 2FA, avec saisie automatique.",
        "Vos photos et vidéos : une visionneuse instantanée, une frise et un lecteur.",
        "Voyez ce qui ralentit votre PC, et arrêtez-le en un clic.",
        "Voyez ce qui remplit vos disques et libérez de la place sans risque.",
        "Des sauvegardes automatiques de vos dossiers sur un disque ou un autre PC.",
        "L'historique du presse-papiers avec Ctrl+Maj+V, les éléments épinglés et les extraits.",
        "Lisez, annotez, signez, fusionnez et découpez vos PDF.",
        "Votre agenda avec rappels, événements récurrents et calendriers abonnés.",
        "Déposez une vidéo, un son ou une image et obtenez le format qu'il vous faut.",
        "Remplace le Bloc-notes de Windows : onglets, rechercher-remplacer et pense-bêtes toujours au premier plan.",
        "Montez vos vidéos sur une frise : coupes, textes, transitions et musique, puis exportez.",
        "Retouche photo et création graphique en une seule appli : calques, looks, textes et modèles.",
        "Des réglages de performance mesurés pour Windows, chacun annulable en un clic.",
    ],
    installed: "Installée",
    not_installed: "Pas installée",
    coming_soon: "Bientôt",
    fex_default: "Les dossiers et Win+E s'ouvrent maintenant dans Plume FEx. Pour l'annuler : Plume FEx, menu ….",
    install: "Installer",
    open: "Ouvrir",
    remove: "Retirer",
    cancel: "Annuler",
    installing: "Installation…",
    removing: "Retrait…",
    checking: "Recherche de mises à jour…",
    up_to_date: "Plume Suite {v} · à jour",
    dev_build: "Plume Suite {v} · version de développement : les mises à jour sont vérifiées une fois installée",
    offline: "Plume Suite {v} · impossible de vérifier les mises à jour",
    available: "Plume Suite {v} est disponible",
    available_sub: "Toutes les applis installées sont mises à jour d'un coup. Celles qui sont ouvertes prennent la nouvelle version à leur redémarrage.",
    update_all: "Tout mettre à jour",
    downloading: "Téléchargement de {v}… {p} %",
    updating: "Installation de la mise à jour…",
    updated: "Tout est à jour. Redémarrez les applis qui étaient ouvertes pour utiliser la nouvelle version.",
    update_failed: "La mise à jour n'a pas abouti : {e}",
    retry: "Réessayer",
    check_now: "Vérifier",
    missing_setup: "L'installeur manque dans le dossier de Plume. Téléchargez de nouveau Plume depuis le site.",
    failed: "Cela n'a pas marché (erreur {e}).",
    confirm_remove: "Retirer {app} ?\n\nVos données restent sur ce PC : le réinstaller retrouve tout.",
    confirm_remove_needed: "Retirer {app} ?\n\n{users} s'en sert et proposera de le réinstaller. Vos données restent sur ce PC.",
    installs_too: "Installe aussi {apps}, dont elle a besoin.",
    website: "Site web",
    notes: "Notes de version",
    set_default: "Définir par défaut",
    is_default: "Par défaut ✓",
    opening: "Ouverture…",
    default_explain: "Faire de {app} votre appli par défaut\n\nWindows vous demande de le confirmer. Dans la page qui s'ouvre, cliquez sur Définir par défaut, en haut. Sur les versions plus anciennes de Windows, choisissez {app} pour chaque type de fichier.",
    continue_: "Continuer",
    default_opened: "Les Paramètres de Windows sont ouverts : cliquez sur Définir par défaut pour terminer.",
};

/// Langue choisie dans Plume (le navigateur, ou Plume Suite), sinon celle de Windows.
pub fn french() -> bool {
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

pub fn strings(fr: bool) -> &'static Strings {
    if fr { &FR } else { &EN }
}

/// Remplace {v}, {p}, {e} et {app}.
pub fn fill(s: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = s.to_string();
    for (k, v) in pairs {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}
