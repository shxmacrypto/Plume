//! Le travail d'un compte, sur son propre fil : garder la connexion au serveur, relire les dossiers, aller chercher
//! un message, envoyer. La fenêtre lui passe des commandes ; il lui donne des nouvelles. Tout ce qu'il apprend
//! est gardé sur le disque : la fois suivante, la boîte s'affiche avant même d'être connecté.

use crate::{
    imap::{self, Flags, Folder, Imap, Role},
    message::{self, Draft, Parsed, Summary},
    smtp,
    store::{Account, Listing, Store},
};
use std::{
    collections::HashMap,
    hash::{BuildHasher, Hasher},
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};

/// Nombre de messages gardés par dossier : les plus récents.
pub const LIMIT: usize = 500;
/// Nombre d'en-têtes demandés au serveur d'un coup.
const BATCH: usize = 100;

#[derive(Debug)]
pub enum Command {
    /// Relire la liste des dossiers, puis les messages de ce dossier.
    Sync {
        folder: String,
    },
    /// Lire un message (du disque s'il y est déjà) ; il devient « lu ».
    Read {
        folder: String,
        uid: u32,
    },
    /// Poser ou retirer un drapeau : « \Seen » (lu), « \Flagged » (suivi).
    Flag {
        folder: String,
        uids: Vec<u32>,
        flag: &'static str,
        on: bool,
    },
    Move {
        folder: String,
        uids: Vec<u32>,
        to: String,
    },
    /// Retirer pour de bon.
    Remove {
        folder: String,
        uids: Vec<u32>,
    },
    Send {
        draft: Box<Draft>,
    },
}

/// Ce que le compte est en train de faire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
    Idle,
    Connecting,
    Syncing,
    Reading,
    Sending,
}

#[derive(Debug)]
pub enum News {
    Folders {
        account: String,
        folders: Vec<Folder>,
    },
    /// La liste d'un dossier, du plus récent au plus ancien.
    Messages {
        account: String,
        folder: String,
        messages: Vec<Summary>,
    },
    Message {
        account: String,
        folder: String,
        uid: u32,
        message: Box<Parsed>,
    },
    Sent {
        account: String,
        result: Result<(), String>,
    },
    Working {
        account: String,
        work: Work,
    },
    /// Une commande n'a pas abouti : la raison, dans les mots du serveur ou du réseau.
    Failed {
        account: String,
        why: String,
    },
}

/// Le lien de la fenêtre vers le fil d'un compte. Le lâcher ferme la connexion.
pub struct Engine {
    tx: mpsc::Sender<Command>,
}

impl Engine {
    pub fn start(account: Account, store: Store, notify: impl Fn(News) + Send + 'static) -> Engine {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut worker = Worker { account, store, imap: None, selected: None, notify: Box::new(notify) };
            for command in rx {
                worker.handle(command);
            }
            if let Some(imap) = worker.imap.take() {
                imap.logout();
            }
        });
        Engine { tx }
    }

    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }
}

/// Pourquoi une commande n'a pas abouti.
enum Trouble {
    /// Le réseau : on se reconnecte et on réessaie une fois.
    Network(String),
    /// Le serveur a dit non, ou il manque quelque chose : réessayer n'y changerait rien.
    Refused(String),
}

impl From<imap::Error> for Trouble {
    fn from(e: imap::Error) -> Self {
        match e {
            imap::Error::Io(e) => Trouble::Network(e.to_string()),
            imap::Error::No(m) | imap::Error::Bad(m) => Trouble::Refused(m),
        }
    }
}

type Done = Result<(), Trouble>;

struct Worker {
    account: Account,
    store: Store,
    imap: Option<Imap>,
    /// Le dossier ouvert sur le serveur.
    selected: Option<String>,
    notify: Box<dyn Fn(News) + Send>,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

impl Worker {
    fn tell(&self, news: News) {
        (self.notify)(news);
    }

    fn working(&self, work: Work) {
        self.tell(News::Working { account: self.account.id.clone(), work });
    }

    fn handle(&mut self, command: Command) {
        let mut result = self.run(&command);
        // la connexion était tombée (veille, changement de réseau) : une nouvelle, et on recommence une fois
        if matches!(result, Err(Trouble::Network(_))) && self.imap.is_some() {
            self.imap = None;
            self.selected = None;
            result = self.run(&command);
        }
        if let Err(Trouble::Network(why) | Trouble::Refused(why)) = result {
            self.imap = None;
            self.selected = None;
            // un envoi manqué a sa propre nouvelle : la fenêtre garde le message ouvert
            let account = self.account.id.clone();
            self.tell(if matches!(command, Command::Send { .. }) { News::Sent { account, result: Err(why) } } else { News::Failed { account, why } });
        }
        self.working(Work::Idle);
    }

    fn password(&self) -> Result<String, Trouble> {
        self.store.password(&self.account.id).ok_or_else(|| Trouble::Refused("No password is saved for this account.".into()))
    }

    /// La connexion au serveur, ouverte et identifiée au besoin.
    fn imap(&mut self) -> Result<&mut Imap, Trouble> {
        if self.imap.is_none() {
            self.working(Work::Connecting);
            let password = self.password()?;
            let a = &self.account;
            let mut imap = Imap::connect(&a.imap_host, a.imap_port, a.imap_security)?;
            imap.login(&a.user, &password)?;
            self.imap = Some(imap);
            self.selected = None;
        }
        Ok(self.imap.as_mut().expect("just connected"))
    }

    /// Ouvre le dossier sur le serveur s'il ne l'est pas déjà.
    fn open(&mut self, folder: &str) -> Done {
        if self.selected.as_deref() != Some(folder) {
            self.imap()?.select(folder)?;
            self.selected = Some(folder.to_string());
        }
        Ok(())
    }

    /// Change la liste gardée d'un dossier et l'annonce à la fenêtre.
    fn update(&self, folder: &str, change: impl FnOnce(&mut Vec<Summary>)) {
        let id = &self.account.id;
        let mut listing = self.store.listing(id, folder);
        change(&mut listing.messages);
        self.store.save_listing(id, folder, &listing);
        self.tell(News::Messages { account: id.clone(), folder: folder.to_string(), messages: listing.messages });
    }

    fn run(&mut self, command: &Command) -> Done {
        match command {
            Command::Sync { folder } => self.sync(folder),
            Command::Read { folder, uid } => self.read(folder, *uid),
            Command::Flag { folder, uids, flag, on } => {
                self.open(folder)?;
                self.imap()?.set_flag(uids, flag, *on)?;
                self.update(folder, |list| {
                    for m in list.iter_mut().filter(|m| uids.contains(&m.uid)) {
                        match *flag {
                            "\\Seen" => m.flags.seen = *on,
                            "\\Flagged" => m.flags.flagged = *on,
                            _ => {}
                        }
                    }
                });
                Ok(())
            }
            Command::Move { folder, uids, to } => {
                self.open(folder)?;
                self.imap()?.move_to(uids, to)?;
                self.update(folder, |list| list.retain(|m| !uids.contains(&m.uid)));
                Ok(())
            }
            Command::Remove { folder, uids } => {
                self.open(folder)?;
                self.imap()?.remove(uids)?;
                self.update(folder, |list| list.retain(|m| !uids.contains(&m.uid)));
                Ok(())
            }
            Command::Send { draft } => self.send(draft),
        }
    }

    fn sync(&mut self, wanted: &str) -> Done {
        self.working(Work::Syncing);
        let id = self.account.id.clone();
        let folders = self.imap()?.folders()?;
        self.store.save_folders(&id, &folders);
        self.tell(News::Folders { account: id.clone(), folders: folders.clone() });
        // le dossier demandé n'existe plus : la boîte de réception
        let usable = |raw: &str| folders.iter().any(|f| f.raw == raw && !f.no_select);
        let folder = if usable(wanted) {
            wanted.to_string()
        } else {
            match folders.iter().find(|f| f.role == Role::Inbox) {
                Some(inbox) => inbox.raw.clone(),
                None => return Ok(()),
            }
        };
        // toujours rouvrir : c'est ce qui donne le nombre de messages du moment
        let mailbox = self.imap()?.select(&folder)?;
        self.selected = Some(folder.clone());
        let mut listing = self.store.listing(&id, &folder);
        if listing.uid_validity != mailbox.uid_validity {
            // le serveur a refait le dossier : les numéros qu'on gardait ne désignent plus les mêmes messages
            self.store.clear_folder(&id, &folder);
            listing = Listing { uid_validity: mailbox.uid_validity, messages: vec![] };
        }
        let on_server: Vec<(u32, Flags)> = if mailbox.exists == 0 { vec![] } else { self.imap()?.all_flags()? };
        let recent = &on_server[on_server.len().saturating_sub(LIMIT)..];
        let mut known: HashMap<u32, Summary> = listing.messages.drain(..).map(|m| (m.uid, m)).collect();
        let missing: Vec<u32> = recent.iter().map(|(uid, _)| *uid).filter(|uid| !known.contains_key(uid)).collect();
        // les plus récents d'abord
        for batch in missing.rchunks(BATCH) {
            for summary in self.imap()?.summaries(batch)? {
                known.insert(summary.uid, summary);
            }
        }
        let mut messages: Vec<Summary> = recent
            .iter()
            .filter_map(|(uid, flags)| {
                let mut m = known.remove(uid)?;
                m.flags = *flags;
                Some(m)
            })
            .collect();
        messages.sort_by(|a, b| b.date.cmp(&a.date).then(b.uid.cmp(&a.uid)));
        listing.messages = messages;
        self.store.save_listing(&id, &folder, &listing);
        self.store.prune_bodies(&id, &folder, &listing.messages.iter().map(|m| m.uid).collect::<Vec<_>>());
        self.tell(News::Messages { account: id, folder, messages: listing.messages });
        Ok(())
    }

    fn read(&mut self, folder: &str, uid: u32) -> Done {
        let id = self.account.id.clone();
        let raw = match self.store.body(&id, folder, uid) {
            Some(raw) => raw,
            None => {
                self.working(Work::Reading);
                self.open(folder)?;
                let raw = self.imap()?.body(uid)?;
                self.store.save_body(&id, folder, uid, &raw);
                raw
            }
        };
        let mut parsed = message::parse(&raw);
        parsed.summary.uid = uid;
        let unread = self.store.listing(&id, folder).messages.iter().any(|m| m.uid == uid && !m.flags.seen);
        parsed.summary.flags.seen = true;
        self.tell(News::Message { account: id, folder: folder.to_string(), uid, message: Box::new(parsed) });
        if unread {
            // la liste change tout de suite ; le serveur l'apprend ensuite
            self.update(folder, |list| list.iter_mut().filter(|m| m.uid == uid).for_each(|m| m.flags.seen = true));
            self.open(folder)?;
            self.imap()?.set_flag(&[uid], "\\Seen", true)?;
        }
        Ok(())
    }

    fn send(&mut self, draft: &Draft) -> Done {
        self.working(Work::Sending);
        let to: Vec<String> = [&draft.to, &draft.cc, &draft.bcc].into_iter().flat_map(|field| message::addresses(field)).collect();
        if to.is_empty() || message::addresses(&draft.to).is_empty() {
            return Err(Trouble::Refused("Check the recipients: an address is missing or badly written.".into()));
        }
        let unique = std::collections::hash_map::RandomState::new().build_hasher().finish();
        let raw = message::build(draft, now(), unique);
        let password = self.password()?;
        let a = self.account.clone();
        let server = smtp::Server { host: &a.smtp_host, port: a.smtp_port, security: a.smtp_security, user: &a.user, password: &password };
        smtp::send(&server, &a.email, &to, &raw).map_err(Trouble::Refused)?;
        self.tell(News::Sent { account: a.id.clone(), result: Ok(()) });
        // La copie dans « Envoyés ». Gmail la range lui-même : l'y déposer la mettrait en double. Si elle échoue,
        // le message est parti quand même : on ne le signale pas comme un envoi manqué.
        if !a.smtp_host.to_lowercase().contains("gmail")
            && let Some(sent) = self.store.folders(&a.id).into_iter().find(|f| f.role == Role::Sent)
            && let Ok(imap) = self.imap()
        {
            let _ = imap.append(&sent.raw, &raw);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{imap::tests::stage, net::Security};
    use std::time::Duration;

    #[test]
    fn syncs_and_reads_through_a_scripted_server() {
        let header = "Subject: Bonjour\r\nFrom: Ann <ann@example.com>\r\nTo: sam@example.com\r\nDate: Tue, 10 Nov 2026 12:00:00 +0000\r\nMessage-ID: <m1@example.com>\r\n\r\n";
        let old = "Subject: Plus ancien\r\nFrom: bob@example.com\r\nDate: Mon, 09 Nov 2026 12:00:00 +0000\r\n\r\n";
        let body = "Subject: Bonjour\r\nFrom: Ann <ann@example.com>\r\n\r\nLe texte.\r\n";
        let port = stage(
            "* OK ready\r\n",
            vec![
                ("CAPABILITY", "* CAPABILITY IMAP4rev1 MOVE\r\n{tag} OK\r\n".into()),
                ("LOGIN \"sam\" \"pw\"", "{tag} OK\r\n".into()),
                ("CAPABILITY", "* CAPABILITY IMAP4rev1 MOVE\r\n{tag} OK\r\n".into()),
                ("LIST", "* LIST () \"/\" INBOX\r\n* LIST (\\Trash) \"/\" Trash\r\n{tag} OK\r\n".into()),
                ("SELECT \"INBOX\"", "* 2 EXISTS\r\n* OK [UIDVALIDITY 5] ok\r\n{tag} OK\r\n".into()),
                ("UID FETCH 1:* (UID FLAGS)", "* 1 FETCH (UID 3 FLAGS (\\Seen))\r\n* 2 FETCH (UID 4 FLAGS ())\r\n{tag} OK\r\n".into()),
                (
                    "UID FETCH 3:4 (UID FLAGS RFC822.SIZE",
                    format!(
                        "* 1 FETCH (UID 3 RFC822.SIZE 50 BODY[HEADER.FIELDS (X)] {{{}}}\r\n{old})\r\n* 2 FETCH (UID 4 RFC822.SIZE 99 BODY[HEADER.FIELDS (X)] {{{}}}\r\n{header})\r\n{{tag}} OK\r\n",
                        old.len(),
                        header.len()
                    ),
                ),
                ("UID FETCH 4 (BODY.PEEK[])", format!("* 2 FETCH (UID 4 BODY[] {{{}}}\r\n{body})\r\n{{tag}} OK\r\n", body.len())),
                ("UID STORE 4 +FLAGS.SILENT (\\Seen)", "{tag} OK\r\n".into()),
                ("UID MOVE 3 \"Trash\"", "{tag} OK\r\n".into()),
                ("LOGOUT", "* BYE\r\n{tag} OK\r\n".into()),
            ],
        );
        let dir = std::env::temp_dir().join(format!("plume-mail-engine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::at(dir.clone());
        let mut account = Account::guess("Sam", "sam@example.com");
        (account.id, account.user, account.imap_host, account.imap_port, account.imap_security) =
            ("a1".into(), "sam".into(), "127.0.0.1".into(), port, Security::Plain);
        store.set_password("a1", "sam", "pw");
        let (tx, rx) = mpsc::channel();
        let engine = Engine::start(account, store.clone(), move |news| {
            let _ = tx.send(news);
        });
        // attend la prochaine nouvelle qui n'est pas « en train de... »
        let next = || loop {
            match rx.recv_timeout(Duration::from_secs(10)).expect("news") {
                News::Working { .. } => continue,
                other => return other,
            }
        };
        engine.send(Command::Sync { folder: "INBOX".into() });
        match next() {
            News::Folders { folders, .. } => {
                assert_eq!(folders.iter().map(|f| (f.name.as_str(), f.role)).collect::<Vec<_>>(), [("INBOX", Role::Inbox), ("Trash", Role::Trash)])
            }
            other => panic!("{other:?}"),
        }
        match next() {
            // du plus récent au plus ancien ; les drapeaux sont ceux du serveur
            News::Messages { folder, messages, .. } => {
                assert_eq!(folder, "INBOX");
                assert_eq!(
                    messages.iter().map(|m| (m.uid, m.subject.as_str(), m.flags.seen)).collect::<Vec<_>>(),
                    [(4, "Bonjour", false), (3, "Plus ancien", true)]
                );
            }
            other => panic!("{other:?}"),
        }
        engine.send(Command::Read { folder: "INBOX".into(), uid: 4 });
        match next() {
            News::Message { uid, message, .. } => assert_eq!((uid, message.text.as_str(), message.summary.from_name.as_str()), (4, "Le texte.", "Ann")),
            other => panic!("{other:?}"),
        }
        match next() {
            News::Messages { messages, .. } => assert!(messages[0].flags.seen),
            other => panic!("{other:?}"),
        }
        engine.send(Command::Move { folder: "INBOX".into(), uids: vec![3], to: "Trash".into() });
        match next() {
            News::Messages { messages, .. } => assert_eq!(messages.iter().map(|m| m.uid).collect::<Vec<_>>(), [4]),
            other => panic!("{other:?}"),
        }
        // tout cela est sur le disque : la boîte se rouvre sans serveur
        assert_eq!(store.listing("a1", "INBOX").messages.len(), 1);
        assert_eq!(store.body("a1", "INBOX", 4).as_deref(), Some(body.as_bytes()));
        assert_eq!(store.folders("a1").len(), 2);
        drop(engine);
        std::thread::sleep(Duration::from_millis(150));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
