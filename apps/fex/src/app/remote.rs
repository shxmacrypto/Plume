//! Les serveurs SFTP dans la fenêtre (version d'essai) : s'y connecter, les parcourir, ouvrir, télécharger,
//! envoyer, renommer, supprimer. Un fil par serveur garde la session ouverte et fait le travail, l'un après l'autre.

use super::{
    App,
    actions::with_app,
    load::{ListOpts, Listing, LoadError, Loaded},
    model::{Item, Load, Location, Restore, sort_items},
    sheet::{Sheet, SheetKind},
};
use crate::{
    shell::{
        self,
        menu::Menu,
        ops,
        sftp::{self, Ask, Entry, Question, Server, Session},
    },
    ui::{Cx, Sender},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// Un travail confié au fil d'un serveur : il reçoit la session, ou la raison pour laquelle il n'y en a pas.
type Job = Box<dyn FnOnce(Result<&mut Session, &str>) + Send>;

/// Les serveurs auxquels la fenêtre est reliée : pour chacun, le numéro de sa liaison et la file de ses travaux.
#[derive(Default)]
pub struct Remotes {
    links: HashMap<String, (u64, mpsc::Sender<Job>)>,
    next: u64,
}

/// Numéros des entrées des menus d'un dossier de serveur.
mod cmd {
    pub const OPEN: u32 = 0x7101;
    pub const OPEN_TAB: u32 = 0x7102;
    pub const DOWNLOAD: u32 = 0x7103;
    pub const DOWNLOAD_TO: u32 = 0x7104;
    pub const RENAME: u32 = 0x7105;
    pub const DELETE: u32 = 0x7106;
    pub const COPY_ADDRESS: u32 = 0x7107;
    pub const PASTE: u32 = 0x7108;
    pub const UPLOAD: u32 = 0x7109;
    pub const NEW_FOLDER: u32 = 0x710A;
    pub const REFRESH: u32 = 0x710B;
}

/// Ce qui vient de se terminer sur un serveur, et ce que la fenêtre doit en faire.
enum Done {
    /// Téléchargé dans ce dossier ; `open` : le fichier à ouvrir ensuite.
    Downloaded { to: PathBuf, open: Option<PathBuf> },
    /// Le dossier du serveur a changé : le relire, en retrouvant ces éléments.
    Changed { server: String, dir: String, select: Vec<String>, rename: bool, message: Option<String> },
}

fn to_app(sender: &Sender, f: impl FnOnce(&mut App, &mut Cx) + Send + 'static) {
    sender.send(move |h, cx| {
        if let Some(app) = h.as_any().downcast_mut::<App>() {
            f(app, cx);
        }
    });
}

/// Ouvre la session. Pendant les essais automatiques, PLUME_FEX_SFTP_DIRECT désigne un programme serveur de ce
/// PC, auquel on parle sans réseau ni mot de passe.
fn open(server: &Server, ask: Ask) -> Result<Session, String> {
    #[cfg(feature = "test-hook")]
    if let Ok(program) = std::env::var("PLUME_FEX_SFTP_DIRECT") {
        return Session::open_direct(&program);
    }
    Session::open(server, ask)
}

/// Un nom de fichier du serveur devenu acceptable pour Windows.
fn windows_name(name: &str) -> String {
    let clean: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let clean = clean.trim_end_matches(['.', ' ']).to_string();
    if clean.is_empty() { "_".into() } else { clean }
}

impl Item {
    /// Un élément d'un dossier de serveur. Comme sur ces systèmes, un nom qui commence par un point est masqué.
    pub fn remote(e: &Entry) -> Item {
        use fex_core::listing::{ATTR_DIRECTORY, ATTR_HIDDEN, ATTR_REPARSE};
        let mut attrs = if e.dir { ATTR_DIRECTORY } else { 0x20 };
        if e.link {
            attrs |= ATTR_REPARSE;
        }
        if e.name.starts_with('.') {
            attrs |= ATTR_HIDDEN;
        }
        Item {
            name: e.name.clone(),
            attrs,
            size: e.size,
            sized: false,
            mtime: e.mtime,
            ctime: 0,
            sel: false,
            icon: std::cell::Cell::new(-1),
            // pas d'icône à demander à Windows pour un fichier qui n'est pas sur ce PC
            asked: std::cell::Cell::new(true),
            drive: None,
            folder: None,
        }
    }
}

/// L'avancement d'un transfert, partagé avec le fil qui le fait. `total` à 0 : taille inconnue (un dossier).
#[derive(Default)]
pub struct Transfer {
    pub total: AtomicU64,
    pub done: AtomicU64,
    pub running: AtomicBool,
}

/// Octets d'un fichier, ou de tout ce que contient un dossier.
pub fn local_size(p: &std::path::Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p).map(|r| r.flatten().map(|e| local_size(&e.path())).sum()).unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Tant que le transfert dure, regarde ce qui est déjà arrivé sur ce PC (un téléchargement s'écrit au fur et à mesure).
fn watch_local(t: Arc<Transfer>, targets: Vec<PathBuf>, sender: Sender) {
    std::thread::spawn(move || {
        while t.running.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(250));
            t.done.store(targets.iter().map(|p| local_size(p)).sum(), Ordering::Relaxed);
            sender.send(|_, cx| cx.repaint());
        }
    });
}

impl App {
    /// Le serveur et le dossier de l'onglet actif, s'il montre un serveur.
    fn remote_here(&self) -> Option<(String, String)> {
        match &self.tab().loc {
            Location::Remote { server, path } if !path.is_empty() => Some((server.clone(), path.clone())),
            _ => None,
        }
    }

    /// Confie un travail au fil du serveur. La connexion se fait à la première demande.
    fn remote_job(&mut self, server: &str, job: impl FnOnce(Result<&mut Session, &str>) + Send + 'static) {
        let mut job: Job = Box::new(job);
        if let Some((_, link)) = self.remotes.links.get(server) {
            match link.send(job) {
                Ok(()) => return,
                // ce fil s'est arrêté (connexion perdue) : on en lance un autre
                Err(mpsc::SendError(back)) => job = back,
            }
        }
        let Some(target) = Server::parse(server) else { return job(Err(self.s.sftp_bad_address)) };
        if sftp::client().is_none() {
            return job(Err(self.s.sftp_no_client));
        }
        let (tx, rx) = mpsc::channel::<Job>();
        let _ = tx.send(job);
        self.remotes.next += 1;
        let link = self.remotes.next;
        self.remotes.links.insert(server.to_string(), (link, tx));
        let (sender, label) = (self.win.sender(), server.to_string());
        std::thread::spawn(move || {
            // une question de ssh (mot de passe) : la fenêtre la pose, ce fil attend la réponse
            let (asking, about) = (sender, label.clone());
            let ask: Ask = Box::new(move |q: Question| {
                let (reply, answer) = mpsc::channel();
                let about = about.clone();
                to_app(&asking, move |app, _| app.ask_secret(about, q, reply));
                answer.recv().ok().flatten()
            });
            let failed = match open(&target, ask) {
                Ok(mut session) => {
                    // jusqu'à ce que la fenêtre se détache de ce serveur, ou que la connexion tombe
                    for job in rx.iter() {
                        job(Ok(&mut session));
                        if !session.alive() {
                            break;
                        }
                    }
                    "The connection to the server is closed.".to_string()
                }
                Err(why) => why,
            };
            // ce qui attendait encore n'aura pas de session
            while let Ok(job) = rx.try_recv() {
                job(Err(&failed));
            }
            to_app(&sender, move |app, _| {
                if app.remotes.links.get(&label).is_some_and(|(n, _)| *n == link) {
                    app.remotes.links.remove(&label);
                }
            });
        });
    }

    /// `ssh` pose une question pendant la connexion : la boîte de dialogue la montre, la réponse part par `reply`.
    fn ask_secret(&mut self, server: String, q: Question, reply: mpsc::Sender<Option<String>>) {
        self.end_edit(true);
        let error = q.again.then(|| self.s.pw_wrong.to_string());
        self.sheet = Some(Sheet { kind: SheetKind::Secret { server, prompt: q.text.trim().to_string(), secret: q.secret, reply }, error, reveal: !q.secret });
        self.open_sheet_field();
        self.win.bring_to_front();
    }

    /// Se détache d'un serveur : sa session se ferme.
    pub(super) fn remote_disconnect(&mut self, server: &str) {
        self.remotes.links.remove(server);
    }

    pub(super) fn remote_connected(&self, server: &str) -> bool {
        self.remotes.links.contains_key(server)
    }

    // ───────────────────────── parcourir ─────────────────────────

    /// (Re)lit le dossier de serveur de l'onglet `i`.
    pub(super) fn remote_load(&mut self, i: usize) {
        let opts = self.list_opts(&self.tabs[i]);
        let sender = self.win.sender();
        let tab = &mut self.tabs[i];
        let Location::Remote { server, path } = tab.loc.clone() else { return };
        tab.cancel.store(true, Ordering::Relaxed);
        tab.cancel = Arc::new(AtomicBool::new(false));
        tab.generation += 1;
        tab.checked = Instant::now();
        tab.load = Load::Loading(Instant::now());
        let (id, generation) = (tab.id, tab.generation);
        self.remote_job(&server, move |session| {
            let t0 = Instant::now();
            let result = session.map_err(str::to_string).and_then(|s| {
                // sans chemin : le dossier où le serveur nous place
                let dir = if path.is_empty() { s.home.clone() } else { path };
                s.list(&dir).map(|entries| (dir, entries))
            });
            let took_ms = t0.elapsed().as_secs_f32() * 1000.0;
            to_app(&sender, move |app, cx| app.on_remote_listed(id, generation, result, opts, took_ms, cx));
        });
        self.win.repaint();
    }

    fn on_remote_listed(&mut self, tab: u64, generation: u64, result: Result<(String, Vec<Entry>), String>, opts: ListOpts, took_ms: f32, cx: &mut Cx) {
        let Some(i) = self.tabs.iter().position(|t| t.id == tab && t.generation == generation) else { return };
        let result = match result {
            Ok((dir, entries)) => {
                if let Location::Remote { server, path } = &mut self.tabs[i].loc {
                    // le dossier d'accueil n'est connu qu'une fois connecté
                    if path.is_empty() {
                        *path = dir;
                    }
                    let server = server.clone();
                    self.remember_server(&server);
                    self.update_title();
                }
                let mut items: Vec<Item> = entries.iter().map(Item::remote).filter(|it| opts.show_hidden || !it.is_hidden()).collect();
                sort_items(&mut items, opts.sort, opts.asc);
                Ok(Listing { items, sig: 0, unchanged: false })
            }
            Err(why) => Err(LoadError::Other(why)),
        };
        self.on_loaded(Loaded { tab, generation, result, took_ms, silent: false }, cx);
    }

    /// Garde un serveur dans la barre latérale, pour y revenir d'un clic.
    fn remember_server(&mut self, server: &str) {
        if !self.set.servers.iter().any(|s| s == server) {
            self.set.servers.push(server.to_string());
            self.set.save();
            self.rebuild_nav();
        }
    }

    pub(super) fn forget_server(&mut self, server: &str) {
        self.remote_disconnect(server);
        self.set.servers.retain(|s| s != server);
        self.set.save();
        self.rebuild_nav();
        self.win.repaint();
    }

    /// Ouvre un élément d'un serveur : on entre dans un dossier (ou un lien) ; un fichier est téléchargé dans le
    /// dossier temporaire puis ouvert avec son application.
    pub(super) fn remote_open(&mut self, row: usize, new_tab: bool) {
        let Some((server, dir)) = self.remote_here() else { return };
        let tab = self.tab();
        if row >= tab.rows() {
            return;
        }
        let it = tab.item(row);
        let target = sftp::join(&dir, &it.name);
        if it.is_dir() || it.attrs & fex_core::listing::ATTR_REPARSE != 0 {
            let loc = Location::Remote { server, path: target };
            return if new_tab { self.new_tab(loc, false) } else { self.navigate(loc) };
        }
        let folder = std::env::temp_dir().join("PlumeFEx").join("sftp").join(windows_name(&server));
        let file = folder.join(windows_name(&it.name));
        self.remote_download(server, vec![(target, false)], folder, Some(file));
    }

    // ───────────────────────── transferts ─────────────────────────

    fn remote_busy(&mut self, label: String) {
        self.jobs += 1;
        self.busy = Some((label, Instant::now()));
        self.win.repaint();
    }

    /// Télécharge des éléments du serveur (chemin, dossier ?) dans un dossier de ce PC.
    fn remote_download(&mut self, server: String, items: Vec<(String, bool)>, to: PathBuf, open: Option<PathBuf>) {
        let what = match items.as_slice() {
            [(one, _)] => one.rsplit('/').next().unwrap_or_default().to_string(),
            many => self.s.count(many.len() as u64, self.s.item),
        };
        self.remote_busy(self.s.fill(self.s.sftp_downloading, &[&what]));
        // la taille attendue : celle des fichiers de la liste (un dossier : inconnue, on montre ce qui arrive)
        let tab = self.tab();
        let total: Option<u64> =
            items.iter().map(|(p, dir)| if *dir { None } else { tab.row_of(p.rsplit('/').next().unwrap_or_default()).map(|r| tab.item(r).size) }).sum();
        let transfer = Arc::new(Transfer::default());
        transfer.total.store(total.unwrap_or(0), Ordering::Relaxed);
        transfer.running.store(true, Ordering::Relaxed);
        self.transfer = Some(transfer.clone());
        // un fichier comme un dossier arrive dans `to` sous son nom
        let targets: Vec<PathBuf> = items.iter().map(|(p, _)| to.join(windows_name(p.rsplit('/').next().unwrap_or_default()))).collect();
        let sender = self.win.sender();
        watch_local(transfer.clone(), targets, sender);
        self.remote_job(&server, move |session| {
            let result = session.map_err(str::to_string).and_then(|s| {
                std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
                for (path, dir) in &items {
                    let name = windows_name(path.rsplit('/').next().unwrap_or_default());
                    // un dossier arrive dans `to` sous son nom ; un fichier reçoit le sien
                    s.get(path, &if *dir { to.clone() } else { to.join(name) }, *dir)?;
                }
                Ok(())
            });
            transfer.running.store(false, Ordering::Relaxed);
            to_app(&sender, move |app, cx| app.remote_done(result, Done::Downloaded { to, open }, cx));
        });
    }

    /// Envoie des fichiers et des dossiers de ce PC dans le dossier de serveur de l'onglet actif.
    pub(crate) fn remote_upload(&mut self, files: Vec<PathBuf>) {
        let Some((server, dir)) = self.remote_here() else { return };
        let files: Vec<PathBuf> = files.into_iter().filter(|f| f.exists()).collect();
        if files.is_empty() {
            return;
        }
        let names: Vec<String> = files.iter().filter_map(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned()).collect();
        let what = match names.as_slice() {
            [one] => one.clone(),
            many => self.s.count(many.len() as u64, self.s.item),
        };
        self.remote_busy(self.s.fill(self.s.sftp_uploading, &[&what]));
        let message = Some(self.s.fill(self.s.sftp_uploaded, &[&what]));
        let sizes: Vec<u64> = files.iter().map(|f| local_size(f)).collect();
        let transfer = Arc::new(Transfer::default());
        transfer.total.store(sizes.iter().sum(), Ordering::Relaxed);
        transfer.running.store(true, Ordering::Relaxed);
        self.transfer = Some(transfer.clone());
        let sender = self.win.sender();
        let (to, link) = (dir.clone(), server.clone());
        self.remote_job(&server, move |session| {
            let result = session.map_err(str::to_string).and_then(|s| {
                for (f, size) in files.iter().zip(&sizes) {
                    let Some(name) = f.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
                    // un dossier arrive dans le dossier du serveur sous son nom ; un fichier reçoit le sien
                    if f.is_dir() {
                        s.put(f, &to, true)?
                    } else {
                        s.put(f, &sftp::join(&to, &name), false)?
                    }
                    transfer.done.fetch_add(*size, Ordering::Relaxed);
                    to_app(&sender, |_, cx| cx.repaint());
                }
                Ok(())
            });
            transfer.running.store(false, Ordering::Relaxed);
            to_app(&sender, move |app, cx| app.remote_done(result, Done::Changed { server: link, dir, select: names, rename: false, message }, cx));
        });
    }

    /// Supprime des éléments du serveur (nom, dossier ?) : appelé une fois la confirmation donnée.
    pub(super) fn remote_delete(&mut self, server: String, dir: String, names: Vec<(String, bool)>) {
        self.remote_busy(self.s.sftp_deleting.to_string());
        let sender = self.win.sender();
        let (at, link) = (dir.clone(), server.clone());
        self.remote_job(&server, move |session| {
            let result = session.map_err(str::to_string).and_then(|s| names.iter().try_for_each(|(name, is_dir)| s.remove(&sftp::join(&at, name), *is_dir)));
            to_app(&sender, move |app, cx| app.remote_done(result, Done::Changed { server: link, dir, select: vec![], rename: false, message: None }, cx));
        });
    }

    /// Renomme un élément du dossier de serveur de l'onglet `tab`.
    pub(super) fn remote_rename(&mut self, old: &str, new: &str) {
        let Some((server, dir)) = self.remote_here() else { return };
        if new.contains('/') {
            return self.toast(self.s.fill(self.s.invalid_name, &["/"]));
        }
        let (from, to, sender) = (sftp::join(&dir, old), sftp::join(&dir, new), self.win.sender());
        let (link, new) = (server.clone(), new.to_string());
        self.remote_busy(String::new());
        self.remote_job(&server, move |session| {
            let result = session.map_err(str::to_string).and_then(|s| s.rename(&from, &to));
            to_app(&sender, move |app, cx| app.remote_done(result, Done::Changed { server: link, dir, select: vec![new], rename: false, message: None }, cx));
        });
    }

    /// Crée un dossier sur le serveur, puis ouvre son renommage.
    pub(super) fn remote_new_folder(&mut self) {
        let Some((server, dir)) = self.remote_here() else { return };
        self.end_edit(true);
        // un nom libre parmi ceux du dossier affiché
        let base = self.s.new_folder;
        let taken = |name: &str| self.tab().items.iter().any(|it| it.name == name);
        let name = (1..).map(|n| if n == 1 { base.to_string() } else { format!("{base} ({n})") }).find(|n| !taken(n)).unwrap_or_default();
        let (path, sender, link) = (sftp::join(&dir, &name), self.win.sender(), server.clone());
        self.remote_busy(String::new());
        self.remote_job(&server, move |session| {
            let result = session.map_err(str::to_string).and_then(|s| s.mkdir(&path));
            to_app(&sender, move |app, cx| app.remote_done(result, Done::Changed { server: link, dir, select: vec![name], rename: true, message: None }, cx));
        });
    }

    fn remote_done(&mut self, result: Result<(), String>, done: Done, cx: &mut Cx) {
        self.jobs = self.jobs.saturating_sub(1);
        if self.jobs == 0 {
            self.busy = None;
            self.transfer = None;
        }
        // même après une erreur, le dossier du serveur a pu changer en partie : on le relit
        if let Done::Changed { server, dir, select, rename, .. } = &done {
            let shown = Location::Remote { server: server.clone(), path: dir.clone() };
            for i in (0..self.tabs.len()).filter(|&i| self.tabs[i].loc == shown).collect::<Vec<_>>() {
                let tab = &mut self.tabs[i];
                tab.filter.clear();
                tab.restore =
                    Some(Restore { select: select.clone(), focus: select.first().cloned(), scroll: Some(tab.target), rename: *rename && result.is_ok() });
                self.remote_load(i);
            }
        }
        match (result, done) {
            (Err(why), _) => self.toast(why),
            (Ok(()), Done::Downloaded { open: Some(file), .. }) => {
                let hwnd = self.win.hwnd;
                cx.after(move |_| {
                    shell::open(hwnd, &file);
                });
            }
            (Ok(()), Done::Downloaded { to, open: None }) => self.toast(self.s.fill(self.s.sftp_downloaded, &[&to.display().to_string()])),
            (Ok(()), Done::Changed { message: Some(message), .. }) => self.toast(message),
            (Ok(()), Done::Changed { .. }) => {}
        }
        cx.repaint();
    }

    // ───────────────────────── menus et clavier ─────────────────────────

    /// Les éléments sélectionnés du dossier de serveur : (nom, dossier ?).
    fn remote_selection(&self) -> Vec<(String, bool)> {
        let tab = self.tab();
        tab.selected_rows().into_iter().map(|r| tab.item(r)).map(|it| (it.name.clone(), it.is_dir())).collect()
    }

    /// Télécharge la sélection : dans le dossier Téléchargements, ou dans un dossier à choisir.
    fn remote_download_selection(&mut self, choose: bool, cx: &mut Cx) {
        let Some((server, dir)) = self.remote_here() else { return };
        let items: Vec<(String, bool)> = self.remote_selection().into_iter().map(|(name, is_dir)| (sftp::join(&dir, &name), is_dir)).collect();
        if items.is_empty() {
            return;
        }
        if !choose {
            let to = shell::places::personal()
                .into_iter()
                .map(|p| p.path)
                .find(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("Downloads")))
                .unwrap_or_else(std::env::temp_dir);
            return self.remote_download(server, items, to, None);
        }
        let (hwnd, title) = (self.win.hwnd, self.s.sftp_download_to);
        cx.after(move |win| {
            if let Some(to) = shell::dialogs::pick_folder(hwnd, title.trim_end_matches('…')) {
                with_app(win, |app, _| app.remote_download(server, items, to, None));
            }
        });
    }

    /// Demande confirmation avant de supprimer la sélection : sur un serveur, rien ne va à la corbeille.
    pub(super) fn remote_ask_delete(&mut self) {
        let Some((server, dir)) = self.remote_here() else { return };
        let names = self.remote_selection();
        if names.is_empty() {
            return;
        }
        self.end_edit(true);
        self.sheet = Some(Sheet { kind: SheetKind::Delete { server, dir, names }, error: None, reveal: false });
        self.win.repaint();
    }

    /// Envoie sur le serveur les fichiers copiés dans le presse-papiers.
    pub(super) fn remote_paste(&mut self) {
        let files = ops::clipboard_files(self.win.hwnd);
        if files.is_empty() {
            return self.toast(self.s.sftp_nothing_to_paste.to_string());
        }
        self.remote_upload(files);
    }

    fn remote_command(&mut self, id: u32, row: Option<usize>, cx: &mut Cx) {
        match (id, row) {
            (cmd::OPEN, Some(r)) => self.remote_open(r, false),
            (cmd::OPEN_TAB, Some(r)) => self.remote_open(r, true),
            (cmd::DOWNLOAD, _) => self.remote_download_selection(false, cx),
            (cmd::DOWNLOAD_TO, _) => self.remote_download_selection(true, cx),
            (cmd::RENAME, _) => self.start_rename(),
            (cmd::DELETE, _) => self.remote_ask_delete(),
            (cmd::COPY_ADDRESS, _) => {
                if let Some((server, dir)) = self.remote_here() {
                    let text =
                        self.remote_selection().iter().map(|(name, _)| Location::remote_url(&server, &sftp::join(&dir, name))).collect::<Vec<_>>().join("\r\n");
                    // rien de sélectionné : l'adresse du dossier lui-même
                    let text = if text.is_empty() { Location::remote_url(&server, &dir) } else { text };
                    ops::copy_text(self.win.hwnd, &text);
                    self.toast(self.s.copied_path.to_string());
                }
            }
            (cmd::PASTE, _) => self.remote_paste(),
            (cmd::UPLOAD, _) => {
                let (hwnd, title) = (self.win.hwnd, self.s.sftp_upload);
                cx.after(move |win| {
                    let files = shell::dialogs::pick_files(hwnd, title.trim_end_matches('…'));
                    with_app(win, |app, _| app.remote_upload(files));
                });
            }
            (cmd::NEW_FOLDER, _) => self.remote_new_folder(),
            (cmd::REFRESH, _) => self.refresh(),
            _ => {}
        }
    }

    /// Menu d'un élément d'un dossier de serveur.
    pub(super) fn remote_row_menu(&mut self, row: usize, at: (i32, i32), cx: &mut Cx) {
        let s = self.s;
        let tab = self.tab();
        if row >= tab.rows() || !matches!(tab.load, Load::Ready) {
            return;
        }
        let (count, folder) = (tab.sel_count, tab.item(row).is_dir());
        let mut m = Menu::new();
        if count <= 1 {
            m.item(cmd::OPEN, s.open);
            if folder {
                m.item(cmd::OPEN_TAB, s.open_new_tab);
            }
            m.sep();
        }
        m.item(cmd::DOWNLOAD, s.sftp_download).item(cmd::DOWNLOAD_TO, s.sftp_download_to).sep();
        if count <= 1 {
            m.item(cmd::RENAME, s.rename);
        }
        m.item(cmd::DELETE, s.delete).sep().item(cmd::COPY_ADDRESS, s.copy_address);
        cx.after(move |win| {
            if let Some(id) = m.show(win, at, false) {
                with_app(win, |app, cx| app.remote_command(id, Some(row), cx));
            }
        });
    }

    /// Menu du fond d'un dossier de serveur.
    pub(super) fn remote_background_menu(&mut self, at: (i32, i32), cx: &mut Cx) {
        let s = self.s;
        if self.remote_here().is_none() {
            return;
        }
        let mut m = Menu::new();
        m.item(cmd::PASTE, s.paste)
            .item(cmd::UPLOAD, s.sftp_upload)
            .sep()
            .item(cmd::NEW_FOLDER, s.new_folder)
            .sep()
            .item(cmd::REFRESH, s.refresh)
            .item(cmd::COPY_ADDRESS, s.copy_address);
        cx.after(move |win| {
            if let Some(id) = m.show(win, at, false) {
                with_app(win, |app, cx| {
                    // « Copier l'adresse » sur le fond : celle du dossier, pas celle de la sélection
                    if id == cmd::COPY_ADDRESS {
                        app.tab_mut().clear_sel();
                    }
                    app.remote_command(id, None, cx)
                });
            }
        });
    }

    /// Un glisser-déposer au-dessus de la fenêtre. Sans fichiers : dit seulement si un dépôt irait vers un serveur
    /// (l'onglet affiché en montre un). Avec : les y envoie. Faux : le dépôt suit son cours habituel.
    pub(crate) fn remote_drop(&mut self, files: Option<&[PathBuf]>) -> bool {
        if self.remote_here().is_none() || self.sheet.is_some() {
            return false;
        }
        if let Some(files) = files {
            self.remote_upload(files.to_vec());
        }
        true
    }

    /// Clic dans la barre latérale ou le menu : la boîte « Se connecter à un serveur ».
    pub(super) fn ask_server(&mut self) {
        self.end_edit(true);
        self.sheet = Some(Sheet { kind: SheetKind::Connect, error: None, reveal: true });
        self.open_sheet_field();
    }

    /// Zone de la fenêtre touchée par un clic droit sur un serveur de la barre latérale : son menu.
    pub(super) fn remote_nav_menu(&mut self, server: String, at: (i32, i32), cx: &mut Cx) {
        let s = self.s;
        let loc = Location::Remote { server: server.clone(), path: String::new() };
        let mut m = Menu::new();
        m.item(cmd::OPEN, s.open).item(cmd::OPEN_TAB, s.open_new_tab).sep();
        if self.remote_connected(&server) {
            m.item(cmd::REFRESH, s.sftp_disconnect);
        }
        m.item(cmd::DELETE, s.sftp_forget);
        cx.after(move |win| {
            let picked = m.show(win, at, false);
            with_app(win, |app, _| match picked {
                Some(cmd::OPEN) => app.navigate(loc),
                Some(cmd::OPEN_TAB) => app.new_tab(loc, false),
                Some(cmd::REFRESH) => app.remote_disconnect(&server),
                Some(cmd::DELETE) => app.forget_server(&server),
                _ => {}
            });
        });
    }
}
