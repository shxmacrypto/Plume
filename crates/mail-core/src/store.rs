//! Ce que Plume Mail garde sur ce PC : les comptes, leurs dossiers, la liste des messages et les messages déjà
//! lus (pour les relire hors connexion). Les mots de passe n'y sont pas : ils vont dans le Gestionnaire
//! d'identification de Windows, chiffrés par Windows pour cet utilisateur.

use crate::{
    imap::{Flags, Folder, Role},
    message::Summary,
    net::Security,
};
use serde_json::{Value, json};
use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Un nom court et stable (« a1 ») : celui de son dossier sur le disque.
    pub id: String,
    /// Le nom de l'expéditeur, tel que les destinataires le voient.
    pub name: String,
    pub email: String,
    /// Le nom pour s'identifier (souvent l'adresse elle-même).
    pub user: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: Security,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: Security,
}

impl Account {
    /// Les serveurs d'une adresse : ceux des grands fournisseurs sont connus, les autres se devinent à partir du
    /// domaine (et se corrigent dans la boîte de dialogue du compte).
    pub fn guess(name: &str, email: &str) -> Account {
        let email = email.trim().to_string();
        let domain = email.rsplit_once('@').map_or("", |(_, d)| d).to_lowercase();
        let known: Option<(&str, u16, &str, u16, Security)> = match domain.as_str() {
            "gmail.com" | "googlemail.com" => Some(("imap.gmail.com", 993, "smtp.gmail.com", 465, Security::Tls)),
            "outlook.com" | "hotmail.com" | "live.com" | "msn.com" | "outlook.fr" | "hotmail.fr" | "live.ca" | "live.fr" => {
                Some(("outlook.office365.com", 993, "smtp-mail.outlook.com", 587, Security::StartTls))
            }
            "yahoo.com" | "yahoo.fr" | "yahoo.ca" | "ymail.com" => Some(("imap.mail.yahoo.com", 993, "smtp.mail.yahoo.com", 465, Security::Tls)),
            "icloud.com" | "me.com" | "mac.com" => Some(("imap.mail.me.com", 993, "smtp.mail.me.com", 587, Security::StartTls)),
            "fastmail.com" | "fastmail.fm" => Some(("imap.fastmail.com", 993, "smtp.fastmail.com", 465, Security::Tls)),
            "gmx.com" | "gmx.net" | "gmx.fr" => Some(("imap.gmx.com", 993, "mail.gmx.com", 587, Security::StartTls)),
            "aol.com" => Some(("imap.aol.com", 993, "smtp.aol.com", 465, Security::Tls)),
            "zoho.com" => Some(("imap.zoho.com", 993, "smtp.zoho.com", 465, Security::Tls)),
            _ => None,
        };
        let (imap_host, imap_port, smtp_host, smtp_port, smtp_security) = match known {
            Some((i, ip, s, sp, sec)) => (i.to_string(), ip, s.to_string(), sp, sec),
            None => (format!("imap.{domain}"), 993, format!("smtp.{domain}"), 587, Security::StartTls),
        };
        Account {
            id: String::new(),
            name: name.trim().to_string(),
            user: email.clone(),
            email,
            imap_host,
            imap_port,
            imap_security: Security::Tls,
            smtp_host,
            smtp_port,
            smtp_security,
        }
    }

    /// Ce fournisseur n'accepte plus de mot de passe pour le courrier (il exige une connexion par son site, que
    /// Plume Mail ne sait pas encore faire).
    pub fn needs_web_sign_in(&self) -> bool {
        self.imap_host.eq_ignore_ascii_case("outlook.office365.com")
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "email": self.email, "user": self.user,
            "imap_host": self.imap_host, "imap_port": self.imap_port, "imap_security": self.imap_security.name(),
            "smtp_host": self.smtp_host, "smtp_port": self.smtp_port, "smtp_security": self.smtp_security.name(),
        })
    }

    fn from_json(v: &Value) -> Option<Account> {
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let port = |k: &str| v.get(k).and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok());
        Some(Account {
            id: text("id").filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))?,
            name: text("name").unwrap_or_default(),
            email: text("email")?,
            user: text("user")?,
            imap_host: text("imap_host")?,
            imap_port: port("imap_port")?,
            imap_security: Security::parse(&text("imap_security").unwrap_or_default()),
            smtp_host: text("smtp_host")?,
            smtp_port: port("smtp_port")?,
            smtp_security: Security::parse(&text("smtp_security").unwrap_or_default()),
        })
    }
}

/// La liste des messages d'un dossier telle qu'on l'a vue la dernière fois.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub uid_validity: u32,
    /// Du plus récent au plus ancien.
    pub messages: Vec<Summary>,
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Inbox => "inbox",
        Role::Drafts => "drafts",
        Role::Sent => "sent",
        Role::Archive => "archive",
        Role::Junk => "junk",
        Role::Trash => "trash",
        Role::All => "all",
        Role::Other => "other",
    }
}

fn role_from(name: &str) -> Role {
    [Role::Inbox, Role::Drafts, Role::Sent, Role::Archive, Role::Junk, Role::Trash, Role::All]
        .into_iter()
        .find(|r| role_name(*r) == name)
        .unwrap_or(Role::Other)
}

fn summary_json(s: &Summary) -> Value {
    let f = s.flags;
    json!({
        "uid": s.uid, "size": s.size, "date": s.date, "subject": s.subject, "from_name": s.from_name, "from": s.from, "to": s.to,
        "id": s.message_id, "refs": s.references, "att": s.attachments,
        "flags": [f.seen, f.flagged, f.answered, f.draft, f.deleted],
    })
}

fn summary_from(v: &Value) -> Option<Summary> {
    let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let flag = |i: usize| v.get("flags").and_then(|f| f.get(i)).and_then(Value::as_bool).unwrap_or(false);
    Some(Summary {
        uid: v.get("uid")?.as_u64()? as u32,
        size: v.get("size").and_then(Value::as_u64).unwrap_or(0) as u32,
        flags: Flags { seen: flag(0), flagged: flag(1), answered: flag(2), draft: flag(3), deleted: flag(4) },
        date: v.get("date").and_then(Value::as_i64).unwrap_or(0),
        subject: text("subject"),
        from_name: text("from_name"),
        from: text("from"),
        to: text("to"),
        message_id: text("id"),
        references: v.get("refs").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default(),
        attachments: v.get("att").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Écrit un fichier par un fichier temporaire : jamais de fichier à moitié écrit.
fn write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

#[derive(Clone, Debug)]
pub struct Store {
    pub dir: PathBuf,
    /// Dossier d'essai (PLUME_MAIL_PROFILE) : les mots de passe y restent, dans un fichier, pour ne jamais toucher
    /// au Gestionnaire d'identification de l'utilisateur.
    test: bool,
}

impl Store {
    /// Le dossier de Plume Mail : `%LOCALAPPDATA%\Plume\Mail`, ou celui que désigne PLUME_MAIL_PROFILE (essais).
    pub fn open() -> Store {
        match std::env::var_os("PLUME_MAIL_PROFILE") {
            Some(dir) => Store { dir: PathBuf::from(dir), test: true },
            None => {
                let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
                Store { dir: base.join("Plume").join("Mail"), test: false }
            }
        }
    }

    /// Un dossier précis, pour les essais.
    pub fn at(dir: PathBuf) -> Store {
        Store { dir, test: true }
    }

    pub fn accounts(&self) -> Vec<Account> {
        read_json(&self.dir.join("accounts.json")).and_then(|v| v.as_array().map(|a| a.iter().filter_map(Account::from_json).collect())).unwrap_or_default()
    }

    pub fn save_accounts(&self, accounts: &[Account]) -> std::io::Result<()> {
        let list: Vec<Value> = accounts.iter().map(Account::to_json).collect();
        write(&self.dir.join("accounts.json"), serde_json::to_string_pretty(&list).unwrap_or_default().as_bytes())
    }

    /// Un identifiant libre pour un nouveau compte.
    pub fn new_id(&self, accounts: &[Account]) -> String {
        (1..).map(|n| format!("a{n}")).find(|id| !accounts.iter().any(|a| a.id == *id) && !self.dir.join(id).exists()).unwrap_or_default()
    }

    /// Retire un compte de ce PC : ses messages gardés et son mot de passe. Rien ne change sur le serveur.
    pub fn remove_account(&self, id: &str) {
        self.forget_password(id);
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()) {
            let _ = std::fs::remove_dir_all(self.dir.join(id));
        }
    }

    // ───────────────────────── mots de passe ─────────────────────────

    fn secrets_file(&self) -> PathBuf {
        self.dir.join("test-secrets.json")
    }

    pub fn password(&self, id: &str) -> Option<String> {
        if self.test {
            return read_json(&self.secrets_file())?.get(id)?.as_str().map(str::to_string);
        }
        credentials::read(&target(id))
    }

    pub fn set_password(&self, id: &str, user: &str, password: &str) -> bool {
        if self.test {
            let mut all = read_json(&self.secrets_file()).filter(Value::is_object).unwrap_or_else(|| json!({}));
            all[id] = json!(password);
            return write(&self.secrets_file(), all.to_string().as_bytes()).is_ok();
        }
        credentials::write(&target(id), user, password)
    }

    pub fn forget_password(&self, id: &str) {
        if self.test {
            if let Some(mut all) = read_json(&self.secrets_file()).filter(Value::is_object) {
                all.as_object_mut().map(|o| o.remove(id));
                let _ = write(&self.secrets_file(), all.to_string().as_bytes());
            }
            return;
        }
        credentials::delete(&target(id));
    }

    // ───────────────────────── la boîte gardée ─────────────────────────

    /// Le dossier du disque pour un dossier du compte : son nom peut contenir n'importe quoi, on n'en garde qu'une empreinte.
    fn folder_dir(&self, account: &str, folder: &str) -> PathBuf {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        folder.hash(&mut h);
        self.dir.join(account).join(format!("{:016x}", h.finish()))
    }

    pub fn folders(&self, account: &str) -> Vec<Folder> {
        let Some(v) = read_json(&self.dir.join(account).join("folders.json")) else { return vec![] };
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|f| {
                        let text = |k: &str| f.get(k).and_then(Value::as_str).map(str::to_string);
                        Some(Folder {
                            raw: text("raw")?,
                            name: text("name")?,
                            path: text("path").unwrap_or_default(),
                            role: role_from(&text("role").unwrap_or_default()),
                            no_select: f.get("no_select").and_then(Value::as_bool).unwrap_or(false),
                            depth: f.get("depth").and_then(Value::as_u64).unwrap_or(0) as usize,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn save_folders(&self, account: &str, folders: &[Folder]) {
        let list: Vec<Value> = folders
            .iter()
            .map(|f| json!({ "raw": f.raw, "name": f.name, "path": f.path, "role": role_name(f.role), "no_select": f.no_select, "depth": f.depth }))
            .collect();
        let _ = write(&self.dir.join(account).join("folders.json"), Value::Array(list).to_string().as_bytes());
    }

    pub fn listing(&self, account: &str, folder: &str) -> Listing {
        let Some(v) = read_json(&self.folder_dir(account, folder).join("index.json")) else { return Listing::default() };
        Listing {
            uid_validity: v.get("uid_validity").and_then(Value::as_u64).unwrap_or(0) as u32,
            messages: v.get("messages").and_then(Value::as_array).map(|a| a.iter().filter_map(summary_from).collect()).unwrap_or_default(),
        }
    }

    pub fn save_listing(&self, account: &str, folder: &str, listing: &Listing) {
        let v = json!({ "uid_validity": listing.uid_validity, "messages": listing.messages.iter().map(summary_json).collect::<Vec<_>>() });
        let _ = write(&self.folder_dir(account, folder).join("index.json"), v.to_string().as_bytes());
    }

    /// Un message déjà lu, tel que le serveur l'a donné.
    pub fn body(&self, account: &str, folder: &str, uid: u32) -> Option<Vec<u8>> {
        std::fs::read(self.folder_dir(account, folder).join(format!("{uid}.eml"))).ok()
    }

    pub fn save_body(&self, account: &str, folder: &str, uid: u32, raw: &[u8]) {
        let _ = write(&self.folder_dir(account, folder).join(format!("{uid}.eml")), raw);
    }

    /// Oublie les messages gardés qui ne sont plus dans la liste (supprimés ou déplacés sur le serveur).
    pub fn prune_bodies(&self, account: &str, folder: &str, keep: &[u32]) {
        let Ok(entries) = std::fs::read_dir(self.folder_dir(account, folder)) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(uid) = name.strip_suffix(".eml").and_then(|u| u.parse::<u32>().ok())
                && !keep.contains(&uid)
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// Les numéros de messages ne valent plus (le serveur a refait le dossier) : tout ce qu'on en gardait part.
    pub fn clear_folder(&self, account: &str, folder: &str) {
        let _ = std::fs::remove_dir_all(self.folder_dir(account, folder));
    }
}

fn target(id: &str) -> String {
    format!("Plume Mail/{id}")
}

/// Le Gestionnaire d'identification de Windows : les mots de passe y sont chiffrés pour l'utilisateur de la session.
mod credentials {
    use windows::{
        Win32::{
            Foundation::FILETIME,
            Security::Credentials::{CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW},
        },
        core::{PCWSTR, PWSTR},
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn write(target: &str, user: &str, password: &str) -> bool {
        let (mut target, mut user) = (wide(target), wide(user));
        let secret = password.as_bytes();
        let credential = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            Comment: PWSTR::null(),
            LastWritten: FILETIME::default(),
            CredentialBlobSize: secret.len() as u32,
            CredentialBlob: secret.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: std::ptr::null_mut(),
            TargetAlias: PWSTR::null(),
            UserName: PWSTR(user.as_mut_ptr()),
        };
        unsafe { CredWriteW(&credential, 0).is_ok() }
    }

    pub fn read(target: &str) -> Option<String> {
        let target = wide(target);
        let mut found: *mut CREDENTIALW = std::ptr::null_mut();
        unsafe {
            CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut found).ok()?;
            let c = &*found;
            let secret = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
            let password = String::from_utf8(secret.to_vec()).ok();
            CredFree(found as *const _);
            password
        }
    }

    pub fn delete(target: &str) {
        let target = wide(target);
        unsafe {
            let _ = CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("plume-mail-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Store::at(dir)
    }

    #[test]
    fn windows_keeps_a_password() {
        // le vrai Gestionnaire d'identification, avec une entrée d'essai au nom unique, retirée aussitôt
        let target = format!("Plume Mail/test-{}", std::process::id());
        assert_eq!(credentials::read(&target), None);
        assert!(credentials::write(&target, "sam@example.com", "s3cret é 🔑"));
        assert_eq!(credentials::read(&target).as_deref(), Some("s3cret é 🔑"));
        assert!(credentials::write(&target, "sam@example.com", "changed"));
        assert_eq!(credentials::read(&target).as_deref(), Some("changed"));
        credentials::delete(&target);
        assert_eq!(credentials::read(&target), None);
    }

    #[test]
    fn servers_are_guessed_from_the_address() {
        let g = Account::guess(" Sam ", " sam@gmail.com ");
        assert_eq!((g.name.as_str(), g.email.as_str(), g.user.as_str()), ("Sam", "sam@gmail.com", "sam@gmail.com"));
        assert_eq!(
            (g.imap_host.as_str(), g.imap_port, g.imap_security, g.smtp_host.as_str(), g.smtp_port, g.smtp_security),
            ("imap.gmail.com", 993, Security::Tls, "smtp.gmail.com", 465, Security::Tls)
        );
        let own = Account::guess("", "me@Mon-Domaine.ca");
        assert_eq!(
            (own.imap_host.as_str(), own.smtp_host.as_str(), own.smtp_port, own.smtp_security),
            ("imap.mon-domaine.ca", "smtp.mon-domaine.ca", 587, Security::StartTls)
        );
        assert!(Account::guess("", "x@hotmail.com").needs_web_sign_in() && !g.needs_web_sign_in());
    }

    #[test]
    fn accounts_and_passwords() {
        let store = scratch("accounts");
        assert!(store.accounts().is_empty());
        let mut a = Account::guess("Sam", "sam@example.com");
        a.id = store.new_id(&[]);
        let mut b = Account::guess("Sam (work)", "sam@work.example");
        b.id = store.new_id(std::slice::from_ref(&a));
        assert_eq!((a.id.as_str(), b.id.as_str()), ("a1", "a2"));
        store.save_accounts(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(store.accounts(), [a.clone(), b.clone()]);
        // le fichier des comptes ne contient aucun mot de passe
        assert!(store.set_password("a1", "sam@example.com", "s3cret é"));
        let saved = std::fs::read_to_string(store.dir.join("accounts.json")).unwrap();
        assert!(!saved.contains("s3cret") && !saved.to_lowercase().contains("password"));
        assert_eq!(store.password("a1").as_deref(), Some("s3cret é"));
        assert_eq!(store.password("a2"), None);
        store.remove_account("a1");
        assert_eq!(store.password("a1"), None);
        // un fichier abîmé : pas de compte, pas de plantage
        std::fs::write(store.dir.join("accounts.json"), b"[{\"id\": \"../x\"}, 42, {\"id\": \"a9\"}]").unwrap();
        assert!(store.accounts().is_empty());
        let _ = std::fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn the_mailbox_is_kept_on_disk() {
        let store = scratch("cache");
        let folders = vec![
            Folder { raw: "INBOX".into(), name: "INBOX".into(), path: "INBOX".into(), role: Role::Inbox, no_select: false, depth: 0 },
            Folder { raw: "Work/&AMk-t&AOk-".into(), name: "Été".into(), path: "Work/Été".into(), role: Role::Other, no_select: false, depth: 1 },
        ];
        store.save_folders("a1", &folders);
        assert_eq!(store.folders("a1"), folders);
        assert!(store.folders("a2").is_empty());
        let one = Summary {
            uid: 7,
            size: 1234,
            flags: Flags { seen: true, flagged: true, ..Flags::default() },
            date: 1_794_312_000,
            subject: "Réunion \"demain\"".into(),
            from_name: "Ann".into(),
            from: "ann@example.com".into(),
            to: "sam@example.com".into(),
            message_id: "m1@example.com".into(),
            references: vec!["m0@example.com".into()],
            attachments: true,
        };
        let listing = Listing { uid_validity: 77, messages: vec![one.clone(), Summary { uid: 8, ..Summary::default() }] };
        store.save_listing("a1", "Work/&AMk-t&AOk-", &listing);
        assert_eq!(store.listing("a1", "Work/&AMk-t&AOk-"), listing);
        assert_eq!(store.listing("a1", "INBOX"), Listing::default());
        store.save_body("a1", "INBOX", 7, b"Subject: x\r\n\r\ny");
        store.save_body("a1", "INBOX", 8, b"z");
        assert_eq!(store.body("a1", "INBOX", 7).as_deref(), Some(b"Subject: x\r\n\r\ny".as_slice()));
        store.prune_bodies("a1", "INBOX", &[8]);
        assert_eq!((store.body("a1", "INBOX", 7), store.body("a1", "INBOX", 8).is_some()), (None, true));
        store.clear_folder("a1", "INBOX");
        assert_eq!(store.body("a1", "INBOX", 8), None);
        let _ = std::fs::remove_dir_all(&store.dir);
    }
}
