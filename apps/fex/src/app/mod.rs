//! L'explorateur : l'état de la fenêtre et ses réactions au clavier et à la souris.
//! Le dessin est dans `view`, le travail hors du fil de l'interface dans `load`.

mod actions;
pub mod format;
pub mod i18n;
pub mod load;
pub mod model;
pub mod preview;
pub mod remote;
mod search;
pub mod settings;
pub mod sheet;
pub mod thumbs;
pub mod undo;
mod view;

use crate::{
    shell::{
        self,
        icons::Icons,
        places::{Drive, Place},
    },
    ui::{Area, Button, Canvas, Color, Cursor, Cx, EditStyle, Event, Gfx, Handler, Hits, Mods, PaintCx, Pt, Rect, Theme, Window, m},
};
use fex_core::{
    SortKey,
    query::{Options, Order},
};
use format::Dates;
use i18n::Strings;
use load::{IconJob, ListOpts, LoadError, Loaded};
use model::{Hist, Load, Location, Restore, Tab, leaf, parse_address};
use settings::Settings;
use std::{
    any::Any,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// Hauteur d'une ligne de la liste, et marge au-dessus de la première.
pub const ROW_H: f32 = 28.0;
pub const LIST_TOP: f32 = 4.0;
pub const PAD: f32 = 8.0;

const TIMER_LASSO: usize = 1;
const TIMER_TOAST: usize = 2;
/// Vérifie régulièrement si le dossier affiché a changé sur le disque.
const TIMER_POLL: usize = 3;

// touches (codes de touches virtuelles de Windows)
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
const K_F3: u16 = 0x72;
const K_F4: u16 = 0x73;
const K_F5: u16 = 0x74;
const fn letter(c: char) -> u16 {
    c as u16
}

/// Les colonnes de la liste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Col {
    Name,
    Date,
    Type,
    Size,
    /// « Ce PC » : taille du lecteur et espace libre.
    Total,
    Free,
    /// Résultats de recherche : dossier de l'élément.
    Folder,
}

/// Ce que la souris peut toucher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Tab(usize),
    TabClose(usize),
    NewTab,
    Min,
    Max,
    Close,
    /// Une version plus récente de la suite : ouvrir Plume Suite, qui met tout à jour.
    Update,
    Back,
    Forward,
    Up,
    Refresh,
    More,
    Crumb(usize),
    Address,
    Search,
    SearchClear,
    Nav(usize),
    NavSplit,
    Header(Col),
    /// Bord gauche d'une colonne : on le tire pour changer sa largeur.
    ColSplit(Col),
    /// Recherche : dans le dossier de départ, ou partout.
    ScopeHere,
    ScopeAll,
    /// Recherche : type d'éléments, façon de comparer, export des résultats (un menu).
    Filter,
    /// Boîte de dialogue du mot de passe : ses boutons, son cadre, et le reste de la fenêtre assombri.
    SheetOk,
    SheetCancel,
    SheetReveal,
    SheetZip,
    Sheet7z,
    SheetRar,
    SheetCard,
    SheetDim,
    Row(usize),
    /// Zone vide de la liste.
    List,
    Thumb,
    Track,
}

/// Geste de souris en cours (bouton gauche enfoncé).
#[derive(Debug)]
pub enum Drag {
    None,
    /// Poignée de la barre de défilement, attrapée à `grab` de son bord haut.
    Thumb {
        grab: f32,
    },
    NavSplit,
    /// Bord d'une colonne attrapé en `x`, alors qu'elle faisait `w` de large.
    Col {
        col: Col,
        x: f32,
        w: f32,
    },
    /// Bouton enfoncé hors du nom d'un fichier : un rectangle de sélection commencera si la souris bouge.
    LassoWait {
        start: Pt,
        ctrl: bool,
    },
    /// Rectangle de sélection, depuis le point (x0, y0) du contenu de la liste.
    Lasso {
        x0: f32,
        y0: f32,
        ctrl: bool,
        base: Vec<bool>,
    },
    /// Bouton enfoncé sur le nom d'un fichier : un glisser-déposer commencera si la souris bouge.
    Item {
        start: Pt,
        row: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum EditKind {
    Address,
    Search,
    /// Renommage de l'élément `name` de l'onglet `tab`.
    Rename {
        tab: u64,
        name: String,
    },
    /// Mot de passe d'une archive, dans la boîte de dialogue.
    Password,
}

/// Une ligne de la barre latérale.
#[derive(Clone, Debug)]
pub struct NavRow {
    pub loc: Location,
    pub label: String,
    /// Icône de Windows (-1 : icône vectorielle).
    pub icon: i32,
    pub indent: f32,
    /// Espace au-dessus de la ligne (début d'un groupe).
    pub gap: f32,
}

/// Les rectangles de la dernière image, pour comprendre les gestes de la souris.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lay {
    pub w: f32,
    /// Volet d'aperçu (vide quand il est fermé).
    pub preview: Rect,
    pub nav: Rect,
    pub header: Rect,
    pub list: Rect,
    pub status: Rect,
    pub addr: Rect,
    pub addr_text: Rect,
    pub search: Rect,
    pub search_text: Rect,
    pub thumb: Rect,
    pub track: Rect,
    pub max_scroll: f32,
    /// Bandeau « Rechercher dans » au-dessus des résultats.
    pub scope: Rect,
}

pub struct App {
    pub win: Window,
    pub theme: Theme,
    pub s: &'static Strings,
    /// Version plus récente de la suite, si elle existe.
    pub update: Option<String>,
    pub preview: preview::Preview,
    /// Ce que Ctrl+Z peut défaire, le plus récent à la fin.
    pub undo: Vec<undo::Undo>,
    pub thumbs: thumbs::Thumbs,
    pub set: Settings,
    pub icons: Icons,
    pub dates: Dates,
    pub tabs: Vec<Tab>,
    pub cur: usize,
    next_id: u64,
    pub places: Vec<Place>,
    pub drives: Vec<Drive>,
    pub nav_rows: Vec<NavRow>,
    pub hits: Hits<Id>,
    pub hover: Option<Id>,
    pub pressed: Option<Id>,
    pub mouse: Pt,
    pub lay: Lay,
    pub edit: Option<EditKind>,
    /// Rectangle du champ de saisie ouvert (pour le replacer si la fenêtre change).
    edit_rect: Rect,
    pub drag: Drag,
    /// Ligne déjà sélectionnée sur laquelle on a cliqué : au relâchement, elle reste seule sélectionnée.
    collapse_on_up: Option<usize>,
    pub active: bool,
    last_frame: Instant,
    icon_tx: mpsc::Sender<IconJob>,
    pub toast: Option<(String, Instant)>,
    /// Bord droit du nom de chaque ligne affichée, à partir de `first_row`.
    name_right: Vec<f32>,
    first_row: usize,
    /// Chemins des éléments du fil d'Ariane affichés.
    crumbs: Vec<Location>,
    wheel_lines: f32,
    /// Le clavier a servi en dernier : la ligne active montre son contour.
    keyboard: bool,
    /// Ce qui recevrait les fichiers en train d'être glissés au-dessus de la fenêtre.
    pub drop_hl: Option<Id>,
    /// L'index de tous les disques, pour la recherche instantanée.
    pub ix: Arc<search::SearchIndex>,
    /// Travail de fond en cours (extraction, compression) : son libellé et son début ; et combien sont en cours.
    pub busy: Option<(String, Instant)>,
    /// Transfert en cours avec un serveur : son avancement.
    pub transfer: Option<std::sync::Arc<remote::Transfer>>,
    jobs: usize,
    /// Boîte de dialogue ouverte (mot de passe d'une archive).
    pub sheet: Option<sheet::Sheet>,
    /// Réglages du bandeau de recherche ; ils valent pour tous les onglets, le temps de la session.
    pub search_opts: Options,
    /// Version de l'index pour laquelle les tailles de dossiers affichées ont été demandées.
    sizes_seen: u64,
    /// Les serveurs SFTP auxquels la fenêtre est reliée.
    remotes: remote::Remotes,
}

/// L'ordre des résultats de recherche pour la colonne de tri choisie.
fn search_order(key: SortKey) -> Order {
    match key {
        SortKey::Size => Order::Size,
        SortKey::Modified => Order::Modified,
        SortKey::Path => Order::Path,
        _ => Order::Name,
    }
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, set: Settings, start: Location) -> App {
        let s = i18n::strings(set.language());
        let theme = Theme::new(set.dark(), set.accent);
        load::spawn_places(win.sender(), set.pinned.clone());
        let mut app = App {
            update: None,
            preview: preview::Preview::new(gfx.clone()),
            undo: Vec::new(),
            transfer: None,
            thumbs: Default::default(),
            win,
            theme,
            s,
            icons: Icons::new(gfx),
            dates: Dates::default(),
            tabs: vec![],
            cur: 0,
            next_id: 1,
            places: vec![],
            drives: vec![],
            nav_rows: vec![],
            hits: Hits::default(),
            hover: None,
            pressed: None,
            mouse: Pt::default(),
            lay: Lay::default(),
            edit: None,
            edit_rect: Rect::default(),
            drag: Drag::None,
            collapse_on_up: None,
            active: true,
            last_frame: Instant::now(),
            icon_tx: load::icon_worker(win.sender()),
            toast: None,
            name_right: vec![],
            first_row: 0,
            crumbs: vec![],
            wheel_lines: 3.0,
            keyboard: false,
            drop_hl: None,
            ix: search::start(win.sender(), set.dir.clone()),
            busy: None,
            jobs: 0,
            sheet: None,
            search_opts: Options::default(),
            sizes_seen: 0,
            remotes: remote::Remotes::default(),
            set,
        };
        app.rebuild_nav();
        app.new_tab(start, true);
        app.watch_updates();
        // si le programme a changé de place depuis son inscription dans Windows, les commandes inscrites suivent
        if let (Some(classes), Ok(exe)) = (shell::integration::Classes::user(), std::env::current_exe()) {
            classes.heal(&exe, app.s.open_in_fex);
        }
        win.timer(TIMER_POLL, Some(500));
        app
    }

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.cur]
    }

    pub fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.cur]
    }

    // ───────────────────────── lieux et titres ─────────────────────────

    /// Nom affiché d'un dossier : celui de Windows pour les lecteurs et les dossiers personnels.
    pub fn label_of(&self, p: &Path) -> String {
        if p.parent().is_none()
            && let Some(d) = self.drives.iter().find(|d| d.root == p)
        {
            return d.label.clone();
        }
        if let Some(pl) = self.places.iter().find(|pl| pl.path == p) {
            return pl.name.clone();
        }
        leaf(p)
    }

    pub fn title_of(&self, loc: &Location) -> String {
        match loc {
            Location::ThisPc => self.s.this_pc.to_string(),
            Location::Path(p) => self.label_of(p),
            Location::Search { query, .. } => format!("{query} - {}", self.s.search_results),
            Location::Remote { server, path } => match path.trim_end_matches('/').rsplit('/').next() {
                Some(leaf) if !leaf.is_empty() => format!("{leaf} - {server}"),
                _ => server.clone(),
            },
        }
    }

    fn update_title(&self) {
        self.win.set_title(&format!("{} - Plume FEx", self.title_of(&self.tab().loc)));
    }

    fn rebuild_nav(&mut self) {
        let mut rows: Vec<NavRow> =
            self.places.iter().map(|p| NavRow { loc: Location::Path(p.path.clone()), label: p.name.clone(), icon: p.icon, indent: 0.0, gap: 0.0 }).collect();
        rows.push(NavRow { loc: Location::ThisPc, label: self.s.this_pc.to_string(), icon: -1, indent: 0.0, gap: if rows.is_empty() { 0.0 } else { 12.0 } });
        rows.extend(self.drives.iter().map(|d| NavRow { loc: Location::Path(d.root.clone()), label: d.label.clone(), icon: d.icon, indent: 14.0, gap: 0.0 }));
        // les serveurs SFTP déjà visités
        rows.extend(self.set.servers.iter().enumerate().map(|(k, s)| NavRow {
            loc: Location::Remote { server: s.clone(), path: String::new() },
            label: s.clone(),
            icon: -1,
            indent: 0.0,
            gap: if k == 0 { 12.0 } else { 0.0 },
        }));
        self.nav_rows = rows;
    }

    pub fn on_places(&mut self, places: Vec<Place>, drives: Vec<Drive>, cx: &mut Cx) {
        self.places = places;
        self.drives = drives;
        self.rebuild_nav();
        self.update_title();
        cx.repaint();
    }

    // ───────────────────────── navigation ─────────────────────────

    fn list_opts(&self, tab: &Tab) -> ListOpts {
        ListOpts { show_hidden: self.set.show_hidden, show_system: self.set.show_system, sort: tab.sort, asc: tab.asc }
    }

    /// (Re)lit le contenu de l'onglet `i` sur un autre fil.
    fn load(&mut self, i: usize) {
        self.load_with(i, false);
    }

    /// `silent` : vérification de fond. L'écran ne change que si le contenu du dossier a changé,
    /// et la sélection, la ligne active et le défilement sont conservés.
    fn load_with(&mut self, i: usize, silent: bool) {
        // un dossier de serveur ne se relit que sur demande : pas de vérification de fond à travers le réseau
        if matches!(self.tabs[i].loc, Location::Remote { .. }) {
            if !silent {
                self.remote_load(i);
            }
            return;
        }
        // un onglet de recherche se « relit » en relançant sa recherche
        if matches!(self.tabs[i].loc, Location::Search { .. }) {
            if !silent {
                self.run_search(i);
            }
            return;
        }
        let opts = self.list_opts(&self.tabs[i]);
        let sender = self.win.sender();
        let tab = &mut self.tabs[i];
        tab.cancel.store(true, Ordering::Relaxed);
        tab.cancel = Arc::new(AtomicBool::new(false));
        tab.generation += 1;
        tab.checked = Instant::now();
        if !silent {
            tab.load = Load::Loading(Instant::now());
        }
        load::spawn_list(sender, tab.id, tab.generation, tab.loc.clone(), opts, tab.cancel.clone(), silent.then_some(tab.sig));
        if !silent {
            self.win.repaint();
        }
    }

    /// Vérifie tout de suite si le dossier affiché a changé (retour sur la fenêtre, fin d'une opération).
    pub fn check_now(&mut self) {
        let tab = self.tab();
        if !tab.stale && !matches!(tab.load, Load::Loading(_)) && !matches!(tab.loc, Location::Search { .. }) {
            self.load_with(self.cur, true);
        }
    }

    /// Minuteur : le dossier affiché est relu de temps en temps, d'autant moins souvent qu'il est long à lire.
    /// Aucun handle n'est gardé sur le dossier : on peut le renommer, le supprimer ou éjecter le lecteur.
    fn poll(&mut self) {
        if !self.ix.is_ready() && !self.ix.sleeping() {
            // l'avancement de l'indexation s'affiche dans la barre d'état
            self.win.repaint();
        }
        // tant qu'on cherche (résultats affichés, champ de recherche ouvert), l'index reste en mémoire
        if matches!(self.tab().loc, Location::Search { .. }) || self.edit == Some(EditKind::Search) {
            self.ix.touch();
        }
        if self.win.is_minimized() || !matches!(self.drag, Drag::None) {
            return;
        }
        // des fichiers ont changé sur le disque depuis que ces résultats ont été calculés : on les rafraîchit
        let tab = self.tab();
        if matches!(tab.loc, Location::Search { .. })
            && tab.load == Load::Ready
            && tab.index_version != self.ix.version.load(Ordering::Relaxed)
            && !matches!(self.edit, Some(EditKind::Rename { .. }))
        {
            let snap = tab.snapshot();
            self.tab_mut().restore = Some(snap);
            self.run_search(self.cur);
            return;
        }
        // l'index a changé : les tailles de dossiers affichées sont recalculées (sans le réveiller s'il dort)
        if self.set.folder_sizes && self.sizes_seen != self.ix.version.load(Ordering::Relaxed) {
            self.ask_folder_sizes(self.cur, false);
        }
        let tab = self.tab();
        let every = (tab.took_ms * 100.0).clamp(if self.active { 500.0 } else { 2000.0 }, 60_000.0);
        if tab.checked.elapsed().as_secs_f32() * 1000.0 >= every {
            self.check_now();
        }
    }

    /// Le dossier d'un onglet a disparu : on remonte au premier dossier parent qui existe encore.
    fn leave_missing(&mut self, i: usize) {
        let mut loc = self.tabs[i].loc.clone();
        while let Some(parent) = loc.parent() {
            loc = parent;
            if loc.path().is_none_or(|p| p.is_dir()) {
                break;
            }
        }
        let tab = &mut self.tabs[i];
        tab.loc = loc;
        tab.stale = true;
        tab.filter.clear();
        tab.restore = None;
        self.load(i);
        if i == self.cur {
            self.update_title();
        }
    }

    fn hist(tab: &Tab) -> Hist {
        Hist { loc: tab.loc.clone(), scroll: tab.target, focus: tab.focus.filter(|&r| r < tab.rows() && !tab.stale).map(|r| tab.item(r).name.clone()) }
    }

    fn go(&mut self, loc: Location, restore: Option<Restore>, push: bool) {
        self.end_edit(false);
        let i = self.cur;
        let tab = &mut self.tabs[i];
        let same = tab.loc == loc;
        if push && !same {
            let h = Self::hist(tab);
            tab.back.push(h);
            tab.fwd.clear();
            if tab.back.len() > 200 {
                tab.back.remove(0);
            }
        }
        if !same {
            tab.filter.clear();
            tab.stale = true;
            tab.typed.clear();
        }
        if let Location::Search { query, .. } = &loc {
            tab.filter = query.clone();
        }
        tab.loc = loc;
        tab.restore = restore;
        self.load(i);
        self.update_title();
    }

    // ───────────────────────── recherche ─────────────────────────

    /// Lance (ou relance) la recherche de l'onglet `i` dans l'index.
    fn run_search(&mut self, i: usize) {
        let (everywhere, options) = (self.set.search_everywhere, self.search_opts);
        let (shared, sender) = (self.ix.clone(), self.win.sender());
        let tab = &mut self.tabs[i];
        let Location::Search { query, from } = &tab.loc else { return };
        tab.cancel.store(true, Ordering::Relaxed);
        tab.cancel = Arc::new(AtomicBool::new(false));
        tab.generation += 1;
        tab.load = Load::Loading(Instant::now());
        let (key, asc) = tab.search_sort;
        let req = search::Request {
            tab: tab.id,
            generation: tab.generation,
            query: query.clone(),
            scope: if everywhere { None } else { from.clone() },
            order: search_order(key),
            descending: !asc,
            cancel: tab.cancel.clone(),
            options,
        };
        search::spawn(shared, sender, req);
        self.win.repaint();
    }

    pub fn on_search(&mut self, r: search::Results, cx: &mut Cx) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == r.tab) else { return };
        if tab.generation != r.generation || !matches!(tab.loc, Location::Search { .. }) {
            return;
        }
        tab.items = r.items;
        tab.shown = None;
        tab.stale = false;
        tab.load = Load::Ready;
        tab.total = r.total;
        tab.took_ms = r.took_ms;
        tab.index_version = r.version;
        tab.focus = None;
        tab.anchor = None;
        tab.target = 0.0;
        tab.scroll = 0.0;
        // résultats rafraîchis (l'index a changé, ou un renommage vient d'avoir lieu) : la sélection et le
        // défilement sont conservés
        if let Some(restore) = tab.restore.take() {
            tab.items.iter_mut().for_each(|it| it.sel = restore.select.contains(&it.name));
            tab.focus = restore.focus.as_deref().and_then(|n| tab.row_of(n));
            tab.anchor = tab.focus;
            if let Some(scroll) = restore.scroll {
                tab.target = scroll;
                tab.scroll = scroll;
            }
        }
        tab.recount();
        cx.repaint();
    }

    /// L'index vient d'être construit : une recherche déjà tapée passe du filtre du dossier à l'index.
    pub fn on_index_ready(&mut self, cx: &mut Cx) {
        let tab = self.tab_mut();
        if !tab.filter.trim().is_empty() && !matches!(tab.loc, Location::Search { .. }) {
            let text = std::mem::take(&mut tab.filter);
            self.set_filter(text);
        }
        cx.repaint();
    }

    /// Quitte les résultats de recherche pour revenir au dossier d'où elle a été lancée.
    fn leave_search(&mut self) {
        let Location::Search { from, .. } = self.tab().loc.clone() else { return };
        let origin = from.map_or(Location::ThisPc, Location::Path);
        // l'étape d'historique ajoutée en entrant dans la recherche n'a plus lieu d'être
        let tab = self.tab_mut();
        let restore = if tab.back.last().is_some_and(|h| h.loc == origin) { tab.back.pop().map(|h| Self::restore_of(&h)) } else { None };
        self.go(origin, restore, false);
    }

    fn set_scope(&mut self, everywhere: bool) {
        if self.set.search_everywhere != everywhere {
            self.set.search_everywhere = everywhere;
            self.set.save();
            self.run_search(self.cur);
        }
    }

    /// Ouvre un lieu dans l'onglet actif.
    pub fn navigate(&mut self, loc: Location) {
        self.go(loc, None, true);
    }

    fn restore_of(h: &Hist) -> Restore {
        Restore { select: h.focus.clone().into_iter().collect(), focus: h.focus.clone(), scroll: Some(h.scroll), rename: false }
    }

    pub fn back(&mut self) {
        let tab = self.tab_mut();
        let Some(h) = tab.back.pop() else { return };
        let here = Self::hist(tab);
        tab.fwd.push(here);
        let r = Self::restore_of(&h);
        self.go(h.loc, Some(r), false);
    }

    pub fn forward(&mut self) {
        let tab = self.tab_mut();
        let Some(h) = tab.fwd.pop() else { return };
        let here = Self::hist(tab);
        tab.back.push(here);
        let r = Self::restore_of(&h);
        self.go(h.loc, Some(r), false);
    }

    /// Remonte au dossier parent, en y sélectionnant le dossier que l'on quitte.
    pub fn up(&mut self) {
        let loc = self.tab().loc.clone();
        let Some(parent) = loc.parent() else { return };
        let from = match &loc {
            Location::Path(p) if p.parent().is_none() => self.label_of(p),
            Location::Path(p) => leaf(p),
            Location::Remote { path, .. } => path.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string(),
            _ => String::new(),
        };
        let restore = (!from.is_empty()).then(|| Restore { select: vec![from.clone()], focus: Some(from), scroll: None, rename: false });
        self.go(parent, restore, true);
    }

    pub fn refresh(&mut self) {
        let loc = self.tab().loc.clone();
        let snap = self.tab().snapshot();
        self.go(loc, Some(snap), false);
        load::spawn_places(self.win.sender(), self.set.pinned.clone());
    }

    pub fn on_loaded(&mut self, l: Loaded, cx: &mut Cx) {
        let Some(i) = self.tabs.iter().position(|t| t.id == l.tab) else { return };
        let s = self.s;
        let tab = &mut self.tabs[i];
        if tab.generation != l.generation {
            return;
        }
        tab.checked = Instant::now();
        if l.silent {
            match &l.result {
                Ok(listing) if listing.unchanged => return,
                Err(LoadError::NotFound) => {
                    self.leave_missing(i);
                    return;
                }
                _ => {}
            }
            // l'utilisateur est en plein geste ou renomme un fichier : la prochaine vérification rattrapera
            let busy = i == self.cur && (!matches!(self.drag, Drag::None) || matches!(self.edit, Some(EditKind::Rename { .. })));
            if busy {
                return;
            }
            let tab = &mut self.tabs[i];
            tab.restore = Some(tab.snapshot());
        }
        let tab = &mut self.tabs[i];
        tab.took_ms = l.took_ms;
        tab.stale = false;
        tab.focus = None;
        tab.anchor = None;
        tab.shown = None;
        let restore = tab.restore.take().unwrap_or_default();
        match l.result {
            Ok(listing) => {
                tab.sig = listing.sig;
                let mut items = listing.items;
                if !restore.select.is_empty() {
                    let wanted: std::collections::HashSet<&str> = restore.select.iter().map(String::as_str).collect();
                    items.iter_mut().for_each(|it| it.sel = wanted.contains(it.name.as_str()));
                }
                // même dossier relu : les tailles de dossiers déjà calculées restent affichées en attendant les nouvelles
                if l.silent {
                    let known: std::collections::HashMap<&str, u64> = tab.items.iter().filter(|it| it.sized).map(|it| (it.name.as_str(), it.size)).collect();
                    for it in items.iter_mut().filter(|it| it.is_dir()) {
                        if let Some(&size) = known.get(it.name.as_str()) {
                            (it.size, it.sized) = (size, true);
                        }
                    }
                }
                tab.items = items;
                tab.load = Load::Ready;
                tab.apply_filter();
                tab.focus = restore.focus.as_deref().and_then(|n| tab.row_of(n));
                tab.anchor = tab.focus;
                let (scroll, focus) = (restore.scroll, tab.focus);
                tab.target = scroll.unwrap_or(0.0);
                tab.scroll = tab.target;
                if i == self.cur
                    && scroll.is_none()
                    && let Some(f) = focus
                {
                    self.reveal(f);
                }
                // nouveau dossier : son nom s'ouvre tout de suite à la saisie
                if restore.rename && i == self.cur && focus.is_some() {
                    self.start_rename();
                }
            }
            Err(e) => {
                tab.items.clear();
                tab.sig = 0;
                tab.sel_count = 0;
                tab.scroll = 0.0;
                tab.target = 0.0;
                tab.load = Load::Failed(match e {
                    LoadError::Denied => s.denied.to_string(),
                    LoadError::NotFound => s.gone.to_string(),
                    LoadError::Other(msg) => format!("{} {msg}", s.cant_open),
                });
            }
        }
        self.ask_folder_sizes(i, true);
        cx.repaint();
    }

    /// Demande à l'index la taille des sous-dossiers du dossier de l'onglet `i` (si le réglage est activé).
    /// `wake` : réveiller l'index s'il dort ; sinon la demande n'aboutit que s'il est déjà en mémoire.
    pub(super) fn ask_folder_sizes(&mut self, i: usize, wake: bool) {
        let tab = &self.tabs[i];
        let Location::Path(dir) = &tab.loc else { return };
        if !self.set.folder_sizes || tab.load != Load::Ready || !tab.items.iter().any(|it| it.is_dir()) {
            return;
        }
        self.sizes_seen = self.ix.version.load(Ordering::Relaxed);
        let (shared, sender, dir, id, generation) = (self.ix.clone(), self.win.sender(), dir.clone(), tab.id, tab.generation);
        std::thread::spawn(move || {
            if let Some(sizes) = search::folder_sizes(&shared, &dir, wake) {
                sender.send(move |h, cx| {
                    if let Some(app) = h.as_any().downcast_mut::<App>() {
                        app.on_folder_sizes(id, generation, sizes, cx);
                    }
                });
            }
        });
    }

    fn on_folder_sizes(&mut self, tab: u64, generation: u64, sizes: Vec<(String, u64)>, cx: &mut Cx) {
        let on = self.set.folder_sizes;
        let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab && t.generation == generation) else { return };
        let by_name: std::collections::HashMap<&str, u64> = sizes.iter().map(|(n, s)| (n.as_str(), *s)).collect();
        let mut changed = false;
        for it in t.items.iter_mut().filter(|it| it.is_dir()) {
            if let Some(&size) = by_name.get(it.name.as_str()).filter(|_| on) {
                changed |= !it.sized || it.size != size;
                (it.size, it.sized) = (size, true);
            }
        }
        if changed {
            // triés par taille, les dossiers prennent leur place
            if t.sort == SortKey::Size {
                t.resort();
            }
            cx.repaint();
        }
    }

    /// Active ou coupe l'affichage de la taille des dossiers.
    pub(super) fn set_folder_sizes(&mut self, on: bool) {
        self.set.folder_sizes = on;
        self.set.save();
        if on {
            self.ask_folder_sizes(self.cur, true);
        } else {
            for t in &mut self.tabs {
                let had = t.items.iter().any(|it| it.sized);
                t.items.iter_mut().filter(|it| it.sized).for_each(|it| (it.size, it.sized) = (0, false));
                if had && t.sort == SortKey::Size {
                    t.resort();
                }
            }
        }
        self.win.repaint();
    }

    /// Largeur d'une colonne que l'on peut régler (None : colonne à largeur fixe).
    fn col_width(&mut self, col: Col) -> Option<&mut f32> {
        match col {
            Col::Date => Some(&mut self.set.cols[0]),
            Col::Type => Some(&mut self.set.cols[1]),
            Col::Size => Some(&mut self.set.cols[2]),
            Col::Folder => Some(&mut self.set.col_folder),
            _ => None,
        }
    }

    pub fn on_icons(&mut self, tab: u64, generation: u64, found: Vec<(String, i32)>, cx: &mut Cx) {
        let Some(t) = self.tabs.iter().find(|t| t.id == tab && t.generation == generation) else { return };
        // les lignes demandées sont celles affichées : on les cherche d'abord autour de la première ligne visible
        let mut by_name: std::collections::HashMap<&str, i32> = found.iter().map(|(n, i)| (n.as_str(), *i)).collect();
        for it in &t.items {
            if by_name.is_empty() {
                break;
            }
            if it.asked.get()
                && it.icon.get() < 0
                && let Some(i) = by_name.remove(it.name.as_str())
            {
                it.icon.set(i);
            }
        }
        cx.repaint();
    }

    /// Demande les icônes exactes de quelques lignes (appelé par le dessin).
    fn ask_icons(&self, paths: Vec<(String, PathBuf)>) {
        if !paths.is_empty() {
            let t = self.tab();
            let _ = self.icon_tx.send(IconJob { tab: t.id, generation: t.generation, paths });
        }
    }

    // ───────────────────────── onglets ─────────────────────────

    pub fn new_tab(&mut self, loc: Location, activate: bool) {
        self.end_edit(false);
        let id = self.next_id;
        self.next_id += 1;
        let at = if self.tabs.is_empty() { 0 } else { self.cur + 1 };
        self.tabs.insert(at, Tab::new(id, loc));
        if activate || self.tabs.len() == 1 {
            self.cur = at;
            self.update_title();
        }
        self.load(at);
    }

    pub fn close_tab(&mut self, i: usize, cx: &mut Cx) {
        if i >= self.tabs.len() {
            return;
        }
        self.end_edit(false);
        self.tabs[i].cancel.store(true, Ordering::Relaxed);
        if self.tabs.len() == 1 {
            cx.after(|w| w.close());
            return;
        }
        self.tabs.remove(i);
        if self.cur > i || self.cur >= self.tabs.len() {
            self.cur = self.cur.saturating_sub(1).min(self.tabs.len() - 1);
        }
        self.update_title();
        cx.repaint();
    }

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

    pub fn activate_tab(&mut self, i: usize) {
        if i < self.tabs.len() && i != self.cur {
            self.end_edit(false);
            self.cur = i;
            self.update_title();
            // un onglet caché n'est pas surveillé : il se remet à jour quand on y revient
            self.check_now();
            self.win.repaint();
        }
    }

    // ───────────────────────── liste : défilement, clavier ─────────────────────────

    /// Fait défiler la liste juste assez pour montrer la ligne `row`.
    pub fn reveal(&mut self, row: usize) {
        let h = self.lay.list.h;
        let (cols, lh) = (self.grid_cols(), self.line_h());
        let tab = self.tab_mut();
        let y = (row / cols) as f32 * lh;
        if y < tab.target {
            tab.target = y;
        } else if h > 0.0 && y + lh + LIST_TOP + 10.0 > tab.target + h {
            tab.target = y + lh + LIST_TOP + 10.0 - h;
        }
        tab.target = tab.target.max(0.0);
        tab.scroll = tab.target;
    }

    fn rows_per_page(&self) -> usize {
        (((self.lay.list.h / self.line_h()).floor() as usize).max(2) - 1) * self.grid_cols()
    }

    fn move_focus(&mut self, vk: u16, mods: Mods) {
        let page = self.rows_per_page();
        let cols = self.grid_cols();
        let tab = self.tab_mut();
        let rows = tab.rows();
        if rows == 0 || tab.stale {
            return;
        }
        let cur = tab.focus.filter(|&f| f < rows);
        let new = match vk {
            // en vignettes, haut et bas changent de ligne de la grille, gauche et droite de case
            K_UP => cur.map_or(0, |f| f.saturating_sub(cols)),
            K_DOWN => cur.map_or(0, |f| (f + cols).min(rows - 1)),
            K_LEFT => cur.map_or(0, |f| f.saturating_sub(1)),
            K_RIGHT => cur.map_or(0, |f| (f + 1).min(rows - 1)),
            K_HOME => 0,
            K_END => rows - 1,
            K_PRIOR => cur.map_or(0, |f| f.saturating_sub(page)),
            _ => cur.map_or(0, |f| (f + page).min(rows - 1)),
        };
        if mods.shift {
            let a = tab.anchor.or(cur).unwrap_or(new).min(rows - 1);
            tab.select_range(a, new, false);
            tab.anchor = Some(a);
            tab.focus = Some(new);
        } else if mods.ctrl {
            tab.focus = Some(new);
        } else {
            tab.select_only(new);
        }
        self.keyboard = true;
        self.reveal(new);
    }

    /// Saisie dans la liste : saute au premier fichier dont le nom commence par ce qui vient d'être tapé.
    fn type_ahead(&mut self, ch: char) {
        if self.sheet.is_some() {
            return;
        }
        let tab = self.tab_mut();
        if tab.rows() == 0 || tab.stale {
            return;
        }
        if tab.typed_at.elapsed() > Duration::from_millis(1000) {
            tab.typed.clear();
        }
        if ch == ' ' && tab.typed.is_empty() {
            return;
        }
        tab.typed.push(ch);
        tab.typed_at = Instant::now();
        let focus = tab.focus.unwrap_or(0).min(tab.rows() - 1);
        let mut chars = tab.typed.chars();
        let first = chars.next().unwrap_or(ch);
        // la même lettre répétée passe d'un fichier au suivant
        let same_letter = tab.typed.chars().count() > 1 && chars.all(|c| c == first);
        let found = if same_letter || tab.typed.chars().count() == 1 {
            let start = if tab.focus.is_some() { focus + 1 } else { 0 };
            tab.find_prefix(start, &first.to_string())
        } else {
            tab.find_prefix(focus, &tab.typed.clone())
        };
        if let Some(r) = found {
            tab.select_only(r);
            self.keyboard = true;
            self.reveal(r);
        }
        self.win.repaint();
    }

    // ───────────────────────── ouvrir ─────────────────────────

    /// Ouvre l'élément d'une ligne : entre dans un dossier, lance un fichier avec son application.
    pub fn open_row(&mut self, row: usize, new_tab: bool, cx: &mut Cx) {
        let tab = self.tab();
        if row >= tab.rows() || tab.stale {
            return;
        }
        if matches!(tab.loc, Location::Remote { .. }) {
            return self.remote_open(row, new_tab);
        }
        let it = tab.item(row);
        let Some(path) = tab.path_of(row) else { return };
        if it.is_dir() {
            if new_tab {
                self.new_tab(Location::Path(path), false);
            } else {
                self.navigate(Location::Path(path));
            }
            return;
        }
        let shortcut = it.ext().eq_ignore_ascii_case("lnk");
        cx.after(move |win| {
            // un raccourci vers un dossier s'ouvre ici même, comme dans l'Explorateur
            if shortcut
                && let Some(target) = shell::link_target(&path)
                && target.is_dir()
            {
                win.with(|h, _| {
                    if let Some(app) = h.as_any().downcast_mut::<App>() {
                        if new_tab { app.new_tab(Location::Path(target), false) } else { app.navigate(Location::Path(target)) }
                    }
                });
                return;
            }
            shell::open(win.hwnd, &path);
        });
    }

    fn open_selection(&mut self, new_tab: bool, cx: &mut Cx) {
        let tab = self.tab();
        let rows = tab.selected_rows();
        match rows.as_slice() {
            [] => {
                if let Some(f) = tab.focus {
                    self.open_row(f, new_tab, cx);
                }
            }
            [one] => self.open_row(*one, new_tab, cx),
            many => {
                // plusieurs éléments : les dossiers s'ouvrent dans des onglets, les fichiers avec leur application
                for &r in many.iter().take(20) {
                    let dir = self.tab().item(r).is_dir();
                    self.open_row(r, dir, cx);
                }
            }
        }
    }

    /// Message bref au bas de la fenêtre.
    pub fn toast(&mut self, text: String) {
        self.toast = Some((text, Instant::now()));
        self.win.timer(TIMER_TOAST, Some(2600));
        self.win.repaint();
    }

    // ───────────────────────── champs de saisie ─────────────────────────

    fn edit_style(&self, bg: Color, select: Option<(usize, usize)>, placeholder: &str) -> EditStyle {
        EditStyle { fg: self.theme.t1, bg, size: 13.0, select, placeholder: placeholder.to_string(), password: false, multiline: false }
    }

    /// Couleur opaque d'un champ de la barre d'outils (remplissage translucide sur l'en-tête).
    pub fn field_bg(&self) -> Color {
        self.theme.fill.over(self.theme.hdr2)
    }

    pub fn start_address_edit(&mut self) {
        self.end_edit(true);
        let text = match &self.tab().loc {
            Location::ThisPc => self.s.this_pc.to_string(),
            Location::Path(p) => p.display().to_string(),
            Location::Search { query, .. } => query.clone(),
            Location::Remote { server, path } => Location::remote_url(server, path),
        };
        self.edit = Some(EditKind::Address);
        self.edit_rect = self.lay.addr_text;
        let n = text.encode_utf16().count();
        self.win.edit_show(self.edit_rect, &text, &self.edit_style(self.field_bg(), Some((0, n)), ""));
        self.win.repaint();
    }

    pub fn start_search_edit(&mut self) {
        if self.lay.search.w <= 0.0 {
            return;
        }
        // on va chercher : si l'index dormait, il a le temps de revenir pendant la frappe
        self.ix.touch();
        self.end_edit(true);
        let text = self.tab().filter.clone();
        self.edit = Some(EditKind::Search);
        self.edit_rect = self.lay.search_text;
        let n = text.encode_utf16().count();
        let hint = self.search_hint();
        self.win.edit_show(self.edit_rect, &text, &self.edit_style(self.field_bg(), Some((0, n)), &hint));
        self.win.repaint();
    }

    /// Texte grisé du champ de recherche : « Rechercher dans Documents ».
    pub fn search_hint(&self) -> String {
        match &self.tab().loc {
            Location::Path(p) => self.s.fill(self.s.search_in, &[&self.label_of(p)]),
            _ => self.s.search.to_string(),
        }
    }

    /// Ferme le champ de saisie ouvert. `commit` : un renommage en cours est appliqué (sinon abandonné).
    pub fn end_edit(&mut self, commit: bool) {
        let Some(kind) = self.edit.take() else { return };
        let text = self.win.edit_text().unwrap_or_default();
        self.win.edit_hide();
        if kind == EditKind::Password {
            // le champ du mot de passe ne va pas sans sa boîte de dialogue
            self.sheet = None;
        }
        if let EditKind::Rename { tab, name } = kind
            && commit
        {
            self.commit_rename(tab, &name, text.trim());
        }
        self.win.repaint();
    }

    fn commit_address(&mut self, text: &str, cx: &mut Cx) {
        let t = text.trim().trim_matches('"').trim();
        if t.is_empty() {
            return;
        }
        let lower = t.to_lowercase();
        if lower == self.s.this_pc.to_lowercase() || lower == "this pc" || lower == "ce pc" {
            self.navigate(Location::ThisPc);
            return;
        }
        // « sftp://utilisateur@hôte/dossier » : un serveur, parcouru ici même
        if let Some(loc) = Location::parse_remote(t) {
            self.navigate(loc);
            return;
        }
        if t.contains("://") || lower.starts_with("mailto:") {
            let (hwnd, url) = (self.win.hwnd, t.to_string());
            cx.after(move |_| {
                shell::open_text(hwnd, &url);
            });
            return;
        }
        let base = self.tab().loc.path().map(Path::to_path_buf);
        match parse_address(t, base.as_deref()) {
            Some(p) if p.is_dir() => self.navigate(Location::Path(p)),
            Some(p) if p.exists() => {
                // un fichier : on ouvre son dossier en le sélectionnant
                let name = leaf(&p);
                let parent = p.parent().map_or(Location::ThisPc, |q| Location::Path(q.to_path_buf()));
                self.go(parent, Some(Restore { select: vec![name.clone()], focus: Some(name), scroll: None, rename: false }), true);
            }
            _ => {
                let msg = self.s.fill(self.s.not_found, &[t]);
                self.toast(msg);
            }
        }
    }

    /// Le texte du champ de recherche a changé. Index prêt : recherche instantanée dans le dossier ouvert et ses
    /// sous-dossiers (ou partout). Sinon, en attendant : simple filtre sur le contenu du dossier.
    fn set_filter(&mut self, text: String) {
        let i = self.cur;
        if self.tabs[i].filter == text {
            return;
        }
        let searching = matches!(self.tabs[i].loc, Location::Search { .. });
        if text.trim().is_empty() && searching {
            self.tabs[i].filter = text;
            self.leave_search();
        } else if !text.trim().is_empty() && self.ix.is_ready() && !matches!(self.tabs[i].loc, Location::Remote { .. }) {
            let tab = &mut self.tabs[i];
            match &mut tab.loc {
                Location::Search { query, .. } => *query = text.clone(),
                loc => {
                    let from = loc.path().map(Path::to_path_buf);
                    let h = Self::hist(tab);
                    tab.back.push(h);
                    tab.fwd.clear();
                    tab.loc = Location::Search { query: text.clone(), from };
                    tab.search_sort = (SortKey::Name, true);
                    tab.stale = true;
                }
            }
            tab.filter = text;
            self.update_title();
            self.run_search(i);
        } else {
            let tab = &mut self.tabs[i];
            tab.filter = text;
            tab.apply_filter();
            tab.target = 0.0;
            tab.scroll = 0.0;
        }
        self.win.repaint();
    }

    fn edit_key(&mut self, vk: u16, _mods: Mods, cx: &mut Cx) {
        let Some(kind) = self.edit.clone() else { return };
        let text = self.win.edit_text().unwrap_or_default();
        match (kind, vk) {
            (EditKind::Password, K_RETURN) => self.sheet_ok(),
            (EditKind::Password, K_ESCAPE) => self.end_edit(false),
            (EditKind::Password, _) => {}
            (EditKind::Address, K_RETURN) => {
                self.end_edit(false);
                self.commit_address(&text, cx);
            }
            (EditKind::Search, K_RETURN) | (EditKind::Search, K_DOWN) => {
                self.end_edit(false);
                if self.tab().rows() > 0 {
                    let tab = self.tab_mut();
                    let r = tab.focus.filter(|&f| f < tab.rows()).unwrap_or(0);
                    tab.select_only(r);
                    self.keyboard = true;
                    self.reveal(r);
                }
            }
            (EditKind::Search, K_ESCAPE) => {
                self.end_edit(false);
                self.set_filter(String::new());
            }
            (EditKind::Rename { .. }, K_RETURN) | (EditKind::Rename { .. }, K_TAB) => self.end_edit(true),
            (_, K_ESCAPE) => self.end_edit(false),
            (EditKind::Address, K_F4) => self.end_edit(false),
            _ => {}
        }
        cx.repaint();
    }

    // ───────────────────────── réglages ─────────────────────────

    pub fn apply_theme(&mut self) {
        self.theme = Theme::new(self.set.dark(), self.set.accent);
        self.win.set_dark(self.theme.dark);
        shell::menu::set_dark_menus(self.theme.dark);
        self.win.repaint();
    }

    pub fn set_show_hidden(&mut self, on: bool) {
        self.set.show_hidden = on;
        self.set.save();
        for i in 0..self.tabs.len() {
            let snap = self.tabs[i].snapshot();
            self.tabs[i].restore = Some(snap);
            self.load(i);
        }
    }

    pub fn sort_by(&mut self, key: SortKey) {
        let tab = self.tab_mut();
        if matches!(tab.loc, Location::Search { .. }) {
            // les résultats sont retriés par l'index : le tri porte sur tous, pas seulement sur ceux affichés
            let (cur, asc) = tab.search_sort;
            tab.search_sort = if cur == key { (key, !asc) } else { (key, matches!(key, SortKey::Name | SortKey::Path)) };
            self.run_search(self.cur);
            return;
        }
        if tab.sort == key {
            tab.asc = !tab.asc;
        } else {
            tab.sort = key;
            // les dates et les tailles se lisent d'abord du plus récent, du plus gros
            tab.asc = !matches!(key, SortKey::Modified | SortKey::Created | SortKey::Size);
        }
        tab.resort();
        if let Some(f) = self.tab().focus {
            self.reveal(f);
        }
        self.win.repaint();
    }

    // ───────────────────────── souris ─────────────────────────

    /// Ce que la souris touche au point `p`.
    pub fn hit(&self, p: Pt) -> Option<Id> {
        match self.hits.at(p).copied() {
            Some(Id::List) => Some(self.row_at(p).map_or(Id::List, Id::Row)),
            other => other,
        }
    }

    pub fn row_at(&self, p: Pt) -> Option<usize> {
        let l = self.lay.list;
        let tab = self.tab();
        if !l.contains(p) || tab.stale {
            return None;
        }
        let y = p.y - l.y - LIST_TOP + tab.scroll;
        if y < 0.0 {
            return None;
        }
        if self.set.thumbs {
            return self.tile_at(p.x, y);
        }
        let r = (y / ROW_H) as usize;
        (r < tab.rows() && p.x >= l.x + 6.0 && p.x < l.right() - 14.0).then_some(r)
    }

    /// Le point est-il sur l'icône ou le nom de la ligne (et non dans le vide à sa droite) ?
    fn on_name(&self, row: usize, p: Pt) -> bool {
        // en vignettes, toute la case compte
        if self.set.thumbs {
            return true;
        }
        row.checked_sub(self.first_row).and_then(|k| self.name_right.get(k)).is_some_and(|&right| p.x <= right + 6.0)
    }

    fn set_hover(&mut self, p: Pt, cx: &mut Cx) {
        let h = self.hit(p);
        if h != self.hover {
            self.hover = h;
            cx.repaint();
        }
    }

    fn mouse_down(&mut self, p: Pt, button: Button, clicks: u8, mods: Mods, cx: &mut Cx) {
        self.mouse = p;
        self.keyboard = false;
        // boîte de dialogue ouverte : seuls ses boutons, et ceux de la fenêtre, réagissent
        if self.sheet.is_some() {
            if button == Button::Left {
                let live =
                    |id: &Id| matches!(id, Id::SheetOk | Id::SheetCancel | Id::SheetReveal | Id::SheetZip | Id::Sheet7z | Id::SheetRar | Id::Min | Id::Max | Id::Close);
                self.pressed = self.hit(p).filter(live);
            }
            cx.repaint();
            return;
        }
        // tout clic dans la fenêtre termine la saisie en cours (un renommage est appliqué)
        self.end_edit(true);
        let id = self.hit(p);
        match button {
            Button::Left => {
                self.pressed = id;
                self.collapse_on_up = None;
                match id {
                    Some(Id::Search) => self.start_search_edit(),
                    Some(Id::Tab(i)) => self.activate_tab(i),
                    Some(Id::Row(r)) => self.row_down(r, p, clicks, mods, cx),
                    Some(Id::List) => {
                        if !mods.ctrl && !mods.shift {
                            self.tab_mut().clear_sel();
                        }
                        self.drag = Drag::LassoWait { start: p, ctrl: mods.ctrl };
                    }
                    Some(Id::Thumb) => self.drag = Drag::Thumb { grab: p.y - self.lay.thumb.y },
                    Some(Id::Track) => {
                        let page = self.lay.list.h - ROW_H;
                        let dir = if p.y < self.lay.thumb.y { -1.0 } else { 1.0 };
                        let tab = self.tab_mut();
                        tab.target = (tab.target + dir * page).max(0.0);
                    }
                    Some(Id::NavSplit) => self.drag = Drag::NavSplit,
                    Some(Id::ColSplit(col)) => {
                        let now = self.columns().iter().find(|c| c.0 == col).map(|c| c.2);
                        if clicks == 2 {
                            // double clic : la largeur de départ
                            let start = match col {
                                Col::Date => settings::COLS[0],
                                Col::Type => settings::COLS[1],
                                Col::Size => settings::COLS[2],
                                _ => 0.0,
                            };
                            if let Some(w) = self.col_width(col) {
                                *w = start;
                            }
                            self.set.save();
                        } else if let Some(w) = now {
                            self.drag = Drag::Col { col, x: p.x, w };
                        }
                    }
                    Some(Id::Header(_)) if clicks == 2 => {}
                    _ => {}
                }
            }
            Button::Middle => match id {
                Some(Id::Tab(i)) | Some(Id::TabClose(i)) => self.close_tab(i, cx),
                Some(Id::Row(r)) => {
                    if self.tab().item(r).is_dir() {
                        self.open_row(r, true, cx);
                    }
                }
                Some(Id::Nav(i)) => {
                    if let Some(row) = self.nav_rows.get(i) {
                        self.new_tab(row.loc.clone(), false);
                    }
                }
                Some(Id::Crumb(i)) => {
                    if let Some(loc) = self.crumbs.get(i).cloned() {
                        self.new_tab(loc, false);
                    }
                }
                _ => {}
            },
            Button::Right => {
                match id {
                    Some(Id::Row(r)) => {
                        if !self.tab().item(r).sel {
                            self.tab_mut().select_only(r);
                        } else {
                            self.tab_mut().focus = Some(r);
                        }
                    }
                    Some(Id::List) => self.tab_mut().clear_sel(),
                    _ => {}
                }
                self.context_menu(id, p, mods, cx);
            }
            Button::Back => self.back(),
            Button::Forward => self.forward(),
        }
        cx.repaint();
    }

    fn row_down(&mut self, r: usize, p: Pt, clicks: u8, mods: Mods, cx: &mut Cx) {
        if clicks == 2 && !mods.ctrl && !mods.shift {
            self.open_row(r, false, cx);
            return;
        }
        let on_name = self.on_name(r, p);
        let tab = self.tab_mut();
        if mods.shift {
            let a = tab.anchor.unwrap_or(r).min(tab.rows().saturating_sub(1));
            tab.select_range(a, r, mods.ctrl);
            tab.anchor = Some(a);
            tab.focus = Some(r);
        } else if mods.ctrl {
            tab.toggle(r);
        } else if !tab.item(r).sel {
            tab.select_only(r);
        } else {
            tab.focus = Some(r);
            tab.anchor = Some(r);
            self.collapse_on_up = Some(r);
        }
        self.drag = if on_name { Drag::Item { start: p, row: r } } else { Drag::LassoWait { start: p, ctrl: mods.ctrl || mods.shift } };
    }

    fn mouse_move(&mut self, p: Pt, cx: &mut Cx) {
        self.mouse = p;
        let far = |a: Pt| (a.x - p.x).abs() > 4.0 || (a.y - p.y).abs() > 4.0;
        match &self.drag {
            Drag::None => self.set_hover(p, cx),
            Drag::Thumb { grab } => {
                let (track, thumb, max) = (self.lay.track, self.lay.thumb, self.lay.max_scroll);
                let room = (track.h - thumb.h).max(1.0);
                let pos = ((p.y - grab - track.y) / room).clamp(0.0, 1.0);
                let tab = self.tab_mut();
                tab.target = pos * max;
                tab.scroll = tab.target;
                cx.repaint();
            }
            Drag::NavSplit => {
                self.set.nav_w = p.x.clamp(140.0, (self.lay.w - 320.0).clamp(140.0, 480.0));
                cx.repaint();
            }
            Drag::Col { col, x, w } => {
                // le bord gauche de la colonne suit la souris : vers la gauche, elle s'élargit
                let (col, wanted) = (*col, (*w + (*x - p.x)).clamp(60.0, 900.0));
                if let Some(w) = self.col_width(col) {
                    *w = wanted;
                }
                cx.repaint();
            }
            Drag::LassoWait { start, ctrl } => {
                if far(*start) {
                    let (start, ctrl) = (*start, *ctrl);
                    let l = self.lay.list;
                    let tab = self.tab();
                    let base = if ctrl { tab.items.iter().map(|it| it.sel).collect() } else { vec![] };
                    self.drag = Drag::Lasso { x0: start.x - l.x, y0: start.y - l.y + tab.scroll, ctrl, base };
                    self.collapse_on_up = None;
                    cx.timer(TIMER_LASSO, Some(30));
                    self.lasso_update();
                    cx.repaint();
                }
            }
            Drag::Lasso { .. } => {
                self.lasso_update();
                cx.repaint();
            }
            Drag::Item { start, row } => {
                if far(*start) {
                    let row = *row;
                    self.drag = Drag::None;
                    self.collapse_on_up = None;
                    self.pressed = None;
                    self.start_item_drag(row, cx);
                }
            }
        }
    }

    /// Met la sélection en accord avec le rectangle de sélection.
    fn lasso_update(&mut self) {
        if self.set.thumbs {
            return self.lasso_tiles();
        }
        let (l, p) = (self.lay.list, self.mouse);
        let Drag::Lasso { y0, ctrl, base, .. } = &self.drag else { return };
        let (y0, ctrl) = (*y0, *ctrl);
        let base = base.clone();
        let tab = self.tab_mut();
        let y1 = p.y - l.y + tab.scroll;
        let (ya, yb) = (y0.min(y1) - LIST_TOP, y0.max(y1) - LIST_TOP);
        let first = (ya / ROW_H).floor().max(0.0) as usize;
        let last = (yb / ROW_H).floor();
        for r in 0..tab.rows() {
            let inside = yb >= 0.0 && r >= first && (r as f32) <= last;
            let i = tab.idx(r);
            let was = base.get(i).copied().unwrap_or(false);
            tab.items[i].sel = if ctrl { was ^ inside } else { inside };
        }
        tab.recount();
        if tab.rows() > 0 {
            let end = ((y1 - LIST_TOP) / ROW_H).floor().clamp(0.0, (tab.rows() - 1) as f32) as usize;
            tab.focus = Some(end);
        }
    }

    fn mouse_up(&mut self, p: Pt, button: Button, mods: Mods, cx: &mut Cx) {
        self.mouse = p;
        if button != Button::Left {
            return;
        }
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        match drag {
            Drag::Lasso { .. } => cx.timer(TIMER_LASSO, None),
            Drag::NavSplit | Drag::Col { .. } => self.set.save(),
            _ => {}
        }
        let pressed = self.pressed.take();
        let id = self.hit(p);
        if let Some(r) = self.collapse_on_up.take()
            && id == Some(Id::Row(r))
            && !mods.ctrl
            && !mods.shift
        {
            self.tab_mut().select_only(r);
        }
        if pressed.is_some() && pressed == id {
            self.click(id.unwrap(), cx);
        }
        self.set_hover(p, cx);
        cx.repaint();
    }

    /// Bouton gauche enfoncé puis relâché au même endroit.
    fn click(&mut self, id: Id, cx: &mut Cx) {
        match id {
            Id::TabClose(i) => self.close_tab(i, cx),
            Id::NewTab => self.new_tab(Location::ThisPc, true),
            Id::Update => {
                plume_update::open_suite_update();
            }
            Id::Min => cx.after(|w| w.minimize()),
            Id::Max => cx.after(|w| w.toggle_maximize()),
            Id::Close => cx.after(|w| w.close()),
            Id::Back => self.back(),
            Id::Forward => self.forward(),
            Id::Up => self.up(),
            Id::Refresh => self.refresh(),
            Id::More => self.more_menu(cx),
            Id::Crumb(i) => {
                if let Some(loc) = self.crumbs.get(i).cloned() {
                    if loc == self.tab().loc { self.start_address_edit() } else { self.navigate(loc) }
                }
            }
            Id::Address => self.start_address_edit(),
            Id::SearchClear => self.set_filter(String::new()),
            Id::ScopeHere => self.set_scope(false),
            Id::ScopeAll => self.set_scope(true),
            Id::Filter => self.filter_menu(cx),
            Id::SheetOk | Id::SheetCancel | Id::SheetReveal | Id::SheetZip | Id::Sheet7z | Id::SheetRar => self.sheet_click(id),
            Id::Nav(i) => {
                if let Some(row) = self.nav_rows.get(i) {
                    self.navigate(row.loc.clone());
                }
            }
            Id::Header(col) => {
                let key = match col {
                    Col::Name => Some(SortKey::Name),
                    Col::Date => Some(SortKey::Modified),
                    Col::Type => Some(SortKey::Type),
                    Col::Size => Some(SortKey::Size),
                    Col::Folder => Some(SortKey::Path),
                    _ => None,
                };
                if let Some(k) = key {
                    self.sort_by(k);
                }
            }
            _ => {}
        }
    }

    fn wheel(&mut self, p: Pt, dy: f32, cx: &mut Cx) {
        if self.lay.nav.contains(p) || self.sheet.is_some() {
            return;
        }
        if matches!(self.edit, Some(EditKind::Rename { .. })) {
            self.end_edit(true);
        }
        let lines = self.wheel_lines;
        let tab = self.tab_mut();
        tab.target = (tab.target - dy * lines * ROW_H).max(0.0);
        cx.repaint();
    }

    /// Commence à faire glisser la sélection (ou la ligne attrapée, si elle n'est pas sélectionnée).
    fn start_item_drag(&mut self, row: usize, cx: &mut Cx) {
        let tab = self.tab();
        if tab.stale || row >= tab.rows() {
            return;
        }
        let paths: Vec<PathBuf> = if tab.item(row).sel { tab.selected_paths() } else { tab.path_of(row).into_iter().collect() };
        if paths.is_empty() {
            return;
        }
        cx.capture(false);
        cx.after(move |win| {
            shell::dnd::drag(win, &paths);
            win.with(|h, _| {
                if let Some(app) = h.as_any().downcast_mut::<App>() {
                    app.drop_at(None);
                    app.check_now();
                }
            });
        });
    }

    /// Un glisser-déposer survole le point `p` : renvoie le dossier qui recevrait les fichiers, et le met en
    /// évidence. `None` : le glisser est fini ou sorti de la fenêtre.
    pub fn drop_at(&mut self, p: Option<Pt>) -> Option<PathBuf> {
        let p = p.filter(|_| self.sheet.is_none());
        let here = || self.tab().loc.path().map(Path::to_path_buf);
        let (hl, path) = match p.and_then(|p| self.hit(p)) {
            Some(Id::Row(r)) if self.tab().item(r).is_dir() => (Id::Row(r), self.tab().path_of(r)),
            Some(Id::Row(_) | Id::List) => (Id::List, here()),
            Some(Id::Nav(i)) => (Id::Nav(i), self.nav_rows.get(i).and_then(|n| n.loc.path().map(Path::to_path_buf))),
            Some(Id::Tab(i)) => (Id::Tab(i), self.tabs.get(i).and_then(|t| t.loc.path().map(Path::to_path_buf))),
            Some(Id::Crumb(i)) => (Id::Crumb(i), self.crumbs.get(i).and_then(|l| l.path().map(Path::to_path_buf))),
            _ => (Id::List, None),
        };
        let hl = path.as_ref().map(|_| hl);
        if hl != self.drop_hl {
            self.drop_hl = hl;
            self.win.repaint();
        }
        path
    }

    // ───────────────────────── clavier ─────────────────────────

    /// Renvoie vrai si la touche a été comprise.
    fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) -> bool {
        let Mods { ctrl, shift, alt } = mods;
        // boîte de dialogue ouverte : Entrée valide, Échap annule, le reste attend
        if self.sheet.is_some() {
            match vk {
                K_RETURN => self.sheet_ok(),
                K_ESCAPE => self.close_sheet(),
                _ => {}
            }
            cx.repaint();
            return true;
        }
        match vk {
            K_LEFT if alt => self.back(),
            K_RIGHT if alt => self.forward(),
            K_UP if alt => self.up(),
            K_BACK if !ctrl && !alt => self.back(),
            K_F5 => self.refresh(),
            v if v == letter('R') && ctrl => self.refresh(),
            v if v == letter('T') && ctrl => {
                let loc = self.tab().loc.clone();
                self.new_tab(if shift { loc } else { Location::ThisPc }, true);
            }
            v if v == letter('W') && ctrl => self.close_tab(self.cur, cx),
            K_F4 if ctrl => self.close_tab(self.cur, cx),
            K_TAB if ctrl => {
                let n = self.tabs.len();
                self.activate_tab(if shift { (self.cur + n - 1) % n } else { (self.cur + 1) % n });
            }
            v if ctrl && !shift && (letter('1')..=letter('9')).contains(&v) => {
                let n = (v - letter('1')) as usize;
                self.activate_tab(if n == 8 { self.tabs.len() - 1 } else { n });
            }
            v if (v == letter('L') && ctrl) || (v == letter('D') && alt) => self.start_address_edit(),
            K_F4 => self.start_address_edit(),
            v if (v == letter('F') || v == letter('E')) && ctrl => self.start_search_edit(),
            K_F3 => self.start_search_edit(),
            v if v == letter('A') && ctrl => self.tab_mut().select_all(),
            v if v == letter('H') && ctrl => self.set_show_hidden(!self.set.show_hidden),
            K_RETURN if !alt => self.open_selection(ctrl, cx),
            K_ESCAPE => {
                if !self.tab().filter.is_empty() {
                    self.set_filter(String::new());
                } else {
                    self.tab_mut().clear_sel();
                }
            }
            K_UP | K_DOWN | K_HOME | K_END | K_PRIOR | K_NEXT if !alt => self.move_focus(vk, mods),
            K_LEFT | K_RIGHT if !alt && self.set.thumbs => self.move_focus(vk, mods),
            K_SPACE if ctrl => {
                if let Some(f) = self.tab().focus {
                    self.tab_mut().toggle(f);
                }
            }
            _ => return self.key_more(vk, mods, cx),
        }
        cx.repaint();
        true
    }

    fn on_close(&mut self) {
        self.end_edit(false);
        for t in &self.tabs {
            t.cancel.store(true, Ordering::Relaxed);
        }
        shell::dnd::revoke(self.win);
        self.set.win = self.win.placement();
        self.set.save();
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        self.paint_all(c, cx, dt);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => self.mouse_move(p, cx),
            Event::MouseDown { p, button, clicks, mods } => self.mouse_down(p, button, clicks, mods, cx),
            Event::MouseUp { p, button, mods } => self.mouse_up(p, button, mods, cx),
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
            Event::Char(ch) => self.type_ahead(ch),
            Event::Active(on) => {
                self.active = on;
                if on {
                    self.check_now();
                }
                cx.repaint();
            }
            Event::Devices => {
                load::spawn_places(self.win.sender(), self.set.pinned.clone());
                self.check_now();
            }
            Event::Timer(TIMER_POLL) => self.poll(),
            Event::Opened(text) => {
                // le programme a été relancé alors qu'il tournait déjà : ce qu'il devait ouvrir s'ouvre ici, dans
                // de nouveaux onglets, et la fenêtre passe devant
                let mut any = false;
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    if let Some(p) = parse_address(line, None).filter(|p| p.is_dir()) {
                        self.new_tab(Location::Path(p), true);
                        any = true;
                    }
                }
                if !any {
                    self.new_tab(Location::ThisPc, true);
                }
                if std::env::var_os("PLUME_FEX_NOACTIVATE").is_none() {
                    cx.after(|w| w.bring_to_front());
                }
                cx.repaint();
            }
            Event::Resized | Event::Scale => {
                if self.edit.is_some() {
                    // le champ suit sa place à la prochaine image
                    cx.repaint();
                }
            }
            Event::SystemTheme => {
                if self.theme.dark != self.set.dark() {
                    self.apply_theme();
                }
            }
            Event::Timer(TIMER_LASSO) => {
                if matches!(self.drag, Drag::Lasso { .. }) {
                    let (l, p) = (self.lay.list, self.mouse);
                    let step = if p.y < l.y {
                        p.y - l.y
                    } else if p.y > l.bottom() {
                        p.y - l.bottom()
                    } else {
                        0.0
                    };
                    if step != 0.0 {
                        let max = self.lay.max_scroll;
                        let tab = self.tab_mut();
                        tab.target = (tab.target + step.clamp(-60.0, 60.0) * 0.6).clamp(0.0, max);
                        tab.scroll = tab.target;
                        self.lasso_update();
                        cx.repaint();
                    }
                } else {
                    cx.timer(TIMER_LASSO, None);
                }
            }
            Event::Timer(TIMER_TOAST) => {
                self.toast = None;
                cx.timer(TIMER_TOAST, None);
                cx.repaint();
            }
            Event::Timer(_) => {}
            Event::EditChanged(text) => {
                if self.edit == Some(EditKind::Search) {
                    self.set_filter(text);
                } else if self.edit == Some(EditKind::Password)
                    && let Some(sheet) = self.sheet.as_mut()
                    && sheet.error.take().is_some()
                {
                    // on retape le mot de passe : le message d'erreur n'a plus lieu d'être
                    cx.repaint();
                }
            }
            Event::EditKey { vk, mods } => self.edit_key(vk, mods, cx),
            Event::EditBlur => {
                if self.sheet.is_some() {
                    // un clic sur un bouton de la boîte de dialogue : le clavier revient au mot de passe
                    self.win.edit_focus();
                } else {
                    // la fenêtre a perdu le clavier pendant une saisie : un renommage est appliqué, le reste est fermé
                    self.end_edit(true);
                }
            }
            Event::Close => self.on_close(),
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            // annoncé comme tel à Windows, le bouton « agrandir » montre les dispositions d'ancrage de Windows 11
            Some(Id::Max) => Area::MaxButton,
            Some(Id::Tab(_) | Id::TabClose(_) | Id::NewTab | Id::Min | Id::Close | Id::Update) => Area::Control,
            _ if p.y < m::TITLE_H => Area::Caption,
            _ => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match (&self.drag, self.hits.at(p)) {
            (Drag::NavSplit | Drag::Col { .. }, _) | (_, Some(Id::NavSplit | Id::ColSplit(_))) => Cursor::SizeWE,
            _ => Cursor::Arrow,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
