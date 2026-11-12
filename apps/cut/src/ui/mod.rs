//! Plume Cut's window, drawn by hand (Direct2D): the start screen (new project, recent projects) and the editor —
//! media bin on the left, the preview in the middle, the selected clip's properties on the right, the timeline
//! below. Probing, thumbnails, frames and the export run on worker threads and report here.

pub mod draw;
mod panels;
mod props;
mod timeline;

use crate::{
    export::{self, Spec},
    ffmpeg::{self, Ctl, Frame},
    i18n::tr,
    model::{self, History, Id, MediaKind, Project, TextStyle, TrackKind},
    preview::{Cache, Grabber, Key, Player},
    project,
    render::Codec,
    settings::Settings,
    tools,
};
use plume_ui::{Area, Button, Canvas, Color, Cursor, Cx, Event, Gfx, Handler, Hits, Mods, PaintCx, Pt, Rect, Window};
use std::{
    any::Any,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use windows::Win32::Graphics::Imaging::IWICImagingFactory;

pub const TICK: usize = 1;
pub const TOP_H: f32 = 52.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Start,
    Edit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialog {
    Export,
    Help,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinTab {
    Media,
    Text,
    Transitions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Picture,
    Sound,
    Speed,
    Colour,
    Text,
    Style,
    Motion,
}

/// A value of a clip (or of the project) set with a slider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prop {
    Scale,
    X,
    Y,
    Rotation,
    Opacity,
    CropL,
    CropT,
    CropR,
    CropB,
    Volume,
    FadeIn,
    FadeOut,
    Speed,
    Brightness,
    Contrast,
    Saturation,
    TextSize,
    BgOpacity,
    AnimLen,
    TransLen,
}

/// Handles of the picture selected in the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Body,
    Corner(u8),
    Rotate,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Home,
    Help,
    Gear,
    Export,
    Undo,
    Redo,
    // start screen
    NewProject,
    OpenProject,
    Recent(usize),
    // bin
    Import,
    Media(Id),
    MediaAdd(Id),
    MediaMusic(Id),
    TextPreset(usize),
    TransPreset(usize),
    InstallTools,
    // preview
    PreviewArea,
    Handle(Handle),
    Play,
    StepBack,
    StepFwd,
    Aspect,
    // properties
    Slider(Prop),
    Chip(&'static str, usize),
    Toggle(&'static str),
    Swatch(&'static str, usize),
    Field(&'static str),
    Action(&'static str),
    // timeline
    Tool(&'static str),
    ZoomBar,
    Ruler,
    TrackArea,
    TrackMute(usize),
    TrackHide(usize),
    Clip(Id),
    TrimL(Id),
    TrimR(Id),
    TransMark(Id),
    // dialogs
    DialogBg,
    DialogCard,
    Toast,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tools {
    Ready,
    Missing,
    /// Download progress, 0..1.
    Installing(f32),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExportState {
    Idle,
    Running { frac: f64, note: String, started: Instant },
    Done { path: PathBuf, size: u64 },
    Failed(String),
}

/// What the mouse is doing after a press.
pub enum Drag {
    None,
    Scrub,
    Move { ids: Vec<Id>, grab: i64, grab_track: usize, before: Box<Project>, moved: bool, start: Pt },
    TrimL { id: Id, before: Box<Project> },
    TrimR { id: Id, before: Box<Project> },
    FromBin { media: Id, start: Pt, on: bool },
    Slider { prop: Prop, track: Rect, before: Box<Project> },
    Handle { handle: Handle, id: Id, start: Pt, orig: Box<model::Clip>, before: Box<Project> },
    ZoomBar { track: Rect },
}

/// Plume Cut's colours: the suite's (plume_ui::Theme), with the magenta of its icon.
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
    pub line: Color,
    pub ring: Color,
    pub t1: Color,
    pub t2: Color,
    pub t3: Color,
    pub t4: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub green: Color,
    pub red: Color,
    pub orange: Color,
    pub close_hover: Color,
}

impl Pal {
    pub fn new(dark: bool) -> Pal {
        let t = plume_ui::Theme::new(dark, Some(if dark { Color::hex(0xee3fa6) } else { Color::hex(0xc2187e) }));
        Pal {
            dark,
            bg: t.bg,
            card: t.card,
            solid: t.solid,
            solid2: t.solid2,
            fill: t.fill,
            fill2: t.fill2,
            hover: t.hover,
            line: t.sep,
            ring: t.ring,
            t1: t.t1,
            t2: t.t2,
            t3: t.t3,
            t4: t.t4,
            accent: t.accent,
            on_accent: Color::hex(0xffffff),
            green: t.green,
            red: t.red,
            orange: t.orange,
            close_hover: t.close_hover,
        }
    }
}

pub struct App {
    pub win: Window,
    #[allow(dead_code)]
    pub gfx: Rc<Gfx>,
    pub wic: IWICImagingFactory,
    pub pal: Pal,
    pub set: Settings,
    pub screen: Screen,
    pub dialog: Option<Dialog>,
    pub project: Project,
    pub file: Option<PathBuf>,
    pub history: History,
    pub dirty: Option<Instant>,
    pub selected: Vec<Id>,
    pub media_sel: Option<Id>,
    pub playhead: i64,
    pub tools: Tools,
    pub importing: usize,
    pub add_after_import: bool,
    pub thumbs: HashMap<Id, Arc<Frame>>,
    pub strips: HashMap<Id, Vec<Arc<Frame>>>,
    pub waves: HashMap<Id, Vec<f32>>,
    pub cache: Cache,
    pub grabber: Option<Grabber>,
    pub player: Option<Player>,
    /// J/K/L shuttle: frames a second (negative backwards); 0 still.
    pub shuttle: f64,
    pub shuttle_at: (Instant, i64),
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub mouse: Pt,
    pub drag: Drag,
    pub bin_tab: BinTab,
    pub tab: Tab,
    pub bin_scroll: f32,
    pub bin_h: f32,
    pub props_scroll: f32,
    pub props_h: f32,
    /// Timeline: pixels per frame, scroll (pixels), vertical scroll.
    pub zoom: f32,
    pub tl_scroll: f32,
    pub tl_vscroll: f32,
    pub tl_rect: Rect,
    /// Where the preview's frame is on screen.
    pub frame_rect: Rect,
    pub snap_line: Option<i64>,
    pub edit_for: Option<&'static str>,
    pub edit_shown: Option<&'static str>,
    pub toast: Option<(String, Instant)>,
    pub export: ExportState,
    pub export_ctl: Option<Arc<Ctl>>,
    pub hw_label: Option<String>,
    pub log: Vec<String>,
    pub max_px: u32,
    pub slider_rects: HashMap<Prop, Rect>,
    /// The timeline's rows on screen: track index, top, height; and the x of frame 0 before scrolling.
    pub tl_rows: Vec<(usize, f32, f32)>,
    pub tl_x0: f32,
    /// Changes with each project opened: media ids start again in every project, so what was made for the
    /// last one (thumbnails, filmstrips, frames on their way) is told apart by it.
    pub generation: u64,
    /// Name and length of the recent projects (read once).
    pub recents: HashMap<PathBuf, (String, String)>,
    pub trim_left: bool,
    /// Typing in a field since this id was set: one undo step for it all.
    pub typing: Option<Id>,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

pub fn muted() -> bool {
    std::env::var_os("PLUME_CUT_MUTE").is_some()
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
        let wic: IWICImagingFactory = unsafe {
            windows::Win32::System::Com::CoCreateInstance(
                &windows::Win32::Graphics::Imaging::CLSID_WICImagingFactory,
                None,
                windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
            )
        }
        .expect("WIC");
        let sender = win.sender();
        let grabber = Grabber::new(move |key, frame| {
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    match frame {
                        Some(f) if app.cache.pending.contains(&key) => app.cache.put(key, f),
                        Some(_) => {}
                        None => {
                            app.cache.pending.remove(&key);
                        }
                    }
                    cx.repaint();
                }
            });
        });
        let app = App {
            win,
            gfx,
            wic,
            pal: Pal::new(dark),
            screen: Screen::Start,
            dialog: None,
            project: Project::default(),
            file: None,
            history: History::default(),
            dirty: None,
            selected: Vec::new(),
            media_sel: None,
            playhead: 0,
            tools: if tools::ready() { Tools::Ready } else { Tools::Missing },
            importing: 0,
            add_after_import: false,
            thumbs: HashMap::new(),
            strips: HashMap::new(),
            waves: HashMap::new(),
            cache: Cache::default(),
            grabber: Some(grabber),
            player: None,
            shuttle: 0.0,
            shuttle_at: (Instant::now(), 0),
            hits: Hits::default(),
            hover: None,
            pressed: None,
            mouse: Pt::default(),
            drag: Drag::None,
            bin_tab: BinTab::Media,
            tab: Tab::Picture,
            bin_scroll: 0.0,
            bin_h: 0.0,
            props_scroll: 0.0,
            props_h: 0.0,
            zoom: 3.0,
            tl_scroll: 0.0,
            tl_vscroll: 0.0,
            tl_rect: Rect::default(),
            frame_rect: Rect::default(),
            snap_line: None,
            edit_for: None,
            edit_shown: None,
            toast: None,
            export: ExportState::Idle,
            export_ctl: None,
            hw_label: None,
            log: Vec::new(),
            max_px: 640,
            slider_rects: HashMap::new(),
            tl_rows: Vec::new(),
            tl_x0: 0.0,
            generation: 0,
            recents: HashMap::new(),
            trim_left: false,
            typing: None,
            set,
        };
        app.win.timer(TICK, Some(250));
        app
    }

    pub fn note(&mut self, s: impl Into<String>) {
        if self.log.len() > 300 {
            self.log.remove(0);
        }
        self.log.push(s.into());
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        self.toast = Some((text.into(), Instant::now()));
        cx.repaint();
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
        self.set.save();
        crate::i18n::set(&self.set.lang);
        self.set_theme();
        cx.repaint();
    }

    // ───────────────────────── projects ─────────────────────────

    fn reset_view(&mut self) {
        self.stop_play();
        self.history.clear();
        self.selected.clear();
        self.media_sel = None;
        self.playhead = 0;
        self.tl_scroll = 0.0;
        self.tl_vscroll = 0.0;
        self.cache.clear();
        self.thumbs.clear();
        self.strips.clear();
        self.waves.clear();
        self.generation += 1;
        self.dirty = None;
    }

    /// A new, empty project saved at once in the projects folder.
    pub fn new_project(&mut self, cx: &mut Cx) -> bool {
        self.save_now();
        let dir = project::projects_dir();
        let file = match project::new_file(&dir, tr("Project")) {
            Ok(f) => f,
            Err(e) => {
                self.say(format!("{}: {e}", tr("Cannot create the project")), cx);
                return false;
            }
        };
        self.reset_view();
        self.project = Project { name: file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), ..Project::default() };
        let _ = project::save(&self.project, &file);
        self.set.opened(&file);
        self.file = Some(file);
        self.screen = Screen::Edit;
        self.fit_zoom();
        cx.repaint();
        true
    }

    pub fn open_project(&mut self, file: &Path, cx: &mut Cx) {
        match project::load(file) {
            Ok(p) => {
                self.save_now();
                self.reset_view();
                let missing = p.media.iter().filter(|m| m.missing).count();
                self.project = p;
                self.file = Some(file.to_path_buf());
                self.set.opened(file);
                self.screen = Screen::Edit;
                self.fit_zoom();
                for m in self.project.media.clone() {
                    if !m.missing {
                        self.load_assets(&m, cx);
                    }
                }
                if missing > 0 {
                    self.say(crate::i18n::fill(tr("{} file(s) of this project cannot be found"), &[&missing.to_string()]), cx);
                }
            }
            Err(e) => self.say(format!("{}: {e}", tr("Cannot open the project")), cx),
        }
        cx.repaint();
    }

    pub fn pick_project(&mut self, cx: &mut Cx) {
        let files = plume_ui::dialogs::pick_files(self.win.hwnd, tr("Open a project"));
        if let Some(f) = files.into_iter().find(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case(project::EXT))) {
            self.open_project(&f, cx);
        }
    }

    /// Writes the project now (when it changed).
    pub fn save_now(&mut self) {
        if self.dirty.take().is_some()
            && let Some(f) = &self.file
            && project::save(&self.project, f).is_err()
        {
            self.note("autosave failed");
        }
    }

    /// A recent project's name and length (from its file, kept until the start screen shows again).
    pub fn recent_info(&mut self, f: &Path) -> (String, String) {
        if let Some(v) = self.recents.get(f) {
            return v.clone();
        }
        let v = match project::load(f) {
            Ok(p) => (p.name.clone(), model::clock(p.duration() as f64 / p.fps())),
            Err(_) => (f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), String::new()),
        };
        self.recents.insert(f.to_path_buf(), v.clone());
        v
    }

    pub fn go_home(&mut self, cx: &mut Cx) {
        self.stop_play();
        self.save_now();
        self.recents.clear();
        self.screen = Screen::Start;
        self.dialog = None;
        self.edit_for = None;
        cx.repaint();
    }

    // ───────────────────────── edits ─────────────────────────

    /// Runs an edit; when it changed something, it can be undone and the project will be saved.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut Project) -> R) -> R {
        let before = self.project.clone();
        let r = f(&mut self.project);
        if self.project != before {
            self.history.push(&before);
            self.changed();
        }
        r
    }

    /// Something in the project changed (already in the history).
    pub fn changed(&mut self) {
        self.dirty = Some(Instant::now());
        self.selected.retain(|id| self.project.find(*id).is_some());
        if self.player.is_some() {
            let at = self.playhead;
            self.stop_play();
            self.playhead = at;
        }
    }

    pub fn commit(&mut self, before: Project) {
        if self.project != before {
            self.history.push(&before);
            self.changed();
        }
    }

    pub fn undo(&mut self, cx: &mut Cx) {
        if self.history.undo(&mut self.project) {
            self.changed();
        }
        cx.repaint();
    }

    pub fn redo(&mut self, cx: &mut Cx) {
        if self.history.redo(&mut self.project) {
            self.changed();
        }
        cx.repaint();
    }

    pub fn split(&mut self, cx: &mut Cx) {
        let at = self.playhead;
        let among = self.selected.clone();
        let made = self.edit(|p| p.split(at, if among.is_empty() { None } else { Some(&among) }));
        if made.is_empty() {
            self.say(tr("Nothing to split under the playhead"), cx);
        }
        cx.repaint();
    }

    pub fn delete(&mut self, ripple: bool, cx: &mut Cx) {
        let ids = self.selected.clone();
        if ids.is_empty() {
            // nothing selected: the gap under the playhead on the main track closes
            let at = self.playhead;
            let main = self.project.first_track(TrackKind::Video);
            if let Some(t) = main
                && self.project.tracks[t].clips.iter().all(|c| !(c.start <= at && at < c.end()))
                && self.edit(|p| p.close_gap(t, at))
            {
                self.say(tr("Gap closed"), cx);
            }
            return;
        }
        self.edit(|p| if ripple { p.ripple_delete(&ids) } else { p.delete(&ids) });
        self.selected.clear();
        cx.repaint();
    }

    pub fn add_text(&mut self, style: TextStyle, cx: &mut Cx) -> Id {
        let at = self.playhead;
        let id = self.edit(|p| p.add_text(at, style));
        self.selected = vec![id];
        self.tab = Tab::Text;
        self.screen = Screen::Edit;
        cx.repaint();
        id
    }

    /// The selected clip (exactly one).
    pub fn sel(&self) -> Option<&model::Clip> {
        match self.selected.as_slice() {
            [id] => self.project.clip(*id),
            _ => None,
        }
    }

    pub fn set_transition(&mut self, kind: Option<model::TransKind>, cx: &mut Cx) {
        let Some(id) = self.selected.first().copied() else {
            self.say(tr("Select the clip a transition leads into"), cx);
            return;
        };
        let len = self.project.clip(id).and_then(|c| c.trans).map_or(self.project.fps as i64 / 2, |t| t.len);
        let r = self.edit(|p| p.set_transition(id, kind.map(|k| model::Transition { kind: k, len })));
        if let Err(e) = r {
            self.say(tr(e), cx);
        }
        cx.repaint();
    }

    // ───────────────────────── media ─────────────────────────

    /// Probes files on a thread; they join the bin (and the timeline, with `append`). A project file opens.
    pub fn import(&mut self, paths: Vec<PathBuf>, append: bool, cx: &mut Cx) {
        if let Some(pf) = paths.iter().find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(project::EXT))) {
            let pf = pf.clone();
            self.open_project(&pf, cx);
            return;
        }
        let mut files = Vec::new();
        for p in paths {
            if p.is_dir() {
                if let Ok(rd) = std::fs::read_dir(&p) {
                    let mut inner: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
                    inner.sort();
                    files.extend(inner);
                }
            } else if p.is_file() {
                files.push(p);
            }
        }
        if files.is_empty() {
            return;
        }
        if self.tools != Tools::Ready {
            self.say(tr("Plume Cut needs FFmpeg to read videos: install it first"), cx);
            self.install_tools(cx);
            return;
        }
        if self.screen == Screen::Start && !self.new_project(cx) {
            return;
        }
        self.importing += files.len();
        self.add_after_import |= append;
        let sender = cx.sender();
        std::thread::spawn(move || {
            // one after the other, in the order given (the timeline keeps it)
            let results: Vec<(PathBuf, Result<model::Media, String>)> = files.into_iter().map(|f| (f.clone(), ffmpeg::probe(&f))).collect();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.imported(results, append, cx);
                }
            });
        });
        cx.repaint();
    }

    fn imported(&mut self, results: Vec<(PathBuf, Result<model::Media, String>)>, append: bool, cx: &mut Cx) {
        self.importing = self.importing.saturating_sub(results.len());
        let mut failed = Vec::new();
        let mut added = Vec::new();
        for (path, r) in results {
            match r {
                Ok(mut m) => {
                    // the same file twice is one media
                    if let Some(old) = self.project.media.iter().find(|o| o.path == path) {
                        added.push(old.id);
                        continue;
                    }
                    let id = self.edit(|p| {
                        m.id = p.new_id();
                        p.media.push(m.clone());
                        m.id
                    });
                    m.id = id;
                    self.load_assets(&m, cx);
                    added.push(id);
                }
                Err(e) => failed.push(format!("{}: {e}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
            }
        }
        if append {
            for id in &added {
                let r = self.edit(|p| p.append(*id));
                if let Err(e) = r {
                    self.note(format!("append: {e}"));
                }
            }
            self.fit_zoom();
        }
        if let Some(e) = failed.first() {
            self.say(e.clone(), cx);
        }
        self.note(format!("imported {} failed {}", added.len(), failed.len()));
        cx.repaint();
    }

    /// Thumbnail, filmstrip and sound outline of a media, made on a thread.
    pub fn load_assets(&mut self, m: &model::Media, cx: &mut Cx) {
        let m = m.clone();
        let sender = cx.sender();
        let born = self.generation;
        std::thread::spawn(move || {
            let mut strip = Vec::new();
            if m.visual() {
                let (w, h) = (96, 54);
                strip = ffmpeg::filmstrip(&m.path, m.duration, m.kind == MediaKind::Image, if m.kind == MediaKind::Image { 1 } else { 24 }, w, h);
            }
            let wave = if m.has_audio { ffmpeg::waveform(&m.path, 20) } else { Vec::new() };
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h)
                    && app.generation == born
                {
                    let strip: Vec<Arc<Frame>> = strip.into_iter().map(Arc::new).collect();
                    if let Some(f) = strip.get(strip.len() / 3) {
                        app.thumbs.insert(m.id, f.clone());
                    }
                    app.strips.insert(m.id, strip);
                    app.waves.insert(m.id, wave);
                    cx.repaint();
                }
            });
        });
    }

    pub fn pick_media(&mut self, cx: &mut Cx) {
        let files = plume_ui::dialogs::pick_files(self.win.hwnd, tr("Import media"));
        if !files.is_empty() {
            self.import(files, false, cx);
        }
    }

    /// Adds a media of the bin at the end of its track.
    pub fn append_media(&mut self, id: Id, cx: &mut Cx) {
        let r = self.edit(|p| p.append(id));
        match r {
            Ok(c) => self.selected = vec![c],
            Err(e) => self.say(tr(e), cx),
        }
        cx.repaint();
    }

    pub fn install_tools(&mut self, cx: &mut Cx) {
        if matches!(self.tools, Tools::Installing(_) | Tools::Ready) {
            return;
        }
        self.tools = Tools::Installing(0.0);
        let sender = cx.sender();
        std::thread::spawn(move || {
            let s2 = sender;
            let r = tools::install(&mut |f| {
                s2.send(move |h, cx| {
                    if let Some(app) = with_app(h)
                        && matches!(app.tools, Tools::Installing(_))
                    {
                        app.tools = Tools::Installing(f);
                        cx.repaint();
                    }
                });
            });
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.tools = match r {
                        Ok(()) => Tools::Ready,
                        Err(e) => Tools::Failed(e),
                    };
                    cx.repaint();
                }
            });
        });
        cx.repaint();
    }

    // ───────────────────────── playing ─────────────────────────

    pub fn playing(&self) -> bool {
        self.player.is_some() || self.shuttle != 0.0
    }

    pub fn stop_play(&mut self) {
        if let Some(mut p) = self.player.take() {
            p.stop();
        }
        self.shuttle = 0.0;
    }

    pub fn toggle_play(&mut self, cx: &mut Cx) {
        if self.playing() {
            self.stop_play();
        } else {
            self.play(cx);
        }
        cx.repaint();
    }

    pub fn play(&mut self, cx: &mut Cx) {
        self.stop_play();
        if self.tools != Tools::Ready {
            return;
        }
        let end = self.project.duration();
        if self.playhead >= end - 1 {
            self.playhead = 0;
        }
        let (w, h) = self.play_size();
        match Player::start(&self.project, self.playhead, w, h, muted()) {
            Some(p) => self.player = Some(p),
            None => self.say(tr("Nothing to play"), cx),
        }
        cx.repaint();
    }

    /// The size the preview plays at (reduced: the shorter side at most 360).
    pub fn play_size(&self) -> (u32, u32) {
        let r = self.project.ratio();
        let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
        let long = self.max_px.clamp(256, 640) as f64;
        if r >= 1.0 { (even(long), even(long / r)) } else { (even(long * r), even(long)) }
    }

    /// J / L: backwards or forwards, faster each press; K stops.
    pub fn shuttle_key(&mut self, dir: i32, cx: &mut Cx) {
        let fps = self.project.fps();
        if dir == 0 {
            self.stop_play();
        } else if dir > 0 && self.shuttle == 0.0 && self.player.is_none() {
            self.play(cx);
        } else {
            let cur = if self.player.is_some() { fps } else { self.shuttle };
            self.stop_play();
            let next = if cur.signum() as i32 == dir { (cur.abs() * 2.0).min(fps * 8.0) * dir as f64 } else { fps * dir as f64 };
            self.shuttle = next;
            self.shuttle_at = (Instant::now(), self.playhead);
        }
        cx.repaint();
    }

    pub fn seek(&mut self, frame: i64, cx: &mut Cx) {
        let was = self.player.is_some();
        self.stop_play();
        self.playhead = frame.clamp(0, self.project.duration().max(0));
        if was {
            self.play(cx);
        }
        cx.repaint();
    }

    /// Timeline zoom that shows the whole edit.
    pub fn fit_zoom(&mut self) {
        let w = (self.tl_rect.w - 160.0).max(300.0);
        let d = (self.project.duration().max(self.project.fps as i64 * 10)) as f32;
        self.zoom = (w / d).clamp(0.02, 40.0);
        self.tl_scroll = 0.0;
    }

    // ───────────────────────── export ─────────────────────────

    pub fn spec(&self) -> Spec {
        Spec {
            short: self.set.export_res,
            aspect: export::aspect_of(&self.set.export_aspect),
            codec: Codec::from_key(&self.set.export_codec),
            hw: self.set.hw,
            dir: self.set.out_dir(),
        }
    }

    pub fn start_export(&mut self, cx: &mut Cx) {
        if matches!(self.export, ExportState::Running { .. }) {
            return;
        }
        if self.project.duration() <= 0 {
            self.say(tr("The timeline is empty"), cx);
            return;
        }
        if self.tools != Tools::Ready {
            self.install_tools(cx);
            return;
        }
        self.stop_play();
        self.save_now();
        let spec = self.spec();
        let proj = self.project.clone();
        let ctl = Arc::new(Ctl::default());
        self.export_ctl = Some(ctl.clone());
        self.export = ExportState::Running { frac: 0.0, note: String::new(), started: Instant::now() };
        self.dialog = Some(Dialog::Export);
        let sender = cx.sender();
        std::thread::spawn(move || {
            let s2 = sender;
            let r = export::run(&proj, &spec, &ctl, &mut |msg| {
                s2.send(move |h, cx| {
                    if let Some(app) = with_app(h)
                        && let ExportState::Running { frac, note, .. } = &mut app.export
                    {
                        match msg {
                            export::Msg::Progress(f) => *frac = f,
                            export::Msg::Note(n) => *note = n,
                        }
                        cx.repaint();
                    }
                });
            });
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.export_ctl = None;
                    app.export = match r {
                        Ok((path, size)) => {
                            app.note(format!("exported {}", path.display()));
                            ExportState::Done { path, size }
                        }
                        Err(e) if e == ffmpeg::CANCELLED => ExportState::Idle,
                        Err(e) => {
                            app.note(format!("export failed: {e}"));
                            ExportState::Failed(e)
                        }
                    };
                    cx.repaint();
                }
            });
        });
        cx.repaint();
    }

    pub fn cancel_export(&mut self) {
        if let Some(c) = &self.export_ctl {
            c.cancel();
        }
    }

    /// The exported file, selected in the user's folder app (Plume FEx when it is theirs, else Explorer).
    pub fn show_in_folder(&self, path: &Path) {
        plume_ui::folder::show(path);
    }

    pub fn check_hw(&mut self, cx: &mut Cx) {
        if self.hw_label.is_some() || self.tools != Tools::Ready {
            return;
        }
        if let Some(k) = ffmpeg::hw_known() {
            self.hw_label = Some(k.map_or_else(|| tr("None found: the processor encodes").to_string(), |h| h.label().to_string()));
            return;
        }
        self.hw_label = Some(tr("Looking…").to_string());
        let sender = cx.sender();
        std::thread::spawn(move || {
            let hw = ffmpeg::hw_cached();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.hw_label = Some(hw.map_or_else(|| tr("None found: the processor encodes").to_string(), |h| h.label().to_string()));
                    cx.repaint();
                }
            });
        });
    }

    // ───────────────────────── frames for the preview ─────────────────────────

    /// Asks for the frames the preview needs now that are not there yet.
    pub fn request_frames(&mut self, wanted: Vec<(Key, PathBuf)>) {
        let missing: Vec<crate::preview::Request> = wanted
            .into_iter()
            .filter(|(k, _)| self.cache.get(k).is_none() && !self.cache.pending.contains(k))
            .map(|(key, path)| crate::preview::Request { key, path })
            .collect();
        if missing.is_empty() {
            return;
        }
        // older requests are dropped by the grabber: forget them
        self.cache.pending.clear();
        for r in &missing {
            self.cache.pending.insert(r.key);
        }
        if let Some(g) = &self.grabber {
            g.want(missing);
        }
    }

    fn tick(&mut self, cx: &mut Cx) {
        if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_millis(4500)) {
            self.toast = None;
            cx.repaint();
        }
        if self.dirty.is_some_and(|d| d.elapsed() > Duration::from_millis(1500)) {
            self.save_now();
        }
        if self.shuttle != 0.0 || matches!(self.export, ExportState::Running { .. }) {
            cx.repaint();
        }
    }

    pub fn open_args(&mut self, text: &str, cx: &mut Cx) {
        let args = split_args(text);
        let mut files: Vec<PathBuf> = Vec::new();
        let mut add = false;
        for a in args {
            if a == "--add" {
                add = true;
            } else if !a.starts_with("--") {
                files.push(PathBuf::from(a));
            }
        }
        if files.is_empty() {
            return;
        }
        if let Some(pf) = files.iter().find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(project::EXT))) {
            let pf = pf.clone();
            self.open_project(&pf, cx);
            return;
        }
        // "--add" (Plume Media's "Open in Plume Cut") and plain files: a new project holding them
        let _ = add;
        if self.new_project(cx) {
            self.import(files, true, cx);
        }
    }

    /// Files dropped on the window at `p`: on the timeline they go where they fall, elsewhere into the bin.
    pub fn dropped(&mut self, files: Vec<PathBuf>, p: Option<Pt>, cx: &mut Cx) {
        let on_timeline = p.is_some_and(|p| self.screen == Screen::Edit && self.tl_rect.contains(p));
        if self.screen == Screen::Start {
            self.open_args(&files.iter().map(|f| format!("\"{}\"", f.display())).collect::<Vec<_>>().join(" "), cx);
        } else {
            self.import(files, on_timeline, cx);
        }
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_all(c, cx);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        self.handle(ev, cx);
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(Hit::Min | Hit::Close) => Area::Control,
            Some(_) => Area::Client,
            None if p.y < TOP_H && self.dialog.is_none() => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::TrimL(_) | Hit::TrimR(_)) => Cursor::SizeWE,
            Some(Hit::Min | Hit::Max | Hit::Close | Hit::Field(_) | Hit::TrackArea | Hit::Ruler | Hit::PreviewArea | Hit::DialogBg | Hit::DialogCard)
            | None => Cursor::Arrow,
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
            Event::MouseMove { p, mods } => {
                self.mouse = p;
                if !matches!(self.drag, Drag::None) {
                    self.drag_to(p, mods, cx);
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
            Event::MouseDown { p, button: Button::Left, clicks, mods } => {
                self.mouse = p;
                self.pressed = self.hits.at(p).cloned();
                cx.capture(true);
                if let Some(h) = self.pressed.clone() {
                    self.press(h, p, clicks, mods, cx);
                }
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, mods } => {
                cx.capture(false);
                let dragged = self.end_drag(p, mods, cx);
                let up = self.hits.at(p).cloned();
                if !dragged
                    && let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    self.act(a, mods, cx);
                }
                self.pressed = None;
                cx.repaint();
            }
            Event::Wheel { p, dx, dy, mods } => self.wheel(p, dx, dy, mods, cx),
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::EditChanged(text) => {
                self.edit_text(text, cx);
            }
            Event::EditKey { vk: 0x0D | 0x1B | 0x09, mods } => {
                // Ctrl+Enter ends a text; Enter alone in the one-line fields
                let _ = mods;
                self.edit_for = None;
                cx.repaint();
            }
            Event::EditBlur => {
                self.edit_for = None;
                cx.repaint();
            }
            Event::Timer(TICK) => self.tick(cx),
            Event::Opened(text) => {
                self.win.bring_to_front();
                self.open_args(&text, cx);
            }
            Event::Close => {
                self.stop_play();
                self.cancel_export();
                self.save_now();
                self.set.save();
                self.grabber = None;
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

    fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        if self.edit_for.is_some() {
            return;
        }
        let fps = self.project.fps as i64;
        if let Some(d) = self.dialog {
            if vk == 0x1B {
                if d == Dialog::Export && matches!(self.export, ExportState::Running { .. }) {
                    return;
                }
                self.dialog = None;
                if !matches!(self.export, ExportState::Running { .. }) {
                    self.export = ExportState::Idle;
                }
                cx.handled();
                cx.repaint();
            }
            return;
        }
        if self.screen == Screen::Start {
            match vk {
                0x4E if mods.ctrl => {
                    self.new_project(cx);
                }
                0x4F if mods.ctrl => self.pick_project(cx),
                0x70 => self.dialog = Some(Dialog::Help),
                _ => return,
            }
            cx.handled();
            cx.repaint();
            return;
        }
        match vk {
            0x20 => self.toggle_play(cx),
            0x4A => self.shuttle_key(-1, cx),
            0x4B => self.shuttle_key(0, cx),
            0x4C => self.shuttle_key(1, cx),
            0x53 if !mods.ctrl => self.split(cx),
            0x53 if mods.ctrl => {
                self.dirty = Some(Instant::now());
                self.save_now();
                self.say(tr("Saved"), cx);
            }
            0x2E | 0x08 => self.delete(mods.shift, cx),
            0x5A if mods.ctrl && mods.shift => self.redo(cx),
            0x5A if mods.ctrl => self.undo(cx),
            0x59 if mods.ctrl => self.redo(cx),
            0x41 if mods.ctrl => self.selected = self.project.all_ids(),
            0x45 if mods.ctrl => {
                self.dialog = Some(Dialog::Export);
                self.check_hw(cx);
            }
            0x49 if mods.ctrl => self.pick_media(cx),
            0x4E if mods.ctrl => {
                self.new_project(cx);
            }
            0x4F if mods.ctrl => self.pick_project(cx),
            0x54 => {
                self.add_text(TextStyle { text: tr("Your text").into(), ..TextStyle::default() }, cx);
            }
            0x4E => {
                self.set.snap = !self.set.snap;
                self.set.save();
                self.say(if self.set.snap { tr("Snapping on") } else { tr("Snapping off") }, cx);
            }
            0x25 => self.seek(self.playhead - if mods.shift { fps } else { 1 }, cx),
            0x27 => self.seek(self.playhead + if mods.shift { fps } else { 1 }, cx),
            0x24 => self.seek(0, cx),
            0x23 => self.seek(self.project.duration(), cx),
            0x26 => self.jump_cut(-1, cx),
            0x28 => self.jump_cut(1, cx),
            0xBB | 0x6B => self.zoom_by(1.25, None),
            0xBD | 0x6D => self.zoom_by(0.8, None),
            0x5A => self.fit_zoom(),
            0x1B => self.selected.clear(),
            0x70 => self.dialog = Some(Dialog::Help),
            _ => return,
        }
        cx.handled();
        cx.repaint();
    }

    /// Up / Down: the previous or next clip edge.
    fn jump_cut(&mut self, dir: i64, cx: &mut Cx) {
        let pts = self.project.snap_points(&[], -1);
        let next = if dir > 0 { pts.iter().copied().find(|p| *p > self.playhead) } else { pts.iter().rev().copied().find(|p| *p < self.playhead && *p >= 0) };
        if let Some(f) = next {
            self.seek(f, cx);
        }
    }

    pub fn zoom_by(&mut self, k: f32, around: Option<f32>) {
        let x = around.unwrap_or(self.tl_x0 + (self.playhead as f32 * self.zoom - self.tl_scroll));
        let frame = (x - self.tl_x0 + self.tl_scroll) / self.zoom;
        self.zoom = (self.zoom * k).clamp(0.02, 40.0);
        self.tl_scroll = (frame * self.zoom - (x - self.tl_x0)).max(0.0);
    }

    fn wheel(&mut self, p: Pt, dx: f32, dy: f32, mods: Mods, cx: &mut Cx) {
        if self.dialog.is_some() {
            return;
        }
        if self.tl_rect.contains(p) {
            if mods.ctrl {
                self.zoom_by(if dy > 0.0 { 1.2 } else { 1.0 / 1.2 }, Some(p.x));
            } else if mods.shift || dx != 0.0 {
                self.tl_scroll = (self.tl_scroll - (dy + dx) * 80.0).max(0.0);
            } else {
                self.tl_vscroll = (self.tl_vscroll - dy * 40.0).max(0.0);
            }
        } else if self.screen == Screen::Edit && p.x < 320.0 {
            self.bin_scroll = (self.bin_scroll - dy * 60.0).clamp(0.0, self.bin_h.max(0.0));
        } else {
            self.props_scroll = (self.props_scroll - dy * 60.0).clamp(0.0, self.props_h.max(0.0));
        }
        cx.repaint();
    }

    fn edit_text(&mut self, text: String, cx: &mut Cx) {
        match self.edit_for {
            Some("text") => {
                if let Some(id) = self.selected.first().copied() {
                    let before = self.project.clone();
                    if let Some(st) = self.project.clip_mut(id).and_then(|c| c.text.as_mut()) {
                        st.text = text;
                    }
                    // one undo step for a whole typing session
                    if !self.history.can_undo() || self.typing.is_none() {
                        self.typing = Some(id);
                        self.commit(before);
                    } else {
                        self.changed();
                    }
                }
            }
            Some("name") => {
                let before = self.project.clone();
                self.project.name = text;
                if self.typing.is_none() {
                    self.typing = Some(0);
                    self.commit(before);
                } else {
                    self.changed();
                }
            }
            _ => {}
        }
        cx.repaint();
    }
}

/// Splits a command line into its arguments (quotes group words).
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for ch in s.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

/// Colours offered for texts and their boxes.
pub const SWATCHES: [&str; 10] = ["#ffffff", "#000000", "#ffd60a", "#ff375f", "#ee3fa6", "#bf5af2", "#0a84ff", "#64d2ff", "#30d158", "#ff9f0a"];

pub fn swatch_color(s: &str) -> Color {
    Color::parse(s).unwrap_or(Color::hex(0xffffff))
}

/// Ready-made texts of the bin: (name, style).
pub fn text_presets() -> Vec<(&'static str, TextStyle)> {
    vec![
        (tr("Title"), TextStyle { text: tr("Title").into(), size: 0.1, ..TextStyle::default() }),
        (tr("Subtitle"), TextStyle { text: tr("Subtitle").into(), size: 0.055, bold: false, ..TextStyle::default() }),
        (
            tr("Caption on a box"),
            TextStyle { text: tr("Your caption").into(), size: 0.05, bold: true, bg_opacity: 0.65, anim_in: model::Anim::Rise, ..TextStyle::default() },
        ),
        (
            tr("Big and bold"),
            TextStyle {
                text: tr("WOW").into(),
                font: "Impact".into(),
                size: 0.16,
                color: "#ffd60a".into(),
                anim_in: model::Anim::Slide,
                ..TextStyle::default()
            },
        ),
    ]
}

pub fn track_label(k: TrackKind) -> &'static str {
    match k {
        TrackKind::Video => tr("Main"),
        TrackKind::Overlay => tr("Overlay"),
        TrackKind::Text => tr("Text"),
        TrackKind::Audio => tr("Sound"),
        TrackKind::Music => tr("Music"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn args() {
        assert_eq!(super::split_args(r#"--add "C:\a b\c.mp4" "d.png"  e"#), vec!["--add", r"C:\a b\c.mp4", "d.png", "e"]);
        assert!(super::split_args("  ").is_empty());
    }
}
