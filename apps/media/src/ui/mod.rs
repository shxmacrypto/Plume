//! Plume Media's window: the library (timeline, videos, folders, favorites, settings), the photo viewer and the
//! video player. All state lives in [`App`]; painting is in `library_view`, `viewer` and `settings_view`.

pub mod draw;
mod library_view;
mod settings_view;
mod viewer;

use crate::{
    i18n::{fill, tr},
    image::{self, Decoded, Px},
    library::{self, Item, Progress, Store},
    settings::{self, Settings},
    srt::{self, Cue},
    video::{self, AudioTrack, Ev, Player},
};
use draw::Bitmaps;
use plume_ui::{Area, Button, Canvas, Color, Cursor, Cx, Event, Gfx, Handler, Mods, PaintCx, Pt, Rect, Window};
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;

pub const TICK: usize = 1;
pub const TOP_H: f32 = 48.0;
pub const SIDEBAR_W: f32 = 216.0;
pub const INFO_W: f32 = 320.0;
pub const CONTROLS_H: f32 = 104.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Timeline,
    Videos,
    Folders,
    Folder(PathBuf),
    Favorites,
    Settings,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Nav(Page),
    AddFolder,
    Rescan,
    /// A tile of the grid: its place in the page's list.
    Tile(usize),
    FolderCard(usize),
    Scrollbar,
    EmptyAdd,
    // the viewer and the player
    Back,
    Prev,
    Next,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    RotL,
    RotR,
    Fav,
    Info,
    Slideshow,
    Fullscreen,
    Stage,
    Panel,
    PlayPause,
    Seek,
    Mute,
    Volume,
    Speed,
    Subs,
    Audio,
    Controls,
    Menu(usize),
    // settings
    Theme(&'static str),
    Lang(&'static str),
    Zoom(&'static str),
    SlideSpeed(u32),
    Tiles(u8),
    RemoveFolder(usize),
    OpenWith,
    ClearCache,
}

#[derive(Clone, Debug)]
pub enum Drag {
    Pan { from: Pt, ox: f32, oy: f32, moved: bool },
    Seek,
    Volume,
    Scroll { from_y: f32, scroll: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuAct {
    Speed(f64),
    Audio(u32),
    Subs(Option<usize>),
    Open(usize),
    Favorite(usize),
    Explorer(usize),
}

#[derive(Clone, Debug)]
pub struct Menu {
    pub items: Vec<(String, bool, MenuAct)>,
    /// The button it opened from: the menu shows above it (player) or below (elsewhere).
    pub anchor: Rect,
    pub up: bool,
}

/// A row of the grid: a month's title, or tiles.
#[derive(Clone, Debug)]
pub enum Row {
    Head { y: f32, title: String, n: usize },
    Tiles { y: f32, first: usize, count: usize },
}

/// The photo on screen.
pub struct Photo {
    pub dec: Option<Arc<Decoded>>,
    pub error: Option<String>,
    pub full_requested: bool,
    pub frame: usize,
    pub next_frame: Instant,
    pub bmps: Vec<Option<ID2D1Bitmap>>,
    pub bmp_rt: usize,
    /// Physical pixels per picture pixel (1.0: 100 %).
    pub zoom: f32,
    pub fit: bool,
    /// Offset of the picture's center from the stage's center, in DIP.
    pub ox: f32,
    pub oy: f32,
    /// Quarter turns shown on top of the decoded picture.
    pub turns: u32,
    /// Quarter turns written into the file and not yet in `dec`.
    pub written: u32,
    pub opened: Instant,
}

impl Photo {
    fn new(turns: u32) -> Photo {
        Photo {
            dec: None,
            error: None,
            full_requested: false,
            frame: 0,
            next_frame: Instant::now(),
            bmps: vec![],
            bmp_rt: 0,
            zoom: 1.0,
            fit: true,
            ox: 0.0,
            oy: 0.0,
            turns,
            written: 0,
            opened: Instant::now(),
        }
    }
    /// Size of the whole picture as shown, turns included, in its own pixels.
    pub fn dims(&self) -> Option<(f32, f32)> {
        let d = self.dec.as_ref()?;
        let (w, h) = (d.full_w as f32, d.full_h as f32);
        Some(if self.turns % 2 == 1 { (h, w) } else { (w, h) })
    }
}

/// The video on screen.
pub struct Video {
    pub player: Option<Player>,
    pub vgen: u64,
    pub error: Option<String>,
    pub loaded: bool,
    pub rate: f64,
    pub tracks: Vec<AudioTrack>,
    pub track: Option<u32>,
    pub subs: Vec<PathBuf>,
    pub sub: Option<usize>,
    pub cues: Vec<Cue>,
    pub bmp: Option<ID2D1Bitmap>,
    pub bmp_size: (u32, u32),
    pub bmp_rt: usize,
    /// The frame above the seek bar: its time and its picture.
    pub preview: Option<(f64, Px)>,
    pub preview_bmp: Option<(f64, ID2D1Bitmap)>,
    pub preview_asked: Option<f64>,
    /// Where the seek bar is being dragged to.
    pub seeking: Option<f64>,
    pub frames: u64,
}

pub struct Open {
    pub seq: Vec<PathBuf>,
    pub pos: usize,
    pub photo: Option<Photo>,
    pub video: Option<Video>,
}

impl Open {
    pub fn path(&self) -> &Path {
        &self.seq[self.pos]
    }
}

pub struct InfoData {
    pub path: PathBuf,
    pub exif: crate::exif_info::Info,
    pub size: u64,
    pub modified: i64,
    pub dims: Option<(u32, u32)>,
    pub duration: f64,
}

/// Colors of Plume Media.
#[derive(Clone)]
pub struct Pal {
    pub dark: bool,
    pub bg: Color,
    pub side: Color,
    pub card: Color,
    pub line: Color,
    pub t1: Color,
    pub t2: Color,
    pub t3: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub stage: Color,
    pub tile: Color,
}

impl Pal {
    pub fn new(dark: bool) -> Pal {
        if dark {
            Pal {
                dark,
                bg: Color::hex(0x121315),
                side: Color::hex(0x0b0c0d),
                card: Color::hex(0x1b1d20),
                line: Color::hex(0xffffff).fade(0.08),
                t1: Color::hex(0xffffff),
                t2: Color::hex(0xb7bbb9),
                t3: Color::hex(0x7d827f),
                accent: Color::hex(0x34d058),
                on_accent: Color::hex(0x06210e),
                stage: Color::hex(0x08090a),
                tile: Color::hex(0x1e2023),
            }
        } else {
            Pal {
                dark,
                bg: Color::hex(0xf6f7f6),
                side: Color::hex(0xecefed),
                card: Color::hex(0xffffff),
                line: Color::hex(0x000000).fade(0.08),
                t1: Color::hex(0x111413),
                t2: Color::hex(0x525a56),
                t3: Color::hex(0x8a918d),
                accent: Color::hex(0x16a34a),
                on_accent: Color::hex(0xffffff),
                stage: Color::hex(0x101112),
                tile: Color::hex(0xe1e5e2),
            }
        }
    }
}

/// A few background jobs at a time (thumbnails), the newest first: what was just scrolled into view.
type Job = Box<dyn FnOnce() + Send>;

pub struct Pool {
    busy: Arc<AtomicUsize>,
    waiting: Arc<Mutex<Vec<Job>>>,
}

impl Pool {
    fn new() -> Pool {
        Pool { busy: Arc::new(AtomicUsize::new(0)), waiting: Arc::new(Mutex::new(Vec::new())) }
    }
    pub fn run(&self, f: impl FnOnce() + Send + 'static) {
        self.waiting.lock().unwrap().push(Box::new(f));
        let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 6);
        if self.busy.load(Ordering::SeqCst) < workers {
            let (busy, waiting) = (self.busy.clone(), self.waiting.clone());
            busy.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || {
                com_mta();
                loop {
                    let job = waiting.lock().unwrap().pop();
                    match job {
                        Some(j) => j(),
                        None => break,
                    }
                }
                busy.fetch_sub(1, Ordering::SeqCst);
            });
        }
    }
}

pub fn com_mta() {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thumb {
    Pending,
    Failed,
}

/// Window state saved while in full screen.
pub struct Saved {
    style: isize,
    placement: windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT,
}

pub struct App {
    pub win: Window,
    #[allow(dead_code)]
    pub gfx: Rc<Gfx>,
    pub pal: Pal,
    pub set: Settings,
    pub store: Store,
    pub lib: Vec<Item>,
    pub page: Page,
    /// Items of the page (indexes into `lib`).
    pub list: Vec<usize>,
    pub rows: Vec<Row>,
    pub rows_w: f32,
    pub tile: f32,
    pub cols: usize,
    pub content_h: f32,
    pub scroll: f32,
    pub scroll_to: f32,
    pub view_h: f32,
    pub folders: Vec<(PathBuf, Vec<usize>)>,
    pub scanning: Option<Arc<Progress>>,
    pub scanned_once: bool,
    pub thumbs: HashMap<u64, Thumb>,
    pub thumb_px: HashMap<u64, Px>,
    pub durs: HashMap<u64, f64>,
    pub bitmaps: Bitmaps,
    pub wanted: Arc<Mutex<HashSet<u64>>>,
    pub pool: Pool,
    pub hits: plume_ui::Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub drag: Option<Drag>,
    pub mouse: Pt,
    pub open: Option<Open>,
    /// Pictures decoded ahead (the neighbours), newest last.
    pub decoded: Vec<(PathBuf, Arc<Decoded>)>,
    pub decoding: HashSet<(PathBuf, bool)>,
    pub info: Option<InfoData>,
    pub menu: Option<Menu>,
    pub toast: Option<(String, Instant)>,
    pub full: Option<Saved>,
    pub slideshow: Option<Instant>,
    pub last_move: Instant,
    pub preview_tx: Option<mpsc::Sender<(PathBuf, f64, u64)>>,
    pub video_gen: u64,
    pub size: (f32, f32),
    pub scale: f32,
    pub log: Vec<String>,
    pub frames: u64,
    pub openwith: bool,
    pub cache_bytes: Option<u64>,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

/// Runs `f` on the window's thread, with the app.
pub fn post(s: &plume_ui::Sender, f: impl FnOnce(&mut App, &mut Cx) + Send + 'static) {
    s.send(move |h, cx| {
        if let Some(app) = with_app(h) {
            f(app, cx);
        }
    });
}

pub fn theme_dark(set: &Settings) -> bool {
    match set.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => crate::system_dark(),
    }
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, set: Settings) -> App {
        let pal = Pal::new(theme_dark(&set));
        let lib = library::load_index();
        let mut app = App {
            win,
            gfx,
            pal,
            set,
            store: Store::load(),
            lib,
            page: Page::Timeline,
            list: vec![],
            rows: vec![],
            rows_w: 0.0,
            tile: 160.0,
            cols: 1,
            content_h: 0.0,
            scroll: 0.0,
            scroll_to: 0.0,
            view_h: 600.0,
            folders: vec![],
            scanning: None,
            scanned_once: false,
            thumbs: HashMap::new(),
            thumb_px: HashMap::new(),
            durs: HashMap::new(),
            bitmaps: Bitmaps::default(),
            wanted: Arc::new(Mutex::new(HashSet::new())),
            pool: Pool::new(),
            hits: Default::default(),
            hover: None,
            pressed: None,
            drag: None,
            mouse: Pt { x: -1.0, y: -1.0 },
            open: None,
            decoded: vec![],
            decoding: HashSet::new(),
            info: None,
            menu: None,
            toast: None,
            full: None,
            slideshow: None,
            last_move: Instant::now(),
            preview_tx: None,
            video_gen: 0,
            size: (1200.0, 800.0),
            scale: 1.0,
            log: vec![],
            frames: 0,
            openwith: crate::openwith::is_registered(),
            cache_bytes: None,
        };
        app.refresh_list();
        app
    }

    pub fn note(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 60 {
            self.log.remove(0);
        }
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        self.toast = Some((text.into(), Instant::now()));
        cx.timer(TICK, Some(200));
        cx.repaint();
    }

    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    // ---------------------------------------------------------------- library

    /// Starts a scan of the library folders in the background.
    pub fn rescan(&mut self) {
        if let Some(p) = &self.scanning {
            p.stop.store(true, Ordering::Relaxed);
        }
        let progress = Arc::new(Progress::default());
        self.scanning = Some(progress.clone());
        self.scanned_once = true;
        let folders = self.set.folders.clone();
        let old: HashMap<PathBuf, Item> = self.lib.iter().map(|i| (i.path.clone(), i.clone())).collect();
        let s = self.win.sender();
        std::thread::spawn(move || {
            com_mta();
            let items = library::scan(&folders, &old, &progress);
            if progress.stop.load(Ordering::Relaxed) {
                return;
            }
            library::save_index(&items);
            post(&s, move |app, cx| {
                if app.scanning.as_ref().is_some_and(|p| Arc::ptr_eq(p, &progress)) {
                    app.scanning = None;
                    app.lib = items;
                    app.refresh_list();
                    app.note(format!("scan: {} items", app.lib.len()));
                    cx.repaint();
                }
            });
        });
    }

    /// Makes the page's list (and the folders) again from the library.
    pub fn refresh_list(&mut self) {
        let fav = &self.store;
        self.list = match &self.page {
            Page::Timeline => (0..self.lib.len()).collect(),
            Page::Videos => (0..self.lib.len()).filter(|&i| self.lib[i].video).collect(),
            Page::Favorites => (0..self.lib.len()).filter(|&i| fav.is_favorite(&self.lib[i].path)).collect(),
            Page::Folder(dir) => {
                let d = library::norm(dir);
                (0..self.lib.len()).filter(|&i| library::norm(&self.lib[i].folder()) == d).collect()
            }
            Page::Folders | Page::Settings => vec![],
        };
        let mut by: HashMap<String, (PathBuf, Vec<usize>)> = HashMap::new();
        for (i, it) in self.lib.iter().enumerate() {
            let f = it.folder();
            by.entry(library::norm(&f)).or_insert_with(|| (f, vec![])).1.push(i);
        }
        let mut folders: Vec<(PathBuf, Vec<usize>)> = by.into_values().collect();
        folders.sort_by(|a, b| {
            let n = |p: &PathBuf| p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            library::natural_cmp(&n(&a.0), &n(&b.0))
        });
        self.folders = folders;
        self.rows_w = 0.0; // layout again at the next image
    }

    pub fn go(&mut self, page: Page, cx: &mut Cx) {
        if self.page != page {
            self.page = page;
            self.scroll = 0.0;
            self.scroll_to = 0.0;
        }
        if matches!(self.page, Page::Settings) {
            self.openwith = crate::openwith::is_registered();
            self.cache_bytes = None;
            let s = self.win.sender();
            self.pool.run(move || {
                let n = image::cache_size();
                post(&s, move |app, cx| {
                    app.cache_bytes = Some(n);
                    cx.repaint();
                });
            });
        }
        self.menu = None;
        self.refresh_list();
        cx.repaint();
    }

    /// Lays the grid out for a width (month titles on the timeline and in Videos).
    pub fn layout(&mut self, w: f32) {
        let target = [128.0, 176.0, 240.0][self.set.tiles.min(2) as usize];
        let gap = 4.0;
        let cols = (((w + gap) / (target + gap)).floor() as usize).max(2);
        let tile = ((w - gap * (cols as f32 - 1.0)) / cols as f32).floor();
        self.cols = cols;
        self.tile = tile;
        self.rows.clear();
        let mut y = 0.0;
        let months = matches!(self.page, Page::Timeline | Page::Videos);
        let mut i = 0;
        while i < self.list.len() {
            let mut end = self.list.len();
            if months {
                let m = self.lib[self.list[i]].month();
                end = i + self.list[i..].iter().take_while(|&&k| self.lib[k].month() == m).count();
                self.rows.push(Row::Head { y, title: crate::i18n::month_year(m.0, m.1), n: end - i });
                y += 52.0;
            }
            let mut k = i;
            while k < end {
                let count = cols.min(end - k);
                self.rows.push(Row::Tiles { y, first: k, count });
                y += tile + gap;
                k += count;
            }
            if months {
                y += 12.0;
            }
            i = end;
        }
        self.content_h = y + 24.0;
        self.rows_w = w;
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_h - self.view_h).max(0.0)
    }

    /// Asks for a thumbnail (it arrives in `thumb_px`).
    pub fn request_thumb(&mut self, idx: usize) {
        let it = &self.lib[idx];
        let key = it.key();
        if self.thumbs.contains_key(&key) || self.thumb_px.contains_key(&key) {
            return;
        }
        self.thumbs.insert(key, Thumb::Pending);
        let (path, video, wanted, s) = (it.path.clone(), it.video, self.wanted.clone(), self.win.sender());
        self.pool.run(move || {
            // scrolled away meanwhile: asked again when seen again
            if !wanted.lock().unwrap().contains(&key) {
                post(&s, move |app, _| {
                    app.thumbs.remove(&key);
                });
                return;
            }
            let r = image::thumbnail(&path, key, video);
            post(&s, move |app, cx| {
                match r {
                    Some((px, dur)) => {
                        app.thumbs.remove(&key);
                        app.thumb_px.insert(key, px);
                        if video {
                            app.durs.insert(key, dur);
                        }
                    }
                    None => {
                        app.thumbs.insert(key, Thumb::Failed);
                    }
                }
                cx.repaint();
            });
        });
    }

    /// The thumbnail bitmap of a library item, if it is ready (made from waiting pixels if needed).
    pub fn thumb_bitmap(&mut self, c: &mut Canvas, key: u64) -> Option<ID2D1Bitmap> {
        if let Some(b) = self.bitmaps.get(key) {
            return Some(b);
        }
        let px = self.thumb_px.remove(&key)?;
        let rt = c.target();
        self.bitmaps.put(&rt, key, &px)
    }

    pub fn add_folder(&mut self, dir: PathBuf, cx: &mut Cx) {
        let n = library::norm(&dir);
        if self.set.folders.iter().any(|f| library::norm(f) == n) {
            self.say(tr("Already in the library"), cx);
            return;
        }
        self.set.folders.push(dir);
        self.set.save();
        self.rescan();
        self.say(tr("Folder added"), cx);
    }

    pub fn pick_folder(&mut self, cx: &mut Cx) {
        cx.after(|w| {
            if let Some(dir) = plume_ui::dialogs::pick_folder(w.hwnd, tr("Add a folder")) {
                w.with(|h, cx| {
                    if let Some(app) = with_app(h) {
                        app.add_folder(dir, cx);
                    }
                });
            }
        });
    }

    pub fn settings_changed(&mut self, cx: &mut Cx) {
        crate::i18n::set(&self.set.lang);
        let dark = theme_dark(&self.set);
        if dark != self.pal.dark {
            self.pal = Pal::new(dark);
            self.win.set_dark(dark);
        }
        self.set.save();
        self.rows_w = 0.0;
        self.refresh_list();
        if let Some(v) = self.open.as_ref().and_then(|o| o.video.as_ref())
            && let Some(p) = &v.player
        {
            let (vol, muted) = self.sound();
            p.set_volume(vol, muted);
        }
        cx.repaint();
    }

    /// Volume and mute as the player must use them (always silent in tests).
    pub fn sound(&self) -> (f64, bool) {
        if settings::sound_allowed() { (self.set.volume as f64, self.set.muted) } else { (0.0, true) }
    }

    // ---------------------------------------------------------------- opening files

    /// Opens a file given on the command line: its folder becomes the sequence for ← →.
    pub fn open_file(&mut self, path: PathBuf, early: Option<std::thread::JoinHandle<Result<Decoded, String>>>, cx: &mut Cx) {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut seq = library::folder_files(&dir);
        let n = library::norm(&path);
        let pos = match seq.iter().position(|p| library::norm(p) == n) {
            Some(i) => i,
            None => {
                seq.insert(0, path.clone());
                0
            }
        };
        if let Some(h) = early {
            // decoded while the window was being made
            self.decoding.insert((path.clone(), false));
            let s = self.win.sender();
            let p = path.clone();
            std::thread::spawn(move || {
                let r = h.join().unwrap_or_else(|_| Err("decode".into()));
                post(&s, move |app, cx| app.decoded_arrived(p, false, 0, r, cx));
            });
        }
        self.open_seq(seq, pos, cx);
    }

    /// Opens the item at `pos` of a sequence (a page of the library, a folder).
    pub fn open_seq(&mut self, seq: Vec<PathBuf>, pos: usize, cx: &mut Cx) {
        if seq.is_empty() {
            return;
        }
        self.leave_current();
        self.open = Some(Open { seq, pos, photo: None, video: None });
        self.show_current(cx);
    }

    /// Saves what must be (where a video stopped) and stops the current video.
    fn leave_current(&mut self) {
        if let Some(o) = &mut self.open
            && let Some(v) = o.video.take()
            && let Some(p) = &v.player
        {
            let (pos, dur) = (p.position(), p.duration());
            let path = o.seq[o.pos].clone();
            self.store.set_resume(&path, pos, dur);
            self.store.save();
            p.pause();
        }
        self.info = None;
        self.menu = None;
    }

    fn show_current(&mut self, cx: &mut Cx) {
        let Some(o) = &mut self.open else { return };
        o.pos = o.pos.min(o.seq.len() - 1);
        let path = o.seq[o.pos].clone();
        self.win.set_title(&format!("{} - Plume Media", path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()));
        if image::is_video(&path) {
            self.open_video(path, cx);
        } else {
            let turns = self.store.rotation(&path);
            let mut ph = Photo::new(turns);
            if let Some((_, d)) = self.decoded.iter().find(|(p, _)| *p == path) {
                ph.dec = Some(d.clone());
            }
            let have = ph.dec.is_some();
            if let Some(o) = &mut self.open {
                o.photo = Some(ph);
                o.video = None;
            }
            if !have {
                self.decode(path.clone(), false);
            }
            self.fit_photo();
            self.preload();
        }
        if self.set.info_open {
            self.load_info();
        }
        cx.timer(TICK, Some(200));
        cx.repaint();
    }

    pub fn close_viewer(&mut self, cx: &mut Cx) {
        self.leave_current();
        self.open = None;
        self.slideshow = None;
        if self.full.is_some() {
            self.set_fullscreen(false);
        }
        self.win.set_title("Plume Media");
        if !self.scanned_once {
            self.rescan();
        }
        cx.repaint();
    }

    pub fn step(&mut self, d: i32, cx: &mut Cx) {
        let Some(o) = &self.open else { return };
        let n = o.seq.len() as i32;
        if n <= 1 {
            return;
        }
        let pos = (o.pos as i32 + d).rem_euclid(n) as usize;
        self.leave_current();
        if let Some(o) = &mut self.open {
            o.pos = pos;
        }
        self.show_current(cx);
    }

    /// Long side, in pixels, of a picture decoded for the screen.
    pub fn screen_px(&self) -> u32 {
        let (w, h) = monitor_size(self.win.hwnd);
        w.max(h).clamp(1280, 7680)
    }

    pub fn decode(&mut self, path: PathBuf, full: bool) {
        if !self.decoding.insert((path.clone(), full)) {
            return;
        }
        let max = if full { 16384 } else { self.screen_px() };
        let written =
            self.open.as_ref().and_then(|o| o.photo.as_ref()).filter(|_| self.open.as_ref().is_some_and(|o| o.path() == path)).map_or(0, |p| p.written);
        let s = self.win.sender();
        std::thread::spawn(move || {
            com_mta();
            let r = image::decode(&path, max);
            post(&s, move |app, cx| app.decoded_arrived(path, full, written, r, cx));
        });
    }

    pub fn decoded_arrived(&mut self, path: PathBuf, full: bool, written: u32, r: Result<Decoded, String>, cx: &mut Cx) {
        self.decoding.remove(&(path.clone(), full));
        let current = self.open.as_ref().is_some_and(|o| o.path() == path && o.photo.is_some());
        match r {
            Ok(d) => {
                let d = Arc::new(d);
                if !full {
                    self.decoded.retain(|(p, _)| *p != path);
                    self.decoded.push((path.clone(), d.clone()));
                    // a few screens' worth kept: the neighbours
                    while self.decoded.len() > 5 || self.decoded.iter().map(|e| e.1.bytes()).sum::<usize>() > 400 << 20 {
                        self.decoded.remove(0);
                    }
                }
                if current && let Some(ph) = self.open.as_mut().and_then(|o| o.photo.as_mut()) {
                    let better = ph.dec.as_ref().is_none_or(|old| full || !old.full || written > 0);
                    if better {
                        let first = ph.dec.is_none();
                        ph.dec = Some(d);
                        ph.bmps.clear();
                        ph.frame = 0;
                        ph.error = None;
                        if written > 0 {
                            ph.turns = (ph.turns + 4 - written % 4) % 4;
                            ph.written = ph.written.saturating_sub(written);
                        }
                        if first || ph.fit {
                            self.fit_photo();
                        }
                    }
                    self.note(format!("decoded {} full={full}", path.display()));
                }
            }
            Err(e) => {
                if current && let Some(ph) = self.open.as_mut().and_then(|o| o.photo.as_mut()) {
                    if ph.dec.is_none() {
                        ph.error = Some(e.clone());
                    }
                    self.note(format!("decode failed: {e}"));
                }
            }
        }
        cx.repaint();
    }

    /// Decodes the next and previous pictures ahead, so ← → show them at once.
    fn preload(&mut self) {
        let Some(o) = &self.open else { return };
        let n = o.seq.len();
        if n < 2 {
            return;
        }
        let mut next = vec![o.seq[(o.pos + 1) % n].clone()];
        if n > 2 {
            next.push(o.seq[(o.pos + n - 1) % n].clone());
        }
        for p in next {
            if image::is_photo(&p) && !self.decoded.iter().any(|(q, _)| *q == p) {
                self.decode(p, false);
            }
        }
    }

    /// The stage: where the picture or the video shows.
    pub fn stage(&self) -> Rect {
        let (w, h) = self.size;
        if self.full.is_some() {
            return Rect { x: 0.0, y: 0.0, w, h };
        }
        let info = if self.set.info_open && self.open.is_some() { INFO_W } else { 0.0 };
        Rect { x: 0.0, y: TOP_H, w: (w - info).max(100.0), h: (h - TOP_H).max(100.0) }
    }

    /// The zoom that shows the whole picture.
    pub fn fit_zoom(&self) -> Option<f32> {
        let ph = self.open.as_ref()?.photo.as_ref()?;
        let (iw, ih) = ph.dims()?;
        let st = self.stage();
        let margin = if self.full.is_some() { 0.0 } else { 16.0 };
        let k = ((st.w - margin) * self.scale / iw).min((st.h - margin) * self.scale / ih);
        Some(if self.set.zoom == "fill" { k } else { k.min(1.0) })
    }

    pub fn fit_photo(&mut self) {
        let z = self.fit_zoom();
        let actual = self.set.zoom == "actual";
        if let Some(ph) = self.open.as_mut().and_then(|o| o.photo.as_mut())
            && let Some(z) = z
        {
            if actual && ph.fit && ph.zoom == 1.0 && ph.dec.is_some() {
                // "actual size" opening: 100 %, unless the picture is smaller than the window anyway
                ph.fit = false;
                ph.zoom = 1.0;
                ph.ox = 0.0;
                ph.oy = 0.0;
                return;
            }
            if ph.fit {
                ph.zoom = z;
                ph.ox = 0.0;
                ph.oy = 0.0;
            }
        }
    }

    /// Zooms by `k` around a point of the window.
    pub fn zoom_at(&mut self, k: f32, p: Pt) {
        let st = self.stage();
        let fit = self.fit_zoom().unwrap_or(1.0);
        let scale = self.scale;
        let Some(ph) = self.open.as_mut().and_then(|o| o.photo.as_mut()) else { return };
        let Some((iw, ih)) = ph.dims() else { return };
        let z1 = ph.zoom;
        let z2 = (z1 * k).clamp(fit.min(0.05), 32.0);
        let (cx, cy) = (st.x + st.w / 2.0, st.y + st.h / 2.0);
        let r = z2 / z1;
        ph.ox = (p.x - cx) - (p.x - cx - ph.ox) * r;
        ph.oy = (p.y - cy) - (p.y - cy - ph.oy) * r;
        ph.zoom = z2;
        ph.fit = (z2 - fit).abs() < 0.001;
        clamp_pan(ph, st, iw, ih, scale);
        self.want_full();
    }

    pub fn zoom_to(&mut self, z: Option<f32>) {
        let st = self.stage();
        match z {
            None => {
                if let Some(ph) = self.open.as_mut().and_then(|o| o.photo.as_mut()) {
                    ph.fit = true;
                }
                self.fit_photo();
            }
            Some(z) => {
                let c = Pt { x: st.x + st.w / 2.0, y: st.y + st.h / 2.0 };
                let z1 = self.open.as_ref().and_then(|o| o.photo.as_ref()).map_or(1.0, |p| p.zoom);
                self.zoom_at(z / z1, c);
            }
        }
    }

    /// Past the decoded size, the picture is decoded again at full size.
    pub fn want_full(&mut self) {
        let Some(o) = &mut self.open else { return };
        let path = o.seq[o.pos].clone();
        let Some(ph) = &mut o.photo else { return };
        let Some(d) = &ph.dec else { return };
        let shown = d.full_w.max(d.full_h) as f32 * ph.zoom;
        if !d.full && !ph.full_requested && shown > d.w().max(d.h()) as f32 * 1.05 {
            ph.full_requested = true;
            self.decode(path, true);
        }
    }

    pub fn rotate(&mut self, cw: bool, cx: &mut Cx) {
        let Some(o) = &mut self.open else { return };
        let path = o.seq[o.pos].clone();
        let Some(ph) = &mut o.photo else { return };
        if ph.dec.is_none() {
            return;
        }
        let t = if cw { 1 } else { 3 };
        ph.turns = (ph.turns + t) % 4;
        ph.fit = true;
        self.fit_photo();
        cx.repaint();
        // the file: in the background, one rotation at a time
        let s = self.win.sender();
        let key = self.lib.iter().find(|i| i.path == path).map(|i| i.key());
        std::thread::spawn(move || {
            com_mta();
            static ONE: Mutex<()> = Mutex::new(());
            let _one = ONE.lock();
            let r = image::rotate_file(&path, t);
            post(&s, move |app, cx| app.rotated(path, t, key, r, cx));
        });
    }

    fn rotated(&mut self, path: PathBuf, t: u32, key: Option<u64>, r: Result<image::Rotated, String>, cx: &mut Cx) {
        self.decoded.retain(|(p, _)| *p != path);
        match r {
            Ok(image::Rotated::ViewOnly) => {
                let k = library::norm(&path);
                let v = (self.store.rotation(&path) + t) % 4;
                if v == 0 {
                    self.store.rotations.remove(&k);
                } else {
                    self.store.rotations.insert(k, v);
                }
                self.store.save();
                self.say(tr("Rotated in Plume Media only: this format can't be rewritten without loss."), cx);
            }
            Ok(how) => {
                self.note(format!("rotated {how:?}"));
                // the thumbnail is made again (the file kept its date)
                if let Some(k) = key {
                    let _ = std::fs::remove_file(image::thumbs_dir().join(format!("{k:016x}.jpg")));
                    self.bitmaps.remove(k);
                    self.thumb_px.remove(&k);
                    self.thumbs.remove(&k);
                }
                let current = self.open.as_ref().is_some_and(|o| o.path() == path && o.photo.is_some());
                if current {
                    if let Some(ph) = self.open.as_mut().and_then(|o| o.photo.as_mut()) {
                        ph.written += t;
                        ph.full_requested = false;
                    }
                    self.decoding.remove(&(path.clone(), false));
                    self.decode(path, false);
                    self.info = None;
                    if self.set.info_open {
                        self.load_info();
                    }
                }
            }
            Err(e) => {
                self.note(format!("rotate failed: {e}"));
                // shown as it really is
                let current = self.open.as_ref().is_some_and(|o| o.path() == path);
                if current && let Some(ph) = self.open.as_mut().and_then(|o| o.photo.as_mut()) {
                    ph.turns = (ph.turns + 4 - t) % 4;
                }
                self.fit_photo();
                self.say(tr("Couldn't rotate the file."), cx);
            }
        }
        cx.repaint();
    }

    pub fn toggle_favorite_path(&mut self, path: &Path, cx: &mut Cx) {
        let on = self.store.toggle_favorite(path);
        self.store.save();
        if self.page == Page::Favorites {
            self.refresh_list();
        }
        self.say(if on { tr("Added to favorites") } else { tr("Removed from favorites") }, cx);
    }

    pub fn load_info(&mut self) {
        let Some(o) = &self.open else { return };
        let path = o.seq[o.pos].clone();
        if self.info.as_ref().is_some_and(|i| i.path == path) {
            return;
        }
        let s = self.win.sender();
        self.pool.run(move || {
            let m = std::fs::metadata(&path).ok();
            let video = image::is_video(&path);
            let exif = if video { Default::default() } else { crate::exif_info::info(&path) };
            let dims = if video { None } else { image::size_of(&path) };
            let duration = if video { video::duration(&path) } else { 0.0 };
            let modified =
                m.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64);
            let data = InfoData { path, exif, size: m.map_or(0, |m| m.len()), modified: library::local(modified), dims, duration };
            post(&s, move |app, cx| {
                if app.open.as_ref().is_some_and(|o| o.path() == data.path) {
                    app.info = Some(data);
                    cx.repaint();
                }
            });
        });
    }

    // ---------------------------------------------------------------- video

    fn open_video(&mut self, path: PathBuf, cx: &mut Cx) {
        self.video_gen += 1;
        let vgen = self.video_gen;
        let s = self.win.sender();
        let on: video::OnEvent = Arc::new(move |ev| post(&s, move |app, cx| app.video_event(vgen, ev, cx)));
        let (vol, muted) = self.sound();
        let (player, error) = match Player::open(&path, on, vol, muted) {
            Ok(p) => (Some(p), None),
            Err(e) => (None, Some(e.message())),
        };
        let subs = srt::find(&path);
        let sub = (self.set.subtitles && !subs.is_empty()).then(|| default_subs(&path, &subs));
        let cues = sub.map(|i| srt::load(&subs[i])).unwrap_or_default();
        let v = Video {
            player,
            vgen,
            error,
            loaded: false,
            rate: 1.0,
            tracks: vec![],
            track: None,
            subs,
            sub,
            cues,
            bmp: None,
            bmp_size: (0, 0),
            bmp_rt: 0,
            preview: None,
            preview_bmp: None,
            preview_asked: None,
            seeking: None,
            frames: 0,
        };
        if let Some(o) = &mut self.open {
            o.video = Some(v);
            o.photo = None;
        }
        let s = self.win.sender();
        self.pool.run(move || {
            let tracks = video::audio_tracks(&path);
            post(&s, move |app, cx| {
                if let Some(v) = app.open.as_mut().and_then(|o| o.video.as_mut()).filter(|v| v.vgen == vgen) {
                    v.track = v.player.as_ref().and_then(|p| tracks.iter().find(|t| p.selected(t.index))).or(tracks.first()).map(|t| t.index);
                    v.tracks = tracks;
                    cx.repaint();
                }
            });
        });
        self.last_move = Instant::now();
        cx.repaint();
    }

    fn video_event(&mut self, vgen: u64, ev: Ev, cx: &mut Cx) {
        let path = self.open.as_ref().map(|o| o.path().to_path_buf());
        let resume = path.as_ref().and_then(|p| self.store.resume_at(p));
        let Some(v) = self.open.as_mut().and_then(|o| o.video.as_mut()).filter(|v| v.vgen == vgen) else { return };
        if !matches!(ev, Ev::Other(_)) {
            self.log.push(format!("video {ev:?}"));
        }
        let mut toast = None;
        match ev {
            Ev::Loaded => {
                v.loaded = true;
                if let Some(p) = &v.player
                    && let Some(t) = v.tracks.iter().find(|t| p.selected(t.index))
                {
                    v.track = Some(t.index);
                }
                if let Some(p) = &v.player {
                    let dur = p.duration();
                    if let Some(t) = resume.filter(|t| *t < dur - 10.0) {
                        p.seek(t);
                        toast = Some(fill(tr("Resuming at {t}"), &[&library::clock(t)]));
                    }
                    p.set_rate(v.rate);
                    p.play();
                }
            }
            Ev::Ended => {
                if let (Some(p), Some(path)) = (&v.player, &path) {
                    self.store.set_resume(path, p.duration(), p.duration());
                    self.store.save();
                }
            }
            Ev::Error(code) => {
                v.error = Some(format!("{code:#x}"));
            }
            _ => {}
        }
        if let Some(t) = toast {
            self.say(t, cx);
        }
        cx.repaint();
    }

    pub fn video(&self) -> Option<&Video> {
        self.open.as_ref()?.video.as_ref()
    }

    pub fn player(&self) -> Option<&Player> {
        self.video()?.player.as_ref()
    }

    pub fn toggle_play(&mut self, cx: &mut Cx) {
        if let Some(p) = self.player() {
            if p.paused() || p.ended() {
                if p.ended() {
                    p.seek(0.0);
                }
                p.play();
            } else {
                p.pause();
                let (path, pos, dur) = (self.open.as_ref().map(|o| o.path().to_path_buf()), p.position(), p.duration());
                if let Some(path) = path {
                    self.store.set_resume(&path, pos, dur);
                    self.store.save();
                }
            }
        }
        self.last_move = Instant::now();
        cx.repaint();
    }

    pub fn seek_by(&mut self, d: f64, cx: &mut Cx) {
        if let Some(p) = self.player() {
            let dur = p.duration();
            let t = (p.position() + d).clamp(0.0, if dur > 0.0 { dur - 0.1 } else { f64::MAX });
            p.seek(t);
        }
        self.last_move = Instant::now();
        cx.repaint();
    }

    pub fn set_volume(&mut self, v: f32, cx: &mut Cx) {
        self.set.volume = v.clamp(0.0, 1.0);
        if self.set.volume > 0.0 {
            self.set.muted = false;
        }
        let (vol, muted) = self.sound();
        if let Some(p) = self.player() {
            p.set_volume(vol, muted);
        }
        cx.repaint();
    }

    pub fn toggle_mute(&mut self, cx: &mut Cx) {
        self.set.muted = !self.set.muted;
        let (vol, muted) = self.sound();
        if let Some(p) = self.player() {
            p.set_volume(vol, muted);
        }
        self.set.save();
        cx.repaint();
    }

    pub fn set_rate(&mut self, r: f64, cx: &mut Cx) {
        if let Some(v) = self.open.as_mut().and_then(|o| o.video.as_mut()) {
            v.rate = r.clamp(0.5, 2.0);
            if let Some(p) = &v.player {
                p.set_rate(v.rate);
            }
        }
        cx.repaint();
    }

    pub fn choose_subs(&mut self, i: Option<usize>, cx: &mut Cx) {
        if let Some(v) = self.open.as_mut().and_then(|o| o.video.as_mut()) {
            v.sub = i.filter(|&k| k < v.subs.len());
            v.cues = v.sub.map(|k| srt::load(&v.subs[k])).unwrap_or_default();
            self.set.subtitles = v.sub.is_some();
            self.set.save();
        }
        cx.repaint();
    }

    pub fn choose_audio(&mut self, index: u32, cx: &mut Cx) {
        if let Some(v) = self.open.as_mut().and_then(|o| o.video.as_mut()) {
            let all: Vec<u32> = v.tracks.iter().map(|t| t.index).collect();
            if let Some(p) = &v.player
                && p.choose_audio(index, &all)
            {
                v.track = Some(index);
            }
        }
        cx.repaint();
    }

    /// Asks for the frame shown above the seek bar at `t`.
    pub fn ask_preview(&mut self, t: f64) {
        let Some(o) = &self.open else { return };
        let path = o.seq[o.pos].clone();
        let Some(v) = &self.open.as_ref().and_then(|o| o.video.as_ref()) else { return };
        let dur = v.player.as_ref().map_or(0.0, |p| p.duration());
        if dur <= 0.0 {
            return;
        }
        // one picture per 1/150 of the video
        let step = (dur / 150.0).max(1.0);
        let t = ((t / step).round() * step).clamp(0.0, dur);
        if v.preview_asked == Some(t) {
            return;
        }
        let vgen = v.vgen;
        if let Some(v) = self.open.as_mut().and_then(|o| o.video.as_mut()) {
            v.preview_asked = Some(t);
        }
        if self.preview_tx.is_none() {
            self.preview_tx = Some(preview_thread(self.win.sender()));
        }
        if let Some(tx) = &self.preview_tx {
            let _ = tx.send((path, t, vgen));
        }
    }

    // ---------------------------------------------------------------- window

    pub fn set_fullscreen(&mut self, on: bool) {
        use windows::Win32::{
            Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
            UI::WindowsAndMessaging::*,
        };
        let hwnd = self.win.hwnd;
        unsafe {
            if on && self.full.is_none() {
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                let mut placement = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
                let _ = GetWindowPlacement(hwnd, &mut placement);
                let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
                let _ = GetMonitorInfoW(mon, &mut mi);
                self.full = Some(Saved { style, placement });
                SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !(WS_OVERLAPPEDWINDOW.0 as isize)) | WS_POPUP.0 as isize | WS_VISIBLE.0 as isize);
                let r = mi.rcMonitor;
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_FRAMECHANGED | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
                );
            } else if !on && let Some(s) = self.full.take() {
                SetWindowLongPtrW(hwnd, GWL_STYLE, s.style);
                let _ = SetWindowPlacement(hwnd, &s.placement);
                let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOOWNERZORDER | SWP_NOACTIVATE);
            }
        }
        self.fit_photo();
    }

    pub fn toggle_fullscreen(&mut self, cx: &mut Cx) {
        let on = self.full.is_none();
        self.set_fullscreen(on);
        if !on {
            self.slideshow = None;
        }
        cx.repaint();
    }

    pub fn start_slideshow(&mut self, cx: &mut Cx) {
        if self.open.is_none() {
            return;
        }
        self.slideshow = Some(Instant::now() + Duration::from_secs(self.set.slideshow as u64));
        if self.full.is_none() {
            self.set_fullscreen(true);
        }
        cx.timer(TICK, Some(200));
        self.say(tr("Slideshow: Esc to stop"), cx);
    }

    /// The controls of the player show while the mouse moves, and always when paused.
    pub fn controls_shown(&self) -> bool {
        let Some(p) = self.player() else { return true };
        p.paused() || p.ended() || self.last_move.elapsed() < Duration::from_millis(2600) || self.menu.is_some() || self.drag.is_some()
    }

    /// The bars of the viewer show unless in full screen with the mouse still.
    pub fn bars_shown(&self) -> bool {
        if self.full.is_none() {
            return true;
        }
        if self.slideshow.is_some() {
            return self.last_move.elapsed() < Duration::from_millis(1500);
        }
        if self.video().is_some() {
            return self.controls_shown();
        }
        self.last_move.elapsed() < Duration::from_millis(2600) || self.mouse.y < TOP_H
    }

    fn tick(&mut self, cx: &mut Cx) {
        let mut keep = false;
        if let Some((_, t)) = &self.toast {
            if t.elapsed() > Duration::from_millis(2600) {
                self.toast = None;
                cx.repaint();
            } else {
                keep = true;
            }
        }
        if let Some(due) = self.slideshow {
            keep = true;
            if Instant::now() >= due {
                // photos only: videos are skipped
                let n = self.open.as_ref().map_or(0, |o| o.seq.len());
                let mut d = 1;
                while d < n as i32 {
                    let o = self.open.as_ref().unwrap();
                    let p = &o.seq[(o.pos + d as usize) % n];
                    if image::is_photo(p) {
                        break;
                    }
                    d += 1;
                }
                self.step(d, cx);
                self.slideshow = Some(Instant::now() + Duration::from_secs(self.set.slideshow as u64));
            }
        }
        if self.open.is_some() {
            keep = true;
            // bars and controls hide by themselves
            cx.repaint();
        }
        if self.scanning.is_some() {
            keep = true;
            cx.repaint();
        }
        if !keep {
            cx.timer(TICK, None);
        }
    }

    fn act(&mut self, h: Hit, p: Pt, clicks: u8, cx: &mut Cx) {
        match h {
            Hit::Min => self.win.minimize(),
            Hit::Max => self.win.toggle_maximize(),
            Hit::Close => self.win.close(),
            Hit::Nav(pg) => {
                if self.open.is_some() {
                    self.close_viewer(cx);
                }
                self.go(pg, cx)
            }
            Hit::AddFolder | Hit::EmptyAdd => self.pick_folder(cx),
            Hit::Rescan => {
                self.rescan();
                cx.timer(TICK, Some(200));
                cx.repaint();
            }
            Hit::Tile(i) => {
                let seq: Vec<PathBuf> = self.list.iter().map(|&k| self.lib[k].path.clone()).collect();
                self.open_seq(seq, i, cx);
            }
            Hit::FolderCard(i) => {
                if let Some((dir, _)) = self.folders.get(i) {
                    let d = dir.clone();
                    self.go(Page::Folder(d), cx);
                }
            }
            Hit::Back => {
                if self.full.is_some() {
                    self.toggle_fullscreen(cx);
                }
                self.close_viewer(cx);
            }
            Hit::Prev => self.step(-1, cx),
            Hit::Next => self.step(1, cx),
            Hit::ZoomIn => {
                self.zoom_at_center(1.25);
                cx.repaint();
            }
            Hit::ZoomOut => {
                self.zoom_at_center(0.8);
                cx.repaint();
            }
            Hit::ZoomFit => {
                let fit = self.open.as_ref().and_then(|o| o.photo.as_ref()).is_some_and(|p| p.fit);
                self.zoom_to(if fit { Some(1.0) } else { None });
                cx.repaint();
            }
            Hit::RotL => self.rotate(false, cx),
            Hit::RotR => self.rotate(true, cx),
            Hit::Fav => {
                if let Some(path) = self.open.as_ref().map(|o| o.path().to_path_buf()) {
                    self.toggle_favorite_path(&path, cx);
                }
            }
            Hit::Info => {
                self.set.info_open = !self.set.info_open;
                self.set.save();
                if self.set.info_open {
                    self.load_info();
                }
                self.fit_photo();
                cx.repaint();
            }
            Hit::Slideshow => {
                if self.slideshow.is_some() {
                    self.slideshow = None;
                } else {
                    self.start_slideshow(cx);
                }
                cx.repaint();
            }
            Hit::Fullscreen => self.toggle_fullscreen(cx),
            Hit::Stage => {
                if self.video().is_some() {
                    self.toggle_play(cx);
                    if clicks == 2 {
                        self.toggle_fullscreen(cx);
                    }
                } else if clicks == 2 {
                    // double click: whole picture ↔ 100 % where clicked
                    let fit = self.open.as_ref().and_then(|o| o.photo.as_ref()).is_some_and(|p| p.fit);
                    if fit {
                        let z1 = self.open.as_ref().and_then(|o| o.photo.as_ref()).map_or(1.0, |p| p.zoom);
                        self.zoom_at(1.0 / z1, p);
                    } else {
                        self.zoom_to(None);
                    }
                    cx.repaint();
                }
            }
            Hit::PlayPause => self.toggle_play(cx),
            Hit::Mute => self.toggle_mute(cx),
            Hit::Speed => {
                let cur = self.video().map_or(1.0, |v| v.rate);
                let items = [0.5f64, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]
                    .iter()
                    .map(|&r| (if r == 1.0 { tr("Normal").to_string() } else { format!("{r}×") }, (r - cur).abs() < 0.01, MenuAct::Speed(r)))
                    .collect();
                self.open_menu(items, Hit::Speed);
            }
            Hit::Subs => {
                let Some(v) = self.video() else { return };
                let path = self.open.as_ref().map(|o| o.path().to_path_buf()).unwrap_or_default();
                let mut items = vec![(tr("Off").to_string(), v.sub.is_none(), MenuAct::Subs(None))];
                for (i, s) in v.subs.iter().enumerate() {
                    let l = srt::label(&path, s);
                    let name = if l.is_empty() { s.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() } else { video::language_name(&l) };
                    items.push((name, v.sub == Some(i), MenuAct::Subs(Some(i))));
                }
                self.open_menu(items, Hit::Subs);
            }
            Hit::Audio => {
                let Some(v) = self.video() else { return };
                let items = v.tracks.iter().map(|t| (t.label.clone(), v.track == Some(t.index), MenuAct::Audio(t.index))).collect();
                self.open_menu(items, Hit::Audio);
            }
            Hit::Menu(i) => {
                if let Some(m) = self.menu.take()
                    && let Some((_, _, a)) = m.items.get(i).cloned()
                {
                    self.menu_act(a, cx);
                }
                cx.repaint();
            }
            Hit::Theme(t) => {
                self.set.theme = t.into();
                self.settings_changed(cx);
            }
            Hit::Lang(l) => {
                self.set.lang = l.into();
                self.settings_changed(cx);
            }
            Hit::Zoom(z) => {
                self.set.zoom = z.into();
                self.settings_changed(cx);
            }
            Hit::SlideSpeed(s) => {
                self.set.slideshow = s;
                self.settings_changed(cx);
            }
            Hit::Tiles(t) => {
                self.set.tiles = t;
                self.settings_changed(cx);
            }
            Hit::RemoveFolder(i) => {
                if i < self.set.folders.len() {
                    self.set.folders.remove(i);
                    self.set.save();
                    self.rescan();
                    cx.timer(TICK, Some(200));
                }
                cx.repaint();
            }
            Hit::OpenWith => {
                if self.openwith {
                    crate::openwith::unregister();
                    self.openwith = crate::openwith::is_registered();
                    self.say(tr("Removed from \"Open with\""), cx);
                } else {
                    let exe = std::env::current_exe().unwrap_or_default();
                    crate::openwith::register(&exe);
                    self.openwith = crate::openwith::is_registered();
                    self.say(tr("Added to \"Open with\""), cx);
                }
            }
            Hit::ClearCache => {
                image::clear_cache();
                self.thumbs.clear();
                self.thumb_px.clear();
                self.bitmaps = Bitmaps::default();
                self.cache_bytes = Some(image::cache_size());
                self.say(tr("Thumbnails cleared"), cx);
            }
            Hit::Seek | Hit::Volume | Hit::Scrollbar | Hit::Panel | Hit::Controls => {}
        }
    }

    pub fn zoom_at_center(&mut self, k: f32) {
        let st = self.stage();
        self.zoom_at(k, Pt { x: st.x + st.w / 2.0, y: st.y + st.h / 2.0 });
    }

    fn open_menu(&mut self, items: Vec<(String, bool, MenuAct)>, from: Hit) {
        if items.is_empty() {
            return;
        }
        let anchor = self.hits.rect_of(|h| *h == from).unwrap_or(Rect { x: self.mouse.x, y: self.mouse.y, w: 1.0, h: 1.0 });
        let up = self.video().is_some();
        self.menu = Some(Menu { items, anchor, up });
    }

    fn menu_act(&mut self, a: MenuAct, cx: &mut Cx) {
        match a {
            MenuAct::Speed(r) => self.set_rate(r, cx),
            MenuAct::Audio(i) => self.choose_audio(i, cx),
            MenuAct::Subs(i) => self.choose_subs(i, cx),
            MenuAct::Open(i) => self.act(Hit::Tile(i), self.mouse, 1, cx),
            MenuAct::Favorite(i) => {
                if let Some(&k) = self.list.get(i) {
                    let p = self.lib[k].path.clone();
                    self.toggle_favorite_path(&p, cx);
                }
            }
            MenuAct::Explorer(i) => {
                if let Some(&k) = self.list.get(i) {
                    show_in_explorer(&self.lib[k].path);
                }
            }
        }
    }

    pub fn open_args(&mut self, text: &str, cx: &mut Cx) {
        let args = split_args(text);
        if let Some(p) = args.into_iter().map(PathBuf::from).find(|p| p.is_file()) {
            self.open_file(p, None, cx);
        }
        // a test window never takes the focus
        if std::env::var_os("PLUME_MEDIA_NOACTIVATE").is_none() {
            self.win.bring_to_front();
        }
    }

    fn handle(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                let moved = (p.x - self.mouse.x).abs() + (p.y - self.mouse.y).abs() > 0.5;
                self.mouse = p;
                if moved {
                    self.last_move = Instant::now();
                    if self.open.is_some() {
                        cx.timer(TICK, Some(200));
                        cx.repaint();
                    }
                }
                if let Some(d) = self.drag.clone() {
                    self.drag_to(d, p, cx);
                    return;
                }
                let h = self.hits.at(p).cloned();
                if h != self.hover {
                    self.hover = h;
                    cx.repaint();
                }
                if self.hover == Some(Hit::Seek) {
                    let t = self.seek_time(p.x);
                    self.ask_preview(t);
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                self.hover = None;
                self.mouse = Pt { x: -1.0, y: -1.0 };
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Left, clicks, .. } => {
                let h = self.hits.at(p).cloned();
                if self.menu.is_some() && !matches!(h, Some(Hit::Menu(_))) {
                    self.menu = None;
                    cx.repaint();
                    return;
                }
                self.pressed = h.clone();
                match h {
                    Some(Hit::Seek) => {
                        self.drag = Some(Drag::Seek);
                        cx.capture(true);
                        self.drag_to(Drag::Seek, p, cx);
                    }
                    Some(Hit::Volume) => {
                        self.drag = Some(Drag::Volume);
                        self.drag_to(Drag::Volume, p, cx);
                    }
                    Some(Hit::Scrollbar) => {
                        self.drag = Some(Drag::Scroll { from_y: p.y, scroll: self.scroll });
                    }
                    Some(Hit::Stage) if self.open.as_ref().is_some_and(|o| o.photo.is_some()) => {
                        let (ox, oy) = self.open.as_ref().and_then(|o| o.photo.as_ref()).map_or((0.0, 0.0), |ph| (ph.ox, ph.oy));
                        self.drag = Some(Drag::Pan { from: p, ox, oy, moved: false });
                        if clicks == 2 {
                            self.act(Hit::Stage, p, 2, cx);
                            self.pressed = None;
                        }
                    }
                    Some(Hit::Stage) if clicks == 2 => {
                        self.act(Hit::Stage, p, 2, cx);
                        self.pressed = None;
                    }
                    _ => {}
                }
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                let drag = self.drag.take();
                cx.capture(false);
                let pressed = self.pressed.take();
                match drag {
                    Some(Drag::Seek) => {
                        if let Some(v) = self.open.as_mut().and_then(|o| o.video.as_mut())
                            && let Some(t) = v.seeking.take()
                            && let Some(pl) = &v.player
                        {
                            pl.seek(t);
                        }
                        cx.repaint();
                        return;
                    }
                    Some(Drag::Volume) => {
                        self.set.save();
                        return;
                    }
                    Some(Drag::Scroll { .. }) => return,
                    Some(Drag::Pan { moved: true, .. }) => return,
                    _ => {}
                }
                let h = self.hits.at(p).cloned();
                if let Some(h) = h.filter(|h| Some(h) == pressed.as_ref()) {
                    // the double click of the stage was handled on its press
                    self.act(h, p, 1, cx);
                }
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Right, .. } => {
                if let Some(Hit::Tile(i)) = self.hits.at(p).cloned() {
                    let fav = self.list.get(i).is_some_and(|&k| self.store.is_favorite(&self.lib[k].path));
                    let items = vec![
                        (tr("Open").to_string(), false, MenuAct::Open(i)),
                        (if fav { tr("Remove from favorites") } else { tr("Add to favorites") }.to_string(), false, MenuAct::Favorite(i)),
                        (tr("Show in File Explorer").to_string(), false, MenuAct::Explorer(i)),
                    ];
                    self.menu = Some(Menu { items, anchor: Rect { x: p.x, y: p.y, w: 1.0, h: 1.0 }, up: false });
                    cx.repaint();
                }
            }
            Event::MouseDown { button: Button::Back, .. } => {
                if self.open.is_some() {
                    self.close_viewer(cx);
                }
            }
            Event::Wheel { p, dy, mods, .. } => self.wheel(p, dy, mods, cx),
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::Timer(TICK) => self.tick(cx),
            Event::Resized | Event::Scale => {
                self.scale = self.win.scale();
                self.fit_photo();
                cx.repaint();
            }
            Event::SystemTheme => {
                if self.set.theme == "system" {
                    let dark = crate::system_dark();
                    if dark != self.pal.dark {
                        self.pal = Pal::new(dark);
                        self.win.set_dark(dark);
                        cx.repaint();
                    }
                }
            }
            Event::Opened(text) => self.open_args(&text, cx),
            Event::Close => {
                self.leave_current();
                if let Some(p) = &self.scanning {
                    p.stop.store(true, Ordering::Relaxed);
                }
                self.set.save();
            }
            _ => {}
        }
    }

    fn drag_to(&mut self, d: Drag, p: Pt, cx: &mut Cx) {
        match d {
            Drag::Pan { from, ox, oy, moved } => {
                let st = self.stage();
                let scale = self.scale;
                let far = (p.x - from.x).abs() + (p.y - from.y).abs() > 3.0;
                if let Some(ph) = self.open.as_mut().and_then(|o| o.photo.as_mut())
                    && let Some((iw, ih)) = ph.dims()
                {
                    ph.ox = ox + p.x - from.x;
                    ph.oy = oy + p.y - from.y;
                    clamp_pan(ph, st, iw, ih, scale);
                }
                self.drag = Some(Drag::Pan { from, ox, oy, moved: moved || far });
                cx.repaint();
            }
            Drag::Seek => {
                let t = self.seek_time(p.x);
                if let Some(v) = self.open.as_mut().and_then(|o| o.video.as_mut()) {
                    v.seeking = Some(t);
                    if let Some(pl) = &v.player {
                        pl.seek_fast(t);
                    }
                }
                self.ask_preview(t);
                cx.repaint();
            }
            Drag::Volume => {
                if let Some(r) = self.hits.rect_of(|h| *h == Hit::Volume) {
                    let v = ((p.x - r.x - 6.0) / (r.w - 12.0)).clamp(0.0, 1.0);
                    self.set_volume(v, cx);
                }
            }
            Drag::Scroll { from_y, scroll } => {
                let track = self.view_h.max(1.0);
                let k = self.content_h / track;
                self.scroll = (scroll + (p.y - from_y) * k).clamp(0.0, self.max_scroll());
                self.scroll_to = self.scroll;
                cx.repaint();
            }
        }
    }

    /// The time under `x` on the seek bar.
    pub fn seek_time(&self, x: f32) -> f64 {
        let Some(r) = self.hits.rect_of(|h| *h == Hit::Seek) else { return 0.0 };
        let dur = self.player().map_or(0.0, |p| p.duration());
        let k = ((x - r.x - 8.0) / (r.w - 16.0)).clamp(0.0, 1.0) as f64;
        k * dur
    }

    fn wheel(&mut self, p: Pt, dy: f32, mods: Mods, cx: &mut Cx) {
        if self.menu.is_some() {
            return;
        }
        if let Some(o) = &self.open {
            if o.photo.is_some() {
                // the wheel and the touchpad's pinch (Ctrl + wheel) zoom where the pointer is
                let k = if mods.ctrl { 1.1f32.powf(dy) } else { 1.2f32.powf(dy) };
                self.zoom_at(k, p);
                cx.repaint();
            } else if o.video.is_some() {
                self.set_volume(self.set.volume + dy * 0.05, cx);
                self.last_move = Instant::now();
            }
            return;
        }
        self.scroll_to = (self.scroll_to - dy * 140.0).clamp(0.0, self.max_scroll());
        cx.repaint();
    }

    fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        const LEFT: u16 = 0x25;
        const UP: u16 = 0x26;
        const RIGHT: u16 = 0x27;
        const DOWN: u16 = 0x28;
        if self.menu.is_some() && vk == 0x1B {
            self.menu = None;
            cx.repaint();
            cx.handled();
            return;
        }
        let Some(o) = &self.open else {
            match vk {
                0x21 => self.scroll_to = (self.scroll_to - self.view_h * 0.9).max(0.0),
                0x22 => self.scroll_to = (self.scroll_to + self.view_h * 0.9).min(self.max_scroll()),
                0x24 => self.scroll_to = 0.0,
                0x23 => self.scroll_to = self.max_scroll(),
                UP => self.scroll_to = (self.scroll_to - 120.0).max(0.0),
                DOWN => self.scroll_to = (self.scroll_to + 120.0).min(self.max_scroll()),
                0x74 => self.rescan(),
                _ => return,
            }
            cx.handled();
            cx.repaint();
            return;
        };
        let video = o.video.is_some();
        cx.handled();
        match vk {
            0x1B => {
                if self.slideshow.is_some() {
                    self.slideshow = None;
                    self.set_fullscreen(false);
                } else if self.full.is_some() {
                    self.set_fullscreen(false);
                } else {
                    self.close_viewer(cx);
                }
            }
            0x7A | 0x46 => self.toggle_fullscreen(cx), // F11, F
            0x49 => self.act(Hit::Info, self.mouse, 1, cx),
            0x44 if mods.ctrl => self.act(Hit::Fav, self.mouse, 1, cx),
            0x21 => self.step(-1, cx),
            0x22 => self.step(1, cx),
            _ if video => match vk {
                0x20 | 0x4B => self.toggle_play(cx),
                LEFT => self.seek_by(if mods.shift { -30.0 } else { -5.0 }, cx),
                RIGHT => self.seek_by(if mods.shift { 30.0 } else { 5.0 }, cx),
                UP => self.set_volume(self.set.volume + 0.05, cx),
                DOWN => self.set_volume(self.set.volume - 0.05, cx),
                0x4D => self.toggle_mute(cx),
                0x43 => {
                    let next = self.video().map(|v| match v.sub {
                        None if !v.subs.is_empty() => Some(0),
                        Some(i) if i + 1 < v.subs.len() => Some(i + 1),
                        _ => None,
                    });
                    if let Some(n) = next {
                        self.choose_subs(n, cx);
                    }
                }
                0xDB => self.set_rate(self.video().map_or(1.0, |v| v.rate) - 0.25, cx),
                0xDD => self.set_rate(self.video().map_or(1.0, |v| v.rate) + 0.25, cx),
                0x24 => self.seek_by(-1e9, cx),
                _ => return,
            },
            LEFT => self.step(-1, cx),
            RIGHT | 0x20 => self.step(1, cx),
            0x24 => {
                let d = -(o.pos as i32);
                self.step(d, cx);
            }
            0x23 => {
                let d = (o.seq.len() - 1 - o.pos) as i32;
                self.step(d, cx);
            }
            0xBB | 0x6B => self.zoom_at_center(1.25),
            0xBD | 0x6D => self.zoom_at_center(0.8),
            0x30 | 0x60 => self.zoom_to(None),
            0x31 | 0x61 => self.zoom_to(Some(1.0)),
            0x52 => self.rotate(!mods.shift, cx),
            0x4C => self.rotate(false, cx),
            0x53 => self.act(Hit::Slideshow, self.mouse, 1, cx),
            _ => return,
        }
        cx.repaint();
    }

    pub fn animating(&self) -> bool {
        if (self.scroll - self.scroll_to).abs() > 0.5 {
            return true;
        }
        if let Some(o) = &self.open {
            if let Some(ph) = &o.photo {
                return ph.dec.as_ref().is_some_and(|d| d.frames.len() > 1);
            }
            if let Some(p) = o.video.as_ref().and_then(|v| v.player.as_ref()) {
                return !p.paused() || !o.video.as_ref().is_some_and(|v| v.loaded) || o.video.as_ref().is_some_and(|v| v.frames == 0);
            }
        }
        false
    }
}

/// Keeps the picture on the stage: centered when smaller, edges never past the stage's when larger.
pub fn clamp_pan(ph: &mut Photo, st: Rect, iw: f32, ih: f32, scale: f32) {
    let (dw, dh) = (iw * ph.zoom / scale, ih * ph.zoom / scale);
    let mx = ((dw - st.w) / 2.0).max(0.0);
    let my = ((dh - st.h) / 2.0).max(0.0);
    ph.ox = ph.ox.clamp(-mx, mx);
    ph.oy = ph.oy.clamp(-my, my);
}

/// The size in pixels of the screen the window is on.
pub fn monitor_size(hwnd: windows::Win32::Foundation::HWND) -> (u32, u32) {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
    unsafe {
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut mi).as_bool() {
            let r = mi.rcMonitor;
            return ((r.right - r.left).max(1) as u32, (r.bottom - r.top).max(1) as u32);
        }
    }
    (2560, 1440)
}

/// A thread that reads frames for the seek bar's preview: only the newest request counts.
fn preview_thread(s: plume_ui::Sender) -> mpsc::Sender<(PathBuf, f64, u64)> {
    let (tx, rx) = mpsc::channel::<(PathBuf, f64, u64)>();
    std::thread::spawn(move || {
        com_mta();
        let mut open: Option<(PathBuf, video::Frames)> = None;
        while let Ok(mut job) = rx.recv() {
            while let Ok(j) = rx.try_recv() {
                job = j;
            }
            let (path, t, vgen) = job;
            if open.as_ref().is_none_or(|(p, _)| *p != path) {
                open = video::Frames::open(&path).map(|f| (path.clone(), f));
            }
            let Some((_, f)) = &open else { continue };
            if let Some(px) = f.at(t, 320) {
                post(&s, move |app, cx| {
                    if let Some(v) = app.open.as_mut().and_then(|o| o.video.as_mut()).filter(|v| v.vgen == vgen) {
                        v.preview = Some((t, px));
                        v.preview_bmp = None;
                        cx.repaint();
                    }
                });
            }
        }
    });
    tx
}

/// The subtitles shown first: those in the interface's language, else the plain "<name>.srt", else the first.
fn default_subs(video: &Path, subs: &[PathBuf]) -> usize {
    let lang = if crate::i18n::french() { "fr" } else { "en" };
    let label = |s: &PathBuf| srt::label(video, s).to_lowercase();
    subs.iter().position(|s| label(s).split(['-', '_']).next() == Some(lang)).or_else(|| subs.iter().position(|s| label(s).is_empty())).unwrap_or(0)
}

pub fn show_in_explorer(path: &Path) {
    let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", path.display())).spawn();
}

/// Splits a command line the way Windows does for simple quoted paths.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quoted = false;
    for ch in s.chars() {
        match ch {
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.frames += 1;
        self.paint_all(c, cx);
        if self.animating() {
            cx.animate = true;
        }
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        self.handle(ev, cx);
        if self.animating() || self.toast.is_some() {
            cx.repaint();
        }
    }

    fn area(&self, p: Pt) -> Area {
        if self.full.is_some() {
            // no edge to resize in full screen
            return if p.y < 6.0 { Area::Control } else { Area::Client };
        }
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(Hit::Min | Hit::Close) => Area::Control,
            Some(Hit::Stage) => Area::Client,
            Some(_) => Area::Client,
            None if p.y < TOP_H - 6.0 => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Min | Hit::Max | Hit::Close | Hit::Stage | Hit::Panel | Hit::Controls) | None => Cursor::Arrow,
            Some(Hit::Scrollbar) => Cursor::Arrow,
            Some(_) => Cursor::Hand,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args() {
        assert_eq!(split_args(r#""C:\a b\c.jpg" x"#), vec![r"C:\a b\c.jpg".to_string(), "x".to_string()]);
    }

    #[test]
    fn pan_stays_inside() {
        let mut ph = Photo::new(0);
        ph.dec = Some(Arc::new(Decoded { frames: vec![Px { w: 4, h: 4, data: vec![0; 64] }], delays: vec![], full_w: 2000, full_h: 1000, full: false }));
        ph.zoom = 1.0;
        ph.ox = 5000.0;
        ph.oy = -5000.0;
        clamp_pan(&mut ph, Rect { x: 0.0, y: 0.0, w: 1000.0, h: 800.0 }, 2000.0, 1000.0, 1.0);
        assert_eq!(ph.ox, 500.0);
        assert_eq!(ph.oy, -100.0);
        ph.turns = 1;
        assert_eq!(ph.dims(), Some((1000.0, 2000.0)));
    }
}
