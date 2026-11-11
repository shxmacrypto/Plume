//! English and French. The interface is written in English; `tr` gives the French when Plume speaks French
//! (this app's choice, else the suite's, else Windows').

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
    fr(en).unwrap_or(en)
}

fn fr(en: &str) -> Option<&'static str> {
    Some(match en {
        "2FA code" => "Code 2FA",
        "2FA codes" => "Codes 2FA",
        "2FA key read from the picture." => "Clé 2FA lue dans l'image.",
        "2FA key" => "Clé 2FA",
        "A CSV file from Chrome, Edge, Firefox, Bitwarden or Plume Web" => "Un fichier CSV de Chrome, Edge, Firefox, Bitwarden ou Plume Web",
        "A CSV file of passwords" => "Un fichier CSV de mots de passe",
        "A Plume Passwords backup" => "Une sauvegarde de Plume Passwords",
        "A copy of the vault that opens with your master password" => "Une copie du coffre, qui s'ouvre avec votre mot de passe maître",
        "A picture of the QR code" => "Une image du code QR",
        "Account" => "Compte",
        "Add a digit" => "Ajouter un chiffre",
        "Adds the items of a backup to this vault" => "Ajoute les éléments d'une sauvegarde à ce coffre",
        "All good: no weak or reused passwords." => "Tout va bien : aucun mot de passe faible ou réutilisé.",
        "All items" => "Tous les éléments",
        "Another program uses this shortcut: choose another one" => "Un autre programme utilise ce raccourci : choisissez-en un autre",
        "App" => "Application",
        "Auto-type shortcut" => "Raccourci de saisie automatique",
        "Auto-type" => "Saisie auto",
        "Backup saved. It opens with your master password." => "Sauvegarde enregistrée. Elle s'ouvre avec votre mot de passe maître.",
        "Between the words" => "Entre les mots",
        "Cancel" => "Annuler",
        "Capital letters (A-Z)" => "Majuscules (A-Z)",
        "Capital letters" => "Majuscules",
        "Card number" => "Numéro de carte",
        "Card" => "Carte",
        "Cardholder" => "Titulaire",
        "Cards" => "Cartes",
        "Change the master password" => "Changer le mot de passe maître",
        "Change" => "Changer",
        "Change…" => "Changer…",
        "Choose a master password. It opens your vault and cannot be recovered: keep it safe." => {
            "Choisissez un mot de passe maître. Il ouvre votre coffre et ne peut pas être récupéré : gardez-le bien."
        }
        "Choose the login to type into:" => "Choisissez l'identifiant à saisir dans :",
        "Clear the clipboard after" => "Vider le presse-papiers après",
        "Click in the other window's field first, then press the shortcut." => {
            "Cliquez d'abord dans le champ de l'autre fenêtre, puis appuyez sur le raccourci."
        }
        "Closing the window keeps auto-type working" => "Fermer la fenêtre laisse la saisie automatique active",
        "Copied secrets also stay out of the clipboard history" => "Les secrets copiés restent aussi hors de l'historique du presse-papiers",
        "Copied. The clipboard clears in {n} s." => "Copié. Le presse-papiers se vide dans {n} s.",
        "Copy" => "Copier",
        "Could not open the vault: {e}" => "Impossible d'ouvrir le coffre : {e}",
        "Could not read that file." => "Impossible de lire ce fichier.",
        "Could not read that picture." => "Impossible de lire cette image.",
        "Could not save the backup: {e}" => "Impossible d'enregistrer la sauvegarde : {e}",
        "Could not save the vault: {e}" => "Impossible d'enregistrer le coffre : {e}",
        "Could not type into that window." => "Impossible de saisir dans cette fenêtre.",
        "Create vault" => "Créer le coffre",
        "Create your vault" => "Créez votre coffre",
        "Create" => "Créer",
        "Creating your vault…" => "Création du coffre…",
        "Current master password" => "Mot de passe maître actuel",
        "Dark" => "Sombre",
        "Delete \u{201c}{name}\u{201d}?" => "Supprimer « {name} » ?",
        "Delete" => "Supprimer",
        "Deleted." => "Supprimé.",
        "Digits (0-9)" => "Chiffres (0-9)",
        "Edit" => "Modifier",
        "Edited {d}" => "Modifié le {d}",
        "Email or user name" => "E-mail ou nom d'utilisateur",
        "Encrypted backup" => "Sauvegarde chiffrée",
        "Enter your master password." => "Entrez votre mot de passe maître.",
        "Example: Mail" => "Exemple : Mail",
        "Expires" => "Expire",
        "Export…" => "Exporter…",
        "FOLDERS" => "DOSSIERS",
        "Face, fingerprint or PIN instead of the master password" => "Visage, empreinte ou code PIN à la place du mot de passe maître",
        "Fair" => "Moyen",
        "Favorites" => "Favoris",
        "Folder" => "Dossier",
        "From a QR picture…" => "Depuis une image QR…",
        "Generator" => "Générateur",
        "Give it a name." => "Donnez-lui un nom.",
        "Import passwords" => "Importer des mots de passe",
        "Imported: {n}. Already in the vault: {d}. Delete the CSV file: it holds your passwords in clear." => {
            "Importés : {n}. Déjà dans le coffre : {d}. Supprimez le fichier CSV : il contient vos mots de passe en clair."
        }
        "Import…" => "Importer…",
        "It cannot be brought back." => "Impossible de le récupérer ensuite.",
        "It cannot be recovered if you forget it" => "Impossible de le récupérer si vous l'oubliez",
        "Items of {file} that are not in your vault are added; newer versions replace older ones." => {
            "Les éléments de {file} absents de votre coffre sont ajoutés ; les versions plus récentes remplacent les anciennes."
        }
        "Its items stay in the vault, without a folder." => "Ses éléments restent dans le coffre, sans dossier.",
        "Keep running near the clock" => "Rester près de l'horloge",
        "Kind" => "Type",
        "Language" => "Langue",
        "Leave out look-alikes (0 O, 1 l I)" => "Éviter les caractères qui se ressemblent (0 O, 1 l I)",
        "Length: {n}" => "Longueur : {n}",
        "Light" => "Clair",
        "Lock after" => "Verrouiller après",
        "Lock when Windows locks" => "Verrouiller quand Windows se verrouille",
        "Lock" => "Verrouiller",
        "Login" => "Identifiant",
        "Logins without 2FA" => "Identifiants sans 2FA",
        "Logins" => "Identifiants",
        "Master password changed." => "Mot de passe maître changé.",
        "Master password" => "Mot de passe maître",
        "Name on the card" => "Nom sur la carte",
        "Name" => "Nom",
        "Never" => "Jamais",
        "New 2FA code" => "Nouveau code 2FA",
        "New card" => "Nouvelle carte",
        "New folder" => "Nouveau dossier",
        "New login" => "Nouvel identifiant",
        "New master password" => "Nouveau mot de passe maître",
        "New secure note" => "Nouvelle note sécurisée",
        "New" => "Nouveau",
        "No QR code found in that picture." => "Aucun code QR dans cette image.",
        "No folders yet" => "Aucun dossier",
        "None" => "Aucun",
        "Not set up on this PC" => "Non configuré sur ce PC",
        "Notes" => "Notes",
        "Nothing here yet." => "Rien ici pour l'instant.",
        "Nothing is sent anywhere: the vault stays on this PC." => "Rien n'est envoyé nulle part : le coffre reste sur ce PC.",
        "Nothing matches your search." => "Aucun résultat.",
        "Open Plume Passwords" => "Ouvrir Plume Passwords",
        "Or press {key} in the other window." => "Ou appuyez sur {key} dans l'autre fenêtre.",
        "Passphrase" => "Phrase de passe",
        "Password" => "Mot de passe",
        "Passwords that are easy to guess, and passwords used for more than one site." => {
            "Les mots de passe faciles à deviner, et ceux utilisés pour plusieurs sites."
        }
        "Plume Passwords backup" => "Sauvegarde Plume Passwords",
        "Press Enter after the password" => "Appuyer sur Entrée après le mot de passe",
        "Quit" => "Quitter",
        "Remove the folder \u{201c}{name}\u{201d}?" => "Retirer le dossier « {name} » ?",
        "Remove" => "Retirer",
        "Restore a backup" => "Restaurer une sauvegarde",
        "Restore from a backup" => "Restaurer depuis une sauvegarde",
        "Restore" => "Restaurer",
        "Restore…" => "Restaurer…",
        "Reused passwords" => "Mots de passe réutilisés",
        "Reused" => "Réutilisés",
        "Same password, {n} items" => "Même mot de passe, {n} éléments",
        "Save the backup" => "Enregistrer la sauvegarde",
        "Save" => "Enregistrer",
        "Search" => "Rechercher",
        "Secret key or otpauth:// address" => "Clé secrète ou adresse otpauth://",
        "Secure note" => "Note sécurisée",
        "Secure notes" => "Notes sécurisées",
        "Security code" => "Code de sécurité",
        "Security report" => "Rapport de sécurité",
        "Security" => "Sécurité",
        "Select an item, or add one with +." => "Choisissez un élément, ou ajoutez-en un avec +.",
        "Separated by commas" => "Séparés par des virgules",
        "Settings" => "Réglages",
        "Shortcut" => "Raccourci",
        "Small letters (a-z)" => "Minuscules (a-z)",
        "Start with Windows" => "Démarrer avec Windows",
        "Strong passwords and passphrases, made on this PC." => "Des mots de passe et phrases de passe solides, créés sur ce PC.",
        "Strong" => "Fort",
        "Symbols (!@#$…)" => "Symboles (!@#$…)",
        "System" => "Système",
        "Tags" => "Étiquettes",
        "That QR code is not a 2FA key." => "Ce code QR n'est pas une clé 2FA.",
        "The account the code is for" => "Le compte de ce code",
        "The backup's master password" => "Le mot de passe maître de la sauvegarde",
        "The current master password is wrong." => "Le mot de passe maître actuel est faux.",
        "The two passwords are not the same." => "Les deux mots de passe sont différents.",
        "Theme" => "Thème",
        "This 2FA key is not valid: paste the secret or the otpauth:// address." => {
            "Cette clé 2FA n'est pas valide : collez le secret ou l'adresse otpauth://."
        }
        "This CSV file has no password columns." => "Ce fichier CSV n'a pas de colonnes de mots de passe.",
        "This file is not a Plume Passwords backup." => "Ce fichier n'est pas une sauvegarde de Plume Passwords.",
        "Turn on Windows Hello for Plume Passwords" => "Activer Windows Hello pour Plume Passwords",
        "Type it again" => "Retapez-le",
        "Types the user name, Tab and the password into the window in front" => {
            "Saisit le nom d'utilisateur, Tab et le mot de passe dans la fenêtre au premier plan"
        }
        "Types the user name, Tab and the password into the window you used last." => {
            "Saisit le nom d'utilisateur, Tab et le mot de passe dans la dernière fenêtre utilisée."
        }
        "Unlock Plume Passwords" => "Déverrouiller Plume Passwords",
        "Unlock with Windows Hello" => "Déverrouiller avec Windows Hello",
        "Unlock your vault" => "Déverrouillez votre coffre",
        "Unlock" => "Déverrouiller",
        "Unlock, and the login will be typed." => "Déverrouillez, et l'identifiant sera saisi.",
        "Unlocking…" => "Déverrouillage…",
        "Use at least 8 characters." => "Utilisez au moins 8 caractères.",
        "User name" => "Nom d'utilisateur",
        "Very strong" => "Très fort",
        "Very weak" => "Très faible",
        "Waiting for Windows Hello…" => "En attente de Windows Hello…",
        "Weak passwords" => "Mots de passe faibles",
        "Weak" => "Faible",
        "Website" => "Site web",
        "Windows Hello did not confirm. Use your master password." => "Windows Hello n'a pas confirmé. Utilisez votre mot de passe maître.",
        "Windows Hello did not confirm." => "Windows Hello n'a pas confirmé.",
        "Windows Hello is not set up on this PC." => "Windows Hello n'est pas configuré sur ce PC.",
        "Windows did not let us copy. Try again." => "Windows n'a pas permis de copier. Réessayez.",
        "Without using Plume Passwords" => "Sans utiliser Plume Passwords",
        "Words: {n}" => "Mots : {n}",
        "Wrong master password." => "Mot de passe maître incorrect.",
        "Wrong password for this backup." => "Mot de passe incorrect pour cette sauvegarde.",
        "Your data" => "Vos données",
        "Your vault is empty. Add a login with +, or import a CSV file in Settings." => {
            "Votre coffre est vide. Ajoutez un identifiant avec +, ou importez un fichier CSV dans Réglages."
        }
        "Your vault is encrypted and never leaves this PC." => "Votre coffre est chiffré et ne quitte jamais ce PC.",
        "Your vault is sealed again with the new password. Backups keep their old password." => {
            "Votre coffre est rescellé avec le nouveau mot de passe. Les sauvegardes gardent l'ancien."
        }
        "space" => "espace",
        "{n} bits of randomness" => "{n} bits d'aléatoire",
        "Restored from the backup: {n}." => "Restaurés depuis la sauvegarde : {n}.",
        "{n} s" => "{n} s",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes() {
        assert_eq!(fill("{a} of {b}", &["3", "10"]), "3 of 10");
        assert_eq!(fill("no holes", &["x"]), "no holes");
    }

    /// Every English text of the interface has its French, with the same holes in the same order.
    #[test]
    fn every_text_has_its_french() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![];
        let mut stack = vec![dir];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("i18n.rs") {
                    files.push(p);
                }
            }
        }
        let mut missing = vec![];
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            let mut rest = src.as_str();
            while let Some(i) = rest.find("tr(\"") {
                rest = &rest[i + 4..];
                let end = rest.find("\")").unwrap();
                let en = rest[..end].replace("\\u{201c}", "\u{201c}").replace("\\u{201d}", "\u{201d}");
                match fr(&en) {
                    Some(fr) => {
                        let holes = |s: &str| s.matches('{').count();
                        assert_eq!(holes(&en), holes(fr), "holes of {en}");
                    }
                    None => missing.push(en),
                }
            }
        }
        assert!(missing.is_empty(), "no French for: {missing:?}");
    }
}
