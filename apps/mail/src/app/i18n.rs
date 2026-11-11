//! Les textes de l'interface, en anglais et en français.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

impl Lang {
    /// « en », « fr », ou la langue de Windows pour tout le reste (« auto »).
    pub fn parse(s: &str) -> Lang {
        match s.to_ascii_lowercase().as_str() {
            "en" => Lang::En,
            "fr" => Lang::Fr,
            _ => {
                // 0x0C : le français, dans les identifiants de langue de Windows
                let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
                if id & 0x3ff == 0x0c { Lang::Fr } else { Lang::En }
            }
        }
    }
}

pub struct Strings {
    pub lang: Lang,
    pub new_message: &'static str,
    /// Bouton de la barre de titre quand une version plus récente de la suite existe ({v} : la version).
    pub update_to: &'static str,
    pub reply: &'static str,
    pub reply_all: &'static str,
    pub forward: &'static str,
    pub delete: &'static str,
    pub mark_unread: &'static str,
    pub mark_read: &'static str,
    pub flag: &'static str,
    pub unflag: &'static str,
    pub sync: &'static str,
    pub search: &'static str,
    pub inbox: &'static str,
    pub drafts: &'static str,
    pub sent: &'static str,
    pub archive: &'static str,
    pub junk: &'static str,
    pub trash: &'static str,
    pub all_mail: &'static str,
    pub add_account: &'static str,
    pub edit_account: &'static str,
    pub remove_account: &'static str,
    pub welcome: &'static str,
    pub welcome_text: &'static str,
    pub no_messages: &'static str,
    pub no_match: &'static str,
    pub pick_message: &'static str,
    pub loading: &'static str,
    pub from_html: &'static str,
    pub to: &'static str,
    pub cc: &'static str,
    pub subject: &'static str,
    pub no_subject: &'static str,
    pub message: (&'static str, &'static str),
    pub unread: &'static str,
    pub connecting: &'static str,
    pub syncing: &'static str,
    pub reading: &'static str,
    pub sending: &'static str,
    pub sent_ok: &'static str,
    pub send: &'static str,
    pub cancel: &'static str,
    pub attach: &'static str,
    pub write_title: &'static str,
    pub body_placeholder: &'static str,
    pub recipients_bad: &'static str,
    pub account_title: &'static str,
    pub account_edit_title: &'static str,
    pub account_hint: &'static str,
    pub f_name: &'static str,
    pub f_email: &'static str,
    pub f_password: &'static str,
    pub f_imap: &'static str,
    pub f_smtp: &'static str,
    pub password_keep: &'static str,
    pub account_bad_email: &'static str,
    pub account_no_password: &'static str,
    pub account_bad_server: &'static str,
    pub account_web_sign_in: &'static str,
    pub save: &'static str,
    pub show: &'static str,
    pub hide: &'static str,
    pub remove_title: &'static str,
    pub remove_text: &'static str,
    pub remove: &'static str,
    pub delete_title: &'static str,
    pub delete_text: &'static str,
    pub theme_system: &'static str,
    pub theme_dark: &'static str,
    pub theme_light: &'static str,
    pub lang_auto: &'static str,
    pub copy_text: &'static str,
    pub copied: &'static str,
    pub links: &'static str,
    pub saved_to: &'static str,
    pub save_failed: &'static str,
    pub attach_title: &'static str,
    pub save_title: &'static str,
    pub all_files: &'static str,
    pub yesterday: &'static str,
    pub months: [&'static str; 12],
    pub wrote: &'static str,
    pub sign_in_refused: &'static str,
}

impl Strings {
    /// Remplace les « {} » d'un modèle, dans l'ordre.
    pub fn fill(&self, template: &str, args: &[&str]) -> String {
        let mut out = String::with_capacity(template.len() + args.iter().map(|a| a.len()).sum::<usize>());
        let mut parts = template.split("{}");
        if let Some(first) = parts.next() {
            out.push_str(first);
        }
        for (k, part) in parts.enumerate() {
            out.push_str(args.get(k).copied().unwrap_or(""));
            out.push_str(part);
        }
        out
    }

    pub fn count(&self, n: usize, forms: (&str, &str)) -> String {
        format!("{n} {}", if n == 1 { forms.0 } else { forms.1 })
    }
}

static EN: Strings = Strings {
    lang: Lang::En,
    new_message: "New message",
    update_to: "Update to {v}",
    reply: "Reply",
    reply_all: "Reply all",
    forward: "Forward",
    delete: "Delete",
    mark_unread: "Mark as unread",
    mark_read: "Mark as read",
    flag: "Flag",
    unflag: "Remove flag",
    sync: "Check for new mail",
    search: "Search this folder",
    inbox: "Inbox",
    drafts: "Drafts",
    sent: "Sent",
    archive: "Archive",
    junk: "Junk",
    trash: "Trash",
    all_mail: "All mail",
    add_account: "Add an account…",
    edit_account: "Account settings: {}…",
    remove_account: "Remove {}…",
    welcome: "Welcome to Plume Mail",
    welcome_text: "Add your mail account to start.",
    no_messages: "No messages in this folder.",
    no_match: "No message matches your search.",
    pick_message: "Pick a message to read it here.",
    loading: "Loading…",
    from_html: "This message was written as a web page. It is shown as plain text: nothing is downloaded or run.",
    to: "To",
    cc: "Cc",
    subject: "Subject",
    no_subject: "(no subject)",
    message: ("message", "messages"),
    unread: "unread",
    connecting: "Connecting…",
    syncing: "Checking for new mail…",
    reading: "Getting the message…",
    sending: "Sending…",
    sent_ok: "Message sent.",
    send: "Send",
    cancel: "Cancel",
    attach: "Attach files…",
    write_title: "New message",
    body_placeholder: "Write your message",
    recipients_bad: "Check the “To” line: an address is missing or badly written.",
    account_title: "Add a mail account",
    account_edit_title: "Account settings",
    account_hint: "Your password stays on this PC, encrypted by Windows.",
    f_name: "Your name",
    f_email: "Email address",
    f_password: "Password",
    f_imap: "Incoming server (IMAP)",
    f_smtp: "Outgoing server (SMTP)",
    password_keep: "Leave empty to keep the saved password",
    account_bad_email: "Write a complete email address, like name@example.com.",
    account_no_password: "Type the password of the account.",
    account_bad_server: "Write the servers as name:port, like imap.example.com:993.",
    account_web_sign_in: "Microsoft accounts need to sign in through Microsoft's page. Plume Mail cannot do that yet.",
    save: "Save",
    show: "Show",
    hide: "Hide",
    remove_title: "Remove this account?",
    remove_text: "Its messages stay on the mail server. Only this PC forgets them.",
    remove: "Remove",
    delete_title: "Delete for good?",
    delete_text: "This cannot be undone: the message leaves the server.",
    theme_system: "Theme: same as Windows",
    theme_dark: "Theme: dark",
    theme_light: "Theme: light",
    lang_auto: "Language: same as Windows",
    copy_text: "Copy the text",
    copied: "Text copied.",
    links: "Links",
    saved_to: "Saved: {}",
    save_failed: "The file could not be saved.",
    attach_title: "Attach files",
    save_title: "Save the attachment",
    all_files: "All files",
    yesterday: "Yesterday",
    months: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    wrote: "{} wrote",
    sign_in_refused: "The mail server refused the sign-in. Check the password.",
};

static FR: Strings = Strings {
    lang: Lang::Fr,
    new_message: "Nouveau message",
    update_to: "Mettre à jour ({v})",
    reply: "Répondre",
    reply_all: "Répondre à tous",
    forward: "Transférer",
    delete: "Supprimer",
    mark_unread: "Marquer comme non lu",
    mark_read: "Marquer comme lu",
    flag: "Suivre",
    unflag: "Ne plus suivre",
    sync: "Relever le courrier",
    search: "Rechercher dans ce dossier",
    inbox: "Boîte de réception",
    drafts: "Brouillons",
    sent: "Envoyés",
    archive: "Archives",
    junk: "Indésirables",
    trash: "Corbeille",
    all_mail: "Tous les messages",
    add_account: "Ajouter un compte…",
    edit_account: "Réglages du compte : {}…",
    remove_account: "Retirer {}…",
    welcome: "Bienvenue dans Plume Mail",
    welcome_text: "Ajoutez votre compte de courrier pour commencer.",
    no_messages: "Aucun message dans ce dossier.",
    no_match: "Aucun message ne correspond à votre recherche.",
    pick_message: "Choisissez un message pour le lire ici.",
    loading: "Chargement…",
    from_html: "Ce message a été écrit comme une page web. Il est affiché en texte : rien n'est téléchargé ni exécuté.",
    to: "À",
    cc: "Cc",
    subject: "Objet",
    no_subject: "(sans objet)",
    message: ("message", "messages"),
    unread: "non lus",
    connecting: "Connexion…",
    syncing: "Relève du courrier…",
    reading: "Récupération du message…",
    sending: "Envoi…",
    sent_ok: "Message envoyé.",
    send: "Envoyer",
    cancel: "Annuler",
    attach: "Joindre des fichiers…",
    write_title: "Nouveau message",
    body_placeholder: "Écrivez votre message",
    recipients_bad: "Vérifiez la ligne « À » : une adresse manque ou est mal écrite.",
    account_title: "Ajouter un compte de courrier",
    account_edit_title: "Réglages du compte",
    account_hint: "Votre mot de passe reste sur ce PC, chiffré par Windows.",
    f_name: "Votre nom",
    f_email: "Adresse de courrier",
    f_password: "Mot de passe",
    f_imap: "Serveur de réception (IMAP)",
    f_smtp: "Serveur d'envoi (SMTP)",
    password_keep: "Laissez vide pour garder le mot de passe enregistré",
    account_bad_email: "Écrivez une adresse complète, comme nom@exemple.com.",
    account_no_password: "Tapez le mot de passe du compte.",
    account_bad_server: "Écrivez les serveurs sous la forme nom:port, comme imap.exemple.com:993.",
    account_web_sign_in: "Les comptes Microsoft exigent de se connecter par la page de Microsoft. Plume Mail ne sait pas encore le faire.",
    save: "Enregistrer",
    show: "Afficher",
    hide: "Masquer",
    remove_title: "Retirer ce compte ?",
    remove_text: "Ses messages restent sur le serveur de courrier. Seul ce PC les oublie.",
    remove: "Retirer",
    delete_title: "Supprimer définitivement ?",
    delete_text: "Sans retour possible : le message quitte le serveur.",
    theme_system: "Thème : comme Windows",
    theme_dark: "Thème : sombre",
    theme_light: "Thème : clair",
    lang_auto: "Langue : comme Windows",
    copy_text: "Copier le texte",
    copied: "Texte copié.",
    links: "Liens",
    saved_to: "Enregistré : {}",
    save_failed: "Le fichier n'a pas pu être enregistré.",
    attach_title: "Joindre des fichiers",
    save_title: "Enregistrer la pièce jointe",
    all_files: "Tous les fichiers",
    yesterday: "Hier",
    months: ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."],
    wrote: "{} a écrit",
    sign_in_refused: "Le serveur de courrier a refusé la connexion. Vérifiez le mot de passe.",
};

pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        Lang::Fr => &FR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates() {
        assert_eq!(EN.fill("Remove {}…", &["sam@example.com"]), "Remove sam@example.com…");
        assert_eq!(EN.fill("{} and {}", &["a"]), "a and ");
        assert_eq!((EN.count(1, EN.message), FR.count(3, FR.message)), ("1 message".to_string(), "3 messages".to_string()));
        assert_eq!((Lang::parse("EN"), Lang::parse("fr")), (Lang::En, Lang::Fr));
        assert!(std::ptr::eq(strings(Lang::Fr), &FR));
    }
}
