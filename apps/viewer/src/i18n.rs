//! The texts of Plume Viewer, in English and French. The language is the one chosen in Plume, otherwise Windows'.

use windows::Win32::Globalization::GetUserDefaultUILanguage;

pub struct Strings {
    pub title: &'static str,
    pub hero: &'static str,
    pub sub: &'static str,
    pub your_id: &'static str,
    pub code: &'static str,
    pub new_code: &'static str,
    pub copy: &'static str,
    pub copied: &'static str,
    pub scan: &'static str,
    pub ready: &'static str,
    pub connecting: &'static str,
    pub offline: &'static str,
    pub locked: &'static str,
    pub password: &'static str,
    pub password_sub: &'static str,
    pub password_on: &'static str,
    pub set_password: &'static str,
    pub change: &'static str,
    pub remove: &'static str,
    pub save: &'static str,
    pub cancel: &'static str,
    pub password_hint: &'static str,
    pub too_short: &'static str,
    pub password_saved: &'static str,
    pub password_removed: &'static str,
    pub autostart: &'static str,
    pub autostart_sub: &'static str,
    pub other: &'static str,
    pub other_sub: &'static str,
    pub open_viewer: &'static str,
    pub asks: &'static str,
    pub asks_sub: &'static str,
    pub allow: &'static str,
    pub refuse: &'static str,
    pub controlled: &'static str,
    pub linking: &'static str,
    pub disconnect: &'static str,
    pub ended: &'static str,
    pub tray_open: &'static str,
    pub tray_quit: &'static str,
    pub tray_tip: &'static str,
}

const EN: Strings = Strings {
    title: "Plume Viewer",
    hero: "Control this PC from your phone",
    sub: "Scan the code with the phone's camera, or type the ID and the code on {site}.",
    your_id: "Your ID",
    code: "One-time code",
    new_code: "New code",
    copy: "Copy",
    copied: "Copied",
    scan: "Scan to connect",
    ready: "Ready",
    connecting: "Connecting…",
    offline: "Offline",
    locked: "Locked for a few minutes: too many wrong codes",
    password: "Permanent password",
    password_sub: "For your own devices: connect without anyone at this PC accepting.",
    password_on: "On: your devices can connect with it while Plume Viewer runs.",
    set_password: "Set a password",
    change: "Change",
    remove: "Remove",
    save: "Save",
    cancel: "Cancel",
    password_hint: "At least 8 characters",
    too_short: "The password needs at least 8 characters.",
    password_saved: "Password saved",
    password_removed: "Password removed",
    autostart: "Start with Windows",
    autostart_sub: "In the background, so the PC can be reached with its password.",
    other: "Control another PC",
    other_sub: "Opens the viewer in your browser: type the ID and the code of the other PC.",
    open_viewer: "Open the viewer",
    asks: "{name} wants to control this PC",
    asks_sub: "It will see your screen and use your mouse and keyboard. Only allow someone you know.",
    allow: "Allow",
    refuse: "Refuse",
    controlled: "{name} is controlling this PC",
    linking: "{name} is connecting…",
    disconnect: "Disconnect",
    ended: "The session ended",
    tray_open: "Open Plume Viewer",
    tray_quit: "Quit",
    tray_tip: "Plume Viewer: ID {id}",
};

const FR: Strings = Strings {
    title: "Plume Viewer",
    hero: "Contrôlez ce PC depuis votre téléphone",
    sub: "Scannez le code avec l'appareil photo du téléphone, ou tapez l'ID et le code sur {site}.",
    your_id: "Votre ID",
    code: "Code à usage unique",
    new_code: "Nouveau code",
    copy: "Copier",
    copied: "Copié",
    scan: "Scanner pour se connecter",
    ready: "Prêt",
    connecting: "Connexion…",
    offline: "Hors ligne",
    locked: "Bloqué quelques minutes : trop de codes faux",
    password: "Mot de passe permanent",
    password_sub: "Pour vos propres appareils : se connecter sans que personne n'accepte devant ce PC.",
    password_on: "Activé : vos appareils peuvent se connecter avec lui tant que Plume Viewer tourne.",
    set_password: "Définir un mot de passe",
    change: "Changer",
    remove: "Retirer",
    save: "Enregistrer",
    cancel: "Annuler",
    password_hint: "Au moins 8 caractères",
    too_short: "Le mot de passe doit faire au moins 8 caractères.",
    password_saved: "Mot de passe enregistré",
    password_removed: "Mot de passe retiré",
    autostart: "Démarrer avec Windows",
    autostart_sub: "En arrière-plan, pour joindre le PC avec son mot de passe.",
    other: "Contrôler un autre PC",
    other_sub: "Ouvre le viewer dans votre navigateur : tapez l'ID et le code de l'autre PC.",
    open_viewer: "Ouvrir le viewer",
    asks: "{name} veut contrôler ce PC",
    asks_sub: "Cet appareil verra votre écran et utilisera votre souris et votre clavier. N'acceptez que quelqu'un que vous connaissez.",
    allow: "Autoriser",
    refuse: "Refuser",
    controlled: "{name} contrôle ce PC",
    linking: "{name} se connecte…",
    disconnect: "Déconnecter",
    ended: "La session est terminée",
    tray_open: "Ouvrir Plume Viewer",
    tray_quit: "Quitter",
    tray_tip: "Plume Viewer : ID {id}",
};

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

/// Replaces {name}, {id}, {site}.
pub fn fill(s: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = s.to_string();
    for (k, v) in pairs {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}
