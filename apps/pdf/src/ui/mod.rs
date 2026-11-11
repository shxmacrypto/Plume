//! Plume PDF's window. Everything is drawn by hand (Direct2D): the tabs in the title bar, the tools, the page
//! thumbnails or the outline on the left, the pages on a calm grey ground, and over them search matches, form
//! fields and the annotations being made. Pages are drawn by Windows on the render thread and kept as pixels
//! at the zoom in use (sharp again once the zoom settles). Edits go through `edit` (lopdf); nothing is written
//! to disk without being asked, and the original file only after a confirmation.

pub mod draw;
mod paint;
mod settings_view;

use crate::{
    edit::{self, Annot, Field, FieldValue, Kind, OutlineItem, PageGeom},
    i18n::{fill, tr},
    render::{Job, LoadFail, Pixels, Renderer, Reply},
    settings::{Memory, Settings, Signature},
    text::{self, Found, PageText},
};
use lopdf::{Document, ObjectId};
use plume_ui::{Area, Button, Canvas, Color, Cursor, Cx, Event, Gfx, Handler, Hits, Mods, PaintCx, Pt, Rect, Window, pt, rect};
use std::{
    any::Any,
    cell::Cell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use windows::Win32::Graphics::Imaging::IWICImagingFactory;

pub const TITLE_H: f32 = 40.0;
pub const BAR_H: f32 = 48.0;
pub const SIDE_W: f32 = 212.0;
pub const THUMB_W: f32 = 112.0;
pub const GAP: f32 = 14.0;
pub const MARGIN: f32 = 24.0;
/// Points → DIPs at 100 %.
pub const PT: f32 = 96.0 / 72.0;

pub const SETTLE: usize = 1;
pub const TICK: usize = 2;
pub const TYPED: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Hand,
    Highlight,
    Note,
    Ink,
    Text,
    Sign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Width,
    Page,
    Free,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Loading,
    Password { wrong: bool },
    Ready,
    Failed(String),
}

#[derive(Clone)]
pub enum Undo {
    Annots(Vec<Annot>),
    Fields(HashMap<ObjectId, FieldValue>),
    Doc(Box<Document>, Vec<Annot>, HashMap<ObjectId, FieldValue>),
}

pub struct Tab {
    pub path: Option<PathBuf>,
    pub name: String,
    /// Changes with every new version of the pages (render cache key).
    pub ver: u64,
    pub bytes: Arc<Vec<u8>>,
    /// Password the bytes need (the original file's; versions made here are kept unprotected in memory).
    pub render_pw: Option<String>,
    /// The file's password: a saved copy is protected with it again.
    pub password: Option<String>,
    pub doc: Option<Document>,
    /// lopdf could not read it: the pages show, but nothing can be changed.
    pub readonly: Option<String>,
    pub status: Status,
    pub geoms: Vec<PageGeom>,
    /// Page sizes from Windows (points), used until (or if) lopdf has read the file.
    pub win_sizes: Vec<(f32, f32)>,
    pub texts: Vec<PageText>,
    pub texts_ready: bool,
    pub outline: Vec<OutlineItem>,
    pub fields: Vec<Field>,
    pub values: HashMap<ObjectId, FieldValue>,
    pub annots: Vec<Annot>,
    pub undo: Vec<Undo>,
    pub dirty: bool,
    pub zoom: f32,
    pub fit: Fit,
    pub sx: f32,
    pub sy: f32,
    /// Page rectangles in content coordinates (DIP), and the content's size.
    pub rects: Vec<Rect>,
    pub content: (f32, f32),
    pub sel: Vec<usize>,
    pub found: Vec<Found>,
    pub found_i: usize,
    pub restore: Option<usize>,
}

impl Tab {
    fn new(path: Option<PathBuf>, name: String, bytes: Vec<u8>, ver: u64, fit: Fit) -> Tab {
        Tab {
            path,
            name,
            ver,
            bytes: Arc::new(bytes),
            render_pw: None,
            password: None,
            doc: None,
            readonly: None,
            status: Status::Loading,
            geoms: Vec::new(),
            win_sizes: Vec::new(),
            texts: Vec::new(),
            texts_ready: false,
            outline: Vec::new(),
            fields: Vec::new(),
            values: HashMap::new(),
            annots: Vec::new(),
            undo: Vec::new(),
            dirty: false,
            zoom: 1.0,
            fit,
            sx: 0.0,
            sy: 0.0,
            rects: Vec::new(),
            content: (0.0, 0.0),
            sel: Vec::new(),
            found: Vec::new(),
            found_i: 0,
            restore: None,
        }
    }

    /// Displayed page sizes in points.
    pub fn sizes(&self) -> Vec<(f32, f32)> {
        if !self.geoms.is_empty() && (self.win_sizes.is_empty() || self.win_sizes.len() == self.geoms.len()) {
            self.geoms.iter().map(|g| g.size()).collect()
        } else {
            self.win_sizes.clone()
        }
    }

    pub fn geom(&self, page: usize) -> PageGeom {
        if let Some(g) = self.geoms.get(page) {
            return *g;
        }
        let (w, h) = self.win_sizes.get(page).copied().unwrap_or((612.0, 792.0));
        PageGeom { x0: 0.0, y0: 0.0, x1: w, y1: h, rotate: 0 }
    }

    pub fn pages(&self) -> usize {
        self.sizes().len()
    }

    pub fn k(&self) -> f32 {
        self.zoom * PT
    }

    pub fn editable(&self) -> bool {
        self.doc.is_some() && self.status == Status::Ready
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Tab(usize),
    TabClose(usize),
    NewTab,
    Btn(&'static str),
    Tool(Tool),
    Swatch(usize),
    PageBox,
    SideTab(u8),
    Thumb(usize),
    Outline(usize),
    Page(usize),
    Annot(usize),
    Handle(usize),
    Field(usize),
    SearchBox,
    Menu(usize),
    ModalOk,
    ModalAlt,
    ModalCancel,
    ModalField,
    Pad,
    Recent(usize),
    RecentForget(usize),
    /// The dimmed window behind a dialog (catches clicks).
    Shade,
    Choice(&'static str, usize),
    Toggle(&'static str),
    VScroll,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Drag {
    Pan { start: Pt, sx: f32, sy: f32 },
    VScroll { start: f32, sy: f32 },
    Thumb { from: usize, start: Pt, active: bool, to: usize },
    Highlight { page: usize, a: (f32, f32), b: (f32, f32) },
    Ink { page: usize, pts: Vec<(f32, f32)> },
    TextBox { page: usize, a: (f32, f32), b: (f32, f32) },
    Click { page: usize, at: (f32, f32) },
    Move { i: usize, start: Pt, orig: [f32; 4] },
    Resize { i: usize, start: Pt, orig: [f32; 4] },
    Pad,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modal {
    Password { wrong: bool },
    ConfirmSave,
    ConfirmClose(usize),
    ConfirmQuit,
    Split,
    NoteText { i: usize, new: bool },
    TextBox { i: usize, new: bool },
    Signature { strokes: Vec<Vec<(f32, f32)>>, pad: Rect },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditFor {
    Search,
    PageBox,
    Modal,
    Field(usize),
}

pub struct Menu {
    pub x: f32,
    pub y: f32,
    pub items: Vec<(String, &'static str)>,
    /// Opened by the toolbar's "more" button (it shows as pressed).
    pub more: bool,
}

/// Colors of Plume PDF.
#[derive(Clone)]
pub struct Pal {
    pub dark: bool,
    pub chrome: Color,
    pub side: Color,
    pub area: Color,
    pub card: Color,
    pub line: Color,
    pub t1: Color,
    pub t2: Color,
    pub t3: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub paper_dark: Color,
}

impl Pal {
    pub fn new(dark: bool) -> Pal {
        if dark {
            Pal {
                dark,
                chrome: Color::hex(0x1b1b1e),
                side: Color::hex(0x18181b),
                area: Color::hex(0x26262a),
                card: Color::hex(0x2c2c30),
                line: Color::hexa(0xffffff, 0.08),
                t1: Color::hex(0xffffff),
                t2: Color::hexa(0xebebf5, 0.72),
                t3: Color::hexa(0xebebf5, 0.45),
                accent: Color::hex(0xff5a5f),
                on_accent: Color::hex(0xffffff),
                paper_dark: Color::hex(0x1e1e1e),
            }
        } else {
            Pal {
                dark,
                chrome: Color::hex(0xf6f6f8),
                side: Color::hex(0xefeff2),
                area: Color::hex(0xe2e2e7),
                card: Color::hex(0xffffff),
                line: Color::hexa(0x000000, 0.09),
                t1: Color::hex(0x111114),
                t2: Color::hexa(0x3c3c43, 0.82),
                t3: Color::hexa(0x3c3c43, 0.55),
                accent: Color::hex(0xd62839),
                on_accent: Color::hex(0xffffff),
                paper_dark: Color::hex(0x1e1e1e),
            }
        }
    }
}

/// Annotation colors offered: highlight, ink and text.
pub const HL_COLORS: [[f32; 3]; 4] = [[1.0, 0.86, 0.2], [0.55, 0.9, 0.45], [0.45, 0.8, 1.0], [1.0, 0.55, 0.75]];
pub const INK_COLORS: [[f32; 3]; 4] = [[0.08, 0.2, 0.55], [0.1, 0.1, 0.1], [0.85, 0.15, 0.2], [0.1, 0.55, 0.3]];
pub const NOTE_COLOR: [f32; 3] = [1.0, 0.83, 0.3];

struct PixEntry {
    px: Rc<Pixels>,
    used: u64,
}

pub struct App {
    pub win: Window,
    #[allow(dead_code)]
    pub gfx: Rc<Gfx>,
    pub wic: IWICImagingFactory,
    pub pal: Pal,
    pub set: Settings,
    pub mem: Memory,
    pub sig: Option<Signature>,
    pub renderer: Renderer,
    pub tabs: Vec<Tab>,
    pub cur: usize,
    pub next_gen: u64,
    pix: HashMap<Job, PixEntry>,
    pix_bytes: usize,
    pub inflight: HashSet<Job>,
    uploaded: HashMap<u64, u64>,
    pub frame: u64,
    pub view: Rect,
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub drag: Option<Drag>,
    pub tool: Tool,
    pub hl_color: usize,
    pub ink_color: usize,
    pub selected: Option<usize>,
    pub side_tab: u8,
    pub side_scroll: f32,
    pub side_h: f32,
    pub thumb_rects: Vec<(usize, Rect)>,
    pub search_open: bool,
    pub query: String,
    pub edit_for: Option<EditFor>,
    pub edit_shown: Option<EditFor>,
    pub edit_rect: Option<Rect>,
    pub modal: Option<Modal>,
    pub modal_text: String,
    pub menu: Option<Menu>,
    pub toast: Option<(String, Instant)>,
    pub settings_open: bool,
    pub zoom_changed: Instant,
    pub dirty_flag: Rc<Cell<bool>>,
    pub force_close: Rc<Cell<bool>>,
    pub log: Vec<String>,
    pub page_box_text: String,
    pub page_box: Rect,
    pub pending_thumbs: Vec<Job>,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "PDF".into())
}

fn job_key(j: &Job) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    j.hash(&mut h);
    h.finish()
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, dark_system: bool) -> App {
        let set = Settings::load();
        crate::i18n::set(&set.lang);
        let dark = match set.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => dark_system,
        };
        let sender = win.sender();
        let renderer = Renderer::start(move |r: Reply| {
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.on_render(r, cx);
                }
            });
        });
        let wic = gfx.wic.clone();
        let dirty_flag = Rc::new(Cell::new(false));
        let force_close = Rc::new(Cell::new(false));
        install_hook(&win, dirty_flag.clone(), force_close.clone());
        win.set_dark(dark);
        win.timer(TICK, Some(500));
        App {
            win,
            gfx,
            wic,
            pal: Pal::new(dark),
            set,
            mem: Memory::load(),
            sig: Signature::load(),
            renderer,
            tabs: Vec::new(),
            cur: 0,
            next_gen: 1,
            pix: HashMap::new(),
            pix_bytes: 0,
            inflight: HashSet::new(),
            uploaded: HashMap::new(),
            frame: 0,
            view: rect(0.0, 0.0, 800.0, 600.0),
            hits: Hits::default(),
            hover: None,
            pressed: None,
            drag: None,
            tool: Tool::Hand,
            hl_color: 0,
            ink_color: 0,
            selected: None,
            side_tab: 0,
            side_scroll: 0.0,
            side_h: 0.0,
            thumb_rects: Vec::new(),
            search_open: false,
            query: String::new(),
            edit_for: None,
            edit_shown: None,
            edit_rect: None,
            modal: None,
            modal_text: String::new(),
            menu: None,
            toast: None,
            settings_open: false,
            zoom_changed: Instant::now() - Duration::from_secs(5),
            dirty_flag,
            force_close,
            log: Vec::new(),
            page_box_text: String::new(),
            page_box: Rect::default(),
            pending_thumbs: Vec::new(),
        }
    }

    pub fn note(&mut self, s: impl Into<String>) {
        if self.log.len() > 300 {
            self.log.remove(0);
        }
        self.log.push(s.into());
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        let t = text.into();
        self.note(format!("toast: {t}"));
        self.toast = Some((t, Instant::now()));
        cx.repaint();
    }

    pub fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.cur)
    }

    pub fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.cur)
    }

    fn new_gen(&mut self) -> u64 {
        self.next_gen += 1;
        self.next_gen
    }

    fn sync_dirty(&self) {
        self.dirty_flag.set(self.tabs.iter().any(|t| t.dirty));
    }

    pub fn set_theme(&mut self) {
        let dark = match self.set.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => crate::system_dark(),
        };
        self.pal = Pal::new(dark);
        self.win.set_dark(dark);
    }

    pub fn settings_changed(&mut self, cx: &mut Cx) {
        crate::i18n::set(&self.set.lang);
        self.set_theme();
        self.set.save();
        cx.repaint();
    }

    fn default_fit(&self) -> (Fit, f32) {
        match self.set.zoom.as_str() {
            "page" => (Fit::Page, 1.0),
            "100" => (Fit::Free, 1.0),
            _ => (Fit::Width, 1.0),
        }
    }

    // ───────────────────────── opening ─────────────────────────

    /// Files given on the command line, dropped on the window or chosen: each in its own tab.
    pub fn open_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Cx) {
        for p in paths {
            self.open_path(p, cx);
        }
    }

    pub fn open_path(&mut self, p: PathBuf, cx: &mut Cx) {
        if let Some(i) = self.tabs.iter().position(|t| t.path.as_deref() == Some(p.as_path())) {
            self.switch_to(i, cx);
            return;
        }
        let bytes = match std::fs::read(&p) {
            Ok(b) => b,
            Err(_) => {
                self.say(fill(tr("Could not open “{name}”."), &[&file_name(&p)]), cx);
                return;
            }
        };
        let ver = self.new_gen();
        let (fit, zoom) = self.default_fit();
        let mut t = Tab::new(Some(p.clone()), file_name(&p), bytes, ver, fit);
        t.zoom = zoom;
        if self.set.remember_page {
            t.restore = self.mem.pages.get(&Memory::key(&p)).copied().filter(|&n| n > 0);
        }
        self.mem.opened(&p);
        self.mem.save();
        self.note(format!("open {}", p.display()));
        self.tabs.push(t);
        self.settings_open = false;
        self.switch_to(self.tabs.len() - 1, cx);
        self.parse(ver, None, false);
    }

    /// A document made here (merge): a new tab, not saved yet.
    fn open_doc(&mut self, name: String, doc: Document, cx: &mut Cx) {
        let ver = self.new_gen();
        let (fit, zoom) = self.default_fit();
        let mut t = Tab::new(None, name, Vec::new(), ver, fit);
        t.zoom = zoom;
        t.doc = Some(doc);
        t.dirty = true;
        t.status = Status::Ready;
        self.tabs.push(t);
        self.cur = self.tabs.len() - 1;
        self.settings_open = false;
        self.reload_tab(cx);
        self.sync_dirty();
    }

    pub fn switch_to(&mut self, i: usize, cx: &mut Cx) {
        if i >= self.tabs.len() {
            return;
        }
        self.cur = i;
        self.selected = None;
        self.menu = None;
        self.drag = None;
        let t = &self.tabs[i];
        self.renderer.load(t.ver, t.bytes.clone(), t.render_pw.clone());
        let status = t.status.clone();
        if self.search_open && !self.query.is_empty() {
            self.run_search(false, cx);
        }
        if let Status::Password { wrong } = status {
            self.ask_password(wrong, cx);
        } else if matches!(self.modal, Some(Modal::Password { .. })) {
            self.close_modal(cx);
        }
        cx.repaint();
    }

    /// Reads the file with lopdf in the background (for editing, text and outline). `text_only`: the document is
    /// already here (a change made in the window), only its text is read again.
    fn parse(&mut self, ver: u64, pw: Option<String>, text_only: bool) {
        let Some(t) = self.tabs.iter().find(|t| t.ver == ver) else { return };
        let bytes = t.bytes.clone();
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let r = edit::load(&bytes, pw.as_deref()).map(|doc| {
                let texts = text::all(&doc);
                let geoms = edit::geometries(&doc);
                let outline = edit::outline(&doc);
                let fields = edit::fields(&doc);
                (doc, geoms, texts, outline, fields)
            });
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.parsed(ver, r, text_only, cx);
                }
            });
        });
    }

    #[allow(clippy::type_complexity)]
    fn parsed(
        &mut self,
        ver: u64,
        r: Result<(Document, Vec<PageGeom>, Vec<PageText>, Vec<OutlineItem>, Vec<Field>), edit::LoadError>,
        text_only: bool,
        cx: &mut Cx,
    ) {
        let Some(i) = self.tabs.iter().position(|t| t.ver == ver) else { return };
        let t = &mut self.tabs[i];
        match r {
            Ok((doc, geoms, texts, outline, fields)) => {
                t.texts = texts;
                t.texts_ready = true;
                if !text_only {
                    t.doc = Some(doc);
                    t.geoms = geoms;
                    t.outline = outline;
                    t.fields = fields;
                    t.readonly = None;
                }
                let n = t.texts.len();
                self.note(format!("parsed ver {ver}: {n} pages"));
            }
            Err(edit::LoadError::Password) => {}
            Err(edit::LoadError::Other(e)) => {
                t.readonly = Some(e.clone());
                t.texts_ready = true;
                self.note(format!("lopdf: {e}"));
            }
        }
        self.restore_page(i);
        if i == self.cur && self.search_open && !self.query.is_empty() {
            self.run_search(false, cx);
        }
        cx.repaint();
    }

    fn restore_page(&mut self, i: usize) {
        let t = &mut self.tabs[i];
        if t.status != Status::Ready {
            return;
        }
        if let Some(p) = t.restore.take() {
            let rects = layout_rects(t, self.view.w, self.view.h);
            if let Some(r) = rects.get(p) {
                t.sy = r.y - 8.0;
            }
        }
    }

    fn on_render(&mut self, r: Reply, cx: &mut Cx) {
        match r {
            Reply::Loaded { ver, result } => {
                let Some(i) = self.tabs.iter().position(|t| t.ver == ver) else { return };
                let t = &mut self.tabs[i];
                match result {
                    Ok(sizes) => {
                        t.win_sizes = sizes;
                        t.status = Status::Ready;
                        if i == self.cur && matches!(self.modal, Some(Modal::Password { .. })) {
                            self.close_modal(cx);
                        }
                        self.restore_page(i);
                    }
                    Err(LoadFail::Password) => {
                        let wrong = t.password.is_some();
                        t.status = Status::Password { wrong };
                        if i == self.cur {
                            self.ask_password(wrong, cx);
                        }
                    }
                    Err(LoadFail::Other(e)) => {
                        t.status = Status::Failed(e.clone());
                        self.note(format!("render load failed: {e}"));
                    }
                }
                cx.repaint();
            }
            Reply::Page { job, px } => {
                self.inflight.remove(&job);
                self.pix_bytes += px.data.len();
                if let Some(old) = self.pix.insert(job, PixEntry { px: Rc::new(px), used: self.frame }) {
                    self.pix_bytes -= old.px.data.len();
                }
                cx.repaint();
            }
            Reply::Failed { job } => {
                self.inflight.remove(&job);
            }
        }
    }

    fn ask_password(&mut self, wrong: bool, cx: &mut Cx) {
        self.modal = Some(Modal::Password { wrong });
        self.modal_text.clear();
        self.edit_for = Some(EditFor::Modal);
        self.edit_shown = None;
        cx.repaint();
    }

    fn give_password(&mut self, pw: String, cx: &mut Cx) {
        let Some(t) = self.tabs.get_mut(self.cur) else { return };
        t.password = Some(pw.clone());
        t.render_pw = Some(pw.clone());
        t.status = Status::Loading;
        let ver = t.ver;
        self.renderer.load(ver, t.bytes.clone(), Some(pw.clone()));
        self.parse(ver, Some(pw), false);
        self.close_modal(cx);
    }

    pub fn close_tab(&mut self, i: usize, force: bool, cx: &mut Cx) {
        if i >= self.tabs.len() {
            return;
        }
        if self.tabs[i].dirty && !force {
            self.modal = Some(Modal::ConfirmClose(i));
            cx.repaint();
            return;
        }
        self.remember_page_of(i);
        let ver = self.tabs[i].ver;
        self.tabs.remove(i);
        self.drop_pixels(|j| j.ver == ver);
        if self.cur >= self.tabs.len() {
            self.cur = self.tabs.len().saturating_sub(1);
        } else if i < self.cur {
            self.cur -= 1;
        }
        if !self.tabs.is_empty() {
            self.switch_to(self.cur, cx);
        }
        self.sync_dirty();
        cx.repaint();
    }

    fn remember_page_of(&mut self, i: usize) {
        let page = self.current_page_of(i);
        if let Some(p) = self.tabs[i].path.clone() {
            self.mem.pages.insert(Memory::key(&p), page);
            if self.mem.pages.len() > 400 {
                let k = self.mem.pages.keys().next().cloned().unwrap_or_default();
                self.mem.pages.remove(&k);
            }
        }
    }

    fn drop_pixels(&mut self, f: impl Fn(&Job) -> bool) {
        let gone: Vec<Job> = self.pix.keys().filter(|j| f(j)).copied().collect();
        for j in gone {
            if let Some(e) = self.pix.remove(&j) {
                self.pix_bytes -= e.px.data.len();
            }
        }
    }

    // ───────────────────────── layout, zoom, pages ─────────────────────────

    /// Page rectangles for the current zoom (and the zoom itself, for the fitting modes).
    pub fn relayout(&mut self) {
        let (w, h) = (self.view.w, self.view.h);
        if let Some(t) = self.tabs.get_mut(self.cur) {
            fit_zoom(t, w, h);
            t.rects = layout_rects(t, w, h);
            t.content = (t.rects.iter().map(|r| r.right()).fold(0.0, f32::max) + MARGIN, t.rects.last().map(|r| r.bottom() + MARGIN).unwrap_or(0.0));
            clamp_scroll(t, w, h);
        }
    }

    pub fn current_page_of(&self, i: usize) -> usize {
        let Some(t) = self.tabs.get(i) else { return 0 };
        let y = t.sy + self.view.h * 0.35;
        t.rects.iter().position(|r| r.bottom() + GAP / 2.0 >= y).unwrap_or(t.rects.len().saturating_sub(1))
    }

    pub fn current_page(&self) -> usize {
        self.current_page_of(self.cur)
    }

    pub fn go_page(&mut self, p: usize, cx: &mut Cx) {
        self.relayout();
        let view_h = self.view.h;
        if let Some(t) = self.tab_mut()
            && let Some(r) = t.rects.get(p.min(t.rects.len().saturating_sub(1)))
        {
            t.sy = r.y - 8.0;
            clamp_scroll(t, 0.0, view_h);
        }
        cx.repaint();
    }

    /// Screen rectangle of page `i`.
    pub fn page_screen(&self, i: usize) -> Option<Rect> {
        let t = self.tab()?;
        t.rects.get(i).map(|r| rect(r.x - t.sx + self.view.x, r.y - t.sy + self.view.y, r.w, r.h))
    }

    /// The page under a screen point, and the point in that page's view points.
    pub fn page_at(&self, p: Pt) -> Option<(usize, (f32, f32))> {
        let t = self.tab()?;
        let k = t.k();
        (0..t.rects.len()).find_map(|i| {
            let r = self.page_screen(i)?;
            r.contains(p).then(|| (i, ((p.x - r.x) / k, (p.y - r.y) / k)))
        })
    }

    /// Page `i`'s view points → screen.
    pub fn to_screen(&self, i: usize, x: f32, y: f32) -> Pt {
        let k = self.tab().map(|t| t.k()).unwrap_or(1.0);
        let r = self.page_screen(i).unwrap_or_default();
        pt(r.x + x * k, r.y + y * k)
    }

    pub fn view_rect_screen(&self, i: usize, v: [f32; 4]) -> Rect {
        let k = self.tab().map(|t| t.k()).unwrap_or(1.0);
        let p = self.to_screen(i, v[0], v[1]);
        rect(p.x, p.y, v[2] * k, v[3] * k)
    }

    pub fn zoom_to(&mut self, z: f32, anchor: Option<Pt>, cx: &mut Cx) {
        self.relayout();
        let a = anchor.unwrap_or(pt(self.view.x + self.view.w / 2.0, self.view.y + self.view.h / 2.0));
        let under = self.page_at(a).or_else(|| {
            let i = self.current_page();
            let r = self.page_screen(i)?;
            let k = self.tab()?.k();
            Some((i, ((a.x - r.x) / k, (a.y - r.y) / k)))
        });
        let view = self.view;
        let Some(t) = self.tab_mut() else { return };
        t.zoom = z.clamp(0.1, 8.0);
        t.fit = Fit::Free;
        t.rects = layout_rects(t, view.w, view.h);
        t.content = (t.rects.iter().map(|r| r.right()).fold(0.0, f32::max) + MARGIN, t.rects.last().map(|r| r.bottom() + MARGIN).unwrap_or(0.0));
        if let Some((i, (x, y))) = under
            && let Some(r) = t.rects.get(i)
        {
            let k = t.zoom * PT;
            t.sx = r.x + x * k - (a.x - view.x);
            t.sy = r.y + y * k - (a.y - view.y);
        }
        clamp_scroll(t, view.w, view.h);
        self.zoom_changed = Instant::now();
        cx.timer(SETTLE, Some(180));
        cx.repaint();
    }

    pub fn set_fit(&mut self, fit: Fit, cx: &mut Cx) {
        let p = self.current_page();
        if let Some(t) = self.tab_mut() {
            t.fit = fit;
        }
        self.zoom_changed = Instant::now();
        cx.timer(SETTLE, Some(180));
        self.go_page(p, cx);
    }

    pub fn zoom_step(&mut self, up: bool, anchor: Option<Pt>, cx: &mut Cx) {
        const STEPS: [f32; 15] = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0, 6.0];
        let z = self.tab().map(|t| t.zoom).unwrap_or(1.0);
        let next = if up {
            STEPS.iter().copied().find(|s| *s > z + 0.001).unwrap_or(8.0)
        } else {
            STEPS.iter().rev().copied().find(|s| *s < z - 0.001).unwrap_or(0.1)
        };
        self.zoom_to(next, anchor, cx);
    }

    pub fn scroll_by(&mut self, dx: f32, dy: f32, cx: &mut Cx) {
        let view = self.view;
        if let Some(t) = self.tab_mut() {
            t.sx += dx;
            t.sy += dy;
            clamp_scroll(t, view.w, view.h);
        }
        cx.repaint();
    }

    // ───────────────────────── pixels ─────────────────────────

    pub fn pixels(&mut self, job: &Job) -> Option<Rc<Pixels>> {
        let frame = self.frame;
        self.pix.get_mut(job).map(|e| {
            e.used = frame;
            e.px.clone()
        })
    }

    /// The closest picture of the same page already drawn (shown stretched while the right one comes).
    pub fn fallback(&mut self, job: &Job) -> Option<(Job, Rc<Pixels>)> {
        let frame = self.frame;
        let best = self
            .pix
            .iter()
            .filter(|(j, _)| j.ver == job.ver && j.page == job.page && j.dark == job.dark)
            .min_by_key(|(j, _)| (j.w as i64 - job.w as i64).unsigned_abs())
            .map(|(j, _)| *j)?;
        let e = self.pix.get_mut(&best)?;
        e.used = frame;
        Some((best, e.px.clone()))
    }

    /// Draws a page picture (uploaded to the graphics card once, forgotten when off screen).
    pub fn blit(&mut self, c: &mut Canvas, job: &Job, px: &Pixels, q: Rect) {
        let key = job_key(job);
        self.uploaded.insert(key, self.frame);
        let wic = self.wic.clone();
        c.image(key, q, 1.0, || unsafe {
            use windows::{Win32::Graphics::Imaging::GUID_WICPixelFormat32bppPBGRA, core::Interface};
            wic.CreateBitmapFromMemory(px.w, px.h, &GUID_WICPixelFormat32bppPBGRA, px.w * 4, &px.data).ok().and_then(|b| b.cast().ok())
        });
    }

    /// Asks for the pages wanted now (most wanted last: it is drawn first) and drops the others.
    pub fn want(&mut self, wanted: Vec<Job>) {
        let set: HashSet<Job> = wanted.iter().copied().collect();
        self.renderer.keep_only(|j| set.contains(j));
        self.inflight.retain(|j| set.contains(j));
        for j in wanted {
            if !self.pix.contains_key(&j) && !self.inflight.contains(&j) {
                self.inflight.insert(j);
                self.renderer.request(j);
            }
        }
    }

    /// End of a frame: pictures not on screen leave the graphics card; old pixels go when memory is full.
    pub fn tidy(&mut self, c: &mut Canvas) {
        let frame = self.frame;
        let old: Vec<u64> = self.uploaded.iter().filter(|(_, f)| **f + 1 < frame).map(|(k, _)| *k).collect();
        for k in old {
            c.forget_image(k);
            self.uploaded.remove(&k);
        }
        const BUDGET: usize = 384 << 20;
        if self.pix_bytes > BUDGET {
            let mut by_age: Vec<(u64, Job)> = self.pix.iter().map(|(j, e)| (e.used, *j)).collect();
            by_age.sort_by_key(|x| x.0);
            for (used, j) in by_age {
                if self.pix_bytes <= BUDGET * 3 / 4 || used + 1 >= frame {
                    break;
                }
                if let Some(e) = self.pix.remove(&j) {
                    self.pix_bytes -= e.px.data.len();
                    c.forget_image(job_key(&j));
                    self.uploaded.remove(&job_key(&j));
                }
            }
        }
    }

    // ───────────────────────── changes ─────────────────────────

    /// A new version of the current tab's document: rendered again, text read again.
    fn reload_tab(&mut self, cx: &mut Cx) {
        let ver = self.new_gen();
        let Some(t) = self.tabs.get_mut(self.cur) else { return };
        let Some(doc) = t.doc.as_ref() else { return };
        match edit::to_bytes(doc, None) {
            Ok(b) => t.bytes = Arc::new(b),
            Err(e) => {
                self.say(e, cx);
                return;
            }
        }
        let old = t.ver;
        t.ver = ver;
        t.render_pw = None;
        t.geoms = edit::geometries(doc);
        t.outline = edit::outline(doc);
        t.fields = edit::fields(doc);
        t.win_sizes.clear();
        t.texts_ready = false;
        t.found.clear();
        let n = t.geoms.len();
        t.sel.retain(|&p| p < n);
        self.renderer.load(ver, t.bytes.clone(), None);
        self.drop_pixels(|j| j.ver == old);
        self.parse(ver, None, true);
        self.relayout();
        self.sync_dirty();
        cx.repaint();
    }

    /// Changes the document's pages. Annotations and form values being made are written into it first.
    pub fn change(&mut self, what: impl FnOnce(&mut Document) -> edit::Res<()>, cx: &mut Cx) -> bool {
        let Some(t) = self.tabs.get_mut(self.cur) else { return false };
        if let Some(why) = &t.readonly {
            let msg = fill(tr("This file can be read but not changed ({why})."), &[why]);
            self.say(msg, cx);
            return false;
        }
        let Some(doc) = t.doc.as_mut() else {
            self.say(tr("The file is still opening."), cx);
            return false;
        };
        let before = doc.clone();
        let values: Vec<(ObjectId, FieldValue)> = t.values.iter().map(|(k, v)| (*k, v.clone())).collect();
        let r = edit::add_annots(doc, &t.annots).and_then(|_| edit::fill(doc, &values)).and_then(|_| what(doc));
        match r {
            Ok(()) => {
                let annots = std::mem::take(&mut t.annots);
                let vals = std::mem::take(&mut t.values);
                t.undo.push(Undo::Doc(Box::new(before), annots, vals));
                if t.undo.len() > 30 {
                    t.undo.remove(0);
                }
                t.dirty = true;
                self.selected = None;
                self.reload_tab(cx);
                true
            }
            Err(e) => {
                *doc = before;
                self.say(e, cx);
                false
            }
        }
    }

    /// The pages an action applies to: the ones selected in the sidebar, else the current one.
    pub fn target_pages(&self) -> Vec<usize> {
        match self.tab() {
            Some(t) if !t.sel.is_empty() => {
                let mut s = t.sel.clone();
                s.sort_unstable();
                s
            }
            _ => vec![self.current_page()],
        }
    }

    pub fn rotate(&mut self, delta: i32, cx: &mut Cx) {
        let pages = self.target_pages();
        if self.change(|d| edit::rotate_pages(d, &pages, delta), cx) {
            self.note(format!("rotated {pages:?} by {delta}"));
        }
    }

    pub fn delete_pages(&mut self, cx: &mut Cx) {
        let pages = self.target_pages();
        if self.change(|d| edit::delete_pages(d, &pages), cx) {
            let n = pages.len();
            if let Some(t) = self.tab_mut() {
                t.sel.clear();
            }
            self.say(if n == 1 { tr("Page deleted.").to_string() } else { fill(tr("{n} pages deleted."), &[&n.to_string()]) }, cx);
        }
    }

    pub fn move_pages(&mut self, moving: Vec<usize>, to: usize, cx: &mut Cx) {
        let n = self.tab().map(|t| t.pages()).unwrap_or(0);
        let order = edit::moved_order(n, &moving, to);
        if order.iter().enumerate().all(|(i, &o)| i == o) {
            return;
        }
        if self.change(|d| edit::set_order(d, &order), cx) {
            let sel: Vec<usize> = order.iter().enumerate().filter(|(_, o)| moving.contains(o)).map(|(i, _)| i).collect();
            if let Some(t) = self.tab_mut() {
                t.sel = sel;
            }
        }
    }

    /// The current document with what is being made written in (for saving).
    fn finished_doc(&self) -> edit::Res<Document> {
        let t = self.tab().ok_or("")?;
        let mut d = t.doc.clone().ok_or_else(|| tr("This file cannot be changed.").to_string())?;
        edit::add_annots(&mut d, &t.annots)?;
        let values: Vec<(ObjectId, FieldValue)> = t.values.iter().map(|(k, v)| (*k, v.clone())).collect();
        edit::fill(&mut d, &values)?;
        Ok(d)
    }

    /// Saves the current tab to `path` (a copy, or the original after confirmation). The tab then shows that file.
    pub fn save_to(&mut self, path: PathBuf, cx: &mut Cx) -> bool {
        let d = match self.finished_doc() {
            Ok(d) => d,
            Err(e) => {
                self.say(e, cx);
                return false;
            }
        };
        let pw = self.tab().and_then(|t| t.password.clone());
        match edit::save(&d, &path, pw.as_deref()) {
            Ok(()) => {
                self.note(format!("saved {}", path.display()));
                if let Some(t) = self.tab_mut() {
                    t.doc = Some(d);
                    t.annots.clear();
                    t.values.clear();
                    t.dirty = false;
                    t.name = file_name(&path);
                    t.path = Some(path.clone());
                }
                self.mem.opened(&path);
                self.mem.save();
                self.reload_tab(cx);
                self.say(fill(tr("Saved “{name}”."), &[&file_name(&path)]), cx);
                true
            }
            Err(e) => {
                self.say(fill(tr("Could not save: {e}"), &[&e]), cx);
                false
            }
        }
    }

    fn suggested_name(&self, suffix: &str) -> String {
        let base = self.tab().map(|t| t.name.trim_end_matches(".pdf").trim_end_matches(".PDF").to_string()).unwrap_or_else(|| "Document".into());
        format!("{base}{suffix}.pdf")
    }

    pub fn ask_save_as(&mut self, cx: &mut Cx) {
        if !self.tab().is_some_and(Tab::editable) {
            return;
        }
        let name = self.suggested_name(if self.tab().is_some_and(|t| t.path.is_some()) { tr(" (edited)") } else { "" });
        let title = tr("Save a copy").to_string();
        cx.after(move |w| {
            if let Some(p) = plume_ui::dialogs::save_as(w.hwnd, &title, &name, "pdf", &[("PDF", "*.pdf")]) {
                w.with(|h, cx| {
                    if let Some(app) = with_app(h) {
                        app.save_to(p, cx);
                    }
                });
            }
        });
    }

    /// Ctrl+S: a file never saved goes through "Save a copy"; the original is replaced only after a yes.
    pub fn save(&mut self, cx: &mut Cx) {
        let Some(t) = self.tab() else { return };
        if !t.editable() {
            return;
        }
        if t.path.is_none() {
            self.ask_save_as(cx);
        } else if !t.dirty {
            self.say(tr("No changes to save."), cx);
        } else {
            self.modal = Some(Modal::ConfirmSave);
            cx.repaint();
        }
    }

    pub fn ask_open(&mut self, cx: &mut Cx) {
        let title = tr("Open PDF files").to_string();
        cx.after(move |w| {
            let files = plume_ui::dialogs::pick_files(w.hwnd, &title);
            if !files.is_empty() {
                w.with(|h, cx| {
                    if let Some(app) = with_app(h) {
                        app.open_paths(files, cx);
                    }
                });
            }
        });
    }

    pub fn ask_merge(&mut self, cx: &mut Cx) {
        let title = tr("PDF files to add").to_string();
        cx.after(move |w| {
            let files = plume_ui::dialogs::pick_files(w.hwnd, &title);
            if !files.is_empty() {
                w.with(|h, cx| {
                    if let Some(app) = with_app(h) {
                        app.merge_paths(files, cx);
                    }
                });
            }
        });
    }

    /// Adds files at the end of the open document, or merges them into a new one.
    pub fn merge_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Cx) {
        let mut docs = Vec::new();
        for p in &paths {
            let read = std::fs::read(p).map_err(|e| e.to_string()).and_then(|b| {
                edit::load(&b, None).map_err(|e| match e {
                    edit::LoadError::Password => tr("it is protected by a password").to_string(),
                    edit::LoadError::Other(e) => e,
                })
            });
            match read {
                Ok(d) => docs.push(d),
                Err(e) => {
                    self.say(fill(tr("Could not add “{name}”: {e}"), &[&file_name(p), &e]), cx);
                    return;
                }
            }
        }
        if docs.is_empty() {
            return;
        }
        let n = docs.len();
        if self.tab().is_some_and(Tab::editable) {
            if self.change(
                move |d| {
                    for o in docs {
                        edit::append(d, o)?;
                    }
                    Ok(())
                },
                cx,
            ) {
                self.say(fill(tr("{n} files added at the end."), &[&n.to_string()]), cx);
            }
        } else {
            match edit::merge(docs) {
                Ok(d) => {
                    self.open_doc(tr("Merged.pdf").to_string(), d, cx);
                    self.say(fill(tr("{n} files merged. Save to keep the result."), &[&n.to_string()]), cx);
                }
                Err(e) => self.say(e, cx),
            }
        }
    }

    /// Writes the chosen pages to a new file.
    pub fn extract_to(&mut self, pages: &[usize], path: PathBuf, cx: &mut Cx) -> bool {
        let r = self.finished_doc().and_then(|d| edit::extract(&d, pages)).and_then(|x| {
            let pw = self.tab().and_then(|t| t.password.clone());
            edit::save(&x, &path, pw.as_deref())
        });
        match r {
            Ok(()) => {
                self.say(fill(tr("Saved “{name}”."), &[&file_name(&path)]), cx);
                true
            }
            Err(e) => {
                self.say(fill(tr("Could not save: {e}"), &[&e]), cx);
                false
            }
        }
    }

    pub fn ask_extract(&mut self, cx: &mut Cx) {
        if !self.tab().is_some_and(Tab::editable) {
            return;
        }
        let pages = self.target_pages();
        let label = if pages.len() == 1 { format!("{}", pages[0] + 1) } else { format!("{}-{}", pages[0] + 1, pages[pages.len() - 1] + 1) };
        let name = self.suggested_name(&format!(" ({} {label})", tr("pages")));
        let title = tr("Save the pages as").to_string();
        cx.after(move |w| {
            if let Some(p) = plume_ui::dialogs::save_as(w.hwnd, &title, &name, "pdf", &[("PDF", "*.pdf")]) {
                w.with(|h, cx| {
                    if let Some(app) = with_app(h) {
                        app.extract_to(&pages, p, cx);
                    }
                });
            }
        });
    }

    /// Splits into files in `folder`, one per range ("1-3, 4-6").
    pub fn split_to(&mut self, ranges_text: &str, folder: &Path, cx: &mut Cx) -> usize {
        let n = self.tab().map(|t| t.pages()).unwrap_or(0);
        let ranges = match edit::parse_ranges(ranges_text, n) {
            Ok(r) => r,
            Err(e) => {
                self.say(fill(tr("Pages not understood: {part}"), &[&e]), cx);
                return 0;
            }
        };
        let base = self.suggested_name("").trim_end_matches(".pdf").to_string();
        let pw = self.tab().and_then(|t| t.password.clone());
        let r = self.finished_doc().and_then(|d| edit::split(&d, &ranges)).and_then(|parts| {
            for (part, (a, b)) in parts.iter().zip(&ranges) {
                let label = if a == b { format!("{}", a + 1) } else { format!("{}-{}", a + 1, b + 1) };
                edit::save(part, &folder.join(format!("{base} ({} {label}).pdf", tr("pages"))), pw.as_deref())?;
            }
            Ok(parts.len())
        });
        match r {
            Ok(k) => {
                self.say(fill(tr("{n} files written."), &[&k.to_string()]), cx);
                k
            }
            Err(e) => {
                self.say(fill(tr("Could not save: {e}"), &[&e]), cx);
                0
            }
        }
    }

    pub fn print(&mut self, cx: &mut Cx) {
        let Some(t) = self.tab() else { return };
        if t.status != Status::Ready {
            return;
        }
        // what is being made is printed too
        let bytes = if t.annots.is_empty() && t.values.is_empty() {
            t.bytes.clone()
        } else {
            match self.finished_doc().and_then(|d| edit::to_bytes(&d, None)) {
                Ok(b) => Arc::new(b),
                Err(_) => t.bytes.clone(),
            }
        };
        let pw = if Arc::ptr_eq(&bytes, &t.bytes) { t.render_pw.clone() } else { None };
        let (name, n) = (t.name.clone(), t.pages());
        let sender = self.win.sender();
        cx.after(move |w| {
            crate::print::run(w.hwnd, &name, bytes, pw, n, move |r| {
                sender.send(move |h, cx| {
                    if let Some(app) = with_app(h) {
                        match r {
                            Ok(k) => app.say(fill(tr("{n} pages sent to the printer."), &[&k.to_string()]), cx),
                            Err(e) => {
                                app.note(format!("print: {e}"));
                                app.say(fill(tr("Could not print: {e}"), &[&e]), cx);
                            }
                        }
                    }
                });
            });
        });
    }

    // ───────────────────────── annotations ─────────────────────────

    fn push_undo_annots(&mut self) {
        if let Some(t) = self.tab_mut() {
            let snap = Undo::Annots(t.annots.clone());
            t.undo.push(snap);
            if t.undo.len() > 30 {
                t.undo.remove(0);
            }
        }
    }

    pub fn add_annot(&mut self, a: Annot, cx: &mut Cx) -> usize {
        self.push_undo_annots();
        let Some(t) = self.tab_mut() else { return 0 };
        t.annots.push(a);
        t.dirty = true;
        let i = t.annots.len() - 1;
        self.sync_dirty();
        cx.repaint();
        i
    }

    pub fn remove_annot(&mut self, i: usize, cx: &mut Cx) {
        self.push_undo_annots();
        if let Some(t) = self.tab_mut()
            && i < t.annots.len()
        {
            t.annots.remove(i);
        }
        self.selected = None;
        cx.repaint();
    }

    pub fn undo(&mut self, cx: &mut Cx) {
        let Some(t) = self.tabs.get_mut(self.cur) else { return };
        let Some(u) = t.undo.pop() else { return };
        self.selected = None;
        match u {
            Undo::Annots(a) => t.annots = a,
            Undo::Fields(v) => t.values = v,
            Undo::Doc(doc, annots, values) => {
                t.doc = Some(*doc);
                t.annots = annots;
                t.values = values;
                self.reload_tab(cx);
            }
        }
        cx.repaint();
    }

    /// A highlight over the text under the dragged area (or the area itself where there is no text).
    pub fn make_highlight(&mut self, page: usize, a: (f32, f32), b: (f32, f32), cx: &mut Cx) -> bool {
        let area = [a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs()];
        if area[2] < 3.0 && area[3] < 3.0 {
            return false;
        }
        let Some(t) = self.tab() else { return false };
        let g = t.geom(page);
        let mut boxes = Vec::new();
        let mut words = String::new();
        if let Some(pt_) = t.texts.get(page)
            && let Some((s, n)) = text::chars_in(pt_, g.pdf_rect(area))
        {
            words = pt_.chars[s..s + n].iter().collect::<String>().replace('\n', " ");
            boxes = text::line_boxes(pt_, s, n).into_iter().map(|b| g.view_rect(b)).collect();
        }
        if boxes.is_empty() {
            boxes.push(area);
        }
        let x0 = boxes.iter().map(|b| b[0]).fold(f32::MAX, f32::min);
        let y0 = boxes.iter().map(|b| b[1]).fold(f32::MAX, f32::min);
        let x1 = boxes.iter().map(|b| b[0] + b[2]).fold(f32::MIN, f32::max);
        let y1 = boxes.iter().map(|b| b[1] + b[3]).fold(f32::MIN, f32::max);
        let a = Annot {
            kind: Kind::Highlight,
            page,
            rect: [x0, y0, x1 - x0, y1 - y0],
            boxes,
            strokes: vec![],
            text: words,
            color: HL_COLORS[self.hl_color],
            width: 1.0,
        };
        self.add_annot(a, cx);
        true
    }

    pub fn make_ink(&mut self, page: usize, pts: &[(f32, f32)], cx: &mut Cx) -> bool {
        if pts.len() < 2 {
            return false;
        }
        let w = 2.0;
        let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (y0, y1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let (bw, bh) = ((x1 - x0).max(0.5), (y1 - y0).max(0.5));
        let stroke = pts.iter().map(|p| [(p.0 - x0) / bw, (p.1 - y0) / bh]).collect();
        let a = Annot {
            kind: Kind::Ink,
            page,
            rect: [x0, y0, bw, bh],
            boxes: vec![],
            strokes: vec![stroke],
            text: String::new(),
            color: INK_COLORS[self.ink_color],
            width: w,
        };
        self.add_annot(a, cx);
        true
    }

    pub fn place_note(&mut self, page: usize, at: (f32, f32), cx: &mut Cx) {
        let a = Annot {
            kind: Kind::Note,
            page,
            rect: [at.0 - 11.0, at.1 - 10.0, 22.0, 20.0],
            boxes: vec![],
            strokes: vec![],
            text: String::new(),
            color: NOTE_COLOR,
            width: 1.0,
        };
        let i = self.add_annot(a, cx);
        self.selected = Some(i);
        self.open_text_modal(Modal::NoteText { i, new: true }, cx);
    }

    pub fn place_text(&mut self, page: usize, a: (f32, f32), b: (f32, f32), cx: &mut Cx) {
        let mut r = [a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs()];
        if r[2] < 24.0 || r[3] < 12.0 {
            r = [a.0, a.1, 180.0, 40.0];
        }
        let an = Annot { kind: Kind::Text, page, rect: r, boxes: vec![], strokes: vec![], text: String::new(), color: INK_COLORS[self.ink_color], width: 12.0 };
        let i = self.add_annot(an, cx);
        self.selected = Some(i);
        self.open_text_modal(Modal::TextBox { i, new: true }, cx);
    }

    pub fn place_signature(&mut self, page: usize, at: (f32, f32), cx: &mut Cx) -> bool {
        let Some(sig) = self.sig.clone() else {
            self.open_signature_pad(cx);
            return false;
        };
        let w = 150.0f32;
        let h = (w / sig.aspect.max(0.2)).clamp(20.0, 150.0);
        let a = Annot {
            kind: Kind::Signature,
            page,
            rect: [at.0 - w / 2.0, at.1 - h / 2.0, w, h],
            boxes: vec![],
            strokes: sig.strokes.clone(),
            text: String::new(),
            color: [0.06, 0.12, 0.42],
            width: 1.6,
        };
        let i = self.add_annot(a, cx);
        self.selected = Some(i);
        self.tool = Tool::Hand;
        true
    }

    pub fn open_signature_pad(&mut self, cx: &mut Cx) {
        self.modal = Some(Modal::Signature { strokes: Vec::new(), pad: Rect::default() });
        cx.repaint();
    }

    fn open_text_modal(&mut self, m: Modal, cx: &mut Cx) {
        let i = match m {
            Modal::NoteText { i, .. } | Modal::TextBox { i, .. } => i,
            _ => return,
        };
        self.modal_text = self.tab().and_then(|t| t.annots.get(i)).map(|a| a.text.clone()).unwrap_or_default();
        self.modal = Some(m);
        self.edit_for = Some(EditFor::Modal);
        self.edit_shown = None;
        cx.repaint();
    }

    pub fn close_modal(&mut self, cx: &mut Cx) {
        // a new note or text box left empty is dropped
        if let Some(Modal::NoteText { i, new: true } | Modal::TextBox { i, new: true }) = self.modal
            && self.tab().and_then(|t| t.annots.get(i)).is_some_and(|a| a.text.trim().is_empty())
        {
            if let Some(t) = self.tab_mut() {
                t.annots.remove(i);
                t.undo.pop();
            }
            self.selected = None;
        }
        self.modal = None;
        if self.edit_for == Some(EditFor::Modal) {
            self.edit_for = None;
        }
        cx.repaint();
    }

    pub fn modal_ok(&mut self, cx: &mut Cx) {
        let Some(m) = self.modal.clone() else { return };
        match m {
            Modal::Password { .. } => {
                let pw = self.modal_text.clone();
                if !pw.is_empty() {
                    self.give_password(pw, cx);
                }
            }
            Modal::ConfirmSave => {
                self.modal = None;
                if let Some(p) = self.tab().and_then(|t| t.path.clone()) {
                    self.save_to(p, cx);
                }
            }
            Modal::ConfirmClose(i) => {
                self.modal = None;
                self.close_tab(i, true, cx);
            }
            Modal::ConfirmQuit => {
                self.modal = None;
                self.force_close.set(true);
                self.win.close();
            }
            Modal::Split => {
                let ranges = self.modal_text.clone();
                let n = self.tab().map(|t| t.pages()).unwrap_or(0);
                if let Err(e) = edit::parse_ranges(&ranges, n) {
                    self.say(fill(tr("Pages not understood: {part}"), &[&e]), cx);
                    return;
                }
                self.close_modal(cx);
                let title = tr("Folder for the new files").to_string();
                cx.after(move |w| {
                    if let Some(folder) = plume_ui::dialogs::pick_folder(w.hwnd, &title) {
                        w.with(|h, cx| {
                            if let Some(app) = with_app(h) {
                                app.split_to(&ranges, &folder, cx);
                            }
                        });
                    }
                });
            }
            Modal::NoteText { i, .. } | Modal::TextBox { i, .. } => {
                let text = self.modal_text.trim_end().to_string();
                if let Some(t) = self.tab_mut()
                    && let Some(a) = t.annots.get_mut(i)
                {
                    a.text = text;
                    t.dirty = true;
                }
                self.close_modal(cx);
            }
            Modal::Signature { strokes, pad } => {
                let pts: Vec<Vec<(f32, f32)>> = strokes.iter().map(|s| s.iter().map(|p| (p.0 - pad.x, p.1 - pad.y)).collect()).collect();
                match Signature::from_pad(&pts) {
                    Some(s) => {
                        s.save();
                        self.sig = Some(s);
                        self.tool = Tool::Sign;
                        self.modal = None;
                        self.say(tr("Signature saved. Click where it goes."), cx);
                    }
                    None => self.say(tr("Draw your signature in the box first."), cx),
                }
            }
        }
        cx.repaint();
    }

    pub fn modal_alt(&mut self, cx: &mut Cx) {
        match self.modal.clone() {
            Some(Modal::ConfirmSave) => {
                self.modal = None;
                self.ask_save_as(cx);
            }
            Some(Modal::Signature { pad, .. }) => self.modal = Some(Modal::Signature { strokes: Vec::new(), pad }),
            _ => {}
        }
        cx.repaint();
    }

    pub fn set_field(&mut self, i: usize, v: FieldValue, cx: &mut Cx) {
        let Some(t) = self.tabs.get_mut(self.cur) else { return };
        let Some(f) = t.fields.get(i) else { return };
        let id = f.id;
        let snap = Undo::Fields(t.values.clone());
        t.undo.push(snap);
        t.values.insert(id, v);
        t.dirty = true;
        self.sync_dirty();
        cx.repaint();
    }

    pub fn field_value(&self, i: usize) -> Option<FieldValue> {
        let t = self.tab()?;
        let f = t.fields.get(i)?;
        Some(t.values.get(&f.id).cloned().unwrap_or(if f.check { FieldValue::Check(f.on) } else { FieldValue::Text(f.value.clone()) }))
    }

    // ───────────────────────── search ─────────────────────────

    pub fn open_search(&mut self, cx: &mut Cx) {
        self.search_open = true;
        self.edit_for = Some(EditFor::Search);
        self.edit_shown = None;
        cx.repaint();
    }

    pub fn run_search(&mut self, jump: bool, cx: &mut Cx) {
        let q = self.query.clone();
        let page = self.current_page();
        let Some(t) = self.tab_mut() else { return };
        t.found = text::find(&t.texts, &q);
        t.found_i = t.found.iter().position(|f| f.page >= page).unwrap_or(0);
        if jump && !t.found.is_empty() {
            self.show_found(cx);
        }
        cx.repaint();
    }

    pub fn next_found(&mut self, back: bool, cx: &mut Cx) {
        let Some(t) = self.tab_mut() else { return };
        let n = t.found.len();
        if n == 0 {
            return;
        }
        t.found_i = if back { (t.found_i + n - 1) % n } else { (t.found_i + 1) % n };
        self.show_found(cx);
    }

    /// Scrolls the current match into view.
    pub fn show_found(&mut self, cx: &mut Cx) {
        self.relayout();
        let view = self.view;
        let Some(t) = self.tab_mut() else { return };
        let Some(f) = t.found.get(t.found_i).copied() else { return };
        let Some(pt_) = t.texts.get(f.page) else { return };
        let g = t.geom(f.page);
        let Some(b) = text::line_boxes(pt_, f.start, f.len).first().map(|b| g.view_rect(*b)) else { return };
        let Some(r) = t.rects.get(f.page).copied() else { return };
        let k = t.k();
        let (y, x) = (r.y + b[1] * k, r.x + b[0] * k);
        if y < t.sy + 40.0 || y > t.sy + view.h - 80.0 {
            t.sy = y - view.h * 0.35;
        }
        if x < t.sx || x > t.sx + view.w - 40.0 {
            t.sx = x - view.w * 0.3;
        }
        clamp_scroll(t, view.w, view.h);
        cx.repaint();
    }

    // ───────────────────────── events ─────────────────────────

    fn act(&mut self, h: Hit, p: Pt, cx: &mut Cx) {
        match h {
            Hit::Min => self.win.minimize(),
            Hit::Max => self.win.toggle_maximize(),
            Hit::Close => {
                if self.tabs.iter().any(|t| t.dirty) {
                    self.modal = Some(Modal::ConfirmQuit);
                } else {
                    self.win.close();
                }
            }
            Hit::Tab(i) => {
                self.settings_open = false;
                self.switch_to(i, cx);
            }
            Hit::TabClose(i) => self.close_tab(i, false, cx),
            Hit::NewTab => self.ask_open(cx),
            Hit::Tool(t) => {
                self.tool = if self.tool == t && t != Tool::Hand { Tool::Hand } else { t };
                self.selected = None;
                if self.tool == Tool::Sign && self.sig.is_none() {
                    self.open_signature_pad(cx);
                }
            }
            Hit::Swatch(i) => match self.tool {
                Tool::Highlight => self.hl_color = i,
                _ => self.ink_color = i,
            },
            Hit::Btn(b) => self.button(b, p, cx),
            Hit::PageBox => {
                self.page_box_text = (self.current_page() + 1).to_string();
                self.edit_for = Some(EditFor::PageBox);
                self.edit_shown = None;
            }
            Hit::SideTab(i) => {
                self.side_tab = i;
                self.side_scroll = 0.0;
            }
            Hit::Outline(i) => {
                if let Some(page) = self.tab().and_then(|t| t.outline.get(i)).map(|o| o.page) {
                    self.go_page(page, cx);
                }
            }
            Hit::SearchBox => {
                self.edit_for = Some(EditFor::Search);
                self.edit_shown = None;
            }
            Hit::Field(i) => match self.field_value(i) {
                Some(FieldValue::Check(on)) => self.set_field(i, FieldValue::Check(!on), cx),
                Some(FieldValue::Text(_)) => {
                    self.edit_for = Some(EditFor::Field(i));
                    self.edit_shown = None;
                }
                None => {}
            },
            Hit::Menu(i) => {
                if let Some(m) = self.menu.take()
                    && let Some((_, id)) = m.items.get(i)
                {
                    self.button(id, p, cx);
                }
            }
            Hit::ModalOk => self.modal_ok(cx),
            Hit::ModalAlt => self.modal_alt(cx),
            Hit::ModalCancel => self.close_modal(cx),
            Hit::ModalField => {
                self.edit_for = Some(EditFor::Modal);
            }
            Hit::Recent(i) => {
                if let Some(p) = self.mem.recent.get(i).cloned() {
                    self.open_path(p, cx);
                }
            }
            Hit::RecentForget(i) => {
                if i < self.mem.recent.len() {
                    self.mem.recent.remove(i);
                    self.mem.save();
                }
            }
            Hit::Choice(name, i) => {
                match name {
                    "theme" => self.set.theme = ["system", "dark", "light"][i.min(2)].into(),
                    "lang" => self.set.lang = ["system", "en", "fr"][i.min(2)].into(),
                    "zoom" => self.set.zoom = ["width", "page", "100"][i.min(2)].into(),
                    _ => {}
                }
                self.settings_changed(cx);
            }
            Hit::Toggle(name) => {
                match name {
                    "dark_pages" => self.set.dark_pages = !self.set.dark_pages,
                    "remember_page" => self.set.remember_page = !self.set.remember_page,
                    _ => {}
                }
                self.settings_changed(cx);
            }
            _ => {}
        }
        cx.repaint();
    }

    pub fn button(&mut self, b: &str, p: Pt, cx: &mut Cx) {
        match b {
            "open" => self.ask_open(cx),
            "save" => self.save(cx),
            "save_as" => self.ask_save_as(cx),
            "print" => self.print(cx),
            "undo" => self.undo(cx),
            "sidebar" => {
                self.set.sidebar = !self.set.sidebar;
                self.set.save();
            }
            "search" => {
                if self.search_open {
                    self.search_open = false;
                    if self.edit_for == Some(EditFor::Search) {
                        self.edit_for = None;
                    }
                } else {
                    self.open_search(cx);
                }
            }
            "search_next" => self.next_found(false, cx),
            "search_prev" => self.next_found(true, cx),
            "search_close" => {
                self.search_open = false;
                self.edit_for = None;
            }
            "zoom_in" => self.zoom_step(true, None, cx),
            "zoom_out" => self.zoom_step(false, None, cx),
            "fit_width" => self.set_fit(Fit::Width, cx),
            "fit_page" => self.set_fit(Fit::Page, cx),
            "zoom_100" => self.zoom_to(1.0, None, cx),
            "prev_page" => {
                let c = self.current_page();
                self.go_page(c.saturating_sub(1), cx);
            }
            "next_page" => {
                let c = self.current_page();
                self.go_page(c + 1, cx);
            }
            "dark_pages" => {
                self.set.dark_pages = !self.set.dark_pages;
                self.set.save();
            }
            "settings" => {
                self.settings_open = !self.settings_open;
                self.menu = None;
            }
            "more" => {
                let mut items: Vec<(String, &'static str)> = vec![
                    (tr("Merge PDFs…").into(), "merge"),
                    (tr("Split into files…").into(), "split"),
                    (tr("Save pages as a new file…").into(), "extract"),
                    (tr("Rotate left").into(), "rotate_l"),
                    (tr("Rotate right").into(), "rotate_r"),
                    (tr("Delete pages").into(), "delete_pages"),
                    (tr("Save a copy…").into(), "save_as"),
                    (tr("Print…").into(), "print"),
                    (tr("New signature…").into(), "new_signature"),
                ];
                if self.tab().is_none() {
                    items = vec![(tr("Merge PDFs…").into(), "merge"), (tr("New signature…").into(), "new_signature")];
                }
                self.menu = Some(Menu { x: p.x - 200.0, y: TITLE_H + BAR_H - 4.0, items, more: true });
            }
            "merge" => self.ask_merge(cx),
            "split" => {
                if self.tab().is_some_and(Tab::editable) {
                    let n = self.tab().map(|t| t.pages()).unwrap_or(1);
                    self.modal_text = if n > 1 { format!("1-{}, {}-{n}", n / 2, n / 2 + 1) } else { "1".into() };
                    self.modal = Some(Modal::Split);
                    self.edit_for = Some(EditFor::Modal);
                    self.edit_shown = None;
                }
            }
            "extract" => self.ask_extract(cx),
            "rotate_l" => self.rotate(-90, cx),
            "rotate_r" => self.rotate(90, cx),
            "delete_pages" => self.delete_pages(cx),
            "new_signature" => self.open_signature_pad(cx),
            "forget_signature" => {
                Signature::forget();
                self.sig = None;
            }
            "default_app" => {
                self.win.open("ms-settings:defaultapps");
            }
            "close_settings" => self.settings_open = false,
            "delete_annot" => {
                if let Some(i) = self.selected {
                    self.remove_annot(i, cx);
                }
            }
            "edit_annot" => {
                if let Some(i) = self.selected {
                    match self.tab().and_then(|t| t.annots.get(i)).map(|a| a.kind) {
                        Some(Kind::Note) => self.open_text_modal(Modal::NoteText { i, new: false }, cx),
                        Some(Kind::Text) => self.open_text_modal(Modal::TextBox { i, new: false }, cx),
                        _ => {}
                    }
                }
            }
            "close_tab" => self.close_tab(self.cur, false, cx),
            "password" => self.ask_password(false, cx),
            _ => {}
        }
        cx.repaint();
    }

    fn mouse_down(&mut self, p: Pt, clicks: u8, cx: &mut Cx) {
        let h = self.hits.at(p).cloned();
        if self.menu.is_some() && !matches!(h, Some(Hit::Menu(_))) {
            self.menu = None;
            cx.repaint();
            return;
        }
        // a field being typed in closes on a click elsewhere
        if matches!(self.edit_for, Some(EditFor::Field(_)) | Some(EditFor::PageBox)) && !matches!(h, Some(Hit::Field(_)) | Some(Hit::PageBox)) {
            self.edit_for = None;
        }
        self.pressed = h.clone();
        cx.capture(true);
        if self.modal.is_some() {
            if let (Some(Hit::Pad), Some(Modal::Signature { strokes, .. })) = (&h, self.modal.as_mut()) {
                strokes.push(vec![(p.x, p.y)]);
                self.drag = Some(Drag::Pad);
            }
            cx.repaint();
            return;
        }
        match h {
            Some(Hit::VScroll) => {
                let sy = self.tab().map(|t| t.sy).unwrap_or(0.0);
                self.drag = Some(Drag::VScroll { start: p.y, sy });
            }
            Some(Hit::Thumb(i)) => self.drag = Some(Drag::Thumb { from: i, start: p, active: false, to: i }),
            Some(Hit::Handle(i)) => {
                if let Some(orig) = self.tab().and_then(|t| t.annots.get(i)).map(|a| a.rect) {
                    self.push_undo_annots();
                    self.drag = Some(Drag::Resize { i, start: p, orig });
                }
            }
            Some(Hit::Annot(i)) if matches!(self.tool, Tool::Hand | Tool::Sign) => {
                self.selected = Some(i);
                if clicks == 2 {
                    self.button("edit_annot", p, cx);
                    self.pressed = None;
                } else if let Some(r) = self.tab().and_then(|t| t.annots.get(i)).map(|a| a.rect) {
                    self.push_undo_annots();
                    self.drag = Some(Drag::Move { i, start: p, orig: r });
                }
            }
            Some(Hit::Page(_) | Hit::Annot(_) | Hit::Field(_)) | None if self.view.contains(p) && self.tab().is_some() => {
                if matches!(h, Some(Hit::Field(_))) && self.tool == Tool::Hand {
                    cx.repaint();
                    return;
                }
                self.selected = None;
                let on = self.page_at(p);
                let editable = self.tab().is_some_and(Tab::editable);
                match (self.tool, on) {
                    (Tool::Hand, _) | (_, None) => {
                        let (sx, sy) = self.tab().map(|t| (t.sx, t.sy)).unwrap_or_default();
                        self.drag = Some(Drag::Pan { start: p, sx, sy });
                    }
                    (_, Some(_)) if !editable => {
                        let msg = match self.tab().and_then(|t| t.readonly.clone()) {
                            Some(why) => fill(tr("This file can be read but not changed ({why})."), &[&why]),
                            None => tr("The file is still opening.").to_string(),
                        };
                        self.say(msg, cx);
                    }
                    (Tool::Highlight, Some((page, at))) => self.drag = Some(Drag::Highlight { page, a: at, b: at }),
                    (Tool::Ink, Some((page, at))) => self.drag = Some(Drag::Ink { page, pts: vec![at] }),
                    (Tool::Text, Some((page, at))) => self.drag = Some(Drag::TextBox { page, a: at, b: at }),
                    (Tool::Note | Tool::Sign, Some((page, at))) => self.drag = Some(Drag::Click { page, at }),
                }
            }
            _ => {}
        }
        cx.repaint();
    }

    fn mouse_move(&mut self, p: Pt, cx: &mut Cx) {
        let Some(d) = self.drag.clone() else {
            let h = self.hits.at(p).cloned();
            if h != self.hover {
                self.hover = h;
                cx.repaint();
            }
            return;
        };
        let k = self.tab().map(|t| t.k()).unwrap_or(1.0);
        let local = |app: &App, page: usize| {
            let r = app.page_screen(page).unwrap_or_default();
            let (w, h) = (r.w / k, r.h / k);
            (((p.x - r.x) / k).clamp(0.0, w), ((p.y - r.y) / k).clamp(0.0, h))
        };
        match d {
            Drag::Pan { start, sx, sy } => {
                let view = self.view;
                if let Some(t) = self.tab_mut() {
                    t.sx = sx - (p.x - start.x);
                    t.sy = sy - (p.y - start.y);
                    clamp_scroll(t, view.w, view.h);
                }
            }
            Drag::VScroll { start, sy } => {
                let view = self.view;
                if let Some(t) = self.tab_mut() {
                    let ratio = t.content.1 / view.h.max(1.0);
                    t.sy = sy + (p.y - start) * ratio;
                    clamp_scroll(t, view.w, view.h);
                }
            }
            Drag::Thumb { from, start, active, .. } => {
                let active = active || (p.y - start.y).abs() > 6.0 || (p.x - start.x).abs() > 6.0;
                let to = self.thumb_target(p.y);
                self.drag = Some(Drag::Thumb { from, start, active, to });
            }
            Drag::Highlight { page, a, .. } => self.drag = Some(Drag::Highlight { page, a, b: local(self, page) }),
            Drag::TextBox { page, a, .. } => self.drag = Some(Drag::TextBox { page, a, b: local(self, page) }),
            Drag::Ink { page, mut pts } => {
                let q = local(self, page);
                if pts.last().is_none_or(|l| (l.0 - q.0).abs() + (l.1 - q.1).abs() > 0.6) {
                    pts.push(q);
                }
                self.drag = Some(Drag::Ink { page, pts });
            }
            Drag::Move { i, start, orig } => {
                if let Some(t) = self.tab_mut()
                    && let Some(a) = t.annots.get_mut(i)
                {
                    let (dx, dy) = ((p.x - start.x) / k, (p.y - start.y) / k);
                    a.rect = [orig[0] + dx, orig[1] + dy, orig[2], orig[3]];
                    if a.kind == Kind::Highlight {
                        // a highlight stays on its text
                        a.rect = orig;
                    }
                    t.dirty = true;
                }
            }
            Drag::Resize { i, start, orig } => {
                if let Some(t) = self.tab_mut()
                    && let Some(a) = t.annots.get_mut(i)
                {
                    let dx = (p.x - start.x) / k;
                    let w = (orig[2] + dx).max(16.0);
                    // a signature or drawing keeps its proportions
                    let h =
                        if matches!(a.kind, Kind::Signature | Kind::Ink) { w * orig[3] / orig[2].max(0.1) } else { (orig[3] + (p.y - start.y) / k).max(12.0) };
                    a.rect = [orig[0], orig[1], w, h];
                    t.dirty = true;
                }
            }
            Drag::Click { .. } => {}
            Drag::Pad => {
                if let Some(Modal::Signature { strokes, pad }) = self.modal.as_mut()
                    && let Some(s) = strokes.last_mut()
                {
                    let q = (p.x.clamp(pad.x, pad.right()), p.y.clamp(pad.y, pad.bottom()));
                    if s.last().is_none_or(|l| (l.0 - q.0).abs() + (l.1 - q.1).abs() > 0.8) {
                        s.push(q);
                    }
                }
            }
        }
        cx.repaint();
    }

    fn mouse_up(&mut self, p: Pt, mods: Mods, cx: &mut Cx) {
        cx.capture(false);
        if let Some(d) = self.drag.take() {
            match d {
                Drag::Thumb { from, active: false, .. } => self.thumb_click(from, mods, cx),
                Drag::Thumb { from, active: true, to, .. } => {
                    let sel = self.tab().map(|t| t.sel.clone()).unwrap_or_default();
                    let moving = if sel.contains(&from) { sel } else { vec![from] };
                    self.move_pages(moving, to, cx);
                }
                Drag::Highlight { page, a, b } => {
                    self.make_highlight(page, a, b, cx);
                }
                Drag::Ink { page, pts } => {
                    self.make_ink(page, &pts, cx);
                }
                Drag::TextBox { page, a, b } => self.place_text(page, a, b, cx),
                Drag::Click { page, at } => match self.tool {
                    Tool::Note => self.place_note(page, at, cx),
                    Tool::Sign => {
                        self.place_signature(page, at, cx);
                    }
                    _ => {}
                },
                Drag::Move { i, orig, .. } | Drag::Resize { i, orig, .. } => {
                    // nothing moved: no undo step
                    if self.tab().and_then(|t| t.annots.get(i)).is_some_and(|a| a.rect == orig)
                        && let Some(t) = self.tab_mut()
                    {
                        t.undo.pop();
                    }
                    self.sync_dirty();
                }
                _ => {}
            }
            self.pressed = None;
            cx.repaint();
            return;
        }
        let up = self.hits.at(p).cloned();
        if let (Some(a), Some(b)) = (self.pressed.take(), up)
            && a == b
        {
            self.act(a, p, cx);
        }
        cx.repaint();
    }

    fn thumb_click(&mut self, i: usize, mods: Mods, cx: &mut Cx) {
        let Some(t) = self.tab_mut() else { return };
        if mods.ctrl {
            if let Some(k) = t.sel.iter().position(|&s| s == i) {
                t.sel.remove(k);
            } else {
                t.sel.push(i);
            }
        } else if mods.shift && !t.sel.is_empty() {
            let a = *t.sel.last().unwrap_or(&i);
            t.sel = if a <= i { (a..=i).collect() } else { (i..=a).rev().collect() };
        } else {
            t.sel = vec![i];
        }
        self.go_page(i, cx);
    }

    /// Where dropped thumbnails go: before the thumbnail under `y` (or after the last).
    fn thumb_target(&self, y: f32) -> usize {
        for (i, r) in &self.thumb_rects {
            if y < r.y + r.h / 2.0 {
                return *i;
            }
        }
        self.thumb_rects.last().map(|(i, _)| i + 1).unwrap_or(0)
    }

    fn right_click(&mut self, p: Pt, cx: &mut Cx) {
        let h = self.hits.at(p).cloned();
        let items: Vec<(String, &'static str)> = match h {
            Some(Hit::Annot(i)) => {
                self.selected = Some(i);
                let mut v = vec![(tr("Delete").to_string(), "delete_annot")];
                if self.tab().and_then(|t| t.annots.get(i)).is_some_and(|a| matches!(a.kind, Kind::Note | Kind::Text)) {
                    v.insert(0, (tr("Edit text").to_string(), "edit_annot"));
                }
                v
            }
            Some(Hit::Thumb(i)) => {
                if let Some(t) = self.tab_mut()
                    && !t.sel.contains(&i)
                {
                    t.sel = vec![i];
                }
                vec![
                    (tr("Rotate left").into(), "rotate_l"),
                    (tr("Rotate right").into(), "rotate_r"),
                    (tr("Save pages as a new file…").into(), "extract"),
                    (tr("Delete pages").into(), "delete_pages"),
                ]
            }
            Some(Hit::Page(_)) => vec![
                (tr("Rotate left").into(), "rotate_l"),
                (tr("Rotate right").into(), "rotate_r"),
                (tr("Save pages as a new file…").into(), "extract"),
                (tr("Delete pages").into(), "delete_pages"),
                (tr("Print…").into(), "print"),
            ],
            _ => return,
        };
        self.menu = Some(Menu { x: p.x, y: p.y, items, more: false });
        cx.repaint();
    }

    fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        let page_h = self.view.h * 0.9;
        match (vk, mods.ctrl, mods.shift) {
            (0x4F, true, _) => self.ask_open(cx),                                     // Ctrl+O
            (0x53, true, false) => self.save(cx),                                     // Ctrl+S
            (0x53, true, true) => self.ask_save_as(cx),                               // Ctrl+Shift+S
            (0x50, true, _) => self.print(cx),                                        // Ctrl+P
            (0x46, true, _) => self.open_search(cx),                                  // Ctrl+F
            (0x5A, true, _) => self.undo(cx),                                         // Ctrl+Z
            (0x57, true, _) | (0x73, true, _) => self.close_tab(self.cur, false, cx), // Ctrl+W, Ctrl+F4
            (0x09, true, back) if !self.tabs.is_empty() => {
                let n = self.tabs.len();
                let i = if back { (self.cur + n - 1) % n } else { (self.cur + 1) % n };
                self.switch_to(i, cx);
            }
            (0xBB | 0x6B, true, _) => self.zoom_step(true, None, cx), // Ctrl+=
            (0xBD | 0x6D, true, _) => self.zoom_step(false, None, cx),
            (0x30 | 0x60, true, _) => self.zoom_to(1.0, None, cx),
            (0x31, true, _) => self.set_fit(Fit::Page, cx),
            (0x32, true, _) => self.set_fit(Fit::Width, cx),
            (0x72, _, back) => self.next_found(back, cx), // F3
            (0x22, _, _) | (0x20, false, false) => self.scroll_by(0.0, page_h, cx),
            (0x21, _, _) | (0x20, false, true) => self.scroll_by(0.0, -page_h, cx),
            (0x28, false, _) => self.scroll_by(0.0, 60.0, cx),
            (0x26, false, _) => self.scroll_by(0.0, -60.0, cx),
            (0x27, false, _) => self.scroll_by(60.0, 0.0, cx),
            (0x25, false, _) => self.scroll_by(-60.0, 0.0, cx),
            (0x24, _, _) => self.go_page(0, cx),
            (0x23, _, _) => {
                let n = self.tab().map(|t| t.pages()).unwrap_or(1);
                self.go_page(n.saturating_sub(1), cx);
            }
            (0x2E, _, _) => {
                if let Some(i) = self.selected {
                    self.remove_annot(i, cx);
                } else if self.tab().is_some_and(|t| !t.sel.is_empty()) {
                    self.delete_pages(cx);
                }
            }
            (0x1B, _, _) => {
                if self.menu.is_some() {
                    self.menu = None;
                } else if self.modal.is_some() {
                    self.close_modal(cx);
                } else if self.selected.is_some() {
                    self.selected = None;
                } else if self.tool != Tool::Hand {
                    self.tool = Tool::Hand;
                } else if self.search_open {
                    self.search_open = false;
                } else if self.settings_open {
                    self.settings_open = false;
                }
            }
            (0x48, false, false) if !mods.alt => self.tool = Tool::Hand, // H
            _ => {}
        }
        cx.repaint();
    }

    fn wheel(&mut self, p: Pt, dx: f32, dy: f32, mods: Mods, cx: &mut Cx) {
        if self.modal.is_some() {
            return;
        }
        if self.settings_open {
            return;
        }
        let side = self.set.sidebar && p.x < SIDE_W && p.y > TITLE_H + BAR_H;
        if side {
            let max = (self.side_h - (self.view.h)).max(0.0);
            self.side_scroll = (self.side_scroll - dy * 80.0).clamp(0.0, max);
            cx.repaint();
            return;
        }
        if mods.ctrl {
            let z = self.tab().map(|t| t.zoom).unwrap_or(1.0);
            self.zoom_to(z * 1.1f32.powf(dy), Some(p), cx);
            return;
        }
        if mods.shift {
            self.scroll_by(-dy * 90.0, 0.0, cx);
        } else {
            self.scroll_by(-dx * 90.0, -dy * 90.0, cx);
        }
    }
}

/// Zoom for the fitting modes, from the page area's size.
fn fit_zoom(t: &mut Tab, w: f32, h: f32) {
    let sizes = t.sizes();
    if sizes.is_empty() || t.fit == Fit::Free {
        return;
    }
    let mw = sizes.iter().map(|s| s.0).fold(1.0, f32::max) * PT;
    let mh = sizes.iter().map(|s| s.1).fold(1.0, f32::max) * PT;
    let zw = (w - 2.0 * MARGIN - 12.0) / mw;
    let z = match t.fit {
        Fit::Page => zw.min((h - 2.0 * MARGIN) / mh),
        _ => zw,
    };
    t.zoom = z.clamp(0.1, 8.0);
}

pub fn layout_rects(t: &Tab, w: f32, _h: f32) -> Vec<Rect> {
    let k = t.k();
    let sizes = t.sizes();
    let maxw = sizes.iter().map(|s| s.0 * k).fold(0.0, f32::max);
    let cw = (maxw + 2.0 * MARGIN).max(w);
    let mut y = MARGIN;
    sizes
        .iter()
        .map(|s| {
            let (pw, ph) = (s.0 * k, s.1 * k);
            let r = rect(((cw - pw) / 2.0).floor(), y, pw, ph);
            y += ph + GAP;
            r
        })
        .collect()
}

fn clamp_scroll(t: &mut Tab, w: f32, h: f32) {
    let cw = t.rects.iter().map(|r| r.right()).fold(0.0, f32::max) + MARGIN;
    let ch = t.rects.last().map(|r| r.bottom() + MARGIN).unwrap_or(0.0);
    t.sx = t.sx.clamp(0.0, (cw - w).max(0.0));
    t.sy = t.sy.clamp(0.0, (ch - h).max(0.0));
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.frame += 1;
        self.paint_all(c, cx);
        self.tidy(c);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        self.handle(ev, cx);
        self.sync_dirty();
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(Hit::Min | Hit::Close) => Area::Control,
            Some(_) => Area::Client,
            None if p.y < TITLE_H && self.modal.is_none() => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Page(_)) | None => Cursor::Arrow,
            Some(Hit::Handle(_)) => Cursor::SizeWE,
            Some(Hit::SearchBox | Hit::ModalField | Hit::Pad) => Cursor::Arrow,
            Some(_) => Cursor::Hand,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl App {
    fn handle(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => self.mouse_move(p, cx),
            Event::MouseLeave => {
                self.hover = None;
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Left, clicks, .. } => self.mouse_down(p, clicks, cx),
            Event::MouseUp { p, button: Button::Left, mods } => self.mouse_up(p, mods, cx),
            Event::MouseDown { p, button: Button::Right, .. } => self.right_click(p, cx),
            Event::Wheel { p, dx, dy, mods } => self.wheel(p, dx, dy, mods, cx),
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::EditChanged(text) => match self.edit_for {
                Some(EditFor::Search) => {
                    self.query = text;
                    cx.timer(TYPED, Some(250));
                }
                Some(EditFor::Modal) => self.modal_text = text,
                Some(EditFor::PageBox) => self.page_box_text = text,
                Some(EditFor::Field(i)) if self.field_value(i) != Some(FieldValue::Text(text.clone())) => {
                    let id = self.tab().and_then(|t| t.fields.get(i)).map(|f| f.id);
                    if let (Some(id), Some(t)) = (id, self.tab_mut()) {
                        t.values.insert(id, FieldValue::Text(text));
                        t.dirty = true;
                    }
                }
                _ => {}
            },
            Event::EditKey { vk: 0x0D, mods } => match self.edit_for {
                Some(EditFor::Search) => {
                    cx.timer(TYPED, None);
                    let same = self.tab().is_some_and(|t| !t.found.is_empty());
                    if same {
                        self.next_found(mods.shift, cx);
                    } else {
                        self.run_search(true, cx);
                    }
                }
                Some(EditFor::Modal) => {
                    if !matches!(self.modal, Some(Modal::NoteText { .. } | Modal::TextBox { .. })) || mods.ctrl {
                        self.modal_ok(cx);
                    }
                }
                Some(EditFor::PageBox) => {
                    let n = self.page_box_text.trim().parse::<usize>().unwrap_or(0);
                    self.edit_for = None;
                    if n > 0 {
                        self.go_page(n - 1, cx);
                    }
                }
                Some(EditFor::Field(_)) => self.edit_for = None,
                None => {}
            },
            Event::EditKey { vk: 0x1B, .. } => match self.edit_for {
                Some(EditFor::Modal) => self.close_modal(cx),
                Some(EditFor::Search) => {
                    self.search_open = false;
                    self.edit_for = None;
                }
                _ => self.edit_for = None,
            },
            Event::EditKey { vk: 0x72, mods } => self.next_found(mods.shift, cx),
            Event::EditBlur => {
                if matches!(self.edit_for, Some(EditFor::Field(_)) | Some(EditFor::PageBox)) {
                    self.edit_for = None;
                }
                cx.repaint();
            }
            Event::Timer(TYPED) => {
                cx.timer(TYPED, None);
                self.run_search(true, cx);
            }
            Event::Timer(SETTLE) => {
                cx.timer(SETTLE, None);
                cx.repaint();
            }
            Event::Timer(TICK) => {
                if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_millis(3500)) {
                    self.toast = None;
                    cx.repaint();
                }
                // pages still coming: keep drawing as they arrive
                if self.renderer.pending() > 0 {
                    cx.repaint();
                }
            }
            Event::Opened(text) => {
                self.win.bring_to_front();
                let paths: Vec<PathBuf> = crate::split_args(&text).into_iter().map(PathBuf::from).filter(|p| p.is_file()).collect();
                self.open_paths(paths, cx);
            }
            Event::Close => {
                for i in 0..self.tabs.len() {
                    self.remember_page_of(i);
                }
                self.mem.save();
                self.set.save();
            }
            Event::SystemTheme => {
                if self.set.theme == "system" {
                    self.set_theme();
                }
                cx.repaint();
            }
            Event::Resized | Event::Scale | Event::Active(_) => cx.repaint(),
            _ => {}
        }
    }
}

/// Files dropped on the window, and closing with unsaved changes (asked first).
fn install_hook(win: &Window, dirty: Rc<Cell<bool>>, force: Rc<Cell<bool>>) {
    use windows::Win32::{
        Foundation::LRESULT,
        UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP},
    };
    const WM_CLOSE: u32 = 0x0010;
    const WM_DROPFILES: u32 = 0x0233;
    unsafe { DragAcceptFiles(win.hwnd, true) };
    let sender = win.sender();
    win.set_hook(Some(Rc::new(move |msg, wp, _lp| match msg {
        WM_DROPFILES => {
            let drop = HDROP(wp.0 as _);
            let paths: Vec<PathBuf> = unsafe {
                (0..DragQueryFileW(drop, u32::MAX, None))
                    .map(|i| {
                        let mut name = vec![0u16; DragQueryFileW(drop, i, None) as usize + 1];
                        let n = DragQueryFileW(drop, i, Some(&mut name)) as usize;
                        PathBuf::from(String::from_utf16_lossy(&name[..n]))
                    })
                    .collect()
            };
            unsafe { DragFinish(drop) };
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    let pdfs: Vec<PathBuf> = paths.into_iter().filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))).collect();
                    app.open_paths(pdfs, cx);
                }
            });
            Some(LRESULT(0))
        }
        WM_CLOSE if dirty.get() && !force.get() => {
            sender.send(|h, cx| {
                if let Some(app) = with_app(h) {
                    app.modal = Some(Modal::ConfirmQuit);
                    cx.repaint();
                }
            });
            Some(LRESULT(0))
        }
        _ => None,
    })));
}
