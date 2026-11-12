//! Plume Clean's window, drawn by hand (Direct2D): a top bar with the three pages (Drives, Map, Cleanup) and the
//! settings, menus and dialogs above. Scans, measures, duplicate checks and cleanups run on other threads and report
//! here through the window's sender.

mod cleanup;
mod map;
mod paint;
mod settings_view;

use crate::{
    clean::{self, Blocked, Kind, Outcome, Places, Rules, Suggestion},
    drives::{self, Drive},
    dupes,
    i18n::{fill, tr},
    scan::{self, Order, Tree},
    settings::Settings,
};
use plume_ui::{Area, Button, Canvas, Color, Cursor, Cx, Event, Gfx, Handler, Hits, Mods, PaintCx, Pt, Rect, Window};
use std::{
    any::Any,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use windows::Win32::Graphics::Imaging::IWICImagingFactory;

pub const TICK: usize = 1;
pub const TOP_H: f32 = 48.0;
pub const ROW_H: f32 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Drives,
    Map,
    Cleanup,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListMode {
    Contents,
    Files,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Tab(View),
    // drives
    ScanDrive(usize),
    ScanFolder,
    OpenMap,
    // map
    Up,
    Crumb(u32),
    Block(u32),
    Row(usize),
    Mode(ListMode),
    Sort(Order),
    Explorer,
    Fex,
    Rescan,
    CancelScan,
    // cleanup
    Tick(usize),
    Review(usize),
    FindDupes,
    CancelDupes,
    Elevate,
    Refresh,
    Clean,
    GoScan,
    DismissResult,
    // review
    ReviewRow(usize),
    ReviewAll(bool),
    KeepOldest,
    ReviewDone,
    // settings
    Choice(&'static str, usize),
    Toggle(&'static str),
    // shared
    MenuItem(usize),
    ModalOk,
    ModalCancel,
    Toast,
}

#[derive(Clone, Debug)]
pub enum MenuAct {
    Enter(u32),
    Open(PathBuf),
    Explorer(PathBuf),
    Fex(PathBuf),
    Copy(PathBuf),
}

pub struct Menu {
    pub x: f32,
    pub y: f32,
    pub items: Vec<(String, MenuAct)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modal {
    /// The cleanup, before it starts.
    Confirm,
    /// Restart with administrator rights.
    Elevate,
}

/// The icon's lime: Plume Clean's accent.
pub const LIME: u32 = 0x84cc16;

/// The colors of the window: Plume's theme (plume_ui::Theme), with the icon's lime as the accent.
#[derive(Clone)]
pub struct Pal {
    pub dark: bool,
    pub bg: Color,
    pub card: Color,
    pub solid: Color,
    pub solid2: Color,
    pub fill: Color,
    pub fill2: Color,
    pub hover: Color,
    pub t1: Color,
    pub t2: Color,
    pub t3: Color,
    pub t4: Color,
    /// Hairlines between things and around cards.
    pub line: Color,
    pub ring: Color,
    pub hdr1: Color,
    pub hdr2: Color,
    pub close_hover: Color,
    pub red: Color,
    pub warn: Color,
    /// Buttons, ticks, switches, selection.
    pub accent: Color,
    /// The accent as text or icon on a card (darker in light mode, to stay readable).
    pub ink: Color,
    /// Text on the accent: lime is light, so near-black.
    pub on_accent: Color,
}

impl Pal {
    pub fn new(dark: bool) -> Pal {
        let t = plume_ui::Theme::new(dark, Some(Color::hex(LIME)));
        Pal {
            dark,
            bg: t.bg,
            card: t.card,
            solid: t.solid,
            solid2: t.solid2,
            fill: t.fill,
            fill2: t.fill2,
            hover: t.hover,
            t1: t.t1,
            t2: t.t2,
            t3: t.t3,
            t4: t.t4,
            line: t.sep,
            ring: t.ring,
            hdr1: t.hdr1,
            hdr2: t.hdr2,
            close_hover: t.close_hover,
            red: t.red,
            warn: t.orange,
            accent: t.accent,
            ink: if dark { Color::hex(0xa3e635) } else { Color::hex(0x4d7c0f) },
            on_accent: Color::hex(0x1a2e05),
        }
    }
}

/// Where the duplicate check stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dups {
    NotYet,
    Running,
    Done,
}

pub struct App {
    pub win: Window,
    pub wic: IWICImagingFactory,
    pub set: Settings,
    pub pal: Pal,
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub view: View,
    pub drives: Vec<Drive>,
    // the scan and its map
    pub tree: Option<Arc<Tree>>,
    pub scanning: Option<Arc<scan::Progress>>,
    pub target: Option<PathBuf>,
    pub cur: u32,
    pub mode: ListMode,
    pub order: Order,
    pub desc: bool,
    pub rows: Vec<u32>,
    pub sel: Option<u32>,
    pub list_scroll: f32,
    pub list_rect: Rect,
    // cleanup
    pub places: Places,
    pub sugg: Vec<Suggestion>,
    pub measuring: bool,
    pub dups: Dups,
    pub dup_progress: Option<Arc<dupes::Progress>>,
    pub cleaning: bool,
    pub result: Option<Outcome>,
    pub clean_scroll: f32,
    pub clean_rect: Rect,
    pub review: Option<usize>,
    pub review_scroll: f32,
    pub review_rect: Rect,
    pub elevated: bool,
    // shared
    pub modal: Option<Modal>,
    pub menu: Option<Menu>,
    pub toast: Option<(String, Instant)>,
    pub log: Vec<String>,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

pub fn dark_for(theme: &str) -> bool {
    match theme {
        "dark" => true,
        "light" => false,
        _ => crate::system_dark(),
    }
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>) -> App {
        let set = Settings::load();
        crate::i18n::set(&set.lang);
        let pal = Pal::new(dark_for(&set.theme));
        win.set_dark(pal.dark);
        let wic = gfx.wic.clone();
        let places = Places::current();
        let mut app = App {
            win,
            wic,
            set,
            pal,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            view: View::Drives,
            drives: drives::list(),
            tree: None,
            scanning: None,
            target: None,
            cur: 0,
            mode: ListMode::Contents,
            order: Order::Size,
            desc: true,
            rows: vec![],
            sel: None,
            list_scroll: 0.0,
            list_rect: Rect::default(),
            places,
            sugg: vec![],
            measuring: false,
            dups: Dups::NotYet,
            dup_progress: None,
            cleaning: false,
            result: None,
            clean_scroll: 0.0,
            clean_rect: Rect::default(),
            review: None,
            review_scroll: 0.0,
            review_rect: Rect::default(),
            elevated: clean::elevated(),
            modal: None,
            menu: None,
            toast: None,
            log: vec![],
        };
        if app.places.sandbox.is_some() {
            app.note("sandbox mode".into());
        }
        app
    }

    pub fn note(&mut self, s: String) {
        self.log.push(s);
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        self.toast = Some((text.into(), Instant::now()));
        cx.timer(TICK, Some(100));
        cx.repaint();
    }

    pub fn go(&mut self, v: View, cx: &mut Cx) {
        self.view = v;
        self.menu = None;
        self.review = None;
        if v == View::Drives {
            self.drives = drives::list();
        }
        if v == View::Cleanup && self.sugg.is_empty() && !self.measuring {
            self.measure(cx);
        }
        cx.repaint();
    }

    // ───────────────────────── scan ─────────────────────────

    pub fn start_scan(&mut self, path: PathBuf, cx: &mut Cx) {
        if let Some(p) = &self.scanning {
            p.cancel.store(true, Ordering::Relaxed);
        }
        let prog = Arc::new(scan::Progress::default());
        self.scanning = Some(prog.clone());
        self.target = Some(scan::normalize(&path));
        self.tree = None;
        self.view = View::Map;
        self.note(format!("scan {}", path.display()));
        let sender = cx.sender();
        std::thread::spawn(move || {
            let tree = scan::scan(&path, scan::threads(), &prog);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.scan_done(tree, &prog, cx);
                }
            });
        });
        cx.timer(TICK, Some(100));
        cx.repaint();
    }

    fn scan_done(&mut self, tree: Tree, prog: &Arc<scan::Progress>, cx: &mut Cx) {
        if !self.scanning.as_ref().is_some_and(|p| Arc::ptr_eq(p, prog)) {
            return;
        }
        self.scanning = None;
        if tree.cancelled {
            self.note("scan cancelled".into());
            self.say(tr("Scan stopped."), cx);
            return;
        }
        self.note(format!("scanned {} entries in {} ms", tree.len(), tree.millis));
        self.tree = Some(Arc::new(tree));
        self.cur = 0;
        self.sel = None;
        self.refresh_rows();
        // what came from the previous scan is out of date
        self.dups = Dups::NotYet;
        if !self.sugg.is_empty() {
            self.measure(cx);
        }
        cx.repaint();
    }

    pub fn cancel_scan(&mut self, cx: &mut Cx) {
        if let Some(p) = &self.scanning {
            p.cancel.store(true, Ordering::Relaxed);
        }
        cx.repaint();
    }

    pub fn refresh_rows(&mut self) {
        let Some(t) = self.tree.clone() else {
            self.rows.clear();
            return;
        };
        self.rows = match self.mode {
            ListMode::Contents => t.sorted_children(self.cur, self.order, self.desc),
            ListMode::Files => {
                let mut v = t.biggest_files(self.cur, 500);
                if self.order != Order::Size || !self.desc {
                    t.sort(&mut v, self.order, self.desc);
                }
                v
            }
        };
        self.list_scroll = 0.0;
    }

    pub fn enter(&mut self, id: u32, cx: &mut Cx) {
        let Some(t) = &self.tree else { return };
        if (id as usize) < t.len() && t.node(id).is_dir() {
            self.cur = id;
            self.sel = None;
            self.refresh_rows();
            cx.repaint();
        }
    }

    pub fn up(&mut self, cx: &mut Cx) {
        if let Some(t) = &self.tree
            && self.cur != 0
        {
            let parent = t.node(self.cur).parent;
            let from = self.cur;
            self.enter(parent, cx);
            self.sel = Some(from);
        }
    }

    // ───────────────────────── cleanup ─────────────────────────

    fn rules(&self) -> Rules {
        Rules {
            old_days: self.set.old_days,
            large_bytes: self.set.large_mb * 1_000_000,
            running: clean::running(),
            elevated: self.elevated,
            now: clean::now_filetime(),
        }
    }

    /// Measures every suggestion again (on another thread). Duplicates found before are kept if the scan did not
    /// change.
    pub fn measure(&mut self, cx: &mut Cx) {
        if self.measuring {
            return;
        }
        self.measuring = true;
        let places = self.places.clone();
        let tree = self.tree.clone();
        let rules = self.rules();
        let sender = cx.sender();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
            }
            let list: Vec<Suggestion> = clean::ALL.iter().map(|&k| clean::suggest(k, &places, tree.as_deref(), &rules)).collect();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.measured(list, cx);
                }
            });
        });
        cx.repaint();
    }

    fn measured(&mut self, mut list: Vec<Suggestion>, cx: &mut Cx) {
        self.measuring = false;
        for s in list.iter_mut() {
            // keep the user's ticks, and the duplicates already found for this scan
            if let Some(old) = self.sugg.iter().find(|o| o.kind == s.kind) {
                if s.kind == Kind::Duplicates && self.dups == Dups::Done {
                    *s = old.clone();
                    continue;
                }
                s.selected = old.selected && s.blocked.is_none() && s.count > 0;
                if s.kind.review() {
                    for it in s.items.iter_mut() {
                        it.selected = old.items.iter().any(|o| o.path == it.path && o.selected);
                    }
                }
            }
        }
        // what holds nothing goes to the end
        list.sort_by_key(|s| s.count == 0 && s.kind != Kind::Duplicates);
        self.sugg = list;
        self.note(format!("measured {} suggestions", self.sugg.len()));
        cx.repaint();
    }

    pub fn find_dupes(&mut self, cx: &mut Cx) {
        let Some(tree) = self.tree.clone() else { return };
        if self.dups == Dups::Running {
            return;
        }
        self.dups = Dups::Running;
        let prog = Arc::new(dupes::Progress::default());
        self.dup_progress = Some(prog.clone());
        let min = self.set.dup_mb * 1_000_000;
        let sender = cx.sender();
        std::thread::spawn(move || {
            let files = clean::duplicate_candidates(&tree, min);
            let groups = dupes::find(files, 4, &prog);
            let cancelled = prog.cancel.load(Ordering::Relaxed);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.dup_progress = None;
                    if cancelled {
                        app.dups = Dups::NotYet;
                    } else {
                        app.dups = Dups::Done;
                        let s = clean::with_duplicates(&groups);
                        app.note(format!("duplicates: {} groups", groups.len()));
                        match app.sugg.iter_mut().find(|x| x.kind == Kind::Duplicates) {
                            Some(x) => *x = s,
                            None => app.sugg.push(s),
                        }
                    }
                    cx.repaint();
                }
            });
        });
        cx.timer(TICK, Some(100));
        cx.repaint();
    }

    pub fn selected_total(&self) -> u64 {
        self.sugg.iter().map(|s| s.selected_size()).sum()
    }

    pub fn start_clean(&mut self, cx: &mut Cx) {
        if self.cleaning {
            return;
        }
        let jobs: Vec<(Kind, Vec<(PathBuf, u64)>)> =
            self.sugg.iter().filter(|s| s.selected && s.blocked.is_none() && s.selected_size() > 0).map(|s| (s.kind, s.to_remove())).collect();
        if jobs.is_empty() {
            return;
        }
        self.cleaning = true;
        self.result = None;
        let places = self.places.clone();
        let recycle = self.set.recycle;
        let sender = cx.sender();
        self.note(format!("clean {}", jobs.iter().map(|j| j.0.key()).collect::<Vec<_>>().join(",")));
        std::thread::spawn(move || {
            let mut total = Outcome::default();
            for (kind, list) in jobs {
                let o = if kind == Kind::RecycleBin { clean::empty_bin(&places) } else { clean::remove(&list, recycle, &places) };
                total.add(&o);
            }
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.cleaning = false;
                    app.note(format!("cleaned: freed {} removed {} failed {} refused {}", total.freed, total.removed, total.failed, total.refused));
                    app.result = Some(total);
                    for s in app.sugg.iter_mut() {
                        s.selected = false;
                    }
                    // what was removed from the scan's lists is gone: measure again
                    if app.dups == Dups::Done {
                        app.dups = Dups::NotYet;
                    }
                    app.measure(cx);
                }
            });
        });
        cx.repaint();
    }

    /// Restarts the app with administrator rights, after the user said yes. Never in a test build.
    pub fn elevate(&mut self, cx: &mut Cx) {
        self.note("elevate requested".into());
        if cfg!(feature = "test-hook") || self.places.sandbox.is_some() {
            self.say(tr("A test build does not restart as administrator."), cx);
            return;
        }
        use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let exe = std::env::current_exe().unwrap_or_default();
        let ew: Vec<u16> = exe.to_string_lossy().encode_utf16().chain(Some(0)).collect();
        let args: Vec<u16> = "--cleanup".encode_utf16().chain(Some(0)).collect();
        let ok = unsafe {
            ShellExecuteW(
                Some(self.win.hwnd),
                windows::core::w!("runas"),
                windows::core::PCWSTR(ew.as_ptr()),
                windows::core::PCWSTR(args.as_ptr()),
                None,
                SW_SHOWNORMAL,
            )
            .0 as usize
                > 32
        };
        if ok {
            self.win.close();
        } else {
            self.say(tr("Windows did not give administrator rights."), cx);
        }
    }

    // ───────────────────────── outside ─────────────────────────

    /// Opens a path with Windows (a file with its app, a folder in Explorer).
    pub fn open_path(&mut self, p: &Path, cx: &mut Cx) {
        self.note(format!("open {}", p.display()));
        if cfg!(feature = "test-hook") {
            return;
        }
        if !self.win.open(&p.to_string_lossy()) {
            self.say(tr("Windows could not open it."), cx);
        }
    }

    /// Shows a file or folder in Explorer (selected in its folder).
    pub fn show_in_explorer(&mut self, p: &Path, _cx: &mut Cx) {
        self.note(format!("explorer {}", p.display()));
        if cfg!(feature = "test-hook") {
            return;
        }
        plume_ui::folder::show(p);
    }

    /// Opens a folder in Plume FEx (installed with the suite, or next to this program).
    pub fn open_in_fex(&mut self, p: &Path, cx: &mut Cx) {
        self.note(format!("fex {}", p.display()));
        let exe = [plume_update::install_dir().join("plume-fex.exe"), std::env::current_exe().unwrap_or_default().with_file_name("plume-fex.exe")]
            .into_iter()
            .find(|e| e.is_file());
        let Some(exe) = exe else {
            self.say(tr("Plume FEx is not installed. Install it from Plume Suite."), cx);
            return;
        };
        if cfg!(feature = "test-hook") {
            return;
        }
        let dir = if p.is_dir() { p.to_path_buf() } else { p.parent().map(Path::to_path_buf).unwrap_or_default() };
        let _ = std::process::Command::new(exe).arg(dir).spawn();
    }

    pub fn copy_path(&mut self, p: &Path, cx: &mut Cx) {
        self.win.copy_text(&p.to_string_lossy());
        self.say(tr("Path copied."), cx);
    }

    pub fn settings_changed(&mut self, cx: &mut Cx) {
        crate::i18n::set(&self.set.lang);
        self.pal = Pal::new(dark_for(&self.set.theme));
        self.win.set_dark(self.pal.dark);
        self.set.save();
        cx.repaint();
    }

    // ───────────────────────── events ─────────────────────────

    pub fn act(&mut self, h: Hit, p: Pt, cx: &mut Cx) {
        match h {
            Hit::Min => self.win.minimize(),
            Hit::Max => self.win.toggle_maximize(),
            Hit::Close => self.win.close(),
            Hit::Tab(v) => self.go(v, cx),
            Hit::ScanDrive(i) => {
                if let Some(d) = self.drives.get(i) {
                    let r = d.root.clone();
                    self.start_scan(r, cx);
                }
            }
            Hit::ScanFolder => {
                let owner = self.win.hwnd;
                cx.after(move |w| {
                    if let Some(dir) = plume_ui::dialogs::pick_folder(owner, tr("Choose a folder to scan")) {
                        w.with(|h, cx| {
                            if let Some(app) = with_app(h) {
                                app.start_scan(dir, cx);
                            }
                        });
                    }
                });
            }
            Hit::OpenMap => self.go(View::Map, cx),
            Hit::GoScan => self.go(View::Drives, cx),
            Hit::Up => self.up(cx),
            Hit::Crumb(id) => self.enter(id, cx),
            Hit::Block(id) => {
                let is_dir = self.tree.as_ref().is_some_and(|t| t.node(id).is_dir());
                if is_dir {
                    self.enter(id, cx);
                } else {
                    self.sel = Some(id);
                }
            }
            Hit::Row(i) => {
                if let Some(&id) = self.rows.get(i) {
                    self.sel = Some(id);
                }
            }
            Hit::Mode(m) => {
                self.mode = m;
                self.refresh_rows();
            }
            Hit::Sort(o) => {
                if self.order == o {
                    self.desc = !self.desc;
                } else {
                    self.order = o;
                    self.desc = o != Order::Name;
                }
                self.refresh_rows();
            }
            Hit::Explorer | Hit::Fex => {
                if let Some(t) = self.tree.clone() {
                    let path = t.path(self.sel.unwrap_or(self.cur));
                    if h == Hit::Explorer { self.show_in_explorer(&path, cx) } else { self.open_in_fex(&path, cx) }
                }
            }
            Hit::Rescan => {
                if let Some(t) = self.target.clone() {
                    self.start_scan(t, cx);
                }
            }
            Hit::CancelScan => self.cancel_scan(cx),
            Hit::Tick(i) => {
                if let Some(s) = self.sugg.get_mut(i)
                    && s.blocked.is_none()
                    && s.count > 0
                {
                    s.selected = !s.selected;
                    // ticking a list with nothing picked yet opens it
                    if s.selected && s.kind.review() && !s.items.iter().any(|x| x.selected) {
                        self.review = Some(i);
                        self.review_scroll = 0.0;
                    }
                }
            }
            Hit::Review(i) => {
                self.review = Some(i);
                self.review_scroll = 0.0;
            }
            Hit::ReviewRow(k) => {
                if let Some(s) = self.review.and_then(|i| self.sugg.get_mut(i))
                    && let Some(it) = s.items.get_mut(k)
                {
                    it.selected = !it.selected;
                    s.selected = s.items.iter().any(|x| x.selected);
                }
            }
            Hit::ReviewAll(on) => {
                if let Some(s) = self.review.and_then(|i| self.sugg.get_mut(i)) {
                    for it in s.items.iter_mut() {
                        it.selected = on;
                    }
                    s.selected = on && !s.items.is_empty();
                }
            }
            Hit::KeepOldest => {
                if let Some(s) = self.review.and_then(|i| self.sugg.get_mut(i)) {
                    clean::keep_oldest(&mut s.items);
                    s.selected = s.items.iter().any(|x| x.selected);
                }
            }
            Hit::ReviewDone => self.review = None,
            Hit::FindDupes => self.find_dupes(cx),
            Hit::CancelDupes => {
                if let Some(p) = &self.dup_progress {
                    p.cancel.store(true, Ordering::Relaxed);
                }
            }
            Hit::Elevate => self.modal = Some(Modal::Elevate),
            Hit::Refresh => self.measure(cx),
            Hit::Clean => {
                if self.selected_total() > 0 || self.sugg.iter().any(|s| s.selected && s.kind == Kind::RecycleBin && s.blocked.is_none()) {
                    self.modal = Some(Modal::Confirm);
                }
            }
            Hit::DismissResult => self.result = None,
            Hit::Choice(key, i) => self.choose(key, i, cx),
            Hit::Toggle(key) => {
                if key == "recycle" {
                    self.set.recycle = !self.set.recycle;
                }
                self.settings_changed(cx);
            }
            Hit::MenuItem(i) => {
                if let Some(menu) = self.menu.take()
                    && let Some((_, a)) = menu.items.get(i).cloned()
                {
                    match a {
                        MenuAct::Enter(id) => self.enter(id, cx),
                        MenuAct::Open(p) => self.open_path(&p, cx),
                        MenuAct::Explorer(p) => self.show_in_explorer(&p, cx),
                        MenuAct::Fex(p) => self.open_in_fex(&p, cx),
                        MenuAct::Copy(p) => self.copy_path(&p, cx),
                    }
                }
            }
            Hit::ModalOk => match self.modal.take() {
                Some(Modal::Confirm) => self.start_clean(cx),
                Some(Modal::Elevate) => self.elevate(cx),
                None => {}
            },
            Hit::ModalCancel => self.modal = None,
            Hit::Toast => self.toast = None,
        }
        let _ = p;
        cx.repaint();
    }

    fn choose(&mut self, key: &str, i: usize, cx: &mut Cx) {
        use crate::settings::{DUP_MB, LARGE_MB, OLD_DAYS};
        match key {
            "theme" => self.set.theme = ["system", "dark", "light"][i.min(2)].into(),
            "lang" => self.set.lang = ["system", "en", "fr"][i.min(2)].into(),
            "old_days" => self.set.old_days = OLD_DAYS[i.min(3)],
            "large_mb" => self.set.large_mb = LARGE_MB[i.min(3)],
            "dup_mb" => {
                self.set.dup_mb = DUP_MB[i.min(3)];
                if self.dups == Dups::Done {
                    self.dups = Dups::NotYet;
                }
            }
            _ => {}
        }
        self.settings_changed(cx);
        if matches!(key, "old_days" | "large_mb" | "dup_mb") && !self.sugg.is_empty() {
            self.measure(cx);
        }
    }

    /// The menu of a file or folder of the map.
    pub fn node_menu(&mut self, id: u32, p: Pt) {
        let Some(t) = self.tree.clone() else { return };
        let path = t.path(id);
        let mut items = Vec::new();
        if t.node(id).is_dir() {
            items.push((tr("Open here").to_string(), MenuAct::Enter(id)));
        } else {
            items.push((tr("Open").to_string(), MenuAct::Open(path.clone())));
        }
        items.push((tr("Show in Explorer").to_string(), MenuAct::Explorer(path.clone())));
        items.push((tr("Open in Plume FEx").to_string(), MenuAct::Fex(path.clone())));
        items.push((tr("Copy path").to_string(), MenuAct::Copy(path)));
        self.sel = Some(id);
        self.menu = Some(Menu { x: p.x, y: p.y, items });
    }

    pub fn item_menu(&mut self, k: usize, p: Pt) {
        let Some(it) = self.review.and_then(|i| self.sugg.get(i)).and_then(|s| s.items.get(k)) else { return };
        let path = it.path.clone();
        self.menu = Some(Menu {
            x: p.x,
            y: p.y,
            items: vec![
                (tr("Open").to_string(), MenuAct::Open(path.clone())),
                (tr("Show in Explorer").to_string(), MenuAct::Explorer(path.clone())),
                (tr("Copy path").to_string(), MenuAct::Copy(path)),
            ],
        });
    }

    fn busy(&self) -> bool {
        self.scanning.is_some() || self.dups == Dups::Running || self.cleaning || self.measuring || self.toast.is_some()
    }

    fn tick(&mut self, cx: &mut Cx) {
        if let Some((_, t)) = &self.toast
            && t.elapsed() > Duration::from_secs(4)
        {
            self.toast = None;
        }
        if !self.busy() {
            cx.timer(TICK, None);
        }
        cx.repaint();
    }

    fn wheel(&mut self, p: Pt, dy: f32, cx: &mut Cx) {
        let step = dy * ROW_H * 3.0;
        if self.review.is_some() && self.review_rect.contains(p) {
            self.review_scroll = (self.review_scroll - step).max(0.0);
        } else if self.view == View::Map && self.list_rect.contains(p) {
            self.list_scroll = (self.list_scroll - step).max(0.0);
        } else if self.view == View::Cleanup && self.clean_rect.contains(p) {
            self.clean_scroll = (self.clean_scroll - step).max(0.0);
        }
        cx.repaint();
    }

    fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        match vk {
            0x1B => {
                if self.menu.take().is_none() && self.modal.take().is_none() {
                    self.review = None;
                }
            }
            0x08 if self.view == View::Map => self.up(cx),
            0x25 if mods.alt && self.view == View::Map => self.up(cx),
            0x74 => match self.view {
                View::Map => self.act(Hit::Rescan, Pt::default(), cx),
                View::Cleanup => self.measure(cx),
                View::Drives => self.drives = drives::list(),
                View::Settings => {}
            },
            0x0D if self.modal.is_some() => self.act(Hit::ModalOk, Pt::default(), cx),
            0x31..=0x33 if mods.ctrl => self.go([View::Drives, View::Map, View::Cleanup][(vk - 0x31) as usize], cx),
            0xBC if mods.ctrl => self.go(View::Settings, cx),
            _ => return,
        }
        cx.handled();
        cx.repaint();
    }

    fn handle(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                let h = self.hits.at(p).cloned();
                if h != self.hover {
                    self.hover = h;
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                self.hover = None;
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Left, clicks, .. } => {
                let h = self.hits.at(p).cloned();
                if self.menu.is_some() && !matches!(h, Some(Hit::MenuItem(_))) {
                    self.menu = None;
                    cx.repaint();
                    return;
                }
                if clicks == 2 {
                    match h {
                        Some(Hit::Row(i)) => {
                            if let Some(&id) = self.rows.get(i) {
                                let dir = self.tree.as_ref().is_some_and(|t| t.node(id).is_dir());
                                if dir {
                                    self.enter(id, cx);
                                } else if let Some(t) = self.tree.clone() {
                                    self.show_in_explorer(&t.path(id), cx);
                                }
                            }
                            self.pressed = None;
                            return;
                        }
                        Some(Hit::Block(id)) => {
                            if let Some(t) = self.tree.clone()
                                && !t.node(id).is_dir()
                            {
                                self.show_in_explorer(&t.path(id), cx);
                            }
                            self.pressed = None;
                            return;
                        }
                        _ => {}
                    }
                }
                self.pressed = h;
                cx.capture(true);
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                let up = self.hits.at(p).cloned();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    self.act(a, p, cx);
                }
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Right, .. } => {
                match self.hits.at(p).cloned() {
                    Some(Hit::Row(i)) => {
                        if let Some(&id) = self.rows.get(i) {
                            self.node_menu(id, p);
                        }
                    }
                    Some(Hit::Block(id)) => self.node_menu(id, p),
                    Some(Hit::ReviewRow(k)) => self.item_menu(k, p),
                    _ => self.menu = None,
                }
                cx.repaint();
            }
            Event::MouseDown { button: Button::Back, .. } => self.up(cx),
            Event::Wheel { p, dy, .. } => self.wheel(p, dy, cx),
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::Timer(TICK) => self.tick(cx),
            Event::Opened(text) => {
                self.win.bring_to_front();
                if text.contains("--cleanup") {
                    self.go(View::Cleanup, cx);
                }
            }
            Event::Close => {
                if let Some(p) = &self.scanning {
                    p.cancel.store(true, Ordering::Relaxed);
                }
                if let Some(p) = &self.dup_progress {
                    p.cancel.store(true, Ordering::Relaxed);
                }
                self.set.save();
            }
            Event::SystemTheme => {
                if self.set.theme == "system" {
                    self.pal = Pal::new(dark_for("system"));
                    self.win.set_dark(self.pal.dark);
                }
                cx.repaint();
            }
            Event::Devices => {
                self.drives = drives::list();
                cx.repaint();
            }
            Event::Resized | Event::Scale | Event::Active(_) => cx.repaint(),
            _ => {}
        }
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_all(c, cx);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        self.handle(ev, cx);
        if self.busy() {
            cx.timer(TICK, Some(100));
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(Hit::Min | Hit::Close) => Area::Control,
            Some(_) => Area::Client,
            None if p.y < TOP_H && self.modal.is_none() => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Min | Hit::Max | Hit::Close) | None => Cursor::Arrow,
            Some(_) => Cursor::Hand,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

/// A note on what blocks a suggestion, for the card.
pub fn blocked_text(b: &Blocked) -> String {
    match b {
        Blocked::NeedScan => tr("Scan a drive first").to_string(),
        Blocked::Close(name) => fill(tr("Close {name} first"), &[name]),
        Blocked::Admin => tr("Needs administrator").to_string(),
    }
}
