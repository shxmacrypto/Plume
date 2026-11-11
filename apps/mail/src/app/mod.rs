//! Plume Mail : la fenêtre. Les comptes et leurs dossiers à gauche, la liste des messages au milieu, le message
//! à droite. Chaque compte travaille sur son propre fil (`mail_core::engine`) ; la fenêtre lui passe des
//! commandes et reçoit ses nouvelles, sans jamais attendre le réseau.

pub mod forms;
pub mod i18n;
pub mod settings;
pub mod view;

use forms::{Confirm, Sheet};
use i18n::Strings;
use mail_core::{
    engine::{Command, Engine, News, Work},
    imap::{Folder, Role},
    message::{self, Parsed, Summary, fold},
    store::{Account, Contact, Store},
};
use plume_ui::{Area, Button, Canvas, Cursor, Cx, EditStyle, Event, Handler, Hits, Mods, PaintCx, Pt, Rect, Theme, Window, m, rect};
use settings::Settings;
use std::{any::Any, time::Instant};

/// Hauteur d'une ligne de la liste : expéditeur et date, puis objet.
pub const ROW_H: f32 = 58.0;
pub const TOP: f32 = m::TITLE_H + m::BAR_H;
const TIMER_SYNC: usize = 1;
const TIMER_TOAST: usize = 2;
/// La recherche dans le texte des messages part un peu après la dernière touche.
const TIMER_SEARCH: usize = 3;
/// Le serveur a annoncé du nouveau : on relève, une fois la rafale d'annonces passée.
const TIMER_PUSH: usize = 4;
/// Le courrier est relevé tout seul à cet intervalle.
const SYNC_EVERY_MS: u32 = 180_000;

const K_BACK: u16 = 0x08;
const K_TAB: u16 = 0x09;
const K_RETURN: u16 = 0x0D;
const K_ESCAPE: u16 = 0x1B;
const K_SPACE: u16 = 0x20;
const K_PRIOR: u16 = 0x21;
const K_NEXT: u16 = 0x22;
const K_END: u16 = 0x23;
const K_HOME: u16 = 0x24;
const K_LEFT: u16 = 0x25;
const K_UP: u16 = 0x26;
const K_RIGHT: u16 = 0x27;
const K_DOWN: u16 = 0x28;
const K_DELETE: u16 = 0x2E;
const K_F5: u16 = 0x74;

/// Ce que la souris peut toucher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Min,
    Max,
    Close,
    /// Une version plus récente de la suite : ouvrir Plume Suite, qui met tout à jour.
    Update,
    New,
    Reply,
    ReplyAll,
    Forward,
    Delete,
    Unread,
    Flag,
    Sync,
    More,
    Search,
    SearchClear,
    /// Un compte de la barre latérale ; un de ses dossiers.
    Account(usize),
    Folder(usize, usize),
    AddAccount,
    /// Une ligne de la liste (sa place parmi les messages affichés) ; le fond de la liste.
    Row(usize),
    /// Le nombre de messages d'une conversation : ouvrir ou fermer la conversation.
    ThreadToggle(usize),
    List,
    /// Le message ouvert, ses pièces jointes, ses boutons.
    Reading,
    Attachment(usize),
    Links,
    CopyText,
    EditDraft,
    /// Le menu ouvert : une de ses lignes ; le reste de la fenêtre, qui le ferme.
    Menu(usize),
    MenuDim,
    /// La boîte de dialogue : ses champs, ses boutons, son cadre, et le reste de la fenêtre assombri.
    Field(usize),
    SheetOk,
    SheetCancel,
    SheetReveal,
    Attach,
    SaveDraft,
    Chip(usize),
    /// Une adresse proposée sous le champ « À » ou « Cc ».
    Suggest(usize),
    SheetCard,
    SheetDim,
}

/// Ce que le champ de saisie ouvert édite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editing {
    Search,
    Field(usize),
}

/// Une ligne du menu, et ce qu'elle fait.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    AddAccount,
    EditAccount(usize),
    RemoveAccount(usize),
    Signature(usize),
    Threads,
    Theme(&'static str),
    Lang(&'static str),
    OpenLink(String),
}

pub struct Menu {
    /// Le coin haut droit du menu.
    pub at: Pt,
    pub items: Vec<(String, Action)>,
}

/// Un compte dans la fenêtre : ses réglages, son fil de travail, ses dossiers.
pub struct MailBox {
    pub account: Account,
    pub engine: Engine,
    pub folders: Vec<Folder>,
    pub work: Work,
    /// La dernière commande n'a pas abouti : pourquoi.
    pub error: Option<String>,
}

/// Le message ouvert à droite.
pub struct Reading {
    pub account: usize,
    pub folder: String,
    pub uid: u32,
    /// None : il arrive (du disque ou du serveur).
    pub message: Option<Parsed>,
    /// Les adresses web que son texte contient, dans l'ordre, sans doublon.
    pub links: Vec<String>,
    /// Son texte, paragraphe par paragraphe, et la hauteur de chacun pour la largeur `laid_for`.
    pub paragraphs: Vec<String>,
    pub heights: Vec<f32>,
    pub laid_for: f32,
    pub scroll: f32,
    pub max_scroll: f32,
}

/// Ce qu'une ligne de la liste montre de sa conversation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowInfo {
    /// Combien de messages la conversation compte (sur la ligne de tête ; 1 pour les autres).
    pub count: usize,
    /// Un message plus ancien d'une conversation ouverte, sous sa ligne de tête.
    pub child: bool,
    /// Ce qui désigne la conversation (vide : regroupement éteint).
    pub key: String,
    /// La ligne a du non-lu (pour une tête : un message de la conversation, au moins).
    pub unread: bool,
}

/// Les rectangles de la fenêtre, calculés à chaque image.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lay {
    pub w: f32,
    pub h: f32,
    pub nav: Rect,
    pub list: Rect,
    pub read: Rect,
    pub status: Rect,
    pub search: Rect,
    pub search_text: Rect,
    pub list_max: f32,
}

pub struct App {
    pub win: Window,
    pub theme: Theme,
    pub s: &'static Strings,
    /// Version plus récente de la suite, si elle existe.
    pub update: Option<String>,
    pub set: Settings,
    pub store: Store,
    pub boxes: Vec<MailBox>,
    /// Le compte et le dossier affichés.
    pub cur: Option<(usize, String)>,
    pub messages: Vec<Summary>,
    /// Les messages affichés (leur place dans `messages`), une fois la recherche appliquée.
    pub shown: Vec<usize>,
    /// Pour chaque ligne affichée : sa conversation.
    pub rows: Vec<RowInfo>,
    /// Les conversations ouvertes dans la liste.
    pub open_threads: std::collections::HashSet<String>,
    /// Ce que la recherche dans le texte des messages a trouvé : (le texte cherché, les numéros, triés).
    pub found: Option<(String, Vec<u32>)>,
    /// Les dossiers où le serveur a annoncé du nouveau, à relever.
    pub pushed: Vec<(usize, String)>,
    /// Les personnes connues du compte qui écrit, et celles proposées sous le champ d'adresse ouvert.
    pub contacts: Vec<Contact>,
    pub suggest: Vec<Contact>,
    pub suggest_at: usize,
    pub sel: Option<u32>,
    pub list_scroll: f32,
    pub reading: Option<Reading>,
    pub filter: String,
    pub edit: Option<Editing>,
    pub sheet: Option<Sheet>,
    pub menu: Option<Menu>,
    pub hits: Hits<Id>,
    pub hover: Option<Id>,
    pub pressed: Option<Id>,
    pub lay: Lay,
    pub toast: Option<(String, Instant)>,
    pub active: bool,
}

/// Les adresses web d'un texte, dans l'ordre, sans doublon (les vingt premières).
fn links_in(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for word in text.split(|c: char| c.is_whitespace() || "()<>[]\"".contains(c)) {
        let url = word.trim_end_matches(['.', ',', ';', ':', '!', '?', '\'']);
        if (url.starts_with("https://") || url.starts_with("http://")) && url.len() > 10 && !out.iter().any(|u| u == url) {
            out.push(url.to_string());
            if out.len() == 20 {
                break;
            }
        }
    }
    out
}

impl App {
    /// Installée, l'application regarde s'il existe une version plus récente de la suite : un bouton l'annonce
    /// dans la barre de titre, et Plume Suite met tout à jour.
    fn watch_updates(&self) {
        if !plume_update::should_check() {
            return;
        }
        let sender = self.win.sender();
        std::thread::spawn(move || {
            if let Some(v) = plume_update::newer_release() {
                sender.send(move |h, cx| {
                    if let Some(app) = h.as_any().downcast_mut::<App>() {
                        app.update = Some(v);
                        cx.repaint();
                    }
                });
            }
        });
    }

    pub fn new(win: Window, set: Settings, store: Store) -> App {
        let s = i18n::strings(set.language());
        let mut app = App {
            update: None,
            win,
            theme: Theme::new(set.dark(), set.accent),
            s,
            set,
            store,
            boxes: vec![],
            cur: None,
            messages: vec![],
            shown: vec![],
            rows: vec![],
            open_threads: Default::default(),
            found: None,
            pushed: vec![],
            contacts: vec![],
            suggest: vec![],
            suggest_at: 0,
            sel: None,
            list_scroll: 0.0,
            reading: None,
            filter: String::new(),
            edit: None,
            sheet: None,
            menu: None,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            lay: Lay::default(),
            toast: None,
            active: true,
        };
        app.watch_updates();
        for account in app.store.accounts() {
            app.start_box(account);
        }
        match app.boxes.is_empty() {
            // premier lancement : la boîte « Ajouter un compte » s'ouvre d'elle-même
            true => app.ask_account(None),
            false => {
                let inbox = app.inbox_of(0);
                app.open_folder(0, inbox);
            }
        }
        win.timer(TIMER_SYNC, Some(SYNC_EVERY_MS));
        app
    }

    /// Met un compte au travail : son fil, et ce qu'on gardait de lui sur le disque.
    pub fn start_box(&mut self, account: Account) -> usize {
        let sender = self.win.sender();
        let push = std::env::var_os("PLUME_MAIL_NO_PUSH").is_none();
        let engine = Engine::start(account.clone(), self.store.clone(), push, move |news| {
            sender.send(move |h, cx| {
                if let Some(app) = h.as_any().downcast_mut::<App>() {
                    app.on_news(news, cx);
                }
            });
        });
        let folders = self.store.folders(&account.id);
        self.boxes.push(MailBox { account, engine, folders, work: Work::Idle, error: None });
        self.boxes.len() - 1
    }

    /// Ce que fait un bouton de la barre d'outils, et son raccourci : affiché quand la souris passe dessus.
    pub fn hint_of(&self, id: Id) -> Option<String> {
        let s = self.s;
        let flags = self.selected().map(|m| m.flags);
        Some(match id {
            Id::Reply => format!("{} (Ctrl+R)", s.reply),
            Id::ReplyAll => format!("{} (Ctrl+Shift+R)", s.reply_all),
            Id::Forward => format!("{} (Ctrl+L)", s.forward),
            Id::Delete => format!("{} (Del)", s.delete),
            Id::Unread => format!("{} (Ctrl+U)", if flags.is_some_and(|f| !f.seen) { s.mark_read } else { s.mark_unread }),
            Id::Flag => format!("{} (Ctrl+D)", if flags.is_some_and(|f| f.flagged) { s.unflag } else { s.flag }),
            Id::Sync => format!("{} (F5)", s.sync),
            Id::New => format!("{} (Ctrl+N)", s.new_message),
            _ => return None,
        })
    }

    /// Le nom, pour le serveur, de la boîte de réception d'un compte.
    pub fn inbox_of(&self, b: usize) -> String {
        self.boxes.get(b).and_then(|mb| mb.folders.iter().find(|f| f.role == Role::Inbox)).map_or("INBOX".to_string(), |f| f.raw.clone())
    }

    /// Le nom affiché d'un dossier : les dossiers habituels dans la langue de l'interface.
    pub fn folder_label(&self, f: &Folder) -> String {
        let s = self.s;
        match f.role {
            Role::Inbox => s.inbox,
            Role::Drafts => s.drafts,
            Role::Sent => s.sent,
            Role::Archive => s.archive,
            Role::Junk => s.junk,
            Role::Trash => s.trash,
            Role::All => s.all_mail,
            Role::Other => return f.name.clone(),
        }
        .to_string()
    }

    fn update_title(&self) {
        let title = match &self.cur {
            Some((b, raw)) => match self.boxes.get(*b) {
                Some(mb) => {
                    let folder = mb.folders.iter().find(|f| f.raw == *raw).map_or(raw.clone(), |f| self.folder_label(f));
                    format!("{folder} - {} - Plume Mail", mb.account.email)
                }
                None => "Plume Mail".into(),
            },
            None => "Plume Mail".into(),
        };
        self.win.set_title(&title);
    }

    /// Affiche un dossier : d'abord ce qu'on en gardait, puis ce que le serveur en dit.
    pub fn open_folder(&mut self, b: usize, raw: String) {
        if b >= self.boxes.len() {
            return;
        }
        self.end_edit();
        let mb = &self.boxes[b];
        self.messages = self.store.listing(&mb.account.id, &raw).messages;
        mb.engine.send(Command::Sync { folder: raw.clone() });
        self.cur = Some((b, raw));
        self.filter.clear();
        self.found = None;
        self.refilter();
        self.sel = None;
        self.reading = None;
        self.list_scroll = 0.0;
        self.update_title();
        self.win.repaint();
    }

    /// Recalcule les messages affichés d'après le texte cherché (tous ses mots, dans l'objet ou les personnes, ou
    /// dans le texte des messages d'après la recherche du compte), puis les regroupe par conversation.
    pub fn refilter(&mut self) {
        self.regroup();
        self.list_scroll = 0.0;
    }

    /// Comme `refilter`, sans bouger la liste.
    pub fn regroup(&mut self) {
        let needle = fold(self.filter.trim());
        let words: Vec<&str> = needle.split_whitespace().collect();
        let found: Option<&Vec<u32>> = self.found.as_ref().filter(|(text, _)| fold(text.trim()) == needle).map(|(_, uids)| uids);
        let matches: Vec<usize> = (0..self.messages.len())
            .filter(|&i| {
                let m = &self.messages[i];
                words.is_empty() || {
                    let hay = fold(&format!("{} {} {} {}", m.subject, m.from_name, m.from, m.to));
                    words.iter().all(|w| hay.contains(w)) || found.is_some_and(|f| f.binary_search(&m.uid).is_ok())
                }
            })
            .collect();
        self.shown.clear();
        self.rows.clear();
        if !self.set.threads {
            for i in matches {
                self.shown.push(i);
                self.rows.push(RowInfo { count: 1, child: false, key: String::new(), unread: !self.messages[i].flags.seen });
            }
            return;
        }
        // la liste va du plus récent au plus ancien : la première ligne d'une conversation est son dernier message
        let thread = message::threads(&self.messages);
        let mut members: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
        for &i in &matches {
            members.entry(thread[i]).or_default().push(i);
        }
        let mut done = std::collections::HashSet::new();
        for &i in &matches {
            if !done.insert(thread[i]) {
                continue;
            }
            let group = &members[&thread[i]];
            // la conversation se reconnaît à son premier message, qui ne change pas quand une réponse arrive
            let first = &self.messages[*group.last().unwrap_or(&i)];
            let key = if first.message_id.is_empty() { format!("uid:{}", first.uid) } else { first.message_id.clone() };
            let open = group.len() > 1 && self.open_threads.contains(&key);
            let unread = group.iter().any(|&j| !self.messages[j].flags.seen);
            self.shown.push(group[0]);
            self.rows.push(RowInfo { count: group.len(), child: false, key: key.clone(), unread });
            if open {
                for &j in &group[1..] {
                    self.shown.push(j);
                    self.rows.push(RowInfo { count: 1, child: true, key: key.clone(), unread: !self.messages[j].flags.seen });
                }
            }
        }
    }

    /// Ouvre ou ferme une conversation de la liste (`open` : None pour basculer).
    pub fn fold_thread(&mut self, row: usize, open: Option<bool>) {
        let Some(info) = self.rows.get(row) else { return };
        let key = info.key.clone();
        let whole = self.rows.iter().filter(|r| r.key == key).map(|r| if r.child { 1 } else { r.count }).max().unwrap_or(1);
        if key.is_empty() || whole < 2 {
            return;
        }
        let now = self.open_threads.contains(&key);
        match open.unwrap_or(!now) {
            true => self.open_threads.insert(key),
            false => self.open_threads.remove(&key),
        };
        self.regroup();
        self.win.repaint();
    }

    /// La recherche dans le texte des messages, auprès du compte (ce qui est gardé ici, puis le serveur).
    fn search_text(&mut self) {
        let text = self.filter.trim().to_string();
        if text.is_empty() {
            return;
        }
        if let Some((b, folder)) = self.cur.clone() {
            self.boxes[b].engine.send(Command::Search { folder, text });
        }
    }

    /// Le message sélectionné, dans la liste.
    pub fn selected(&self) -> Option<&Summary> {
        let uid = self.sel?;
        self.messages.iter().find(|m| m.uid == uid)
    }

    /// Sélectionne la ligne `row` de la liste affichée et ouvre son message.
    pub fn select(&mut self, row: usize) {
        let (Some(&i), Some((b, folder))) = (self.shown.get(row), self.cur.clone()) else { return };
        let uid = self.messages[i].uid;
        self.sel = Some(uid);
        // la tête d'une conversation fermée : elle s'ouvre, ses messages plus anciens apparaissent dessous
        if self.rows.get(row).is_some_and(|r| !r.child && r.count > 1) {
            self.fold_thread(row, Some(true));
        }
        self.reading = Some(Reading {
            account: b,
            folder: folder.clone(),
            uid,
            message: None,
            links: vec![],
            paragraphs: vec![],
            heights: vec![],
            laid_for: 0.0,
            scroll: 0.0,
            max_scroll: 0.0,
        });
        self.boxes[b].engine.send(Command::Read { folder, uid });
        let row = self.shown.iter().position(|&k| self.messages[k].uid == uid).unwrap_or(row);
        self.reveal(row);
        self.win.repaint();
    }

    /// Fait défiler la liste pour que la ligne `row` soit visible.
    fn reveal(&mut self, row: usize) {
        let (top, view) = (row as f32 * ROW_H, self.lay.list.h);
        if top < self.list_scroll {
            self.list_scroll = top;
        } else if view > 0.0 && top + ROW_H > self.list_scroll + view {
            self.list_scroll = top + ROW_H - view;
        }
    }

    pub fn toast(&mut self, text: String) {
        self.toast = Some((text, Instant::now()));
        self.win.timer(TIMER_TOAST, Some(3600));
        self.win.repaint();
    }

    // ───────────────────────── les nouvelles des comptes ─────────────────────────

    pub fn on_news(&mut self, news: News, cx: &mut Cx) {
        let find = |boxes: &[MailBox], id: &str| boxes.iter().position(|mb| mb.account.id == id);
        match news {
            News::Folders { account, folders } => {
                if let Some(b) = find(&self.boxes, &account) {
                    self.boxes[b].folders = folders;
                    self.update_title();
                }
            }
            News::Messages { account, folder, messages } => {
                let Some(b) = find(&self.boxes, &account) else { return };
                self.boxes[b].error = None;
                if self.cur.as_ref().is_some_and(|(cb, cf)| *cb == b && *cf == folder) {
                    self.messages = messages;
                    self.regroup();
                    // le message ouvert a quitté le dossier (supprimé, déplacé) : plus rien à droite
                    if self.sel.is_some_and(|uid| !self.messages.iter().any(|m| m.uid == uid)) {
                        self.sel = None;
                        self.reading = None;
                    }
                }
            }
            News::Message { account, folder, uid, message } => {
                let Some(b) = find(&self.boxes, &account) else { return };
                if let Some(r) = self.reading.as_mut().filter(|r| r.account == b && r.folder == folder && r.uid == uid) {
                    r.paragraphs = message.text.lines().map(str::to_string).collect();
                    r.links = links_in(&message.text);
                    r.message = Some(*message);
                    r.laid_for = 0.0;
                    r.scroll = 0.0;
                }
            }
            News::Sent { account, result } => {
                if find(&self.boxes, &account).is_some() {
                    self.on_sent(result);
                }
            }
            News::DraftSaved { account, result } => {
                if let Some(b) = find(&self.boxes, &account) {
                    if let Ok((folder, _)) = &result
                        && self.cur.as_ref().is_some_and(|(cb, cf)| *cb == b && cf == folder)
                    {
                        self.boxes[b].engine.send(Command::Sync { folder: folder.clone() });
                    }
                    self.on_draft_saved(result);
                }
            }
            News::Found { account, folder, text, uids, extra } => {
                let Some(b) = find(&self.boxes, &account) else { return };
                if !self.cur.as_ref().is_some_and(|(cb, cf)| *cb == b && *cf == folder) {
                    return;
                }
                // trouvés plus loin que les messages gardés : ils rejoignent la liste le temps de la recherche
                let mut added = false;
                for m in extra {
                    if !self.messages.iter().any(|k| k.uid == m.uid) {
                        self.messages.push(m);
                        added = true;
                    }
                }
                if added {
                    self.messages.sort_by(|a, b| b.date.cmp(&a.date).then(b.uid.cmp(&a.uid)));
                }
                self.found = Some((text, uids));
                self.regroup();
            }
            News::Changed { account, folder } => {
                if let Some(b) = find(&self.boxes, &account) {
                    if !self.pushed.contains(&(b, folder.clone())) {
                        self.pushed.push((b, folder));
                    }
                    cx.timer(TIMER_PUSH, Some(400));
                }
            }
            News::Working { account, work } => {
                if let Some(b) = find(&self.boxes, &account) {
                    self.boxes[b].work = work;
                }
            }
            News::Failed { account, why } => {
                if let Some(b) = find(&self.boxes, &account) {
                    // un refus de s'identifier : dit simplement, suivi des mots du serveur
                    let lower = why.to_lowercase();
                    let refused =
                        ["authenticationfailed", "invalid credentials", "authentication failed", "login failed", "535"].iter().any(|k| lower.contains(k));
                    let why = if refused { format!("{} ({why})", self.s.sign_in_refused) } else { why };
                    self.boxes[b].error = Some(why.clone());
                    self.toast(why);
                }
            }
        }
        cx.repaint();
    }

    // ───────────────────────── ce que l'on fait des messages ─────────────────────────

    pub fn sync(&mut self) {
        if let Some((b, folder)) = self.cur.clone() {
            self.boxes[b].engine.send(Command::Sync { folder });
        } else {
            for b in 0..self.boxes.len() {
                let folder = self.inbox_of(b);
                self.boxes[b].engine.send(Command::Sync { folder });
            }
        }
    }

    fn toggle(&mut self, flag: &'static str) {
        let (Some((b, folder)), Some(m)) = (self.cur.clone(), self.selected()) else { return };
        let on = if flag == "\\Seen" { !m.flags.seen } else { !m.flags.flagged };
        let uid = m.uid;
        self.boxes[b].engine.send(Command::Flag { folder, uids: vec![uid], flag, on });
    }

    /// Supprime le message sélectionné : à la corbeille ; s'il y est déjà (ou s'il n'y en a pas), pour de bon,
    /// après confirmation.
    pub fn delete(&mut self) {
        let (Some((b, folder)), Some(uid)) = (self.cur.clone(), self.sel) else { return };
        let trash = self.boxes[b].folders.iter().find(|f| f.role == Role::Trash).map(|f| f.raw.clone());
        // la sélection passe au message suivant
        let row = self.shown.iter().position(|&i| self.messages[i].uid == uid);
        let next = row.and_then(|r| self.shown.get(r + 1).or(r.checked_sub(1).and_then(|p| self.shown.get(p)))).map(|&i| self.messages[i].uid);
        match trash {
            Some(trash) if trash != folder => {
                self.boxes[b].engine.send(Command::Move { folder, uids: vec![uid], to: trash });
                self.sel = None;
                self.reading = None;
                if let Some(next) = next
                    && let Some(row) = self.shown.iter().position(|&i| self.messages[i].uid == next)
                {
                    self.select(row);
                }
            }
            _ => {
                self.end_edit();
                self.sheet = Some(Sheet::Confirm {
                    title: self.s.delete_title.into(),
                    text: self.s.delete_text.into(),
                    action: self.s.delete.into(),
                    what: Confirm::DeleteMessage { account: b, folder, uid },
                });
            }
        }
        self.win.repaint();
    }

    /// Les adresses web du message ouvert.
    pub fn links(&self) -> &[String] {
        self.reading.as_ref().map_or(&[], |r| r.links.as_slice())
    }

    // ───────────────────────── champs de saisie ─────────────────────────

    pub fn edit_style(&self, placeholder: &str, password: bool, multiline: bool) -> EditStyle {
        EditStyle {
            fg: self.theme.t1,
            bg: self.theme.fill.over(self.theme.solid),
            size: 13.0,
            select: None,
            placeholder: placeholder.to_string(),
            password,
            multiline,
        }
    }

    pub fn start_search(&mut self) {
        if self.sheet.is_some() || self.cur.is_none() {
            return;
        }
        self.edit = Some(Editing::Search);
        let style = EditStyle { bg: self.theme.fill.over(self.theme.hdr2), ..self.edit_style(self.s.search, false, false) };
        self.win.edit_show(self.lay.search_text, &self.filter, &style);
        self.win.repaint();
    }

    /// Ferme le champ de saisie ouvert (ce qui y était tapé est déjà pris en compte).
    pub fn end_edit(&mut self) {
        if self.edit.take().is_some() {
            self.win.edit_hide();
            self.win.repaint();
        }
    }

    // ───────────────────────── souris et clavier ─────────────────────────

    fn click(&mut self, id: Id, cx: &mut Cx) {
        match id {
            Id::Update => {
                plume_update::open_suite_update();
            }
            Id::Min => cx.after(|w| w.minimize()),
            Id::Max => cx.after(|w| w.toggle_maximize()),
            Id::Close => cx.after(|w| w.close()),
            Id::New => self.compose(forms::Start::New),
            Id::Reply => self.compose(forms::Start::Reply { all: false }),
            Id::ReplyAll => self.compose(forms::Start::Reply { all: true }),
            Id::Forward => self.compose(forms::Start::Forward),
            Id::Delete => self.delete(),
            Id::Unread => self.toggle("\\Seen"),
            Id::Flag => self.toggle("\\Flagged"),
            Id::Sync => self.sync(),
            Id::More => self.more_menu(),
            Id::Search => self.start_search(),
            Id::SearchClear => {
                self.end_edit();
                self.filter.clear();
                self.refilter();
            }
            Id::Account(b) => {
                let inbox = self.inbox_of(b);
                self.open_folder(b, inbox);
            }
            Id::Folder(b, f) => {
                if let Some(folder) = self.boxes.get(b).and_then(|mb| mb.folders.get(f)).filter(|f| !f.no_select) {
                    let raw = folder.raw.clone();
                    self.open_folder(b, raw);
                }
            }
            Id::AddAccount => self.ask_account(None),
            Id::Row(row) => self.select(row),
            Id::ThreadToggle(row) => self.fold_thread(row, None),
            Id::EditDraft => self.edit_draft(),
            Id::Attachment(i) => self.save_attachment(i, cx),
            Id::Links => self.links_menu(),
            Id::CopyText => {
                if let Some(text) = self.reading.as_ref().and_then(|r| r.message.as_ref()).map(|m| m.text.clone()) {
                    self.win.copy_text(&text);
                    self.toast(self.s.copied.to_string());
                }
            }
            Id::Menu(i) => {
                if let Some(action) = self.menu.take().and_then(|m| m.items.into_iter().nth(i)).map(|(_, a)| a) {
                    self.act(action, cx);
                }
            }
            Id::MenuDim => self.menu = None,
            Id::Field(_) | Id::SheetOk | Id::SheetCancel | Id::SheetReveal | Id::Attach | Id::SaveDraft | Id::Chip(_) | Id::Suggest(_) => self.sheet_click(id, cx),
            Id::List | Id::Reading | Id::SheetCard | Id::SheetDim => {}
        }
        cx.repaint();
    }

    fn act(&mut self, action: Action, cx: &mut Cx) {
        match action {
            Action::AddAccount => self.ask_account(None),
            Action::EditAccount(b) => self.ask_account(Some(b)),
            Action::RemoveAccount(b) => {
                if let Some(email) = self.boxes.get(b).map(|mb| mb.account.email.clone()) {
                    self.end_edit();
                    self.sheet = Some(Sheet::Confirm {
                        title: self.s.remove_title.into(),
                        text: format!("{email} — {}", self.s.remove_text),
                        action: self.s.remove.into(),
                        what: Confirm::RemoveAccount(b),
                    });
                }
            }
            Action::Signature(b) => self.ask_signature(b),
            Action::Threads => {
                self.set.threads = !self.set.threads;
                self.set.save();
                self.regroup();
            }
            Action::Theme(theme) => {
                self.set.theme = theme.to_string();
                self.set.own_theme = true;
                self.set.save();
                self.theme = Theme::new(self.set.dark(), self.set.accent);
            }
            Action::Lang(lang) => {
                self.set.lang = lang.to_string();
                self.set.own_lang = true;
                self.set.save();
                self.s = i18n::strings(self.set.language());
                self.update_title();
            }
            Action::OpenLink(url) => {
                // seules les adresses web s'ouvrent : jamais un fichier ni un programme
                if url.starts_with("https://") || url.starts_with("http://") {
                    cx.after(move |w| {
                        w.open(&url);
                    });
                }
            }
        }
    }

    /// Le menu « … » : les comptes, le thème, la langue.
    fn more_menu(&mut self) {
        let s = self.s;
        let Some(button) = self.hits.rect_of(|id| *id == Id::More) else { return };
        let mut items = vec![(s.add_account.to_string(), Action::AddAccount)];
        for (b, mb) in self.boxes.iter().enumerate() {
            items.push((s.fill(s.edit_account, &[&mb.account.email]), Action::EditAccount(b)));
            items.push((s.fill(s.signature_menu, &[&mb.account.email]), Action::Signature(b)));
            items.push((s.fill(s.remove_account, &[&mb.account.email]), Action::RemoveAccount(b)));
        }
        items.extend([
            ((if self.set.threads { s.threads_off } else { s.threads_on }).to_string(), Action::Threads),
            (s.theme_system.to_string(), Action::Theme("system")),
            (s.theme_dark.to_string(), Action::Theme("dark")),
            (s.theme_light.to_string(), Action::Theme("light")),
            (s.lang_auto.to_string(), Action::Lang("auto")),
            ("English".to_string(), Action::Lang("en")),
            ("Français".to_string(), Action::Lang("fr")),
        ]);
        self.end_edit();
        self.menu = Some(Menu { at: plume_ui::pt(button.right(), button.bottom() + 6.0), items });
    }

    /// Le menu des liens du message ouvert.
    fn links_menu(&mut self) {
        let Some(button) = self.hits.rect_of(|id| *id == Id::Links) else { return };
        let items: Vec<(String, Action)> = self.links().iter().map(|url| (url.clone(), Action::OpenLink(url.clone()))).collect();
        if !items.is_empty() {
            self.menu = Some(Menu { at: plume_ui::pt(button.right(), button.bottom() + 6.0), items });
        }
    }

    /// Enregistre une pièce jointe du message ouvert, là où l'utilisateur le demande.
    fn save_attachment(&mut self, index: usize, cx: &mut Cx) {
        let Some(r) = self.reading.as_ref() else { return };
        let Some(name) = r.message.as_ref().and_then(|m| m.attachments.get(index)).map(|a| a.name.clone()) else { return };
        let Some(raw) = self.boxes.get(r.account).and_then(|mb| self.store.body(&mb.account.id, &r.folder, r.uid)) else { return };
        let Some(data) = mail_core::message::attachment(&raw, index) else { return };
        let (s, hwnd) = (self.s, self.win.hwnd);
        // un nom sans dossier ni caractère interdit par Windows
        let safe: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
        let ext = safe.rsplit_once('.').map(|(_, e)| e.to_string()).unwrap_or_default();
        cx.after(move |win| {
            let Some(path) = plume_ui::dialogs::save_as(hwnd, s.save_title, &safe, &ext, &[(s.all_files, "*.*")]) else { return };
            let text = match std::fs::write(&path, &data) {
                Ok(()) => s.fill(s.saved_to, &[&path.display().to_string()]),
                Err(_) => s.save_failed.to_string(),
            };
            win.with(|h, _| {
                if let Some(app) = h.as_any().downcast_mut::<App>() {
                    app.toast(text);
                }
            });
        });
    }

    fn hit(&self, p: Pt) -> Option<Id> {
        self.hits.at(p).copied()
    }

    fn mouse_down(&mut self, p: Pt, button: Button, cx: &mut Cx) {
        if button != Button::Left {
            return;
        }
        let id = self.hit(p);
        // un clic hors d'un champ de recherche ouvert le referme ; dans une boîte de dialogue, le champ reste
        if self.edit == Some(Editing::Search) && id != Some(Id::Search) {
            self.end_edit();
        }
        self.pressed = id;
        cx.repaint();
    }

    fn mouse_up(&mut self, p: Pt, button: Button, cx: &mut Cx) {
        if button != Button::Left {
            return;
        }
        let pressed = self.pressed.take();
        let id = self.hit(p);
        if let Some(id) = id.filter(|id| Some(*id) == pressed) {
            self.click(id, cx);
        }
        cx.repaint();
    }

    fn wheel(&mut self, p: Pt, dy: f32, cx: &mut Cx) {
        if self.sheet.is_some() || self.menu.is_some() {
            return;
        }
        if self.lay.read.contains(p) {
            if let Some(r) = self.reading.as_mut() {
                r.scroll = (r.scroll - dy * 60.0).clamp(0.0, r.max_scroll);
            }
        } else if self.lay.list.contains(p) {
            self.list_scroll = (self.list_scroll - dy * ROW_H * 1.5).clamp(0.0, self.lay.list_max);
        }
        cx.repaint();
    }

    fn scroll_reading(&mut self, by: f32) {
        if let Some(r) = self.reading.as_mut() {
            r.scroll = (r.scroll + by).clamp(0.0, r.max_scroll);
        }
    }

    /// Déplace la sélection dans la liste.
    fn step(&mut self, by: i32) {
        if self.shown.is_empty() {
            return;
        }
        let row = self.sel.and_then(|uid| self.shown.iter().position(|&i| self.messages[i].uid == uid));
        let to = match row {
            Some(r) => (r as i32 + by).clamp(0, self.shown.len() as i32 - 1) as usize,
            None => 0,
        };
        if row != Some(to) {
            self.select(to);
        }
    }

    /// Choisit la ligne `row` de la liste.
    fn step_to(&mut self, row: usize) {
        if row < self.shown.len() {
            self.select(row);
        }
    }

    fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) -> bool {
        let Mods { ctrl, shift, .. } = mods;
        let letter = |c: char| vk == c as u16;
        if self.menu.is_some() {
            if vk == K_ESCAPE {
                self.menu = None;
            }
            cx.repaint();
            return true;
        }
        if self.sheet.is_some() {
            match vk {
                K_ESCAPE => self.close_sheet(),
                K_RETURN => self.sheet_ok(cx),
                _ => {}
            }
            cx.repaint();
            return true;
        }
        let page = (self.lay.read.h - 80.0).max(60.0);
        match vk {
            K_UP => self.step(-1),
            K_DOWN => self.step(1),
            // fermer ou ouvrir la conversation du message choisi
            K_LEFT | K_RIGHT => {
                if let Some(row) = self.sel.and_then(|uid| self.shown.iter().position(|&i| self.messages[i].uid == uid)) {
                    let head = (0..=row).rev().find(|&r| !self.rows.get(r).is_some_and(|i| i.child)).unwrap_or(row);
                    if vk == K_LEFT && head != row {
                        // le message choisi disparaît dans sa conversation : c'est sa tête qui reste choisie
                        self.fold_thread(head, Some(false));
                        self.step_to(head);
                    } else {
                        self.fold_thread(head, Some(vk == K_RIGHT));
                    }
                }
            }
            K_HOME => self.step(-(self.shown.len() as i32)),
            K_END => self.step(self.shown.len() as i32),
            K_NEXT | K_SPACE => self.scroll_reading(page),
            K_PRIOR => self.scroll_reading(-page),
            K_DELETE | K_BACK => self.delete(),
            K_F5 => self.sync(),
            K_ESCAPE => {
                self.filter.clear();
                self.refilter();
            }
            _ if letter('N') && ctrl => self.compose(forms::Start::New),
            _ if letter('R') && ctrl => self.compose(forms::Start::Reply { all: shift }),
            _ if letter('L') && ctrl => self.compose(forms::Start::Forward),
            _ if (letter('F') || letter('E') || letter('K')) && ctrl => self.start_search(),
            _ if letter('U') && ctrl => self.toggle("\\Seen"),
            _ if letter('D') && ctrl => self.toggle("\\Flagged"),
            _ => return false,
        }
        cx.repaint();
        true
    }

    fn edit_key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        match self.edit {
            Some(Editing::Search) => match vk {
                K_ESCAPE => {
                    self.filter.clear();
                    self.refilter();
                    self.end_edit();
                }
                // Entrée ou flèche bas : dans la liste, sur le premier message trouvé
                K_RETURN | K_DOWN | K_TAB => {
                    self.end_edit();
                    self.step(0);
                }
                _ => {}
            },
            Some(Editing::Field(_)) => self.field_key(vk, mods, cx),
            None => {}
        }
        cx.repaint();
    }

    fn on_close(&mut self) {
        self.set.win = self.win.placement();
        self.set.save();
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_all(c, cx);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                let id = self.hit(p);
                if id != self.hover {
                    self.hover = id;
                    cx.repaint();
                }
            }
            Event::MouseDown { p, button, .. } => self.mouse_down(p, button, cx),
            Event::MouseUp { p, button, .. } => self.mouse_up(p, button, cx),
            Event::MouseLeave => {
                if self.hover.take().is_some() {
                    cx.repaint();
                }
            }
            Event::Wheel { p, dy, .. } => self.wheel(p, dy, cx),
            Event::Key { vk, mods, .. } => {
                if self.key(vk, mods, cx) {
                    cx.handled();
                }
            }
            Event::Active(on) => {
                self.active = on;
                cx.repaint();
            }
            Event::Resized | Event::Scale => {
                // le champ ouvert suit sa case
                self.place_edit();
                cx.repaint();
            }
            Event::SystemTheme => {
                self.theme = Theme::new(self.set.dark(), self.set.accent);
                cx.repaint();
            }
            Event::Timer(TIMER_SYNC) => self.sync(),
            Event::Timer(TIMER_SEARCH) => {
                cx.timer(TIMER_SEARCH, None);
                self.search_text();
            }
            Event::Timer(TIMER_PUSH) => {
                cx.timer(TIMER_PUSH, None);
                for (b, folder) in std::mem::take(&mut self.pushed) {
                    if let Some(mb) = self.boxes.get(b) {
                        mb.engine.send(Command::Sync { folder });
                    }
                }
            }
            Event::Timer(TIMER_TOAST) => {
                self.toast = None;
                cx.timer(TIMER_TOAST, None);
                cx.repaint();
            }
            Event::EditChanged(text) => {
                match self.edit {
                    Some(Editing::Search) => {
                        self.filter = text;
                        self.refilter();
                        // le texte des messages : un peu après la dernière touche, pour ne pas chercher à chaque lettre
                        cx.timer(TIMER_SEARCH, if self.filter.trim().chars().count() >= 2 { Some(450) } else { None });
                    }
                    Some(Editing::Field(i)) => self.field_changed(i, text),
                    None => {}
                }
                cx.repaint();
            }
            Event::EditKey { vk, mods } => self.edit_key(vk, mods, cx),
            Event::EditBlur => {
                // le champ de recherche se referme quand on le quitte ; ceux d'une boîte de dialogue restent
                if self.edit == Some(Editing::Search) {
                    self.end_edit();
                }
            }
            Event::Close => self.on_close(),
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Id::Max) => Area::MaxButton,
            Some(Id::Min | Id::Close | Id::Update) => Area::Control,
            Some(_) => Area::Client,
            None if p.y < m::TITLE_H => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, _p: Pt) -> Cursor {
        Cursor::Arrow
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

/// La zone de texte d'un champ dessiné : un peu en retrait de son cadre.
pub fn text_of(field: Rect) -> Rect {
    rect(field.x + 10.0, field.y, (field.w - 20.0).max(10.0), field.h)
}
