//! The main window: the whole history with its filters, the snippets, the settings.

mod draw;
mod paint;
pub mod panel;
mod settings_view;

use crate::{
    clip::{self, Captured, Read},
    i18n::tr,
    image,
    model::{self, Model, PRUNE, RETRY, SAVE, TOAST},
    settings::{self, Settings},
    store::{self, Filter},
};
use plume_ui::{Area, Button, Canvas, Cursor, Cx, Event, Handler, Hits, PaintCx, Pt, Rect, Window};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
    time::Instant,
};

pub const SIDEBAR_W: f32 = 224.0;
/// The list starts below the title row.
pub const LIST_TOP: f32 = 112.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    History(Filter),
    Snippets,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Search,
    Name,
    Trigger,
    Body,
    Ignored,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
    Close,
    Nav(Page),
    Search,
    Row(u64),
    RowPin(u64),
    RowCopy(u64),
    RowDelete(u64),
    RowUp(u64),
    RowDown(u64),
    Clear,
    OpenPanel,
    NewSnippet,
    Snippet(u64),
    Field(Field),
    Save,
    Cancel,
    DeleteSnippet,
    Toggle(&'static str),
    Choice(&'static str, usize),
    Hotkey,
    IgnoredRemove(usize),
    IgnoredAdd,
    Toast,
}

/// The snippet being written or changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub id: Option<u64>,
    pub name: String,
    pub trigger: String,
    pub text: String,
}

pub struct App {
    pub win: Window,
    pub model: Rc<RefCell<Model>>,
    pub page: Page,
    pub query: String,
    /// The items of the page (ids), and what they were computed from.
    pub rows: Vec<u64>,
    rows_from: (u64, Page, String),
    pub hits: Hits<Hit>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub scroll: f32,
    pub content_h: f32,
    pub view_h: f32,
    pub edit_for: Option<Field>,
    edit_shown: Option<Field>,
    pub field_rects: Vec<(Field, Rect)>,
    pub draft: Option<Draft>,
    pub ignored_text: String,
    pub capturing: bool,
    pub hotkey_error: bool,
    pub confirm_clear: Option<Instant>,
    pub toast: Option<(String, Instant)>,
    retries: u32,
    /// Thumbnails drawn, to forget those of removed items.
    drawn: HashSet<u64>,
    pub quitting: Rc<Cell<bool>>,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

/// The key under which a thumbnail is kept by a window's canvas.
pub fn thumb_key(id: u64) -> u64 {
    id | 1 << 62
}

impl App {
    pub fn new(win: Window, model: Rc<RefCell<Model>>, quitting: Rc<Cell<bool>>) -> App {
        App {
            win,
            model,
            page: Page::History(Filter::All),
            query: String::new(),
            rows: Vec::new(),
            rows_from: (0, Page::Settings, String::new()),
            hits: Hits::default(),
            hover: None,
            pressed: None,
            scroll: 0.0,
            content_h: 0.0,
            view_h: 0.0,
            edit_for: None,
            edit_shown: None,
            field_rects: Vec::new(),
            draft: None,
            ignored_text: String::new(),
            capturing: false,
            hotkey_error: false,
            confirm_clear: None,
            toast: None,
            retries: 0,
            drawn: HashSet::new(),
            quitting,
        }
    }

    pub fn say(&mut self, text: impl Into<String>, cx: &mut Cx) {
        self.toast = Some((text.into(), Instant::now()));
        cx.timer(TOAST, Some(50));
        cx.repaint();
    }

    /// The page's items, computed again when the history, the page or the search changed.
    pub fn refresh_rows(&mut self) {
        let m = self.model.borrow();
        let key = (m.version, self.page, self.query.clone());
        if key == self.rows_from {
            return;
        }
        self.rows = match self.page {
            Page::History(f) => m.store.search(&self.query, f),
            _ => Vec::new(),
        };
        self.rows_from = key;
    }

    pub fn go(&mut self, p: Page, cx: &mut Cx) {
        if self.page != p {
            self.page = p;
            self.scroll = 0.0;
            self.draft = None;
            self.capturing = false;
            if !matches!(p, Page::History(_)) && self.edit_for == Some(Field::Search) {
                self.edit_for = None;
            }
            if self.edit_for.is_some_and(|f| f != Field::Search) {
                self.edit_for = None;
            }
        }
        cx.repaint();
    }

    // ---- the clipboard ----

    /// Windows says the clipboard changed.
    pub fn on_clipboard(&mut self, cx: &mut Cx) {
        let (seq, own) = (clip::sequence(), self.model.borrow().own_seq);
        if seq == own {
            return;
        }
        let app = clip::owner_app();
        let refused = self.model.borrow().refuse(&app);
        if let Some(why) = refused {
            self.model.borrow_mut().note(format!("skipped: {why} ({app})"));
            return;
        }
        let (images, files, max) = {
            let s = &self.model.borrow().set;
            (s.images, s.files, s.max_image_mb as usize * 1024 * 1024)
        };
        match clip::read(self.win.hwnd, images, files, max) {
            Read::Busy => {
                // another program still holds the clipboard: a few tries, a moment apart
                if self.retries < 6 {
                    self.retries += 1;
                    cx.timer(RETRY, Some(80));
                }
            }
            Read::Skip(why) => {
                self.retries = 0;
                self.model.borrow_mut().note(format!("skipped: {why}"));
            }
            Read::Got(c) => {
                self.retries = 0;
                self.ingest(c, app, cx);
            }
        }
    }

    /// Keeps what was read (or what a test gives). A picture is turned into PNG on another thread.
    pub fn ingest(&mut self, c: Captured, app: String, cx: &mut Cx) {
        let refused = self.model.borrow().refuse(&app);
        if let Some(why) = refused {
            self.model.borrow_mut().note(format!("skipped: {why} ({app})"));
            return;
        }
        match c {
            Captured::Text { text, html, rtf } => {
                self.model.borrow_mut().add_text(text, html, rtf, &app);
            }
            Captured::Files(list) => {
                self.model.borrow_mut().add_files(list, &app);
            }
            Captured::Image { data, png } => {
                let sender = cx.sender();
                std::thread::spawn(move || {
                    image::init_thread();
                    let pixels = if png { image::decode(&data) } else { image::decode_dib(&data) };
                    let Some(b) = pixels else { return };
                    let mut hash_input = Vec::with_capacity(b.px.len() + 8);
                    hash_input.extend_from_slice(&b.w.to_le_bytes());
                    hash_input.extend_from_slice(&b.h.to_le_bytes());
                    hash_input.extend_from_slice(&b.px);
                    let hash = store::hash(store::Kind::Image, &hash_input);
                    let (Some(full), Some(thumb)) = (image::encode_png(&b), image::thumb_png(&b)) else { return };
                    sender.send(move |h, _cx| {
                        if let Some(app_) = with_app(h) {
                            app_.model.borrow_mut().add_image(b.w, b.h, hash, &full, &thumb, &app);
                        }
                    });
                });
            }
        }
        cx.repaint();
    }

    /// An item clicked in the window: back on the clipboard.
    pub fn copy(&mut self, id: u64, cx: &mut Cx) {
        let ok = self.model.borrow_mut().put(id, false);
        self.say(if ok { tr("Copied") } else { tr("Could not copy") }, cx);
    }

    pub fn open_panel(&mut self) {
        let panel = self.model.borrow().panel;
        if let Some(p) = panel {
            let (target, at) = panel::anchor();
            p.with(|h, cx| {
                if let Some(pn) = h.as_any().downcast_mut::<panel::Panel>() {
                    pn.open(target, Some(at), cx);
                }
            });
        }
    }

    // ---- settings ----

    /// A setting changed: everything that depends on it follows.
    pub fn settings_changed(&mut self, old: &Settings, cx: &mut Cx) {
        let mut m = self.model.borrow_mut();
        if m.set.lang != old.lang {
            crate::i18n::set(&m.set.lang);
        }
        if m.set.theme != old.theme {
            m.theme = model::theme_for(&m.set);
            let dark = m.theme.dark;
            self.win.set_dark(dark);
            if let Some(p) = m.panel {
                p.set_dark(dark);
                p.repaint();
            }
        }
        if m.set.encrypt != old.encrypt {
            m.reseal();
        }
        if m.set.hotkey != old.hotkey {
            m.register_hotkey();
        }
        if (m.set.max_items, m.set.days) != (old.max_items, old.days) {
            let (max, days) = (m.set.max_items as usize, m.set.days);
            let removed = m.store.prune(max, days, store::now());
            for it in &removed {
                store::delete_image(&m.dir, it.id);
            }
            if !removed.is_empty() {
                m.changed();
            }
        }
        m.save_settings();
        drop(m);
        let tip = self.tray_tip();
        crate::sys::tray_tip(self.win.hwnd, &tip);
        cx.repaint();
    }

    pub fn change_setting(&mut self, f: impl FnOnce(&mut Settings), cx: &mut Cx) {
        let old = self.model.borrow().set.clone();
        f(&mut self.model.borrow_mut().set);
        self.settings_changed(&old, cx);
    }

    pub fn tray_tip(&self) -> String {
        let m = self.model.borrow();
        let mut s = "Plume Clipboard".to_string();
        if m.set.paused {
            s.push_str(" — ");
            s.push_str(tr("Paused"));
        } else if let Some(h) = m.hotkey.filter(|_| m.hotkey_ok) {
            s.push_str(" — ");
            s.push_str(&h.label());
        }
        s
    }

    pub fn add_ignored(&mut self, cx: &mut Cx) {
        let Some(app) = settings::clean_app(&self.ignored_text) else { return };
        self.change_setting(
            |s| {
                if !s.ignored.contains(&app) {
                    s.ignored.push(app);
                }
            },
            cx,
        );
        self.ignored_text.clear();
        self.win.edit_set_text("");
    }

    pub fn save_draft(&mut self, cx: &mut Cx) {
        let Some(d) = self.draft.take() else { return };
        if d.name.trim().is_empty() && d.text.is_empty() {
            return;
        }
        let name = if d.name.trim().is_empty() { d.text.lines().next().unwrap_or("").chars().take(40).collect() } else { d.name.clone() };
        let mut m = self.model.borrow_mut();
        m.store.save_snippet(d.id, &name, &d.trigger, &d.text);
        m.note(format!("snippet saved {}", d.trigger));
        m.changed();
        drop(m);
        self.edit_for = None;
        self.say(tr("Snippet saved"), cx);
    }

    pub fn quit(&mut self, cx: &mut Cx) {
        self.quitting.set(true);
        let panel = self.model.borrow().panel;
        cx.after(move |w| {
            if let Some(p) = panel {
                p.close();
            }
            w.close();
        });
    }

    // ---- events ----

    fn act(&mut self, h: Hit, cx: &mut Cx) {
        match h {
            Hit::Min => self.win.minimize(),
            Hit::Max => self.win.toggle_maximize(),
            Hit::Close => self.win.close(),
            Hit::Nav(p) => self.go(p, cx),
            Hit::Search => {
                self.edit_for = Some(Field::Search);
                cx.repaint();
            }
            Hit::Row(id) | Hit::RowCopy(id) => self.copy(id, cx),
            Hit::RowPin(id) => {
                let on = self.model.borrow().store.get(id).is_some_and(|i| i.pin.is_none());
                self.model.borrow_mut().set_pin(id, on);
                self.say(if on { tr("Pinned: kept forever") } else { tr("Unpinned") }, cx);
            }
            Hit::RowDelete(id) => self.model.borrow_mut().remove(id),
            Hit::RowUp(id) | Hit::RowDown(id) => {
                let d = if matches!(h, Hit::RowUp(_)) { -1 } else { 1 };
                let mut m = self.model.borrow_mut();
                m.store.move_pin(id, d);
                m.changed();
            }
            Hit::Clear => {
                if self.confirm_clear.is_some_and(|t| t.elapsed().as_secs_f32() < 4.0) {
                    self.confirm_clear = None;
                    self.model.borrow_mut().clear();
                    self.say(tr("History cleared"), cx);
                } else {
                    self.confirm_clear = Some(Instant::now());
                }
            }
            Hit::OpenPanel => self.open_panel(),
            Hit::NewSnippet => {
                self.draft = Some(Draft::default());
                self.edit_for = Some(Field::Name);
            }
            Hit::Snippet(id) => {
                if let Some(s) = self.model.borrow().store.snippet(id) {
                    self.draft = Some(Draft { id: Some(id), name: s.name.clone(), trigger: s.trigger.clone(), text: s.text.clone() });
                }
                self.edit_for = None;
            }
            Hit::Field(f) => {
                self.edit_for = Some(f);
                if f == Field::Ignored {
                    self.ignored_text.clear();
                }
            }
            Hit::Save => self.save_draft(cx),
            Hit::Cancel => {
                self.draft = None;
                self.edit_for = None;
            }
            Hit::DeleteSnippet => {
                if let Some(id) = self.draft.take().and_then(|d| d.id) {
                    let mut m = self.model.borrow_mut();
                    m.store.remove_snippet(id);
                    m.changed();
                }
                self.edit_for = None;
            }
            Hit::Toggle(name) => self.toggle(name, cx),
            Hit::Choice(name, i) => self.choose(name, i, cx),
            Hit::Hotkey => {
                self.capturing = !self.capturing;
                self.hotkey_error = false;
                // while a new shortcut is typed, the old one must not open the panel
                let hwnd = self.win.hwnd;
                if self.capturing {
                    crate::hotkey::unregister(hwnd);
                } else {
                    self.model.borrow_mut().register_hotkey();
                }
            }
            Hit::IgnoredRemove(i) => self.change_setting(
                |s| {
                    if i < s.ignored.len() {
                        s.ignored.remove(i);
                    }
                },
                cx,
            ),
            Hit::IgnoredAdd => self.add_ignored(cx),
            Hit::Toast => self.toast = None,
        }
        cx.repaint();
    }

    fn toggle(&mut self, name: &str, cx: &mut Cx) {
        if name == "autostart" {
            let on = !self.model.borrow().autostart;
            if crate::sys::set_autostart(on) {
                self.model.borrow_mut().autostart = on;
            }
            cx.repaint();
            return;
        }
        self.change_setting(
            |s| match name {
                "images" => s.images = !s.images,
                "files" => s.files = !s.files,
                "encrypt" => s.encrypt = !s.encrypt,
                "paste" => s.paste = !s.paste,
                "paused" => s.paused = !s.paused,
                _ => {}
            },
            cx,
        );
    }

    fn choose(&mut self, name: &str, i: usize, cx: &mut Cx) {
        self.change_setting(
            |s| match name {
                "theme" => s.theme = ["system", "dark", "light"][i.min(2)].into(),
                "lang" => s.lang = ["system", "en", "fr"][i.min(2)].into(),
                "max_items" => s.max_items = settings::MAX_ITEMS[i.min(settings::MAX_ITEMS.len() - 1)],
                "days" => s.days = settings::DAYS[i.min(settings::DAYS.len() - 1)],
                "image_mb" => s.max_image_mb = settings::IMAGE_MB[i.min(settings::IMAGE_MB.len() - 1)],
                _ => {}
            },
            cx,
        );
    }

    fn key(&mut self, vk: u16, mods: plume_ui::Mods, cx: &mut Cx) {
        if self.capturing {
            // the new shortcut: a key with Ctrl or Alt (Escape gives up)
            if vk == 0x1B {
                self.capturing = false;
                self.model.borrow_mut().register_hotkey();
            } else if !matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5) {
                let h = crate::hotkey::Hotkey { ctrl: mods.ctrl, shift: mods.shift, alt: mods.alt, win: false, vk };
                if h.valid() {
                    self.capturing = false;
                    let label = h.label();
                    self.change_setting(|s| s.hotkey = label, cx);
                    self.hotkey_error = !self.model.borrow().hotkey_ok;
                } else {
                    self.hotkey_error = true;
                }
            }
            cx.handled();
            cx.repaint();
            return;
        }
        match vk {
            // Ctrl+F: the search
            0x46 if mods.ctrl && matches!(self.page, Page::History(_)) => {
                self.edit_for = Some(Field::Search);
                cx.handled();
            }
            0x1B => {
                if self.draft.is_some() {
                    self.draft = None;
                    self.edit_for = None;
                }
                cx.handled();
            }
            0x24 => self.scroll = 0.0,
            0x23 => self.scroll = (self.content_h - self.view_h).max(0.0),
            0x21 => self.scroll = (self.scroll - self.view_h * 0.9).max(0.0),
            0x22 => self.scroll = (self.scroll + self.view_h * 0.9).min((self.content_h - self.view_h).max(0.0)),
            _ => return,
        }
        cx.repaint();
    }

    fn edit_changed(&mut self, text: String, cx: &mut Cx) {
        match self.edit_for {
            Some(Field::Search) => {
                self.query = text;
                self.scroll = 0.0;
            }
            Some(Field::Name) => {
                if let Some(d) = &mut self.draft {
                    d.name = text;
                }
            }
            Some(Field::Trigger) => {
                if let Some(d) = &mut self.draft {
                    d.trigger = text;
                }
            }
            Some(Field::Body) => {
                if let Some(d) = &mut self.draft {
                    d.text = text;
                }
            }
            Some(Field::Ignored) => self.ignored_text = text,
            None => {}
        }
        cx.repaint();
    }

    fn edit_key(&mut self, vk: u16, mods: plume_ui::Mods, cx: &mut Cx) {
        match (self.edit_for, vk) {
            (Some(Field::Search), 0x1B) => {
                if self.query.is_empty() {
                    self.edit_for = None;
                } else {
                    self.query.clear();
                    self.win.edit_set_text("");
                }
            }
            (Some(Field::Name), 0x0D | 0x09) => self.edit_for = Some(Field::Trigger),
            (Some(Field::Trigger), 0x0D | 0x09) => self.edit_for = Some(Field::Body),
            (Some(Field::Body), 0x0D) if mods.ctrl => self.save_draft(cx),
            (Some(Field::Ignored), 0x0D) => self.add_ignored(cx),
            (Some(_), 0x1B) => self.edit_for = None,
            _ => {}
        }
        cx.repaint();
    }

    fn wheel(&mut self, dy: f32, cx: &mut Cx) {
        let max = (self.content_h - self.view_h).max(0.0);
        self.scroll = (self.scroll - dy * 96.0).clamp(0.0, max);
        cx.repaint();
    }

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
                // a click outside the open field closes it (its text stays)
                if self.edit_for.is_some() && !matches!(self.pressed, Some(Hit::Field(_) | Hit::Search | Hit::IgnoredAdd | Hit::Save)) {
                    self.edit_for = None;
                }
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                let up = self.hits.at(p).cloned();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    self.act(a, cx);
                }
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Right, .. } => {
                // right click on an item: pin or unpin
                if let Some(Hit::Row(id)) = self.hits.at(p).cloned() {
                    self.act(Hit::RowPin(id), cx);
                }
            }
            Event::Wheel { dy, .. } => self.wheel(dy, cx),
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::Char(ch) => {
                // typing anywhere in the history: it goes to the search
                if matches!(self.page, Page::History(_)) && self.edit_for.is_none() && !self.capturing {
                    self.query.push(ch);
                    self.edit_for = Some(Field::Search);
                    self.edit_shown = None;
                    cx.repaint();
                }
            }
            Event::EditChanged(text) => self.edit_changed(text, cx),
            Event::EditKey { vk, mods } => self.edit_key(vk, mods, cx),
            Event::EditBlur => {
                if self.edit_for.is_some_and(|f| f != Field::Search) || self.query.is_empty() {
                    self.edit_for = None;
                    cx.repaint();
                }
            }
            Event::Timer(SAVE) => {
                cx.timer(SAVE, None);
                self.model.borrow_mut().save();
            }
            Event::Timer(PRUNE) => {
                let mut m = self.model.borrow_mut();
                let (max, days) = (m.set.max_items as usize, m.set.days);
                let removed = m.store.prune(max, days, store::now());
                for it in &removed {
                    store::delete_image(&m.dir, it.id);
                }
                if !removed.is_empty() {
                    m.changed();
                }
            }
            Event::Timer(RETRY) => {
                cx.timer(RETRY, None);
                self.on_clipboard(cx);
            }
            Event::Timer(TOAST) => {
                let done = self.toast.as_ref().is_none_or(|(_, at)| at.elapsed().as_secs_f32() > 2.6);
                if done {
                    self.toast = None;
                    cx.timer(TOAST, None);
                }
                if self.confirm_clear.is_some_and(|t| t.elapsed().as_secs_f32() >= 4.0) {
                    self.confirm_clear = None;
                }
                cx.repaint();
            }
            Event::Opened(text) => match text.as_str() {
                "quit" => self.quit(cx),
                "panel" => self.open_panel(),
                _ => crate::sys::show(self.win.hwnd, true),
            },
            Event::Close => {
                let mut m = self.model.borrow_mut();
                m.save();
                m.save_settings();
                let hwnd = self.win.hwnd;
                crate::hotkey::unregister(hwnd);
                unsafe {
                    let _ = windows::Win32::System::DataExchange::RemoveClipboardFormatListener(hwnd);
                }
                crate::sys::tray_remove(hwnd);
            }
            Event::SystemTheme => {
                let mut m = self.model.borrow_mut();
                if m.set.theme == "system" {
                    m.theme = model::theme_for(&m.set);
                    let dark = m.theme.dark;
                    self.win.set_dark(dark);
                    if let Some(p) = m.panel {
                        p.set_dark(dark);
                        p.repaint();
                    }
                }
                cx.repaint();
            }
            Event::Resized | Event::Scale | Event::Active(_) => cx.repaint(),
            _ => {}
        }
    }

    pub fn forget_gone(&mut self, c: &mut Canvas) {
        let m = self.model.borrow();
        self.drawn.retain(|id| {
            let keep = m.store.get(*id).is_some();
            if !keep {
                c.forget_image(thumb_key(*id));
            }
            keep
        });
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_all(c, cx);
        self.forget_gone(c);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        self.handle(ev, cx);
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(Hit::Min | Hit::Close) => Area::Control,
            Some(_) => Area::Client,
            None if p.y < 44.0 => Area::Caption,
            None => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Hit::Min | Hit::Max | Hit::Close | Hit::Search | Hit::Field(_)) | None => Cursor::Arrow,
            Some(_) => Cursor::Hand,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
