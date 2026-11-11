//! Le travail d'un compte, sur son propre fil : garder la connexion au serveur, relire les dossiers, aller chercher
//! un message, envoyer. La fenêtre lui passe des commandes ; il lui donne des nouvelles. Tout ce qu'il apprend
//! est gardé sur le disque : la fois suivante, la boîte s'affiche avant même d'être connecté.

use crate::{
    imap::{self, Flags, Folder, Imap, Role},
    message::{self, Draft, Parsed, Summary},
    oauth, smtp,
    store::{Account, Listing, SignIn, Store},
};
use std::{
    collections::HashMap,
    hash::{BuildHasher, Hasher},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// Ce qui porte les nouvelles d'un compte à la fenêtre (depuis son fil et depuis son guet).
type Notify = Arc<dyn Fn(News) + Send + Sync>;

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
    /// Envoyer un message. `drop_draft` : le brouillon (dossier, numéro) qu'il remplace, retiré une fois parti.
    Send {
        draft: Box<Draft>,
        drop_draft: Option<(String, u32)>,
    },
    /// Garder un message pas fini dans le dossier « Brouillons » du serveur. `replace` : la version précédente,
    /// retirée une fois la nouvelle déposée.
    SaveDraft {
        draft: Box<Draft>,
        replace: Option<(String, u32)>,
    },
    /// Chercher dans le texte des messages d'un dossier : ceux gardés sur ce PC, puis le serveur.
    Search {
        folder: String,
        text: String,
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
    Searching,
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
    /// Le brouillon est gardé sur le serveur : (son dossier, son numéro s'il est connu), ou pourquoi il ne l'est pas.
    DraftSaved {
        account: String,
        result: Result<(String, Option<u32>), String>,
    },
    /// Les messages du dossier dont le texte contient ce qui est cherché. `extra` : ceux qui ne sont pas parmi les
    /// plus récents gardés dans la liste (le serveur les a trouvés plus loin).
    Found {
        account: String,
        folder: String,
        text: String,
        uids: Vec<u32>,
        extra: Vec<Summary>,
    },
    /// Le serveur annonce du nouveau dans ce dossier (un message arrivé, parti, lu ailleurs).
    Changed {
        account: String,
        folder: String,
    },
    /// Une commande n'a pas abouti : la raison, dans les mots du serveur ou du réseau.
    Failed {
        account: String,
        why: String,
    },
}

/// Le lien de la fenêtre vers le fil d'un compte. Le lâcher ferme ses connexions.
pub struct Engine {
    tx: mpsc::Sender<Command>,
    stop: Arc<AtomicBool>,
}

impl Engine {
    /// Met un compte au travail. `push` : guetter aussi le nouveau courrier sur une seconde connexion.
    pub fn start(account: Account, store: Store, push: bool, notify: impl Fn(News) + Send + Sync + 'static) -> Engine {
        let (tx, rx) = mpsc::channel();
        let notify: Notify = Arc::new(notify);
        let stop = Arc::new(AtomicBool::new(false));
        let keys = Arc::new(Keys { account: account.clone(), store: store.clone(), cache: Default::default() });
        // le guet : sur un serveur qui sait prévenir (IDLE), le nouveau courrier arrive sans attendre
        if push {
            let (keys, store, stop, notify) = (keys.clone(), store.clone(), stop.clone(), notify.clone());
            std::thread::spawn(move || watch(keys, store, stop, notify));
        }
        std::thread::spawn(move || {
            let mut worker = Worker { account, store, keys, imap: None, selected: None, notify };
            for command in rx {
                worker.handle(command);
            }
            if let Some(imap) = worker.imap.take() {
                imap.logout();
            }
        });
        Engine { tx, stop }
    }

    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Le guet d'un compte : une seconde connexion, ouverte sur la boîte de réception, qui attend que le serveur
/// annonce du nouveau (IDLE) et le dit aussitôt. Un serveur qui ne sait pas le faire est relevé à intervalles,
/// comme avant. Le guet s'arrête quand le compte est lâché.
fn watch(keys: Arc<Keys>, store: Store, stop: Arc<AtomicBool>, notify: Notify) {
    let account = keys.account.clone();
    let stopped = || stop.load(Ordering::Relaxed);
    // attend `secs` secondes, ou moins si l'on demande d'arrêter
    let pause = |secs: u64| {
        for _ in 0..secs * 4 {
            if stopped() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    };
    let mut backoff = 5;
    while !stopped() {
        let inbox = store.folders(&account.id).into_iter().find(|f| f.role == Role::Inbox).map_or("INBOX".to_string(), |f| f.raw);
        let mut imap = match keys.imap() {
            Ok(imap) => imap,
            Err(_) => {
                // pas de réseau, mot de passe refusé : le fil du compte le dira ; on réessaie plus tard
                pause(backoff);
                backoff = (backoff * 2).min(300);
                continue;
            }
        };
        if !imap.can("IDLE") {
            imap.logout();
            return;
        }
        if imap.select(&inbox).is_err() {
            pause(backoff);
            backoff = (backoff * 2).min(300);
            continue;
        }
        backoff = 5;
        let id = account.id.clone();
        let result = imap.idle(&stopped, &mut || notify(News::Changed { account: id.clone(), folder: inbox.clone() }));
        if result.is_ok() {
            imap.logout();
            return;
        }
        // la connexion est tombée (veille, réseau) : on se reconnecte, et ce qui a pu arriver entre-temps se relève
        notify(News::Changed { account: account.id.clone(), folder: inbox });
        pause(2);
    }
}



/// De quoi s'identifier, partagé par le fil du compte et son guet.
enum Secret {
    Password(String),
    /// Un jeton du fournisseur (connexion Microsoft).
    Bearer(String),
}

struct Keys {
    account: Account,
    store: Store,
    /// Le jeton d'accès en cours et l'instant où le renouveler.
    cache: std::sync::Mutex<Option<(String, std::time::Instant)>>,
}

impl Keys {
    fn secret(&self) -> Result<Secret, Trouble> {
        let id = &self.account.id;
        match self.account.sign_in {
            SignIn::Password => self.store.password(id).map(Secret::Password).ok_or_else(|| Trouble::Refused("No password is saved for this account.".into())),
            SignIn::Microsoft => {
                let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
                if let Some((token, until)) = cache.as_ref()
                    && std::time::Instant::now() < *until
                {
                    return Ok(Secret::Bearer(token.clone()));
                }
                let provider = oauth::microsoft().ok_or_else(|| Trouble::Refused(SIGN_IN_AGAIN.into()))?;
                let refresh = self.store.password(id).ok_or_else(|| Trouble::Refused(SIGN_IN_AGAIN.into()))?;
                let tokens = oauth::refresh(&provider, &refresh).map_err(|e| match e {
                    oauth::Failure::Network(m) => Trouble::Network(m),
                    oauth::Failure::Refused(m) => Trouble::Refused(format!("{SIGN_IN_AGAIN} ({m})")),
                })?;
                // Microsoft donne parfois un nouveau jeton de renouvellement : c'est lui qu'on garde
                if !tokens.refresh.is_empty() && tokens.refresh != refresh {
                    self.store.set_password(id, &self.account.user, &tokens.refresh);
                }
                let until = std::time::Instant::now() + tokens.expires.saturating_sub(std::time::Duration::from_secs(120));
                *cache = Some((tokens.access.clone(), until));
                Ok(Secret::Bearer(tokens.access))
            }
        }
    }

    /// Le jeton a été refusé : le prochain essai en demandera un neuf.
    fn forget(&self) {
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Une connexion au serveur de réception, identifiée.
    fn imap(&self) -> Result<Imap, Trouble> {
        let a = &self.account;
        let secret = self.secret()?;
        let mut imap = Imap::connect(&a.imap_host, a.imap_port, a.imap_security)?;
        let signed = match &secret {
            Secret::Password(p) => imap.login(&a.user, p),
            Secret::Bearer(t) => imap.login_token(&a.user, t),
        };
        if signed.is_err() && matches!(secret, Secret::Bearer(_)) {
            self.forget();
        }
        signed?;
        Ok(imap)
    }
}

/// Ce que dit un compte Microsoft dont la connexion ne vaut plus.
const SIGN_IN_AGAIN: &str = "Sign in to Microsoft again, in the account settings.";

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
    keys: Arc<Keys>,
    imap: Option<Imap>,
    /// Le dossier ouvert sur le serveur.
    selected: Option<String>,
    notify: Notify,
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
            self.tell(match command {
                Command::Send { .. } => News::Sent { account, result: Err(why) },
                Command::SaveDraft { .. } => News::DraftSaved { account, result: Err(why) },
                _ => News::Failed { account, why },
            });
        }
        self.working(Work::Idle);
    }

    /// La connexion au serveur, ouverte et identifiée au besoin.
    fn imap(&mut self) -> Result<&mut Imap, Trouble> {
        if self.imap.is_none() {
            self.working(Work::Connecting);
            self.imap = Some(self.keys.imap()?);
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
            Command::Send { draft, drop_draft } => {
                self.send(draft)?;
                // le brouillon qu'il remplace n'a plus lieu d'être ; s'il reste, ce n'est pas un envoi manqué
                if let Some((folder, uid)) = drop_draft {
                    let _ = self.drop_message(folder, *uid);
                }
                Ok(())
            }
            Command::SaveDraft { draft, replace } => self.save_draft(draft, replace.as_ref()),
            Command::Search { folder, text } => self.search(folder, text),
        }
    }

    /// Retire un message d'un dossier sur le serveur et de la liste gardée.
    fn drop_message(&mut self, folder: &str, uid: u32) -> Done {
        self.open(folder)?;
        self.imap()?.remove(&[uid])?;
        if !self.store.listing(&self.account.id, folder).messages.is_empty() {
            self.update(folder, |list| list.retain(|m| m.uid != uid));
        }
        Ok(())
    }

    fn save_draft(&mut self, draft: &Draft, replace: Option<&(String, u32)>) -> Done {
        self.working(Work::Sending);
        let id = self.account.id.clone();
        let Some(drafts) = self.store.folders(&id).into_iter().find(|f| f.role == Role::Drafts) else {
            return Err(Trouble::Refused("This account has no Drafts folder on its server.".into()));
        };
        let unique = std::collections::hash_map::RandomState::new().build_hasher().finish();
        let raw = message::build(draft, now(), unique);
        let uid = self.imap()?.append_draft(&drafts.raw, &raw)?;
        if let Some((folder, old)) = replace {
            let _ = self.drop_message(folder, *old);
        }
        self.tell(News::DraftSaved { account: id, result: Ok((drafts.raw, uid)) });
        Ok(())
    }

    /// Cherche dans le texte des messages : d'abord ceux gardés sur ce PC (la réponse vient tout de suite, même
    /// sans connexion), puis le serveur, qui connaît aussi les plus anciens.
    fn search(&mut self, folder: &str, text: &str) -> Done {
        let words: Vec<String> = message::fold(text).split_whitespace().map(str::to_string).collect();
        if words.is_empty() {
            return Ok(());
        }
        let id = self.account.id.clone();
        let listing = self.store.listing(&id, folder);
        let mut found: Vec<u32> = listing
            .messages
            .iter()
            .filter(|m| {
                self.store.body(&id, folder, m.uid).is_some_and(|raw| {
                    let body = message::fold(&message::parse(&raw).text);
                    words.iter().all(|w| body.contains(w.as_str()))
                })
            })
            .map(|m| m.uid)
            .collect();
        let tell = |worker: &Worker, uids: &[u32], extra: Vec<Summary>| {
            worker.tell(News::Found { account: id.clone(), folder: folder.to_string(), text: text.to_string(), uids: uids.to_vec(), extra });
        };
        tell(self, &found, vec![]);
        self.working(Work::Searching);
        // le serveur cherche avec les mots tels qu'ils ont été tapés (il ignore souvent la casse, pas les accents)
        let typed: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        self.open(folder)?;
        let on_server = self.imap()?.search_text(&typed)?;
        let known: std::collections::HashSet<u32> = listing.messages.iter().map(|m| m.uid).collect();
        // trouvés plus loin que les messages gardés : leurs lignes de liste (les plus récents, cinquante au plus)
        let mut farther: Vec<u32> = on_server.iter().copied().filter(|u| !known.contains(u)).collect();
        farther.sort_unstable_by(|a, b| b.cmp(a));
        farther.truncate(50);
        let mut extra = if farther.is_empty() { vec![] } else { self.imap()?.summaries(&farther)? };
        extra.sort_by(|a, b| b.date.cmp(&a.date).then(b.uid.cmp(&a.uid)));
        found.extend(on_server);
        found.sort_unstable();
        found.dedup();
        tell(self, &found, extra);
        Ok(())
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
        let (secret, bearer) = match self.keys.secret()? {
            Secret::Password(p) => (p, false),
            Secret::Bearer(t) => (t, true),
        };
        let a = self.account.clone();
        let server = smtp::Server { host: &a.smtp_host, port: a.smtp_port, security: a.smtp_security, user: &a.user, password: &secret, bearer };
        smtp::send(&server, &a.email, &to, &raw).map_err(|why| {
            if bearer {
                self.keys.forget();
            }
            Trouble::Refused(why)
        })?;
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
        let engine = Engine::start(account, store.clone(), false, move |news| {
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
