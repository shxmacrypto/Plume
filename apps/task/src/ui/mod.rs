//! Plume Task's window. Everything is drawn by hand (Direct2D): a sidebar with the pages, then the page itself —
//! Apps (programs and their processes), Performance (live graphs), Startup apps, Settings. The numbers come from
//! the sampling thread once per interval; the window only sorts, filters and draws them.

mod apps;
pub mod draw;
mod paint;
mod perf;
mod settings_view;
mod startup_view;

use crate::{
    i18n::{fill, tr},
    model::{self, Col, Group, Summary, Sys},
    sampler::{self, Control, Sample},
    settings::Settings,
    startup::{self, Entry},
    sys::{self, Why},
};
use plume_ui::{Area, Button, Canvas, Color, Cursor, Cx, Event, Gfx, Handler, Hits, Mods, PaintCx, Pt, Window};
use std::{
    any::Any,
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

pub const SIDEBAR_W: f32 = 224.0;
pub const ROW_H: f32 = 40.0;
/// Seconds of history in the graphs.
pub const HIST: usize = 60;
const TOAST: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Apps,
    Perf,
    Startup,
    Settings,
}

impl Page {
    pub fn key(self) -> &'static str {
        match self {
            Page::Apps => "apps",
            Page::Perf => "perf",
            Page::Startup => "startup",
            Page::Settings => "settings",
        }
    }
    pub fn from_key(k: &str) -> Page {
        match k {
            "perf" => Page::Perf,
            "startup" => Page::Startup,
            "settings" => Page::Settings,
            _ => Page::Apps,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Nav(Page),
    SummaryShow,
    SearchBox,
    SearchClear,
    /// "Resume" in the empty list while refreshing is paused.
    Resume,
    EndTask,
    Details,
    Header(Col),
    Row(usize),
    Chevron(usize),
    MenuItem(usize),
    ModalOk,
    ModalCancel,
    ModalAlt,
    Toast,
    StartToggle(usize),
    StartRow(usize),
    StartRefresh,
    Choice(&'static str, usize),
    Toggle(&'static str),
    Scrollbar,
    /// Covers what is under a dialog: clicks there do nothing.
    Inert,
}

/// A line of the Apps list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    Section(&'static str, usize),
    Group(usize),
    Proc(usize, usize),
}

/// What is selected (kept across refreshes).
#[derive(Clone, Debug, PartialEq)]
pub enum Sel {
    Group(String),
    Proc(u32, i64),
}

#[derive(Clone, Debug)]
pub enum MenuAct {
    End,
    Location,
    Details,
    Expand,
}

pub struct Menu {
    pub x: f32,
    pub y: f32,
    pub items: Vec<(String, MenuAct)>,
}

/// A process the "End task" dialog is about.
#[derive(Clone, Debug)]
pub struct Target {
    pub pid: u32,
    pub create: i64,
    pub ok: Result<(), Why>,
}

#[derive(Clone, Debug)]
pub enum Modal {
    End { name: String, targets: Vec<Target> },
    Details { title: String, fields: Vec<(String, String)>, path: Option<String> },
}

/// The colours of the window: plume-ui's theme (the same as every Plume app) with Plume Task's accent, the purple
/// of its icon. Only the graphs add hues of their own (one per part of the PC), never greys.
#[derive(Clone, Debug)]
pub struct Pal {
    pub bg: Color,
    /// The sidebar.
    pub side: Color,
    pub card: Color,
    /// Dialogs, menus (opaque).
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
    pub accent: Color,
    pub heat: Color,
    pub red: Color,
    pub orange: Color,
    pub green: Color,
    pub close_hover: Color,
    pub blue: Color,
    pub teal: Color,
}

/// Plume Task's icon colour.
pub const ACCENT: u32 = 0xa855f7;

impl Pal {
    pub fn new(dark: bool) -> Pal {
        let t = plume_ui::Theme::new(dark, Some(Color::hex(ACCENT)));
        let base = if dark { plume_ui::Theme::dark() } else { plume_ui::Theme::light() };
        Pal {
            bg: t.bg,
            side: t.bg2,
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
            accent: t.accent,
            heat: t.orange,
            red: t.red,
            orange: t.orange,
            green: t.green,
            close_hover: t.close_hover,
            // graph hues: Plume's blue, and a teal for memory
            blue: base.accent,
            teal: if dark { Color::hex(0x40c8e0) } else { Color::hex(0x0e8fa8) },
        }
    }
}

/// The graphs' history.
#[derive(Default)]
pub struct Hist {
    pub cpu: VecDeque<f32>,
    pub mem: VecDeque<f32>,
    pub disk: VecDeque<f32>,
    pub net_in: VecDeque<f32>,
    pub net_out: VecDeque<f32>,
    pub gpu: VecDeque<f32>,
    pub own: VecDeque<f32>,
}

fn push(q: &mut VecDeque<f32>, v: f32) {
    if q.len() >= HIST {
        q.pop_front();
    }
    q.push_back(v);
}

fn avg(q: &VecDeque<f32>, n: usize) -> f32 {
    let n = n.min(q.len());
    if n == 0 { 0.0 } else { q.iter().rev().take(n).sum::<f32>() / n as f32 }
}

pub struct App {
    pub win: Window,
    #[allow(dead_code)]
    pub gfx: Rc<Gfx>,
    pub pal: Pal,
    pub set: Settings,
    pub page: Page,
    pub ctl: Arc<Control>,
    pub sys: Sys,
    pub groups: Vec<Group>,
    pub rows: Vec<Row>,
    pub expanded: HashSet<String>,
    pub sel: Option<Sel>,
    pub query: String,
    pub summary: Option<Summary>,
    pub hist: Hist,
    pub icons: HashMap<u64, (u32, u32, Vec<u8>)>,
    pub samples: u64,
    pub took_us: u64,
    pub startup: Option<Vec<Entry>>,
    pub startup_busy: bool,
    pub startup_src: Arc<startup::Source>,
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub scroll: HashMap<Page, f32>,
    pub content_h: f32,
    pub view_h: f32,
    pub list_top: f32,
    pub drag_scroll: Option<(f32, f32)>,
    pub menu: Option<Menu>,
    pub modal: Option<Modal>,
    pub toast: Option<(String, Instant)>,
    pub edit_for: Option<&'static str>,
    pub edit_shown: Option<&'static str>,
    pub search_rect: Option<plume_ui::Rect>,
    pub log: Vec<String>,
    #[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
    pub started: Instant,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

fn deliver(s: Sample) -> sampler::Deliver {
    Box::new(move |h, cx| {
        if let Some(app) = with_app(h) {
            app.on_sample(s, cx);
        }
    })
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
        let ctl = sampler::start(win.sender(), set.interval, deliver);
        let page = Page::from_key(&set.page);
        App {
            win,
            gfx,
            pal: Pal::new(dark),
            page,
            ctl,
            sys: Sys::default(),
            groups: Vec::new(),
            rows: Vec::new(),
            expanded: HashSet::new(),
            sel: None,
            query: String::new(),
            summary: None,
            hist: Hist::default(),
            icons: HashMap::new(),
            samples: 0,
            took_us: 0,
            startup: None,
            startup_busy: false,
            startup_src: Arc::new(startup::Source::from_env()),
            hits: Hits::default(),
            hover: None,
            pressed: None,
            scroll: HashMap::new(),
            content_h: 0.0,
            view_h: 0.0,
            list_top: 0.0,
            drag_scroll: None,
            menu: None,
            modal: None,
            toast: None,
            edit_for: None,
            edit_shown: None,
            search_rect: None,
            log: Vec::new(),
            started: Instant::now(),
            set,
        }
    }

    pub fn note(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 50 {
            self.log.remove(0);
        }
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        let t = text.into();
        self.note(format!("toast: {t}"));
        self.toast = Some((t, Instant::now()));
        cx.timer(TOAST, Some(4500));
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
        self.ctl.interval.store(self.set.interval, Ordering::Relaxed);
        self.ctl.wake();
        self.rebuild();
        cx.repaint();
    }

    pub fn go(&mut self, p: Page, cx: &mut Cx) {
        self.page = p;
        self.set.page = p.key().into();
        self.menu = None;
        if self.edit_for.is_some() {
            self.edit_for = None;
        }
        if p == Page::Startup && self.startup.is_none() {
            self.load_startup(cx);
        }
        cx.repaint();
    }

    pub fn scroll(&self) -> f32 {
        self.scroll.get(&self.page).copied().unwrap_or(0.0)
    }

    pub fn set_scroll(&mut self, v: f32) {
        let max = (self.content_h - self.view_h).max(0.0);
        self.scroll.insert(self.page, v.clamp(0.0, max));
    }

    pub fn on_sample(&mut self, s: Sample, cx: &mut Cx) {
        self.samples += 1;
        self.took_us = s.took_us;
        let sys = s.sys;
        push(&mut self.hist.cpu, sys.cpu);
        push(&mut self.hist.mem, sys.mem_pct());
        push(&mut self.hist.disk, sys.disk_active);
        push(&mut self.hist.net_in, sys.net_in as f32);
        push(&mut self.hist.net_out, sys.net_out as f32);
        push(&mut self.hist.gpu, sys.gpu);
        if self.samples > 2 {
            push(&mut self.hist.own, sys.own_cpu);
        }
        self.sys = sys;
        for (k, w, h, px) in s.icons {
            self.icons.insert(k, (w, h, px));
        }
        if let Some(groups) = s.groups {
            self.groups = groups;
            // the summary looks at the last few seconds, not one noisy reading
            let mut avg_sys = self.sys.clone();
            avg_sys.cpu = avg(&self.hist.cpu, 5);
            avg_sys.disk_active = avg(&self.hist.disk, 5);
            avg_sys.gpu = avg(&self.hist.gpu, 5);
            self.summary = Some(model::summary(&avg_sys, &self.groups));
            self.rebuild();
        }
        cx.repaint();
    }

    /// Sorts, filters and flattens the programs into the list's rows.
    pub fn rebuild(&mut self) {
        model::sort_groups(&mut self.groups, self.set.sort, self.set.desc);
        self.rows.clear();
        let shown: Vec<usize> = (0..self.groups.len()).filter(|&i| model::matches(&self.groups[i], &self.query)).collect();
        let add = |rows: &mut Vec<Row>, gi: usize, groups: &[Group], expanded: &HashSet<String>| {
            rows.push(Row::Group(gi));
            let g = &groups[gi];
            if g.procs.len() > 1 && expanded.contains(&g.key) {
                for pi in 0..g.procs.len() {
                    rows.push(Row::Proc(gi, pi));
                }
            }
        };
        if self.set.sections {
            let apps: Vec<usize> = shown.iter().copied().filter(|&i| self.groups[i].windowed).collect();
            let bg: Vec<usize> = shown.iter().copied().filter(|&i| !self.groups[i].windowed).collect();
            for (label, list) in [("Apps", apps), ("Background processes", bg)] {
                if list.is_empty() {
                    continue;
                }
                self.rows.push(Row::Section(label, list.len()));
                for gi in list {
                    add(&mut self.rows, gi, &self.groups, &self.expanded);
                }
            }
        } else {
            for gi in shown {
                add(&mut self.rows, gi, &self.groups, &self.expanded);
            }
        }
    }

    pub fn group_by_key(&self, key: &str) -> Option<usize> {
        self.groups.iter().position(|g| g.key == key)
    }

    pub fn row_sel(&self, r: Row) -> Option<Sel> {
        match r {
            Row::Section(..) => None,
            Row::Group(gi) => Some(Sel::Group(self.groups[gi].key.clone())),
            Row::Proc(gi, pi) => {
                let p = &self.groups[gi].procs[pi];
                Some(Sel::Proc(p.pid, p.create))
            }
        }
    }

    pub fn sel_row(&self) -> Option<usize> {
        let sel = self.sel.as_ref()?;
        self.rows.iter().position(|r| self.row_sel(*r).as_ref() == Some(sel))
    }

    /// The selected program, and the process if one is selected.
    pub fn selected(&self) -> Option<(usize, Option<usize>)> {
        match self.sel.as_ref()? {
            Sel::Group(k) => self.group_by_key(k).map(|g| (g, None)),
            Sel::Proc(pid, create) => {
                self.groups.iter().enumerate().find_map(|(gi, g)| g.procs.iter().position(|p| p.pid == *pid && p.create == *create).map(|pi| (gi, Some(pi))))
            }
        }
    }

    pub fn toggle_expand(&mut self, gi: usize) {
        let key = self.groups[gi].key.clone();
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
        self.rebuild();
    }

    /// Selects a program and brings its row into view.
    pub fn reveal(&mut self, key: &str) {
        if !self.query.is_empty() && self.group_by_key(key).is_some_and(|g| !model::matches(&self.groups[g], &self.query)) {
            self.query.clear();
            self.rebuild();
        }
        self.sel = Some(Sel::Group(key.to_string()));
        if let Some(i) = self.sel_row() {
            let y = i as f32 * ROW_H;
            let s = self.scroll.get(&Page::Apps).copied().unwrap_or(0.0);
            let vh = self.view_h.max(ROW_H);
            if y < s || y + ROW_H > s + vh {
                self.scroll.insert(Page::Apps, (y - vh / 2.0).max(0.0));
            }
        }
    }

    pub fn open_end(&mut self, cx: &mut Cx) {
        let Some((gi, pi)) = self.selected() else { return };
        let g = &self.groups[gi];
        let procs: Vec<&model::Proc> = match pi {
            Some(pi) => vec![&g.procs[pi]],
            None => g.procs.iter().collect(),
        };
        let name = match pi {
            Some(pi) => format!("{} ({})", g.procs[pi].name, g.procs[pi].pid),
            None => g.name.clone(),
        };
        let targets = procs
            .iter()
            .map(|p| Target { pid: p.pid, create: p.create, ok: if p.pid == sys::own_pid() { Err(Why::System) } else { sys::can_end(p.pid, p.session) } })
            .collect();
        self.modal = Some(Modal::End { name, targets });
        self.menu = None;
        cx.repaint();
    }

    pub fn open_details(&mut self, cx: &mut Cx) {
        let Some((gi, pi)) = self.selected() else { return };
        let g = &self.groups[gi];
        let none = || tr("Not available").to_string();
        let mut f: Vec<(String, String)> = Vec::new();
        let (title, path) = match pi {
            Some(pi) => {
                let p = &g.procs[pi];
                f.push((tr("Program").into(), g.name.clone()));
                f.push((tr("Process").into(), p.name.clone()));
                f.push((tr("PID").into(), p.pid.to_string()));
                f.push((tr("Parent PID").into(), p.ppid.to_string()));
                f.push((tr("Started").into(), sys::local_time(p.create)));
                f.push((tr("Location").into(), p.path.clone().unwrap_or_else(none)));
                f.push((tr("Command line").into(), sys::command_line(p.pid).unwrap_or_else(none)));
                f.push((tr("Memory").into(), model::size(p.mem)));
                f.push((tr("Processor").into(), model::percent(p.cpu)));
                f.push((tr("Threads").into(), p.threads.to_string()));
                f.push((tr("Handles").into(), p.handles.to_string()));
                (format!("{} ({})", p.name, p.pid), p.path.clone())
            }
            None => {
                let main = g.main().cloned().unwrap_or_default();
                f.push((tr("Program").into(), g.name.clone()));
                if !g.publisher.is_empty() {
                    f.push((tr("Publisher").into(), g.publisher.clone()));
                }
                f.push((tr("Processes").into(), g.procs.len().to_string()));
                let pids: Vec<String> = g.procs.iter().take(12).map(|p| p.pid.to_string()).collect();
                let more = if g.procs.len() > 12 { " …" } else { "" };
                f.push((tr("PID").into(), format!("{}{more}", pids.join(", "))));
                f.push((tr("Started").into(), sys::local_time(main.create)));
                f.push((tr("Location").into(), g.path.clone().unwrap_or_else(none)));
                f.push((tr("Command line").into(), sys::command_line(main.pid).unwrap_or_else(none)));
                f.push((tr("Memory").into(), model::size(g.mem)));
                f.push((tr("Processor").into(), model::percent(g.cpu)));
                f.push((tr("Threads").into(), g.procs.iter().map(|p| p.threads).sum::<u32>().to_string()));
                (g.name.clone(), g.path.clone())
            }
        };
        self.modal = Some(Modal::Details { title, fields: f, path });
        self.menu = None;
        cx.repaint();
    }

    pub fn open_location(&mut self, cx: &mut Cx) {
        let path = self.selected().and_then(|(gi, pi)| match pi {
            Some(pi) => self.groups[gi].procs[pi].path.clone(),
            None => self.groups[gi].path.clone(),
        });
        match path {
            Some(p) => {
                self.note(format!("location: {p}"));
                sys::show_in_folder(&p);
            }
            None => self.say(tr("Windows doesn't show where this one is."), cx),
        }
    }

    /// Ends what the dialog lists (only those Windows allows), on another thread.
    pub fn confirm_end(&mut self, cx: &mut Cx) {
        let Some(Modal::End { name, targets }) = self.modal.take() else { return };
        let list: Vec<(u32, i64)> = targets.iter().filter(|t| t.ok.is_ok()).map(|t| (t.pid, t.create)).collect();
        if list.is_empty() {
            cx.repaint();
            return;
        }
        self.note(format!("end: {:?}", list.iter().map(|t| t.0).collect::<Vec<_>>()));
        let sender = cx.sender();
        let ctl = self.ctl.clone();
        std::thread::spawn(move || {
            let mut failed = 0;
            for (pid, create) in &list {
                if let Err(w) = sys::end(*pid, *create)
                    && w != Why::Gone
                {
                    failed += 1;
                }
            }
            ctl.wake();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    let msg = if failed == 0 { fill(tr("{} was ended."), &[&name]) } else { fill(tr("Windows refused to end part of {}."), &[&name]) };
                    app.say(msg, cx);
                }
            });
        });
        cx.repaint();
    }

    pub fn load_startup(&mut self, cx: &mut Cx) {
        if self.startup_busy {
            return;
        }
        self.startup_busy = true;
        let src = self.startup_src.clone();
        let sender = cx.sender();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
            }
            let list = src.list();
            let icons: Vec<(u64, sampler::Pixels)> =
                list.iter().filter_map(|e| e.exe.as_ref()).filter_map(|p| sys::icon_pixels(p).map(|px| (sampler::icon_key(p), px))).collect();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.startup_busy = false;
                    app.note(format!("startup: {} entries", list.len()));
                    app.startup = Some(list);
                    for (k, px) in icons {
                        app.icons.entry(k).or_insert(px);
                    }
                    cx.repaint();
                }
            });
        });
        cx.repaint();
    }

    pub fn toggle_startup(&mut self, i: usize, cx: &mut Cx) {
        let Some(e) = self.startup.as_ref().and_then(|l| l.get(i)).cloned() else { return };
        let src = self.startup_src.clone();
        let sender = cx.sender();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
            }
            let r = src.set_enabled(&e, !e.enabled);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    match r {
                        Ok(()) => {
                            app.note(format!("startup {}: {}", if e.enabled { "off" } else { "on" }, e.id));
                            if let Some(x) = app.startup.as_mut().and_then(|l| l.iter_mut().find(|x| x.id == e.id && x.kind == e.kind)) {
                                x.enabled = !e.enabled;
                            }
                            cx.repaint();
                        }
                        Err(m) => app.say(m, cx),
                    }
                }
            });
        });
    }

    pub fn startup_details(&mut self, i: usize, cx: &mut Cx) {
        let Some(e) = self.startup.as_ref().and_then(|l| l.get(i)).cloned() else { return };
        let mut f: Vec<(String, String)> = vec![(tr("Program").into(), e.name.clone())];
        if !e.publisher.is_empty() {
            f.push((tr("Publisher").into(), e.publisher.clone()));
        }
        f.push((tr("Where").into(), e.kind.place().into()));
        f.push((tr("Entry").into(), e.id.clone()));
        f.push((tr("Command line").into(), e.command.clone()));
        f.push((tr("State").into(), if e.enabled { tr("On") } else { tr("Off") }.into()));
        self.modal = Some(Modal::Details { title: e.name, fields: f, path: e.exe.filter(|p| std::path::Path::new(p).is_absolute()) });
        cx.repaint();
    }

    pub fn own_cpu_avg(&self) -> f32 {
        avg(&self.hist.own, HIST)
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
            None if p.y < 44.0 && self.modal.is_none() => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Min | Hit::Max | Hit::Close | Hit::SearchBox | Hit::Row(_) | Hit::Inert) | None => Cursor::Arrow,
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
                if let Some((y0, s0)) = self.drag_scroll {
                    let track = self.view_h.max(1.0);
                    let k = self.content_h / track;
                    self.set_scroll(s0 + (p.y - y0) * k);
                    cx.repaint();
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
                let h = self.hits.at(p).cloned();
                if self.menu.is_some() && !matches!(h, Some(Hit::MenuItem(_))) {
                    self.menu = None;
                    cx.repaint();
                    return;
                }
                if h == Some(Hit::Scrollbar) {
                    self.drag_scroll = Some((p.y, self.scroll()));
                    cx.capture(true);
                    return;
                }
                // rows react on press, like a list should
                if let Some(Hit::Row(i)) = h {
                    self.click_row(i, clicks, mods, cx);
                    return;
                }
                self.pressed = h;
                cx.capture(true);
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                if self.drag_scroll.take().is_some() {
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
                if let Some(Hit::Row(i)) = self.hits.at(p).cloned() {
                    self.click_row(i, 1, Mods::default(), cx);
                    self.row_menu(i, p, cx);
                }
            }
            Event::Wheel { dy, .. } => {
                if self.modal.is_none() {
                    self.menu = None;
                    self.set_scroll(self.scroll() - dy * ROW_H * 3.0);
                    cx.repaint();
                }
            }
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::EditChanged(text) => {
                if self.edit_for == Some("search") {
                    self.query = text;
                    self.scroll.insert(Page::Apps, 0.0);
                    self.rebuild();
                    cx.repaint();
                }
            }
            Event::EditKey { vk: 0x1B, .. } => {
                self.query.clear();
                self.edit_for = None;
                self.rebuild();
                cx.repaint();
            }
            Event::EditKey { vk: 0x0D | 0x28, .. } => {
                // Enter or Down: from the search to the list
                self.edit_for = None;
                if self.sel.is_none()
                    && let Some(r) = self.rows.iter().find(|r| !matches!(r, Row::Section(..)))
                {
                    self.sel = self.row_sel(*r);
                }
                cx.repaint();
            }
            Event::EditBlur => {}
            Event::Timer(TOAST) => {
                cx.timer(TOAST, None);
                self.toast = None;
                cx.repaint();
            }
            Event::Resized => {
                let visible = !self.win.is_minimized();
                if self.ctl.visible.swap(visible, Ordering::Relaxed) != visible && visible {
                    self.ctl.wake();
                }
                cx.repaint();
            }
            Event::Close => {
                self.set.save();
            }
            Event::SystemTheme => {
                if self.set.theme == "system" {
                    self.set_theme();
                }
                cx.repaint();
            }
            Event::Scale | Event::Active(_) => cx.repaint(),
            Event::Opened(_) => self.win.bring_to_front(),
            _ => {}
        }
    }

    fn click_row(&mut self, i: usize, clicks: u8, _mods: Mods, cx: &mut Cx) {
        let Some(&r) = self.rows.get(i) else { return };
        match r {
            Row::Section(..) => {}
            Row::Group(gi) => {
                self.sel = self.row_sel(r);
                if clicks == 2 {
                    if self.groups[gi].procs.len() > 1 {
                        self.toggle_expand(gi);
                    } else {
                        self.open_details(cx);
                    }
                }
            }
            Row::Proc(..) => {
                self.sel = self.row_sel(r);
                if clicks == 2 {
                    self.open_details(cx);
                }
            }
        }
        if self.edit_for.is_some() {
            self.edit_for = None;
        }
        cx.repaint();
    }

    fn row_menu(&mut self, i: usize, p: Pt, cx: &mut Cx) {
        let Some(&r) = self.rows.get(i) else { return };
        let mut items = vec![
            (tr("End task").to_string(), MenuAct::End),
            (tr("Open file location").to_string(), MenuAct::Location),
            (tr("Details").to_string(), MenuAct::Details),
        ];
        if let Row::Group(gi) = r
            && self.groups[gi].procs.len() > 1
        {
            let open = self.expanded.contains(&self.groups[gi].key);
            items.push((if open { tr("Collapse") } else { tr("Show its processes") }.to_string(), MenuAct::Expand));
        }
        self.menu = Some(Menu { x: p.x, y: p.y, items });
        cx.repaint();
    }

    fn menu_act(&mut self, a: MenuAct, cx: &mut Cx) {
        self.menu = None;
        match a {
            MenuAct::End => self.open_end(cx),
            MenuAct::Location => self.open_location(cx),
            MenuAct::Details => self.open_details(cx),
            MenuAct::Expand => {
                if let Some((gi, _)) = self.selected() {
                    self.toggle_expand(gi);
                }
            }
        }
        cx.repaint();
    }

    pub fn act(&mut self, hit: Hit, _p: Pt, cx: &mut Cx) {
        match hit {
            Hit::Min => self.win.minimize(),
            Hit::Max => self.win.toggle_maximize(),
            Hit::Close => self.win.close(),
            Hit::Nav(p) => self.go(p, cx),
            Hit::SummaryShow => {
                if let Some(k) = self.summary.as_ref().and_then(|s| s.key.clone()) {
                    self.go(Page::Apps, cx);
                    self.reveal(&k);
                }
            }
            Hit::SearchBox => {
                self.edit_for = Some("search");
                self.edit_shown = None;
            }
            Hit::Resume => {
                self.set.interval = 1000;
                self.settings_changed(cx);
            }
            Hit::SearchClear => {
                self.query.clear();
                self.edit_for = None;
                self.rebuild();
            }
            Hit::EndTask => self.open_end(cx),
            Hit::Details => self.open_details(cx),
            Hit::Header(col) => {
                if self.set.sort == col {
                    self.set.desc = !self.set.desc;
                } else {
                    self.set.sort = col;
                    self.set.desc = col != Col::Name;
                }
                self.rebuild();
                self.set.save();
            }
            Hit::Row(_) => {}
            Hit::Chevron(i) => {
                if let Some(Row::Group(gi)) = self.rows.get(i).copied() {
                    self.toggle_expand(gi);
                }
            }
            Hit::MenuItem(i) => {
                if let Some(a) = self.menu.as_ref().and_then(|m| m.items.get(i)).map(|x| x.1.clone()) {
                    self.menu_act(a, cx);
                }
            }
            Hit::ModalOk => match &self.modal {
                Some(Modal::End { .. }) => self.confirm_end(cx),
                _ => self.modal = None,
            },
            Hit::ModalCancel => self.modal = None,
            Hit::ModalAlt => {
                if let Some(Modal::Details { path: Some(p), .. }) = &self.modal {
                    let p = p.clone();
                    self.note(format!("location: {p}"));
                    sys::show_in_folder(&p);
                }
            }
            Hit::Toast => self.toast = None,
            Hit::StartToggle(i) => self.toggle_startup(i, cx),
            Hit::StartRow(i) => self.startup_details(i, cx),
            Hit::StartRefresh => {
                self.startup = None;
                self.load_startup(cx);
            }
            Hit::Choice(..) | Hit::Toggle(_) => self.settings_act(hit, cx),
            Hit::Scrollbar | Hit::Inert => {}
        }
        cx.repaint();
    }

    fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        if self.modal.is_some() {
            match vk {
                0x1B => self.modal = None,
                0x0D => self.act(Hit::ModalOk, Pt::default(), cx),
                _ => return,
            }
            cx.handled();
            cx.repaint();
            return;
        }
        if self.menu.is_some() && vk == 0x1B {
            self.menu = None;
            cx.repaint();
            return;
        }
        match (vk, mods.ctrl) {
            // Ctrl+F: search
            (0x46, true) => {
                self.go(Page::Apps, cx);
                self.act(Hit::SearchBox, Pt::default(), cx);
            }
            // Ctrl+1..4: pages
            (0x31..=0x34, true) => self.go([Page::Apps, Page::Perf, Page::Startup, Page::Settings][(vk - 0x31) as usize], cx),
            (0x2E, false) if self.page == Page::Apps => self.open_end(cx),
            (0x0D, false) if self.page == Page::Apps => self.open_details(cx),
            (0x26 | 0x28, false) if self.page == Page::Apps => {
                let rows: Vec<usize> = (0..self.rows.len()).filter(|&i| !matches!(self.rows[i], Row::Section(..))).collect();
                if rows.is_empty() {
                    return;
                }
                let cur = self.sel_row().and_then(|r| rows.iter().position(|&x| x == r));
                let next = match (cur, vk) {
                    (None, _) => 0,
                    (Some(c), 0x26) => c.saturating_sub(1),
                    (Some(c), _) => (c + 1).min(rows.len() - 1),
                };
                self.sel = self.row_sel(self.rows[rows[next]]);
                if let Some(Sel::Group(k)) = self.sel.clone() {
                    self.reveal(&k);
                }
            }
            (0x25 | 0x27, false) if self.page == Page::Apps => {
                if let Some((gi, None)) = self.selected() {
                    let open = self.expanded.contains(&self.groups[gi].key);
                    if (vk == 0x27) != open {
                        self.toggle_expand(gi);
                    }
                }
            }
            (0x74, false) => self.ctl.wake(),
            _ => return,
        }
        cx.handled();
        cx.repaint();
    }
}
