//! Windows' settings pages: their "ms-settings:" address, their name in English and in French, and the words
//! people type to find them (both languages).

use crate::rank::{fold, score};

pub struct Page {
    pub uri: &'static str,
    pub en: &'static str,
    pub fr: &'static str,
    /// Words, separated by commas.
    pub words: &'static str,
}

const fn p(uri: &'static str, en: &'static str, fr: &'static str, words: &'static str) -> Page {
    Page { uri, en, fr, words }
}

pub const PAGES: &[Page] = &[
    p("ms-settings:display", "Display", "Écran", "screen, resolution, scale, brightness, monitor, résolution, échelle, luminosité, moniteur, affichage"),
    p("ms-settings:nightlight", "Night light", "Éclairage nocturne", "blue light, warm, lumière bleue, nuit"),
    p("ms-settings:sound", "Sound", "Son", "audio, volume, speakers, microphone, output, input, haut-parleurs, micro, sortie, entrée"),
    p("ms-settings:apps-volume", "Volume mixer", "Mélangeur de volume", "app volume, sound per app, volume des applications"),
    p("ms-settings:notifications", "Notifications", "Notifications", "alerts, do not disturb, focus, alertes, ne pas déranger"),
    p("ms-settings:quiethours", "Do not disturb", "Ne pas déranger", "focus, quiet hours, concentration"),
    p("ms-settings:powersleep", "Power & sleep", "Alimentation et mise en veille", "sleep, screen off, power, veille, énergie, alimentation"),
    p("ms-settings:batterysaver", "Battery", "Batterie", "battery saver, économiseur de batterie, power"),
    p("ms-settings:storagesense", "Storage", "Stockage", "disk space, storage sense, cleanup, espace disque, assistant stockage, nettoyage"),
    p("ms-settings:multitasking", "Multitasking", "Multitâche", "snap, alt tab, virtual desktops, ancrage, bureaux virtuels"),
    p("ms-settings:clipboard", "Clipboard", "Presse-papiers", "clipboard history, historique du presse-papiers, copy, paste, copier, coller"),
    p("ms-settings:remotedesktop", "Remote Desktop", "Bureau à distance", "rdp, remote, à distance"),
    p(
        "ms-settings:about",
        "About this PC",
        "Informations système",
        "about, system, pc name, rename, specs, version, windows version, à propos, nom du pc, renommer, caractéristiques",
    ),
    p("ms-settings:bluetooth", "Bluetooth & devices", "Bluetooth et appareils", "bluetooth, pair, devices, appareils, associer, casque, headphones"),
    p("ms-settings:printers", "Printers & scanners", "Imprimantes et scanners", "printer, scanner, print, imprimante, imprimer"),
    p("ms-settings:mousetouchpad", "Mouse", "Souris", "mouse, cursor speed, scroll, pointeur, défilement, vitesse"),
    p("ms-settings:devices-touchpad", "Touchpad", "Pavé tactile", "touchpad, trackpad, gestures, gestes"),
    p("ms-settings:typing", "Typing", "Saisie", "keyboard, autocorrect, spelling, clavier, correction, orthographe"),
    p("ms-settings:autoplay", "AutoPlay", "Exécution automatique", "autoplay, usb, exécution automatique"),
    p("ms-settings:usb", "USB", "USB", "usb, devices, appareils"),
    p("ms-settings:network-status", "Network & internet", "Réseau et Internet", "network, internet, connection, réseau, connexion"),
    p("ms-settings:network-wifi", "Wi-Fi", "Wi-Fi", "wifi, wireless, wlan, sans fil, réseau"),
    p("ms-settings:network-ethernet", "Ethernet", "Ethernet", "ethernet, cable, lan, câble"),
    p("ms-settings:network-vpn", "VPN", "VPN", "vpn, private network, réseau privé"),
    p("ms-settings:network-proxy", "Proxy", "Proxy", "proxy, network, réseau"),
    p("ms-settings:network-mobilehotspot", "Mobile hotspot", "Point d'accès sans fil mobile", "hotspot, share connection, partage de connexion, tethering"),
    p("ms-settings:network-airplanemode", "Airplane mode", "Mode avion", "airplane, flight, avion"),
    p("ms-settings:personalization", "Personalization", "Personnalisation", "personalize, look, apparence, personnaliser"),
    p("ms-settings:personalization-background", "Background", "Arrière-plan", "wallpaper, desktop background, fond d'écran, image du bureau"),
    p("ms-settings:colors", "Colors", "Couleurs", "dark mode, light mode, accent color, theme, mode sombre, mode clair, couleur d'accentuation, thème"),
    p("ms-settings:themes", "Themes", "Thèmes", "theme, icons, desktop icons, thème, icônes du bureau"),
    p("ms-settings:lockscreen", "Lock screen", "Écran de verrouillage", "lock, verrouillage"),
    p("ms-settings:personalization-start", "Start", "Démarrer", "start menu, menu démarrer"),
    p("ms-settings:taskbar", "Taskbar", "Barre des tâches", "taskbar, tray, system tray, barre des tâches, zone de notification"),
    p("ms-settings:fonts", "Fonts", "Polices", "fonts, typeface, polices de caractères"),
    p(
        "ms-settings:appsfeatures",
        "Installed apps",
        "Applications installées",
        "uninstall, programs, apps, remove, désinstaller, programmes, applications, supprimer",
    ),
    p("ms-settings:defaultapps", "Default apps", "Applications par défaut", "default browser, open with, navigateur par défaut, ouvrir avec"),
    p("ms-settings:startupapps", "Startup apps", "Applications de démarrage", "startup, launch at startup, démarrage, lancement"),
    p("ms-settings:optionalfeatures", "Optional features", "Fonctionnalités facultatives", "features, fonctionnalités"),
    p("ms-settings:yourinfo", "Your info", "Vos informations", "account, profile picture, compte, photo de profil"),
    p(
        "ms-settings:signinoptions",
        "Sign-in options",
        "Options de connexion",
        "password, pin, windows hello, fingerprint, face, mot de passe, empreinte, visage",
    ),
    p("ms-settings:emailandaccounts", "Email & accounts", "E-mail et comptes", "email, accounts, mail, comptes"),
    p("ms-settings:otherusers", "Other users", "Autres utilisateurs", "users, family, add user, utilisateurs, famille, ajouter un utilisateur"),
    p("ms-settings:dateandtime", "Date & time", "Date et heure", "time, clock, time zone, date, heure, horloge, fuseau horaire"),
    p("ms-settings:regionlanguage", "Language & region", "Langue et région", "language, region, keyboard layout, langue, région, disposition du clavier"),
    p("ms-settings:speech", "Speech", "Voix", "speech, voice, dictation, reconnaissance vocale, dictée"),
    p("ms-settings:gaming-gamebar", "Game Bar", "Game Bar", "xbox, game bar, capture, jeux"),
    p("ms-settings:gaming-gamemode", "Game Mode", "Mode Jeu", "game mode, gaming, jeu, jeux"),
    p("ms-settings:easeofaccess-display", "Text size", "Taille du texte", "accessibility, bigger text, accessibilité, texte plus grand"),
    p("ms-settings:easeofaccess-narrator", "Narrator", "Narrateur", "screen reader, accessibility, lecteur d'écran, accessibilité"),
    p("ms-settings:easeofaccess-magnifier", "Magnifier", "Loupe", "zoom, accessibility, loupe, accessibilité"),
    p("ms-settings:easeofaccess-highcontrast", "Contrast themes", "Thèmes de contraste", "high contrast, contraste élevé, accessibility"),
    p(
        "ms-settings:easeofaccess-keyboard",
        "Keyboard accessibility",
        "Accessibilité du clavier",
        "sticky keys, on-screen keyboard, touches rémanentes, clavier visuel",
    ),
    p("ms-settings:privacy", "Privacy & security", "Confidentialité et sécurité", "privacy, security, confidentialité, sécurité"),
    p("ms-settings:privacy-location", "Location", "Localisation", "location, gps, position, localisation"),
    p("ms-settings:privacy-webcam", "Camera permissions", "Autorisations de la caméra", "camera, webcam, caméra"),
    p("ms-settings:privacy-microphone", "Microphone permissions", "Autorisations du microphone", "microphone, mic, micro"),
    p("ms-settings:windowsdefender", "Windows Security", "Sécurité Windows", "antivirus, defender, firewall, virus, pare-feu"),
    p("ms-settings:windowsupdate", "Windows Update", "Windows Update", "update, updates, upgrade, mise à jour, mises à jour"),
    p("ms-settings:windowsupdate-history", "Update history", "Historique des mises à jour", "updates installed, historique, mises à jour installées"),
    p("ms-settings:recovery", "Recovery", "Récupération", "reset, restore, reset this pc, réinitialiser, restaurer"),
    p("ms-settings:activation", "Activation", "Activation", "product key, license, clé de produit, licence"),
    p("ms-settings:backup", "Windows Backup", "Sauvegarde Windows", "backup, sauvegarde, onedrive"),
    p("ms-settings:developers", "For developers", "Pour les développeurs", "developer mode, sudo, mode développeur"),
    p("ms-settings:troubleshoot", "Troubleshoot", "Résolution des problèmes", "fix, problems, résoudre, problèmes, dépannage"),
    p("ms-settings:powersleep", "Screen timeout", "Délai de mise en veille de l'écran", "screen off, turn off display, éteindre l'écran"),
    p("ms-settings:", "Settings", "Paramètres", "settings, control panel, paramètres, réglages, panneau de configuration"),
];

/// The settings pages that match `q` (folded), with their score; the name in the interface's language first.
pub fn search(q: &str, french: bool) -> Vec<(&'static Page, i32)> {
    let mut out = vec![];
    for page in PAGES {
        let (main, other) = if french { (page.fr, page.en) } else { (page.en, page.fr) };
        // a page's name must really match (not a few scattered letters)
        let mut best = score(q, &fold(main)).filter(|s| *s >= 500);
        if let Some(s) = score(q, &fold(other)).filter(|s| *s >= 500) {
            best = Some(best.unwrap_or(0).max(s - 40));
        }
        for w in page.words.split(',') {
            if let Some(s) = score(q, &fold(w.trim())) {
                // a keyword counts less than the name, and a fuzzy keyword match not at all
                if s >= 500 {
                    best = Some(best.unwrap_or(0).max(s - 160));
                }
            }
        }
        if let Some(s) = best {
            out.push((page, s));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(q: &str, fr: bool) -> &'static str {
        let mut v = search(&fold(q), fr);
        v.sort_by_key(|a| std::cmp::Reverse(a.1));
        v.first().map_or("", |(p, _)| p.uri)
    }

    #[test]
    fn keywords_both_languages() {
        assert_eq!(top("wifi", false), "ms-settings:network-wifi");
        assert_eq!(top("fond d'ecran", true), "ms-settings:personalization-background");
        assert_eq!(top("wallpaper", false), "ms-settings:personalization-background");
        assert_eq!(top("dark mode", false), "ms-settings:colors");
        assert_eq!(top("mode sombre", true), "ms-settings:colors");
        assert_eq!(top("uninstall", false), "ms-settings:appsfeatures");
        assert_eq!(top("désinstaller", true), "ms-settings:appsfeatures");
        assert_eq!(top("bluetooth", false), "ms-settings:bluetooth");
        assert_eq!(top("ecran", true), "ms-settings:display");
        assert_eq!(top("son", true), "ms-settings:sound");
        assert_eq!(top("mise a jour", true), "ms-settings:windowsupdate");
    }

    #[test]
    fn every_page_is_well_formed() {
        for p in PAGES {
            assert!(p.uri.starts_with("ms-settings:"), "{}", p.uri);
            assert!(!p.en.is_empty() && !p.fr.is_empty() && !p.words.is_empty());
        }
        // a name typed in the other language still finds its page
        assert_eq!(top("display", true), "ms-settings:display");
    }

    #[test]
    fn nothing_for_noise() {
        assert!(search(&fold("qqqzzz"), false).is_empty());
        // "note" is not "Éclairage nocturne" nor "Windows Update"
        assert!(search(&fold("note"), true).is_empty(), "{:?}", search(&fold("note"), true).iter().map(|p| p.0.uri).collect::<Vec<_>>());
    }
}
