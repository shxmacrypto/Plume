//! The panel: a small window over everything, opened by the shortcut near the text cursor (or the mouse). Typing
//! searches, the arrows choose, Enter pastes into the window that was in front, Shift+Enter pastes the text
//! without its formatting, Del removes, 1 to 9 paste at once, Esc closes. A snippet's trigger (";sig") finds it.

use super::{draw, draw::ic, paint, thumb_key};
use crate::{
    i18n::tr,
    image, input,
    model::Model,
    store::{self, Entry, Kind},
};
use plume_ui::{Align, Area, Button, Canvas, Cursor, Cx, Event, Font, Handler, Hits, Mods, PaintCx, Pt, Window, icons, rect};
use std::{any::Any, cell::RefCell, collections::HashSet, rc::Rc};
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    UI::WindowsAndMessaging::{
        GUITHREADINFO, GetCursorPos, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, HWND_TOPMOST, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
        SetForegroundWindow, SetWindowPos, ShowWindow,
    },
};

pub const PANEL_W: f32 = 400.0;
pub const PANEL_H: f32 = 480.0;
const BLINK: usize = 10;
const PASTE: usize = 11;
const ROW_H: f32 = 54.0;
const LIST_Y: f32 = 64.0;
const FOOT_H: f32 = 34.0;

#[derive(Clone, Debug, PartialEq)]
pub enum PHit {
    Row(usize),
    Pin(usize),
    Search,
}

pub struct Panel {
    pub win: Window,
    pub model: Rc<RefCell<Model>>,
    pub query: String,
    pub sel: usize,
    pub scroll: f32,
    pub list: Vec<Entry>,
    list_from: (u64, String),
    pub hits: Hits<PHit>,
    pub hover: Option<PHit>,
    pressed: Option<PHit>,
    /// The window that was in front when the panel opened: the paste goes there.
    pub target: isize,
    pub open: bool,
    caret_on: bool,
    pending: Option<String>,
    /// What the last pick did (for the test socket).
    pub last: Option<String>,
    drawn: HashSet<u64>,
    /// Tests: the panel shows without taking the keyboard from the user's window.
    noactivate: bool,
}

/// The window in front, and where its text cursor is (screen pixels), or else the mouse.
pub fn anchor() -> (isize, (i32, i32)) {
    unsafe {
        let fg = GetForegroundWindow();
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO { cbSize: size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        if tid != 0 && GetGUIThreadInfo(tid, &mut gi).is_ok() && !gi.hwndCaret.0.is_null() {
            let mut p = POINT { x: gi.rcCaret.left, y: gi.rcCaret.bottom };
            if ClientToScreen(gi.hwndCaret, &mut p).as_bool() && (p.x != 0 || p.y != 0) {
                return (fg.0 as isize, (p.x, p.y));
            }
        }
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        (fg.0 as isize, (p.x, p.y))
    }
}

/// Where the panel goes: below the point, inside the screen's work area (above the point if there is no room).
pub fn place(at: (i32, i32), size: (i32, i32), work: RECT) -> (i32, i32) {
    let (pw, ph) = size;
    let x = at.0.min(work.right - pw - 8).max(work.left + 8);
    let mut y = at.1 + 6;
    if y + ph > work.bottom - 8 {
        y = (at.1 - ph - 28).max(work.top + 8);
    }
    (x, y)
}

impl Panel {
    pub fn new(win: Window, model: Rc<RefCell<Model>>) -> Panel {
        Panel {
            win,
            model,
            query: String::new(),
            sel: 0,
            scroll: 0.0,
            list: Vec::new(),
            list_from: (0, "\u{1}".into()),
            hits: Hits::default(),
            hover: None,
            pressed: None,
            target: 0,
            open: false,
            caret_on: true,
            pending: None,
            last: None,
            drawn: HashSet::new(),
            noactivate: std::env::var_os("PLUME_CLIPBOARD_NOACTIVATE").is_some(),
        }
    }

    pub fn refresh(&mut self) {
        let m = self.model.borrow();
        let key = (m.version, self.query.clone());
        if key != self.list_from {
            self.list = m.store.panel(&self.query);
            self.list_from = key;
        }
        self.sel = self.sel.min(self.list.len().saturating_sub(1));
    }

    pub fn open(&mut self, target: isize, at: Option<(i32, i32)>, cx: &mut Cx) {
        // opened from Plume Clipboard's own window or icon: a pick only goes back on the clipboard
        let main = self.model.borrow().main_hwnd().0 as isize;
        let own = target == main || target == self.win.hwnd.0 as isize;
        self.target = if own { 0 } else { target };
        self.query.clear();
        self.sel = 0;
        self.scroll = 0.0;
        self.open = true;
        self.caret_on = true;
        self.refresh();
        let s = self.win.scale();
        let size = ((PANEL_W * s) as i32, (PANEL_H * s) as i32);
        let at = at.unwrap_or_else(|| anchor().1);
        unsafe {
            let mon = MonitorFromPoint(POINT { x: at.0, y: at.1 }, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            let work = if GetMonitorInfoW(mon, &mut mi).as_bool() { mi.rcWork } else { RECT { left: 0, top: 0, right: 1920, bottom: 1080 } };
            let (x, y) = place(at, size, work);
            let flags = if self.noactivate { SWP_SHOWWINDOW | SWP_NOACTIVATE } else { SWP_SHOWWINDOW };
            let _ = SetWindowPos(self.win.hwnd, Some(HWND_TOPMOST), x, y, size.0, size.1, flags);
            if !self.noactivate {
                let _ = SetForegroundWindow(self.win.hwnd);
            }
        }
        self.model.borrow_mut().note("panel opened");
        cx.timer(BLINK, Some(530));
        cx.repaint();
    }

    pub fn hide(&mut self, cx: &mut Cx) {
        if !self.open {
            return;
        }
        self.open = false;
        unsafe {
            let _ = ShowWindow(self.win.hwnd, SW_HIDE);
        }
        cx.timer(BLINK, None);
    }

    /// Pastes the line `i` (`plain`: the text without its formatting).
    pub fn pick(&mut self, i: usize, plain: bool, cx: &mut Cx) {
        self.refresh();
        let Some(e) = self.list.get(i).copied() else { return };
        let mut m = self.model.borrow_mut();
        let (ok, what) = match e {
            Entry::Item(id) => (m.put(id, plain), format!("item #{id}{}", if plain { " plain" } else { "" })),
            Entry::Snippet(id) => (m.put_snippet(id), format!("snippet #{id}")),
        };
        let paste = m.set.paste;
        m.note(format!("picked {what}"));
        drop(m);
        self.last = Some(what.clone());
        self.hide(cx);
        if ok && paste && self.target != 0 {
            let target = HWND(self.target as _);
            if input::fake().is_some() {
                input::paste(target, &what);
            } else {
                // the window in front gets the keyboard back first, then the keys
                unsafe {
                    let _ = SetForegroundWindow(target);
                }
                self.pending = Some(what);
                cx.timer(PASTE, Some(60));
            }
        }
    }

    fn remove(&mut self, i: usize) {
        if let Some(Entry::Item(id)) = self.list.get(i).copied() {
            self.model.borrow_mut().remove(id);
            self.refresh();
        }
    }

    fn toggle_pin(&mut self, i: usize) {
        if let Some(Entry::Item(id)) = self.list.get(i).copied() {
            let on = self.model.borrow().store.get(id).is_some_and(|it| it.pin.is_none());
            self.model.borrow_mut().set_pin(id, on);
            self.refresh();
        }
    }

    fn view_h(&self) -> f32 {
        PANEL_H - LIST_Y - FOOT_H
    }

    fn show_sel(&mut self) {
        let top = self.sel as f32 * ROW_H;
        if top < self.scroll {
            self.scroll = top;
        } else if top + ROW_H > self.scroll + self.view_h() {
            self.scroll = top + ROW_H - self.view_h();
        }
    }

    fn typed(&mut self) {
        self.sel = 0;
        self.scroll = 0.0;
        self.caret_on = true;
        self.refresh();
    }

    pub fn key(&mut self, vk: u16, mods: Mods, cx: &mut Cx) {
        let n = self.list.len();
        match vk {
            0x1B => self.hide(cx),
            0x26 => self.sel = self.sel.saturating_sub(1),
            0x28 => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            0x21 => self.sel = self.sel.saturating_sub(7),
            0x22 => self.sel = (self.sel + 7).min(n.saturating_sub(1)),
            0x24 => self.sel = 0,
            0x23 => self.sel = n.saturating_sub(1),
            0x0D => self.pick(self.sel, mods.shift, cx),
            0x2E => self.remove(self.sel),
            0x08 => {
                if mods.ctrl {
                    self.query.clear();
                } else {
                    self.query.pop();
                }
                self.typed();
            }
            // Ctrl+P: pin or unpin
            0x50 if mods.ctrl => self.toggle_pin(self.sel),
            _ => return,
        }
        self.show_sel();
        cx.handled();
        cx.repaint();
    }

    pub fn char(&mut self, ch: char, cx: &mut Cx) {
        // 1 to 9 with nothing typed: that line at once
        if self.query.is_empty()
            && let Some(d) = ch.to_digit(10).filter(|d| *d >= 1)
        {
            self.pick(d as usize - 1, false, cx);
            return;
        }
        self.query.push(ch);
        self.typed();
        cx.repaint();
    }

    fn paint_all(&mut self, c: &mut Canvas, cx: &PaintCx) {
        self.refresh();
        let model = self.model.clone();
        let m = model.borrow();
        let t = m.theme.clone();
        let (w, h) = (cx.w, cx.h);
        self.hits.clear();
        c.clear(t.bg);
        c.stroke_round(rect(0.5, 0.5, w - 1.0, h - 1.0), 8.0, t.ring, 1.0);
        // the search
        let sr = rect(12.0, 12.0, w - 24.0, 42.0);
        c.fill_round(sr, 12.0, t.fill);
        c.icon(&icons::SEARCH, rect(sr.x + 14.0, sr.y + 13.0, 16.0, 16.0), t.t3);
        let tx = sr.x + 40.0;
        if self.query.is_empty() {
            c.text(tr("Search, or type a snippet's ;trigger"), rect(tx, sr.y, sr.w - 52.0, sr.h), Font::Body, t.t3, Align::Left);
        } else {
            draw::text(c, &self.query, rect(tx, sr.y, sr.w - 52.0, sr.h), 14.0, 400, t.t1, Align::Left);
        }
        if self.caret_on && self.open {
            let qw = if self.query.is_empty() { 0.0 } else { draw_width(c, &self.query) };
            c.fill(rect((tx + qw + 1.0).min(sr.right() - 12.0), sr.y + 12.0, 1.5, 18.0), t.accent);
        }
        self.hits.add(sr, PHit::Search);
        // the lines
        let area = rect(0.0, LIST_Y, w, h - LIST_Y - FOOT_H);
        let max = (self.list.len() as f32 * ROW_H - area.h).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
        c.clip(area);
        if self.list.is_empty() {
            let msg = if self.query.is_empty() { tr("Nothing copied yet") } else { tr("Nothing found") };
            c.text(msg, rect(0.0, area.y + 40.0, w, 24.0), Font::Body, t.t3, Align::Center);
        }
        let now = store::now();
        let first = (self.scroll / ROW_H) as usize;
        let last = (((self.scroll + area.h) / ROW_H).ceil() as usize).min(self.list.len());
        let list = self.list.clone();
        for (i, e) in list.iter().enumerate().take(last).skip(first) {
            let r = rect(8.0, area.y + i as f32 * ROW_H - self.scroll, w - 16.0, ROW_H - 4.0);
            let hit = PHit::Row(i);
            if i == self.sel {
                c.fill_round(r, 10.0, t.accent.fade(if t.dark { 0.24 } else { 0.15 }));
            } else if self.hover == Some(hit.clone()) {
                c.fill_round(r, 10.0, t.hover);
            }
            self.hits.add(r, hit);
            if i < 9 {
                let b = rect(r.x + 8.0, r.y + (r.h - 20.0) / 2.0, 20.0, 20.0);
                c.fill_round(b, 6.0, t.fill);
                c.text(&(i + 1).to_string(), b, Font::SmallBold, t.t2, Align::Center);
            }
            let tile = rect(r.x + 36.0, r.y + (r.h - 34.0) / 2.0, 34.0, 34.0);
            let tx = tile.right() + 12.0;
            let tw = r.right() - tx - 40.0;
            match e {
                Entry::Snippet(id) => {
                    let Some(s) = m.store.snippet(*id) else { continue };
                    c.fill_round(tile, 9.0, t.accent.fade(0.16));
                    c.icon(&ic::SNIPPET, tile.centered(16.0, 16.0), t.accent);
                    let nw = c.text_width(&s.name, Font::BodyBold).min(tw * 0.6);
                    c.text(&s.name, rect(tx, r.y + 7.0, nw + 2.0, 20.0), Font::BodyBold, t.t1, Align::Left);
                    c.text(&s.trigger, rect(tx + nw + 8.0, r.y + 7.0, tw - nw - 8.0, 20.0), Font::SmallBold, t.accent, Align::Left);
                    let preview = s.text.split_whitespace().collect::<Vec<_>>().join(" ");
                    c.text(&preview, rect(tx, r.y + 27.0, tw, 18.0), Font::Small, t.t3, Align::Left);
                }
                Entry::Item(id) => {
                    let Some(it) = m.store.get(*id) else { continue };
                    if it.kind == Kind::Image {
                        c.fill_round(tile, 8.0, t.fill);
                        let (iw, ih) = image::fit(it.w, it.h, 34);
                        let dir = m.dir.clone();
                        let item = *id;
                        c.image(thumb_key(item), tile.centered(iw as f32, ih as f32), 1.0, || {
                            store::read_image(&dir, item, true).and_then(|p| image::drawable(&p))
                        });
                        self.drawn.insert(item);
                    } else {
                        c.fill_round(tile, 9.0, t.accent.fade(0.16));
                        c.icon(paint::kind_icon(it.kind), tile.centered(16.0, 16.0), t.accent);
                    }
                    let (title, ink) = match it.kind {
                        Kind::Image => (format!("{} · {} × {}", tr("Picture"), it.w, it.h), t.t1),
                        Kind::Link => (it.preview(160), t.accent),
                        _ => (it.preview(160), t.t1),
                    };
                    c.text(&title, rect(tx, r.y + 7.0, tw, 20.0), Font::Body, ink, Align::Left);
                    c.text(&paint::meta(it, now), rect(tx, r.y + 27.0, tw, 18.0), Font::Small, t.t3, Align::Left);
                    let pr = rect(r.right() - 34.0, r.y + (r.h - 28.0) / 2.0, 28.0, 28.0);
                    let phit = PHit::Pin(i);
                    if it.pin.is_some() {
                        c.icon_fill(&icons::PIN, pr.centered(14.0, 14.0), t.accent);
                        self.hits.add(pr, phit);
                    } else if self.hover.as_ref().is_some_and(|h| *h == PHit::Row(i) || *h == phit) {
                        if self.hover == Some(phit.clone()) {
                            c.fill_round(pr, 14.0, t.t1.fade(0.1));
                        }
                        c.icon(&icons::PIN, pr.centered(14.0, 14.0), t.t2);
                        self.hits.add(pr, phit);
                    }
                }
            }
        }
        c.unclip();
        // what the keys do
        c.hline(0.0, h - FOOT_H, w, t.sep);
        c.text(tr("Enter paste · Shift+Enter plain text · Del remove · Esc close"), rect(10.0, h - FOOT_H, w - 20.0, FOOT_H), Font::Small, t.t3, Align::Center);
        drop(m);
        // thumbnails of removed items
        let m = model.borrow();
        self.drawn.retain(|id| {
            let keep = m.store.get(*id).is_some();
            if !keep {
                c.forget_image(thumb_key(*id));
            }
            keep
        });
    }
}

fn draw_width(c: &mut Canvas, s: &str) -> f32 {
    // the query is drawn at 14 px: Body (13 px) measured and scaled
    c.text_width(s, Font::Body) * 14.0 / 13.0
}

impl Handler for Panel {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.paint_all(c, cx);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                let h = self.hits.at(p).cloned();
                if h != self.hover {
                    if let Some(PHit::Row(i)) = h {
                        self.sel = i;
                    }
                    self.hover = h;
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                self.hover = None;
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Left, .. } => self.pressed = self.hits.at(p).cloned(),
            Event::MouseUp { p, button: Button::Left, mods } => {
                let up = self.hits.at(p).cloned();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    match a {
                        PHit::Row(i) => self.pick(i, mods.shift, cx),
                        PHit::Pin(i) => self.toggle_pin(i),
                        PHit::Search => {}
                    }
                }
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Right, .. } => {
                if let Some(PHit::Row(i)) = self.hits.at(p).cloned() {
                    self.toggle_pin(i);
                    cx.repaint();
                }
            }
            Event::Wheel { dy, .. } => {
                self.scroll -= dy * ROW_H * 2.0;
                cx.repaint();
            }
            Event::Key { vk, mods, .. } => self.key(vk, mods, cx),
            Event::Char(ch) => self.char(ch, cx),
            Event::Active(false) if self.open && !self.noactivate => self.hide(cx),
            Event::Timer(BLINK) => {
                self.caret_on = !self.caret_on;
                cx.repaint();
            }
            Event::Timer(PASTE) => {
                cx.timer(PASTE, None);
                if let Some(what) = self.pending.take() {
                    input::paste(HWND(self.target as _), &what);
                }
            }
            Event::Resized | Event::Scale | Event::SystemTheme => cx.repaint(),
            _ => {}
        }
    }

    fn area(&self, _p: Pt) -> Area {
        // no caption, no resizing: the whole panel is content
        Area::Control
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(PHit::Row(_) | PHit::Pin(_)) => Cursor::Hand,
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

    #[test]
    fn placed_on_screen() {
        let work = RECT { left: 0, top: 0, right: 1920, bottom: 1040 };
        assert_eq!(place((100, 200), (400, 480), work), (100, 206));
        // near the right edge: moved left
        assert_eq!(place((1900, 200), (400, 480), work), (1512, 206));
        // near the bottom: above the point
        assert_eq!(place((100, 900), (400, 480), work), (100, 392));
        // second monitor on the left
        let left = RECT { left: -1920, top: 0, right: 0, bottom: 1040 };
        assert_eq!(place((-1910, 10), (400, 480), left), (-1910, 16));
    }
}
