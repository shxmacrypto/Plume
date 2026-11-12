//! The floating box: a search field and the results under it. It lives hidden and comes up in an instant when
//! the shortcut is pressed; Esc, opening a result or clicking elsewhere puts it away.

use crate::{
    apps, calc, files,
    i18n::{fill, french, tr},
    icons::{self as shell_icons, Pixels},
    pages,
    rank::{self, History, Item, Kind, fold, kind_bonus, score},
    settings::{Settings, data_dir},
    settings_ui, sys,
};
use plume_ui::{
    Align, Area, Button, Canvas, Color, Cursor, Cx, EditStyle, Event, Font, Gfx, Handler, Hits, Mods, PaintCx, Pt, Rect, Theme, Window,
    icons::{self, Icon, Part},
    rect,
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Graphics::Imaging::GUID_WICPixelFormat32bppPBGRA,
        System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
    },
    core::{Interface, w},
};

/// Width of the box, heights of its parts (DIP).
pub const W: f32 = 660.0;
const BAR_H: f32 = 60.0;
const ROW_H: f32 = 48.0;
const HEAD_H: f32 = 28.0;
const MAX_ROWS: usize = 8;
const FOOT_H: f32 = 36.0;
const PAD: f32 = 8.0;
/// Size of a result's icon.
const ICON: f32 = 32.0;
/// The list of apps is read again when the box opens, if it is older than this.
const APPS_FRESH: Duration = Duration::from_secs(180);

pub const CALC: Icon = Icon {
    name: "LAUNCHER_CALC",
    vb: 16.0,
    parts: &[Part::R(3.0, 1.5, 10.0, 13.0, 2.0), Part::P("M5.5 4.5h5"), Part::P("M5.8 8h.01M8 8h.01M10.2 8h.01M5.8 11h.01M8 11h.01M10.2 11h.01")],
};
pub const GLOBE: Icon = Icon {
    name: "LAUNCHER_GLOBE",
    vb: 16.0,
    parts: &[Part::C(8.0, 8.0, 6.0), Part::P("M2 8h12"), Part::P("M8 2c-2.4 2.2-2.4 9.8 0 12M8 2c2.4 2.2 2.4 9.8 0 12")],
};
pub const APPS: Icon = Icon {
    name: "LAUNCHER_APPS",
    vb: 16.0,
    parts: &[Part::R(2.5, 2.5, 4.5, 4.5, 1.2), Part::R(9.0, 2.5, 4.5, 4.5, 1.2), Part::R(2.5, 9.0, 4.5, 4.5, 1.2), Part::R(9.0, 9.0, 4.5, 4.5, 1.2)],
};

/// What the box and the settings window share.
pub struct Shared {
    pub set: RefCell<Settings>,
    pub history: RefCell<History>,
    pub files: Arc<files::Files>,
    /// The shortcut could be taken (false: another app has it).
    pub hotkey_ok: Cell<bool>,
    pub popup: Cell<Option<Window>>,
    pub settings_win: Cell<Option<Window>>,
}

impl Shared {
    pub fn save_history(&self) {
        let file = data_dir().join("history.json");
        let _ = std::fs::create_dir_all(data_dir());
        if let Ok(b) = serde_json::to_vec(&*self.history.borrow()) {
            let tmp = file.with_extension("tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, file);
            }
        }
    }
}

pub fn load_history() -> History {
    std::fs::read(data_dir().join("history.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn system_dark() -> bool {
    let (mut data, mut size) = (1u32, 4u32);
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    read.is_ok() && data == 0
}

/// The theme chosen in the settings (or Windows'), with Plume Launcher's indigo.
pub fn theme_for(set: &Settings) -> Theme {
    let env = std::env::var("PLUME_SUITE_THEME").ok();
    let dark = match env.as_deref().unwrap_or(set.theme.as_str()) {
        "dark" => true,
        "light" => false,
        _ => system_dark(),
    };
    // the indigo of the icon
    Theme::new(dark, Some(Color::hex(0x6366f1)))
}

/// The box's background.
pub fn surface(t: &Theme) -> Color {
    // a card of Plume, made opaque: the window has no transparency
    t.card.over(t.bg)
}

fn enabled(set: &Settings, k: Kind) -> bool {
    match k {
        Kind::App => set.apps,
        Kind::File | Kind::Folder => set.files,
        Kind::Setting => set.settings,
        Kind::Calc => set.calc,
        Kind::Web => set.web,
    }
}

/// A file's folder as people read it: from their user folder on ("Documents\Work"), else the full path.
fn short_folder(p: &Path) -> String {
    let parent = p.parent().map(|p| p.display().to_string()).unwrap_or_default();
    if let Some(home) = std::env::var_os("USERPROFILE").map(|h| h.to_string_lossy().to_string())
        && parent.len() > home.len()
        && parent[..home.len()].eq_ignore_ascii_case(&home)
    {
        return parent[home.len()..].trim_start_matches('\\').to_string();
    }
    parent
}

/// Every result for `q`, best first: the calculation, then apps, settings pages and files by how well they match
/// (and how often and lately they were picked), the web search last. An empty query gives the recent picks.
pub fn results(q: &str, set: &Settings, apps: &[apps::App], found: &[files::Hit], hist: &History, fr: bool, now: u64) -> Vec<Item> {
    let q = q.trim();
    if q.is_empty() {
        return hist.recent(MAX_ROWS, now).into_iter().filter(|i| enabled(set, i.kind)).collect();
    }
    let fq = fold(q);
    let mut out: Vec<(i32, Item)> = vec![];
    let mut seen = HashSet::new();
    let mut add = |out: &mut Vec<(i32, Item)>, s: i32, item: Item| {
        if seen.insert(item.key()) {
            let total = s + kind_bonus(item.kind) + hist.boost(&fq, &item.key(), now);
            out.push((total, item));
        }
    };
    if set.calc
        && let Some(a) = calc::eval(q)
    {
        add(&mut out, 1_000_000, Item { kind: Kind::Calc, title: a.value.clone(), sub: a.detail, target: a.value, icon: String::new() });
    }
    if set.apps {
        for a in apps {
            if let Some(s) = score(&fq, &a.folded) {
                add(&mut out, s, Item { kind: Kind::App, title: a.name.clone(), sub: tr("App").into(), target: a.target.clone(), icon: a.target.clone() });
            }
        }
    }
    if set.settings {
        for (p, s) in pages::search(&fq, fr) {
            let title = if fr { p.fr } else { p.en };
            add(
                &mut out,
                s,
                Item {
                    kind: Kind::Setting,
                    title: title.into(),
                    sub: tr("Windows settings").into(),
                    target: p.uri.into(),
                    icon: shell_icons::SETTINGS_APP.into(),
                },
            );
        }
    }
    if set.files {
        let recent = now.saturating_sub(7 * 86_400);
        for h in found {
            if let Some(s) = score(&fq, &fold(&h.name)) {
                let target = h.path.display().to_string();
                let kind = if h.dir { Kind::Folder } else { Kind::File };
                let fresh = if h.mtime > recent { 25 } else { 0 };
                add(&mut out, s + fresh, Item { kind, title: h.name.clone(), sub: short_folder(&h.path), target: target.clone(), icon: target });
            }
        }
    }
    // results picked before for such a query come at once, even before the file search answers
    for p in hist.picks.values() {
        if enabled(set, p.item.kind)
            && let Some(s) = score(&fq, &fold(&p.item.title))
        {
            add(&mut out, s, p.item.clone());
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.0));
    let mut per: HashMap<&str, usize> = HashMap::new();
    let mut items: Vec<Item> = vec![];
    for (_, it) in out {
        let group = match it.kind {
            Kind::App => "app",
            Kind::Setting => "setting",
            Kind::File | Kind::Folder => "file",
            Kind::Calc | Kind::Web => "other",
        };
        let cap = match group {
            "app" => 8,
            "setting" => 5,
            "file" => 10,
            _ => 2,
        };
        let n = per.entry(group).or_default();
        if *n < cap && items.len() < 24 {
            *n += 1;
            items.push(it);
        }
    }
    if set.web {
        items.push(Item {
            kind: Kind::Web,
            title: fill(tr("Search the web for \u{201c}{q}\u{201d}"), &[q]),
            sub: set.engine_name().into(),
            target: set.search_url(q),
            icon: String::new(),
        });
    }
    items
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Row(usize),
    Gear,
    Field,
}

pub struct App {
    pub win: Window,
    pub sh: Rc<Shared>,
    pub gfx: Rc<Gfx>,
    pub theme: Theme,
    pub query: String,
    pub rows: Vec<Item>,
    pub sel: usize,
    /// First row on screen.
    pub top: usize,
    pub hits: Hits<Hit>,
    hover: Option<Hit>,
    pub apps: Vec<apps::App>,
    pub apps_ready: bool,
    apps_loading: bool,
    apps_at: Option<Instant>,
    pub file_hits: Vec<files::Hit>,
    file_gen: u64,
    icons: HashMap<(String, u32), Option<Rc<Pixels>>>,
    asked: HashSet<(String, u32)>,
    worker: shell_icons::Worker,
    pub visible: bool,
    active_seen: bool,
    /// Shown without the keyboard (tests): focus changes elsewhere do not close it.
    quiet_show: bool,
    shown_at: Option<Instant>,
    /// Time from the call to the first image of the box, the last time it was shown.
    pub show_ms: Option<f64>,
    pub shows: u32,
    pub height: f32,
    pub quitting: bool,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, sh: Rc<Shared>) -> App {
        let theme = theme_for(&sh.set.borrow());
        let sender = win.sender();
        let worker = shell_icons::start(Box::new(move |name, px, pixels| {
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.icons.insert((name, px), pixels.map(Rc::new));
                    if app.visible {
                        cx.repaint();
                    }
                }
            });
        }));
        let mut app = App {
            win,
            sh,
            gfx,
            theme,
            query: String::new(),
            rows: vec![],
            sel: 0,
            top: 0,
            hits: Hits::default(),
            hover: None,
            apps: vec![],
            apps_ready: false,
            apps_loading: false,
            apps_at: None,
            file_hits: vec![],
            file_gen: 0,
            icons: HashMap::new(),
            asked: HashSet::new(),
            worker,
            visible: false,
            active_seen: false,
            quiet_show: false,
            shown_at: None,
            show_ms: None,
            shows: 0,
            height: BAR_H + FOOT_H,
            quitting: false,
        };
        app.load_apps();
        app
    }

    /// Reads the installed apps again, on a worker thread.
    fn load_apps(&mut self) {
        if self.apps_loading {
            return;
        }
        self.apps_loading = true;
        let sender = self.win.sender();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
            }
            let list = apps::load();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.on_apps(list, cx);
                }
            });
        });
    }

    fn on_apps(&mut self, list: Vec<apps::App>, cx: &mut Cx) {
        self.apps_loading = false;
        self.apps_ready = true;
        self.apps_at = Some(Instant::now());
        // their icons are fetched now, so the box shows them at once
        let px = self.icon_px();
        for a in &list {
            self.ask_icon(&a.target, px, false);
        }
        self.apps = list;
        if self.visible {
            self.refresh(true, cx);
        }
    }

    pub fn on_files(&mut self, generation: u64, hits: Vec<files::Hit>, cx: &mut Cx) {
        if generation != self.file_gen {
            return;
        }
        self.file_hits = hits;
        if self.visible {
            self.refresh(true, cx);
        }
    }

    fn icon_px(&self) -> u32 {
        (ICON * self.win.scale()).round() as u32
    }

    fn ask_icon(&mut self, name: &str, px: u32, urgent: bool) {
        let key = (name.to_string(), px);
        if !name.is_empty() && !self.icons.contains_key(&key) && self.asked.insert(key) {
            self.worker.ask(name, px, urgent);
        }
    }

    // ───────────────────────── results ─────────────────────────

    fn visible_rows(&self) -> usize {
        self.rows.len().min(MAX_ROWS)
    }

    fn recent_mode(&self) -> bool {
        self.query.trim().is_empty() && !self.rows.is_empty()
    }

    fn list_y(&self) -> f32 {
        BAR_H + PAD + if self.recent_mode() { HEAD_H } else { 0.0 }
    }

    fn wanted_height(&self) -> f32 {
        if self.rows.is_empty() { BAR_H + FOOT_H } else { self.list_y() + self.visible_rows() as f32 * ROW_H + PAD + FOOT_H }
    }

    /// Computes the results again. `keep`: the selected result stays selected if it is still there.
    pub fn refresh(&mut self, keep: bool, cx: &mut Cx) {
        let old = self.rows.get(self.sel).map(|i| i.key());
        let set = self.sh.set.borrow().clone();
        self.rows = results(&self.query, &set, &self.apps, &self.file_hits, &self.sh.history.borrow(), french(), rank::now());
        // the selection follows its result only if it was moved there: results that arrive late (files, apps)
        // must not leave the top one unselected
        let moved = self.sel > 0;
        self.sel = 0;
        if keep
            && moved
            && let Some(k) = old
            && let Some(i) = self.rows.iter().position(|r| r.key() == k)
        {
            self.sel = i;
        }
        self.top = 0;
        self.keep_in_view();
        let px = self.icon_px();
        let wanted: Vec<String> = self.rows.iter().map(|r| r.icon.clone()).collect();
        for name in wanted {
            self.ask_icon(&name, px, true);
        }
        self.resize(cx);
        cx.repaint();
    }

    fn resize(&mut self, cx: &mut Cx) {
        let h = self.wanted_height();
        if (h - self.height).abs() > 0.5 {
            self.height = h;
            if self.visible {
                let hwnd = self.win.hwnd;
                cx.after(move |_| sys::set_height(hwnd, W, h));
            }
        }
    }

    fn keep_in_view(&mut self) {
        let n = self.visible_rows();
        if self.sel < self.top {
            self.top = self.sel;
        } else if n > 0 && self.sel >= self.top + n {
            self.top = self.sel + 1 - n;
        }
    }

    fn move_sel(&mut self, d: i32, cx: &mut Cx) {
        if self.rows.is_empty() {
            return;
        }
        let n = self.rows.len() as i32;
        self.sel = (self.sel as i32 + d).rem_euclid(n) as usize;
        self.keep_in_view();
        cx.repaint();
    }

    fn search_files(&mut self) {
        let q = self.query.trim().to_string();
        if !self.sh.set.borrow().files || q.chars().count() < 2 {
            return;
        }
        self.file_gen += 1;
        self.sh.files.search(self.file_gen, &q);
    }

    /// Opens the selected result (`reveal`: its folder, with it selected).
    pub fn open_selected(&mut self, reveal: bool, cx: &mut Cx) {
        let Some(item) = self.rows.get(self.sel).cloned() else { return };
        let q = self.query.clone();
        if item.remembered() {
            self.sh.history.borrow_mut().record(&q, &item, rank::now());
            self.sh.save_history();
        }
        self.hide(cx);
        let win = self.win;
        let sh = self.sh.clone();
        cx.after(move |_| {
            let path = PathBuf::from(&item.target);
            let ok = match item.kind {
                Kind::Calc => sys::copy(&win, &item.target),
                Kind::Web | Kind::Setting => sys::open(&item.target),
                Kind::App if reveal => item.target.starts_with("shell:") || sys::reveal(&path),
                Kind::File | Kind::Folder if reveal => sys::reveal(&path),
                Kind::App => sys::open(&item.target),
                Kind::File | Kind::Folder => {
                    if !sys::test_mode() && !path.exists() {
                        false
                    } else {
                        sys::open(&item.target)
                    }
                }
            };
            // a file that is gone leaves the history
            if !ok && matches!(item.kind, Kind::File | Kind::Folder) && !path.exists() {
                sh.history.borrow_mut().forget(&item.key());
                sh.save_history();
            }
        });
    }

    // ───────────────────────── showing ─────────────────────────

    fn edit_style(&self) -> EditStyle {
        EditStyle {
            fg: self.theme.t1,
            bg: surface(&self.theme),
            size: 19.0,
            select: None,
            placeholder: tr("Search apps, files, settings…").into(),
            password: false,
            multiline: false,
        }
    }

    fn field_rect(&self) -> Rect {
        rect(52.0, (BAR_H - 30.0) / 2.0, W - 52.0 - 20.0, 30.0)
    }

    /// Brings the box up (`activate`: it takes the keyboard; tests show it without).
    pub fn show(&mut self, activate: bool, cx: &mut Cx) {
        self.shown_at = Some(Instant::now());
        self.shows += 1;
        self.theme = theme_for(&self.sh.set.borrow());
        self.query.clear();
        self.file_hits.clear();
        self.visible = true;
        self.active_seen = false;
        self.quiet_show = !activate;
        self.hover = None;
        if self.apps_at.is_none_or(|t| t.elapsed() > APPS_FRESH) {
            self.load_apps();
        }
        self.refresh(false, cx);
        self.height = self.wanted_height();
        let (hwnd, h, field, style) = (self.win.hwnd, self.height, self.field_rect(), self.edit_style());
        cx.after(move |win| {
            sys::place(hwnd, W, h, activate);
            win.edit_show(field, "", &style);
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
        });
    }

    pub fn hide(&mut self, cx: &mut Cx) {
        if !self.visible {
            return;
        }
        self.visible = false;
        self.query.clear();
        self.rows.clear();
        self.file_hits.clear();
        self.hover = None;
        let hwnd = self.win.hwnd;
        cx.after(move |win| {
            win.edit_set_text("");
            win.edit_hide();
            sys::hide(hwnd);
        });
    }

    pub fn toggle(&mut self, cx: &mut Cx) {
        if self.visible && sys::visible(self.win.hwnd) {
            self.hide(cx);
        } else {
            self.show(true, cx);
        }
    }

    pub fn open_settings(&mut self, cx: &mut Cx) {
        self.hide(cx);
        let (gfx, sh) = (self.gfx.clone(), self.sh.clone());
        cx.after(move |_| settings_ui::open(&gfx, &sh, true));
    }

    pub fn quit(&mut self, cx: &mut Cx) {
        self.quitting = true;
        self.hide(cx);
        let sh = self.sh.clone();
        cx.after(move |win| {
            sh.files.stop();
            sys::unregister_hotkey(win.hwnd);
            sys::tray_remove(win.hwnd);
            if let Some(s) = sh.settings_win.take() {
                s.close();
            }
            win.close();
        });
    }

    // ───────────────────────── drawing ─────────────────────────

    fn draw_icon(&mut self, c: &mut Canvas, item: &Item, r: Rect) {
        let t = self.theme.clone();
        let px = (r.w * c.scale()).round() as u32;
        let key = (item.icon.clone(), px);
        if let Some(Some(p)) = self.icons.get(&key).or_else(|| self.icons.get(&(item.icon.clone(), self.icon_px()))) {
            let p = p.clone();
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            (&p.data.len(), &item.icon, p.w).hash(&mut hs);
            let gfx = self.gfx.clone();
            c.image(hs.finish(), r, 1.0, || unsafe {
                gfx.wic.CreateBitmapFromMemory(p.w, p.h, &GUID_WICPixelFormat32bppPBGRA, p.w * 4, &p.data).ok().and_then(|b| b.cast().ok())
            });
            return;
        }
        // the kind's own icon until the shell's arrives
        let (ic, tinted) = match item.kind {
            Kind::App => (&APPS, false),
            Kind::File => (&icons::FILE, false),
            Kind::Folder => (&icons::FOLDER, false),
            Kind::Setting => (&icons::GEAR, false),
            Kind::Calc => (&CALC, true),
            Kind::Web => (&GLOBE, true),
        };
        c.fill_round(r, 9.0, if tinted { t.accent.fade(0.16) } else { t.fill });
        c.icon(ic, r.centered(18.0, 18.0), if tinted { t.accent } else { t.t2 });
    }

    fn kind_label(k: Kind) -> &'static str {
        match k {
            Kind::App => tr("App"),
            Kind::File => tr("File"),
            Kind::Folder => tr("Folder"),
            Kind::Setting => tr("Setting"),
            Kind::Calc => tr("Calculator"),
            Kind::Web => tr("Web"),
        }
    }

    fn action_label(k: Kind) -> &'static str {
        match k {
            Kind::Calc => tr("Copy"),
            Kind::Web => tr("Search"),
            _ => tr("Open"),
        }
    }

    fn paint_row(&mut self, c: &mut Canvas, i: usize, r: Rect) {
        let t = self.theme.clone();
        let item = self.rows[i].clone();
        let selected = i == self.sel;
        if selected {
            c.fill_round(r, 10.0, t.accent.fade(0.16));
        } else if self.hover == Some(Hit::Row(i)) {
            c.fill_round(r, 10.0, t.hover);
        }
        let ir = rect(r.x + 12.0, r.y + (r.h - ICON) / 2.0, ICON, ICON);
        self.draw_icon(c, &item, ir);
        let tx = ir.right() + 12.0;
        let right_w = 96.0;
        let tw = (r.right() - tx - right_w - 8.0).max(40.0);
        let has_sub = !item.sub.is_empty();
        let (title_font, title_h) = if item.kind == Kind::Calc { (Font::Large, 22.0) } else { (Font::Title, 20.0) };
        let ty = if has_sub { r.y + 5.0 } else { r.y + (r.h - title_h) / 2.0 };
        c.text(&item.title, rect(tx, ty, tw, title_h), title_font, t.t1, Align::Left);
        if has_sub {
            c.text(&item.sub, rect(tx, r.y + 26.0, tw, 16.0), Font::Small, t.t3, Align::Left);
        }
        let lr = rect(r.right() - right_w - 10.0, r.y, right_w, r.h);
        if selected {
            let label = format!("{}  \u{21b5}", Self::action_label(item.kind));
            c.text(&label, lr, Font::SmallBold, t.accent, Align::Right);
        } else {
            c.text(Self::kind_label(item.kind), lr, Font::Small, t.t3, Align::Right);
        }
        self.hits.add(r, Hit::Row(i));
    }

    fn paint_box(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let t = self.theme.clone();
        c.clear(surface(&t));
        self.hits.clear();
        self.hits.add(rect(0.0, 0.0, w, BAR_H), Hit::Field);
        c.icon(&icons::SEARCH, rect(20.0, (BAR_H - 18.0) / 2.0, 18.0, 18.0), if self.query.is_empty() { t.t3 } else { t.accent });
        if !self.rows.is_empty() {
            c.hline(0.0, BAR_H, w, t.sep);
        }
        if self.recent_mode() {
            c.text(tr("Recent"), rect(PAD + 12.0, BAR_H + PAD, 200.0, HEAD_H), Font::SmallBold, t.t3, Align::Left);
        }
        let y0 = self.list_y();
        let n = self.visible_rows();
        for k in 0..n {
            let i = self.top + k;
            if i >= self.rows.len() {
                break;
            }
            let r = rect(PAD, y0 + k as f32 * ROW_H, w - 2.0 * PAD, ROW_H);
            self.paint_row(c, i, r);
        }
        // where the list is, when it holds more than it shows
        if self.rows.len() > n && n > 0 {
            let track = n as f32 * ROW_H - 8.0;
            let th = (track * n as f32 / self.rows.len() as f32).max(18.0);
            let ty = y0 + 4.0 + (track - th) * self.top as f32 / (self.rows.len() - n) as f32;
            c.fill_round(rect(w - 5.0, ty, 3.0, th), 1.5, t.t4);
        }
        // the foot: what the keys do, and the settings
        let fy = h - FOOT_H;
        c.hline(0.0, fy, w, t.sep);
        let hint = if self.query.trim().is_empty() {
            if self.rows.is_empty() {
                tr("Type to search apps, files and settings, or to calculate")
            } else {
                tr("Enter to open, Ctrl+Enter to open its folder")
            }
        } else {
            match self.rows.get(self.sel).map(|r| r.kind) {
                Some(Kind::Calc) => tr("Enter to copy the result"),
                Some(Kind::Web) => tr("Enter to search the web"),
                Some(Kind::Setting) | Some(Kind::App) => tr("Up and down to move, Enter to open"),
                Some(_) => tr("Enter to open, Ctrl+Enter to open its folder"),
                None => tr("Nothing found"),
            }
        };
        c.text(hint, rect(20.0, fy, w - 200.0, FOOT_H), Font::Small, t.t3, Align::Left);
        let gear = rect(w - 44.0, fy + (FOOT_H - 28.0) / 2.0, 28.0, 28.0);
        if self.sh.files.building.load(std::sync::atomic::Ordering::Relaxed) && self.sh.set.borrow().files {
            c.text(tr("Indexing files…"), rect(gear.x - 170.0, fy, 160.0, FOOT_H), Font::Small, t.t3, Align::Right);
        }
        if self.hover == Some(Hit::Gear) {
            c.fill_round(gear, 14.0, t.hover);
        }
        c.icon(&icons::GEAR, gear.centered(16.0, 16.0), t.t2);
        self.hits.add(gear, Hit::Gear);
    }

    fn on_key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) -> bool {
        match vk {
            0x0D => self.open_selected(mods.ctrl, cx),
            0x1B => self.hide(cx),
            0x26 => self.move_sel(-1, cx),
            0x28 => self.move_sel(1, cx),
            0x09 => self.move_sel(if mods.shift { -1 } else { 1 }, cx),
            // Page up / down
            0x21 => self.move_sel(-(MAX_ROWS as i32 - 1), cx),
            0x22 => self.move_sel(MAX_ROWS as i32 - 1, cx),
            _ => return false,
        }
        true
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_box(c, cx.w, cx.h);
        if let Some(t) = self.shown_at.take() {
            self.show_ms = Some(t.elapsed().as_secs_f64() * 1000.0);
        }
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::EditChanged(text) => {
                if text != self.query {
                    self.query = text;
                    self.refresh(false, cx);
                    self.search_files();
                }
            }
            Event::EditKey { vk, mods } => {
                self.on_key(vk, mods, cx);
            }
            Event::Key { vk, mods, .. } => {
                if self.on_key(vk, mods, cx) {
                    cx.handled();
                }
            }
            Event::MouseMove { p, .. } => {
                let h = self.hits.at(p).cloned().filter(|h| *h != Hit::Field);
                if h != self.hover {
                    self.hover = h;
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                if self.hover.take().is_some() {
                    cx.repaint();
                }
            }
            Event::MouseDown { p, button: Button::Left, mods, .. } => match self.hits.at(p).cloned() {
                Some(Hit::Row(i)) => {
                    self.sel = i;
                    self.open_selected(mods.ctrl, cx);
                }
                Some(Hit::Gear) => self.open_settings(cx),
                Some(Hit::Field) | None => cx.after(|w| w.edit_focus()),
            },
            Event::Wheel { dy, .. } => {
                let n = self.visible_rows();
                if self.rows.len() > n {
                    let max = self.rows.len() - n;
                    self.top = (self.top as i32 - dy.signum() as i32 * 2).clamp(0, max as i32) as usize;
                    cx.repaint();
                }
            }
            Event::Active(true) => {
                self.active_seen = true;
                if self.visible {
                    cx.after(|w| w.edit_focus());
                }
            }
            Event::Active(false) => {
                if self.visible && self.active_seen && !self.quiet_show {
                    sys::test_note("hide: lost focus");
                    self.hide(cx);
                }
            }
            Event::SystemTheme => {
                self.theme = theme_for(&self.sh.set.borrow());
                cx.repaint();
            }
            Event::Scale => cx.repaint(),
            Event::Opened(text) => match text.trim() {
                "settings" => self.open_settings(cx),
                "quit" => self.quit(cx),
                "background" => {}
                _ => {
                    if !self.visible {
                        self.show(true, cx);
                    } else {
                        cx.after(|w| w.bring_to_front());
                    }
                }
            },
            _ => {}
        }
    }

    fn area(&self, _p: Pt) -> Area {
        Area::Client
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Row(_)) | Some(Hit::Gear) => Cursor::Hand,
            _ => Cursor::Arrow,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> apps::App {
        apps::App { name: name.into(), folded: fold(name), target: format!("C:\\menu\\{name}.lnk"), path: None }
    }

    fn titles(v: &[Item]) -> Vec<&str> {
        v.iter().map(|i| i.title.as_str()).collect()
    }

    #[test]
    fn sources_together() {
        let set = Settings::default();
        let apps = vec![app("Notepad"), app("Paint"), app("Notepad++")];
        let found = vec![files::Hit { path: PathBuf::from(r"C:\Users\x\Documents\notes.txt"), name: "notes.txt".into(), dir: false, mtime: 0 }];
        let hist = History::default();
        let r = results("note", &set, &apps, &found, &hist, false, 1000);
        assert_eq!(titles(&r)[..3], ["Notepad", "Notepad++", "notes.txt"]);
        // the web search always comes last
        assert_eq!(r.last().unwrap().kind, Kind::Web);
        assert!(r.last().unwrap().target.ends_with("q=note"));
    }

    #[test]
    fn calculation_first() {
        let set = Settings::default();
        let r = results("12*3", &set, &[app("Calculator")], &[], &History::default(), false, 1000);
        assert_eq!(r[0].kind, Kind::Calc);
        assert_eq!(r[0].title, "36");
    }

    #[test]
    fn picks_come_first() {
        let set = Settings::default();
        let apps = vec![app("Notepad"), app("Notion Calendar")];
        let mut hist = History::default();
        let before = results("no", &set, &apps, &[], &hist, false, 1000);
        assert_eq!(before[0].title, "Notepad");
        let notion = before.iter().find(|i| i.title == "Notion Calendar").unwrap().clone();
        hist.record("no", &notion, 1000);
        let after = results("no", &set, &apps, &[], &hist, false, 1000);
        assert_eq!(after[0].title, "Notion Calendar");
        // the empty box shows it as a recent pick
        let empty = results("", &set, &apps, &[], &hist, false, 1000);
        assert_eq!(titles(&empty), ["Notion Calendar"]);
        // a file picked before shows up even before the file search answers
        let file = Item { kind: Kind::File, title: "budget.xlsx".into(), sub: String::new(), target: r"C:\b\budget.xlsx".into(), icon: String::new() };
        hist.record("bud", &file, 1000);
        let r = results("budg", &set, &apps, &[], &hist, false, 1000);
        assert_eq!(r[0].title, "budget.xlsx");
    }

    #[test]
    fn sources_can_be_turned_off() {
        let set = Settings { apps: false, web: false, settings: false, ..Settings::default() };
        let r = results("display", &set, &[app("Display Tool")], &[], &History::default(), false, 1000);
        assert!(r.is_empty(), "{:?}", titles(&r));
        let on = Settings::default();
        let r = results("display", &on, &[app("Display Tool")], &[], &History::default(), false, 1000);
        assert!(r.iter().any(|i| i.kind == Kind::Setting && i.target == "ms-settings:display"));
    }

    #[test]
    fn folder_shown_from_the_user_folder() {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            let p = PathBuf::from(home).join("Documents").join("a.txt");
            assert_eq!(short_folder(&p), "Documents");
        }
        assert_eq!(short_folder(Path::new(r"D:\Data\x.txt")), r"D:\Data");
    }
}
