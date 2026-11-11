//! Les textes de l'interface, en anglais et en français.

use windows::Win32::Globalization::GetUserDefaultUILanguage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

impl Lang {
    /// « fr », « en » ; autre chose (ou « auto ») : la langue de Windows.
    pub fn parse(s: &str) -> Lang {
        match s.trim().to_ascii_lowercase().as_str() {
            "fr" => Lang::Fr,
            "en" => Lang::En,
            _ => Lang::system(),
        }
    }
    pub fn system() -> Lang {
        // identifiant de langue principal : 0x0C = français
        if unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x0c { Lang::Fr } else { Lang::En }
    }
    #[cfg(feature = "test-hook")]
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }
}

pub struct Strings {
    // lu par la prise d’essai
    #[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
    pub lang: Lang,
    pub this_pc: &'static str,
    pub new_tab: &'static str,
    /// Bouton de la barre de titre quand une version plus récente de la suite existe ({v} : la version).
    pub update_to: &'static str,
    pub preview_pane: &'static str,
    pub preview_none: &'static str,
    pub preview_loading: &'static str,
    pub undo: &'static str,
    pub view_thumbs: &'static str,
    pub undone: &'static str,
    pub nothing_to_undo: &'static str,
    pub undo_failed: &'static str,
    pub search: &'static str,
    pub search_in: &'static str,
    pub col_name: &'static str,
    pub col_date: &'static str,
    pub col_type: &'static str,
    pub col_size: &'static str,
    pub col_total: &'static str,
    pub col_free: &'static str,
    pub col_folder: &'static str,
    pub empty: &'static str,
    pub no_match: &'static str,
    pub loading: &'static str,
    pub denied: &'static str,
    pub cant_open: &'static str,
    pub not_found: &'static str,
    pub gone: &'static str,
    pub item: (&'static str, &'static str),
    pub selected: (&'static str, &'static str),
    /// « octet », puis octets, Ko, Mo, Go, To.
    pub byte: &'static str,
    pub units: [&'static str; 5],
    pub decimal: char,
    pub group: &'static str,
    // menus
    pub open: &'static str,
    pub open_new_tab: &'static str,
    pub paste: &'static str,
    pub copy_path: &'static str,
    pub properties: &'static str,
    pub new_folder: &'static str,
    pub refresh: &'static str,
    pub sort_by: &'static str,
    pub ascending: &'static str,
    pub descending: &'static str,
    pub show_hidden: &'static str,
    pub show_ext: &'static str,
    pub open_terminal: &'static str,
    pub pin: &'static str,
    pub unpin: &'static str,
    pub close_tab: &'static str,
    pub close_others: &'static str,
    pub duplicate_tab: &'static str,
    pub select_all: &'static str,
    pub theme: &'static str,
    pub theme_system: &'static str,
    pub theme_dark: &'static str,
    pub theme_light: &'static str,
    pub language: &'static str,
    pub about: &'static str,
    // recherche
    pub everywhere: &'static str,
    pub results: (&'static str, &'static str),
    pub results_shown: &'static str,
    pub search_in_label: &'static str,
    pub indexing: &'static str,
    pub search_results: &'static str,
    pub open_location: &'static str,
    pub copied_path: &'static str,
    pub invalid_name: &'static str,
    pub extract_here: &'static str,
    pub extract_to: &'static str,
    pub compress: &'static str,
    pub extracting: &'static str,
    pub compressing: &'static str,
    pub compress_password: &'static str,
    pub pw_title_lock: &'static str,
    pub pw_title_unlock: &'static str,
    pub pw_placeholder: &'static str,
    pub pw_show: &'static str,
    pub pw_hide: &'static str,
    pub pw_wrong: &'static str,
    pub pw_empty: &'static str,
    pub pw_note_zip: &'static str,
    pub pw_note_rar: &'static str,
    pub pw_note_7z: &'static str,
    pub pw_note_no_rar: &'static str,
    pub need_winrar: &'static str,
    pub btn_cancel: &'static str,
    pub btn_create: &'static str,
    pub btn_extract: &'static str,
    pub open_in_fex: &'static str,
    pub ctx_entry: &'static str,
    pub default_explorer: &'static str,
    pub folder_sizes: &'static str,
    pub rename: &'static str,
    pub delete: &'static str,
    pub btn_ok: &'static str,
    /// Serveurs SFTP (version d'essai).
    pub sftp_connect: &'static str,
    pub sftp_title: &'static str,
    pub sftp_hint: &'static str,
    pub sftp_placeholder: &'static str,
    pub btn_connect: &'static str,
    pub sftp_bad_address: &'static str,
    pub sftp_no_client: &'static str,
    pub sftp_download: &'static str,
    pub sftp_download_to: &'static str,
    pub sftp_upload: &'static str,
    pub sftp_downloading: &'static str,
    pub sftp_uploading: &'static str,
    pub sftp_deleting: &'static str,
    pub sftp_downloaded: &'static str,
    pub sftp_uploaded: &'static str,
    pub sftp_nothing_to_paste: &'static str,
    pub sftp_delete_title: &'static str,
    pub sftp_delete_text: &'static str,
    pub sftp_disconnect: &'static str,
    pub sftp_forget: &'static str,
    /// Bandeau de recherche : types d'éléments (ceux de fichiers dans l'ordre de `fex_core::query::KINDS`).
    pub filter_all: &'static str,
    pub kind_folder: &'static str,
    pub kinds: [&'static str; 6],
    pub match_case: &'static str,
    pub match_word: &'static str,
    pub match_path: &'static str,
    pub match_regex: &'static str,
    pub match_content: &'static str,
    pub export_results: &'static str,
    pub export_title: &'static str,
    pub csv_files: &'static str,
    pub exported: &'static str,
    pub export_failed: &'static str,
    pub default_on: &'static str,
    pub default_off: &'static str,
    pub registry_failed: &'static str,
    pub copy_address: &'static str,
    pub edit_address: &'static str,
}

pub static EN: Strings = Strings {
    lang: Lang::En,
    this_pc: "This PC",
    new_tab: "New tab",
    update_to: "Update to {v}",
    preview_pane: "Preview pane",
    preview_none: "Select a file to see it here.",
    preview_loading: "Loading…",
    undo: "Undo",
    view_thumbs: "Thumbnails",
    undone: "Undone.",
    nothing_to_undo: "Nothing to undo.",
    undo_failed: "That could not be undone (it changed in the meantime).",
    search: "Search",
    search_in: "Search in {}",
    col_name: "Name",
    col_date: "Date modified",
    col_type: "Type",
    col_size: "Size",
    col_total: "Total size",
    col_free: "Free space",
    col_folder: "Folder",
    empty: "This folder is empty.",
    no_match: "No items match your search.",
    loading: "Loading…",
    denied: "You don't have permission to open this folder.",
    cant_open: "This folder can't be opened.",
    not_found: "Windows can't find “{}”. Check the spelling and try again.",
    gone: "This folder no longer exists.",
    item: ("item", "items"),
    selected: ("item selected", "items selected"),
    byte: "byte",
    units: ["bytes", "KB", "MB", "GB", "TB"],
    decimal: '.',
    group: ",",
    open: "Open",
    open_new_tab: "Open in new tab",
    paste: "Paste",
    copy_path: "Copy as path",
    properties: "Properties",
    new_folder: "New folder",
    refresh: "Refresh",
    sort_by: "Sort by",
    ascending: "Ascending",
    descending: "Descending",
    show_hidden: "Show hidden items",
    show_ext: "Show file name extensions",
    open_terminal: "Open in Terminal",
    pin: "Pin to sidebar",
    unpin: "Unpin from sidebar",
    close_tab: "Close tab",
    close_others: "Close other tabs",
    duplicate_tab: "Duplicate tab",
    select_all: "Select all",
    theme: "Theme",
    theme_system: "Same as Windows",
    theme_dark: "Dark",
    theme_light: "Light",
    language: "Language",
    about: "About Plume FEx",
    everywhere: "Everywhere",
    results: ("result", "results"),
    results_shown: "First {} of {} results",
    search_in_label: "Search in:",
    indexing: "Preparing instant search…",
    search_results: "Search results",
    open_location: "Open file location",
    copied_path: "Path copied",
    invalid_name: "A file name can't contain any of these characters:  {}",
    extract_here: "Extract here",
    extract_to: "Extract to “{}\\”",
    compress: "Compress to ZIP file",
    extracting: "Extracting {}…",
    compressing: "Compressing to {}…",
    compress_password: "Compress with a password…",
    pw_title_lock: "Protect with a password",
    pw_title_unlock: "Password required",
    pw_placeholder: "Password",
    pw_show: "Show",
    pw_hide: "Hide",
    pw_wrong: "Wrong password. Try again.",
    pw_empty: "Enter a password.",
    pw_note_zip: "AES-256. Opens in WinRAR, 7-Zip and Windows 11.",
    pw_note_rar: "File names are hidden too. Made with your WinRAR.",
    pw_note_7z: "AES-256, file names hidden too. Opens in 7-Zip and WinRAR.",
    pw_note_no_rar: "AES-256. RAR needs WinRAR, which is not installed.",
    need_winrar: "This password-protected archive can only be opened with WinRAR installed.",
    btn_cancel: "Cancel",
    btn_create: "Create",
    btn_extract: "Extract",
    open_in_fex: "Open in Plume FEx",
    ctx_entry: "Add “Open in Plume FEx” to the right-click menu of folders",
    default_explorer: "Open folders and Win+E with Plume FEx",
    folder_sizes: "Show folder sizes",
    rename: "Rename",
    delete: "Delete",
    btn_ok: "OK",
    sftp_connect: "Connect to a server (SFTP, beta)…",
    sftp_title: "Connect to a server",
    sftp_hint: "SFTP (beta). Write user@host, or user@host:port.",
    sftp_placeholder: "user@host",
    btn_connect: "Connect",
    sftp_bad_address: "Write the address as user@host or user@host:port.",
    sftp_no_client: "Windows' OpenSSH client is missing. Add it in Settings > System > Optional features.",
    sftp_download: "Download",
    sftp_download_to: "Download to…",
    sftp_upload: "Upload files…",
    sftp_downloading: "Downloading {}…",
    sftp_uploading: "Uploading {}…",
    sftp_deleting: "Deleting…",
    sftp_downloaded: "Downloaded to {}",
    sftp_uploaded: "Uploaded: {}",
    sftp_nothing_to_paste: "Copy files first, then paste them here to upload them.",
    sftp_delete_title: "Delete from the server?",
    sftp_delete_text: "This cannot be undone: a server has no recycle bin.",
    sftp_disconnect: "Disconnect",
    sftp_forget: "Remove from the list",
    filter_all: "All types",
    kind_folder: "Folder",
    kinds: ["Audio", "Compressed", "Document", "Executable", "Picture", "Video"],
    match_case: "Match case",
    match_word: "Match whole word",
    match_path: "Match path",
    match_regex: "Regular expression",
    match_content: "Search inside files (slower)",
    export_results: "Export results…",
    export_title: "Export results",
    csv_files: "CSV file",
    exported: "Exported: {}",
    export_failed: "The results could not be saved.",
    default_on: "Folders now open in Plume FEx. Turn this off before removing Plume FEx.",
    default_off: "Folders open in Windows Explorer again.",
    registry_failed: "Windows did not accept the change.",
    copy_address: "Copy address",
    edit_address: "Edit address",
};

pub static FR: Strings = Strings {
    lang: Lang::Fr,
    this_pc: "Ce PC",
    new_tab: "Nouvel onglet",
    update_to: "Mettre à jour ({v})",
    preview_pane: "Volet d'aperçu",
    preview_none: "Sélectionnez un fichier pour le voir ici.",
    preview_loading: "Chargement…",
    undo: "Annuler",
    view_thumbs: "Vignettes",
    undone: "Annulé.",
    nothing_to_undo: "Rien à annuler.",
    undo_failed: "Impossible d'annuler (cela a changé entre-temps).",
    search: "Rechercher",
    search_in: "Rechercher dans {}",
    col_name: "Nom",
    col_date: "Modifié le",
    col_type: "Type",
    col_size: "Taille",
    col_total: "Taille totale",
    col_free: "Espace libre",
    col_folder: "Dossier",
    empty: "Ce dossier est vide.",
    no_match: "Aucun élément ne correspond à votre recherche.",
    loading: "Chargement…",
    denied: "Vous n'avez pas l'autorisation d'ouvrir ce dossier.",
    cant_open: "Impossible d'ouvrir ce dossier.",
    not_found: "Windows ne trouve pas « {} ». Vérifiez l'orthographe et réessayez.",
    gone: "Ce dossier n'existe plus.",
    item: ("élément", "éléments"),
    selected: ("élément sélectionné", "éléments sélectionnés"),
    byte: "octet",
    units: ["octets", "Ko", "Mo", "Go", "To"],
    decimal: ',',
    group: "\u{202f}",
    open: "Ouvrir",
    open_new_tab: "Ouvrir dans un nouvel onglet",
    paste: "Coller",
    copy_path: "Copier le chemin d'accès",
    properties: "Propriétés",
    new_folder: "Nouveau dossier",
    refresh: "Actualiser",
    sort_by: "Trier par",
    ascending: "Croissant",
    descending: "Décroissant",
    show_hidden: "Afficher les éléments masqués",
    show_ext: "Afficher les extensions de noms de fichiers",
    open_terminal: "Ouvrir dans le Terminal",
    pin: "Épingler à la barre latérale",
    unpin: "Désépingler de la barre latérale",
    close_tab: "Fermer l'onglet",
    close_others: "Fermer les autres onglets",
    duplicate_tab: "Dupliquer l'onglet",
    select_all: "Tout sélectionner",
    theme: "Thème",
    theme_system: "Comme Windows",
    theme_dark: "Sombre",
    theme_light: "Clair",
    language: "Langue",
    about: "À propos de Plume FEx",
    everywhere: "Partout",
    results: ("résultat", "résultats"),
    results_shown: "{} premiers résultats sur {}",
    search_in_label: "Rechercher dans :",
    indexing: "Préparation de la recherche instantanée…",
    search_results: "Résultats de la recherche",
    open_location: "Ouvrir l'emplacement du fichier",
    copied_path: "Chemin copié",
    invalid_name: "Un nom de fichier ne peut contenir aucun de ces caractères :  {}",
    extract_here: "Extraire ici",
    extract_to: "Extraire vers « {}\\ »",
    compress: "Compresser en fichier ZIP",
    extracting: "Extraction de {}…",
    compressing: "Compression vers {}…",
    compress_password: "Compresser avec un mot de passe…",
    pw_title_lock: "Protéger par un mot de passe",
    pw_title_unlock: "Mot de passe requis",
    pw_placeholder: "Mot de passe",
    pw_show: "Afficher",
    pw_hide: "Masquer",
    pw_wrong: "Mot de passe incorrect. Réessayez.",
    pw_empty: "Saisissez un mot de passe.",
    pw_note_zip: "AES-256. S'ouvre dans WinRAR, 7-Zip et Windows 11.",
    pw_note_rar: "Les noms de fichiers sont cachés aussi. Créé avec votre WinRAR.",
    pw_note_7z: "AES-256, noms de fichiers cachés aussi. S'ouvre dans 7-Zip et WinRAR.",
    pw_note_no_rar: "AES-256. Le format RAR demande WinRAR, qui n'est pas installé.",
    need_winrar: "Cette archive protégée ne s'ouvre qu'avec WinRAR installé.",
    btn_cancel: "Annuler",
    btn_create: "Créer",
    btn_extract: "Extraire",
    open_in_fex: "Ouvrir dans Plume FEx",
    ctx_entry: "Ajouter « Ouvrir dans Plume FEx » au clic droit sur les dossiers",
    default_explorer: "Ouvrir les dossiers et Win+E avec Plume FEx",
    folder_sizes: "Afficher la taille des dossiers",
    rename: "Renommer",
    delete: "Supprimer",
    btn_ok: "OK",
    sftp_connect: "Se connecter à un serveur (SFTP, essai)…",
    sftp_title: "Se connecter à un serveur",
    sftp_hint: "SFTP (version d'essai). Écrivez utilisateur@hôte, ou utilisateur@hôte:port.",
    sftp_placeholder: "utilisateur@hôte",
    btn_connect: "Se connecter",
    sftp_bad_address: "Écrivez l'adresse sous la forme utilisateur@hôte ou utilisateur@hôte:port.",
    sftp_no_client: "Le client OpenSSH de Windows manque. Ajoutez-le dans Paramètres > Système > Fonctionnalités facultatives.",
    sftp_download: "Télécharger",
    sftp_download_to: "Télécharger dans…",
    sftp_upload: "Envoyer des fichiers…",
    sftp_downloading: "Téléchargement de {}…",
    sftp_uploading: "Envoi de {}…",
    sftp_deleting: "Suppression…",
    sftp_downloaded: "Téléchargé dans {}",
    sftp_uploaded: "Envoyé : {}",
    sftp_nothing_to_paste: "Copiez d'abord des fichiers, puis collez-les ici pour les envoyer.",
    sftp_delete_title: "Supprimer du serveur ?",
    sftp_delete_text: "Suppression définitive : un serveur n'a pas de corbeille.",
    sftp_disconnect: "Se déconnecter",
    sftp_forget: "Retirer de la liste",
    filter_all: "Tous les types",
    kind_folder: "Dossier",
    kinds: ["Audio", "Compressé", "Document", "Exécutable", "Image", "Vidéo"],
    match_case: "Respecter la casse",
    match_word: "Mot entier",
    match_path: "Chercher dans le chemin",
    match_regex: "Expression régulière",
    match_content: "Chercher dans le contenu (plus lent)",
    export_results: "Exporter les résultats…",
    export_title: "Exporter les résultats",
    csv_files: "Fichier CSV",
    exported: "Exporté : {}",
    export_failed: "Les résultats n'ont pas pu être enregistrés.",
    default_on: "Les dossiers s'ouvrent maintenant dans Plume FEx. Désactivez ceci avant de supprimer Plume FEx.",
    default_off: "Les dossiers s'ouvrent de nouveau dans l'Explorateur Windows.",
    registry_failed: "Windows n'a pas accepté le changement.",
    copy_address: "Copier l'adresse",
    edit_address: "Modifier l'adresse",
};

pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        Lang::Fr => &FR,
    }
}

impl Strings {
    /// Nombre entier avec séparateur de milliers : 12,345 ou 12 345.
    pub fn num(&self, n: u64) -> String {
        let d = n.to_string();
        let mut out = String::with_capacity(d.len() + d.len() / 3 * self.group.len());
        for (i, ch) in d.chars().enumerate() {
            if i > 0 && (d.len() - i).is_multiple_of(3) {
                out.push_str(self.group);
            }
            out.push(ch);
        }
        out
    }

    /// « 1 item », « 12,345 items ».
    pub fn count(&self, n: u64, forms: (&str, &str)) -> String {
        format!("{} {}", self.num(n), if n == 1 { forms.0 } else { forms.1 })
    }

    /// Remplace les « {} » d'un modèle, dans l'ordre.
    pub fn fill(&self, template: &str, args: &[&str]) -> String {
        let mut out = String::with_capacity(template.len() + args.iter().map(|a| a.len()).sum::<usize>());
        let mut parts = template.split("{}");
        if let Some(first) = parts.next() {
            out.push_str(first);
        }
        for (i, part) in parts.enumerate() {
            out.push_str(args.get(i).copied().unwrap_or(""));
            out.push_str(part);
        }
        out
    }

    /// Taille lisible : « 512 bytes », « 12 KB », « 4.4 MB », « 1.25 GB ».
    pub fn size(&self, bytes: u64) -> String {
        if bytes < 1024 {
            return format!("{} {}", bytes, if bytes == 1 { self.byte } else { self.units[0] });
        }
        let mut v = bytes as f64 / 1024.0;
        let mut unit = 1;
        while v >= 1024.0 && unit < 4 {
            v /= 1024.0;
            unit += 1;
        }
        let digits = match unit {
            1 => 0,
            _ if v < 10.0 => 2,
            _ if v < 100.0 => 1,
            _ => 0,
        };
        let text = if digits == 0 {
            // les Ko s'arrondissent vers le haut, comme dans l'Explorateur : un fichier de 1 octet occupe « 1 Ko »
            self.num(if unit == 1 { v.ceil() as u64 } else { v.round() as u64 })
        } else {
            format!("{v:.digits$}").replace('.', &self.decimal.to_string())
        };
        format!("{} {}", text, self.units[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_sizes() {
        assert_eq!(EN.num(0), "0");
        assert_eq!(EN.num(999), "999");
        assert_eq!(EN.num(1000), "1,000");
        assert_eq!(EN.num(1234567), "1,234,567");
        assert_eq!(FR.num(12345), "12\u{202f}345");
        assert_eq!(EN.size(0), "0 bytes");
        assert_eq!(EN.size(1023), "1023 bytes");
        assert_eq!(EN.size(1), "1 byte");
        assert_eq!(FR.size(1), "1 octet");
        assert_eq!(EN.size(1024), "1 KB");
        assert_eq!(EN.size(1025), "2 KB");
        assert_eq!(EN.size(4_600_000), "4.39 MB");
        assert_eq!(EN.size(46_000_000), "43.9 MB");
        assert_eq!(EN.size(460_000_000), "439 MB");
        assert_eq!(FR.size(1_342_177_280), "1,25 Go");
        assert_eq!(EN.size(5 * 1024 * 1024 * 1024 * 1024), "5.00 TB");
        assert_eq!(EN.count(1, EN.item), "1 item");
        assert_eq!(EN.count(2500, EN.item), "2,500 items");
        assert_eq!(FR.fill(FR.results_shown, &["10", "200"]), "10 premiers résultats sur 200");
    }
}
