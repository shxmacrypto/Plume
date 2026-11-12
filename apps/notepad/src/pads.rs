//! Sticky pads: small coloured note windows, over every other window unless unpinned, resizable, saved as they are
//! typed, listed near the clock. Each pad is its own window with the same editor as the tabs.

use crate::{
    editor::{Colors, Editor},
    i18n::{fill, tr},
    model::Shared,
    store::Pad,
    sys,
    text::Doc,
    ui::draw::{self, ic},
};
use plume_ui::{Align, Area, Button, Canvas, Color, Cursor, Cx, Event, Font, Handler, Hits, PaintCx, Pt, Rect, Theme, Window, icons, rect, window};
use std::{any::Any, cell::RefCell, rc::Rc};

pub const BAR_H: f32 = 34.0;
const BLINK: usize = 1;
const PAD_W: f32 = 290.0;
const PAD_H: f32 = 270.0;

/// The pad colours: name, light body, light bar.
pub const COLORS: [(&str, u32, u32); 6] = [
    ("Yellow", 0xfff6bf, 0xffe680),
    ("Green", 0xe0f5d8, 0xbde6ad),
    ("Pink", 0xffe1ec, 0xffbfd5),
    ("Blue", 0xdeecff, 0xb7d3fb),
    ("Purple", 0xece3ff, 0xd5c3ff),
    ("Sand", 0xf4e6d2, 0xe4c9a4),
];

/// The ink of the bar: the bar is pastel in both themes.
pub const BAR_INK: u32 = 0x1d1d1f;

/// (body, bar, text) of a pad colour: all pastel in light mode; in dark mode a dark body under the pastel bar.
pub fn pad_colors(t: &Theme, i: usize) -> (Color, Color, Color) {
    let (_, body, bar) = COLORS[i % COLORS.len()];
    if t.dark { (Color::hex(bar).fade(0.07).over(t.solid), Color::hex(bar), t.t1) } else { (Color::hex(body), Color::hex(bar), Color::hex(0x1d1d1f)) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadHit {
    New,
    Palette,
    Pin,
    More,
    Close,
    Color(usize),
    Delete,
    KeepIt,
    Text,
}

pub struct PadView {
    pub win: Window,
    pub id: u64,
    pub sh: Rc<RefCell<Shared>>,
    pub ed: Editor,
    pub hits: Hits<PadHit>,
    pub hover: Option<PadHit>,
    pub pressed: Option<PadHit>,
    pub picking: bool,
    pub confirm_delete: bool,
    synced: u64,
}

pub fn with_pad(h: &mut dyn Handler) -> Option<&mut PadView> {
    h.as_any().downcast_mut::<PadView>()
}

impl PadView {
    fn new(win: Window, id: u64, sh: Rc<RefCell<Shared>>) -> PadView {
        let (text, m) = {
            let s = sh.borrow();
            (s.pads.get(id).map(|p| p.text.clone()).unwrap_or_default(), s.pad_measure.clone())
        };
        let mut doc = Doc::new(&text);
        doc.sel = crate::text::Sel::at(doc.end());
        let mut ed = Editor::new(doc, m);
        ed.wrap = true;
        ed.numbers = false;
        let synced = ed.doc.version;
        PadView { win, id, sh, ed, hits: Hits::default(), hover: None, pressed: None, picking: false, confirm_delete: false, synced }
    }

    pub fn pad(&self) -> Option<Pad> {
        self.sh.borrow().pads.get(self.id).cloned()
    }

    /// The text goes to the pads list (saved a moment later by the main window).
    fn sync(&mut self) {
        if self.ed.doc.version == self.synced {
            return;
        }
        self.synced = self.ed.doc.version;
        let text = self.ed.doc.text();
        let mut s = self.sh.borrow_mut();
        if let Some(p) = s.pads.get_mut(self.id) {
            p.text = text;
        }
        s.pads_dirty = true;
        s.arm_save();
    }

    fn colors(&self) -> (Colors, Color, Color, Theme) {
        let t = self.sh.borrow().theme.clone();
        let color = self.pad().map_or(0, |p| p.color);
        let (body, bar, ink) = pad_colors(&t, color);
        let k = Colors {
            bg: body,
            text: ink,
            gutter: body,
            numbers: ink,
            numbers_cur: ink,
            line: Color::TRANSPARENT,
            sel: t.accent.fade(if t.dark { 0.4 } else { 0.3 }),
            caret: ink,
            found: Color::TRANSPARENT,
            found_cur: Color::TRANSPARENT,
            bar: ink.fade(0.18),
            bar_hot: ink.fade(0.35),
        };
        (k, bar, Color::hex(BAR_INK), t)
    }

    fn btn(&mut self, c: &mut Canvas, icon: &plume_ui::icons::Icon, r: Rect, hit: PadHit, ink: Color, on: bool) {
        if self.hover == Some(hit) || on {
            c.fill_round(r, r.h / 2.0, ink.fade(if self.pressed == Some(hit) { 0.18 } else { 0.1 }));
        }
        c.icon(icon, r.centered(16.0, 16.0), if on { ink } else { ink.fade(0.72) });
        self.hits.add(r, hit);
    }

    fn act(&mut self, h: PadHit, cx: &mut Cx) {
        let id = self.id;
        let sh = self.sh.clone();
        match h {
            PadHit::New => {
                let activate = !sh.borrow().noactivate;
                cx.after(move |_| {
                    new_pad(&sh, activate);
                });
            }
            PadHit::Palette => self.picking = !self.picking,
            PadHit::Color(i) => {
                if let Some(p) = sh.borrow_mut().pads.get_mut(id) {
                    p.color = i;
                }
                self.mark_dirty();
                self.picking = false;
            }
            PadHit::Pin => {
                let on = {
                    let mut s = sh.borrow_mut();
                    let p = s.pads.get_mut(id);
                    p.map(|p| {
                        p.on_top = !p.on_top;
                        p.on_top
                    })
                };
                if let Some(on) = on {
                    sys::set_on_top(self.win.hwnd, on);
                }
                self.mark_dirty();
            }
            PadHit::More => self.menu(cx),
            PadHit::Close => {
                self.sync();
                close_pad(&sh, id);
            }
            PadHit::Delete => {
                cx.after(move |_| delete_pad(&sh, id));
            }
            PadHit::KeepIt => self.confirm_delete = false,
            PadHit::Text => {}
        }
        cx.repaint();
    }

    fn mark_dirty(&mut self) {
        let mut s = self.sh.borrow_mut();
        s.pads_dirty = true;
        s.arm_save();
    }

    fn menu(&mut self, cx: &mut Cx) {
        let r = self.hits.rect_of(|h| *h == PadHit::More).unwrap_or_default();
        let at = self.win.to_screen(Pt { x: r.x, y: r.bottom() });
        let items = vec![
            sys::item(1, format!("{}\tCtrl+N", tr("New pad"))),
            sys::item(2, tr("Open in Plume Notepad")),
            sys::sep(),
            sys::item(3, format!("{}\tCtrl+D", tr("Delete pad"))),
        ];
        cx.after(move |w| {
            let cmd = sys::popup(w.hwnd, &items, Some(at), false);
            w.with(|h, cx| {
                if let Some(p) = with_pad(h) {
                    match cmd {
                        1 => p.act(PadHit::New, cx),
                        2 => p.send_to_tab(),
                        3 => p.ask_delete(cx),
                        _ => {}
                    }
                }
            });
        });
    }

    fn ask_delete(&mut self, cx: &mut Cx) {
        if self.ed.doc.is_empty() {
            self.act(PadHit::Delete, cx);
        } else {
            self.confirm_delete = true;
            cx.repaint();
        }
    }

    /// The pad's text in a new tab of the main window.
    fn send_to_tab(&mut self) {
        let text = self.ed.doc.text();
        let main = self.sh.borrow().main;
        let noact = self.sh.borrow().noactivate;
        if let Some(m) = main {
            m.with(|h, cx| {
                if let Some(a) = crate::ui::with_app(h) {
                    a.new_tab();
                    a.ed().type_text(&text, crate::text::Kind::Other);
                    a.settings = false;
                    a.touch(cx);
                }
            });
            sys::show(m.hwnd, true, !noact);
        }
    }
}

impl Handler for PadView {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let (k, bar, ink, t) = self.colors();
        self.hits.clear();
        let (w, h) = (cx.w, cx.h);
        c.clear(k.bg);
        let body = rect(0.0, BAR_H, w, h - BAR_H);
        self.hits.add(body, PadHit::Text);
        self.ed.focused = cx.active && !self.confirm_delete;
        self.ed.paint(c, rect(body.x + 4.0, body.y + 2.0, body.w - 6.0, body.h - 6.0), &k, None, None);
        c.fill(rect(0.0, 0.0, w, BAR_H), bar);
        let y = (BAR_H - 28.0) / 2.0;
        if self.picking {
            // the colours, in place of the buttons
            let cur = self.pad().map_or(0, |p| p.color);
            let mut x = 10.0;
            for (i, (_, _, bar_rgb)) in COLORS.iter().enumerate() {
                let r = rect(x, y + 3.0, 22.0, 22.0);
                c.fill_round(r, 11.0, Color::hex(*bar_rgb).mix(Color::hex(0xffffff), 0.15));
                c.stroke_round(r, 11.0, ink.fade(if i == cur || self.hover == Some(PadHit::Color(i)) { 0.7 } else { 0.18 }), if i == cur { 2.0 } else { 1.0 });
                self.hits.add(r.inset(-3.0), PadHit::Color(i));
                x += 30.0;
            }
            self.btn(c, &icons::CLOSE, rect(w - 34.0, y, 28.0, 28.0), PadHit::Palette, ink, false);
        } else {
            self.btn(c, &icons::PLUS, rect(6.0, y, 28.0, 28.0), PadHit::New, ink, false);
            let on_top = self.pad().is_some_and(|p| p.on_top);
            let mut x = w - 34.0;
            self.btn(c, &icons::CLOSE, rect(x, y, 28.0, 28.0), PadHit::Close, ink, false);
            x -= 30.0;
            self.btn(c, &icons::MORE, rect(x, y, 28.0, 28.0), PadHit::More, ink, false);
            x -= 30.0;
            self.btn(c, &icons::PIN, rect(x, y, 28.0, 28.0), PadHit::Pin, ink, on_top);
            x -= 30.0;
            self.btn(c, &ic::PALETTE, rect(x, y, 28.0, 28.0), PadHit::Palette, ink, false);
        }
        if self.ed.doc.is_empty() && !self.confirm_delete {
            c.text(tr("Write something…"), rect(14.0 + 2.0, BAR_H + 8.0, w - 28.0, self.ed.m.line_h), Font::Body, k.text.fade(0.45), Align::Left);
        }
        if self.confirm_delete {
            c.fill(rect(0.0, BAR_H, w, h - BAR_H), Color::hexa(0x000000, 0.25));
            let cw = (w - 24.0).min(260.0);
            let card = rect((w - cw) / 2.0, BAR_H + ((h - BAR_H) - 118.0) / 2.0, cw, 118.0);
            c.fill_round(card, 16.0, t.solid);
            c.stroke_round(card, 16.0, t.ring, 1.0);
            c.text(tr("Delete this pad?"), rect(card.x + 18.0, card.y + 14.0, cw - 36.0, 24.0), Font::Title, t.t1, Align::Left);
            let del = tr("Delete");
            let keep = tr("Cancel");
            let dw = draw::pill_w(c, del);
            let kw = draw::pill_w(c, keep);
            let by = card.bottom() - 18.0 - 34.0;
            let dr = rect(card.right() - 18.0 - dw, by, dw, 34.0);
            let kr = rect(dr.x - 8.0 - kw, by, kw, 34.0);
            draw::pill(c, &t, kr, keep, 0, self.hover == Some(PadHit::KeepIt), false, Font::BodyBold);
            draw::pill(c, &t, dr, del, 2, self.hover == Some(PadHit::Delete), false, Font::BodyBold);
            self.hits.add(rect(0.0, BAR_H, w, h - BAR_H), PadHit::Text);
            self.hits.add(kr, PadHit::KeepIt);
            self.hits.add(dr, PadHit::Delete);
        }
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                if self.ed.dragging() {
                    if self.ed.mouse_move(p) {
                        cx.repaint();
                    }
                    return;
                }
                let h = self.hits.at(p).copied();
                let bar = self.ed.mouse_move(p);
                if h != self.hover || bar {
                    self.hover = h;
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                self.hover = None;
                cx.repaint();
            }
            Event::MouseDown { p, button, clicks, mods } => {
                if button != Button::Left {
                    return;
                }
                let h = self.hits.at(p).copied();
                self.pressed = h;
                if h == Some(PadHit::Text) && !self.confirm_delete {
                    self.picking = false;
                    if self.ed.mouse_down(p, clicks, mods) {
                        cx.capture(true);
                    }
                    cx.timer(BLINK, Some(530));
                }
                cx.repaint();
            }
            Event::MouseUp { p, button, .. } => {
                if self.ed.mouse_up() {
                    cx.capture(false);
                }
                if button != Button::Left {
                    return;
                }
                let h = self.hits.at(p).copied();
                if let Some(h) = h
                    && Some(h) == self.pressed.take()
                    && h != PadHit::Text
                {
                    self.act(h, cx);
                }
                cx.repaint();
            }
            Event::Wheel { dx, dy, mods, .. } => {
                self.ed.wheel(dx, dy, mods.shift);
                cx.repaint();
            }
            Event::Key { vk, mods, .. } => {
                let ctrl = mods.ctrl && !mods.alt;
                if self.confirm_delete {
                    match vk {
                        0x1B => self.confirm_delete = false,
                        0x0D => self.act(PadHit::Delete, cx),
                        _ => {}
                    }
                    cx.handled();
                    cx.repaint();
                    return;
                }
                let done = match vk {
                    0x4E if ctrl => {
                        self.act(PadHit::New, cx);
                        true
                    }
                    0x44 if ctrl => {
                        self.ask_delete(cx);
                        true
                    }
                    0x57 if ctrl => {
                        self.act(PadHit::Close, cx);
                        true
                    }
                    0x1B if self.picking => {
                        self.picking = false;
                        true
                    }
                    _ => self.ed.key(vk, mods, self.win),
                };
                if done {
                    cx.handled();
                    cx.timer(BLINK, Some(530));
                    self.sync();
                    cx.repaint();
                }
            }
            Event::Char(ch) => {
                if !self.confirm_delete {
                    self.ed.char(ch);
                    self.sync();
                    cx.timer(BLINK, Some(530));
                    cx.repaint();
                }
            }
            Event::Timer(BLINK) => {
                self.ed.blink_on = !self.ed.blink_on;
                cx.repaint();
            }
            Event::Active(on) => {
                cx.timer(BLINK, if on { Some(530) } else { None });
                if !on {
                    self.picking = false;
                }
                cx.repaint();
            }
            Event::Resized | Event::Scale => {
                self.ed.ensure_visible();
                let mut s = self.sh.borrow_mut();
                s.pads_dirty = true;
                cx.repaint();
            }
            Event::Close => {
                self.sync();
                let rect = sys::window_rect(self.win.hwnd);
                let mut s = self.sh.borrow_mut();
                if let Some(p) = s.pads.get_mut(self.id) {
                    p.rect = rect;
                }
                s.pad_wins.remove(&self.id);
                s.pads_dirty = true;
            }
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        if p.y < BAR_H {
            match self.hits.at(p) {
                Some(PadHit::Text) | None => Area::Caption,
                Some(_) => Area::Control,
            }
        } else {
            Area::Client
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        if matches!(self.hits.at(p), Some(PadHit::Color(_))) { Cursor::Hand } else { Cursor::Arrow }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

// ---- the pads as a whole ----

/// Where a new pad goes: near the top right of the screen, a little lower than the last one.
fn place_new(sh: &Shared, scale: f32) -> (i32, i32, i32, i32) {
    let (wx, wy, ww, _) = sys::work_area();
    let (w, h) = ((PAD_W * scale) as i32, (PAD_H * scale) as i32);
    let n = sh.pads.list.iter().filter(|p| p.open).count() as i32;
    let step = (28.0 * scale) as i32;
    (wx + ww - w - (24.0 * scale) as i32 - (n % 8) * step, wy + (24.0 * scale) as i32 + (n % 8) * step, w, h)
}

/// Makes a pad and shows it. Returns its id.
pub fn new_pad(sh: &Rc<RefCell<Shared>>, activate: bool) -> u64 {
    let id = {
        let mut s = sh.borrow_mut();
        let id = s.pads.next_id.max(1);
        s.pads.next_id = id + 1;
        let scale = s.main.map_or(1.0, |m| m.scale());
        let rect = place_new(&s, scale);
        let color = s.pads.list.last().map_or(0, |p| (p.color + 1) % COLORS.len());
        s.pads.list.push(Pad { id, text: String::new(), color, rect, on_top: true, open: true });
        s.pads_dirty = true;
        s.note(format!("pad {id} new"));
        id
    };
    open_pad(sh, id, activate);
    refresh_tray(sh);
    id
}

/// Shows a pad (its window is made if it is closed).
pub fn open_pad(sh: &Rc<RefCell<Shared>>, id: u64, activate: bool) {
    let existing = sh.borrow().pad_wins.get(&id).copied();
    if let Some(w) = existing {
        sys::show(w.hwnd, true, activate);
        return;
    }
    let (gfx, theme, pad) = {
        let s = sh.borrow();
        let Some(p) = s.pads.get(id).cloned() else { return };
        (s.gfx.clone(), s.theme.clone(), p)
    };
    let (body, _, _) = pad_colors(&theme, pad.color);
    let o = window::Options {
        title: if pad.title().is_empty() { tr("Sticky pad").to_string() } else { pad.title() },
        size: (PAD_W, PAD_H),
        min: (200.0, 140.0),
        dark: theme.dark,
        background: body,
        activate: false,
        placement: None,
    };
    let s2 = sh.clone();
    let Ok(win) = window::create(&gfx, o, move |w| Box::new(PadView::new(w, id, s2))) else { return };
    sys::make_pad_window(win.hwnd);
    let (x, y, w, h) = pad.rect;
    if w > 50 && h > 50 {
        sys::place(win.hwnd, pad.rect);
    } else {
        let scale = win.scale();
        let r = place_new(&sh.borrow(), scale);
        sys::place(win.hwnd, r);
    }
    let _ = (x, y);
    sys::set_on_top(win.hwnd, pad.on_top);
    sys::accept_files(win.hwnd);
    sys::show(win.hwnd, true, activate);
    {
        let mut s = sh.borrow_mut();
        s.pad_wins.insert(id, win);
        if let Some(p) = s.pads.get_mut(id) {
            p.open = true;
        }
        s.pads_dirty = true;
    }
    install_hook(sh, win);
}

/// The pad's window messages Plume Notepad handles itself: the input method at the caret.
fn install_hook(sh: &Rc<RefCell<Shared>>, win: Window) {
    let sh = sh.clone();
    win.set_hook(Some(Rc::new(move |msg, wp, _lp| {
        if msg == sys::WM_IME_STARTCOMPOSITION || msg == sys::WM_IME_COMPOSITION {
            let info = win.with(|h, _| with_pad(h).and_then(|p| p.ed.caret_rect.map(|r| (r, p.ed.m.family.clone(), p.ed.m.size)))).flatten();
            if let Some((r, fam, size)) = info {
                sys::ime_at(win.hwnd, r, win.scale(), &fam, size);
            }
        }
        // files dropped on a pad open in the main window
        if msg == sys::WM_DROPFILES {
            let files = sys::dropped(wp);
            let (main, noact) = {
                let s = sh.borrow();
                (s.main, s.noactivate)
            };
            if let Some(m) = main {
                m.with(|h, cx| {
                    if let Some(a) = crate::ui::with_app(h) {
                        for f in &files {
                            a.open_path(std::path::Path::new(f), cx);
                        }
                        a.settings = false;
                    }
                });
                sys::show(m.hwnd, true, !noact);
            }
            return Some(windows::Win32::Foundation::LRESULT(0));
        }
        None
    })));
}

/// Hides a pad (it stays in the list near the clock).
pub fn close_pad(sh: &Rc<RefCell<Shared>>, id: u64) {
    let w = {
        let mut s = sh.borrow_mut();
        if let Some(p) = s.pads.get_mut(id) {
            p.open = false;
        }
        s.pads_dirty = true;
        s.note(format!("pad {id} closed"));
        s.pad_wins.get(&id).copied()
    };
    if let Some(w) = w {
        w.close();
    }
}

pub fn delete_pad(sh: &Rc<RefCell<Shared>>, id: u64) {
    let w = {
        let mut s = sh.borrow_mut();
        s.pads.list.retain(|p| p.id != id);
        s.pads_dirty = true;
        s.note(format!("pad {id} deleted"));
        s.pad_wins.remove(&id)
    };
    if let Some(w) = w {
        w.close();
    }
    flush(sh);
    refresh_tray(sh);
}

pub fn show_all(sh: &Rc<RefCell<Shared>>, activate: bool) {
    let ids: Vec<u64> = sh.borrow().pads.list.iter().map(|p| p.id).collect();
    if ids.is_empty() {
        new_pad(sh, activate);
        return;
    }
    for id in ids {
        open_pad(sh, id, false);
    }
}

pub fn hide_all(sh: &Rc<RefCell<Shared>>) {
    let ids: Vec<u64> = sh.borrow().pad_wins.keys().copied().collect();
    for id in ids {
        close_pad(sh, id);
    }
}

/// Opens the pads that were open last time (start, sign-in).
pub fn reopen(sh: &Rc<RefCell<Shared>>) {
    let ids: Vec<u64> = sh.borrow().pads.list.iter().filter(|p| p.open).map(|p| p.id).collect();
    for id in ids {
        open_pad(sh, id, false);
    }
}

/// Windows closed for quitting (the pads stay marked open for next time).
pub fn close_all_windows(sh: &Rc<RefCell<Shared>>) {
    let wins: Vec<Window> = sh.borrow().pad_wins.values().copied().collect();
    let open: Vec<u64> = sh.borrow().pad_wins.keys().copied().collect();
    flush(sh);
    for w in wins {
        w.close();
    }
    // closing a window marks nothing: they come back next time
    let mut s = sh.borrow_mut();
    for id in open {
        if let Some(p) = s.pads.get_mut(id) {
            p.open = true;
        }
    }
    let dir = s.dir.clone();
    s.pads.save(&dir);
}

/// Writes pads.json if something changed (places read from the windows).
pub fn flush(sh: &Rc<RefCell<Shared>>) {
    let wins: Vec<(u64, Window)> = sh.borrow().pad_wins.iter().map(|(k, v)| (*k, *v)).collect();
    for (id, w) in &wins {
        let text = w.with(|h, _| with_pad(h).map(|p| p.ed.doc.text())).flatten();
        let r = sys::window_rect(w.hwnd);
        let mut s = sh.borrow_mut();
        if let Some(p) = s.pads.get_mut(*id) {
            if p.rect != r && r.2 > 0 {
                p.rect = r;
                s.pads_dirty = true;
            }
            if let Some(t) = text
                && s.pads.get(*id).is_some_and(|p| p.text != t)
            {
                if let Some(p) = s.pads.get_mut(*id) {
                    p.text = t;
                }
                s.pads_dirty = true;
            }
        }
    }
    let mut s = sh.borrow_mut();
    if s.pads_dirty {
        let dir = s.dir.clone();
        if s.pads.save(&dir) {
            s.pads_dirty = false;
        }
    }
}

/// The theme or the language changed.
pub fn retheme(sh: &Rc<RefCell<Shared>>) {
    let (wins, dark): (Vec<Window>, bool) = {
        let s = sh.borrow();
        (s.pad_wins.values().copied().collect(), s.theme.dark)
    };
    for w in wins {
        w.set_dark(dark);
        w.repaint();
    }
}

/// The icon near the clock: there while there are pads, or while they open at sign-in.
pub fn refresh_tray(sh: &Rc<RefCell<Shared>>) {
    let (main, want, n) = {
        let s = sh.borrow();
        (s.main, !s.pads.list.is_empty() || crate::registry::run_on(&crate::model::user_root()), s.pads.list.len())
    };
    let Some(m) = main else { return };
    if want {
        let tip = if n == 1 { tr("Plume Notepad: 1 pad").to_string() } else { fill(tr("Plume Notepad: {n} pads"), &[&n.to_string()]) };
        sys::tray_add(m.hwnd, &tip);
    } else {
        sys::tray_remove(m.hwnd);
    }
}

/// The menu of the icon near the clock.
pub fn tray_items(sh: &Shared) -> Vec<sys::Item> {
    let mut items = vec![sys::item(1, format!("{}\tCtrl+Alt+N", tr("New pad"))), sys::sep()];
    for (i, p) in sh.pads.list.iter().enumerate().take(20) {
        let title = p.title();
        let mut it = sys::item(100 + i as u32, if title.is_empty() { tr("(empty pad)").to_string() } else { title });
        it.checked = p.open;
        items.push(it);
    }
    if !sh.pads.list.is_empty() {
        items.push(sys::sep());
        items.push(sys::item(2, tr("Show all pads")));
        items.push(sys::item(3, tr("Hide all pads")));
        items.push(sys::sep());
    }
    items.push(sys::item(4, tr("Open Plume Notepad")));
    items.push(sys::item(5, tr("Quit")));
    items
}

/// What a command of the tray menu does.
pub fn tray_command(sh: &Rc<RefCell<Shared>>, cmd: u32) {
    let activate = !sh.borrow().noactivate;
    match cmd {
        1 => {
            new_pad(sh, activate);
        }
        2 => show_all(sh, activate),
        3 => hide_all(sh),
        4 => {
            if let Some(m) = sh.borrow().main {
                sys::show(m.hwnd, true, activate);
            }
        }
        5 => crate::quit(sh),
        100..=119 => {
            let p = sh.borrow().pads.list.get((cmd - 100) as usize).map(|p| (p.id, p.open));
            match p {
                Some((id, true)) => close_pad(sh, id),
                Some((id, false)) => open_pad(sh, id, activate),
                None => {}
            }
        }
        _ => {}
    }
    flush(sh);
}
