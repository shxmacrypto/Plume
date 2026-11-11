//! Les textes de Plume Suite, en anglais et en français. La langue est celle choisie dans Plume, sinon celle de Windows.

use windows::Win32::Globalization::GetUserDefaultUILanguage;

pub struct Strings {
    pub title: &'static str,
    pub welcome: &'static str,
    pub sub_none: &'static str,
    pub sub_some: &'static str,
    pub desc: [&'static str; 3],
    pub installed: &'static str,
    pub not_installed: &'static str,
    pub coming_soon: &'static str,
    pub fex_default: &'static str,
    pub install: &'static str,
    pub open: &'static str,
    pub remove: &'static str,
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
    pub website: &'static str,
    pub notes: &'static str,
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
    ],
    installed: "Installed",
    not_installed: "Not installed",
    coming_soon: "Coming soon",
    fex_default: "Folders and Win+E now open in Plume FEx. To undo it: Plume FEx, … menu.",
    install: "Install",
    open: "Open",
    remove: "Remove",
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
    website: "Website",
    notes: "Release notes",
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
    ],
    installed: "Installée",
    not_installed: "Pas installée",
    coming_soon: "Bientôt",
    fex_default: "Les dossiers et Win+E s'ouvrent maintenant dans Plume FEx. Pour l'annuler : Plume FEx, menu ….",
    install: "Installer",
    open: "Ouvrir",
    remove: "Retirer",
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
    website: "Site web",
    notes: "Notes de version",
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
