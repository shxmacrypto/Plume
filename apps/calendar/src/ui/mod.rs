//! The window of Plume Calendar: its state, what clicks, drags and keys do. Drawing is in `paint` (the frame, the
//! sidebar, the overlays), `views` (month, week, day, agenda), `editor` (the event card) and `settings_view`.

mod editor;
mod paint;
mod settings_view;
mod views;

pub use editor::Editor;

use crate::{
    date::{DAY, Date, Wall},
    i18n::{self, fill, tr},
    ics,
    model::{Event, Occ, Store, new_uid, provider_for},
    popup, quickadd,
    reminders::{self, Due},
    settings::Settings,
    sys,
    tz::{self, Zone},
};
use plume_ui::{Area, Button, Canvas, Color, Cursor, Cx, EditStyle, Event as UiEvent, Gfx, Handler, Hits, Mods, PaintCx, Pt, Rect, Theme, Window, dialogs, m};
use std::{any::Any, cell::Cell, rc::Rc, time::Instant};

/// Timers: the clock (reminders, the now line, subscriptions), the toast.
pub const TICK: usize = 1;
pub const TOAST: usize = 2;
/// How long subscriptions are kept before downloading again.
pub const REFRESH_EVERY: i64 = 3600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Day,
    Week,
    Month,
    Agenda,
}

impl View {
    pub fn key(self) -> &'static str {
        match self {
            View::Day => "day",
            View::Week => "week",
            View::Month => "month",
            View::Agenda => "agenda",
        }
    }
    pub fn from_key(k: &str) -> View {
        match k {
            "day" => View::Day,
            "week" => View::Week,
            "agenda" => View::Agenda,
            _ => View::Month,
        }
    }
}

/// Text fields (the window has one native edit field at a time).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Quick,
    Title,
    Location,
    Notes,
    StartDate,
    StartTime,
    EndDate,
    EndTime,
    Interval,
    Count,
    Until,
    Dialog,
}

/// Everything that can be clicked.
#[derive(Clone, Debug, PartialEq)]
pub enum Id {
    Min,
    Max,
    Close,
    Today,
    Prev,
    Next,
    Tab(View),
    Settings,
    NewEvent,
    Field(Field),
    MiniPrev,
    MiniNext,
    /// A day of the mini month (day number).
    Mini(i64),
    CalToggle(usize),
    CalMore(usize),
    CalAdd,
    /// A day cell of the month grid (day number).
    Cell(i64),
    /// "N more" in a month cell.
    More(i64),
    /// A day's head in the week view (goes to the day).
    DayHead(i64),
    /// The all-day row of a day.
    AllDay(i64),
    /// The hour grid of a day.
    Grid(i64),
    /// An occurrence on screen (index in `occs`), its bottom edge (resize).
    Occ(usize),
    Resize(usize),
    AgendaRow(usize),
    PeekEdit,
    PeekDelete,
    PeekClose,
    /// Editor buttons.
    EdAllDay,
    EdFreq(u8),
    EdDay(u32),
    EdMonthBy(bool),
    EdEnds(u8),
    EdCal,
    EdReminder,
    EdColor(Option<usize>),
    EdStep(Field, i32),
    EdSave,
    EdCancel,
    EdDelete,
    ScopeThis,
    ScopeAll,
    ScopeCancel,
    DlgOk,
    DlgCancel,
    ConfirmOk,
    ConfirmCancel,
    MenuItem(usize),
    MenuColor(usize),
    Undo,
    AlertSnooze(usize),
    AlertDismiss(usize),
    AlertOpen(usize),
    /// A setting's choice: (key, value).
    Set(&'static str, &'static str),
    SetBack,
    OpenData,
    /// Behind a modal (clicking it closes light overlays).
    Scrim,
    Scroll,
}

/// An occurrence remembered across recomputations.
#[derive(Clone, Debug, PartialEq)]
pub struct OccRef {
    pub cal: usize,
    pub ev: usize,
    pub rid: Option<i64>,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
}

impl OccRef {
    pub fn of(o: &Occ) -> OccRef {
        OccRef { cal: o.cal, ev: o.ev, rid: o.rid, start: o.start, end: o.end, all_day: o.all_day }
    }
}

#[derive(Clone, Debug)]
pub enum Drag {
    None,
    /// Pressed on an occurrence; becomes a move when the pointer travels.
    Press {
        occ: usize,
        p0: Pt,
        at0: i64,
    },
    /// New timed event (instants, the second follows the pointer).
    Create {
        a: i64,
        b: i64,
    },
    /// New all-day event (day numbers).
    CreateDays {
        a: i64,
        b: i64,
    },
    Move {
        occ: usize,
        grab: i64,
        start: i64,
    },
    MoveDays {
        occ: usize,
        grab: i64,
        day: i64,
    },
    Resize {
        occ: usize,
        end: i64,
    },
}

/// Layout of the last image, for turning the pointer into a moment.
#[derive(Clone, Debug, Default)]
pub struct Geo {
    /// Week / day grid: the visible grid, the first column's x, a column's width, the days, the hour's height.
    pub grid: Rect,
    pub col_x: f32,
    pub col_w: f32,
    pub days: Vec<Date>,
    pub hour_h: f32,
    pub allday: Rect,
    /// Month: the cells' area, a cell's size, the first day shown.
    pub month: Rect,
    pub cell_w: f32,
    pub cell_h: f32,
    pub month_first: Option<Date>,
    pub agenda: Rect,
}

pub struct Peek {
    pub occ: OccRef,
    pub at: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DlgKind {
    NewCalendar,
    Subscribe,
    Rename(usize),
}

pub struct Dialog {
    pub kind: DlgKind,
    pub text: String,
    pub error: Option<String>,
}

pub enum MenuAct {
    NewCalendar,
    Subscribe,
    Import,
    Rename(usize),
    Export(usize),
    Refresh(usize),
    Remove(usize),
}

pub struct Menu {
    pub items: Vec<(String, MenuAct)>,
    pub at: Pt,
    /// The calendar whose colors are offered.
    pub colors_for: Option<usize>,
}

/// What waits for "this event / all events".
pub enum ScopeAct {
    Save(Box<Event>),
    Move { start: i64, end: i64 },
    Delete,
}

pub struct Scope {
    pub occ: OccRef,
    pub act: ScopeAct,
    pub delete: bool,
}

pub enum ConfirmAct {
    RemoveCalendar(usize),
}

pub struct Confirm {
    pub text: String,
    pub act: ConfirmAct,
}

pub struct Toast {
    pub text: String,
    pub since: Instant,
    pub undo: bool,
}

pub struct App {
    pub win: Window,
    pub gfx: Rc<Gfx>,
    pub t: Theme,
    pub set: Settings,
    pub store: Store,
    pub view: View,
    pub settings_open: bool,
    /// The date the view shows, the selected day, the mini month's month.
    pub anchor: Date,
    pub selected: Date,
    pub mini: Date,
    pub hits: Hits<Id>,
    pub hover: Option<Id>,
    pub pressed: Option<Id>,
    pub occs: Vec<Occ>,
    occ_key: (i64, i64, u64),
    pub scroll: f32,
    pub scroll_to: Option<f32>,
    pub agenda_scroll: f32,
    pub agenda_h: f32,
    pub settings_scroll: f32,
    pub geo: Geo,
    pub drag: Drag,
    pub pointer: Pt,
    pub peek: Option<Peek>,
    pub editor: Option<Editor>,
    pub dialog: Option<Dialog>,
    pub menu: Option<Menu>,
    pub scope: Option<Scope>,
    pub confirm: Option<Confirm>,
    pub toast: Option<Toast>,
    /// Reminders shown in the window.
    pub alerts: Vec<Due>,
    pub snoozed: Vec<Due>,
    pub last_check: i64,
    /// Tests: the clock is moved.
    pub now_override: Option<i64>,
    /// Quick-add text.
    pub quick: String,
    /// The text field that should be open, and the one that is (with where it is drawn).
    pub edit_for: Option<Field>,
    pub edit_shown: Option<Field>,
    pub edit_rect: Option<Rect>,
    pub refreshing: Vec<String>,
    pub log: Vec<String>,
    pub autostart: bool,
    /// Tests: reminders only in the window and in the log, never on the screen outside it.
    pub quiet: bool,
    /// Read by the window hook: closing the window hides it (Plume Calendar stays near the clock).
    pub keep: Rc<Cell<bool>>,
    pub quitting: bool,
    pub popup: Option<Window>,
    pub undo: Option<(usize, Vec<Event>)>,
    pub tray: bool,
    /// Tests: the window never takes the keyboard (the native field would bring it to the front).
    pub no_focus: bool,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

pub fn theme_for(set: &Settings) -> Theme {
    let dark = match set.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => crate::system_dark(),
    };
    Theme::new(dark, Color::parse(if dark { "#818cf8" } else { "#4f46e5" }))
}

/// A color written "#rrggbb".
pub fn color(s: &str) -> Color {
    Color::parse(s).unwrap_or(Color::hex(0x818cf8))
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, set: Settings, keep: Rc<Cell<bool>>, quiet: bool) -> App {
        let store = Store::load();
        let t = theme_for(&set);
        let now = tz::now();
        let today = tz::date_at(&Zone::local(), now);
        let last = reminders::load_last().map(|l| l.max(now - 15 * 60)).unwrap_or(now).min(now);
        let view = View::from_key(&set.view);
        let mut app = App {
            win,
            gfx,
            t,
            store,
            view,
            settings_open: false,
            anchor: today,
            selected: today,
            mini: today.first_of_month(),
            hits: Hits::default(),
            hover: None,
            pressed: None,
            occs: Vec::new(),
            occ_key: (0, 0, u64::MAX),
            scroll: 0.0,
            scroll_to: None,
            agenda_scroll: 0.0,
            agenda_h: 0.0,
            settings_scroll: 0.0,
            geo: Geo::default(),
            drag: Drag::None,
            pointer: Pt::default(),
            peek: None,
            editor: None,
            dialog: None,
            menu: None,
            scope: None,
            confirm: None,
            toast: None,
            alerts: Vec::new(),
            snoozed: Vec::new(),
            last_check: last,
            now_override: None,
            quick: String::new(),
            edit_for: None,
            edit_shown: None,
            edit_rect: None,
            refreshing: Vec::new(),
            log: Vec::new(),
            autostart: sys::autostart(),
            quiet,
            keep,
            quitting: false,
            popup: None,
            undo: None,
            tray: false,
            no_focus: std::env::var_os("PLUME_CALENDAR_NOACTIVATE").is_some(),
            set,
        };
        app.scroll_to_hour(app.set.day_start as f32);
        app.update_keep();
        app
    }

    // ───────────────────────── basics ─────────────────────────

    pub fn now(&self) -> i64 {
        self.now_override.unwrap_or_else(tz::now)
    }

    pub fn zone(&self) -> Zone {
        Zone::local()
    }

    pub fn today(&self) -> Date {
        tz::date_at(&self.zone(), self.now())
    }

    pub fn h24(&self) -> bool {
        self.set.h24()
    }

    pub fn note(&mut self, what: impl Into<String>) {
        if self.log.len() > 300 {
            self.log.remove(0);
        }
        self.log.push(what.into());
    }

    pub fn say(&mut self, text: impl Into<String>, undo: bool, cx: &mut Cx) {
        let text = text.into();
        self.note(format!("toast: {text}"));
        self.toast = Some(Toast { text, since: Instant::now(), undo });
        cx.timer(TOAST, Some(500));
        cx.repaint();
    }

    pub fn update_keep(&mut self) {
        self.keep.set(self.set.background && !self.quitting);
    }

    fn scroll_to_hour(&mut self, h: f32) {
        self.scroll_to = Some(h * views::HOUR_H);
    }

    /// First and last+1 day the view covers.
    pub fn range(&self) -> (Date, Date) {
        let ws = self.set.week_start();
        match self.view {
            View::Month => {
                let first = self.anchor.first_of_month().week_start(ws);
                (first, first.add_days(42))
            }
            View::Week => {
                let first = self.anchor.week_start(ws);
                (first, first.add_days(7))
            }
            View::Day => (self.anchor, self.anchor.add_days(1)),
            View::Agenda => (self.anchor, self.anchor.add_days(views::AGENDA_DAYS)),
        }
    }

    /// Recomputes the occurrences on screen when the range or the data changed.
    pub fn refresh_occs(&mut self) {
        let (a, b) = self.range();
        let z = self.zone();
        let (from, to) = (tz::day_start(&z, a), tz::day_start(&z, b));
        let key = (from, to, self.store.version);
        if key != self.occ_key {
            self.occs = self.store.occurrences(from, to);
            self.occ_key = key;
        }
    }

    pub fn go(&mut self, view: View, date: Date) {
        self.view = view;
        self.anchor = date;
        self.selected = date;
        self.mini = date.first_of_month();
        self.settings_open = false;
        self.peek = None;
        if self.set.view != view.key() {
            self.set.view = view.key().into();
            self.set.save();
        }
    }

    fn step(&mut self, dir: i64) {
        let d = match self.view {
            View::Month => self.anchor.first_of_month().add_months(dir as i32),
            View::Week => self.anchor.add_days(7 * dir),
            View::Day => self.anchor.add_days(dir),
            View::Agenda => self.anchor.add_days(30 * dir),
        };
        self.anchor = d;
        self.selected = if self.view == View::Month && d.first_of_month() == self.today().first_of_month() { self.today() } else { d };
        self.mini = d.first_of_month();
        self.peek = None;
    }

    pub fn close_overlays(&mut self) {
        self.peek = None;
        self.menu = None;
    }

    // ───────────────────────── text fields ─────────────────────────

    /// Opens a text field (closing and keeping the one that was open).
    pub fn open_field(&mut self, f: Field, cx: &mut Cx) {
        if self.edit_for == Some(f) {
            return;
        }
        self.commit_field();
        self.edit_for = Some(f);
        cx.repaint();
    }

    pub fn close_field(&mut self, cx: &mut Cx) {
        self.commit_field();
        self.edit_for = None;
        cx.repaint();
    }

    /// The text a field starts with.
    fn field_text(&self, f: Field) -> String {
        match f {
            Field::Quick => self.quick.clone(),
            Field::Dialog => self.dialog.as_ref().map(|d| d.text.clone()).unwrap_or_default(),
            _ => self.editor.as_ref().map(|e| e.text_of(f, self.h24())).unwrap_or_default(),
        }
    }

    fn field_changed(&mut self, text: String) {
        match self.edit_for {
            Some(Field::Quick) => self.quick = text,
            Some(Field::Dialog) => {
                if let Some(d) = self.dialog.as_mut() {
                    d.text = text;
                    d.error = None;
                }
            }
            Some(f) => {
                if let Some(e) = self.editor.as_mut() {
                    e.set_text(f, text);
                }
            }
            None => {}
        }
    }

    /// A date or time field is read when it is left.
    fn commit_field(&mut self) {
        if let (Some(f), Some(e)) = (self.edit_for, self.editor.as_mut()) {
            let today = tz::date_at(&Zone::local(), self.now_override.unwrap_or_else(tz::now));
            e.commit(f, today);
        }
    }

    /// After painting: the native field follows what the image wants.
    pub fn sync_edit(&mut self) {
        let mut want = self.edit_for.filter(|_| self.edit_rect.is_some());
        if self.no_focus && !self.win.is_active() {
            // the text is drawn by the window and typed by the test socket
            want = None;
        }
        if want != self.edit_shown {
            match (want, self.edit_rect) {
                (Some(f), Some(r)) => {
                    let text = self.field_text(f);
                    let style = EditStyle {
                        fg: self.t.t1,
                        bg: paint::field_bg(&self.t, f),
                        size: 13.0,
                        select: Some((0, text.encode_utf16().count())),
                        placeholder: match f {
                            Field::Quick => tr("Add an event: “Lunch with Max tomorrow 12:30”").into(),
                            Field::Title => tr("Title").into(),
                            Field::Location => tr("Location").into(),
                            Field::Notes => tr("Notes").into(),
                            Field::Dialog => match self.dialog.as_ref().map(|d| d.kind) {
                                Some(DlgKind::Subscribe) => "https://… .ics".into(),
                                _ => tr("Calendar name").into(),
                            },
                            _ => String::new(),
                        },
                        password: false,
                        multiline: false,
                    };
                    self.win.edit_show(r, &text, &style);
                    self.win.edit_focus();
                }
                _ => self.win.edit_hide(),
            }
            self.edit_shown = want;
        } else if let (Some(_), Some(r)) = (want, self.edit_rect) {
            self.win.edit_place(r);
        }
    }

    // ───────────────────────── events: create, change, delete ─────────────────────────

    /// The editor for a new event.
    pub fn new_event(&mut self, start: Wall, end: Wall, all_day: bool, cx: &mut Cx) {
        let Some(cal) = self.store.default_cal() else {
            self.say(tr("No calendar you can write to"), false, cx);
            return;
        };
        let rem = if self.set.default_reminder >= 0 { self.set.default_reminder * 60 } else { -1 };
        self.close_overlays();
        self.editor = Some(Editor::new_event(cal, start, end, all_day, rem));
        self.edit_for = None;
        self.open_field(Field::Title, cx);
        cx.repaint();
    }

    /// The editor for an occurrence on screen.
    pub fn edit_occ(&mut self, r: OccRef, cx: &mut Cx) {
        let ev = self.store.events[r.cal][r.ev].clone();
        let master = if ev.recurrence_id.is_some() { self.store.events[r.cal].iter().position(|e| e.uid == ev.uid && e.recurrence_id.is_none()) } else { None };
        let rule = master.and_then(|m| self.store.events[r.cal][m].rrule.clone()).or(ev.rrule.clone());
        if self.store.cals[r.cal].read_only() {
            // nothing to change: the details are enough
            let at = self.peek.as_ref().map(|p| p.at).unwrap_or_default();
            self.menu = None;
            self.peek = Some(Peek { occ: r, at });
            cx.repaint();
            return;
        }
        self.close_overlays();
        self.editor = Some(Editor::from_event(&ev, &r, rule));
        self.edit_for = None;
        cx.repaint();
    }

    /// The event the editor builds is saved (asking "this one or all" for a series).
    pub fn save_editor(&mut self, cx: &mut Cx) {
        self.commit_field();
        let Some(ed) = self.editor.as_mut() else { return };
        if let Some(err) = ed.error.clone() {
            self.say(err, false, cx);
            return;
        }
        let ev = ed.build(&self.store);
        match ed.target.clone() {
            None => {
                let cal = ed.cal;
                let title = ev.title.clone();
                let date = tz::date_at(&self.zone(), if ev.all_day { tz::day_start(&self.zone(), tz::floating_date(ev.start)) } else { ev.start });
                self.store.merge(cal, vec![ev]);
                self.note(format!("added: {title}"));
                self.editor = None;
                self.edit_for = None;
                self.selected = date;
                self.say(fill(tr("Event added: {}"), &[&title]), false, cx);
            }
            Some(r) => {
                let series = self.store.events[r.cal][r.ev].rrule.is_some() || self.store.events[r.cal][r.ev].recurrence_id.is_some();
                if series && r.rid.is_some() {
                    self.scope = Some(Scope { occ: r, act: ScopeAct::Save(Box::new(ev)), delete: false });
                    self.edit_for = None;
                } else {
                    let cal = ed.cal;
                    self.replace_event(&r, ev, cal);
                    self.editor = None;
                    self.edit_for = None;
                }
            }
        }
        cx.repaint();
    }

    /// Replaces a single event (it may change calendar).
    fn replace_event(&mut self, r: &OccRef, mut ev: Event, cal: usize) {
        let old = self.store.events[r.cal][r.ev].clone();
        ev.uid = old.uid.clone();
        ev.recurrence_id = old.recurrence_id;
        ev.exdates = if ev.rrule.is_some() { old.exdates.clone() } else { Vec::new() };
        if cal != r.cal && !self.store.cals[cal].read_only() {
            self.store.delete_event(r.cal, r.ev);
            self.store.merge(cal, vec![ev]);
        } else {
            self.store.events[r.cal][r.ev] = ev;
            self.store.save_cal(r.cal);
            self.store.touch();
        }
        self.note(format!("saved: {}", old.title));
    }

    /// "This event" or "all events" was chosen.
    pub fn apply_scope(&mut self, all: bool, cx: &mut Cx) {
        let Some(s) = self.scope.take() else { return };
        let r = s.occ;
        let rid = r.rid.unwrap_or(r.start);
        // the series' own event
        let uid = self.store.events[r.cal][r.ev].uid.clone();
        let master = self.store.events[r.cal].iter().position(|e| e.uid == uid && e.recurrence_id.is_none()).unwrap_or(r.ev);
        let occ_start = self.store_start_of(&r);
        match s.act {
            ScopeAct::Delete => {
                self.undo = Some((r.cal, self.store.events[r.cal].clone()));
                if all {
                    self.store.delete_event(r.cal, master);
                } else {
                    self.store.skip_occurrence(r.cal, r.ev, rid);
                }
                self.note(format!("deleted ({})", if all { "all" } else { "one" }));
                self.say(tr("Event deleted"), true, cx);
            }
            ScopeAct::Move { start, end } => {
                if all {
                    self.store.shift_series(r.cal, master, start - occ_start);
                } else {
                    let base = self.store.events[r.cal][r.ev].clone();
                    let (s0, e0) = self.to_store(start, end, base.all_day);
                    self.store.change_occurrence(r.cal, master, rid, Event { start: s0, end: e0, ..base });
                }
                self.note(format!("moved ({})", if all { "all" } else { "one" }));
            }
            ScopeAct::Save(ev) => {
                let mut ev = *ev;
                if all {
                    let m = self.store.events[r.cal][master].clone();
                    let delta = ev.start - occ_start;
                    let dur = ev.end - ev.start;
                    ev.uid = m.uid.clone();
                    ev.start = m.start + delta;
                    ev.end = ev.start + dur;
                    ev.exdates = m.exdates.iter().map(|x| x + delta).collect();
                    ev.recurrence_id = None;
                    if ev.rrule.is_none() {
                        // no more repeating: the changed occurrences go
                        self.store.events[r.cal].retain(|e| e.uid != m.uid || e.recurrence_id.is_none());
                    }
                    let master = self.store.events[r.cal].iter().position(|e| e.uid == m.uid && e.recurrence_id.is_none()).unwrap_or(0);
                    self.store.events[r.cal][master] = ev;
                    self.store.save_cal(r.cal);
                    self.store.touch();
                } else {
                    self.store.change_occurrence(r.cal, master, rid, ev);
                }
                self.editor = None;
                self.note(format!("saved ({})", if all { "all" } else { "one" }));
            }
        }
        self.peek = None;
        cx.repaint();
    }

    /// The start, in the store's terms (floating for all-day), of an occurrence on screen.
    fn store_start_of(&self, r: &OccRef) -> i64 {
        if r.all_day { tz::date_at(&self.zone(), r.start).days() * DAY } else { r.start }
    }

    /// Instants on screen to the store's (all-day: floating midnights).
    fn to_store(&self, start: i64, end: i64, all_day: bool) -> (i64, i64) {
        if all_day {
            let z = self.zone();
            let a = tz::date_at(&z, start).days() * DAY;
            let b = tz::date_at(&z, end - 1).days() * DAY + DAY;
            (a, b.max(a + DAY))
        } else {
            (start, end)
        }
    }

    pub fn delete_occ(&mut self, r: OccRef, cx: &mut Cx) {
        if self.store.cals[r.cal].read_only() {
            self.say(tr("This calendar is read-only"), false, cx);
            return;
        }
        let e = &self.store.events[r.cal][r.ev];
        if (e.rrule.is_some() || e.recurrence_id.is_some()) && r.rid.is_some() {
            self.scope = Some(Scope { occ: r, act: ScopeAct::Delete, delete: true });
        } else {
            self.undo = Some((r.cal, self.store.events[r.cal].clone()));
            let title = e.title.clone();
            self.store.delete_event(r.cal, r.ev);
            self.note(format!("deleted: {title}"));
            self.say(tr("Event deleted"), true, cx);
        }
        self.peek = None;
        self.editor = None;
        self.edit_for = None;
        cx.repaint();
    }

    /// A drag ended on new times.
    fn move_occ(&mut self, i: usize, start: i64, end: i64, cx: &mut Cx) {
        let Some(o) = self.occs.get(i).cloned() else { return };
        if o.start == start && o.end == end {
            return;
        }
        let r = OccRef::of(&o);
        if self.store.cals[o.cal].read_only() {
            self.say(tr("This calendar is read-only"), false, cx);
            return;
        }
        let e = self.store.event(&o).clone();
        if o.rid.is_some() && (e.rrule.is_some() || e.recurrence_id.is_some()) {
            if e.recurrence_id.is_some() {
                // already a single changed occurrence: it just moves
                let (s0, e0) = self.to_store(start, end, e.all_day);
                self.store.events[o.cal][o.ev].start = s0;
                self.store.events[o.cal][o.ev].end = e0;
                self.store.save_cal(o.cal);
                self.store.touch();
            } else {
                self.scope = Some(Scope { occ: r, act: ScopeAct::Move { start, end }, delete: false });
            }
        } else {
            let (s0, e0) = self.to_store(start, end, e.all_day);
            let ev = &mut self.store.events[o.cal][o.ev];
            ev.start = s0;
            ev.end = e0;
            ev.stamp = tz::now();
            self.store.save_cal(o.cal);
            self.store.touch();
            self.note(format!("moved: {}", e.title));
        }
        cx.repaint();
    }

    /// The quick-add box: Enter.
    pub fn quick_add(&mut self, text: &str, cx: &mut Cx) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        let Some(cal) = self.store.default_cal() else {
            self.say(tr("No calendar you can write to"), false, cx);
            return false;
        };
        let base = self.today();
        let q = quickadd::parse(text, base, i18n::french());
        let z = self.zone();
        let (start, end, all_day) = match (q.start, q.end) {
            (Some(s), e) => {
                let s0 = z.to_utc(Wall { date: q.date, secs: s });
                let e = e.unwrap_or(s + 3600);
                let e0 = z.to_utc(Wall::from_naive(q.date.days() * DAY + e as i64));
                (s0, e0, false)
            }
            _ => (q.date.days() * DAY, q.date.days() * DAY + DAY, true),
        };
        let title = if q.title.is_empty() { tr("Untitled").to_string() } else { q.title.clone() };
        let ev = Event {
            uid: new_uid(),
            title: title.clone(),
            location: q.location.clone(),
            all_day,
            start,
            end,
            tzid: if all_day { String::new() } else { z.tzid() },
            alarms: if self.set.default_reminder >= 0 && !all_day { vec![self.set.default_reminder * 60] } else { vec![] },
            stamp: tz::now(),
            ..Default::default()
        };
        self.store.merge(cal, vec![ev]);
        self.note(format!("quick: {title} on {}", q.date.iso()));
        // show it
        if q.date < self.range().0 || q.date >= self.range().1 {
            let v = self.view;
            self.go(v, q.date);
        }
        self.selected = q.date;
        self.mini = q.date.first_of_month();
        if let Some(s) = q.start
            && matches!(self.view, View::Week | View::Day)
        {
            self.scroll_to_hour((s as f32 / 3600.0 - 1.0).max(0.0));
        }
        self.say(fill(tr("Event added: {}"), &[&title]), false, cx);
        true
    }

    // ───────────────────────── calendars ─────────────────────────

    pub fn subscribe(&mut self, url: &str, cx: &mut Cx) {
        let url = url.trim().to_string();
        if url.is_empty() {
            return;
        }
        let host = crate::model::normalize_url(&url).split('/').nth(2).unwrap_or("").to_string();
        let i = self.store.add_calendar(if host.is_empty() { tr("Subscriptions") } else { &host }, Some(url));
        self.refresh_cal(i, true, cx);
    }

    /// Downloads a subscription again (in the background).
    pub fn refresh_cal(&mut self, i: usize, first: bool, cx: &mut Cx) {
        let Some(cal) = self.store.cals.get(i) else { return };
        let Some(p) = provider_for(cal) else { return };
        let id = cal.id.clone();
        if self.refreshing.contains(&id) {
            return;
        }
        self.refreshing.push(id.clone());
        self.note(format!("refresh ({}): {}", p.kind(), cal.url.clone().unwrap_or_default()));
        let sender = cx.sender();
        std::thread::spawn(move || {
            let result = p.pull();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.refreshed(&id, result, first, cx);
                }
            });
        });
        cx.repaint();
    }

    fn refreshed(&mut self, id: &str, result: Result<ics::Parsed, String>, first: bool, cx: &mut Cx) {
        self.refreshing.retain(|x| x != id);
        let Some(i) = self.store.index(id) else { return };
        match result {
            Ok(p) => {
                let n = p.events.len();
                let c = &mut self.store.cals[i];
                c.refreshed = tz::now();
                c.error = None;
                if first {
                    if let Some(name) = p.name.clone() {
                        c.name = name;
                    }
                    if let Some(col) = p.color.clone() {
                        c.color = col;
                    }
                }
                let name = c.name.clone();
                self.store.replace(i, p.events);
                self.store.save_list();
                self.note(format!("subscribed: {name} ({n})"));
                if first {
                    self.say(fill(tr("Subscribed to {} ({} events)"), &[&name, &n.to_string()]), false, cx);
                }
            }
            Err(e) => {
                self.note(format!("subscription error: {e}"));
                self.store.cals[i].error = Some(e.clone());
                self.store.save_list();
                if first {
                    self.say(fill(tr("Could not download: {}"), &[&e]), false, cx);
                }
            }
        }
        cx.repaint();
    }

    /// Subscriptions older than an hour are downloaded again.
    fn refresh_due(&mut self, cx: &mut Cx) {
        let now = tz::now();
        let due: Vec<usize> =
            (0..self.store.cals.len()).filter(|&i| self.store.cals[i].url.is_some() && now - self.store.cals[i].refreshed >= REFRESH_EVERY).collect();
        for i in due {
            // a failed download waits the full hour too
            self.store.cals[i].refreshed = self.store.cals[i].refreshed.max(now - REFRESH_EVERY + 300).min(now);
            self.refresh_cal(i, false, cx);
        }
    }

    /// Imports an .ics file: a few events go to the default calendar, a whole calendar becomes a new one.
    pub fn import_file(&mut self, path: &std::path::Path, cx: &mut Cx) {
        let text = match std::fs::read(path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                self.say(e.to_string(), false, cx);
                return;
            }
        };
        let p = ics::parse(&text);
        if p.events.is_empty() {
            self.say(tr("Nothing to import in this file"), false, cx);
            return;
        }
        let n = p.events.len();
        let first = p.events[0].clone();
        let i = match self.store.default_cal() {
            Some(d) if n <= 3 => d,
            _ => {
                let name = p.name.clone().unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
                let i = self.store.add_calendar(&name, None);
                if let Some(c) = p.color.clone() {
                    self.store.cals[i].color = c;
                    self.store.save_list();
                }
                i
            }
        };
        self.store.merge(i, p.events);
        let name = self.store.cals[i].name.clone();
        self.note(format!("imported: {n} into {name}"));
        // show the first one
        let z = self.zone();
        let d = if first.all_day { tz::floating_date(first.start) } else { tz::date_at(&z, first.start) };
        let v = self.view;
        self.go(v, d);
        self.say(fill(tr("{} events imported into {}"), &[&n.to_string(), &name]), false, cx);
    }

    pub fn export_cal(&mut self, i: usize, cx: &mut Cx) {
        let Some(c) = self.store.cals.get(i) else { return };
        let name = format!("{}.ics", c.name.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_"));
        let text = ics::write(&c.name, Some(&c.color), &self.store.events[i]);
        let title = tr("Export .ics");
        let Some(path) = dialogs::save_as(self.win.hwnd, title, &name, "ics", &[("iCalendar (*.ics)", "*.ics")]) else { return };
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.note(format!("exported: {}", path.display()));
                self.say(fill(tr("Exported to {}"), &[&path.display().to_string()]), false, cx);
            }
            Err(e) => self.say(e.to_string(), false, cx),
        }
    }

    pub fn pick_import(&mut self, cx: &mut Cx) {
        let files = dialogs::pick_files(self.win.hwnd, tr("Import .ics"));
        for f in files {
            self.import_file(&f, cx);
        }
    }

    fn menu_act(&mut self, act: MenuAct, cx: &mut Cx) {
        self.menu = None;
        match act {
            MenuAct::NewCalendar => {
                self.dialog = Some(Dialog { kind: DlgKind::NewCalendar, text: String::new(), error: None });
                self.edit_for = None;
                self.open_field(Field::Dialog, cx);
            }
            MenuAct::Subscribe => {
                self.dialog = Some(Dialog { kind: DlgKind::Subscribe, text: String::new(), error: None });
                self.edit_for = None;
                self.open_field(Field::Dialog, cx);
            }
            MenuAct::Import => self.pick_import(cx),
            MenuAct::Rename(i) => {
                let name = self.store.cals.get(i).map(|c| c.name.clone()).unwrap_or_default();
                self.dialog = Some(Dialog { kind: DlgKind::Rename(i), text: name, error: None });
                self.edit_for = None;
                self.open_field(Field::Dialog, cx);
            }
            MenuAct::Export(i) => self.export_cal(i, cx),
            MenuAct::Refresh(i) => self.refresh_cal(i, false, cx),
            MenuAct::Remove(i) => {
                let text = if self.store.cals[i].read_only() { tr("Unsubscribe from this calendar?") } else { tr("Delete this calendar and its events?") };
                self.confirm = Some(Confirm { text: format!("{}\n{}", self.store.cals[i].name, text), act: ConfirmAct::RemoveCalendar(i) });
            }
        }
        cx.repaint();
    }

    fn dialog_ok(&mut self, cx: &mut Cx) {
        let Some(d) = self.dialog.take() else { return };
        let text = self.win.edit_text().unwrap_or(d.text.clone());
        self.edit_for = None;
        match d.kind {
            DlgKind::NewCalendar => {
                if !text.trim().is_empty() {
                    let i = self.store.add_calendar(text.trim(), None);
                    self.note(format!("calendar: {}", self.store.cals[i].name));
                }
            }
            DlgKind::Subscribe => self.subscribe(&text, cx),
            DlgKind::Rename(i) => {
                if let Some(c) = self.store.cals.get_mut(i)
                    && !text.trim().is_empty()
                {
                    c.name = text.trim().to_string();
                    self.store.save_list();
                    self.store.save_cal(i);
                }
            }
        }
        cx.repaint();
    }

    // ───────────────────────── reminders ─────────────────────────

    pub fn check_reminders(&mut self, cx: &mut Cx) {
        let now = self.now();
        if now <= self.last_check {
            return;
        }
        let mut due = reminders::due(&self.store, self.last_check, now);
        let (ready, later): (Vec<Due>, Vec<Due>) = std::mem::take(&mut self.snoozed).into_iter().partition(|d| d.at <= now);
        self.snoozed = later;
        due.extend(ready);
        self.last_check = now;
        if self.now_override.is_none() {
            reminders::save_last(now);
        }
        for d in due {
            self.deliver(d, cx);
        }
    }

    fn deliver(&mut self, d: Due, cx: &mut Cx) {
        self.note(format!("reminder: {} ({}s before)", d.title, d.before));
        let in_window = self.quiet || (sys::visible(self.win.hwnd) && self.win.is_active());
        if in_window {
            if !self.alerts.iter().any(|a| a.key() == d.key()) {
                self.alerts.push(d);
            }
            cx.repaint();
            return;
        }
        if self.set.reminder_style == "toast" {
            let when = popup::when(&d, self.now(), self.h24());
            let text = if d.location.is_empty() { when } else { format!("{when} · {}", d.location) };
            sys::notify(self.win.hwnd, &d.title, &text);
            return;
        }
        let main = self.win;
        match self.popup {
            Some(p) => {
                let h24 = self.h24();
                let shown = p.with(move |h, cx| {
                    if let Some(pp) = h.as_any().downcast_mut::<popup::Popup>() {
                        pp.add(d, h24, cx);
                    }
                });
                if shown.is_none() {
                    self.popup = None;
                }
            }
            None => {
                let t = self.t.clone();
                let h24 = self.h24();
                let gfx = self.gfx.clone();
                cx.after(move |_| {
                    if let Some(w) = popup::open(main, gfx, t, d, h24) {
                        main.with(|h, _| {
                            if let Some(app) = with_app(h) {
                                app.popup = Some(w);
                            }
                        });
                    }
                });
            }
        }
    }

    /// From a reminder: again in five minutes.
    pub fn snooze(&mut self, mut d: Due) {
        d.at = self.now() + 300;
        self.note(format!("snoozed: {}", d.title));
        self.snoozed.push(d);
    }

    /// From a reminder: show its day.
    pub fn open_due(&mut self, d: &Due) {
        let z = self.zone();
        let date = tz::date_at(&z, d.start);
        let v = if self.view == View::Month || self.view == View::Agenda { View::Day } else { self.view };
        self.go(v, date);
        self.scroll_to_hour((z.to_wall(d.start).secs as f32 / 3600.0 - 1.0).max(0.0));
        sys::show(self.win.hwnd, true);
    }

    // ───────────────────────── clicks ─────────────────────────

    pub fn act(&mut self, id: Id, clicks: u8, cx: &mut Cx) {
        let z = self.zone();
        match id {
            Id::Min => self.win.minimize(),
            Id::Max => self.win.toggle_maximize(),
            Id::Close => self.win.close(),
            Id::Today => {
                let (v, t) = (self.view, self.today());
                self.go(v, t);
                let h = z.to_wall(self.now()).secs as f32 / 3600.0;
                self.scroll_to_hour((h - 2.0).max(0.0));
            }
            Id::Prev => self.step(-1),
            Id::Next => self.step(1),
            Id::Tab(v) => {
                let d = self.selected;
                self.go(v, d);
            }
            Id::Settings => {
                self.settings_open = !self.settings_open;
                self.close_overlays();
                if self.edit_for == Some(Field::Quick) {
                    self.edit_for = None;
                }
            }
            Id::NewEvent => {
                let d = self.selected;
                let now = z.to_wall(self.now());
                let h = if d == now.date { (now.hour() + 1).min(23) } else { 9 };
                self.new_event(Wall::new(d, h, 0, 0), Wall::new(d, (h + 1).min(23), if h >= 23 { 59 } else { 0 }, 0), false, cx);
            }
            Id::Field(f) => self.open_field(f, cx),
            Id::MiniPrev => self.mini = self.mini.add_months(-1),
            Id::MiniNext => self.mini = self.mini.add_months(1),
            Id::Mini(n) => {
                let d = Date::from_days(n);
                let v = self.view;
                self.go(v, d);
            }
            Id::CalToggle(i) => {
                if let Some(c) = self.store.cals.get_mut(i) {
                    c.visible = !c.visible;
                    self.store.save_list();
                    self.store.touch();
                }
            }
            Id::CalMore(i) => {
                let c = &self.store.cals[i];
                let mut items = vec![(tr("Rename").to_string(), MenuAct::Rename(i)), (tr("Export .ics").to_string(), MenuAct::Export(i))];
                if c.read_only() {
                    items.push((tr("Refresh").to_string(), MenuAct::Refresh(i)));
                }
                items.push((tr("Remove").to_string(), MenuAct::Remove(i)));
                self.menu = Some(Menu { items, at: self.pointer, colors_for: Some(i) });
            }
            Id::CalAdd => {
                self.menu = Some(Menu {
                    items: vec![
                        (tr("New calendar").to_string(), MenuAct::NewCalendar),
                        (tr("Subscribe to a calendar").to_string(), MenuAct::Subscribe),
                        (tr("Import .ics").to_string(), MenuAct::Import),
                    ],
                    at: self.pointer,
                    colors_for: None,
                })
            }
            Id::MenuItem(k) => {
                if let Some(m) = self.menu.take()
                    && let Some((_, act)) = m.items.into_iter().nth(k)
                {
                    self.menu_act(act, cx);
                }
            }
            Id::MenuColor(k) => {
                if let Some(i) = self.menu.as_ref().and_then(|m| m.colors_for)
                    && let Some(c) = self.store.cals.get_mut(i)
                {
                    c.color = crate::model::PALETTE[k % crate::model::PALETTE.len()].into();
                    self.store.save_list();
                    self.store.save_cal(i);
                    self.store.touch();
                }
            }
            Id::Cell(n) => {
                let d = Date::from_days(n);
                if clicks >= 2 {
                    self.new_event(Wall { date: d, secs: 0 }, Wall { date: d, secs: 0 }, true, cx);
                } else {
                    self.selected = d;
                    self.peek = None;
                }
            }
            Id::More(n) | Id::DayHead(n) => self.go(View::Day, Date::from_days(n)),
            Id::Occ(i) | Id::AgendaRow(i) => {
                let Some(o) = self.occs.get(i) else { return };
                let r = OccRef::of(o);
                if clicks >= 2 {
                    self.edit_occ(r, cx);
                } else {
                    let at = self.hits.rect_of(|h| *h == Id::Occ(i) || *h == Id::AgendaRow(i)).unwrap_or_default();
                    self.peek = Some(Peek { occ: r, at });
                }
            }
            Id::PeekClose | Id::Scrim => {
                self.peek = None;
                self.menu = None;
            }
            Id::PeekEdit => {
                if let Some(p) = self.peek.take() {
                    self.edit_occ(p.occ, cx);
                }
            }
            Id::PeekDelete => {
                if let Some(p) = self.peek.take() {
                    self.delete_occ(p.occ, cx);
                }
            }
            Id::EdSave => self.save_editor(cx),
            Id::EdCancel => {
                self.editor = None;
                self.edit_for = None;
            }
            Id::EdDelete => {
                if let Some(r) = self.editor.as_ref().and_then(|e| e.target.clone()) {
                    self.delete_occ(r, cx);
                }
            }
            Id::EdCal => {
                let n = self.store.cals.len();
                if let Some(e) = self.editor.as_mut() {
                    for k in 1..=n {
                        let c = (e.cal + k) % n;
                        if !self.store.cals[c].read_only() {
                            e.cal = c;
                            break;
                        }
                    }
                }
            }
            id @ (Id::EdAllDay | Id::EdFreq(_) | Id::EdDay(_) | Id::EdMonthBy(_) | Id::EdEnds(_) | Id::EdReminder | Id::EdColor(_) | Id::EdStep(..)) => {
                self.commit_field();
                if let Some(e) = self.editor.as_mut() {
                    e.click(&id);
                    if let Id::EdEnds(k @ 1..=2) = id {
                        let f = if k == 1 { Field::Count } else { Field::Until };
                        self.edit_for = None;
                        self.open_field(f, cx);
                    }
                }
                // a field showing a changed value is opened again with it
                if matches!(id, Id::EdStep(..)) {
                    self.edit_shown = None;
                }
            }
            Id::ScopeThis => self.apply_scope(false, cx),
            Id::ScopeAll => self.apply_scope(true, cx),
            Id::ScopeCancel => {
                self.scope = None;
            }
            Id::DlgOk => self.dialog_ok(cx),
            Id::DlgCancel => {
                self.dialog = None;
                self.edit_for = None;
            }
            Id::ConfirmOk => {
                if let Some(c) = self.confirm.take() {
                    match c.act {
                        ConfirmAct::RemoveCalendar(i) => {
                            self.note(format!("removed calendar: {}", self.store.cals[i].name));
                            self.store.remove_calendar(i);
                            if self.store.cals.is_empty() {
                                self.store.add_calendar(tr("Personal"), None);
                            }
                            self.peek = None;
                        }
                    }
                }
            }
            Id::ConfirmCancel => self.confirm = None,
            Id::Undo => {
                if let Some((cal, evs)) = self.undo.take()
                    && cal < self.store.events.len()
                {
                    self.store.replace(cal, evs);
                    self.note("undo");
                }
                self.toast = None;
            }
            Id::AlertSnooze(i) => {
                if i < self.alerts.len() {
                    let d = self.alerts.remove(i);
                    self.snooze(d);
                }
            }
            Id::AlertDismiss(i) => {
                if i < self.alerts.len() {
                    self.alerts.remove(i);
                }
            }
            Id::AlertOpen(i) => {
                if i < self.alerts.len() {
                    let d = self.alerts.remove(i);
                    self.open_due(&d);
                }
            }
            Id::Set(k, v) => self.change_setting(k, v, cx),
            Id::SetBack => self.settings_open = false,
            Id::OpenData => {
                let dir = crate::model::data_dir();
                let _ = std::fs::create_dir_all(&dir);
                self.win.open(&dir.display().to_string());
            }
            Id::AllDay(n) if clicks >= 2 => {
                let d = Date::from_days(n);
                self.new_event(Wall { date: d, secs: 0 }, Wall { date: d, secs: 0 }, true, cx);
            }
            Id::AllDay(n) | Id::Grid(n) => {
                self.selected = Date::from_days(n);
                self.peek = None;
            }
            Id::Resize(_) | Id::Scroll => {}
        }
        cx.repaint();
    }

    pub fn change_setting(&mut self, k: &str, v: &str, cx: &mut Cx) {
        match k {
            "theme" => {
                self.set.theme = v.into();
                self.t = theme_for(&self.set);
                self.win.set_dark(self.t.dark);
                self.edit_shown = None;
            }
            "lang" => {
                self.set.lang = v.into();
                i18n::set(v);
            }
            "first_day" => self.set.first_day = v.into(),
            "clock" => self.set.clock = v.into(),
            "default_reminder" => self.set.default_reminder = v.parse().unwrap_or(-1),
            "week_numbers" => self.set.week_numbers = v == "on",
            "reminder_style" => self.set.reminder_style = v.into(),
            "background" => {
                self.set.background = v == "on";
                self.update_keep();
                if self.set.background && !self.tray && !self.quiet {
                    sys::tray_add(self.win.hwnd, "Plume Calendar");
                    self.tray = true;
                }
            }
            "autostart" => {
                let on = v == "on";
                if sys::set_autostart(on) {
                    self.autostart = on;
                }
                self.note(format!("autostart: {}", sys::autostart()));
            }
            _ => {}
        }
        self.set.save();
        self.store.touch();
        cx.repaint();
    }

    // ───────────────────────── pointer ─────────────────────────

    /// The instant under the pointer in the week / day grid (snapped to 15 minutes), and its day.
    pub fn grid_instant(&self, p: Pt) -> Option<(Date, i64)> {
        let g = &self.geo;
        if g.days.is_empty() || g.col_w <= 0.0 {
            return None;
        }
        let col = (((p.x - g.col_x) / g.col_w).floor() as i64).clamp(0, g.days.len() as i64 - 1) as usize;
        let d = g.days[col];
        let mins = ((p.y - g.grid.y + self.scroll) / g.hour_h * 60.0).clamp(0.0, 24.0 * 60.0);
        let q = ((mins / 15.0).round() as u32 * 15).min(24 * 60);
        let t = if q >= 24 * 60 { tz::day_start(&self.zone(), d.add_days(1)) } else { self.zone().to_utc(Wall { date: d, secs: q * 60 }) };
        Some((d, t))
    }

    /// The day under the pointer in the month grid or the all-day row.
    pub fn day_at(&self, p: Pt) -> Option<Date> {
        let g = &self.geo;
        if let Some(first) = g.month_first
            && g.month.contains(p)
        {
            let col = ((p.x - g.month.x) / g.cell_w).floor().clamp(0.0, 6.0) as i64;
            let row = ((p.y - g.month.y) / g.cell_h).floor().clamp(0.0, 5.0) as i64;
            return Some(first.add_days(row * 7 + col));
        }
        if !g.days.is_empty() && g.col_w > 0.0 {
            let col = (((p.x - g.col_x) / g.col_w).floor() as i64).clamp(0, g.days.len() as i64 - 1) as usize;
            return Some(g.days[col]);
        }
        None
    }

    fn mouse_down(&mut self, p: Pt, clicks: u8, cx: &mut Cx) {
        let id = self.hits.at(p).cloned();
        self.pressed = id.clone();
        // clicking elsewhere closes the open text field (its value is kept)
        if let Some(f) = self.edit_for
            && !matches!(&id, Some(Id::Field(g)) if *g == f)
        {
            if f == Field::Quick {
                self.edit_for = None;
            } else {
                self.commit_field();
            }
        }
        let writable = |app: &App, i: usize| app.occs.get(i).is_some_and(|o| !app.store.cals[o.cal].read_only());
        match id {
            Some(Id::Occ(i)) if clicks == 1 => {
                let at0 = self.grid_instant(p).map(|x| x.1).unwrap_or(0);
                self.drag = Drag::Press { occ: i, p0: p, at0 };
            }
            Some(Id::Resize(i)) if writable(self, i) => {
                let end = self.occs[i].end;
                self.drag = Drag::Resize { occ: i, end };
            }
            Some(Id::Grid(_)) if clicks == 1 => {
                if let Some((_, t)) = self.grid_instant(p) {
                    self.drag = Drag::Create { a: t, b: t };
                }
            }
            Some(Id::Grid(_)) => {
                // double click: a new hour there
                if let Some((d, t)) = self.grid_instant(p) {
                    let w = self.zone().to_wall(t);
                    let start = Wall { date: d, secs: w.secs / 1800 * 1800 };
                    let end = Wall::from_naive(start.naive() + 3600);
                    self.new_event(start, end, false, cx);
                }
                self.pressed = None;
            }
            Some(Id::Cell(n) | Id::AllDay(n)) if clicks == 1 => self.drag = Drag::CreateDays { a: n, b: n },
            _ => {}
        }
        cx.capture(true);
        cx.repaint();
    }

    fn mouse_move(&mut self, p: Pt, cx: &mut Cx) {
        self.pointer = p;
        match self.drag.clone() {
            Drag::None => {
                let h = self.hits.at(p).cloned();
                if h != self.hover {
                    self.hover = h;
                    cx.repaint();
                }
            }
            Drag::Press { occ, p0, at0 } => {
                if (p.x - p0.x).abs() + (p.y - p0.y).abs() > 5.0 && self.occs.get(occ).is_some_and(|o| !self.store.cals[o.cal].read_only()) {
                    let o = self.occs[occ].clone();
                    let in_grid = matches!(self.view, View::Week | View::Day) && !o.all_day && (o.end - o.start) < DAY && self.geo.grid.contains(p0);
                    self.drag = if in_grid {
                        Drag::Move { occ, grab: at0 - o.start, start: o.start }
                    } else {
                        let z = self.zone();
                        let grab_day = self.day_at(p0).map(|d| d.days()).unwrap_or(0) - tz::date_at(&z, o.start).days();
                        Drag::MoveDays { occ, grab: grab_day, day: tz::date_at(&z, o.start).days() }
                    };
                    self.peek = None;
                    self.mouse_move(p, cx);
                }
            }
            Drag::Create { a, .. } => {
                if let Some((_, t)) = self.grid_instant(p) {
                    self.drag = Drag::Create { a, b: t };
                    cx.repaint();
                }
            }
            Drag::CreateDays { a, .. } => {
                if let Some(d) = self.day_at(p) {
                    self.drag = Drag::CreateDays { a, b: d.days() };
                    cx.repaint();
                }
            }
            Drag::Move { occ, grab, .. } => {
                if let Some((_, t)) = self.grid_instant(p) {
                    // keep 15-minute steps from the original start
                    let o = &self.occs[occ];
                    let raw = t - grab;
                    let steps = ((raw - o.start) as f64 / 900.0).round() as i64;
                    self.drag = Drag::Move { occ, grab, start: o.start + steps * 900 };
                    cx.repaint();
                }
            }
            Drag::MoveDays { occ, grab, .. } => {
                if let Some(d) = self.day_at(p) {
                    self.drag = Drag::MoveDays { occ, grab, day: d.days() - grab };
                    cx.repaint();
                }
            }
            Drag::Resize { occ, .. } => {
                if let Some((_, t)) = self.grid_instant(p) {
                    let start = self.occs[occ].start;
                    self.drag = Drag::Resize { occ, end: t.max(start + 900) };
                    cx.repaint();
                }
            }
        }
    }

    fn mouse_up(&mut self, p: Pt, cx: &mut Cx) {
        cx.capture(false);
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        let up = self.hits.at(p).cloned();
        let pressed = self.pressed.take();
        let z = self.zone();
        match drag {
            Drag::Create { a, b } if a != b => {
                let (s, e) = if a < b { (a, b) } else { (b, a) };
                self.new_event(z.to_wall(s), z.to_wall(e), false, cx);
                return;
            }
            Drag::CreateDays { a, b } if a != b => {
                let (s, e) = if a < b { (a, b) } else { (b, a) };
                self.new_event(Wall { date: Date::from_days(s), secs: 0 }, Wall { date: Date::from_days(e), secs: 0 }, true, cx);
                return;
            }
            Drag::Move { occ, start, .. } => {
                if let Some(o) = self.occs.get(occ).cloned() {
                    self.move_occ(occ, start, start + (o.end - o.start), cx);
                }
                return;
            }
            Drag::MoveDays { occ, day, .. } => {
                if let Some(o) = self.occs.get(occ).cloned() {
                    let from = tz::date_at(&z, o.start);
                    let delta = day - from.days();
                    if delta != 0 {
                        // same wall-clock times, other days
                        let ws = z.to_wall(o.start);
                        let we = z.to_wall(o.end);
                        let s = z.to_utc(Wall { date: ws.date.add_days(delta), secs: ws.secs });
                        let e = z.to_utc(Wall { date: we.date.add_days(delta), secs: we.secs });
                        self.move_occ(occ, s, e, cx);
                    }
                }
                return;
            }
            Drag::Resize { occ, end } => {
                if let Some(o) = self.occs.get(occ).cloned() {
                    self.move_occ(occ, o.start, end, cx);
                }
                return;
            }
            _ => {}
        }
        if let (Some(a), Some(b)) = (pressed, up)
            && a == b
        {
            self.act(a, 1, cx);
        }
        cx.repaint();
    }

    fn wheel(&mut self, p: Pt, dy: f32, cx: &mut Cx) {
        if self.settings_open {
            self.settings_scroll = (self.settings_scroll - dy * 60.0).max(0.0);
        } else {
            match self.view {
                View::Week | View::Day if self.geo.grid.contains(p) || self.geo.allday.contains(p) => {
                    let max = (24.0 * self.geo.hour_h - self.geo.grid.h).max(0.0);
                    self.scroll = (self.scroll - dy * 72.0).clamp(0.0, max);
                }
                View::Agenda => {
                    let max = (self.agenda_h - self.geo.agenda.h).max(0.0);
                    self.agenda_scroll = (self.agenda_scroll - dy * 72.0).clamp(0.0, max);
                }
                View::Month if self.geo.month.contains(p) => self.step(if dy > 0.0 { -1 } else { 1 }),
                _ => {}
            }
        }
        self.peek = None;
        cx.repaint();
    }

    fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        let modal = self.editor.is_some() || self.dialog.is_some() || self.scope.is_some() || self.confirm.is_some();
        match vk {
            0x1B => {
                if self.scope.is_some() {
                    self.scope = None;
                } else if self.confirm.is_some() {
                    self.confirm = None;
                } else if self.dialog.is_some() {
                    self.dialog = None;
                    self.edit_for = None;
                } else if self.editor.is_some() {
                    self.editor = None;
                    self.edit_for = None;
                } else if self.menu.is_some() || self.peek.is_some() {
                    self.close_overlays();
                } else if self.settings_open {
                    self.settings_open = false;
                }
            }
            _ if modal => {
                if vk == 0x0D && self.editor.is_some() && mods.ctrl {
                    self.save_editor(cx);
                }
                return;
            }
            0x25 | 0x21 => self.step(-1),
            0x27 | 0x22 => self.step(1),
            0x26 | 0x28 => {
                let down = vk == 0x28;
                match self.view {
                    View::Month => {
                        let d = self.selected.add_days(if down { 7 } else { -7 });
                        self.selected = d;
                        if d.first_of_month() != self.anchor.first_of_month() {
                            self.anchor = d;
                            self.mini = d.first_of_month();
                        }
                    }
                    View::Agenda => {
                        self.agenda_scroll = (self.agenda_scroll + if down { 72.0 } else { -72.0 }).clamp(0.0, (self.agenda_h - self.geo.agenda.h).max(0.0))
                    }
                    _ => self.scroll = (self.scroll + if down { 48.0 } else { -48.0 }).clamp(0.0, (24.0 * self.geo.hour_h - self.geo.grid.h).max(0.0)),
                }
            }
            0x54 => self.act(Id::Today, 1, cx),
            0x4D => self.act(Id::Tab(View::Month), 1, cx),
            0x57 => self.act(Id::Tab(View::Week), 1, cx),
            0x44 => self.act(Id::Tab(View::Day), 1, cx),
            0x41 => self.act(Id::Tab(View::Agenda), 1, cx),
            0x4E => self.act(Id::NewEvent, 1, cx),
            0x2E => {
                if let Some(p) = self.peek.take() {
                    self.delete_occ(p.occ, cx);
                }
            }
            0x0D => {
                if let Some(p) = self.peek.take() {
                    self.edit_occ(p.occ, cx);
                }
            }
            0x51 | 0x46 if mods.ctrl || vk == 0x51 => self.open_field(Field::Quick, cx),
            0xBC if mods.ctrl => self.act(Id::Settings, 1, cx),
            _ => return,
        }
        cx.handled();
        cx.repaint();
    }

    fn edit_key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        match (self.edit_for, vk) {
            (Some(Field::Quick), 0x0D) => {
                let text = self.win.edit_text().unwrap_or_else(|| self.quick.clone());
                if self.quick_add(&text, cx) {
                    self.quick.clear();
                    self.win.edit_set_text("");
                }
            }
            (Some(Field::Quick), 0x1B) => {
                self.quick.clear();
                self.close_field(cx);
            }
            (Some(Field::Dialog), 0x0D) => self.dialog_ok(cx),
            (Some(Field::Dialog), 0x1B) => {
                self.dialog = None;
                self.edit_for = None;
            }
            (Some(_), 0x0D) if self.editor.is_some() => {
                if mods.ctrl {
                    self.save_editor(cx);
                } else {
                    self.commit_field();
                    let next = self.editor.as_ref().and_then(|e| e.next_field(self.edit_for));
                    match next {
                        Some(f) => {
                            self.edit_for = None;
                            self.open_field(f, cx);
                        }
                        None => self.save_editor(cx),
                    }
                }
            }
            (Some(_), 0x09) if self.editor.is_some() => {
                self.commit_field();
                let next = self.editor.as_ref().and_then(|e| e.next_field(self.edit_for));
                self.edit_for = None;
                if let Some(f) = next {
                    self.open_field(f, cx);
                }
            }
            (Some(_), 0x1B) if self.editor.is_some() => self.close_field(cx),
            _ => {}
        }
        cx.repaint();
    }

    /// Text given on the command line or by another launch: .ics files to import, or "open".
    pub fn open_args(&mut self, line: &str, cx: &mut Cx) {
        for arg in crate::split_args(line) {
            let p = std::path::PathBuf::from(&arg);
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ics")) && p.is_file() {
                self.import_file(&p, cx);
            } else if arg.starts_with("webcal://") || (arg.starts_with("http") && arg.ends_with(".ics")) {
                self.subscribe(&arg, cx);
            }
        }
    }

    pub fn tray_tip(&self) -> String {
        "Plume Calendar".into()
    }

    pub fn shutdown(&mut self) {
        if self.now_override.is_none() {
            reminders::save_last(self.last_check);
        }
        if self.tray {
            sys::tray_remove(self.win.hwnd);
        }
        if let Some(p) = self.popup.take() {
            p.close();
        }
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_all(c, cx);
        self.sync_edit();
    }

    fn event(&mut self, ev: UiEvent, cx: &mut Cx) {
        match ev {
            UiEvent::MouseMove { p, .. } => self.mouse_move(p, cx),
            UiEvent::MouseLeave => {
                if matches!(self.drag, Drag::None) {
                    self.hover = None;
                    cx.repaint();
                }
            }
            UiEvent::MouseDown { p, button: Button::Left, clicks, .. } => {
                self.pointer = p;
                self.mouse_down(p, clicks, cx);
                if clicks >= 2
                    && let Some(id) = self.hits.at(p).cloned()
                    && matches!(id, Id::Occ(_) | Id::AgendaRow(_) | Id::Cell(_) | Id::AllDay(_))
                {
                    self.drag = Drag::None;
                    self.pressed = None;
                    self.act(id, 2, cx);
                }
            }
            UiEvent::MouseUp { p, button: Button::Left, .. } => self.mouse_up(p, cx),
            UiEvent::MouseDown { p, button: Button::Right, .. } => {
                self.pointer = p;
                match self.hits.at(p).cloned() {
                    Some(Id::CalToggle(i) | Id::CalMore(i)) => self.act(Id::CalMore(i), 1, cx),
                    Some(Id::Occ(i) | Id::AgendaRow(i)) => self.act(Id::Occ(i), 1, cx),
                    _ => {}
                }
            }
            UiEvent::MouseDown { button: Button::Back, .. } => {
                self.step(-1);
                cx.repaint();
            }
            UiEvent::MouseDown { button: Button::Forward, .. } => {
                self.step(1);
                cx.repaint();
            }
            UiEvent::Wheel { p, dy, .. } => self.wheel(p, dy, cx),
            UiEvent::Key { vk, mods, .. } => self.key(vk, mods, cx),
            UiEvent::EditChanged(text) => {
                self.field_changed(text);
                cx.repaint();
            }
            UiEvent::EditKey { vk, mods } => self.edit_key(vk, mods, cx),
            UiEvent::EditBlur => {
                if self.edit_for == Some(Field::Quick) && self.quick.is_empty() {
                    self.edit_for = None;
                    cx.repaint();
                }
            }
            UiEvent::Timer(TICK) => {
                self.check_reminders(cx);
                self.refresh_due(cx);
                cx.repaint();
            }
            UiEvent::Timer(TOAST) => {
                if self.toast.as_ref().is_none_or(|t| t.since.elapsed().as_secs() >= if t.undo { 6 } else { 4 }) {
                    self.toast = None;
                    cx.timer(TOAST, None);
                }
                cx.repaint();
            }
            UiEvent::Opened(text) if text == "quit" => {
                self.quitting = true;
                self.update_keep();
                cx.after(|w| w.close());
            }
            UiEvent::Opened(text) => {
                if text != "--background" {
                    sys::show(self.win.hwnd, true);
                    self.win.bring_to_front();
                }
                self.open_args(&text, cx);
                cx.repaint();
            }
            UiEvent::Close => self.shutdown(),
            UiEvent::SystemTheme => {
                if self.set.theme == "system" {
                    let t = theme_for(&self.set);
                    if t.dark != self.t.dark {
                        self.t = t;
                        self.win.set_dark(self.t.dark);
                        self.edit_shown = None;
                    }
                }
                cx.repaint();
            }
            UiEvent::Resized | UiEvent::Scale | UiEvent::Active(_) => cx.repaint(),
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Id::Max) => Area::MaxButton,
            Some(Id::Min | Id::Close) => Area::Control,
            Some(_) if p.y < m::TITLE_H => Area::Control,
            _ if p.y < m::TITLE_H => Area::Caption,
            _ => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match (&self.drag, self.hits.at(p)) {
            (Drag::Resize { .. }, _) | (_, Some(Id::Resize(_))) => Cursor::SizeNS,
            (_, Some(Id::Min | Id::Max | Id::Close | Id::Grid(_) | Id::Cell(_) | Id::AllDay(_) | Id::Scrim | Id::Scroll)) | (_, None) => Cursor::Arrow,
            (_, Some(Id::Field(_))) => Cursor::Arrow,
            _ => Cursor::Hand,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
