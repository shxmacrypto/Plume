//! Plume Convert's window, drawn by hand (Direct2D): the presets and their choices on the left, the drop zone and
//! the queue on the right, the settings page. Conversions run one at a time on a worker thread and report here.

mod choices;
pub mod draw;
mod paint;
mod settings_view;

use crate::{
    ffmpeg,
    i18n::tr,
    opts::{Preset, parse_time},
    queue::{self, Ctl, Msg, Spec},
    settings::Settings,
    tools,
};
use plume_ui::{Area, Button, Canvas, Color, Cursor, Cx, Event, Gfx, Handler, Hits, PaintCx, Pt, Window};
use std::{
    any::Any,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use windows::Win32::Graphics::Imaging::IWICImagingFactory;

pub const TICK: usize = 1;
pub const TOP_H: f32 = 56.0;
pub const LEFT_W: f32 = 372.0;
pub const BOTTOM_H: f32 = 76.0;
pub const ROW_H: f32 = 68.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Main,
    Settings,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Gear,
    Back,
    Preset(Preset),
    /// A choice in a row of chips: the option's name and its index.
    Chip(&'static str, usize),
    Toggle(&'static str),
    Field(&'static str),
    AddFiles,
    DropZone,
    Convert,
    CancelAll,
    ClearDone,
    OutFolder,
    OutReset,
    ItemCancel(u64),
    ItemRetry(u64),
    ItemOpen(u64),
    ItemRemove(u64),
    Action(&'static str),
    Toast,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// Added, not started.
    Ready,
    Waiting,
    Running,
    Done,
    Failed(String),
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: u64,
    pub src: PathBuf,
    pub size: u64,
    pub state: State,
    pub frac: f64,
    pub started: Option<Instant>,
    pub note: String,
    pub out: Option<PathBuf>,
    pub out_size: u64,
    /// What it was asked to become (set when it is started).
    pub spec: Option<Spec>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tools {
    Ready,
    Missing,
    /// Download progress, 0..1.
    Installing(f32),
    Failed(String),
}

/// Plume Convert's colors: dark by default, a warm vermilion accent.
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
    pub green: Color,
    pub red: Color,
}

impl Pal {
    pub fn new(dark: bool) -> Pal {
        if dark {
            Pal {
                dark,
                bg: Color::hex(0x141416),
                side: Color::hex(0x0e0e10),
                card: Color::hex(0x1e1e22),
                line: Color::hex(0xffffff).fade(0.08),
                t1: Color::hex(0xffffff),
                t2: Color::hex(0xb8b8c0),
                t3: Color::hex(0x7e7e88),
                accent: Color::hex(0xff7a45),
                on_accent: Color::hex(0xffffff),
                green: Color::hex(0x30d158),
                red: Color::hex(0xff5a4f),
            }
        } else {
            Pal {
                dark,
                bg: Color::hex(0xf6f6f8),
                side: Color::hex(0xececf0),
                card: Color::hex(0xffffff),
                line: Color::hex(0x000000).fade(0.08),
                t1: Color::hex(0x111114),
                t2: Color::hex(0x55555c),
                t3: Color::hex(0x8a8a92),
                accent: Color::hex(0xd9480f),
                on_accent: Color::hex(0xffffff),
                green: Color::hex(0x248a3d),
                red: Color::hex(0xd70015),
            }
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
    pub set: Settings,
    pub view: View,
    pub items: Vec<Item>,
    pub next_id: u64,
    pub running: Option<(u64, Arc<Ctl>)>,
    pub tools: Tools,
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub left_scroll: f32,
    pub left_h: f32,
    pub queue_scroll: f32,
    pub queue_h: f32,
    pub edit_for: Option<&'static str>,
    pub edit_shown: Option<&'static str>,
    pub trim_text: (String, String),
    pub toast: Option<(String, Instant)>,
    pub hw_label: Option<String>,
    pub hw_checking: bool,
    pub menu_on: bool,
    pub log: Vec<String>,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
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
        let wic = crate::image::factory().expect("WIC");
        let trim_text =
            (crate::opts::clock(set.opts.trim_start, true), if set.opts.trim_end > 0.0 { crate::opts::clock(set.opts.trim_end, true) } else { String::new() });
        let app = App {
            win,
            gfx,
            wic,
            pal: Pal::new(dark),
            view: View::Main,
            items: Vec::new(),
            next_id: 1,
            running: None,
            tools: if tools::ready() { Tools::Ready } else { Tools::Missing },
            hits: Hits::default(),
            hover: None,
            pressed: None,
            left_scroll: 0.0,
            left_h: 0.0,
            queue_scroll: 0.0,
            queue_h: 0.0,
            edit_for: None,
            edit_shown: None,
            trim_text,
            toast: None,
            hw_label: None,
            hw_checking: false,
            menu_on: crate::shell::installed(),
            log: Vec::new(),
            set,
        };
        app.win.timer(TICK, Some(500));
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

    // ───────────────────────── files ─────────────────────────

    /// Files (and the files of dropped folders) join the queue, ready.
    pub fn add_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Cx) {
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
        let mut added = 0;
        for f in files {
            if self.items.iter().any(|i| i.src == f && matches!(i.state, State::Ready | State::Waiting | State::Running)) {
                continue;
            }
            let size = std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
            self.items.push(Item {
                id: self.next_id,
                src: f,
                size,
                state: State::Ready,
                frac: 0.0,
                started: None,
                note: String::new(),
                out: None,
                out_size: 0,
                spec: None,
            });
            self.next_id += 1;
            added += 1;
        }
        if added > 0 {
            self.note(format!("added {added}"));
        }
        cx.repaint();
    }

    pub fn pick_files(&mut self, cx: &mut Cx) {
        let files = plume_ui::dialogs::pick_files(self.win.hwnd, tr("Add files"));
        if !files.is_empty() {
            self.add_paths(files, cx);
        }
    }

    pub fn open_args(&mut self, text: &str, cx: &mut Cx) {
        let paths: Vec<PathBuf> = split_args(text).into_iter().map(PathBuf::from).collect();
        if !paths.is_empty() {
            self.add_paths(paths, cx);
        }
    }

    /// What the chosen preset and options ask for.
    pub fn spec_for(&self, src: &std::path::Path) -> Spec {
        let mut opts = self.set.opts.clone();
        if let Some(t) = parse_time(&self.trim_text.0) {
            opts.trim_start = t;
        }
        opts.trim_end = parse_time(&self.trim_text.1).unwrap_or(0.0);
        Spec { src: src.to_path_buf(), preset: self.set.preset, opts, out_dir: self.set.out_dir.clone(), keep_meta: self.set.keep_meta, hw: self.set.hw }
    }

    /// "Convert": every ready file gets the current preset and waits its turn.
    pub fn start(&mut self, cx: &mut Cx) {
        let ready: Vec<usize> = (0..self.items.len()).filter(|&i| self.items[i].state == State::Ready).collect();
        if ready.is_empty() {
            if self.items.is_empty() {
                self.pick_files(cx);
            }
            return;
        }
        for i in ready {
            let spec = self.spec_for(&self.items[i].src);
            let it = &mut self.items[i];
            it.spec = Some(spec);
            it.state = State::Waiting;
            it.frac = 0.0;
            it.note.clear();
        }
        self.set.save();
        self.pump(cx);
        cx.repaint();
    }

    pub fn retry(&mut self, id: u64, cx: &mut Cx) {
        if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
            if it.spec.is_none() {
                return;
            }
            it.state = State::Waiting;
            it.frac = 0.0;
            it.note.clear();
            it.out = None;
        }
        self.pump(cx);
        cx.repaint();
    }

    pub fn cancel(&mut self, id: u64, cx: &mut Cx) {
        if let Some((rid, ctl)) = &self.running
            && *rid == id
        {
            ctl.cancel();
        }
        if let Some(it) = self.items.iter_mut().find(|i| i.id == id)
            && matches!(it.state, State::Waiting | State::Ready)
        {
            it.state = State::Cancelled;
        }
        cx.repaint();
    }

    pub fn cancel_all(&mut self, cx: &mut Cx) {
        for it in self.items.iter_mut() {
            if it.state == State::Waiting {
                it.state = State::Cancelled;
            }
        }
        if let Some((_, ctl)) = &self.running {
            ctl.cancel();
        }
        cx.repaint();
    }

    pub fn remove(&mut self, id: u64, cx: &mut Cx) {
        if self.running.as_ref().is_some_and(|(r, _)| *r == id) {
            return;
        }
        self.items.retain(|i| i.id != id);
        cx.repaint();
    }

    pub fn clear_done(&mut self, cx: &mut Cx) {
        self.items.retain(|i| !matches!(i.state, State::Done | State::Cancelled | State::Failed(_)));
        self.queue_scroll = 0.0;
        cx.repaint();
    }

    /// Starts the next waiting file when nothing runs (FFmpeg is installed first when needed).
    pub fn pump(&mut self, cx: &mut Cx) {
        if self.running.is_some() {
            return;
        }
        let Some(i) = self.items.iter().position(|it| it.state == State::Waiting) else { return };
        let Some(spec) = self.items[i].spec.clone() else { return };
        let needs_ffmpeg = !spec.preset.is_image() || Fmt::from_name(&spec.opts.img_fmt) == Fmt::Webp;
        if needs_ffmpeg && !tools::ready() {
            if !matches!(self.tools, Tools::Installing(_)) {
                self.install_tools(cx);
            }
            return;
        }
        let id = self.items[i].id;
        let it = &mut self.items[i];
        it.state = State::Running;
        it.started = Some(Instant::now());
        it.frac = 0.0;
        let ctl = Arc::new(Ctl::default());
        self.running = Some((id, ctl.clone()));
        self.note(format!("start {id} {:?}", spec.preset));
        let sender = self.win.sender();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
            }
            let s2 = sender;
            let mut report = move |m: Msg| {
                s2.send(move |h, cx| {
                    if let Some(app) = with_app(h) {
                        app.on_msg(id, m, cx);
                    }
                });
            };
            let r = queue::run(&spec, &ctl, &mut report);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.finished(id, r, cx);
                }
            });
        });
        cx.repaint();
    }

    fn on_msg(&mut self, id: u64, m: Msg, cx: &mut Cx) {
        if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
            match m {
                Msg::Progress(f) => it.frac = f,
                Msg::Note(n) => it.note = n,
            }
            cx.repaint();
        }
    }

    fn finished(&mut self, id: u64, r: Result<(PathBuf, u64), String>, cx: &mut Cx) {
        if self.running.as_ref().is_some_and(|(r, _)| *r == id) {
            self.running = None;
        }
        if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
            match r {
                Ok((out, size)) => {
                    it.state = State::Done;
                    it.frac = 1.0;
                    it.out = Some(out);
                    it.out_size = size;
                    it.note.clear();
                }
                Err(e) if e == queue::CANCELLED => it.state = State::Cancelled,
                Err(e) => it.state = State::Failed(e),
            }
            let line = format!("end {id} {:?}", it.state);
            self.note(line);
        }
        self.pump(cx);
        cx.repaint();
    }

    pub fn install_tools(&mut self, cx: &mut Cx) {
        if matches!(self.tools, Tools::Installing(_)) {
            return;
        }
        self.tools = Tools::Installing(0.0);
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let s2 = sender;
            let mut last = 0.0f32;
            let r = tools::install(&mut |f| {
                if f - last > 0.01 {
                    last = f;
                    s2.send(move |h, cx| {
                        if let Some(app) = with_app(h)
                            && matches!(app.tools, Tools::Installing(_))
                        {
                            app.tools = Tools::Installing(f);
                            cx.repaint();
                        }
                    });
                }
            });
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    match r {
                        Ok(()) => {
                            app.tools = Tools::Ready;
                            app.say(tr("FFmpeg is ready"), cx);
                            app.pump(cx);
                        }
                        Err(e) => {
                            app.tools = Tools::Failed(e.clone());
                            for it in app.items.iter_mut().filter(|i| i.state == State::Waiting) {
                                it.state = State::Failed(crate::i18n::colon(tr("FFmpeg could not be downloaded"), &e));
                            }
                            cx.repaint();
                        }
                    }
                }
            });
        });
        cx.repaint();
    }

    /// Asks the graphics card once (on a thread) which encoder it has, for the settings page.
    pub fn check_hw(&mut self) {
        if self.hw_checking || self.hw_label.is_some() || !tools::ready() {
            return;
        }
        if let Some(known) = ffmpeg::hw_known() {
            self.hw_label = Some(known.map(|h| h.label().to_string()).unwrap_or_default());
            return;
        }
        self.hw_checking = true;
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let hw = ffmpeg::hw_cached(&tools::ffmpeg());
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.hw_checking = false;
                    app.hw_label = Some(hw.map(|h| h.label().to_string()).unwrap_or_default());
                    cx.repaint();
                }
            });
        });
    }

    pub fn toggle_menu(&mut self, cx: &mut Cx) {
        let exe = std::env::current_exe().unwrap_or_default();
        let r = if self.menu_on { crate::shell::uninstall() } else { crate::shell::install(&exe, tr("Convert with Plume")) };
        match r {
            Ok(()) => {
                self.menu_on = crate::shell::installed();
                self.set.context_menu = self.menu_on;
                self.set.save();
            }
            Err(e) => self.say(e, cx),
        }
        cx.repaint();
    }

    pub fn open_result(&mut self, id: u64) {
        let Some(out) = self.items.iter().find(|i| i.id == id).and_then(|i| i.out.clone()) else { return };
        // Explorer opens on the folder with the file selected
        let arg = if out.is_dir() { format!("\"{}\"", out.display()) } else { format!("/select,\"{}\"", out.display()) };
        let _ = std::process::Command::new("explorer.exe").raw_arg(arg).spawn();
    }

    pub fn busy(&self) -> bool {
        self.running.is_some() || self.items.iter().any(|i| i.state == State::Waiting)
    }

    fn tick(&mut self, cx: &mut Cx) {
        if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(4)) {
            self.toast = None;
            cx.repaint();
        }
        if self.running.is_some() {
            cx.repaint();
        }
    }
}

use crate::image::Fmt;
use std::os::windows::process::CommandExt;

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
            None if p.y < TOP_H => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Min | Hit::Max | Hit::Close | Hit::Field(_)) | None => Cursor::Arrow,
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
            Event::MouseDown { p, button: Button::Left, .. } => {
                self.pressed = self.hits.at(p).cloned();
                cx.capture(true);
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                let up = self.hits.at(p).cloned();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    self.act(a, cx);
                }
                cx.repaint();
            }
            Event::Wheel { p, dy, .. } => {
                let step = dy * 60.0;
                if self.view == View::Settings || p.x < LEFT_W {
                    self.left_scroll = (self.left_scroll - step).clamp(0.0, self.left_h.max(0.0));
                } else {
                    self.queue_scroll = (self.queue_scroll - step).clamp(0.0, self.queue_h.max(0.0));
                }
                cx.repaint();
            }
            Event::Key { vk, mods, .. } => match vk {
                // Ctrl+O: add files
                0x4F if mods.ctrl => self.pick_files(cx),
                0x0D if self.view == View::Main => self.start(cx),
                0x1B if self.view == View::Settings => {
                    self.view = View::Main;
                    self.left_scroll = 0.0;
                    cx.repaint();
                }
                _ => {}
            },
            Event::EditChanged(text) => {
                match self.edit_for {
                    Some("trim_start") => self.trim_text.0 = text,
                    Some("trim_end") => self.trim_text.1 = text,
                    _ => {}
                }
                self.sync_trim();
                cx.repaint();
            }
            Event::EditKey { vk: 0x0D | 0x1B | 0x09, .. } | Event::EditBlur => {
                self.edit_for = None;
                cx.repaint();
            }
            Event::Timer(TICK) => self.tick(cx),
            Event::Opened(text) => {
                self.win.bring_to_front();
                self.open_args(&text, cx);
            }
            Event::Close => {
                self.set.save();
                if let Some((_, ctl)) = &self.running {
                    ctl.cancel();
                }
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

    /// The trim fields, read back into the options when they make sense.
    pub fn sync_trim(&mut self) {
        if let Some(t) = parse_time(&self.trim_text.0) {
            self.set.opts.trim_start = t;
        } else if self.trim_text.0.trim().is_empty() {
            self.set.opts.trim_start = 0.0;
        }
        self.set.opts.trim_end = parse_time(&self.trim_text.1).unwrap_or(0.0);
    }

    pub fn act(&mut self, hit: Hit, cx: &mut Cx) {
        match hit {
            Hit::Min => cx.after(|w| w.minimize()),
            Hit::Max => cx.after(|w| w.toggle_maximize()),
            Hit::Close => cx.after(|w| w.close()),
            Hit::Gear => {
                self.view = if self.view == View::Settings { View::Main } else { View::Settings };
                self.left_scroll = 0.0;
                self.edit_for = None;
                if self.view == View::Settings {
                    self.check_hw();
                }
            }
            Hit::Back => {
                self.view = View::Main;
                self.left_scroll = 0.0;
            }
            Hit::Preset(p) => {
                self.set.preset = p;
                self.edit_for = None;
                self.set.save();
            }
            Hit::Chip(name, i) => {
                self.chip(name, i);
                self.set.save();
                if matches!(name, "theme" | "lang") {
                    self.settings_changed(cx);
                }
            }
            Hit::Toggle(name) => match name {
                "keep_meta" => {
                    self.set.keep_meta = !self.set.keep_meta;
                    self.set.save();
                }
                "hw" => {
                    self.set.hw = !self.set.hw;
                    self.set.save();
                    if self.set.hw {
                        self.check_hw();
                    }
                }
                "trim_exact" => {
                    self.set.opts.trim_exact = !self.set.opts.trim_exact;
                    self.set.save();
                }
                "context_menu" => self.toggle_menu(cx),
                _ => {}
            },
            Hit::Field(name) => {
                self.edit_for = Some(name);
                self.edit_shown = None;
            }
            Hit::AddFiles | Hit::DropZone => self.pick_files(cx),
            Hit::Convert => self.start(cx),
            Hit::CancelAll => self.cancel_all(cx),
            Hit::ClearDone => self.clear_done(cx),
            Hit::OutFolder => {
                if let Some(d) = plume_ui::dialogs::pick_folder(self.win.hwnd, tr("Save results in")) {
                    self.set.out_dir = Some(d);
                    self.set.save();
                }
            }
            Hit::OutReset => {
                self.set.out_dir = None;
                self.set.save();
            }
            Hit::ItemCancel(id) => self.cancel(id, cx),
            Hit::ItemRetry(id) => self.retry(id, cx),
            Hit::ItemOpen(id) => self.open_result(id),
            Hit::ItemRemove(id) => self.remove(id, cx),
            Hit::Action("install") => self.install_tools(cx),
            Hit::Toast => self.toast = None,
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

#[cfg(test)]
mod tests {
    #[test]
    fn args() {
        assert_eq!(super::split_args(r#""C:\a b\c.mp4" "d.png"  e"#), vec![r"C:\a b\c.mp4", "d.png", "e"]);
        assert!(super::split_args("  ").is_empty());
    }
}
