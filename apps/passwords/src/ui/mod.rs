//! The window of Plume Passwords: the lock screen, then the vault (sidebar, list, details), the generator, the
//! security report and the settings. Slow work (Argon2, Windows Hello, auto-type) runs on other threads and comes
//! back through the window's sender.

pub mod draw;
mod paint;
mod views;

use crate::{
    autotype::{self, Step},
    clip, csv,
    generator::{self, Strength},
    hello,
    i18n::{fill, tr},
    settings::{self, Settings},
    sys,
    totp::Totp,
    vault::{self, Entry, Kind, Vault},
};
use plume_ui::{Area, Button, Canvas, Cursor, Cx, Event, Gfx, Handler, Hits, PaintCx, Pt, Rect, Theme, Window, dialogs, m};
use std::{
    any::Any,
    cell::Cell,
    collections::HashSet,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::Win32::Foundation::HWND;
use zeroize::{Zeroize, Zeroizing};

pub const TICK: usize = 1;
pub const SIDEBAR_W: f32 = 214.0;
pub const LIST_W: f32 = 330.0;
pub const ROW_H: f32 = 58.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Create,
    Unlock,
    Main,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    All,
    Favorites,
    Kind(Kind),
    Folder(String),
    Generator,
    Report,
    Settings,
}

/// The text fields. One native field at a time sits over the one being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum F {
    Master,
    Master2,
    Search,
    Title,
    User,
    Pass,
    Url,
    Totp,
    Notes,
    Folder,
    Tags,
    Holder,
    Number,
    Expiry,
    Cvv,
    ModalA,
    ModalB,
    ModalC,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Lock,
    Field(F),
    Reveal(F),
    Submit,
    HelloUnlock,
    Nav(View),
    AddFolder,
    New,
    NewKind(Kind),
    Row(String),
    /// A row of the security report: shows that entry.
    Open(String),
    Fav,
    Edit,
    Delete,
    /// Copy a field of the selected entry: "username", "password", "url", "code", "number", "cvv", "notes"…
    Copy(&'static str),
    /// Show or hide a secret of the selected entry.
    Show(&'static str),
    OpenUrl,
    AutoType,
    Save,
    Cancel,
    Generate,
    QrImage,
    FolderChip(String),
    Kind(Kind),
    /// Generator: "mode0", "mode1", "upper", "lower", "digits", "symbols", "lookalikes", "capitalize", "number",
    /// "new", "copy".
    Gen(&'static str),
    Slider(&'static str),
    Choice(&'static str, usize),
    Toggle(&'static str),
    Action(&'static str),
    ModalOk,
    ModalCancel,
    CancelPick,
    Backdrop,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modal {
    Delete(String),
    NewFolder,
    DeleteFolder(String),
    ChangePassword,
    Restore(PathBuf),
}

/// Weak passwords (entry, score) and groups of entries sharing a password.
pub type Report = (Vec<(String, u8)>, Vec<Vec<String>>);

pub struct Form {
    pub e: Entry,
    pub new: bool,
    pub tags: String,
    pub error: String,
    pub reveal: bool,
}

pub struct App {
    pub win: Window,
    pub gfx: Rc<Gfx>,
    pub t: Theme,
    pub set: Settings,
    pub vault: Vault,
    pub screen: Screen,
    pub view: View,
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pressed: Option<Hit>,
    /// Work in progress on another thread ("Unlocking…").
    pub busy: Option<&'static str>,
    pub error: String,
    pub pw1: Zeroizing<String>,
    pub pw2: Zeroizing<String>,
    pub reveal_master: bool,
    pub query: String,
    pub sel: Option<String>,
    pub form: Option<Form>,
    pub modal: Option<Modal>,
    pub modal_a: Zeroizing<String>,
    pub modal_b: Zeroizing<String>,
    pub modal_c: Zeroizing<String>,
    pub modal_error: String,
    pub reveal_modal: bool,
    pub menu_new: bool,
    pub new_btn: Rect,
    /// Secrets of the selected entry shown in clear.
    pub shown: HashSet<&'static str>,
    pub edit_for: Option<F>,
    pub edit_shown: Option<F>,
    pub fields: Vec<(F, Rect)>,
    pub list_scroll: f32,
    pub page_scroll: f32,
    pub page_h: f32,
    pub generated: Zeroizing<String>,
    pub toast: Option<(String, Instant)>,
    /// What we copied (clipboard sequence number) and when to clear it.
    pub clip: Option<(u32, Instant)>,
    pub last_use: Instant,
    /// The window auto-type types into, and its title.
    pub target: Option<(isize, String)>,
    /// The last window in front that was not ours (for the Auto-type button).
    pub last_other: Option<(isize, String)>,
    /// The hotkey found several logins (or none): the list shows these, to choose from.
    pub pick: Option<(String, Vec<String>)>,
    pub pending_autotype: bool,
    pub hello_on: bool,
    pub hello_ok: bool,
    pub autostart: bool,
    pub hotkey_ok: bool,
    pub drag: Option<(&'static str, Rect)>,
    /// Weak and reused passwords, worked out when the report opens.
    pub report: Option<Report>,
    pub log: Vec<String>,
    /// Frames painted (tests wait for the next one).
    pub paints: u64,
    pub keep: Rc<Cell<bool>>,
    pub quitting: bool,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, t: Theme, set: Settings, keep: Rc<Cell<bool>>) -> App {
        let vault = Vault::new(settings::vault_path());
        let screen = if vault.exists() { Screen::Unlock } else { Screen::Create };
        let generated = Zeroizing::new(generator::generate(&set.generator));
        let mut app = App {
            win,
            gfx,
            t,
            vault,
            screen,
            view: View::All,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            busy: None,
            error: String::new(),
            pw1: Zeroizing::new(String::new()),
            pw2: Zeroizing::new(String::new()),
            reveal_master: false,
            query: String::new(),
            sel: None,
            form: None,
            modal: None,
            modal_a: Zeroizing::new(String::new()),
            modal_b: Zeroizing::new(String::new()),
            modal_c: Zeroizing::new(String::new()),
            modal_error: String::new(),
            reveal_modal: false,
            menu_new: false,
            new_btn: Rect::default(),
            shown: HashSet::new(),
            edit_for: Some(F::Master),
            edit_shown: None,
            fields: Vec::new(),
            list_scroll: 0.0,
            page_scroll: 0.0,
            page_h: 0.0,
            generated,
            toast: None,
            clip: None,
            last_use: Instant::now(),
            target: None,
            last_other: None,
            pick: None,
            pending_autotype: false,
            hello_on: settings::hello_path().is_file(),
            hello_ok: false,
            autostart: sys::autostart(),
            hotkey_ok: true,
            drag: None,
            report: None,
            log: Vec::new(),
            paints: 0,
            keep,
            quitting: false,
            set,
        };
        app.update_keep();
        // whether Windows Hello is set up takes a moment to ask: off the interface thread
        let sender = app.win.sender();
        std::thread::spawn(move || {
            let ok = hello::available();
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.hello_ok = ok;
                    cx.repaint();
                }
            });
        });
        app
    }

    pub fn note(&mut self, what: impl Into<String>) {
        if self.log.len() > 300 {
            self.log.remove(0);
        }
        self.log.push(what.into());
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        self.toast = Some((text.into(), Instant::now()));
        cx.timer(TICK, Some(1000));
        cx.repaint();
    }

    pub fn update_keep(&mut self) {
        self.keep.set(self.set.keep_running && !self.quitting);
    }

    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    // ───────────────────────── the vault's contents ─────────────────────────

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.vault.data.entries.iter().find(|e| e.id == id)
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.sel.as_deref().and_then(|id| self.entry(id))
    }

    /// Every folder: the ones made, and the ones entries name.
    pub fn folders(&self) -> Vec<String> {
        let mut f: Vec<String> = self.vault.data.folders.clone();
        for e in &self.vault.data.entries {
            if !e.folder.is_empty() && !f.contains(&e.folder) {
                f.push(e.folder.clone());
            }
        }
        f.sort_by_key(|s| s.to_lowercase());
        f
    }

    /// Indices of the entries the list shows, favorites first, then by title.
    pub fn visible(&self) -> Vec<usize> {
        let q = self.query.trim().to_lowercase();
        let es = &self.vault.data.entries;
        let mut v: Vec<usize> = (0..es.len())
            .filter(|&i| {
                let e = &es[i];
                if let Some((_, ids)) = &self.pick {
                    return ids.contains(&e.id);
                }
                let in_view = match &self.view {
                    View::Favorites => e.favorite,
                    View::Kind(Kind::Totp) => e.kind == Kind::Totp || !e.totp.is_empty(),
                    View::Kind(k) => e.kind == *k,
                    View::Folder(f) => &e.folder == f,
                    _ => true,
                };
                in_view && (q.is_empty() || q.split_whitespace().all(|w| e.haystack().contains(w)))
            })
            .collect();
        v.sort_by(|&a, &b| es[b].favorite.cmp(&es[a].favorite).then_with(|| es[a].title.to_lowercase().cmp(&es[b].title.to_lowercase())));
        v
    }

    pub fn count(&self, view: &View) -> usize {
        let es = &self.vault.data.entries;
        match view {
            View::All => es.len(),
            View::Favorites => es.iter().filter(|e| e.favorite).count(),
            View::Kind(Kind::Totp) => es.iter().filter(|e| e.kind == Kind::Totp || !e.totp.is_empty()).count(),
            View::Kind(k) => es.iter().filter(|e| e.kind == *k).count(),
            View::Folder(f) => es.iter().filter(|e| &e.folder == f).count(),
            _ => 0,
        }
    }

    /// Writes the vault after a change.
    fn changed(&mut self, cx: &mut Cx) {
        self.report = None;
        if let Err(e) = self.vault.save() {
            self.say(fill(tr("Could not save the vault: {e}"), &[&e.to_string()]), cx);
        }
        cx.repaint();
    }

    pub fn select(&mut self, id: Option<String>) {
        if self.sel != id {
            self.shown.clear();
        }
        self.sel = id;
    }

    pub fn go(&mut self, v: View, cx: &mut Cx) {
        if matches!(v, View::Report) {
            self.compute_report();
        }
        self.view = v;
        self.pick = None;
        self.form = None;
        self.menu_new = false;
        self.list_scroll = 0.0;
        self.page_scroll = 0.0;
        if self.edit_for != Some(F::Search) {
            self.edit_for = None;
        }
        cx.repaint();
    }

    pub fn compute_report(&mut self) {
        let es = &self.vault.data.entries;
        let mut weak: Vec<(String, u8)> = es
            .iter()
            .filter(|e| e.kind == Kind::Login && !e.password.is_empty())
            .filter_map(|e| {
                let s = generator::strength(&e.password).0;
                s.weak().then(|| (e.id.clone(), s.0))
            })
            .collect();
        weak.sort_by_key(|w| w.1);
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut seen: HashSet<usize> = HashSet::new();
        for i in 0..es.len() {
            if seen.contains(&i) || es[i].kind != Kind::Login || es[i].password.is_empty() {
                continue;
            }
            let same: Vec<usize> = (i..es.len()).filter(|&j| es[j].kind == Kind::Login && es[j].password == es[i].password).collect();
            if same.len() > 1 {
                seen.extend(same.iter().copied());
                groups.push(same.iter().map(|&j| es[j].id.clone()).collect());
            }
        }
        self.report = Some((weak, groups));
    }

    // ───────────────────────── text fields ─────────────────────────

    pub fn value(&self, f: F) -> String {
        let fe = self.form.as_ref();
        let g = |k: fn(&Entry) -> &String| fe.map(|x| k(&x.e).clone()).unwrap_or_default();
        match f {
            F::Master => self.pw1.to_string(),
            F::Master2 => self.pw2.to_string(),
            F::Search => self.query.clone(),
            F::Title => g(|e| &e.title),
            F::User => g(|e| &e.username),
            F::Pass => g(|e| &e.password),
            F::Url => g(|e| &e.url),
            F::Totp => g(|e| &e.totp),
            F::Notes => g(|e| &e.notes),
            F::Folder => g(|e| &e.folder),
            F::Holder => g(|e| &e.card_holder),
            F::Number => g(|e| &e.card_number),
            F::Expiry => g(|e| &e.card_expiry),
            F::Cvv => g(|e| &e.card_cvv),
            F::Tags => fe.map(|x| x.tags.clone()).unwrap_or_default(),
            F::ModalA => self.modal_a.to_string(),
            F::ModalB => self.modal_b.to_string(),
            F::ModalC => self.modal_c.to_string(),
        }
    }

    fn set_value(&mut self, f: F, mut v: String) {
        if f == F::Notes {
            v = v.replace("\r\n", "\n");
        }
        let put = |s: &mut String, v: String| {
            s.zeroize();
            *s = v;
        };
        match f {
            F::Master => put(&mut self.pw1, v),
            F::Master2 => put(&mut self.pw2, v),
            F::ModalA => put(&mut self.modal_a, v),
            F::ModalB => put(&mut self.modal_b, v),
            F::ModalC => put(&mut self.modal_c, v),
            F::Search => {
                self.query = v;
                self.list_scroll = 0.0;
                // the first match is shown right away
                let vis = self.visible();
                if !vis.is_empty() && self.sel.as_ref().is_none_or(|s| !vis.iter().any(|&i| &self.vault.data.entries[i].id == s)) {
                    let id = self.vault.data.entries[vis[0]].id.clone();
                    self.select(Some(id));
                }
            }
            _ => {
                if let Some(fm) = &mut self.form {
                    let e = &mut fm.e;
                    let s = match f {
                        F::Title => &mut e.title,
                        F::User => &mut e.username,
                        F::Pass => &mut e.password,
                        F::Url => &mut e.url,
                        F::Totp => &mut e.totp,
                        F::Notes => &mut e.notes,
                        F::Folder => &mut e.folder,
                        F::Holder => &mut e.card_holder,
                        F::Number => &mut e.card_number,
                        F::Expiry => &mut e.card_expiry,
                        F::Cvv => &mut e.card_cvv,
                        _ => &mut fm.tags,
                    };
                    put(s, v);
                    fm.error.clear();
                }
            }
        }
    }

    /// Is this field shown as dots?
    pub fn masked(&self, f: F) -> bool {
        match f {
            F::Master | F::Master2 => !self.reveal_master,
            F::Pass | F::Number | F::Cvv => !self.form.as_ref().is_some_and(|fm| fm.reveal),
            F::ModalA | F::ModalB | F::ModalC => matches!(self.modal, Some(Modal::ChangePassword | Modal::Restore(_))) && !self.reveal_modal,
            _ => false,
        }
    }

    /// The fields in the order Tab follows, where the keyboard is now.
    fn order(&self) -> Vec<F> {
        if let Some(m) = &self.modal {
            return match m {
                Modal::ChangePassword => vec![F::ModalA, F::ModalB, F::ModalC],
                Modal::NewFolder | Modal::Restore(_) => vec![F::ModalA],
                _ => vec![],
            };
        }
        match self.screen {
            Screen::Create => return vec![F::Master, F::Master2],
            Screen::Unlock => return vec![F::Master],
            Screen::Main => {}
        }
        match &self.form {
            Some(fm) => form_fields(fm.e.kind).to_vec(),
            None => vec![F::Search],
        }
    }

    fn tab(&mut self, back: bool, cx: &mut Cx) {
        let order = self.order();
        let Some(cur) = self.edit_for else { return };
        if let Some(i) = order.iter().position(|f| *f == cur) {
            let n = order.len();
            self.edit_for = Some(order[if back { (i + n - 1) % n } else { (i + 1) % n }]);
            cx.repaint();
        }
    }

    // ───────────────────────── locking ─────────────────────────

    fn create(&mut self, cx: &mut Cx) {
        if self.busy.is_some() {
            return;
        }
        if self.pw1.chars().count() < 8 {
            self.error = tr("Use at least 8 characters.").into();
        } else if *self.pw1 != *self.pw2 {
            self.error = tr("The two passwords are not the same.").into();
        } else {
            self.error.clear();
            self.busy = Some(tr("Creating your vault…"));
            let pw = self.pw1.clone();
            let path = self.vault.path.clone();
            let sender = self.win.sender();
            std::thread::spawn(move || {
                let mut v = Vault::new(path);
                let r = v.create(&pw).map(|_| v);
                drop(pw);
                sender.send(move |h, cx| {
                    if let Some(app) = with_app(h) {
                        app.unlocked(r, cx);
                    }
                });
            });
        }
        cx.repaint();
    }

    fn unlock(&mut self, cx: &mut Cx) {
        if self.busy.is_some() || self.pw1.is_empty() {
            return;
        }
        self.error.clear();
        self.busy = Some(tr("Unlocking…"));
        let pw = self.pw1.clone();
        let path = self.vault.path.clone();
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let mut v = Vault::new(path);
            let r = v.unlock(&pw).map(|_| v);
            drop(pw);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.unlocked(r, cx);
                }
            });
        });
        cx.repaint();
    }

    fn hello_unlock(&mut self, cx: &mut Cx) {
        if self.busy.is_some() || !self.hello_on {
            return;
        }
        self.error.clear();
        self.busy = Some(tr("Waiting for Windows Hello…"));
        let owner = self.win.hwnd.0 as isize;
        let path = self.vault.path.clone();
        let msg = tr("Unlock Plume Passwords");
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let r = if hello::verify(HWND(owner as _), msg) {
                match hello::key(&settings::hello_path()) {
                    Some(k) => {
                        let mut v = Vault::new(path);
                        v.unlock_with_key(&k).map(|_| v)
                    }
                    None => Err(vault::Error::Corrupt),
                }
            } else {
                Err(vault::Error::Io("hello".into()))
            };
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.unlocked(r, cx);
                }
            });
        });
        cx.repaint();
    }

    fn unlocked(&mut self, r: Result<Vault, vault::Error>, cx: &mut Cx) {
        self.busy = None;
        match r {
            Ok(v) => {
                self.vault = v;
                self.pw1.zeroize();
                self.pw2.zeroize();
                self.reveal_master = false;
                self.screen = Screen::Main;
                self.error.clear();
                self.edit_for = None;
                self.last_use = Instant::now();
                self.note("unlocked");
                cx.timer(TICK, Some(1000));
                if self.pending_autotype {
                    self.pending_autotype = false;
                    self.autotype_match(cx);
                }
            }
            Err(vault::Error::Wrong) => {
                self.error = tr("Wrong master password.").into();
                self.note("wrong password");
                self.edit_for = Some(F::Master);
                self.edit_shown = None;
            }
            Err(vault::Error::Io(e)) if e == "hello" => {
                self.error = tr("Windows Hello did not confirm. Use your master password.").into();
                self.edit_for = Some(F::Master);
            }
            Err(e) => {
                self.error = fill(tr("Could not open the vault: {e}"), &[&e.to_string()]);
                self.note(format!("unlock error {e}"));
            }
        }
        cx.repaint();
    }

    pub fn lock(&mut self, why: &str, cx: &mut Cx) {
        if self.screen != Screen::Main {
            return;
        }
        self.vault.lock();
        self.screen = Screen::Unlock;
        if let Some(mut f) = self.form.take() {
            f.e.wipe();
            f.tags.zeroize();
        }
        self.modal = None;
        for s in [&mut self.modal_a, &mut self.modal_b, &mut self.modal_c, &mut self.pw1, &mut self.pw2] {
            s.zeroize();
        }
        self.query.clear();
        self.shown.clear();
        self.pick = None;
        self.report = None;
        self.menu_new = false;
        self.view = View::All;
        self.edit_for = Some(F::Master);
        self.edit_shown = None;
        self.note(format!("locked ({why})"));
        cx.after(|w| w.edit_hide());
        cx.repaint();
    }

    // ───────────────────────── copying and auto-type ─────────────────────────

    pub fn copy(&mut self, text: &str, what: &str, cx: &mut Cx) {
        if text.is_empty() {
            return;
        }
        let secs = self.set.clear_clipboard_s.max(5);
        #[cfg(feature = "test-hook")]
        if std::env::var_os("PLUME_PASSWORDS_FAKE_CLIPBOARD").is_some() {
            self.note(format!("copy {what} ({} chars)", text.chars().count()));
            self.clip = Some((0, Instant::now() + Duration::from_secs(secs as u64)));
            self.say(fill(tr("Copied. The clipboard clears in {n} s."), &[&secs.to_string()]), cx);
            return;
        }
        match clip::copy_secret(self.win.hwnd, text) {
            Some(seq) => {
                self.note(format!("copy {what}"));
                self.clip = Some((seq, Instant::now() + Duration::from_secs(secs as u64)));
                self.say(fill(tr("Copied. The clipboard clears in {n} s."), &[&secs.to_string()]), cx);
            }
            None => self.say(tr("Windows did not let us copy. Try again."), cx),
        }
    }

    fn clear_clipboard(&mut self) {
        if let Some((seq, _)) = self.clip.take() {
            #[cfg(feature = "test-hook")]
            if std::env::var_os("PLUME_PASSWORDS_FAKE_CLIPBOARD").is_some() {
                self.note("clipboard cleared");
                return;
            }
            if clip::clear_if_ours(self.win.hwnd, seq) {
                self.note("clipboard cleared");
            }
        }
    }

    /// The auto-type shortcut was pressed while `target` (titled `title`) was in front.
    pub fn on_hotkey(&mut self, target: isize, title: String, cx: &mut Cx) {
        if target != 0 && target == self.win.hwnd.0 as isize {
            return;
        }
        self.note(format!("hotkey in \"{title}\""));
        self.target = Some((target, title));
        if !self.vault.unlocked() {
            self.pending_autotype = true;
            self.show_window();
            self.say(tr("Unlock, and the login will be typed."), cx);
            return;
        }
        self.autotype_match(cx);
    }

    fn autotype_match(&mut self, cx: &mut Cx) {
        let Some((_, title)) = self.target.clone() else { return };
        let es = &self.vault.data.entries;
        let mut scored: Vec<(u32, bool, usize)> = es
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == Kind::Login && !(e.password.is_empty() && e.username.is_empty()))
            .map(|(i, e)| (autotype::score(&title, &e.title, &e.url), e.favorite, i))
            .filter(|s| s.0 > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        if scored.len() == 1 || (scored.len() > 1 && scored[0].0 > scored[1].0) {
            let id = es[scored[0].2].id.clone();
            self.note(format!("autotype match {}", es[scored[0].2].title));
            self.autotype(&id, cx);
            return;
        }
        // several, or none: the person chooses
        let ids: Vec<String> = if scored.is_empty() {
            es.iter().filter(|e| e.kind == Kind::Login).map(|e| e.id.clone()).collect()
        } else {
            scored.iter().map(|s| es[s.2].id.clone()).collect()
        };
        self.note(format!("autotype pick among {}", ids.len()));
        self.view = View::All;
        self.form = None;
        self.select(ids.first().cloned());
        self.pick = Some((title, ids));
        self.show_window();
        cx.repaint();
    }

    pub fn autotype(&mut self, id: &str, cx: &mut Cx) {
        let target = self.target.take().or_else(|| self.last_other.clone());
        let Some((h, title)) = target else {
            self.say(tr("Click in the other window's field first, then press the shortcut."), cx);
            return;
        };
        let Some(e) = self.entry(id) else { return };
        let mut steps = Vec::new();
        if !e.username.is_empty() {
            steps.push(Step::Text(Zeroizing::new(e.username.clone())));
            steps.push(Step::Tab);
        }
        steps.push(Step::Text(Zeroizing::new(e.password.clone())));
        if self.set.autotype_enter {
            steps.push(Step::Enter);
        }
        self.pick = None;
        self.last_use = Instant::now();
        let sender = self.win.sender();
        autotype::type_into(HWND(h as _), title, steps, move |ok| {
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.note(if ok { "autotype done" } else { "autotype failed" });
                    if !ok {
                        app.say(tr("Could not type into that window."), cx);
                    }
                }
            });
        });
        cx.repaint();
    }

    pub fn show_window(&self) {
        sys::show(self.win.hwnd, true);
    }

    // ───────────────────────── entries ─────────────────────────

    fn new_entry(&mut self, kind: Kind, cx: &mut Cx) {
        let mut e = Entry::new(kind);
        if let View::Folder(f) = &self.view {
            e.folder = f.clone();
        }
        if kind == Kind::Login {
            e.password = generator::generate(&self.set.generator);
        }
        if self.view == View::Favorites {
            e.favorite = true;
        }
        self.form = Some(Form { e, new: true, tags: String::new(), error: String::new(), reveal: false });
        self.menu_new = false;
        if matches!(self.view, View::Generator | View::Report | View::Settings) {
            self.view = View::All;
        }
        self.edit_for = Some(F::Title);
        self.edit_shown = None;
        cx.repaint();
    }

    fn edit_selected(&mut self, cx: &mut Cx) {
        let Some(e) = self.selected().cloned() else { return };
        let tags = e.tags.join(", ");
        self.form = Some(Form { e, new: false, tags, error: String::new(), reveal: false });
        self.edit_for = Some(F::Title);
        self.edit_shown = None;
        cx.repaint();
    }

    fn save_form(&mut self, cx: &mut Cx) {
        let Some(fm) = &mut self.form else { return };
        let e = &mut fm.e;
        e.title = e.title.trim().to_string();
        e.folder = e.folder.trim().to_string();
        e.tags = fm.tags.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        if e.title.is_empty() {
            e.title = match e.kind {
                Kind::Login => csv::host_of(&e.url),
                Kind::Totp => Totp::parse(&e.totp).map(|t| t.issuer).unwrap_or_default(),
                _ => String::new(),
            };
        }
        if e.title.is_empty() {
            fm.error = tr("Give it a name.").into();
            self.edit_for = Some(F::Title);
            cx.repaint();
            return;
        }
        if !e.totp.trim().is_empty() && Totp::parse(&e.totp).is_none() {
            fm.error = tr("This 2FA key is not valid: paste the secret or the otpauth:// address.").into();
            self.edit_for = Some(F::Totp);
            cx.repaint();
            return;
        }
        e.modified = vault::now();
        let e = e.clone();
        let id = e.id.clone();
        match self.vault.data.entries.iter_mut().find(|x| x.id == id) {
            Some(x) => {
                x.wipe();
                *x = e;
            }
            None => self.vault.data.entries.push(e),
        }
        if let Some(mut f) = self.form.take() {
            f.e.wipe();
        }
        self.edit_for = None;
        self.select(Some(id));
        self.note("saved");
        self.changed(cx);
    }

    fn delete(&mut self, id: &str, cx: &mut Cx) {
        if let Some(i) = self.vault.data.entries.iter().position(|e| e.id == id) {
            let mut e = self.vault.data.entries.remove(i);
            e.wipe();
            let vis = self.visible();
            let next = vis.first().map(|&j| self.vault.data.entries[j].id.clone());
            self.select(next);
            self.note("deleted");
            self.changed(cx);
            self.say(tr("Deleted."), cx);
        }
    }

    fn toggle_fav(&mut self, cx: &mut Cx) {
        let Some(id) = self.sel.clone() else { return };
        if let Some(e) = self.vault.data.entries.iter_mut().find(|e| e.id == id) {
            e.favorite = !e.favorite;
            e.modified = vault::now();
        }
        self.changed(cx);
    }

    fn qr_into_form(&mut self, cx: &mut Cx) {
        let Some(path) = dialogs::pick_files(self.win.hwnd, tr("A picture of the QR code")).into_iter().next() else { return };
        match crate::qr::otpauth_from_file(&path) {
            Ok(uri) => {
                let t = Totp::parse(&uri);
                if let Some(fm) = &mut self.form {
                    fm.e.totp.zeroize();
                    fm.e.totp = uri;
                    if let Some(t) = t {
                        if fm.e.title.is_empty() {
                            fm.e.title = t.issuer.clone();
                        }
                        if fm.e.username.is_empty() {
                            fm.e.username = t.account.clone();
                        }
                    }
                    fm.error.clear();
                }
                self.edit_shown = None;
                self.note("qr read");
                self.say(tr("2FA key read from the picture."), cx);
            }
            Err(e) => {
                self.note(format!("qr: {e}"));
                self.say(
                    match e {
                        "no QR code" => tr("No QR code found in that picture."),
                        "not a 2FA code" => tr("That QR code is not a 2FA key."),
                        _ => tr("Could not read that picture."),
                    },
                    cx,
                );
            }
        }
    }

    // ───────────────────────── import, backup ─────────────────────────

    pub fn import_csv(&mut self, path: &std::path::Path, cx: &mut Cx) {
        let text = match std::fs::read(path) {
            Ok(b) => Zeroizing::new(String::from_utf8_lossy(&b).into_owned()),
            Err(_) => return self.say(tr("Could not read that file."), cx),
        };
        let Some(mut imp) = csv::parse(&text) else {
            return self.say(tr("This CSV file has no password columns."), cx);
        };
        let (mut added, mut dup) = (0, 0);
        for e in imp.entries.drain(..) {
            if self.vault.data.entries.iter().any(|x| csv::same_login(x, &e)) {
                dup += 1;
                let mut e = e;
                e.wipe();
                continue;
            }
            self.vault.data.entries.push(e);
            added += 1;
        }
        for f in imp.folders {
            if !self.vault.data.folders.contains(&f) {
                self.vault.data.folders.push(f);
            }
        }
        self.note(format!("imported {added}, {dup} already there, {} skipped", imp.skipped));
        self.changed(cx);
        self.say(
            fill(
                tr("Imported: {n}. Already in the vault: {d}. Delete the CSV file: it holds your passwords in clear."),
                &[&added.to_string(), &dup.to_string()],
            ),
            cx,
        );
    }

    pub fn export_backup(&mut self, path: &std::path::Path, cx: &mut Cx) {
        let r = self.vault.sealed().and_then(|b| vault::write_atomic(path, &b));
        match r {
            Ok(()) => {
                self.note("backup written");
                self.say(tr("Backup saved. It opens with your master password."), cx);
            }
            Err(e) => self.say(fill(tr("Could not save the backup: {e}"), &[&e.to_string()]), cx),
        }
    }

    fn restore(&mut self, path: PathBuf, cx: &mut Cx) {
        match Vault::read_other(&path, &self.modal_a) {
            Ok(data) => {
                let n = self.vault.data.merge(data);
                self.modal = None;
                self.modal_a.zeroize();
                self.edit_for = None;
                self.note(format!("restored {n}"));
                self.changed(cx);
                self.say(fill(tr("Restored from the backup: {n}."), &[&n.to_string()]), cx);
            }
            Err(vault::Error::Wrong) => self.modal_error = tr("Wrong password for this backup.").into(),
            Err(_) => self.modal_error = tr("This file is not a Plume Passwords backup.").into(),
        }
        cx.repaint();
    }

    fn change_password(&mut self, cx: &mut Cx) {
        if !self.vault.check(&self.modal_a) {
            self.modal_error = tr("The current master password is wrong.").into();
        } else if self.modal_b.chars().count() < 8 {
            self.modal_error = tr("Use at least 8 characters.").into();
        } else if *self.modal_b != *self.modal_c {
            self.modal_error = tr("The two passwords are not the same.").into();
        } else {
            match self.vault.change_password(&self.modal_b) {
                Ok(()) => {
                    // Windows Hello's copy follows the new data key
                    if self.hello_on
                        && let Some(k) = self.vault.key()
                    {
                        self.hello_on = hello::enable(&settings::hello_path(), &k);
                    }
                    self.modal = None;
                    for s in [&mut self.modal_a, &mut self.modal_b, &mut self.modal_c] {
                        s.zeroize();
                    }
                    self.edit_for = None;
                    self.note("master password changed");
                    self.say(tr("Master password changed."), cx);
                }
                Err(e) => self.modal_error = e.to_string(),
            }
        }
        cx.repaint();
    }

    fn set_hello(&mut self, on: bool, cx: &mut Cx) {
        if !on {
            hello::disable(&settings::hello_path());
            self.hello_on = false;
            self.note("hello off");
            cx.repaint();
            return;
        }
        if !self.hello_ok {
            return self.say(tr("Windows Hello is not set up on this PC."), cx);
        }
        let owner = self.win.hwnd.0 as isize;
        let sender = self.win.sender();
        let msg = tr("Turn on Windows Hello for Plume Passwords");
        std::thread::spawn(move || {
            let ok = hello::verify(HWND(owner as _), msg);
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    if ok && let Some(k) = app.vault.key() {
                        app.hello_on = hello::enable(&settings::hello_path(), &k);
                        app.note(if app.hello_on { "hello on" } else { "hello failed" });
                    } else {
                        app.say(tr("Windows Hello did not confirm."), cx);
                    }
                    cx.repaint();
                }
            });
        });
    }

    pub fn apply_hotkey(&mut self) {
        let key = self.set.autotype.then(|| {
            let (_, mods, vk) = self.set.hotkey();
            (mods, vk)
        });
        self.hotkey_ok = sys::set_hotkey(self.win.hwnd, key);
    }

    pub fn settings_changed(&mut self, cx: &mut Cx) {
        crate::i18n::set(&self.set.lang);
        self.t = crate::theme(&self.set);
        self.win.set_dark(self.t.dark);
        self.apply_hotkey();
        self.update_keep();
        self.set.save();
        self.edit_shown = None;
        cx.repaint();
    }

    // ───────────────────────── clicks ─────────────────────────

    fn act(&mut self, hit: Hit, cx: &mut Cx) {
        if !matches!(hit, Hit::New | Hit::NewKind(_)) {
            self.menu_new = false;
        }
        match hit {
            Hit::Min => cx.after(|w| w.minimize()),
            Hit::Max => cx.after(|w| w.toggle_maximize()),
            Hit::Close => cx.after(|w| w.close()),
            Hit::Lock => self.lock("button", cx),
            Hit::Field(f) => {
                self.edit_for = Some(f);
            }
            Hit::Reveal(f) => {
                match f {
                    F::Master | F::Master2 => self.reveal_master = !self.reveal_master,
                    F::ModalA | F::ModalB | F::ModalC => self.reveal_modal = !self.reveal_modal,
                    _ => {
                        if let Some(fm) = &mut self.form {
                            fm.reveal = !fm.reveal;
                        }
                    }
                }
                if let Some(e) = self.edit_for {
                    let masked = self.masked(e);
                    cx.after(move |w| w.edit_mask(masked));
                }
            }
            Hit::Submit => match self.screen {
                Screen::Create => self.create(cx),
                Screen::Unlock => self.unlock(cx),
                Screen::Main => {}
            },
            Hit::HelloUnlock => self.hello_unlock(cx),
            Hit::Nav(v) => self.go(v, cx),
            Hit::AddFolder => self.open_modal(Modal::NewFolder, cx),
            Hit::New => self.menu_new = !self.menu_new,
            Hit::NewKind(k) => self.new_entry(k, cx),
            Hit::Row(id) => {
                if self.form.is_some() {
                    self.form = None;
                    self.edit_for = None;
                }
                self.select(Some(id));
            }
            Hit::Open(id) => {
                self.go(View::All, cx);
                self.select(Some(id));
            }
            Hit::Fav => self.toggle_fav(cx),
            Hit::Edit => self.edit_selected(cx),
            Hit::Delete => {
                if let Some(id) = self.sel.clone() {
                    self.open_modal(Modal::Delete(id), cx);
                }
            }
            Hit::Copy(what) => {
                if let Some(e) = self.selected().cloned() {
                    let mut text = match what {
                        "username" => e.username.clone(),
                        "password" => e.password.clone(),
                        "url" => e.url.clone(),
                        "code" => Totp::parse(&e.totp).map(|t| t.now()).unwrap_or_default(),
                        "number" => e.card_number.chars().filter(|c| !c.is_whitespace()).collect(),
                        "cvv" => e.card_cvv.clone(),
                        "expiry" => e.card_expiry.clone(),
                        "holder" => e.card_holder.clone(),
                        "notes" => e.notes.clone(),
                        _ => String::new(),
                    };
                    self.copy(&text, what, cx);
                    text.zeroize();
                    let mut e = e;
                    e.wipe();
                }
            }
            Hit::Show(what) => {
                if !self.shown.remove(what) {
                    self.shown.insert(what);
                }
            }
            Hit::OpenUrl => {
                if let Some(e) = self.selected() {
                    let u = if e.url.contains("://") { e.url.clone() } else { format!("https://{}", e.url) };
                    if u.starts_with("https://") || u.starts_with("http://") {
                        self.win.open(&u);
                    }
                }
            }
            Hit::AutoType => {
                if let Some(id) = self.sel.clone() {
                    self.autotype(&id, cx);
                }
            }
            Hit::CancelPick => {
                self.pick = None;
                self.target = None;
            }
            Hit::Save => self.save_form(cx),
            Hit::Cancel => {
                if let Some(mut f) = self.form.take() {
                    f.e.wipe();
                }
                self.edit_for = None;
            }
            Hit::Generate => {
                let p = generator::generate(&self.set.generator);
                if let Some(fm) = &mut self.form {
                    fm.e.password.zeroize();
                    fm.e.password = p;
                    fm.reveal = true;
                }
                self.edit_shown = None;
            }
            Hit::QrImage => self.qr_into_form(cx),
            Hit::FolderChip(name) => {
                if let Some(fm) = &mut self.form {
                    fm.e.folder = if fm.e.folder == name { String::new() } else { name };
                }
                self.edit_shown = None;
            }
            Hit::Kind(k) => {
                if let Some(fm) = &mut self.form
                    && fm.new
                {
                    fm.e.kind = k;
                    if k != Kind::Login {
                        fm.e.password.zeroize();
                        fm.e.password.clear();
                    } else if fm.e.password.is_empty() {
                        fm.e.password = generator::generate(&self.set.generator);
                    }
                }
            }
            Hit::Gen(_) | Hit::Slider(_) | Hit::Choice(..) | Hit::Toggle(_) | Hit::Action(_) => self.views_act(hit, cx),
            Hit::ModalOk => self.modal_ok(cx),
            Hit::ModalCancel => self.close_modal(cx),
            Hit::Backdrop => {}
        }
        cx.repaint();
    }

    pub fn open_modal(&mut self, m: Modal, cx: &mut Cx) {
        for s in [&mut self.modal_a, &mut self.modal_b, &mut self.modal_c] {
            s.zeroize();
        }
        self.modal_error.clear();
        self.reveal_modal = false;
        self.edit_for = match m {
            Modal::NewFolder | Modal::ChangePassword | Modal::Restore(_) => Some(F::ModalA),
            _ => None,
        };
        self.edit_shown = None;
        self.modal = Some(m);
        cx.repaint();
    }

    fn close_modal(&mut self, cx: &mut Cx) {
        self.modal = None;
        for s in [&mut self.modal_a, &mut self.modal_b, &mut self.modal_c] {
            s.zeroize();
        }
        self.edit_for = None;
        cx.repaint();
    }

    fn modal_ok(&mut self, cx: &mut Cx) {
        let Some(m) = self.modal.clone() else { return };
        match m {
            Modal::Delete(id) => {
                self.close_modal(cx);
                self.delete(&id, cx);
            }
            Modal::NewFolder => {
                let name = self.modal_a.trim().to_string();
                if name.is_empty() {
                    return;
                }
                if !self.vault.data.folders.contains(&name) {
                    self.vault.data.folders.push(name.clone());
                }
                self.close_modal(cx);
                self.changed(cx);
                self.go(View::Folder(name), cx);
            }
            Modal::DeleteFolder(name) => {
                self.vault.data.folders.retain(|f| f != &name);
                for e in &mut self.vault.data.entries {
                    if e.folder == name {
                        e.folder.clear();
                    }
                }
                self.close_modal(cx);
                self.changed(cx);
                self.go(View::All, cx);
            }
            Modal::ChangePassword => self.change_password(cx),
            Modal::Restore(path) => self.restore(path, cx),
        }
    }

    /// Enter in a field.
    fn submit(&mut self, cx: &mut Cx) {
        if self.modal.is_some() {
            return self.modal_ok(cx);
        }
        match self.screen {
            Screen::Create => {
                if self.edit_for == Some(F::Master) {
                    self.edit_for = Some(F::Master2);
                    cx.repaint();
                } else {
                    self.create(cx);
                }
            }
            Screen::Unlock => self.unlock(cx),
            Screen::Main if self.form.is_some() => self.save_form(cx),
            Screen::Main => {
                // Enter in the search: copy the first match's password
                if self.edit_for == Some(F::Search)
                    && let Some(e) = self.selected().cloned()
                    && !e.password.is_empty()
                {
                    let mut p = e.password.clone();
                    self.copy(&p, "password", cx);
                    p.zeroize();
                }
            }
        }
    }

    fn escape(&mut self, cx: &mut Cx) {
        if self.menu_new {
            self.menu_new = false;
        } else if self.modal.is_some() {
            self.close_modal(cx);
        } else if self.form.is_some() {
            self.act(Hit::Cancel, cx);
        } else if self.pick.is_some() {
            self.act(Hit::CancelPick, cx);
        } else if self.edit_for == Some(F::Search) && !self.query.is_empty() {
            self.query.clear();
            self.edit_shown = None;
        } else if self.screen == Screen::Main {
            self.edit_for = None;
        }
        cx.repaint();
    }

    fn tick(&mut self, cx: &mut Cx) {
        if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(5)) {
            self.toast = None;
        }
        if self.clip.as_ref().is_some_and(|(_, at)| Instant::now() >= *at) {
            self.clear_clipboard();
        }
        if self.screen == Screen::Main {
            let mins = self.set.autolock_min;
            if mins > 0 && self.last_use.elapsed() >= Duration::from_secs(mins as u64 * 60) {
                self.lock("idle", cx);
            }
            // the window in front, when it is not ours: where the Auto-type button types
            let (h, title) = autotype::foreground();
            if !h.is_invalid() && h != self.win.hwnd && autotype::process_of(h) != std::process::id() && !title.is_empty() {
                self.last_other = Some((h.0 as isize, title));
            }
        }
        let busy = self.screen == Screen::Main || self.toast.is_some() || self.clip.is_some();
        if !busy {
            cx.timer(TICK, None);
        }
        cx.repaint();
    }

    /// Quitting for good: the clipboard is cleared if it still holds a secret, the shortcut and icon go away.
    pub fn shutdown(&mut self) {
        self.clear_clipboard();
        sys::set_hotkey(self.win.hwnd, None);
        sys::watch_session(self.win.hwnd, false);
        sys::tray_remove(self.win.hwnd);
        self.vault.lock();
    }
}

/// The fields of the form for each kind, in Tab order.
pub fn form_fields(k: Kind) -> &'static [F] {
    match k {
        Kind::Login => &[F::Title, F::User, F::Pass, F::Url, F::Totp, F::Folder, F::Tags, F::Notes],
        Kind::Totp => &[F::Title, F::User, F::Totp, F::Folder, F::Tags, F::Notes],
        Kind::Note => &[F::Title, F::Folder, F::Tags, F::Notes],
        Kind::Card => &[F::Title, F::Holder, F::Number, F::Expiry, F::Cvv, F::Folder, F::Tags, F::Notes],
    }
}

pub fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Login => tr("Login"),
        Kind::Totp => tr("2FA code"),
        Kind::Note => tr("Secure note"),
        Kind::Card => tr("Card"),
    }
}

pub fn strength_name(s: Strength) -> &'static str {
    match s.0 {
        0 => tr("Very weak"),
        1 => tr("Weak"),
        2 => tr("Fair"),
        3 => tr("Strong"),
        _ => tr("Very strong"),
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_all(c, cx);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        if matches!(ev, Event::MouseDown { .. } | Event::Key { .. } | Event::Wheel { .. } | Event::EditChanged(_) | Event::EditKey { .. }) {
            self.last_use = Instant::now();
        }
        match ev {
            Event::MouseMove { p, .. } => {
                if let Some((name, r)) = self.drag {
                    let v = ((p.x - r.x) / r.w).clamp(0.0, 1.0);
                    self.set_slider(name, v);
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
                if let Some(Hit::Slider(name)) = &h
                    && let Some(r) = self.hits.rect_of(|x| x == &Hit::Slider(name))
                {
                    self.drag = Some((name, r));
                    self.set_slider(name, ((p.x - r.x) / r.w).clamp(0.0, 1.0));
                }
                if !matches!(h, Some(Hit::Field(_) | Hit::Reveal(_) | Hit::Generate | Hit::QrImage | Hit::FolderChip(_))) {
                    // a click outside the fields closes the open one (its text stays)
                    if self.edit_for.is_some() && self.screen == Screen::Main && self.modal.is_none() {
                        self.edit_for = None;
                    }
                }
                self.pressed = h;
                cx.capture(true);
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Right, .. } => {
                if let Some(Hit::Nav(View::Folder(name))) = self.hits.at(p).cloned() {
                    self.open_modal(Modal::DeleteFolder(name), cx);
                }
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                if self.drag.take().is_some() {
                    self.set.save();
                    self.pressed = None;
                    cx.repaint();
                    return;
                }
                let up = self.hits.at(p).cloned();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    self.act(a, cx);
                } else if self.menu_new {
                    self.menu_new = false;
                }
                cx.repaint();
            }
            Event::Wheel { p, dy, .. } => {
                let step = dy * 60.0;
                if self.screen == Screen::Main && self.form.is_none() && matches!(self.view, View::All | View::Favorites | View::Kind(_) | View::Folder(_)) {
                    if p.x < SIDEBAR_W + LIST_W {
                        self.list_scroll = (self.list_scroll - step).max(0.0);
                    }
                } else {
                    self.page_scroll = (self.page_scroll - step).max(0.0);
                }
                cx.repaint();
            }
            Event::EditChanged(text) => {
                if let Some(f) = self.edit_shown {
                    self.set_value(f, text);
                    if (f == F::Master && self.screen == Screen::Create) || matches!(f, F::Pass | F::Search) {
                        cx.repaint();
                    }
                }
            }
            Event::EditKey { vk: 0x0D, .. } => self.submit(cx),
            Event::EditKey { vk: 0x1B, .. } => self.escape(cx),
            Event::EditKey { vk: 0x09, mods } => self.tab(mods.shift, cx),
            Event::EditKey { .. } | Event::EditBlur => {}
            Event::Key { vk, mods, .. } => {
                match (vk, mods.ctrl) {
                    (0x1B, _) => self.escape(cx),
                    (0x0D, _) if self.modal.is_some() => self.modal_ok(cx),
                    (0x46, true) if self.screen == Screen::Main => {
                        self.edit_for = Some(F::Search);
                        self.form = None;
                        if matches!(self.view, View::Generator | View::Report | View::Settings) {
                            self.view = View::All;
                        }
                    }
                    (0x4E, true) if self.screen == Screen::Main => self.new_entry(Kind::Login, cx),
                    (0x4C, true) => self.lock("keyboard", cx),
                    (0x09, _) if self.screen != Screen::Main => self.edit_for = Some(F::Master),
                    _ => return,
                }
                cx.handled();
                cx.repaint();
            }
            Event::Timer(TICK) => self.tick(cx),
            Event::Opened(text) if text == "quit" => {
                self.quitting = true;
                self.update_keep();
                cx.after(|w| w.close());
            }
            Event::Opened(_) => {
                self.show_window();
                cx.repaint();
            }
            Event::SystemTheme => {
                self.t = crate::theme(&self.set);
                self.win.set_dark(self.t.dark);
                self.edit_shown = None;
                cx.repaint();
            }
            Event::Close => self.shutdown(),
            Event::Resized | Event::Scale | Event::Active(_) => cx.repaint(),
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(Hit::Min | Hit::Close) => Area::Control,
            Some(_) if p.y < m::TITLE_H => Area::Control,
            _ if p.y < m::TITLE_H => Area::Caption,
            _ => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Min | Hit::Max | Hit::Close | Hit::Backdrop) | None => Cursor::Arrow,
            Some(Hit::Field(_)) => Cursor::Arrow,
            Some(_) => Cursor::Hand,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
