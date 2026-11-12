//! Plume Boost's window, drawn by hand (Direct2D): a sidebar with the pages, then the page itself — Overview (live
//! numbers, before/after), Tweaks (tick, then Apply), Processor & graphics (clocks, the plan's processor limits, and
//! where real overclocking is done), Changes (undo), Settings.
//!
//! Applying, undoing and measuring run on other threads; the window only shows their results. Nothing changes the
//! PC without a click on Apply (or on a Processor page choice, or Undo), and anything that needs administrator rights
//! goes through Windows' own UAC prompt.

mod changes_view;
mod cpu_view;
pub mod draw;
pub mod flows;
mod gpu_view;
pub mod monitor;
mod overview;
mod paint;
mod settings_view;
mod tweaks_view;

use crate::{
    backend::{Fail, Shared, guids},
    engine::{Engine, Status, UndoLog},
    i18n::{fill, tr},
    live::{self, Live, Wake},
    settings::{self, Settings},
    tweaks::{After, Profile, TWEAKS, by_id},
};
use flows::{Pending, Stress, Trying};
use plume_ui::{Area, Button, Canvas, Color, Cursor, Cx, Event, Gfx, Handler, Hits, PaintCx, Pt, Window};
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{Arc, Mutex, atomic::Ordering},
    time::Instant,
};

pub const SIDEBAR_W: f32 = 224.0;
const TOAST: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Overview,
    Tweaks,
    Cpu,
    Gpu,
    Changes,
    Settings,
}

impl Page {
    pub fn key(self) -> &'static str {
        match self {
            Page::Overview => "overview",
            Page::Tweaks => "tweaks",
            Page::Cpu => "cpu",
            Page::Gpu => "gpu",
            Page::Changes => "changes",
            Page::Settings => "settings",
        }
    }
    pub fn from_key(k: &str) -> Page {
        match k {
            "tweaks" => Page::Tweaks,
            "cpu" => Page::Cpu,
            "gpu" => Page::Gpu,
            "changes" => Page::Changes,
            "settings" => Page::Settings,
            _ => Page::Overview,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Nav(Page),
    /// A profile card of the Tweaks page.
    Profile(usize),
    /// The tick box of TWEAKS[i].
    Tick(usize),
    /// A tweak's row: opens or closes its details.
    Expand(usize),
    Apply,
    UndoAll,
    /// Undo of the i-th tweak of the Changes page.
    UndoTweak(usize),
    Measure,
    /// Overview's profile buttons: go to Tweaks with that profile ticked.
    StartProfile(usize),
    Choice(&'static str, usize),
    Toggle(&'static str),
    ModalOk,
    ModalCancel,
    ModalSwitch,
    Toast,
    Scrollbar,
    /// Covers what is under a dialog: clicks there do nothing.
    Inert,
    // ── graphics card page ──
    Card(usize),
    /// One step down (-1) or up (+1) of a knob.
    KnobStep(crate::gpu::Knob, i8),
    /// A slider's track (dragged).
    Slider(crate::gpu::Knob),
    FanMode(usize),
    FanFixed(i8),
    /// A fan curve point: (point, temperature or speed, step).
    FanPoint(usize, bool, i8),
    GpuTry,
    GpuStock,
    GpuDiscard,
    GpuTest,
    ProfileSave,
    ProfileLoad(usize),
    ProfileDelete(usize),
    ProfileSignin(usize),
    Monitor,
    // ── processor page ──
    CpuAttrib,
    // ── snapshots and Plume Backup ──
    SnapRestore(usize),
    SnapOpen(usize),
    BackupInstall,
    BackupWithout,
}

/// Why administrator rights are asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Why {
    /// Tweaks that change the whole PC (names).
    Apply(Vec<&'static str>),
    RestorePoint,
    Undo,
    /// A processor setting Windows refused.
    Power,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modal {
    Confirm {
        ids: Vec<&'static str>,
        restore: bool,
    },
    Elevate(Why),
    RestoreFailed {
        ids: Vec<&'static str>,
        reason: String,
    },
    UndoAll,
    Done {
        applied: Vec<&'static str>,
        already: usize,
        failed: Vec<(String, String)>,
        after: Option<After>,
    },
    /// Plume Backup isn't installed: install it, or go on without it.
    BackupMissing {
        pending: Pending,
    },
    /// "Keep these settings?" with its countdown.
    Keep,
    /// The stability test is running.
    Stress,
    StressDone {
        result: crate::gpu::StressResult,
        reverted: bool,
    },
    /// Restore a snapshot: what would change.
    Restore {
        dir: std::path::PathBuf,
        lines: Vec<String>,
        admin: bool,
    },
    /// The crash guard put the previous settings back.
    GpuReverted {
        reason: String,
    },
    /// Intel's overclocking waiver, before an Arc card is changed.
    Waiver,
}

/// Numbers taken just before and just after a batch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snap {
    pub plan: String,
    pub cpu_mhz: Option<f32>,
    pub timer: Option<(f32, f32, f32)>,
    pub mem_used: u64,
    pub wake: Option<Wake>,
}

fn take_snap(be: &Shared) -> Snap {
    let (used, _) = live::memory();
    Snap { plan: live::plan(be).1, cpu_mhz: live::cpu_now(), timer: live::timer_resolution(), mem_used: used, wake: live::measure_wake(250) }
}

/// The colours: plume-ui's theme (the same as every Plume app) with Plume Boost's accent, the cyan of its icon.
#[derive(Clone, Debug)]
pub struct Pal {
    pub bg: Color,
    pub side: Color,
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
    /// Text on the accent (the cyan is light: near-black on it).
    pub on_accent: Color,
    pub red: Color,
    pub orange: Color,
    pub green: Color,
    pub close_hover: Color,
}

/// Plume Boost's icon colour, a little lighter in dark mode and deeper in light mode.
pub const ACCENT_DARK: u32 = 0x22d3ee;
pub const ACCENT_LIGHT: u32 = 0x0891b2;

impl Pal {
    pub fn new(dark: bool) -> Pal {
        let t = plume_ui::Theme::new(dark, Some(Color::hex(if dark { ACCENT_DARK } else { ACCENT_LIGHT })));
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
            t4: t.t4,
            accent: t.accent,
            on_accent: if dark { Color::hex(0x0b1f24) } else { Color::hex(0xffffff) },
            red: t.red,
            orange: t.orange,
            green: t.green,
            close_hover: t.close_hover,
        }
    }
}

pub struct App {
    pub win: Window,
    #[allow(dead_code)]
    pub gfx: Rc<Gfx>,
    pub pal: Pal,
    pub set: Settings,
    pub page: Page,
    pub be: Shared,
    pub engine: Arc<Mutex<Engine>>,
    pub live: Live,
    pub live_ctl: Arc<live::Control>,
    /// Status of each tweak, in the order of TWEAKS.
    pub statuses: Vec<Status>,
    /// A copy of the undo log for drawing.
    pub log: UndoLog,
    /// The active plan's processor limits: [plugged in, on battery] × [min, max, boost].
    pub limits: [[Option<u32>; 5]; 2],
    /// Whether the processor settings are shown in Windows' advanced power options (attribute 0).
    pub attribs_shown: bool,
    pub cards: Vec<crate::gpu::Card>,
    /// What the selected card is set to, and the values in the controls (not applied until Try).
    pub gpu_now: Option<crate::gpu::Tuning>,
    pub draft: Option<crate::gpu::Tuning>,
    pub trying: Option<Trying>,
    pub stress: Option<Stress>,
    pub profiles: crate::gpu::Profiles,
    pub snaps_list: Vec<(std::path::PathBuf, crate::snapshot::Manifest)>,
    /// The last word about the snapshot and Plume Backup's copy.
    pub backup_line: String,
    /// "Apply without Plume Backup" was chosen: not asked again until Plume Boost restarts.
    pub backup_skip: bool,
    pub monitor: Option<Window>,
    /// Sliders drawn this frame, and the one being dragged.
    pub sliders: Vec<(crate::gpu::Knob, plume_ui::Rect)>,
    pub drag_knob: Option<crate::gpu::Knob>,
    first_live: bool,
    pub admin: bool,
    pub battery: bool,
    pub ticked: Vec<&'static str>,
    pub expanded: HashSet<&'static str>,
    pub wake: Option<Wake>,
    pub measuring: bool,
    pub busy: Option<&'static str>,
    pub snaps: Option<(Snap, Snap)>,
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub scroll: HashMap<Page, f32>,
    pub content_h: f32,
    pub view_h: f32,
    pub drag_scroll: Option<(f32, f32)>,
    pub modal: Option<Modal>,
    pub toast: Option<(String, Instant)>,
    pub notes: Vec<String>,
    #[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
    pub started: Instant,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, dark_system: bool, be: Shared) -> App {
        let set = Settings::load();
        crate::i18n::set(&set.lang);
        let dark = match set.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => dark_system,
        };
        let sender = win.sender();
        let live_ctl = live::start(be.clone(), move |l| {
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.on_live(l, cx);
                }
            });
        });
        let engine = Arc::new(Mutex::new(Engine::new(be.clone(), Some(settings::data_dir()))));
        let ticked: Vec<&'static str> = set.ticked.iter().filter_map(|id| by_id(id)).map(|t| t.id).collect();
        let mut app = App {
            win,
            gfx,
            pal: Pal::new(dark),
            page: Page::from_key(&set.page),
            be: be.clone(),
            engine,
            live: Live::default(),
            live_ctl,
            statuses: vec![Status::Off; TWEAKS.len()],
            log: UndoLog::default(),
            limits: [[None; 5]; 2],
            attribs_shown: false,
            cards: Vec::new(),
            gpu_now: None,
            draft: None,
            trying: None,
            stress: None,
            profiles: Default::default(),
            snaps_list: Vec::new(),
            backup_line: String::new(),
            backup_skip: false,
            monitor: None,
            sliders: Vec::new(),
            drag_knob: None,
            first_live: true,
            admin: be.is_admin(),
            battery: be.has_battery(),
            ticked,
            expanded: HashSet::new(),
            wake: None,
            measuring: false,
            busy: None,
            snaps: None,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            scroll: HashMap::new(),
            content_h: 0.0,
            view_h: 0.0,
            drag_scroll: None,
            modal: None,
            toast: None,
            notes: Vec::new(),
            started: Instant::now(),
            set,
        };
        app.refresh();
        app.reload_gpu();
        app.snaps_list = crate::snapshot::list();
        app
    }

    pub fn note(&mut self, s: impl Into<String>) {
        self.notes.push(s.into());
        if self.notes.len() > 60 {
            self.notes.remove(0);
        }
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        let t = text.into();
        self.note(format!("toast: {t}"));
        self.toast = Some((t, Instant::now()));
        cx.timer(TOAST, Some(4500));
        cx.repaint();
    }

    /// Reads the tweaks' state, the log and the plan's limits again (skipped while a batch holds the engine).
    pub fn refresh(&mut self) {
        let Ok(e) = self.engine.try_lock() else { return };
        self.statuses = TWEAKS.iter().map(|t| e.status(t)).collect();
        self.log = e.log.clone();
        drop(e);
        let plan = self.be.active_plan().unwrap_or_default();
        self.limits = live::limits(&self.be, &plan);
        self.attribs_shown = crate::snapshot::ATTRIB_SETTINGS.iter().all(|s| self.be.power_attrib(guids::SUB_PROCESSOR, s) == Some(0));
        self.admin = self.be.is_admin();
    }

    pub fn status_of(&self, id: &str) -> Status {
        TWEAKS.iter().position(|t| t.id == id).map(|i| self.statuses[i]).unwrap_or(Status::Unavailable)
    }

    pub fn available(&self, id: &str) -> bool {
        self.status_of(id) != Status::Unavailable
    }

    /// The ticked tweaks that Apply would change (not already on, available).
    pub fn to_apply(&self) -> Vec<&'static str> {
        self.ticked.iter().copied().filter(|id| matches!(self.status_of(id), Status::Off | Status::Partly)).collect()
    }

    fn on_live(&mut self, l: Live, cx: &mut Cx) {
        let samples = l.samples;
        self.live = l;
        if samples % 3 == 1 && self.busy.is_none() {
            self.refresh();
            if self.trying.is_none() && self.stress.is_none() && !self.draft_changed() {
                self.reload_gpu();
            }
        }
        if self.first_live {
            self.first_live = false;
            if self.set.monitor && self.monitor.is_none() {
                self.monitor = monitor::open(&self.gfx, self.pal.clone(), self.live.clone());
            }
        }
        if samples % 5 == 1 {
            self.check_guard(cx);
        }
        // the monitor window: its readings, or forget it once closed
        if let Some(w) = &self.monitor {
            if monitor::alive(w) {
                let (l, pal) = (self.live.clone(), self.pal.clone());
                w.with(move |h, cx| monitor::update(h, l, pal, cx));
            } else {
                self.monitor = None;
                self.set.monitor = false;
                self.set.save();
            }
        }
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
        self.set.ticked = self.ticked.iter().map(|s| s.to_string()).collect();
        self.set.save();
        crate::i18n::set(&self.set.lang);
        self.set_theme();
        cx.repaint();
    }

    pub fn go(&mut self, p: Page, cx: &mut Cx) {
        self.page = p;
        self.set.page = p.key().into();
        self.set.save();
        self.refresh();
        if p == Page::Gpu {
            self.reload_gpu();
        }
        if p == Page::Changes {
            self.snaps_list = crate::snapshot::list();
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

    // ── ticking ──────────────────────────────────────────────────────────────────────────────────────────────────

    fn profile_ticks(&self, p: Profile) -> Vec<&'static str> {
        p.ticks(|id| self.available(id))
    }

    fn match_profile(&mut self) {
        let mut mine = self.ticked.clone();
        mine.sort();
        self.set.profile = Profile::ALL
            .into_iter()
            .find(|p| {
                let mut t = self.profile_ticks(*p);
                t.sort();
                t == mine
            })
            .map(|p| p.key().to_string())
            .unwrap_or_default();
    }

    pub fn choose_profile(&mut self, p: Profile, cx: &mut Cx) {
        self.ticked = self.profile_ticks(p);
        self.set.profile = p.key().into();
        self.note(format!("profile {}: {:?}", p.key(), self.ticked));
        self.settings_changed(cx);
    }

    pub fn tick(&mut self, i: usize, cx: &mut Cx) {
        let Some(t) = TWEAKS.get(i) else { return };
        if !self.available(t.id) {
            return;
        }
        if let Some(pos) = self.ticked.iter().position(|x| *x == t.id) {
            self.ticked.remove(pos);
        } else {
            if let Some(g) = t.group {
                self.ticked.retain(|x| by_id(x).is_none_or(|o| o.group != Some(g)));
            }
            self.ticked.push(t.id);
        }
        self.match_profile();
        self.settings_changed(cx);
    }

    // ── applying ─────────────────────────────────────────────────────────────────────────────────────────────────

    pub fn ask_apply(&mut self, cx: &mut Cx) {
        if self.busy.is_some() {
            return;
        }
        let ids = self.to_apply();
        if ids.is_empty() {
            let msg = if self.ticked.is_empty() { tr("Tick at least one tweak first.") } else { tr("Everything ticked is already on.") };
            self.say(msg, cx);
            return;
        }
        let admin_ids = self.engine.lock().map(|e| e.admin_needed(&ids)).unwrap_or_default();
        self.modal = Some(if !admin_ids.is_empty() && !self.admin {
            Modal::Elevate(Why::Apply(admin_ids))
        } else {
            Modal::Confirm { ids, restore: self.set.restore_point }
        });
        cx.repaint();
    }

    fn confirm_ok(&mut self, cx: &mut Cx) {
        let Some(Modal::Confirm { ids, restore }) = self.modal.clone() else { return };
        if restore && !self.admin {
            self.modal = Some(Modal::Elevate(Why::RestorePoint));
            cx.repaint();
            return;
        }
        self.modal = None;
        self.guarded(Pending::Apply { ids, restore }, cx);
    }

    pub fn start_apply(&mut self, ids: Vec<&'static str>, restore: bool, cx: &mut Cx) {
        self.busy = Some(if restore { "Creating a restore point…" } else { "Applying…" });
        self.note(format!("apply {ids:?} restore={restore}"));
        cx.repaint();
        let (engine, be, sender) = (self.engine.clone(), self.be.clone(), cx.sender());
        let (pbe, why, detail, backup) = self.protect_args(&Pending::Apply { ids: ids.clone(), restore });
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
            }
            let pr = flows::protect(&pbe, why, &detail, backup);
            let before = take_snap(&be);
            let mut e = engine.lock().unwrap();
            if restore && let Err(f) = e.restore_point() {
                drop(e);
                let reason = f.text();
                sender.send(move |h, cx| {
                    if let Some(app) = with_app(h) {
                        app.busy = None;
                        app.on_protected(&pr);
                        app.note(format!("restore point failed: {reason}"));
                        app.modal = Some(Modal::RestoreFailed { ids, reason });
                        app.refresh();
                        cx.repaint();
                    }
                });
                return;
            }
            let r = e.apply(&ids);
            drop(e);
            // let Windows settle (timer, clocks) before reading again
            std::thread::sleep(std::time::Duration::from_millis(600));
            let after = take_snap(&be);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.busy = None;
                    app.on_protected(&pr);
                    app.note(format!("applied {:?}, already {:?}, failed {:?}", r.applied, r.already, r.failed));
                    app.snaps = Some((before, after));
                    app.refresh();
                    let needs_admin = r.failed.iter().any(|(_, f)| *f == Fail::NeedsAdmin);
                    app.modal = Some(if needs_admin && r.applied.is_empty() {
                        Modal::Elevate(Why::Apply(r.failed.iter().map(|f| f.0).collect()))
                    } else {
                        Modal::Done {
                            applied: r.applied.clone(),
                            already: r.already.len(),
                            failed: r.failed.iter().map(|(id, f)| (by_id(id).map(|t| t.title.get()).unwrap_or(id).to_string(), f.text())).collect(),
                            after: r.after,
                        }
                    });
                    cx.repaint();
                }
            });
        });
    }

    // ── undoing ──────────────────────────────────────────────────────────────────────────────────────────────────

    pub fn ask_undo_all(&mut self, cx: &mut Cx) {
        if self.log.entries.is_empty() || self.busy.is_some() {
            return;
        }
        self.modal = Some(Modal::UndoAll);
        cx.repaint();
    }

    fn undo_all_ok(&mut self, cx: &mut Cx) {
        if self.log.entries.iter().any(|e| e.before.needs_admin()) && !self.admin {
            self.modal = Some(Modal::Elevate(Why::Undo));
            cx.repaint();
            return;
        }
        self.modal = None;
        self.guarded(Pending::UndoAll, cx);
    }

    pub fn undo_tweak(&mut self, i: usize, cx: &mut Cx) {
        let changed = self.changed();
        let Some(id) = changed.get(i).cloned() else { return };
        if self.log.entries.iter().any(|e| e.tweak == id && e.before.needs_admin()) && !self.admin {
            self.modal = Some(Modal::Elevate(Why::Undo));
            cx.repaint();
            return;
        }
        self.run_undo(Some(id), cx);
    }

    /// Undoes one tweak, or everything (None: after a snapshot), on another thread.
    pub fn run_undo(&mut self, id: Option<String>, cx: &mut Cx) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some("Undoing…");
        self.note(format!("undo {}", id.as_deref().unwrap_or("everything")));
        cx.repaint();
        let (engine, sender) = (self.engine.clone(), cx.sender());
        let prot = id.is_none().then(|| self.protect_args(&Pending::UndoAll));
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
            }
            let pr = prot.map(|(be, why, detail, backup)| flows::protect(&be, why, &detail, backup));
            let fails: Vec<Fail> = {
                let mut e = engine.lock().unwrap();
                match &id {
                    Some(t) => e.undo(t),
                    None => e.undo_all().into_iter().map(|f| f.1).collect(),
                }
            };
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.busy = None;
                    if let Some(pr) = &pr {
                        app.on_protected(pr);
                    }
                    crate::gpu::Guard::clear();
                    app.refresh();
                    app.draft = None;
                    app.reload_gpu();
                    let msg = match (&id, fails.len()) {
                        (_, n) if n > 0 => fill(tr("{} changes could not be put back: {}"), &[&n.to_string(), &fails[0].text()]),
                        (None, _) => tr("Everything Plume Boost changed is back as it was.").to_string(),
                        (Some(t), _) => fill(tr("{} is back as it was."), &[&changed_title(t)]),
                    };
                    app.say(msg, cx);
                }
            });
        });
    }

    /// The tweaks with changes in the log, newest first.
    pub fn changed(&self) -> Vec<String> {
        self.log.changed()
    }

    // ── the processor page ───────────────────────────────────────────────────────────────────────────────────────

    /// A processor limit chosen on the Processor page: applied at once to the active plan, logged as "cpu".
    pub fn set_limit(&mut self, setting: &'static str, ac: bool, val: u32, cx: &mut Cx) {
        if self.busy.is_some() {
            return;
        }
        let cur = self.limits[if ac { 0 } else { 1 }][limit_index(setting)];
        if cur == Some(val) {
            return;
        }
        self.note(format!("limit {setting} ac={ac} -> {val}"));
        self.guarded(Pending::Limit { setting, ac, val }, cx);
    }

    /// A slider dragged to `x`.
    fn slide_to(&mut self, k: crate::gpu::Knob, x: f32) {
        let Some((_, r)) = self.sliders.iter().find(|(n, _)| *n == k).copied() else { return };
        let Some(range) = self.gpu_card().and_then(|c| c.caps.range(k)) else { return };
        let t = ((x - r.x) / r.w.max(1.0)).clamp(0.0, 1.0) as f64;
        self.knob_set(k, range.min + t * (range.max - range.min));
    }

    pub fn measure(&mut self, cx: &mut Cx) {
        if self.measuring {
            return;
        }
        self.measuring = true;
        cx.repaint();
        let sender = cx.sender();
        std::thread::spawn(move || {
            let w = live::measure_wake(400);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.measuring = false;
                    app.wake = w;
                    app.note(format!("wake: {w:?}"));
                    cx.repaint();
                }
            });
        });
    }

    // ── administrator rights ─────────────────────────────────────────────────────────────────────────────────────

    /// "Restart as administrator": Windows' UAC prompt, then this window closes and the new one opens on the same
    /// page with the same ticks. On the imaginary PC of the tests, it simply gets the rights.
    pub fn elevate(&mut self, cx: &mut Cx) {
        self.modal = None;
        if self.be.grant_admin_for_test() {
            self.note("elevate: test PC granted administrator rights");
            self.refresh();
            self.say(tr("Test mode: the imaginary PC now has administrator rights."), cx);
            return;
        }
        self.settings_changed(cx);
        match crate::run_elevated(Some(self.win.hwnd), "--elevated", false) {
            Some(_) => {
                self.note("elevate: started as administrator, closing");
                self.live_ctl.stop.store(true, Ordering::Relaxed);
                self.win.close();
            }
            None => self.say(tr("Plume Boost was not started as administrator."), cx),
        }
    }
}

/// The title of a tweak (or of the Processor / Graphics pages' own changes).
pub fn changed_title(id: &str) -> String {
    match crate::tweaks::any_by_id(id) {
        Some(t) => t.title.get().to_string(),
        None if id == "cpu" => tr("Processor limits").to_string(),
        None if id == "gpu" => crate::i18n::tf("Graphics card", "Carte graphique").to_string(),
        None if id == "gpu_signin" => crate::i18n::tf("Graphics profile at sign-in", "Profil graphique à la connexion").to_string(),
        None => id.to_string(),
    }
}

/// "Plugged in" / "On battery" × the processor settings: the Processor page's choices.
pub const LIMIT_ROWS: [(&str, bool, &str); 10] = [
    ("min_ac", true, guids::PROC_MIN),
    ("max_ac", true, guids::PROC_MAX),
    ("boost_ac", true, guids::BOOST_MODE),
    ("epp_ac", true, guids::EPP),
    ("park_ac", true, guids::PARK_MIN),
    ("min_dc", false, guids::PROC_MIN),
    ("max_dc", false, guids::PROC_MAX),
    ("boost_dc", false, guids::BOOST_MODE),
    ("epp_dc", false, guids::EPP),
    ("park_dc", false, guids::PARK_MIN),
];
pub const MIN_CHOICES: [u32; 3] = [5, 50, 100];
pub const MAX_CHOICES: [u32; 3] = [100, 99, 80];
pub const BOOST_CHOICES: [u32; 4] = [0, 1, 2, 3];
pub const EPP_CHOICES: [u32; 5] = [0, 25, 50, 80, 100];
pub const PARK_CHOICES: [u32; 3] = [10, 50, 100];

pub fn choices_of(setting: &str) -> &'static [u32] {
    match setting {
        guids::PROC_MIN => &MIN_CHOICES,
        guids::PROC_MAX => &MAX_CHOICES,
        guids::EPP => &EPP_CHOICES,
        guids::PARK_MIN => &PARK_CHOICES,
        _ => &BOOST_CHOICES,
    }
}

/// The index of a setting in `limits`.
pub fn limit_index(setting: &str) -> usize {
    match setting {
        guids::PROC_MIN => 0,
        guids::PROC_MAX => 1,
        guids::BOOST_MODE => 2,
        guids::EPP => 3,
        _ => 4,
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
            Some(Hit::Min | Hit::Max | Hit::Close | Hit::Inert | Hit::Toast) | None => Cursor::Arrow,
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
                if let Some(k) = self.drag_knob {
                    self.slide_to(k, p.x);
                    cx.repaint();
                    return;
                }
                if let Some((y0, s0)) = self.drag_scroll {
                    let k = self.content_h / self.view_h.max(1.0);
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
            Event::MouseDown { p, button: Button::Left, .. } => {
                let h = self.hits.at(p).cloned();
                if h == Some(Hit::Scrollbar) {
                    self.drag_scroll = Some((p.y, self.scroll()));
                    cx.capture(true);
                    return;
                }
                if let Some(Hit::Slider(k)) = h {
                    self.drag_knob = Some(k);
                    self.slide_to(k, p.x);
                    cx.capture(true);
                    cx.repaint();
                    return;
                }
                self.pressed = h;
                cx.capture(true);
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                if self.drag_scroll.take().is_some() || self.drag_knob.take().is_some() {
                    cx.repaint();
                    return;
                }
                let up = self.hits.at(p).cloned();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    self.act(a, cx);
                }
                cx.repaint();
            }
            Event::Wheel { dy, .. } => {
                if self.modal.is_none() {
                    let s = self.scroll();
                    self.set_scroll(s - dy * 56.0);
                    cx.repaint();
                }
            }
            Event::Key { vk, .. } => match vk {
                0x1b if self.modal.is_some() => {
                    self.act(Hit::ModalCancel, cx);
                    cx.handled();
                    cx.repaint();
                }
                0x0d if self.modal.is_some() => {
                    self.act(Hit::ModalOk, cx);
                    cx.handled();
                }
                _ => {}
            },
            Event::Timer(flows::KEEP_TIMER) => self.keep_tick(cx),
            Event::Timer(TOAST) => {
                if self.toast.as_ref().is_some_and(|(_, t)| t.elapsed().as_millis() >= 4400) {
                    self.toast = None;
                    cx.timer(TOAST, None);
                    cx.repaint();
                }
            }
            Event::SystemTheme => {
                if self.set.theme == "system" {
                    self.set_theme();
                    cx.repaint();
                }
            }
            Event::Opened(_) => self.win.bring_to_front(),
            Event::Resized | Event::Scale | Event::Active(_) => cx.repaint(),
            Event::Close => {
                self.live_ctl.stop.store(true, Ordering::Relaxed);
                self.stress_stop();
                if let Some(w) = self.monitor.take() {
                    w.close();
                }
                // a try still open when the window closes is not kept
                if self.trying.is_some() {
                    self.revert("window closed", cx);
                }
                self.set.save();
            }
            _ => {}
        }
    }

    pub fn act(&mut self, h: Hit, cx: &mut Cx) {
        match h {
            Hit::Min => self.win.minimize(),
            Hit::Max => self.win.toggle_maximize(),
            Hit::Close => self.win.close(),
            Hit::Nav(p) => self.go(p, cx),
            Hit::Profile(i) | Hit::StartProfile(i) => {
                if let Some(p) = Profile::ALL.get(i) {
                    self.choose_profile(*p, cx);
                    if self.page != Page::Tweaks {
                        self.go(Page::Tweaks, cx);
                    }
                }
            }
            Hit::Tick(i) => self.tick(i, cx),
            Hit::Expand(i) => {
                if let Some(t) = TWEAKS.get(i)
                    && !self.expanded.remove(t.id)
                {
                    self.expanded.insert(t.id);
                }
                cx.repaint();
            }
            Hit::Apply => self.ask_apply(cx),
            Hit::UndoAll => self.ask_undo_all(cx),
            Hit::UndoTweak(i) => self.undo_tweak(i, cx),
            Hit::Measure => self.measure(cx),
            Hit::Choice(key, i) if ["min_", "max_", "boost_", "epp_", "park_"].iter().any(|p| key.starts_with(p)) => {
                if let Some((_, ac, setting)) = LIMIT_ROWS.iter().find(|r| r.0 == key)
                    && let Some(v) = choices_of(setting).get(i)
                {
                    self.set_limit(setting, *ac, *v, cx);
                }
            }
            Hit::Choice(..) | Hit::Toggle(_) => self.settings_act(h, cx),
            Hit::ModalSwitch => {
                if let Some(Modal::Confirm { restore, .. }) = &mut self.modal {
                    *restore = !*restore;
                    self.set.restore_point = *restore;
                    self.set.save();
                }
                cx.repaint();
            }
            Hit::Card(i) => self.select_card(i, cx),
            Hit::KnobStep(k, d) => self.knob_step(k, d, cx),
            Hit::Slider(_) => {}
            Hit::FanMode(i) => self.fan_mode(i, cx),
            Hit::FanFixed(d) => self.fan_fixed_step(d, cx),
            Hit::FanPoint(i, t, d) => self.fan_point(i, t, d, cx),
            Hit::GpuTry => self.gpu_try(cx),
            Hit::GpuStock => self.gpu_stock(cx),
            Hit::GpuDiscard => {
                self.draft = self.gpu_now.clone();
                cx.repaint();
            }
            Hit::GpuTest => self.stress_start(cx),
            Hit::ProfileSave => self.profile_save(cx),
            Hit::ProfileLoad(i) => self.profile_load(i, cx),
            Hit::ProfileDelete(i) => self.profile_delete(i, cx),
            Hit::ProfileSignin(i) => {
                let name = self.profiles.list.get(i).map(|p| p.name.clone());
                let off = name.is_some() && self.profiles.signin == name && !self.set.gpu_signin_blocked;
                self.set_signin(if off { None } else { name }, cx);
            }
            Hit::Monitor => self.toggle_monitor(cx),
            Hit::CpuAttrib => {
                let val = if self.attribs_shown { 1 } else { 0 };
                if !self.admin {
                    self.modal = Some(Modal::Elevate(Why::Power));
                    cx.repaint();
                } else {
                    self.guarded(Pending::Attrib { setting: guids::EPP, val }, cx);
                }
            }
            Hit::SnapRestore(i) => self.ask_restore(i, cx),
            Hit::SnapOpen(i) => {
                if let Some((d, _)) = self.snaps_list.get(i) {
                    if !self.be.is_test() {
                        self.win.open(&d.display().to_string());
                    }
                    self.note(format!("open {}", d.display()));
                }
            }
            Hit::BackupInstall => self.backup_install(cx),
            Hit::BackupWithout => self.backup_without(cx),
            Hit::ModalOk => match self.modal.clone() {
                Some(Modal::Confirm { .. }) => self.confirm_ok(cx),
                Some(Modal::BackupMissing { .. }) => self.backup_install(cx),
                Some(Modal::Keep) => self.keep(cx),
                Some(Modal::Stress) => self.stress_stop(),
                Some(Modal::Restore { .. }) => self.restore_ok(cx),
                Some(Modal::Waiver) => self.accept_waiver(cx),
                Some(Modal::StressDone { .. }) | Some(Modal::GpuReverted { .. }) => {
                    self.modal = None;
                    cx.repaint();
                }
                Some(Modal::Elevate(_)) => self.elevate(cx),
                Some(Modal::RestoreFailed { ids, .. }) => {
                    self.modal = None;
                    self.start_apply(ids, false, cx);
                }
                Some(Modal::UndoAll) => self.undo_all_ok(cx),
                Some(Modal::Done { .. }) | None => {
                    self.modal = None;
                    cx.repaint();
                }
            },
            Hit::ModalCancel => match self.modal {
                Some(Modal::Keep) => self.revert("user", cx),
                Some(Modal::Stress) => self.stress_stop(),
                _ => {
                    self.modal = None;
                    cx.repaint();
                }
            },
            Hit::Toast => {
                self.toast = None;
                cx.repaint();
            }
            Hit::Scrollbar | Hit::Inert => {}
        }
    }
}
