//! English and French. The window is written in English; `tr` gives the French when Plume speaks French (the
//! setting, otherwise Plume's choice, otherwise Windows').

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
        "Send to your phone" => "Envoyer vers votre téléphone",
        "Scan with your phone" => "Scannez avec votre téléphone",
        "Open the camera and point it at the code. Same Wi-Fi, nothing to install." => {
            "Ouvrez l'appareil photo et visez le code. Même Wi-Fi, rien à installer."
        }
        "Copy link" => "Copier le lien",
        "Copied" => "Copié",
        "New code" => "Nouveau code",
        "The first time, Windows Firewall may ask whether Plume Drop can use the network: allow it on private networks." => {
            "La première fois, le Pare-feu Windows peut demander si Plume Drop peut utiliser le réseau : autorisez-le sur les réseaux privés."
        }
        "Not on a network" => "Pas de réseau",
        "Connect this PC to Wi-Fi or Ethernet." => "Connectez ce PC au Wi-Fi ou à un câble réseau.",
        "Can't start" => "Démarrage impossible",
        "Ready" => "Prêt",
        "Drop files here" => "Déposez des fichiers ici",
        "Phones that are connected can save them." => "Les téléphones connectés peuvent les enregistrer.",
        "Send files…" => "Envoyer des fichiers…",
        "Send text…" => "Envoyer un texte…",
        "Text or link for your phone" => "Texte ou lien pour votre téléphone",
        "Send" => "Envoyer",
        "Cancel" => "Annuler",
        "Devices" => "Appareils",
        "No device yet. Scan the code to connect one." => "Aucun appareil. Scannez le code pour en connecter un.",
        "Connected" => "Connecté",
        "Not connected" => "Non connecté",
        "Waiting for your OK" => "Attend votre accord",
        "Disconnect" => "Déconnecter",
        "Forget" => "Oublier",
        "History" => "Historique",
        "Clear" => "Effacer",
        "Nothing sent or received yet." => "Rien d'envoyé ni de reçu pour l'instant.",
        "from {}" => "de {}",
        "to phones" => "vers les téléphones",
        "Offered" => "Proposé",
        "saved by {}" => "enregistré par {}",
        "Received" => "Reçu",
        "Failed" => "Échec",
        "Sent" => "Envoyé",
        "Open" => "Ouvrir",
        "Show in folder" => "Afficher dans le dossier",
        "Copy" => "Copier",
        "Stop" => "Retirer",
        "{} wants to connect" => "{} veut se connecter",
        "Allow it to send files to this PC and to save what you share. It is asked only once per device." => {
            "Autorisez-le à envoyer des fichiers vers ce PC et à enregistrer ce que vous partagez. Ce n'est demandé qu'une fois par appareil."
        }
        "Allow" => "Autoriser",
        "Refuse" => "Refuser",
        "Settings" => "Réglages",
        "Back" => "Retour",
        "Received files" => "Fichiers reçus",
        "Change…" => "Modifier…",
        "Open folder" => "Ouvrir le dossier",
        "Received text" => "Texte reçu",
        "Copy it to the clipboard" => "Le copier dans le presse-papiers",
        "Received links" => "Liens reçus",
        "Open them in the browser" => "Les ouvrir dans le navigateur",
        "Language" => "Langue",
        "System" => "Système",
        "Security" => "Sécurité",
        "Phones need the code in the QR code, and your OK the first time. New code makes every phone scan again." => {
            "Les téléphones ont besoin du code du QR code, et de votre accord la première fois. Nouveau code oblige chaque téléphone à scanner de nouveau."
        }
        "About" => "À propos",
        "Phones talk straight to this PC on your Wi-Fi. Nothing goes through the internet, no account." => {
            "Les téléphones parlent directement à ce PC sur votre Wi-Fi. Rien ne passe par internet, sans compte."
        }
        "Choose where received files go" => "Choisir où vont les fichiers reçus",
        "Choose files to send" => "Choisir les fichiers à envoyer",
        "Text copied to the clipboard" => "Texte copié dans le presse-papiers",
        "Link received" => "Lien reçu",
        "New code: phones must scan again" => "Nouveau code : les téléphones doivent scanner de nouveau",
        "{} disconnected" => "{} déconnecté",
        "{} file(s) offered to your phones" => "{} fichier(s) proposé(s) à vos téléphones",
        "Offered to your phones" => "Proposé à vos téléphones",
        "Received {}" => "Reçu : {}",
        "Address" => "Adresse",
        "now" => "à l'instant",
        "{} min ago" => "il y a {} min",
        "Today" => "Aujourd'hui",
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
        assert_eq!(fill("{} wants", &[]), " wants");
    }

    #[test]
    fn french_texts() {
        set("fr");
        assert_eq!(tr("Allow"), "Autoriser");
        assert_eq!(tr("not translated"), "not translated");
        set("en");
        assert_eq!(tr("Allow"), "Allow");
    }
}
