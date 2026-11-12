//! The window of Plume Backup: the plans, the editor of a plan, the restore view and the settings. Backups run on
//! their own thread, one at a time, and report here through the window's sender; the schedules are checked every
//! half minute and whenever a drive is plugged in.

mod paint;

use crate::{
    engine::{self, Entry, Fail, Mode, Plan, Progress, Report, Schedule, Snapshot, Target},
    i18n::{self, fill, tr},
    settings::{Last, PlanState, Settings, Store, fail_code},
    sys,
};
use plume_ui::{Area, Button, Color, Cursor, Cx, Event, Gfx, Handler, Hits, Pt, Rect, Theme, Window, dialogs, m};
use std::{
    any::Any,
    cell::Cell,
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Emerald, the colour of the icon.
pub const ACCENT: u32 = 0x059669;

const TICK: usize = 1;
const SCHED: usize = 2;
/// After a failed attempt, a schedule waits this long before trying again.
const RETRY: u64 = 15 * 60;

/// Choices offered by the steppers of the editor (0: no limit / forever).
pub const KEEP: [u32; 10] = [3, 5, 7, 10, 14, 20, 30, 50, 100, 0];
pub const DAYS: [u32; 8] = [0, 7, 14, 30, 60, 90, 180, 365];
pub const DELETED_DAYS: [u32; 8] = [7, 14, 30, 60, 90, 180, 365, 0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Plans,
    Edit,
    Restore,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Dest,
    Excludes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Min,
    Max,
    Close,
    Nav(View),
    NewPlan,
    Run(usize),
    Stop(usize),
    Edit(usize),
    Open(usize),
    Field(Field),
    AddSource,
    RemoveSource(usize),
    ChooseDest,
    Sched(Schedule),
    HourDown,
    HourUp,
    SetMode(Mode),
    KeepDown,
    KeepUp,
    DaysDown,
    DaysUp,
    DeletedDown,
    DeletedUp,
    SkipTemp,
    Verify,
    Paused,
    Save,
    Cancel,
    Delete,
    DeleteYes,
    DeleteNo,
    RPlan(usize),
    Snap(usize),
    Row(usize),
    Crumb(usize),
    Up,
    RestoreHere,
    RestoreTo,
    Autostart,
    Background,
    NotifyOk,
    NotifyFail,
    Lang(u8),
    Theme(u8),
}

/// The plan being edited.
pub struct Draft {
    /// Its place in the list (None: a new plan).
    pub index: Option<usize>,
    pub plan: Plan,
    pub excludes: String,
}

pub struct Running {
    pub id: u64,
    pub stop: Arc<AtomicBool>,
    pub prog: Progress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browse {
    Idle,
    Loading,
    /// The destination is not reachable.
    Missing,
    Ready,
}

/// Scrollable zones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Plans,
    Form,
    Snaps,
    Rows,
    Settings,
}
const ZONES: usize = 5;

pub struct App {
    pub win: Window,
    pub gfx: Rc<Gfx>,
    pub theme: Theme,
    pub set: Settings,
    pub store: Store,
    pub view: View,
    pub hits: Hits<Id>,
    pub hover: Option<Id>,
    pressed: Option<Id>,
    // the editor
    pub draft: Option<Draft>,
    pub field: Option<Field>,
    edit_pending: bool,
    pub confirm_delete: bool,
    // backups
    pub running: Option<Running>,
    pub queue: Vec<u64>,
    /// Is each plan's destination reachable (last check)?
    pub present: HashMap<u64, bool>,
    checking: bool,
    // restore
    pub rplan: usize,
    pub browse: Browse,
    pub snaps: Vec<Snapshot>,
    pub snap: usize,
    pub path: Vec<String>,
    pub rows: Vec<Entry>,
    pub sel: Option<usize>,
    pub restoring: bool,
    // the rest
    pub scroll: [f32; ZONES],
    scroll_max: [f32; ZONES],
    areas: [Rect; ZONES],
    /// While a scrolled list is drawn: its visible rectangle (what is outside is not clickable).
    clip: Option<Rect>,
    pub toast: Option<(String, Instant)>,
    pub autostart: bool,
    /// Read by the window hook: closing the window hides it instead.
    pub keep: Rc<Cell<bool>>,
    pub quitting: bool,
    /// What happened, for the tests.
    pub log: Vec<String>,
    /// Command-line requests waiting for a plan's run to end: (plan id, answer id).
    pub cli_waits: Vec<(u64, String)>,
    tip: String,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

/// The theme: the setting, otherwise Windows'; the accent is the emerald of Plume Backup's icon.
pub fn theme(set: &Settings) -> Theme {
    let dark = match std::env::var("PLUME_SUITE_THEME").ok().as_deref().unwrap_or(set.theme.as_str()) {
        "dark" => true,
        "light" => false,
        _ => crate::system_dark(),
    };
    Theme::new(dark, Some(Color::hex(ACCENT)))
}

fn new_id() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)
}

/// The next value of a stepper's list (`up`: towards the end).
pub fn step(list: &[u32], cur: u32, up: bool) -> u32 {
    let i = list.iter().position(|&v| v == cur).unwrap_or(0);
    let j = if up { (i + 1).min(list.len() - 1) } else { i.saturating_sub(1) };
    list[j]
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, keep: Rc<Cell<bool>>) -> App {
        let set = Settings::load();
        i18n::set(&set.lang);
        let mut app = App {
            win,
            gfx,
            theme: theme(&set),
            store: Store::load(),
            set,
            view: View::Plans,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            draft: None,
            field: None,
            edit_pending: false,
            confirm_delete: false,
            running: None,
            queue: Vec::new(),
            present: HashMap::new(),
            checking: false,
            rplan: 0,
            browse: Browse::Idle,
            snaps: Vec::new(),
            snap: 0,
            path: Vec::new(),
            rows: Vec::new(),
            sel: None,
            restoring: false,
            scroll: [0.0; ZONES],
            scroll_max: [0.0; ZONES],
            areas: [Rect::default(); ZONES],
            clip: None,
            toast: None,
            autostart: sys::autostart(),
            keep,
            quitting: false,
            log: Vec::new(),
            cli_waits: Vec::new(),
            tip: String::new(),
        };
        app.update_keep();
        app
    }

    /// Starts the clock of the schedules (once the window exists).
    pub fn start(&mut self) {
        self.win.timer(SCHED, Some(30_000));
        self.check_schedules();
    }

    pub fn note(&mut self, what: String) {
        if self.log.len() > 300 {
            self.log.remove(0);
        }
        self.log.push(what);
    }

    fn say(&mut self, text: String, cx: &mut Cx) {
        self.toast = Some((text, Instant::now()));
        cx.timer(TICK, Some(500));
        cx.repaint();
    }

    pub fn update_keep(&mut self) {
        self.keep.set(!self.quitting && self.set.background);
    }

    /// Runs `work` on another thread, then `done` here with its result.
    fn bg<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(&mut App, T, &mut Cx) + Send + 'static) {
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let r = work();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    done(app, r, cx);
                    cx.repaint();
                }
            });
        });
    }

    // ───────────────────────── texts ─────────────────────────

    pub fn fail_text(f: &Fail) -> String {
        match f {
            Fail::Stopped => tr("Stopped. The next backup carries on from there.").into(),
            Fail::NoSources => tr("Add at least one folder to back up.").into(),
            Fail::NoDest => tr("Choose where to save the backup.").into(),
            Fail::DestMissing => tr("The destination is not available: plug in the drive or check the network share.").into(),
            Fail::SourceMissing(p) => fill(tr("A folder to back up is missing: {}. Nothing was changed."), &[&p.display().to_string()]),
            Fail::Overlap => tr("The backup can't be inside a folder it backs up.").into(),
            Fail::DestFull => tr("The destination is full.").into(),
            Fail::Io(e) => fill(tr("Error: {}"), &[e]),
        }
    }

    pub fn tray_text(&self) -> String {
        if let Some(r) = &self.running {
            let name = self.store.plan(r.id).map(|p| p.name.clone()).unwrap_or_default();
            return fill(tr("Plume Backup: backing up {}… {} %"), &[&name, &percent(&r.prog).to_string()]);
        }
        let last = self.store.states.values().map(|s| s.last_ok).max().unwrap_or(0);
        if last == 0 { "Plume Backup".into() } else { fill(tr("Plume Backup: last backup {}"), &[&i18n::when(last, engine::now())]) }
    }

    fn update_tip(&mut self) {
        let tip = self.tray_text();
        if tip != self.tip {
            sys::tray_tip(self.win.hwnd, &tip);
            self.tip = tip;
        }
    }

    // ───────────────────────── schedules and runs ─────────────────────────

    pub fn busy(&self, id: u64) -> bool {
        self.running.as_ref().is_some_and(|r| r.id == id) || self.queue.contains(&id)
    }

    /// Looks at which destinations are there (on another thread: a network share can take a while), then starts
    /// what is due.
    pub fn check_schedules(&mut self) {
        if self.checking {
            return;
        }
        self.checking = true;
        let plans: Vec<(u64, PathBuf, Option<u32>)> = self.store.plans.iter().map(|p| (p.id, p.dest.clone(), p.dest_serial)).collect();
        self.bg(
            move || plans.into_iter().map(|(id, d, s)| (id, sys::resolve(&d, s).is_some())).collect::<Vec<_>>(),
            |app, res, cx| {
                app.checking = false;
                app.on_presence(res, engine::now(), cx);
            },
        );
    }

    /// Is a plan due, given whether its destination is there and was there at the last check?
    pub fn due(&self, plan: &Plan, here: bool, was: Option<bool>, now: u64) -> bool {
        is_due(plan, &self.store.state(plan.id), self.busy(plan.id), here, was, now)
    }

    pub fn on_presence(&mut self, res: Vec<(u64, bool)>, now: u64, cx: &mut Cx) {
        for (id, here) in res {
            let was = self.present.insert(id, here);
            let Some(plan) = self.store.plan(id).cloned() else { continue };
            if self.due(&plan, here, was, now) {
                self.note(format!("due {}", plan.name));
                self.enqueue(id, cx);
            }
        }
        cx.repaint();
    }

    pub fn enqueue(&mut self, id: u64, cx: &mut Cx) {
        if !self.busy(id) {
            self.queue.push(id);
        }
        self.start_next(cx);
    }

    fn start_next(&mut self, cx: &mut Cx) {
        if self.running.is_some() {
            return;
        }
        while !self.queue.is_empty() {
            let id = self.queue.remove(0);
            let Some(plan) = self.store.plan(id).cloned() else { continue };
            let stop = Arc::new(AtomicBool::new(false));
            self.running = Some(Running { id, stop: stop.clone(), prog: Progress::default() });
            self.note(format!("start {}", plan.name));
            let sender = self.win.sender();
            std::thread::spawn(move || {
                let now = engine::now();
                let result = if plan.sources.is_empty() {
                    Err(Fail::NoSources)
                } else if plan.dest.as_os_str().is_empty() {
                    Err(Fail::NoDest)
                } else {
                    match sys::resolve(&plan.dest, plan.dest_serial) {
                        None => Err(Fail::DestMissing),
                        Some(dest) => {
                            let mut last = Instant::now() - Duration::from_secs(1);
                            let s2 = sender;
                            engine::run(&plan, &dest, &sys::stamp(now), now, &stop, &mut |p| {
                                if last.elapsed() >= Duration::from_millis(120) || p.done == p.total {
                                    last = Instant::now();
                                    let p = p.clone();
                                    s2.send(move |h, cx| {
                                        if let Some(app) = with_app(h) {
                                            app.on_progress(id, p, cx);
                                        }
                                    });
                                }
                            })
                        }
                    }
                };
                sender.send(move |h, cx| {
                    if let Some(app) = with_app(h) {
                        app.on_done(id, now, result, cx);
                    }
                });
            });
            cx.timer(TICK, Some(500));
            self.update_tip();
            cx.repaint();
            return;
        }
    }

    fn on_progress(&mut self, id: u64, p: Progress, cx: &mut Cx) {
        if let Some(r) = &mut self.running
            && r.id == id
        {
            r.prog = p;
            cx.repaint();
        }
    }

    pub fn on_done(&mut self, id: u64, now: u64, result: Result<Report, Fail>, cx: &mut Cx) {
        if self.running.as_ref().is_some_and(|r| r.id == id) {
            self.running = None;
        }
        let name = self.store.plan(id).map(|p| p.name.clone()).unwrap_or_default();
        let st = self.store.states.entry(id).or_default();
        st.last_try = now;
        match &result {
            Ok(r) => {
                st.last_ok = now;
                st.last = Some(Last { time: now, ok: true, report: r.clone(), fail: String::new() });
                self.note(format!(
                    "done {name} copied={} linked={} unchanged={} removed={} errors={}",
                    r.copied,
                    r.linked,
                    r.unchanged,
                    r.removed,
                    r.errors.len()
                ));
            }
            Err(f) => {
                st.last = Some(Last { time: now, ok: false, report: Report::default(), fail: fail_code(f) });
                self.note(format!("failed {name} {}", fail_code(f)));
            }
        }
        self.store.save();
        // a command line waiting for this plan (`--run-plan`) gets its answer
        let waits: Vec<String> = self.cli_waits.iter().filter(|w| w.0 == id).map(|w| w.1.clone()).collect();
        self.cli_waits.retain(|w| w.0 != id);
        let quiet = !waits.is_empty() && result.is_ok();
        for nonce in waits {
            let answer = match &result {
                Ok(r) => crate::cli::Answer { ok: true, message: crate::cli::report_text(r) },
                Err(f) => crate::cli::Answer { ok: false, message: crate::cli::fail_text(f) },
            };
            crate::cli::write_answer(&nonce, &answer);
        }
        // another app asked for it: a success needs no balloon (a failure still shows one)
        if !quiet {
            self.notify(&name, &result);
        }
        // the restore view shows the new backup
        if self.view == View::Restore && self.store.plans.get(self.rplan).is_some_and(|p| p.id == id) {
            self.load_snaps();
        }
        self.start_next(cx);
        self.update_tip();
        cx.repaint();
    }

    /// A request of `plume-backup.exe --ensure-plan` / `--run-plan` handed to this window, which owns plans.json.
    pub fn cli_request(&mut self, req: crate::cli::Cli, nonce: String, cx: &mut Cx) {
        use crate::cli::{Answer, Cli};
        match req {
            Cli::Ensure(e) => {
                let answer = match crate::cli::ensure(&mut self.store, &e, new_id()) {
                    Ok((_, changed)) => {
                        if changed {
                            self.store.save();
                        }
                        self.note(format!("cli ensure {} changed={changed}", e.name));
                        Answer { ok: true, message: if changed { "saved".into() } else { "up to date".into() } }
                    }
                    Err(m) => Answer { ok: false, message: m },
                };
                crate::cli::write_answer(&nonce, &answer);
                cx.repaint();
            }
            Cli::Run(name) => match crate::cli::runnable(&self.store, &name) {
                Ok(plan) => {
                    self.note(format!("cli run {}", plan.name));
                    self.cli_waits.push((plan.id, nonce));
                    // already running: it runs once more afterwards, so the answer covers files written just now
                    if !self.queue.contains(&plan.id) {
                        self.queue.push(plan.id);
                    }
                    self.start_next(cx);
                }
                Err(m) => crate::cli::write_answer(&nonce, &Answer { ok: false, message: m }),
            },
        }
    }

    fn notify(&mut self, name: &str, result: &Result<Report, Fail>) {
        let (title, text, error) = match result {
            Ok(r) if r.errors.is_empty() => {
                if !self.set.notify_ok {
                    return;
                }
                (tr("Backup done"), fill(tr("{}: {} copied, {} in all."), &[name, &i18n::count(r.copied), &i18n::files(r.files)]), false)
            }
            Ok(r) => {
                if !self.set.notify_fail {
                    return;
                }
                (tr("Backup done, with problems"), fill(tr("{}: {} files could not be backed up."), &[name, &i18n::count(r.errors.len() as u64)]), true)
            }
            Err(Fail::Stopped) => return,
            Err(f) => {
                if !self.set.notify_fail {
                    return;
                }
                (tr("Backup failed"), fill(tr("{}: {}"), &[name, &Self::fail_text(f)]), true)
            }
        };
        self.note(format!("notify {title}: {text}"));
        if !sys::testing() {
            sys::balloon(self.win.hwnd, title, &text, error);
        }
    }

    // ───────────────────────── the editor ─────────────────────────

    fn edit_plan(&mut self, index: Option<usize>) {
        let plan = match index.and_then(|i| self.store.plans.get(i)) {
            Some(p) => p.clone(),
            None => Plan { id: new_id(), name: fill(tr("Backup {}"), &[&(self.store.plans.len() + 1).to_string()]), ..Default::default() },
        };
        let excludes = plan.excludes.join(", ");
        self.draft = Some(Draft { index, plan, excludes });
        self.confirm_delete = false;
        self.scroll[Zone::Form as usize] = 0.0;
        self.view = View::Edit;
    }

    pub fn field_text(&self, f: Field) -> String {
        let Some(d) = &self.draft else { return String::new() };
        match f {
            Field::Name => d.plan.name.clone(),
            Field::Dest => d.plan.dest.display().to_string(),
            Field::Excludes => d.excludes.clone(),
        }
    }

    fn set_field(&mut self, f: Field, text: String) {
        let Some(d) = &mut self.draft else { return };
        match f {
            Field::Name => d.plan.name = text,
            Field::Dest => d.plan.dest = PathBuf::from(text.trim()),
            Field::Excludes => d.excludes = text,
        }
    }

    /// Closes the open field (its text is already in the draft).
    fn close_field(&mut self, cx: &mut Cx) {
        if self.field.take().is_some() {
            cx.after(|w| w.edit_hide());
        }
        cx.repaint();
    }

    pub fn add_source(&mut self, p: PathBuf) {
        if let Some(d) = &mut self.draft
            && !d.plan.sources.iter().any(|s| s == &p)
        {
            d.plan.sources.push(p);
        }
    }

    pub fn set_dest(&mut self, p: PathBuf) {
        if let Some(d) = &mut self.draft {
            d.plan.dest = p;
        }
    }

    fn save_draft(&mut self, cx: &mut Cx) {
        self.close_field(cx);
        let Some(d) = &self.draft else { return };
        let mut plan = d.plan.clone();
        plan.excludes = engine::parse_patterns(&d.excludes);
        plan.name = plan.name.trim().to_string();
        if plan.name.is_empty() {
            plan.name = tr("Backup").into();
        }
        if plan.sources.is_empty() {
            return self.say(tr("Add at least one folder to back up.").into(), cx);
        }
        if plan.dest.as_os_str().is_empty() {
            return self.say(tr("Choose where to save the backup.").into(), cx);
        }
        // the checks that don't need the drive to be there
        if let Err(f @ Fail::Overlap) = engine::check(&plan, &plan.dest) {
            return self.say(Self::fail_text(&f), cx);
        }
        if plan.folder.is_empty() {
            // two plans saving to the same place get their own folders
            let base = engine::safe_name(&plan.name);
            let mut folder = base.clone();
            let mut n = 2;
            while self.store.plans.iter().any(|p| p.id != plan.id && p.folder.eq_ignore_ascii_case(&folder)) {
                folder = format!("{base} ({n})");
                n += 1;
            }
            plan.folder = folder;
        }
        let old_dest = d.index.and_then(|i| self.store.plans.get(i)).map(|p| p.dest.clone());
        if old_dest.as_ref() != Some(&plan.dest) || plan.dest_serial.is_none() {
            plan.dest_serial = sys::serial_of(&plan.dest);
        }
        let name = plan.name.clone();
        match d.index {
            Some(i) if i < self.store.plans.len() => self.store.plans[i] = plan,
            _ => self.store.plans.push(plan),
        }
        self.store.save();
        self.note(format!("saved {name}"));
        self.draft = None;
        self.view = View::Plans;
        self.say(fill(tr("Plan “{}” saved"), &[&name]), cx);
        self.check_schedules();
    }

    fn delete_plan(&mut self, cx: &mut Cx) {
        let Some(i) = self.draft.as_ref().and_then(|d| d.index) else { return };
        if i >= self.store.plans.len() {
            return;
        }
        let plan = self.store.plans.remove(i);
        self.store.states.remove(&plan.id);
        self.queue.retain(|&q| q != plan.id);
        if let Some(r) = &self.running
            && r.id == plan.id
        {
            r.stop.store(true, Ordering::Relaxed);
        }
        self.store.save();
        self.note(format!("deleted {}", plan.name));
        self.draft = None;
        self.confirm_delete = false;
        self.view = View::Plans;
        self.rplan = 0;
        self.say(fill(tr("Plan “{}” deleted. Its backups are still on the destination."), &[&plan.name]), cx);
    }

    // ───────────────────────── restore ─────────────────────────

    pub fn load_snaps(&mut self) {
        let Some(plan) = self.store.plans.get(self.rplan).cloned() else {
            self.browse = Browse::Idle;
            return;
        };
        self.browse = Browse::Loading;
        let keep = self.snaps.get(self.snap).map(|s| s.path.clone());
        self.bg(
            move || sys::resolve(&plan.dest, plan.dest_serial).map(|d| engine::snapshots(&plan.root(&d))),
            move |app, res, _| match res {
                None => {
                    app.browse = Browse::Missing;
                    app.snaps.clear();
                    app.rows.clear();
                }
                Some(snaps) => {
                    app.browse = Browse::Ready;
                    let same = keep.as_ref().and_then(|k| snaps.iter().position(|s| &s.path == k));
                    app.snaps = snaps;
                    match same {
                        Some(i) => app.snap = i,
                        None => {
                            app.snap = 0;
                            app.path.clear();
                        }
                    }
                    app.list_dir();
                }
            },
        );
    }

    pub fn dir_now(&self) -> Option<PathBuf> {
        let s = self.snaps.get(self.snap)?;
        Some(self.path.iter().fold(s.path.clone(), |p, n| p.join(n)))
    }

    pub fn list_dir(&mut self) {
        self.sel = None;
        self.scroll[Zone::Rows as usize] = 0.0;
        let Some(dir) = self.dir_now() else {
            self.rows.clear();
            return;
        };
        self.bg(move || engine::list(&dir), |app, rows, _| app.rows = rows);
    }

    /// What the restore buttons act on: the selected row, otherwise the folder shown (not the top of a backup).
    pub fn restore_rel(&self) -> Option<PathBuf> {
        let mut rel: PathBuf = self.path.iter().collect();
        match self.sel.and_then(|i| self.rows.get(i)) {
            Some(e) => rel.push(&e.name),
            None if !self.path.is_empty() => {}
            None => return None,
        }
        Some(rel)
    }

    pub fn do_restore(&mut self, to: Target, cx: &mut Cx) {
        let (Some(rel), Some(snap)) = (self.restore_rel(), self.snaps.get(self.snap).cloned()) else {
            return self.say(tr("Choose a file or a folder first.").into(), cx);
        };
        if self.restoring {
            return;
        }
        self.restoring = true;
        self.note(format!("restore {} {:?}", rel.display(), to));
        let label = sys::stamp(snap.time);
        self.bg(
            move || engine::restore(&snap.path, &rel, &to, &label, &AtomicBool::new(false)),
            |app, res, cx| {
                app.restoring = false;
                let text = match res {
                    Ok(r) => {
                        app.note(format!("restored files={} same={} kept={} errors={}", r.files, r.same, r.kept, r.errors.len()));
                        let to = r.to.display().to_string();
                        let mut t = if r.files == 0 && r.same > 0 && r.errors.is_empty() {
                            tr("Already there and identical: nothing to restore.").to_string()
                        } else if r.files == 1 && r.same == 0 {
                            fill(tr("Restored: {}"), &[&to])
                        } else {
                            fill(tr("Restored {} to {}"), &[&i18n::files(r.files), &to])
                        };
                        if r.kept == 1 {
                            t.push_str(". ");
                            t.push_str(tr("The different file that was there is kept, with “before restore” in its name."));
                        } else if r.kept > 1 {
                            t.push_str(". ");
                            t.push_str(&fill(tr("{} different files that were there are kept, with “before restore” in their name."), &[&i18n::count(r.kept)]));
                        }
                        if !r.errors.is_empty() {
                            t.push_str(". ");
                            t.push_str(&fill(tr("{} could not be restored."), &[&i18n::count(r.errors.len() as u64)]));
                        }
                        t
                    }
                    Err(f) => {
                        app.note(format!("restore failed {}", fail_code(&f)));
                        Self::fail_text(&f)
                    }
                };
                app.say(text, cx);
            },
        );
    }

    fn open_row(&mut self, i: usize) {
        if let Some(e) = self.rows.get(i)
            && e.dir
        {
            self.path.push(e.name.clone());
            self.list_dir();
        }
    }

    // ───────────────────────── actions ─────────────────────────

    pub fn goto(&mut self, v: View, cx: &mut Cx) {
        self.close_field(cx);
        if v != View::Edit {
            self.draft = None;
            self.confirm_delete = false;
        }
        self.view = v;
        if v == View::Restore {
            if self.rplan >= self.store.plans.len() {
                self.rplan = 0;
            }
            self.load_snaps();
        }
        cx.repaint();
    }

    fn draft_mut(&mut self) -> Option<&mut Plan> {
        self.draft.as_mut().map(|d| &mut d.plan)
    }

    fn pick_folder(&self, title: &'static str, then: impl FnOnce(&mut App, PathBuf) + 'static, cx: &mut Cx) {
        cx.after(move |w| {
            if let Some(p) = dialogs::pick_folder(w.hwnd, title) {
                w.with(|h, cx| {
                    if let Some(app) = with_app(h) {
                        then(app, p);
                        cx.repaint();
                    }
                });
            }
        });
    }

    pub fn act(&mut self, id: Id, cx: &mut Cx) {
        match id {
            Id::Min => cx.after(|w| w.minimize()),
            Id::Max => cx.after(|w| w.toggle_maximize()),
            Id::Close => cx.after(|w| w.close()),
            Id::Nav(v) => self.goto(v, cx),
            Id::NewPlan => {
                self.close_field(cx);
                self.edit_plan(None);
            }
            Id::Run(i) => {
                if let Some(p) = self.store.plans.get(i) {
                    let id = p.id;
                    self.enqueue(id, cx);
                }
            }
            Id::Stop(i) => {
                if let Some(p) = self.store.plans.get(i) {
                    let id = p.id;
                    self.queue.retain(|&q| q != id);
                    if let Some(r) = &self.running
                        && r.id == id
                    {
                        r.stop.store(true, Ordering::Relaxed);
                        self.note("stop".into());
                    }
                }
            }
            Id::Edit(i) => {
                self.close_field(cx);
                self.edit_plan(Some(i));
            }
            Id::Open(i) => {
                self.rplan = i;
                self.snaps.clear();
                self.path.clear();
                self.goto(View::Restore, cx);
            }
            Id::Field(f) => {
                if self.field != Some(f) {
                    self.field = Some(f);
                    self.edit_pending = true;
                }
            }
            Id::AddSource => self.pick_folder(tr("Folder to back up"), |app, p| app.add_source(p), cx),
            Id::RemoveSource(k) => {
                if let Some(p) = self.draft_mut()
                    && k < p.sources.len()
                {
                    p.sources.remove(k);
                }
            }
            Id::ChooseDest => self.pick_folder(tr("Where to save the backup"), |app, p| app.set_dest(p), cx),
            Id::Sched(s) => {
                if let Some(p) = self.draft_mut() {
                    p.schedule = s;
                }
            }
            Id::HourDown | Id::HourUp => {
                if let Some(p) = self.draft_mut() {
                    p.hour = if id == Id::HourUp { (p.hour + 1) % 24 } else { (p.hour + 23) % 24 };
                }
            }
            Id::SetMode(md) => {
                if let Some(p) = self.draft_mut() {
                    p.mode = md;
                }
            }
            Id::KeepDown | Id::KeepUp => {
                if let Some(p) = self.draft_mut() {
                    p.keep_versions = step(&KEEP, p.keep_versions, id == Id::KeepUp);
                }
            }
            Id::DaysDown | Id::DaysUp => {
                if let Some(p) = self.draft_mut() {
                    p.keep_days = step(&DAYS, p.keep_days, id == Id::DaysUp);
                }
            }
            Id::DeletedDown | Id::DeletedUp => {
                if let Some(p) = self.draft_mut() {
                    p.deleted_days = step(&DELETED_DAYS, p.deleted_days, id == Id::DeletedUp);
                }
            }
            Id::SkipTemp => {
                if let Some(p) = self.draft_mut() {
                    p.skip_temp = !p.skip_temp;
                }
            }
            Id::Verify => {
                if let Some(p) = self.draft_mut() {
                    p.verify = !p.verify;
                }
            }
            Id::Paused => {
                if let Some(p) = self.draft_mut() {
                    p.paused = !p.paused;
                }
            }
            Id::Save => self.save_draft(cx),
            Id::Cancel => self.goto(View::Plans, cx),
            Id::Delete => {
                self.close_field(cx);
                self.confirm_delete = true;
            }
            Id::DeleteYes => self.delete_plan(cx),
            Id::DeleteNo => self.confirm_delete = false,
            Id::RPlan(i) => {
                if i != self.rplan {
                    self.rplan = i;
                    self.snaps.clear();
                    self.rows.clear();
                    self.path.clear();
                    self.load_snaps();
                }
            }
            Id::Snap(i) => {
                if i != self.snap {
                    self.snap = i;
                    self.path.clear();
                    self.list_dir();
                }
            }
            Id::Row(i) => self.sel = Some(i),
            Id::Crumb(k) => {
                self.path.truncate(k);
                self.list_dir();
            }
            Id::Up => {
                if self.path.pop().is_some() {
                    self.list_dir();
                }
            }
            Id::RestoreHere => self.do_restore(Target::Original, cx),
            Id::RestoreTo => {
                if self.restore_rel().is_none() {
                    return self.say(tr("Choose a file or a folder first.").into(), cx);
                }
                self.pick_folder(
                    tr("Restore to this folder"),
                    |app, p| {
                        // the dialog is closed: a fresh context for the restore
                        let w = app.win;
                        w.sender().send(move |h, cx| {
                            if let Some(app) = with_app(h) {
                                app.do_restore(Target::Folder(p), cx);
                            }
                        });
                    },
                    cx,
                );
            }
            Id::Autostart => {
                let on = !self.autostart;
                if sys::set_autostart(on) {
                    self.autostart = on;
                }
            }
            Id::Background => {
                self.set.background = !self.set.background;
                self.set.save();
                self.update_keep();
            }
            Id::NotifyOk => {
                self.set.notify_ok = !self.set.notify_ok;
                self.set.save();
            }
            Id::NotifyFail => {
                self.set.notify_fail = !self.set.notify_fail;
                self.set.save();
            }
            Id::Lang(k) => {
                self.set.lang = ["system", "en", "fr"][k.min(2) as usize].into();
                i18n::set(&self.set.lang);
                self.set.save();
                self.tip.clear();
                self.update_tip();
            }
            Id::Theme(k) => {
                self.set.theme = ["system", "dark", "light"][k.min(2) as usize].into();
                self.set.save();
                self.apply_theme();
            }
        }
        cx.repaint();
    }

    fn apply_theme(&mut self) {
        self.theme = theme(&self.set);
        self.win.set_dark(self.theme.dark);
    }

    /// The window goes away for good: a backup in progress stops (it carries on next time), the icon goes.
    pub fn shutdown(&mut self) {
        if let Some(r) = &self.running {
            r.stop.store(true, Ordering::Relaxed);
        }
        sys::tray_remove(self.win.hwnd);
    }

    fn zone_at(&self, p: Pt) -> Option<Zone> {
        [Zone::Plans, Zone::Form, Zone::Snaps, Zone::Rows, Zone::Settings].into_iter().find(|z| self.areas[*z as usize].contains(p))
    }
}

/// The rules of the schedules. `here`: the destination is reachable now; `was`: it was at the previous check.
pub fn is_due(plan: &Plan, st: &PlanState, busy: bool, here: bool, was: Option<bool>, now: u64) -> bool {
    if !here || plan.paused || busy {
        return false;
    }
    // after a failed (or stopped) attempt, wait a little before trying again
    if st.last_try > st.last_ok && now.saturating_sub(st.last_try) < RETRY {
        return false;
    }
    match plan.schedule {
        Schedule::Manual => false,
        Schedule::Hourly => now.saturating_sub(st.last_ok) >= 3600,
        Schedule::Daily => {
            let start = sys::day_start(now) + plan.hour as u64 * 3600;
            let boundary = if now >= start { start } else { start.saturating_sub(86_400) };
            st.last_ok < boundary
        }
        // plugged in now, or there all along and not backed up for half a day
        Schedule::Plug => was == Some(false) || now.saturating_sub(st.last_ok) >= 12 * 3600,
    }
}

/// Progress of a run, in percent (by bytes, by files when there are no bytes).
pub fn percent(p: &Progress) -> u32 {
    let (a, b) = if p.bytes_total > 0 { (p.bytes_done, p.bytes_total) } else { (p.done, p.total.max(1)) };
    ((a as f64 / b.max(1) as f64) * 100.0).round().clamp(0.0, 100.0) as u32
}

impl Handler for App {
    fn paint(&mut self, c: &mut plume_ui::Canvas, cx: &mut plume_ui::PaintCx) {
        self.paint_all(c, cx);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                let h = self.hits.at(p).copied();
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
                self.pressed = self.hits.at(p).copied();
                if clicks == 2
                    && let Some(Id::Row(i)) = self.pressed
                {
                    self.open_row(i);
                }
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                let up = self.hits.at(p).copied();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    // while the delete question is open, only its answer counts
                    if !self.confirm_delete || matches!(a, Id::DeleteYes | Id::DeleteNo | Id::Min | Id::Max | Id::Close) {
                        self.act(a, cx);
                    }
                }
                cx.repaint();
            }
            Event::Wheel { p, dy, .. } => {
                if let Some(z) = self.zone_at(p) {
                    let k = z as usize;
                    self.scroll[k] = (self.scroll[k] - dy * 60.0).clamp(0.0, self.scroll_max[k]);
                    if z == Zone::Form {
                        self.close_field(cx);
                    }
                    cx.repaint();
                }
            }
            Event::EditChanged(text) => {
                if let Some(f) = self.field {
                    self.set_field(f, text);
                    cx.repaint();
                }
            }
            Event::EditKey { vk: 0x0D | 0x1B | 0x09, .. } | Event::EditBlur => self.close_field(cx),
            Event::Key { vk: 0x1B, .. } => {
                if self.confirm_delete {
                    self.confirm_delete = false;
                } else if self.view == View::Edit {
                    self.goto(View::Plans, cx);
                }
                cx.repaint();
            }
            Event::Key { vk: 0x08, .. } if self.view == View::Restore => self.act(Id::Up, cx),
            Event::Key { vk: 0x0D, .. } if self.view == View::Restore => {
                if let Some(i) = self.sel {
                    self.open_row(i);
                    cx.repaint();
                }
            }
            Event::Timer(TICK) => {
                if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(5)) {
                    self.toast = None;
                }
                if self.toast.is_none() && self.running.is_none() {
                    cx.timer(TICK, None);
                }
                self.update_tip();
                cx.repaint();
            }
            Event::Timer(SCHED) | Event::Devices => {
                self.check_schedules();
                if self.view == View::Restore && self.browse == Browse::Missing {
                    self.load_snaps();
                }
            }
            Event::Opened(text) if text == "quit" => {
                self.quitting = true;
                self.update_keep();
                cx.after(|w| w.close());
            }
            Event::Opened(text) if crate::cli::read_message(&text).is_some() => {
                if let Some((req, nonce)) = crate::cli::read_message(&text) {
                    self.cli_request(req, nonce, cx);
                }
            }
            Event::Opened(text) if text == "run" => {
                let ids: Vec<u64> = self.store.plans.iter().filter(|p| !p.paused).map(|p| p.id).collect();
                for id in ids {
                    self.enqueue(id, cx);
                }
            }
            Event::Opened(_) => {
                sys::show(self.win.hwnd, true);
                self.win.bring_to_front();
                cx.repaint();
            }
            Event::SystemTheme => {
                if self.set.theme == "system" {
                    self.apply_theme();
                    cx.repaint();
                }
            }
            Event::Close => self.shutdown(),
            Event::Resized | Event::Scale | Event::Active(_) => cx.repaint(),
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Id::Max) => Area::MaxButton,
            Some(_) if p.y < m::TITLE_H => Area::Control,
            _ if p.y < m::TITLE_H => Area::Caption,
            _ => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Id::Min | Id::Max | Id::Close) | None => Cursor::Arrow,
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
    fn steppers() {
        assert_eq!(step(&KEEP, 30, true), 50);
        assert_eq!(step(&KEEP, 3, false), 3);
        assert_eq!(step(&KEEP, 100, true), 0);
        assert_eq!(step(&KEEP, 0, true), 0);
        assert_eq!(step(&DAYS, 0, true), 7);
        // a value not in the list starts from the beginning
        assert_eq!(step(&KEEP, 42, true), 5);
    }

    #[test]
    fn schedules() {
        let now = sys::day_start(engine::now()) + 15 * 3600; // 15:00 today
        let mut plan = Plan { schedule: Schedule::Hourly, ..Default::default() };
        let mut st = PlanState::default();
        assert!(is_due(&plan, &st, false, true, None, now), "never backed up");
        assert!(!is_due(&plan, &st, false, false, None, now), "destination missing");
        assert!(!is_due(&plan, &st, true, true, None, now), "already running");
        st.last_ok = now - 1800;
        st.last_try = now - 1800;
        assert!(!is_due(&plan, &st, false, true, None, now));
        st.last_ok = now - 3700;
        st.last_try = now - 3700;
        assert!(is_due(&plan, &st, false, true, None, now));
        // a failure five minutes ago: wait
        st.last_try = now - 300;
        assert!(!is_due(&plan, &st, false, true, None, now));
        st.last_try = now - 20 * 60;
        assert!(is_due(&plan, &st, false, true, None, now));
        // daily from 12:00: done at 11:00 today, due now; done at 12:30, not due
        plan.schedule = Schedule::Daily;
        plan.hour = 12;
        st.last_ok = now - 4 * 3600;
        st.last_try = st.last_ok;
        assert!(is_due(&plan, &st, false, true, None, now));
        st.last_ok = now - 2 * 3600 - 1800;
        st.last_try = st.last_ok;
        assert!(!is_due(&plan, &st, false, true, None, now));
        // from 18:00: yesterday's run at 19:00 still counts at 15:00
        plan.hour = 18;
        st.last_ok = now - 20 * 3600;
        st.last_try = st.last_ok;
        assert!(!is_due(&plan, &st, false, true, None, now));
        // plugged in: when the drive appears, not while it stays
        plan.schedule = Schedule::Plug;
        st.last_ok = now - 3600;
        st.last_try = st.last_ok;
        assert!(is_due(&plan, &st, false, true, Some(false), now));
        assert!(!is_due(&plan, &st, false, true, Some(true), now));
        plan.schedule = Schedule::Manual;
        assert!(!is_due(&plan, &st, false, true, Some(false), now));
        plan.schedule = Schedule::Hourly;
        plan.paused = true;
        st.last_ok = 0;
        assert!(!is_due(&plan, &st, false, true, None, now));
    }

    #[test]
    fn percents() {
        let p = Progress { bytes_done: 50, bytes_total: 200, ..Default::default() };
        assert_eq!(percent(&p), 25);
        let p = Progress { done: 3, total: 4, ..Default::default() };
        assert_eq!(percent(&p), 75);
        assert_eq!(percent(&Progress::default()), 0);
    }
}
