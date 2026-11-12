//! The main window: tabs, the tool bar, find and replace, the status bar, dialogs and the settings page.

pub mod draw;
mod paint;
mod settings_view;

use crate::{
    codec::{self, Encoding, Eol},
    editor::Editor,
    i18n::{fill, tr},
    layout::Measure,
    model::{self, Shared},
    pads, registry,
    search::{self, Matcher, Query},
    store::{self, Session, TabInfo},
    sys,
    text::{Doc, Kind, Pos, Sel, pos},
};
use plume_ui::{Area, Button, Canvas, Cursor, Cx, EditStyle, Event, Handler, Hits, Mods, PaintCx, Pt, Rect, Window, dialogs};
use std::{
    any::Any,
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

pub const TITLE_H: f32 = 42.0;
pub const TOOL_H: f32 = 46.0;
pub const FIND_H: f32 = 46.0;
pub const STATUS_H: f32 = 30.0;
pub const TOAST_SECS: f32 = 4.5;

pub const BLINK: usize = 1;
pub const SAVE: usize = 2;
pub const TOAST: usize = 3;
pub const TIP: usize = 4;

/// Files bigger than this are not opened (they would not fit in memory comfortably).
pub const MAX_FILE: u64 = 512 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    New,
    Open,
    Save,
    Undo,
    Redo,
    Find,
    Replace,
    Wrap,
    Pad,
    More,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindPart {
    Case,
    Word,
    Regex,
    Prev,
    Next,
    Close,
    Mode,
    One,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Find,
    Replace,
    Goto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pos,
    Zoom,
    Eol,
    Enc,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Tab(usize),
    TabClose(usize),
    NewTab,
    Tool(Tool),
    Find(FindPart),
    Field(Field),
    Status(Status),
    Dialog(usize),
    Toggle(&'static str),
    Choice(&'static str, usize),
    Button(&'static str),
    Back,
    Toast,
    Editor,
    Scrim,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DialogKind {
    /// Close a tab with changes: don't save, cancel, save.
    SaveChanges(u64),
    /// Some characters do not fit ANSI: save anyway, cancel, save as UTF-8. The flag: close the tab after.
    Ansi(u64, bool),
    Goto,
}

pub struct Dialog {
    pub kind: DialogKind,
    pub title: String,
    pub text: String,
    /// Label and kind (0 secondary, 1 primary, 2 red), the primary on the right.
    pub buttons: Vec<(String, u8)>,
}

pub struct Tab {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub enc: Encoding,
    pub eol: Eol,
    pub ed: Editor,
    pub number: u32,
    /// The text version last written to the backup (u64::MAX: none written).
    pub backed: u64,
}

impl Tab {
    pub fn name(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string()),
            None if self.number > 1 => fill(tr("Untitled {n}"), &[&self.number.to_string()]),
            None => tr("Untitled").to_string(),
        }
    }
    pub fn dirty(&self) -> bool {
        self.ed.doc.dirty()
    }
    /// An empty new tab nobody typed in: an opened file takes its place.
    pub fn pristine(&self) -> bool {
        self.path.is_none() && self.ed.doc.is_empty() && !self.ed.doc.can_undo()
    }
}

/// Find and replace.
#[derive(Default)]
pub struct Find {
    pub open: bool,
    pub replace: bool,
    pub q: Query,
    pub with: String,
    pub matcher: Option<Result<Matcher, String>>,
    /// (matches, which one is selected), computed for `count_key`.
    pub count: Option<(usize, usize)>,
    pub count_key: (u64, u64, Pos),
    /// Where the search started (typing in the field searches again from there).
    pub origin: Pos,
}

pub struct App {
    pub win: Window,
    pub sh: Rc<RefCell<Shared>>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub measure: Rc<Measure>,
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub find: Find,
    /// The field being typed in, the one shown, and their rectangles.
    pub field: Option<Field>,
    pub field_shown: Option<Field>,
    pub field_focus: bool,
    pub field_rects: Vec<(Field, Rect)>,
    pub goto_text: String,
    pub dialog: Option<Dialog>,
    pub settings: bool,
    pub settings_scroll: f32,
    pub settings_h: f32,
    pub toast: Option<(String, Instant)>,
    pub tip: Option<(Hit, bool)>,
    pub next_id: u64,
    /// Replacing Windows Notepad: on, and waiting for Windows' answer.
    pub default_on: bool,
    pub default_busy: bool,
    pub session_saved: Option<Session>,
    pub last_title: String,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

/// Reads a file for a tab. Err: what went wrong, for the user.
pub fn read_file(path: &Path) -> Result<codec::Decoded, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        return Err(tr("This is a folder").to_string());
    }
    if meta.len() > MAX_FILE {
        return Err(tr("The file is too big (more than 512 MB)").to_string());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(codec::decode(&bytes))
}

impl App {
    pub fn new(win: Window, sh: Rc<RefCell<Shared>>) -> App {
        let measure = {
            let s = sh.borrow();
            Rc::new(Measure::new(&s.gfx.dw, &Self::family(&s), 14.0 * s.set.zoom))
        };
        let mut app = App {
            win,
            sh,
            tabs: Vec::new(),
            active: 0,
            measure,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            find: Find::default(),
            field: None,
            field_shown: None,
            field_focus: false,
            field_rects: Vec::new(),
            goto_text: String::new(),
            dialog: None,
            settings: false,
            settings_scroll: 0.0,
            settings_h: 0.0,
            toast: None,
            tip: None,
            next_id: 1,
            default_on: false,
            default_busy: false,
            session_saved: None,
            last_title: String::new(),
        };
        app.restore();
        if app.tabs.is_empty() {
            app.new_tab();
        }
        app.default_on = registry::ifeo_on(&registry::read_ifeo(&model::machine_root()));
        app
    }

    fn family(s: &Shared) -> String {
        if s.set.font == "sans" { s.gfx.ui_font.to_string() } else { model::mono_font(&s.gfx).to_string() }
    }

    pub fn theme(&self) -> plume_ui::Theme {
        self.sh.borrow().theme.clone()
    }

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active.min(self.tabs.len() - 1)]
    }
    pub fn tab_mut(&mut self) -> &mut Tab {
        let i = self.active.min(self.tabs.len() - 1);
        &mut self.tabs[i]
    }
    pub fn ed(&mut self) -> &mut Editor {
        &mut self.tab_mut().ed
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        let text = text.into();
        self.sh.borrow_mut().note(format!("toast: {text}"));
        self.toast = Some((text, Instant::now()));
        cx.timer(TOAST, Some(50));
        cx.repaint();
    }

    fn make_editor(&self, doc: Doc) -> Editor {
        let s = self.sh.borrow();
        let mut ed = Editor::new(doc, self.measure.clone());
        ed.wrap = s.set.wrap;
        ed.numbers = s.set.numbers;
        ed
    }

    // ---- tabs and files ----

    fn restore(&mut self) {
        let dir = self.sh.borrow().dir.clone();
        let session = Session::load(&dir);
        let mut keep = Vec::new();
        for info in &session.tabs {
            let text = if info.unsaved {
                store::read_backup(&dir, info.id).or_else(|| info.path.as_ref().and_then(|p| read_file(p).ok()).map(|d| d.text))
            } else {
                info.path.as_ref().and_then(|p| read_file(p).ok()).map(|d| d.text)
            };
            let Some(text) = text else { continue };
            let mut doc = Doc::new(&text);
            if info.unsaved {
                doc.mark_unsaved();
            }
            doc.sel = Sel::at(doc.clamp(pos(info.caret.0, info.caret.1)));
            let mut ed = self.make_editor(doc);
            ed.top = info.top;
            keep.push(info.id);
            self.tabs.push(Tab { id: info.id, path: info.path.clone(), enc: info.enc, eol: info.eol, ed, number: info.number, backed: u64::MAX });
            self.next_id = self.next_id.max(info.id + 1);
        }
        store::sweep_backups(&dir, &keep);
        self.active = session.active.min(self.tabs.len().saturating_sub(1));
        self.session_saved = Some(session);
    }

    pub fn new_tab(&mut self) {
        let used: Vec<u32> = self.tabs.iter().filter(|t| t.path.is_none()).map(|t| t.number).collect();
        let number = (1..).find(|n| !used.contains(n)).unwrap_or(1);
        let ed = self.make_editor(Doc::default());
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab { id, path: None, enc: Encoding::Utf8, eol: Eol::Crlf, ed, number, backed: u64::MAX });
        self.active = self.tabs.len() - 1;
    }

    /// Opens a file in a tab (or goes to its tab). A file that does not exist yet opens empty: it is created when
    /// saved, like Windows Notepad does from the command line.
    pub fn open_path(&mut self, path: &Path, cx: &mut Cx) -> bool {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self.tabs.iter().position(|t| t.path.as_deref().is_some_and(|p| p.as_os_str().eq_ignore_ascii_case(path.as_os_str()))) {
            self.active = i;
            cx.repaint();
            return true;
        }
        let (doc, enc, eol) = if !path.exists() {
            self.say(fill(tr("{name} will be created when you save"), &[&path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()]), cx);
            (Doc::default(), Encoding::Utf8, Eol::Crlf)
        } else {
            match read_file(&path) {
                Ok(d) => {
                    if d.lossy {
                        self.say(fill(tr("Some bytes are not valid {enc}: they show as \u{FFFD}"), &[&d.enc.label()]), cx);
                    }
                    (Doc::new(&d.text), d.enc, d.eol)
                }
                Err(e) => {
                    self.say(fill(tr("Couldn't open the file: {e}"), &[&e]), cx);
                    return false;
                }
            }
        };
        let ed = self.make_editor(doc);
        let id = self.next_id;
        self.next_id += 1;
        let tab = Tab { id, path: Some(path), enc, eol, ed, number: 0, backed: u64::MAX };
        if self.tabs.get(self.active).is_some_and(|t| t.pristine()) {
            self.tabs[self.active] = tab;
        } else {
            self.tabs.push(tab);
            self.active = self.tabs.len() - 1;
        }
        self.sh.borrow_mut().note(format!("opened {}", self.tab().name()));
        self.touch(cx);
        true
    }

    pub fn open_dialog(&mut self, cx: &mut Cx) {
        cx.after(move |w| {
            let files = dialogs::pick_files(w.hwnd, tr("Open"));
            if files.is_empty() {
                return;
            }
            w.with(|h, cx| {
                if let Some(a) = with_app(h) {
                    for f in files {
                        a.open_path(&f, cx);
                    }
                }
            });
        });
    }

    /// Writes the tab to its file. False if it needs a name, or the user must choose (ANSI), or it failed.
    pub fn save_tab(&mut self, i: usize, force_ansi: bool, then_close: bool, cx: &mut Cx) -> bool {
        let Some(tab) = self.tabs.get(i) else { return false };
        let Some(path) = tab.path.clone() else {
            self.save_as(i, then_close, cx);
            return false;
        };
        let (bytes, lossy) = codec::encode(&tab.ed.doc.text(), tab.enc, tab.eol);
        if lossy && !force_ansi {
            let id = tab.id;
            self.dialog = Some(Dialog {
                kind: DialogKind::Ansi(id, then_close),
                title: tr("Some characters can't be saved in ANSI").into(),
                text: tr("They would become \"?\". Save in UTF-8 instead?").into(),
                buttons: vec![(tr("Save anyway").into(), 0), (tr("Cancel").into(), 0), (tr("Save as UTF-8").into(), 1)],
            });
            cx.repaint();
            return false;
        }
        if let Err(e) = std::fs::write(&path, &bytes) {
            self.say(fill(tr("Couldn't save: {e}"), &[&e.to_string()]), cx);
            return false;
        }
        let dir = self.sh.borrow().dir.clone();
        let tab = &mut self.tabs[i];
        tab.ed.doc.mark_saved();
        store::drop_backup(&dir, tab.id);
        tab.backed = u64::MAX;
        let note = format!("saved {} ({}, {})", path.display(), tab.enc.key(), tab.eol.key());
        self.sh.borrow_mut().note(note);
        self.touch(cx);
        if then_close {
            self.remove_tab(i, cx);
        }
        true
    }

    pub fn save_as(&mut self, i: usize, then_close: bool, cx: &mut Cx) {
        let name = self.tabs[i].path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled.txt".into());
        let id = self.tabs[i].id;
        cx.after(move |w| {
            let kinds = [(tr("Text files"), "*.txt"), (tr("All files"), "*.*")];
            let Some(path) = dialogs::save_as(w.hwnd, tr("Save as"), &name, "txt", &kinds) else { return };
            w.with(|h, cx| {
                if let Some(a) = with_app(h)
                    && let Some(i) = a.tabs.iter().position(|t| t.id == id)
                {
                    a.save_to(i, path, then_close, cx);
                }
            });
        });
    }

    /// Save as: the tab takes the new path and is written there.
    pub fn save_to(&mut self, i: usize, path: PathBuf, then_close: bool, cx: &mut Cx) -> bool {
        self.tabs[i].path = Some(path);
        self.tabs[i].number = 0;
        self.save_tab(i, false, then_close, cx)
    }

    pub fn close_tab(&mut self, i: usize, cx: &mut Cx) {
        let Some(tab) = self.tabs.get(i) else { return };
        if tab.dirty() && !tab.ed.doc.is_empty() {
            self.active = i;
            self.dialog = Some(Dialog {
                kind: DialogKind::SaveChanges(tab.id),
                title: fill(tr("Save changes to {name}?"), &[&tab.name()]),
                text: tr("Your changes will be lost if you don't save them.").into(),
                buttons: vec![(tr("Don't save").into(), 0), (tr("Cancel").into(), 0), (tr("Save").into(), 1)],
            });
            cx.repaint();
            return;
        }
        self.remove_tab(i, cx);
    }

    fn remove_tab(&mut self, i: usize, cx: &mut Cx) {
        if i >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(i);
        let dir = self.sh.borrow().dir.clone();
        store::drop_backup(&dir, tab.id);
        if self.tabs.is_empty() {
            self.new_tab();
        }
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
        self.touch(cx);
    }

    /// Something changed: the session is written soon, the window redrawn.
    pub fn touch(&mut self, cx: &mut Cx) {
        self.sh.borrow_mut().arm_save();
        cx.repaint();
    }

    /// Writes the text of tabs not saved yet, and the list of tabs. Called a moment after changes and on close.
    pub fn flush(&mut self) {
        let dir = self.sh.borrow().dir.clone();
        for t in &mut self.tabs {
            let keep = t.dirty() || (t.path.is_none() && !t.ed.doc.is_empty());
            if keep {
                if t.backed != t.ed.doc.version && store::write_backup(&dir, t.id, &t.ed.doc.text()) {
                    t.backed = t.ed.doc.version;
                }
            } else if t.backed != u64::MAX {
                store::drop_backup(&dir, t.id);
                t.backed = u64::MAX;
            }
        }
        let session = Session {
            tabs: self
                .tabs
                .iter()
                .filter(|t| t.path.is_some() || !t.ed.doc.is_empty() || t.dirty())
                .map(|t| TabInfo {
                    id: t.id,
                    path: t.path.clone(),
                    enc: t.enc,
                    eol: t.eol,
                    caret: (t.ed.doc.sel.caret.line, t.ed.doc.sel.caret.col),
                    top: t.ed.top,
                    unsaved: t.dirty() || t.path.is_none(),
                    number: t.number,
                })
                .collect(),
            active: self.active,
        };
        if self.session_saved.as_ref() != Some(&session) && session.save(&dir) {
            self.session_saved = Some(session);
        }
        // the window's place
        if let Some(p) = self.win.placement() {
            let mut s = self.sh.borrow_mut();
            let place = Some((p.x, p.y, p.w, p.h, p.maximized));
            if s.set.placement != place && sys::visible(self.win.hwnd) && !self.win.is_minimized() {
                s.set.placement = place;
                s.set.save(&dir);
            }
        }
        pads::flush(&self.sh);
    }

    /// Changes the encoding the tab is saved in.
    pub fn set_encoding(&mut self, enc: Encoding, cx: &mut Cx) {
        let t = self.tab_mut();
        if t.enc != enc {
            t.enc = enc;
            // the file on disk is no longer what saving would write
            if t.path.is_some() {
                t.ed.doc.mark_unsaved();
            }
        }
        self.touch(cx);
    }

    pub fn set_eol(&mut self, eol: Eol, cx: &mut Cx) {
        let t = self.tab_mut();
        if t.eol != eol {
            t.eol = eol;
            if t.path.is_some() {
                t.ed.doc.mark_unsaved();
            }
        }
        self.touch(cx);
    }

    /// Reads the file again as another encoding (when it was guessed wrong).
    pub fn reopen_as(&mut self, enc: Encoding, cx: &mut Cx) {
        let Some(path) = self.tab().path.clone() else { return };
        match std::fs::read(&path) {
            Ok(bytes) => {
                let d = codec::decode_as(&bytes, enc);
                let t = self.tab_mut();
                t.ed.doc.set_text(&d.text);
                t.ed.doc.mark_saved();
                t.enc = enc;
                t.eol = d.eol;
                t.ed.ensure_visible();
                self.touch(cx);
            }
            Err(e) => self.say(fill(tr("Couldn't open the file: {e}"), &[&e.to_string()]), cx),
        }
    }

    // ---- the view ----

    pub fn zoom(&self) -> f32 {
        self.sh.borrow().set.zoom
    }

    pub fn set_zoom(&mut self, z: f32, cx: &mut Cx) {
        let z = (z * 10.0).round() / 10.0;
        let z = z.clamp(0.5, 4.0);
        {
            let mut s = self.sh.borrow_mut();
            if (s.set.zoom - z).abs() < 0.001 {
                return;
            }
            s.set.zoom = z;
            s.set.save(&s.dir.clone());
        }
        self.rebuild_font();
        cx.repaint();
    }

    pub fn rebuild_font(&mut self) {
        let m = {
            let s = self.sh.borrow();
            Rc::new(Measure::new(&s.gfx.dw, &Self::family(&s), 14.0 * s.set.zoom))
        };
        self.measure = m.clone();
        for t in &mut self.tabs {
            t.ed.m = m.clone();
            t.ed.ensure_visible();
        }
    }

    /// Word wrap, line numbers and the status bar follow the settings.
    pub fn apply_view(&mut self) {
        let (wrap, numbers) = {
            let s = self.sh.borrow();
            (s.set.wrap, s.set.numbers)
        };
        for t in &mut self.tabs {
            t.ed.wrap = wrap;
            t.ed.numbers = numbers;
            if wrap {
                t.ed.sx = 0.0;
            }
            t.ed.ensure_visible();
        }
    }

    pub fn change_setting(&mut self, f: impl FnOnce(&mut store::Settings), cx: &mut Cx) {
        let (old, new) = {
            let mut s = self.sh.borrow_mut();
            let old = s.set.clone();
            f(&mut s.set);
            let dir = s.dir.clone();
            s.set.save(&dir);
            (old, s.set.clone())
        };
        if old.theme != new.theme {
            let t = model::theme_for(&new);
            let dark = t.dark;
            self.sh.borrow_mut().theme = t;
            self.win.set_dark(dark);
            pads::retheme(&self.sh);
        }
        if old.lang != new.lang {
            crate::i18n::set(&new.lang);
            pads::retheme(&self.sh);
        }
        if old.font != new.font || old.zoom != new.zoom {
            self.rebuild_font();
        }
        self.apply_view();
        cx.repaint();
    }

    // ---- find and replace ----

    pub fn open_find(&mut self, replace: bool, cx: &mut Cx) {
        let sel = self.tab().ed.doc.sel;
        // a selection on one line is what to look for
        if !sel.is_empty() && sel.start().line == sel.end().line {
            let s = self.tab().ed.doc.selected();
            if s.len() < 200 {
                self.find.q.text = s;
                self.field_shown = None;
            }
        }
        self.find.open = true;
        self.find.replace = replace;
        self.find.origin = sel.start();
        self.compile();
        self.field = Some(if replace && !self.find.q.text.is_empty() { Field::Replace } else { Field::Find });
        self.field_focus = true;
        self.field_shown = None;
        cx.repaint();
    }

    pub fn close_find(&mut self, cx: &mut Cx) {
        self.find.open = false;
        if matches!(self.field, Some(Field::Find | Field::Replace)) {
            self.field = None;
            self.field_focus = false;
        }
        self.ed().focused = true;
        cx.repaint();
    }

    pub fn compile(&mut self) {
        self.find.matcher = if self.find.q.text.is_empty() { None } else { Some(self.find.q.compile()) };
        self.find.count = None;
    }

    pub fn matcher(&self) -> Option<&Matcher> {
        self.find.matcher.as_ref().and_then(|m| m.as_ref().ok())
    }

    /// Selects the next (or previous) match. From the caret, or from `from`.
    pub fn find_step(&mut self, back: bool, from: Option<Pos>, cx: &mut Cx) -> bool {
        let Some(Ok(m)) = self.find.matcher.take() else {
            cx.repaint();
            return false;
        };
        let i = self.active;
        let ed = &mut self.tabs[i].ed;
        let sel = ed.doc.sel;
        let found = if back { m.prev(&ed.doc, from.unwrap_or(sel.start()), true) } else { m.next(&ed.doc, from.unwrap_or(sel.end()), true) };
        if let Some((a, b)) = found {
            ed.doc.sel = Sel { anchor: a, caret: b };
            ed.doc.seal();
            ed.ensure_visible();
        }
        self.find.matcher = Some(Ok(m));
        if found.is_none() {
            self.say(fill(tr("\"{q}\" not found"), &[&self.find.q.text]), cx);
        }
        cx.repaint();
        found.is_some()
    }

    pub fn replace_one(&mut self, cx: &mut Cx) {
        let Some(Ok(m)) = self.find.matcher.take() else { return };
        let with = self.find.with.clone();
        let i = self.active;
        let ed = &mut self.tabs[i].ed;
        let sel = ed.doc.sel;
        if !sel.is_empty() && m.is_match(&ed.doc, sel.start(), sel.end()) {
            let r = m.replacement(ed.doc.line(sel.start().line), sel.start().col, &with);
            ed.doc.seal();
            ed.doc.replace(sel.start(), sel.end(), &r, Kind::Other);
        }
        self.find.matcher = Some(Ok(m));
        self.find_step(false, None, cx);
        self.touch(cx);
    }

    pub fn replace_all(&mut self, cx: &mut Cx) {
        let Some(Ok(m)) = self.find.matcher.take() else { return };
        let with = self.find.with.clone();
        let n = search::replace_all(&mut self.tab_mut().ed.doc, &m, &with);
        self.find.matcher = Some(Ok(m));
        self.ed().ensure_visible();
        self.say(if n == 1 { tr("1 replacement").to_string() } else { fill(tr("{n} replacements"), &[&n.to_string()]) }, cx);
        self.touch(cx);
    }

    /// "3 of 12", computed again when the text, the search or the selection changed.
    pub fn match_count(&mut self) -> Option<(usize, usize)> {
        let m = self.find.matcher.as_ref()?.as_ref().ok()?;
        let t = self.tab();
        if t.ed.doc.chars() > 40_000_000 {
            return None;
        }
        let key = (
            t.ed.doc.version ^ (t.id << 40),
            self.find.q.text.len() as u64 ^ ((self.find.q.case as u64) << 1 | (self.find.q.word as u64) << 2 | (self.find.q.regex as u64) << 3) << 32,
            t.ed.doc.sel.start(),
        );
        if self.find.count.is_none() || self.find.count_key != key {
            self.find.count = Some(m.count(&t.ed.doc, t.ed.doc.sel.start()));
            self.find.count_key = key;
        }
        self.find.count
    }

    /// The text of a field changed (typing in the native field).
    pub fn field_changed(&mut self, f: Field, s: String, cx: &mut Cx) {
        match f {
            Field::Find => {
                self.find.q.text = s;
                self.compile();
                let origin = self.find.origin;
                if self.matcher().is_some() {
                    let ed = &mut self.tabs[self.active].ed;
                    ed.doc.sel = Sel::at(origin);
                    // typing a search does not say "not found" at every letter: the count says it
                    let toast = self.toast.take();
                    self.find_step(false, Some(origin), cx);
                    self.toast = toast;
                }
            }
            Field::Replace => self.find.with = s,
            Field::Goto => self.goto_text = s,
        }
        cx.repaint();
    }

    pub fn open_goto(&mut self, cx: &mut Cx) {
        let c = self.tab().ed.doc.sel.caret;
        self.goto_text = (c.line + 1).to_string();
        self.dialog = Some(Dialog {
            kind: DialogKind::Goto,
            title: tr("Go to line").into(),
            text: fill(tr("Line number (1 to {n})"), &[&self.tab().ed.doc.line_count().to_string()]),
            buttons: vec![(tr("Cancel").into(), 0), (tr("Go").into(), 1)],
        });
        self.field = Some(Field::Goto);
        self.field_focus = true;
        self.field_shown = None;
        cx.repaint();
    }

    /// "12" or "12:5" (line, column).
    pub fn goto(&mut self, text: &str, cx: &mut Cx) -> bool {
        let mut parts = text.trim().split([':', ',']);
        let Some(line) = parts.next().and_then(|l| l.trim().parse::<usize>().ok()).filter(|&l| l >= 1) else {
            self.say(tr("Type a line number"), cx);
            return false;
        };
        let col = parts.next().and_then(|c| c.trim().parse::<usize>().ok()).unwrap_or(1).max(1);
        let ed = self.ed();
        let line = (line - 1).min(ed.doc.line_count() - 1);
        let s = ed.doc.line(line);
        let byte = s.char_indices().nth(col - 1).map_or(s.len(), |(i, _)| i);
        ed.doc.sel = Sel::at(pos(line, byte));
        ed.doc.seal();
        // the line in the middle of the screen
        let vis = ed.visible_rows() as isize;
        ed.top = ed.offset_rows(ed.vpos(ed.doc.sel.caret), -(vis / 3));
        ed.ensure_visible();
        self.close_dialog(cx);
        true
    }

    pub fn close_dialog(&mut self, cx: &mut Cx) {
        if self.dialog.as_ref().is_some_and(|d| d.kind == DialogKind::Goto) && self.field == Some(Field::Goto) {
            self.field = if self.find.open { Some(Field::Find) } else { None };
            self.field_focus = false;
            self.field_shown = None;
        }
        self.dialog = None;
        cx.repaint();
    }

    pub fn dialog_choose(&mut self, i: usize, cx: &mut Cx) {
        let Some(d) = self.dialog.take() else { return };
        let tab = |a: &App, id: u64| a.tabs.iter().position(|t| t.id == id);
        match d.kind {
            DialogKind::SaveChanges(id) => match i {
                0 => {
                    if let Some(t) = tab(self, id) {
                        self.remove_tab(t, cx);
                    }
                }
                2 => {
                    if let Some(t) = tab(self, id) {
                        self.save_tab(t, false, true, cx);
                    }
                }
                _ => {}
            },
            DialogKind::Ansi(id, close) => match i {
                0 => {
                    if let Some(t) = tab(self, id) {
                        self.save_tab(t, true, close, cx);
                    }
                }
                2 => {
                    if let Some(t) = tab(self, id) {
                        self.tabs[t].enc = Encoding::Utf8;
                        self.save_tab(t, false, close, cx);
                    }
                }
                _ => {}
            },
            DialogKind::Goto => {
                if i == 1 {
                    self.dialog = Some(d);
                    let text = self.win.edit_text().unwrap_or_else(|| self.goto_text.clone());
                    self.goto(&text, cx);
                    return;
                }
                self.dialog = Some(d);
                self.close_dialog(cx);
                return;
            }
        }
        cx.repaint();
    }

    // ---- commands ----

    pub fn tool(&mut self, t: Tool, cx: &mut Cx) {
        match t {
            Tool::New => {
                self.new_tab();
                self.touch(cx);
            }
            Tool::Open => self.open_dialog(cx),
            Tool::Save => {
                let i = self.active;
                self.save_tab(i, false, false, cx);
            }
            Tool::Undo => {
                self.ed().doc.undo();
                self.ed().ensure_visible();
                self.touch(cx);
            }
            Tool::Redo => {
                self.ed().doc.redo();
                self.ed().ensure_visible();
                self.touch(cx);
            }
            Tool::Find => {
                if self.find.open && !self.find.replace {
                    self.close_find(cx);
                } else {
                    self.open_find(false, cx);
                }
            }
            Tool::Replace => {
                if self.find.open && self.find.replace {
                    self.close_find(cx);
                } else {
                    self.open_find(true, cx);
                }
            }
            Tool::Wrap => self.change_setting(|s| s.wrap = !s.wrap, cx),
            Tool::Pad => self.new_pad(cx),
            Tool::More => self.more_menu(cx),
            Tool::Settings => self.open_settings(!self.settings, cx),
        }
    }

    pub fn new_pad(&mut self, cx: &mut Cx) {
        let sh = self.sh.clone();
        let activate = !sh.borrow().noactivate;
        cx.after(move |_| {
            pads::new_pad(&sh, activate);
        });
        self.touch(cx);
    }

    pub fn open_settings(&mut self, on: bool, cx: &mut Cx) {
        self.settings = on;
        self.settings_scroll = 0.0;
        if on {
            self.default_on = registry::ifeo_on(&registry::read_ifeo(&model::machine_root()));
            if matches!(self.field, Some(Field::Find | Field::Replace)) {
                self.field = None;
            }
        }
        cx.repaint();
    }

    /// The "…" menu of the tool bar.
    fn more_menu(&mut self, cx: &mut Cx) {
        let r = self.hits.rect_of(|h| *h == Hit::Tool(Tool::More)).unwrap_or_default();
        let at = self.win.to_screen(Pt { x: r.x, y: r.bottom() + 4.0 });
        let (wrap, numbers, status) = {
            let s = self.sh.borrow();
            (s.set.wrap, s.set.numbers, s.set.status)
        };
        let check = |mut i: sys::Item, on: bool| {
            i.checked = on;
            i
        };
        let items = vec![
            sys::item(1, format!("{}\tCtrl+Shift+S", tr("Save as…"))),
            sys::item(2, tr("Save all")),
            sys::sep(),
            sys::item(3, format!("{}\tCtrl+G", tr("Go to line…"))),
            sys::item(4, format!("{}\tF5", tr("Insert time and date"))),
            sys::sep(),
            check(sys::item(5, format!("{}\tAlt+Z", tr("Word wrap"))), wrap),
            check(sys::item(6, tr("Line numbers")), numbers),
            check(sys::item(7, tr("Status bar")), status),
            sys::sep(),
            sys::item(8, format!("{}\tCtrl++", tr("Zoom in"))),
            sys::item(9, format!("{}\tCtrl+-", tr("Zoom out"))),
            sys::item(10, format!("{}\tCtrl+0", tr("Reset zoom"))),
            sys::sep(),
            sys::item(11, format!("{}\tCtrl+Alt+N", tr("New sticky pad"))),
            sys::item(12, tr("Settings")),
        ];
        cx.after(move |w| {
            let cmd = sys::popup(w.hwnd, &items, Some(at), false);
            w.with(|h, cx| {
                if let Some(a) = with_app(h) {
                    a.menu_command(cmd, cx);
                }
            });
        });
    }

    pub fn menu_command(&mut self, cmd: u32, cx: &mut Cx) {
        match cmd {
            1 => {
                let i = self.active;
                self.save_as(i, false, cx);
            }
            2 => {
                for i in 0..self.tabs.len() {
                    if self.tabs[i].dirty() && self.tabs[i].path.is_some() {
                        self.save_tab(i, false, false, cx);
                    }
                }
            }
            3 => self.open_goto(cx),
            4 => self.insert_date(),
            5 => self.change_setting(|s| s.wrap = !s.wrap, cx),
            6 => self.change_setting(|s| s.numbers = !s.numbers, cx),
            7 => self.change_setting(|s| s.status = !s.status, cx),
            8 => self.set_zoom(self.zoom() + 0.1, cx),
            9 => self.set_zoom(self.zoom() - 0.1, cx),
            10 => self.set_zoom(1.0, cx),
            11 => self.new_pad(cx),
            12 => self.open_settings(true, cx),
            _ => {}
        }
        cx.repaint();
    }

    /// F5: the time and date, like Windows Notepad.
    pub fn insert_date(&mut self) {
        use windows::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
        let t: SYSTEMTIME = unsafe { GetLocalTime() };
        let s = if crate::i18n::french() {
            format!("{:02}:{:02} {:02}/{:02}/{}", t.wHour, t.wMinute, t.wDay, t.wMonth, t.wYear)
        } else {
            let h = if t.wHour.is_multiple_of(12) { 12 } else { t.wHour % 12 };
            format!("{}:{:02} {} {}/{}/{}", h, t.wMinute, if t.wHour < 12 { "AM" } else { "PM" }, t.wMonth, t.wDay, t.wYear)
        };
        self.ed().type_text(&s, Kind::Other);
    }

    /// The encoding menu of the status bar.
    fn enc_menu(&mut self, cx: &mut Cx) {
        let r = self.hits.rect_of(|h| *h == Hit::Status(Status::Enc)).unwrap_or_default();
        let at = self.win.to_screen(Pt { x: r.x, y: r.y });
        let (cur, has_path) = (self.tab().enc, self.tab().path.is_some());
        let mut items = vec![];
        for (k, e) in codec::ENCODINGS.iter().enumerate() {
            let mut it = sys::item(100 + k as u32, fill(tr("Save as {enc}"), &[&e.label()]));
            it.checked = *e == cur;
            items.push(it);
        }
        items.push(sys::sep());
        for (k, e) in codec::ENCODINGS.iter().enumerate() {
            let mut it = sys::item(200 + k as u32, fill(tr("Reopen as {enc}"), &[&e.label()]));
            it.disabled = !has_path;
            items.push(it);
        }
        cx.after(move |w| {
            let (x, y) = at;
            let cmd = sys::popup(w.hwnd, &items, Some((x, y - 8)), false);
            w.with(|h, cx| {
                if let Some(a) = with_app(h) {
                    match cmd {
                        100..=104 => a.set_encoding(codec::ENCODINGS[(cmd - 100) as usize], cx),
                        200..=204 => a.reopen_as(codec::ENCODINGS[(cmd - 200) as usize], cx),
                        _ => {}
                    }
                }
            });
        });
    }

    fn eol_menu(&mut self, cx: &mut Cx) {
        let r = self.hits.rect_of(|h| *h == Hit::Status(Status::Eol)).unwrap_or_default();
        let at = self.win.to_screen(Pt { x: r.x, y: r.y });
        let cur = self.tab().eol;
        let items: Vec<sys::Item> = codec::EOLS
            .iter()
            .enumerate()
            .map(|(k, e)| {
                let mut it = sys::item(300 + k as u32, e.label());
                it.checked = *e == cur;
                it
            })
            .collect();
        cx.after(move |w| {
            let cmd = sys::popup(w.hwnd, &items, Some((at.0, at.1 - 8)), false);
            w.with(|h, cx| {
                if let Some(a) = with_app(h)
                    && (300..303).contains(&cmd)
                {
                    a.set_eol(codec::EOLS[(cmd - 300) as usize], cx);
                }
            });
        });
    }

    /// The switch "Open Plume Notepad instead of Windows Notepad". Windows asks the administrator (a test build
    /// writes on its test root instead, without asking).
    pub fn set_default(&mut self, on: bool, cx: &mut Cx) {
        if self.default_busy {
            return;
        }
        #[cfg(feature = "test-hook")]
        {
            let root = model::machine_root();
            let plan = registry::ifeo_plan(on, &registry::read_ifeo(&root), &registry::debugger_cmd(&model::exe()));
            let ok = registry::apply(&root, &plan).is_ok();
            self.sh.borrow_mut().note(format!("ifeo {} ops={} ok={ok}", if on { "on" } else { "off" }, plan.len()));
            self.default_done(ok, on, cx);
        }
        #[cfg(not(feature = "test-hook"))]
        {
            self.default_busy = true;
            let sender = cx.sender();
            std::thread::spawn(move || {
                let r = crate::set_default_elevated(on);
                sender.send(move |h, cx| {
                    if let Some(a) = with_app(h) {
                        a.default_busy = false;
                        match r {
                            Some(ok) => a.default_done(ok, on, cx),
                            None => a.say(tr("Windows did not allow it"), cx),
                        }
                    }
                });
            });
            cx.repaint();
        }
    }

    fn default_done(&mut self, ok: bool, on: bool, cx: &mut Cx) {
        self.default_on = registry::ifeo_on(&registry::read_ifeo(&model::machine_root()));
        let msg = match (ok, on) {
            (true, true) => tr("Windows now opens Plume Notepad instead of Notepad"),
            (true, false) => tr("Windows Notepad is back"),
            (false, _) => tr("Couldn't change it"),
        };
        self.say(msg, cx);
    }

    /// "Choose for text files": Plume Notepad is listed for .txt and the like, then Windows' Default apps opens
    /// (Windows lets only the user pick the default).
    pub fn default_apps(&mut self, cx: &mut Cx) {
        let ok = registry::apply(&model::user_root(), &registry::assoc_plan(true, &model::exe())).is_ok();
        sys::assoc_changed();
        #[cfg(not(feature = "test-hook"))]
        {
            if !self.win.open("ms-settings:defaultapps?registeredAppUser=Plume%20Notepad") {
                self.win.open("ms-settings:defaultapps");
            }
        }
        self.sh.borrow_mut().note(format!("assoc registered ok={ok}"));
        self.say(tr("Choose Plume Notepad for .txt in Windows Settings"), cx);
    }

    pub fn set_autostart(&mut self, on: bool, cx: &mut Cx) {
        let ok = registry::apply(&model::user_root(), &registry::run_plan(on, &model::exe())).is_ok();
        if !ok {
            self.say(tr("Couldn't change it"), cx);
        }
        pads::refresh_tray(&self.sh);
        cx.repaint();
    }

    // ---- keys ----

    pub fn key(&mut self, vk: u16, m: Mods, cx: &mut Cx) {
        let ctrl = m.ctrl && !m.alt;
        if self.dialog.is_some() {
            match vk {
                0x1B => {
                    let goto = self.dialog.as_ref().is_some_and(|d| d.kind == DialogKind::Goto);
                    if goto {
                        self.close_dialog(cx);
                    } else {
                        self.dialog = None;
                    }
                }
                0x0D => {
                    let n = self.dialog.as_ref().map_or(0, |d| d.buttons.len());
                    self.dialog_choose(n.saturating_sub(1), cx);
                }
                _ => {}
            }
            cx.handled();
            cx.repaint();
            return;
        }
        let before = self.tab().ed.doc.version;
        let mut done = true;
        match vk {
            0x4E if ctrl && m.shift => self.new_pad(cx),
            0x4E if ctrl => self.tool(Tool::New, cx),
            0x4F if ctrl => self.tool(Tool::Open, cx),
            0x53 if ctrl && m.shift => {
                let i = self.active;
                self.save_as(i, false, cx);
            }
            0x53 if ctrl => self.tool(Tool::Save, cx),
            0x57 if ctrl => {
                let i = self.active;
                self.close_tab(i, cx);
            }
            0x46 if ctrl => self.open_find(false, cx),
            0x48 if ctrl => self.open_find(true, cx),
            0x47 if ctrl => self.open_goto(cx),
            0x72 => {
                if self.find.q.text.is_empty() {
                    self.open_find(false, cx);
                } else {
                    self.find_step(m.shift, None, cx);
                }
            }
            0x74 => self.insert_date(),
            0x5A if m.alt && !m.ctrl => self.change_setting(|s| s.wrap = !s.wrap, cx),
            0x09 if ctrl => {
                let n = self.tabs.len();
                self.active = if m.shift { (self.active + n - 1) % n } else { (self.active + 1) % n };
            }
            0xBB | 0x6B if ctrl => self.set_zoom(self.zoom() + 0.1, cx),
            0xBD | 0x6D if ctrl => self.set_zoom(self.zoom() - 0.1, cx),
            0x30 | 0x60 if ctrl => self.set_zoom(1.0, cx),
            0xBC if ctrl => self.open_settings(!self.settings, cx),
            0x1B if self.settings => self.open_settings(false, cx),
            0x1B if self.find.open => self.close_find(cx),
            _ => done = false,
        }
        if !done && !self.settings {
            let win = self.win;
            done = self.ed().key(vk, m, win);
        }
        if done {
            cx.handled();
            cx.timer(BLINK, Some(530));
            if self.tab().ed.doc.version != before {
                self.touch(cx);
            }
            cx.repaint();
        }
    }

    pub fn char(&mut self, ch: char, cx: &mut Cx) {
        if self.dialog.is_some() || self.settings {
            return;
        }
        self.ed().char(ch);
        cx.timer(BLINK, Some(530));
        self.touch(cx);
    }

    /// The window's one native text field, on the field being typed in.
    fn place_field(&mut self) {
        let t = self.theme();
        let want = self.field.and_then(|f| self.field_rects.iter().find(|(g, _)| *g == f).map(|(_, r)| (f, *r)));
        match want {
            Some((f, r)) => {
                let inner = Rect { x: r.x + 12.0, y: r.y + 6.0, w: r.w - 24.0, h: r.h - 12.0 };
                if self.field_shown != Some(f) {
                    let text = match f {
                        Field::Find => self.find.q.text.clone(),
                        Field::Replace => self.find.with.clone(),
                        Field::Goto => self.goto_text.clone(),
                    };
                    let len = text.encode_utf16().count();
                    let style = EditStyle {
                        fg: t.t1,
                        bg: t.fill.over(if f == Field::Goto { t.solid } else { t.bg }),
                        size: 13.0,
                        select: Some((0, len)),
                        placeholder: match f {
                            Field::Find => tr("Find").to_string(),
                            Field::Replace => tr("Replace with").to_string(),
                            Field::Goto => String::new(),
                        },
                        password: false,
                        multiline: false,
                    };
                    self.win.edit_show(inner, &text, &style);
                    if self.field_focus {
                        self.win.edit_focus();
                    } else {
                        unsafe {
                            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(self.win.hwnd));
                        }
                    }
                    self.field_shown = Some(f);
                } else {
                    self.win.edit_place(inner);
                }
            }
            None => {
                if self.field_shown.is_some() {
                    self.win.edit_hide();
                    self.field_shown = None;
                }
            }
        }
    }

    fn click(&mut self, hit: Hit, p: Pt, clicks: u8, m: Mods, cx: &mut Cx) {
        match hit {
            Hit::Min => self.win.minimize(),
            Hit::Max => self.win.toggle_maximize(),
            Hit::Close => self.win.close(),
            Hit::Tab(i) => {
                self.active = i;
                self.settings = false;
                self.touch(cx);
            }
            Hit::TabClose(i) => self.close_tab(i, cx),
            Hit::NewTab => {
                self.new_tab();
                self.settings = false;
                self.touch(cx);
            }
            Hit::Tool(t) => self.tool(t, cx),
            Hit::Find(part) => match part {
                FindPart::Case => {
                    self.find.q.case = !self.find.q.case;
                    self.compile();
                }
                FindPart::Word => {
                    self.find.q.word = !self.find.q.word;
                    self.compile();
                }
                FindPart::Regex => {
                    self.find.q.regex = !self.find.q.regex;
                    self.compile();
                }
                FindPart::Prev => {
                    self.find_step(true, None, cx);
                }
                FindPart::Next => {
                    self.find_step(false, None, cx);
                }
                FindPart::Close => self.close_find(cx),
                FindPart::Mode => {
                    self.find.replace = !self.find.replace;
                    if !self.find.replace && self.field == Some(Field::Replace) {
                        self.field = Some(Field::Find);
                    }
                }
                FindPart::One => self.replace_one(cx),
                FindPart::All => self.replace_all(cx),
            },
            Hit::Field(f) => {
                self.field = Some(f);
                self.field_focus = true;
                if self.field_shown == Some(f) {
                    self.win.edit_focus();
                }
                self.ed().focused = false;
            }
            Hit::Status(s) => match s {
                Status::Pos => self.open_goto(cx),
                Status::Zoom => self.set_zoom(1.0, cx),
                Status::Eol => self.eol_menu(cx),
                Status::Enc => self.enc_menu(cx),
            },
            Hit::Dialog(i) => self.dialog_choose(i, cx),
            Hit::Scrim => {}
            Hit::Toggle(name) => self.toggle(name, cx),
            Hit::Choice(name, i) => self.choice(name, i, cx),
            Hit::Button(name) => self.button(name, cx),
            Hit::Back => self.open_settings(false, cx),
            Hit::Toast => self.toast = None,
            Hit::Editor => {
                self.field_focus = false;
                let win = self.win;
                let ed = self.ed();
                ed.focused = true;
                if ed.mouse_down(p, clicks, m) {
                    cx.capture(true);
                }
                let _ = win;
                cx.timer(BLINK, Some(530));
            }
        }
        cx.repaint();
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_all(c, cx);
        self.place_field();
        let title = format!("{}{} - Plume Notepad", if self.tab().dirty() { "• " } else { "" }, self.tab().name());
        if title != self.last_title {
            self.win.set_title(&title);
            self.last_title = title;
        }
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                if self.tab().ed.dragging() {
                    if self.ed().mouse_move(p) {
                        cx.repaint();
                    }
                    return;
                }
                let h = self.hits.at(p).cloned();
                let on_bar = self.ed().mouse_move(p);
                if h != self.hover || on_bar {
                    if matches!(h, Some(Hit::Tool(_))) && h != self.hover {
                        self.tip = h.clone().map(|h| (h, false));
                        cx.timer(TIP, Some(600));
                    } else if !matches!(h, Some(Hit::Tool(_))) {
                        self.tip = None;
                    }
                    self.hover = h;
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                self.hover = None;
                self.tip = None;
                cx.repaint();
            }
            Event::MouseDown { p, button, clicks, mods } => {
                self.tip = None;
                let h = self.hits.at(p).cloned();
                if button == Button::Middle {
                    if let Some(Hit::Tab(i)) = h {
                        self.close_tab(i, cx);
                    }
                    return;
                }
                if button != Button::Left {
                    return;
                }
                self.pressed = h.clone();
                if let Some(h) = h {
                    match h {
                        // the editor reacts at once, the others on release
                        Hit::Editor | Hit::Field(_) => self.click(h, p, clicks, mods, cx),
                        Hit::Tab(i) => self.click(Hit::Tab(i), p, clicks, mods, cx),
                        _ => {}
                    }
                }
                cx.repaint();
            }
            Event::MouseUp { p, button, mods } => {
                if self.tab().ed.dragging() {
                    self.ed().mouse_up();
                    cx.capture(false);
                    self.pressed = None;
                    cx.repaint();
                    return;
                }
                if button != Button::Left {
                    return;
                }
                let h = self.hits.at(p).cloned();
                let pressed = self.pressed.take();
                if let Some(h) = h
                    && Some(&h) == pressed.as_ref()
                    && !matches!(h, Hit::Editor | Hit::Field(_) | Hit::Tab(_))
                {
                    self.click(h, p, 1, mods, cx);
                }
                cx.repaint();
            }
            Event::Wheel { p, dx, dy, mods } => {
                if self.settings {
                    self.settings_scroll = (self.settings_scroll - dy * 60.0).clamp(0.0, self.settings_h.max(0.0));
                } else if mods.ctrl {
                    let z = self.sh.borrow().set.zoom + if dy > 0.0 { 0.1 } else { -0.1 };
                    self.set_zoom(z, cx);
                } else if self.tab().ed.area.contains(p) || self.dialog.is_none() {
                    self.ed().wheel(dx, dy, mods.shift);
                }
                cx.repaint();
            }
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::Char(ch) => self.char(ch, cx),
            Event::EditChanged(s) => {
                if let Some(f) = self.field {
                    self.field_changed(f, s, cx);
                }
            }
            Event::EditKey { vk, mods } => match (vk, self.field) {
                (0x0D, Some(Field::Goto)) => {
                    let text = self.win.edit_text().unwrap_or_default();
                    self.goto(&text, cx);
                }
                (0x1B, Some(Field::Goto)) => self.close_dialog(cx),
                (0x0D, Some(Field::Find)) => {
                    self.find_step(mods.shift, None, cx);
                }
                (0x0D, Some(Field::Replace)) => {
                    if mods.ctrl {
                        self.replace_all(cx);
                    } else {
                        self.replace_one(cx);
                    }
                }
                (0x26, Some(Field::Find)) => {
                    self.find_step(true, None, cx);
                }
                (0x28, Some(Field::Find)) => {
                    self.find_step(false, None, cx);
                }
                (0x09, Some(Field::Find)) if self.find.replace => {
                    self.field = Some(Field::Replace);
                    self.field_shown = None;
                    cx.repaint();
                }
                (0x09, Some(Field::Replace)) => {
                    self.field = Some(Field::Find);
                    self.field_shown = None;
                    cx.repaint();
                }
                (0x09, _) => {
                    // back to the text
                    self.field_focus = false;
                    self.win.edit_hide();
                    self.field_shown = None;
                    self.ed().focused = true;
                    cx.repaint();
                }
                (0x1B, _) => self.close_find(cx),
                _ => {}
            },
            Event::EditBlur => {
                self.field_focus = false;
                self.ed().focused = true;
                cx.repaint();
            }
            Event::Timer(BLINK) => {
                let e = self.ed();
                e.blink_on = !e.blink_on;
                cx.repaint();
            }
            Event::Timer(SAVE) => {
                cx.timer(SAVE, None);
                self.sh.borrow_mut().save_armed = false;
                self.flush();
            }
            Event::Timer(TOAST) => {
                if self.toast.as_ref().is_none_or(|(_, at)| at.elapsed().as_secs_f32() > TOAST_SECS) {
                    self.toast = None;
                    cx.timer(TOAST, None);
                }
                cx.repaint();
            }
            Event::Timer(TIP) => {
                cx.timer(TIP, None);
                if let Some((h, _)) = self.tip.clone()
                    && self.hover.as_ref() == Some(&h)
                {
                    self.tip = Some((h, true));
                    cx.repaint();
                }
            }
            Event::Active(on) => {
                self.ed().focused = on && !self.field_focus;
                if on {
                    cx.timer(BLINK, Some(530));
                } else {
                    cx.timer(BLINK, None);
                    self.flush();
                }
                cx.repaint();
            }
            Event::Resized | Event::Scale => {
                self.ed().ensure_visible();
                cx.repaint();
            }
            Event::SystemTheme => {
                let t = model::theme_for(&self.sh.borrow().set);
                if t != self.sh.borrow().theme {
                    let dark = t.dark;
                    self.sh.borrow_mut().theme = t;
                    self.win.set_dark(dark);
                    pads::retheme(&self.sh);
                    cx.repaint();
                }
            }
            Event::Opened(msg) => self.opened(&msg, cx),
            Event::Close => {
                self.flush();
                pads::close_all_windows(&self.sh);
            }
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        if p.y >= TITLE_H {
            return Area::Client;
        }
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(_) => Area::Control,
            None => Area::Caption,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        if matches!(self.hits.at(p), Some(Hit::Tab(_) | Hit::Toggle(_) | Hit::Choice(..) | Hit::Status(_))) { Cursor::Hand } else { Cursor::Arrow }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl App {
    /// The text cursor goes over the text (plume-ui has no I-beam: the window's hook sets it).
    pub fn ibeam_at(&self, p: Pt) -> bool {
        let t = self.tab();
        self.dialog.is_none() && !self.settings && t.ed.area.contains(p) && !t.ed.on_bar(p) && matches!(self.hits.at(p), Some(Hit::Editor))
    }

    /// A message from another launch: "open\n<file>\n<file>", "show", "pad", "pads", "quit".
    pub fn opened(&mut self, msg: &str, cx: &mut Cx) {
        let mut lines = msg.lines();
        let noact = self.sh.borrow().noactivate;
        match lines.next().unwrap_or("") {
            "open" => {
                let files: Vec<String> = lines.map(str::to_string).filter(|l| !l.is_empty()).collect();
                if files.is_empty() && !self.tab().pristine() {
                    self.new_tab();
                }
                for f in files {
                    self.open_path(Path::new(&f), cx);
                }
                self.settings = false;
                sys::show(self.win.hwnd, true, !noact);
            }
            "new" => {
                self.new_tab();
                sys::show(self.win.hwnd, true, !noact);
            }
            "show" => sys::show(self.win.hwnd, true, !noact),
            "pad" => self.new_pad(cx),
            "pads" => {
                let sh = self.sh.clone();
                cx.after(move |_| pads::show_all(&sh, true));
            }
            "quit" => crate::quit(&self.sh),
            _ => {}
        }
        self.touch(cx);
    }
}
