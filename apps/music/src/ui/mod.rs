//! Plume Music's window. Everything is drawn by hand (Direct2D): a sidebar (library and playlists), the page in the
//! middle, the player bar at the bottom, a side panel (lyrics or queue) and the full-window "Now playing" with its
//! animated background. Network, decoding and covers run on other threads and report here.

pub mod draw;
mod now;
mod paint;
mod player;
mod settings_view;
mod views;

use crate::{
    art,
    audio::{Engine, Note, Status},
    i18n::tr,
    library::{Library, Track},
    lyrics::Lyrics,
    settings::Settings,
    yt,
};
use plume_ui::{Area, Button, Canvas, Color, Cursor, Cx, Event, Gfx, Handler, Hits, PaintCx, Pt, Rect, Window, m};
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use windows::Win32::Graphics::Imaging::IWICImagingFactory;

pub const TICK: usize = 1;
pub const ANIM: usize = 2;
pub const TYPED: usize = 3;
pub const SIDEBAR_W: f32 = 248.0;
pub const SIDEBAR_SMALL: f32 = 76.0;
pub const PLAYER_H: f32 = 92.0;
pub const TOP_H: f32 = 64.0;
pub const PANEL_W: f32 = 340.0;
pub const ROW_H: f32 = 58.0;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum View {
    Home,
    Search,
    Songs,
    Albums,
    Artists,
    Liked,
    Downloads,
    Playlist(String),
    /// Album key (artist \1 album).
    Album(String),
    Artist(String),
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Lyrics,
    Queue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CardKind {
    Playlist(String),
    Album(String),
    Artist(String),
    /// A song's radio ("Made for you").
    Mix(String),
    Liked,
    Track(String),
}

#[derive(Clone, Debug)]
pub struct Card {
    pub kind: CardKind,
    pub title: String,
    pub sub: String,
    pub cover: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Back,
    Forward,
    SearchBox,
    Suggestion(usize),
    Recent(usize),
    Nav(View),
    NavPlaylist(String),
    NewPlaylist,
    ImportOpen,
    SidebarToggle,
    Row(usize),
    RowPlay(usize),
    RowLike(usize),
    RowMore(usize),
    RowDownload(usize),
    Card(usize),
    CardPlay(usize),
    PlayAll,
    ShuffleAll,
    DownloadAll,
    HeaderMore,
    // player bar
    PlayPause,
    Next,
    Prev,
    Shuffle,
    Repeat,
    Seek,
    Volume,
    Mute,
    LikeNow,
    LyricsBtn,
    QueueBtn,
    NowOpen,
    NowClose,
    NowTab(Panel),
    NowTitle,
    NowArtist,
    LyricLine(usize),
    QueueRow(usize),
    QueueRemove(usize),
    QueueClear,
    Timer,
    // menus and dialogs
    MenuItem(usize),
    ModalOk,
    ModalCancel,
    ModalField,
    Toast,
    // settings
    Choice(&'static str, usize),
    Toggle(&'static str),
    Slider(&'static str),
    EqBand(usize),
    Preset(usize),
    Action(&'static str),
    FolderRemove(usize),
    Scrollbar,
}

/// What a drag holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Drag {
    Seek(f32),
    Volume,
    Slider(&'static str),
    EqBand(usize),
    Scrollbar(f32, f32),
}

#[derive(Clone, Debug)]
pub enum MenuAct {
    PlayNext(String),
    AddQueue(String),
    AddTo(String, String),
    NewPlaylistWith(String),
    Like(String),
    Download(String),
    GoAlbum(String),
    GoArtist(String),
    Radio(String),
    RemoveFrom(String, usize),
    ShowFile(String),
    CopyLink(String),
    Rename(String),
    DeletePlaylist(String),
    DownloadPlaylist(String),
    Sleep(u32),
    SleepEnd,
    SleepOff,
    Header,
}

pub struct Menu {
    pub x: f32,
    pub y: f32,
    pub items: Vec<(String, MenuAct)>,
}

#[derive(Clone, Debug)]
pub enum Modal {
    Import { stage: ImportStage },
    NewPlaylist { with: Option<String> },
    Rename(String),
    Tools,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImportStage {
    Link,
    Reading,
    Matching {
        done: usize,
        total: usize,
        name: String,
    },
    /// `missing`: the songs not found ("Title — Artist"); `capped`: Spotify only showed the first 100.
    Done {
        playlist: String,
        found: usize,
        missing: Vec<String>,
        capped: bool,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Dl {
    Queued,
    Running,
    Done,
    Failed(String),
}

/// Colors of Plume Music (dark like a stage, or light).
/// The colours of the window: plume-ui's theme (the same as every Plume app) with Plume Music's accent.
#[derive(Clone)]
pub struct Pal {
    pub dark: bool,
    pub bg: Color,
    pub side: Color,
    pub card: Color,
    /// Dialogs, menus, toasts (opaque).
    pub solid: Color,
    pub solid2: Color,
    pub fill: Color,
    pub fill2: Color,
    pub line: Color,
    pub ring: Color,
    pub t1: Color,
    pub t2: Color,
    pub t3: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub bar: Color,
    pub close_hover: Color,
}

impl Pal {
    pub fn new(dark: bool, accent: Color) -> Pal {
        let t = plume_ui::Theme::new(dark, Some(accent));
        Pal {
            dark,
            bg: t.bg,
            side: t.bg2,
            card: t.card,
            solid: t.solid,
            solid2: t.solid2,
            fill: t.fill,
            fill2: t.fill2,
            line: t.sep,
            ring: t.ring,
            t1: t.t1,
            t2: t.t2,
            t3: t.t3,
            accent,
            on_accent: Color::hex(0xffffff),
            bar: if dark { t.bg2 } else { t.solid },
            close_hover: t.close_hover,
        }
    }
}

pub fn accent_named(name: &str, dark: bool) -> Color {
    let hex = match (name, dark) {
        ("teal", _) => 0x14b8a6,
        ("blue", _) => 0x3b82f6,
        ("green", _) => 0x22c55e,
        ("orange", _) => 0xf97316,
        ("violet", _) => 0x8b5cf6,
        (_, true) => 0xfb3e6e,
        (_, false) => 0xe11d48,
    };
    Color::hex(hex)
}

/// A few background jobs at a time (covers, lyrics, searches), so a page full of covers does not open 200 threads.
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
        if self.busy.load(Ordering::SeqCst) < 6 {
            let (busy, waiting) = (self.busy.clone(), self.waiting.clone());
            busy.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || {
                unsafe {
                    let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                }
                loop {
                    // newest first: what was just scrolled into view
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

pub struct App {
    pub win: Window,
    /// Kept alive with the window (Direct2D and DirectWrite factories).
    #[allow(dead_code)]
    pub gfx: Rc<Gfx>,
    pub wic: IWICImagingFactory,
    pub pal: Pal,
    pub lib: Library,
    pub set: Settings,
    pub engine: Engine,
    pub st: Status,
    pub pool: Pool,
    // the player
    pub queue: Vec<String>,
    pub qpos: usize,
    pub unshuffled: Option<Vec<String>>,
    pub context: String,
    pub next_id: u64,
    pub cur: Option<(u64, String)>,
    pub prepared: Option<(u64, String)>,
    pub load_gen: u64,
    pub loading: Option<String>,
    pub sleep_at: Option<Instant>,
    pub sleep_end_of_song: bool,
    pub muted_volume: Option<f32>,
    // pages
    pub view: View,
    pub back: Vec<View>,
    pub fwd: Vec<View>,
    pub scroll: f32,
    pub scroll_to: f32,
    pub content_h: f32,
    pub view_h: f32,
    pub rows: Vec<String>,
    pub rows_for: Option<View>,
    pub cards: Vec<Card>,
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub drag: Option<Drag>,
    pub sel: Option<usize>,
    // search
    pub query: String,
    pub results: Vec<String>,
    pub local_results: Vec<String>,
    pub suggestions: Vec<String>,
    pub searching: bool,
    pub search_gen: u64,
    pub search_error: Option<String>,
    pub edit_for: Option<&'static str>,
    pub edit_shown: Option<&'static str>,
    pub search_rect: Option<Rect>,
    // overlays
    pub now_open: bool,
    pub now_tab: Panel,
    pub panel: Option<Panel>,
    pub menu: Option<Menu>,
    pub modal: Option<Modal>,
    pub modal_text: String,
    pub toast: Option<(String, Instant)>,
    // lyrics
    pub lyrics: Option<(String, Option<Lyrics>)>,
    pub lyrics_loading: Option<String>,
    pub lyric_y: f32,
    // pictures
    pub covers_pending: HashSet<String>,
    pub palettes: HashMap<u64, Vec<Color>>,
    pub drawn: Vec<u64>,
    pub motion: Option<(String, art::Motion)>,
    pub motion_loading: Option<String>,
    pub motion_last: Option<u64>,
    pub gif: Option<art::Gif>,
    pub gif_due: Instant,
    pub gif_frame: u64,
    pub gif_failed: bool,
    pub blur: Option<(u64, windows::Win32::Graphics::Direct2D::ID2D1Bitmap, usize)>,
    pub t0: Instant,
    pub downloads: HashMap<String, Dl>,
    pub dl_running: usize,
    pub tools_busy: bool,
    pub scanning: bool,
    pub resolving: HashSet<String>,
    pub smtc: Option<crate::smtc::Smtc>,
    pub log: Vec<String>,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, dark_system: bool) -> App {
        let lib = Library::load();
        let set = Settings::load();
        crate::i18n::set(&set.lang);
        yt::CACHE_MB.store(set.cache_mb, Ordering::Relaxed);
        let dark = match set.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => dark_system,
        };
        let sender = win.sender();
        let engine = Engine::start(move |n: Note| {
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.on_note(n, cx);
                }
            });
        });
        engine.send(crate::audio::Cmd::Volume(set.volume));
        engine.send(crate::audio::Cmd::Eq(set.eq_cmd()));
        engine.send(crate::audio::Cmd::Crossfade(set.crossfade));
        let wic = art::wic().expect("WIC");
        let mut app = App {
            win,
            gfx,
            wic,
            pal: Pal::new(dark, accent_named(&set.accent, dark)),
            lib,
            set,
            engine,
            st: Status::default(),
            pool: Pool::new(),
            queue: Vec::new(),
            qpos: 0,
            unshuffled: None,
            context: String::new(),
            next_id: 1,
            cur: None,
            prepared: None,
            load_gen: 0,
            loading: None,
            sleep_at: None,
            sleep_end_of_song: false,
            muted_volume: None,
            view: View::Home,
            back: Vec::new(),
            fwd: Vec::new(),
            scroll: 0.0,
            scroll_to: 0.0,
            content_h: 0.0,
            view_h: 0.0,
            rows: Vec::new(),
            rows_for: None,
            cards: Vec::new(),
            hits: Hits::default(),
            hover: None,
            pressed: None,
            drag: None,
            sel: None,
            query: String::new(),
            results: Vec::new(),
            local_results: Vec::new(),
            suggestions: Vec::new(),
            searching: false,
            search_gen: 0,
            search_error: None,
            edit_for: None,
            edit_shown: None,
            search_rect: None,
            now_open: false,
            now_tab: Panel::Lyrics,
            panel: None,
            menu: None,
            modal: None,
            modal_text: String::new(),
            toast: None,
            lyrics: None,
            lyrics_loading: None,
            lyric_y: 0.0,
            covers_pending: HashSet::new(),
            palettes: HashMap::new(),
            drawn: Vec::new(),
            motion: None,
            motion_loading: None,
            motion_last: None,
            gif: None,
            gif_due: Instant::now(),
            gif_frame: 0,
            gif_failed: false,
            blur: None,
            t0: Instant::now(),
            downloads: HashMap::new(),
            dl_running: 0,
            tools_busy: false,
            scanning: false,
            resolving: HashSet::new(),
            smtc: None,
            log: Vec::new(),
        };
        if app.set.show_lyrics_panel {
            app.panel = Some(Panel::Lyrics);
        }
        app.restore_queue();
        app.rescan();
        yt::update_tools_weekly();
        app.win.timer(TICK, Some(250));
        if app.set.notify {
            app.smtc = crate::smtc::Smtc::new(&app.win);
        }
        app
    }

    pub fn note(&mut self, s: String) {
        if self.log.len() > 300 {
            self.log.remove(0);
        }
        self.log.push(s);
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        self.toast = Some((text.into(), Instant::now()));
        cx.repaint();
    }

    pub fn current(&self) -> Option<&Track> {
        self.cur.as_ref().and_then(|(_, k)| self.lib.tracks.get(k)).or_else(|| self.loading.as_ref().and_then(|k| self.lib.tracks.get(k)))
    }

    /// The accent: the cover's color when chosen, else the fixed one.
    pub fn refresh_accent(&mut self) {
        let mut a = accent_named(&self.set.accent, self.pal.dark);
        if self.set.accent == "cover"
            && let Some(p) = self.current().and_then(art::source).and_then(|s| self.palette_of(&s)).and_then(|p| p.first().copied())
        {
            a = art::vivid(p);
            if !self.pal.dark {
                a = a.mix(Color::hex(0), 0.25);
            }
        }
        self.pal.accent = a;
    }

    pub fn set_theme(&mut self) {
        let dark = match self.set.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => crate::system_dark(),
        };
        self.pal = Pal::new(dark, self.pal.accent);
        self.win.set_dark(dark);
        self.refresh_accent();
    }

    /// The colors of a cover (computed once it is in the cache).
    pub fn palette_of(&self, src: &str) -> Option<Vec<Color>> {
        self.palettes.get(&art::key(src)).cloned()
    }

    pub fn ensure_palette(&mut self, src: &str) {
        let k = art::key(src);
        if self.palettes.contains_key(&k) {
            return;
        }
        if let Some(p) = art::cached(src) {
            let pal = art::palette(&self.wic, &p);
            self.palettes.insert(k, pal);
        } else {
            self.request_cover(src);
        }
    }

    /// Asks for a cover (downloaded or taken from the file); the window is repainted when it arrives.
    pub fn request_cover(&mut self, src: &str) {
        if art::cached(src).is_some() || self.covers_pending.contains(src) {
            return;
        }
        self.covers_pending.insert(src.to_string());
        let s = src.to_string();
        let sender = self.win.sender();
        self.pool.run(move || {
            let ok = art::fetch(&s);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.covers_pending.remove(&s);
                    if ok {
                        let cur = app.current().and_then(art::source);
                        if cur.as_deref() == Some(s.as_str()) {
                            app.ensure_palette(&s);
                            app.refresh_accent();
                        }
                    }
                    cx.repaint();
                }
            });
        });
    }

    // ───────────────────────── pages ─────────────────────────

    pub fn go(&mut self, v: View, cx: &mut Cx) {
        if v == self.view {
            self.scroll_to = 0.0;
            cx.repaint();
            return;
        }
        self.back.push(std::mem::replace(&mut self.view, v));
        self.fwd.clear();
        self.enter(cx);
    }

    fn enter(&mut self, cx: &mut Cx) {
        self.scroll = 0.0;
        self.scroll_to = 0.0;
        self.sel = None;
        self.rows_for = None;
        self.menu = None;
        self.now_open = false;
        if self.view == View::Search {
            self.focus_search();
        }
        cx.repaint();
    }

    pub fn nav_back(&mut self, cx: &mut Cx) {
        if let Some(v) = self.back.pop() {
            self.fwd.push(std::mem::replace(&mut self.view, v));
            self.enter(cx);
        }
    }

    pub fn nav_forward(&mut self, cx: &mut Cx) {
        if let Some(v) = self.fwd.pop() {
            self.back.push(std::mem::replace(&mut self.view, v));
            self.enter(cx);
        }
    }

    /// The songs of the current page (kept until the library changes).
    pub fn refresh_rows(&mut self) {
        if self.rows_for.as_ref() == Some(&self.view) && !self.lib.dirty {
            return;
        }
        self.rows = match &self.view {
            View::Songs => self.lib.songs().into_iter().map(|t| t.key.clone()).collect(),
            View::Liked => self.lib.liked(),
            View::Downloads => {
                let mut v: Vec<String> = self.downloads.iter().filter(|(_, d)| !matches!(d, Dl::Done)).map(|(k, _)| k.clone()).collect();
                v.sort();
                v.extend(self.lib.downloaded());
                v.dedup();
                v
            }
            View::Playlist(id) => self.lib.playlist(id).map(|p| p.tracks.clone()).unwrap_or_default(),
            View::Album(k) => self
                .lib
                .albums()
                .into_iter()
                .find(|(ar, al, _)| format!("{}\u{1}{}", ar.split([',', '&']).next().unwrap_or("").trim().to_lowercase(), al.to_lowercase()) == *k)
                .map(|a| a.2)
                .unwrap_or_default(),
            View::Artist(name) => {
                let n = name.to_lowercase();
                let mut v: Vec<&Track> = self.lib.tracks.values().filter(|t| t.artist.to_lowercase().contains(&n)).collect();
                v.sort_by(|a, b| b.plays.cmp(&a.plays).then(a.title.cmp(&b.title)));
                v.into_iter().map(|t| t.key.clone()).collect()
            }
            _ => Vec::new(),
        };
        self.rows_for = Some(self.view.clone());
    }

    // ───────────────────────── search ─────────────────────────

    pub fn focus_search(&mut self) {
        self.edit_for = Some("search");
    }

    pub fn search_typed(&mut self, text: String, cx: &mut Cx) {
        self.query = text;
        if self.view != View::Search {
            self.go(View::Search, cx);
        }
        self.local_results = self.lib.search(&self.query);
        cx.timer(TYPED, Some(350));
        cx.repaint();
    }

    pub fn run_search(&mut self, cx: &mut Cx) {
        let q = self.query.trim().to_string();
        if q.is_empty() {
            self.results.clear();
            self.suggestions.clear();
            cx.repaint();
            return;
        }
        // a link: import it
        if crate::import::service(&q).is_some() {
            self.open_import(Some(q), cx);
            return;
        }
        self.search_gen += 1;
        let g = self.search_gen;
        self.searching = true;
        self.search_error = None;
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let res = yt::search(&q);
            let sug = yt::suggest(&q);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h)
                    && app.search_gen == g
                {
                    app.searching = false;
                    match res {
                        Ok(list) => {
                            app.results = list
                                .into_iter()
                                .filter(|t| app.set.explicit || !t.explicit)
                                .map(|t| {
                                    let k = t.key.clone();
                                    if !app.lib.tracks.contains_key(&k) {
                                        // known for display, not saved until played, liked or added
                                        app.lib.tracks.insert(k.clone(), Track { added: 0, ..t });
                                    }
                                    k
                                })
                                .collect();
                        }
                        Err(e) => app.search_error = Some(e),
                    }
                    app.suggestions = sug;
                    cx.repaint();
                }
            });
        });
        cx.repaint();
    }

    pub fn remember_search(&mut self) {
        let q = self.query.trim().to_string();
        if q.len() > 1 {
            self.lib.searches.retain(|s| !s.eq_ignore_ascii_case(&q));
            self.lib.searches.insert(0, q);
            self.lib.searches.truncate(12);
            self.lib.dirty = true;
        }
    }

    // ───────────────────────── import ─────────────────────────

    pub fn open_import(&mut self, link: Option<String>, cx: &mut Cx) {
        self.modal = Some(Modal::Import { stage: ImportStage::Link });
        self.modal_text = link.clone().unwrap_or_default();
        self.edit_for = Some("modal");
        if link.is_some() {
            self.start_import(cx);
        }
        cx.repaint();
    }

    /// The songs copied in Spotify (Ctrl+A, Ctrl+C in a playlist), taken from the clipboard.
    pub fn import_clipboard(&mut self, cx: &mut Cx) {
        let text = clipboard_text(self.win.hwnd).unwrap_or_default();
        if crate::import::spotify_track_ids(&text).is_empty() {
            self.modal = Some(Modal::Import {
                stage: ImportStage::Failed(tr("No Spotify songs were copied. In Spotify, open the playlist, press Ctrl+A then Ctrl+C, and try again.").into()),
            });
            cx.repaint();
            return;
        }
        self.modal_text = text;
        self.start_import(cx);
    }

    pub fn start_import(&mut self, cx: &mut Cx) {
        let link = self.modal_text.trim().to_string();
        // song links copied from Spotify (more than one): every song, read one by one
        let copied = crate::import::spotify_track_ids(&link);
        let copied = if copied.len() > 1 || (copied.len() == 1 && !link.contains("/playlist/")) { copied } else { Vec::new() };
        if copied.is_empty() && crate::import::service(&link).is_none() {
            self.modal = Some(Modal::Import {
                stage: ImportStage::Failed(
                    tr("Paste a link to a playlist, an album or a song from Spotify, Apple Music, Deezer, SoundCloud or YouTube.").into(),
                ),
            });
            cx.repaint();
            return;
        }
        let needs_tool = matches!(crate::import::service(&link), Some(crate::import::Service::SoundCloud | crate::import::Service::YouTube));
        if needs_tool && !yt::have_ytdlp() {
            self.modal = Some(Modal::Tools);
            cx.repaint();
            return;
        }
        self.modal = Some(Modal::Import { stage: ImportStage::Reading });
        self.edit_for = None;
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let found = if copied.is_empty() {
                crate::import::read(&link)
            } else {
                // the songs are read while they are matched (below)
                let n = copied.len();
                Ok(crate::import::Found {
                    name: if n == 1 { tr("Songs from Spotify").to_string() } else { format!("{} ({n})", tr("Songs from Spotify")) },
                    cover: None,
                    tracks: copied.iter().map(|id| Track { key: format!("sp:{id}"), ..Default::default() }).collect(),
                    service: crate::import::Service::Spotify,
                })
            };
            let found = match found {
                Ok(f) => f,
                Err(e) => {
                    sender.send(move |h, cx| {
                        if let Some(app) = with_app(h) {
                            app.modal = Some(Modal::Import { stage: ImportStage::Failed(e) });
                            cx.repaint();
                        }
                    });
                    return;
                }
            };
            let total = found.tracks.len();
            let capped = found.service == crate::import::Service::Spotify && total == 100 && found.tracks.iter().all(|t| !t.key.starts_with("sp:"));
            let name = found.name.clone();
            let n2 = name.clone();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.modal = Some(Modal::Import { stage: ImportStage::Matching { done: 0, total, name: n2 } });
                    cx.repaint();
                }
            });
            // Spotify and Apple songs: their YouTube Music twin, 4 at a time, in order
            let needs_match = matches!(found.service, crate::import::Service::Spotify | crate::import::Service::Apple | crate::import::Service::Deezer);
            let results: Arc<Mutex<Vec<Option<Track>>>> = Arc::new(Mutex::new(vec![None; total]));
            let labels: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![String::new(); total]));
            let next = Arc::new(AtomicUsize::new(0));
            let done = Arc::new(AtomicUsize::new(0));
            let tracks = Arc::new(found.tracks);
            let workers: Vec<_> = (0..4)
                .map(|_| {
                    let (results, next, done, tracks, name, labels) =
                        (results.clone(), next.clone(), done.clone(), tracks.clone(), name.clone(), labels.clone());
                    std::thread::spawn(move || {
                        loop {
                            let i = next.fetch_add(1, Ordering::SeqCst);
                            if i >= tracks.len() {
                                break;
                            }
                            // a copied Spotify song: its page first
                            let read;
                            let want = match tracks[i].key.strip_prefix("sp:") {
                                Some(id) => {
                                    read = crate::import::spotify_track(id)
                                        .unwrap_or_else(|| Track { title: format!("spotify:track:{id}"), ..Default::default() });
                                    &read
                                }
                                None => &tracks[i],
                            };
                            labels.lock().unwrap()[i] = if want.artist.is_empty() { want.title.clone() } else { format!("{} — {}", want.title, want.artist) };
                            let got = if want.title.starts_with("spotify:track:") {
                                None
                            } else if needs_match {
                                yt::find_match(want).map(|mut m| {
                                    // the service's names, YouTube's sound and cover
                                    if !want.album.is_empty() {
                                        m.album = want.album.clone();
                                    }
                                    if m.cover.is_none() {
                                        m.cover = want.cover.clone();
                                    }
                                    m.explicit |= want.explicit;
                                    m
                                })
                            } else {
                                Some(want.clone())
                            };
                            results.lock().unwrap()[i] = got;
                            let d = done.fetch_add(1, Ordering::SeqCst) + 1;
                            let name = name.clone();
                            sender.send(move |h, cx| {
                                if let Some(app) = with_app(h)
                                    && let Some(Modal::Import { stage: ImportStage::Matching { done, .. } }) = &mut app.modal
                                {
                                    *done = d.max(*done);
                                    let _ = name;
                                    cx.repaint();
                                }
                            });
                        }
                    })
                })
                .collect();
            for w in workers {
                let _ = w.join();
            }
            let list = std::mem::take(&mut *results.lock().unwrap());
            let wanted: Vec<String> = std::mem::take(&mut *labels.lock().unwrap());
            let cover = found.cover.clone();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    let id = app.lib.new_playlist(&name);
                    let mut keys = Vec::new();
                    let mut missing = Vec::new();
                    for (i, t) in list.into_iter().enumerate() {
                        match t {
                            Some(t) => {
                                let k = app.lib.upsert(t);
                                if !keys.contains(&k) {
                                    keys.push(k);
                                }
                            }
                            None => missing.push(wanted.get(i).cloned().unwrap_or_default()),
                        }
                    }
                    if let Some(p) = app.lib.playlist_mut(&id) {
                        p.tracks = keys.clone();
                        p.source = Some(link.clone());
                        p.cover = cover;
                    }
                    app.lib.save();
                    app.note(format!("imported {} {} missing {:?}", name, keys.len(), missing));
                    app.modal = Some(Modal::Import { stage: ImportStage::Done { playlist: id, found: keys.len(), missing, capped } });
                    cx.repaint();
                }
            });
        });
        cx.repaint();
    }

    pub fn install_tools(&mut self, ffmpeg: bool, cx: &mut Cx) {
        if self.tools_busy {
            return;
        }
        self.tools_busy = true;
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let r = yt::install(ffmpeg);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.tools_busy = false;
                    match r {
                        Ok(()) => {
                            if matches!(app.modal, Some(Modal::Tools)) {
                                app.modal = None;
                            }
                            app.say(tr("Installed"), cx);
                        }
                        Err(e) => app.say(e, cx),
                    }
                    cx.repaint();
                }
            });
        });
        cx.repaint();
    }

    // ───────────────────────── library ─────────────────────────

    pub fn rescan(&mut self) {
        if self.scanning || self.lib.folders.is_empty() {
            return;
        }
        self.scanning = true;
        let folders = self.lib.folders.clone();
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let mut found = Vec::new();
            crate::library::scan(&folders, |t| found.push(t));
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.scanning = false;
                    let n = found.len();
                    for t in found {
                        // a scanned file of a song known by its source: the file joins that song
                        app.lib.upsert(t);
                    }
                    app.lib.forget_missing();
                    app.lib.save();
                    app.rows_for = None;
                    app.note(format!("scanned {n}"));
                    cx.repaint();
                }
            });
        });
    }

    pub fn toggle_like(&mut self, key: &str, cx: &mut Cx) {
        if let Some(t) = self.lib.tracks.get_mut(key) {
            t.liked = !t.liked;
            if t.added == 0 {
                t.added = crate::library::now();
            }
            self.lib.dirty = true;
            self.rows_for = None;
        }
        cx.repaint();
    }

    pub fn add_to_playlist(&mut self, key: &str, id: &str, cx: &mut Cx) {
        if let Some(t) = self.lib.tracks.get_mut(key)
            && t.added == 0
        {
            t.added = crate::library::now();
        }
        let name = match self.lib.playlist_mut(id) {
            Some(p) => {
                if !p.tracks.iter().any(|k| k == key) {
                    p.tracks.push(key.to_string());
                }
                p.name.clone()
            }
            None => return,
        };
        self.rows_for = None;
        self.say(crate::i18n::fill(tr("Added to {s}"), &[&name]), cx);
    }

    /// Saves a song to the download folder (in the background, two at a time).
    pub fn download(&mut self, key: &str, cx: &mut Cx) {
        let Some(t) = self.lib.tracks.get(key).cloned() else { return };
        if t.local() || matches!(self.downloads.get(key), Some(Dl::Queued | Dl::Running)) {
            return;
        }
        if !yt::have_ytdlp() {
            self.modal = Some(Modal::Tools);
            cx.repaint();
            return;
        }
        if let Some(tr_) = self.lib.tracks.get_mut(key)
            && tr_.added == 0
        {
            tr_.added = crate::library::now();
        }
        self.downloads.insert(key.to_string(), Dl::Queued);
        self.rows_for = None;
        self.pump_downloads();
        cx.repaint();
    }

    pub fn pump_downloads(&mut self) {
        while self.dl_running < 2 {
            let Some(key) = self.downloads.iter().find(|(_, d)| **d == Dl::Queued).map(|(k, _)| k.clone()) else { return };
            let Some(t) = self.lib.tracks.get(&key).cloned() else {
                self.downloads.remove(&key);
                continue;
            };
            self.downloads.insert(key.clone(), Dl::Running);
            self.dl_running += 1;
            let folder = self.set.download_dir.clone();
            let mp3 = self.set.format == "mp3" && yt::have_ffmpeg();
            let sender = self.win.sender();
            std::thread::spawn(move || {
                let cover = art::source(&t).and_then(|s| {
                    art::fetch(&s);
                    art::cached(&s).and_then(|p| std::fs::read(p).ok())
                });
                let r = yt::download(&t, &folder, mp3, cover);
                sender.send(move |h, cx| {
                    if let Some(app) = with_app(h) {
                        app.dl_running -= 1;
                        match r {
                            Ok(path) => {
                                if let Some(tr_) = app.lib.tracks.get_mut(&key) {
                                    tr_.path = Some(path);
                                }
                                app.lib.dirty = true;
                                app.downloads.insert(key.clone(), Dl::Done);
                                app.note(format!("downloaded {key}"));
                            }
                            Err(e) => {
                                app.note(format!("download failed {key}: {e}"));
                                app.downloads.insert(key.clone(), Dl::Failed(e));
                            }
                        }
                        app.rows_for = None;
                        app.pump_downloads();
                        cx.repaint();
                    }
                });
            });
        }
    }

    pub fn settings_changed(&mut self, cx: &mut Cx) {
        self.set.save();
        crate::i18n::set(&self.set.lang);
        yt::CACHE_MB.store(self.set.cache_mb, Ordering::Relaxed);
        self.engine.send(crate::audio::Cmd::Eq(self.set.eq_cmd()));
        self.engine.send(crate::audio::Cmd::Crossfade(self.set.crossfade));
        self.set_theme();
        cx.repaint();
    }

    /// Bitmaps not drawn for a while are dropped (a long list of covers would fill the graphics memory).
    pub fn forget_old_bitmaps(&mut self, c: &mut Canvas) {
        const KEEP: usize = 260;
        if self.drawn.len() > KEEP * 2 {
            let mut seen = HashSet::new();
            let mut keep = Vec::new();
            for k in self.drawn.iter().rev() {
                if seen.insert(*k) {
                    keep.push(*k);
                }
            }
            for k in keep.iter().skip(KEEP) {
                c.forget_image(*k);
            }
            keep.truncate(KEEP);
            keep.reverse();
            self.drawn = keep;
        }
    }

    pub fn animating(&self) -> bool {
        let reduce = self.set.reduce_motion;
        (self.now_open && !reduce)
            || self.drag.is_some()
            || (self.scroll - self.scroll_to).abs() > 0.5
            || (self.set.gif_everywhere && self.gif.is_some() && !reduce)
            || (self.motion.is_some() && !reduce && (self.now_open || self.panel.is_some()))
            || (self.panel == Some(Panel::Lyrics) && self.st.playing)
            || self.toast.is_some()
    }

    pub fn kick(&mut self, cx: &mut Cx) {
        if self.animating() {
            cx.timer(ANIM, Some(16));
        }
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_all(c, cx);
        self.forget_old_bitmaps(c);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        self.handle(ev, cx);
        self.kick(cx);
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(Hit::Min | Hit::Close) => Area::Control,
            Some(_) => Area::Client,
            None if p.y < 40.0 && !self.now_open && self.modal.is_none() => Area::Caption,
            None if self.now_open && p.y < 40.0 => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Min | Hit::Max | Hit::Close) | None => Cursor::Arrow,
            Some(Hit::SearchBox | Hit::ModalField) => Cursor::Arrow,
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
            Event::MouseMove { p, .. } => {
                if let Some(d) = self.drag.clone() {
                    self.drag_to(d, p, cx);
                    return;
                }
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
                // a menu closes on any click outside it
                if self.menu.is_some() && !matches!(h, Some(Hit::MenuItem(_))) {
                    self.menu = None;
                    cx.repaint();
                    return;
                }
                self.pressed = h.clone();
                if let Some(h) = &h
                    && let Some(d) = self.drag_start(h, p)
                {
                    self.drag = Some(d.clone());
                    cx.capture(true);
                    self.drag_to(d, p, cx);
                    return;
                }
                if clicks == 2
                    && let Some(Hit::Row(i)) = h
                {
                    self.play_rows(i, cx);
                    self.pressed = None;
                }
                cx.capture(true);
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                if let Some(d) = self.drag.take() {
                    self.drag_end(d, p, cx);
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
            Event::MouseDown { p, button: Button::Right, .. } => {
                let h = self.hits.at(p).cloned();
                match h {
                    Some(Hit::Row(i) | Hit::RowMore(i)) => self.row_menu(i, p, cx),
                    Some(Hit::NavPlaylist(id)) => self.playlist_menu(&id, p, cx),
                    Some(Hit::QueueRow(i)) => {
                        if let Some(k) = self.queue.get(i).cloned() {
                            self.track_menu(&k, None, p, cx);
                        }
                    }
                    _ => {}
                }
            }
            Event::MouseDown { button: Button::Back, .. } => self.nav_back(cx),
            Event::MouseDown { button: Button::Forward, .. } => self.nav_forward(cx),
            Event::Wheel { p, dy, mods, .. } => self.wheel(p, dy, mods.ctrl, cx),
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::EditChanged(text) => match self.edit_for {
                Some("search") => self.search_typed(text, cx),
                Some("modal") => {
                    self.modal_text = text;
                    cx.repaint();
                }
                _ => {}
            },
            Event::EditKey { vk: 0x0D, .. } => match self.edit_for {
                Some("search") => {
                    cx.timer(TYPED, None);
                    self.remember_search();
                    self.run_search(cx);
                }
                Some("modal") => self.act(Hit::ModalOk, Pt { x: 0.0, y: 0.0 }, cx),
                _ => {}
            },
            Event::EditKey { vk: 0x1B, .. } => {
                if self.modal.is_some() {
                    self.close_modal(cx);
                } else {
                    cx.after(|w| w.edit_hide());
                    self.edit_for = None;
                }
            }
            Event::Timer(TICK) => self.tick(cx),
            Event::Timer(ANIM) => {
                if !self.animating() {
                    cx.timer(ANIM, None);
                }
                cx.repaint();
            }
            Event::Timer(TYPED) => {
                cx.timer(TYPED, None);
                self.run_search(cx);
            }
            Event::Opened(text) => {
                self.win.bring_to_front();
                self.open_args(&text, cx);
            }
            Event::Close => {
                self.save_queue();
                self.lib.save();
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

    /// Files given on the command line (or dropped on the icon): played.
    pub fn open_args(&mut self, text: &str, cx: &mut Cx) {
        let mut keys = Vec::new();
        for part in split_args(text) {
            let p = std::path::PathBuf::from(&part);
            if p.is_file()
                && let Some(t) = crate::library::read_file(&p)
            {
                keys.push(self.lib.upsert(t));
            }
        }
        if !keys.is_empty() {
            self.play_list(keys, 0, String::new(), cx);
        }
    }

    fn tick(&mut self, cx: &mut Cx) {
        let st = self.engine.status();
        let changed = (st.pos - self.st.pos).abs() > 0.2 || st.playing != self.st.playing || st.buffering != self.st.buffering || st.dur != self.st.dur;
        self.st = st;
        if changed {
            cx.repaint();
            self.update_smtc();
        }
        // sleep timer
        if let Some(at) = self.sleep_at
            && Instant::now() >= at
        {
            self.sleep_at = None;
            self.engine.send(crate::audio::Cmd::Pause);
        }
        if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(4)) {
            self.toast = None;
            cx.repaint();
        }
        if self.lib.dirty {
            static LAST: Mutex<Option<Instant>> = Mutex::new(None);
            let mut l = LAST.lock().unwrap();
            if l.is_none_or(|t| t.elapsed() > Duration::from_secs(5)) {
                *l = Some(Instant::now());
                self.rows_for = None;
                self.lib.save();
            }
        }
        self.kick(cx);
    }

    pub fn close_modal(&mut self, cx: &mut Cx) {
        self.modal = None;
        if self.edit_for == Some("modal") {
            self.edit_for = None;
            cx.after(|w| w.edit_hide());
        }
        cx.repaint();
    }

    fn key(&mut self, vk: u16, mods: plume_ui::Mods, cx: &mut Cx) {
        match (vk, mods.ctrl, mods.shift) {
            (0x20, false, _) => self.toggle_play(cx),
            (0x27, true, _) | (0xB0, _, _) => self.skip(1, cx),
            (0x25, true, _) | (0xB1, _, _) => self.skip(-1, cx),
            (0xB3, _, _) => self.toggle_play(cx),
            (0x27, false, true) => self.seek_by(10.0),
            (0x25, false, true) => self.seek_by(-10.0),
            (0x26, true, _) => self.change_volume(0.05),
            (0x28, true, _) => self.change_volume(-0.05),
            (0x4C, true, _) => self.toggle_panel(Panel::Lyrics, cx), // Ctrl+L
            (0x46, true, _) | (0x4B, true, _) => {
                // Ctrl+F, Ctrl+K: search
                self.go(View::Search, cx);
                self.focus_search();
            }
            (0x53, true, _) => {
                // Ctrl+S: shuffle
                self.toggle_shuffle(cx);
            }
            (0x52, true, _) => self.cycle_repeat(cx),
            (0x49, true, _) => self.open_import(None, cx), // Ctrl+I
            (0x4E, true, _) => {
                self.modal = Some(Modal::NewPlaylist { with: None });
                self.modal_text = crate::i18n::fill(tr("My playlist #{n}"), &[&(self.lib.playlists.len() + 1).to_string()]);
                self.edit_for = Some("modal");
            }
            (0x7A, _, _) => {
                self.now_open = !self.now_open; // F11
                if self.now_open {
                    cx.after(|w| w.edit_hide());
                }
            }
            (0x1B, _, _) => {
                if self.menu.is_some() {
                    self.menu = None;
                } else if self.modal.is_some() {
                    self.close_modal(cx);
                } else if self.now_open {
                    self.now_open = false;
                }
            }
            (0x25, false, false) if mods.alt => self.nav_back(cx),
            (0x27, false, false) if mods.alt => self.nav_forward(cx),
            (0x4C, false, false) if !mods.alt => {
                if let Some((_, k)) = self.cur.clone() {
                    self.toggle_like(&k, cx);
                }
            }
            _ => {}
        }
        cx.repaint();
    }

    fn wheel(&mut self, p: Pt, dy: f32, ctrl: bool, cx: &mut Cx) {
        if ctrl {
            return;
        }
        // over the volume: the volume
        if matches!(self.hits.at(p), Some(Hit::Volume | Hit::Mute)) {
            self.change_volume(dy * 0.05);
            cx.repaint();
            return;
        }
        if self.now_open || self.modal.is_some() {
            return;
        }
        let max = (self.content_h - self.view_h).max(0.0);
        self.scroll_to = (self.scroll_to - dy * 120.0).clamp(0.0, max);
        if self.set.reduce_motion {
            self.scroll = self.scroll_to;
        }
        cx.repaint();
    }
}

/// The text in the clipboard.
pub fn clipboard_text(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::{
        Foundation::{HANDLE, HGLOBAL},
        System::{
            DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
            Memory::{GlobalLock, GlobalUnlock},
        },
    };
    const CF_UNICODETEXT: u32 = 13;
    unsafe {
        OpenClipboard(Some(hwnd)).ok()?;
        let text = (|| {
            let h: HANDLE = GetClipboardData(CF_UNICODETEXT).ok()?;
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u16;
            if p.is_null() {
                return None;
            }
            let mut n = 0;
            while *p.add(n) != 0 && n < 4_000_000 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
            let _ = GlobalUnlock(g);
            Some(s)
        })();
        let _ = CloseClipboard();
        text
    }
}

/// Splits a command line into its arguments (quotes kept together).
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in s.chars() {
        match ch {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
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

/// Easing for the animations.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[allow(dead_code)]
pub fn rect_contains(r: Rect, p: Pt) -> bool {
    r.contains(p)
}

#[allow(dead_code)]
const _TITLE: f32 = m::TITLE_H;
