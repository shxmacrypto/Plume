//! The small settings window: the shortcut, the sources searched, the web search engine, the look, the start
//! with Windows, the file index and the history.

use crate::{
    i18n::{self, fill, tr},
    settings::ENGINES,
    sys::{self, Hotkey},
    ui::{Shared, theme_for},
};
use plume_ui::{Align, Area, Button, Canvas, Color, Cursor, Cx, Event, Font, Gfx, Handler, Hits, PaintCx, Pt, Rect, Theme, Window, rect, window};
use std::{any::Any, rc::Rc, sync::atomic::Ordering};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_LWIN, VK_RWIN};

const TITLE_H: f32 = 46.0;
const MARGIN: f32 = 26.0;
const TIMER: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Close,
    Hotkey,
    Toggle(&'static str),
    Choice(&'static str, usize),
    Rebuild,
    ClearHistory,
}

pub struct SettingsApp {
    pub win: Window,
    sh: Rc<Shared>,
    theme: Theme,
    pub hits: Hits<Hit>,
    hover: Option<Hit>,
    /// Waiting for the new shortcut to be pressed.
    pub capturing: bool,
    pub note: Option<String>,
    scroll: f32,
    content_h: f32,
}

/// Opens the settings window (or brings it forward).
pub fn open(gfx: &Rc<Gfx>, sh: &Rc<Shared>, activate: bool) {
    if let Some(w) = sh.settings_win.get() {
        w.bring_to_front();
        return;
    }
    let theme = theme_for(&sh.set.borrow());
    let o = window::Options {
        title: tr("Plume Launcher settings").into(),
        size: (540.0, 720.0),
        min: (460.0, 420.0),
        dark: theme.dark,
        background: theme.bg,
        activate: activate && !sys::test_mode(),
        placement: None,
    };
    let s = sh.clone();
    if let Ok(w) = window::create(gfx, o, move |win| {
        Box::new(SettingsApp { win, sh: s, theme, hits: Hits::default(), hover: None, capturing: false, note: None, scroll: 0.0, content_h: 0.0 })
    }) {
        sh.settings_win.set(Some(w));
        w.timer(TIMER, Some(1000));
    }
}

impl SettingsApp {
    fn popup(&self) -> Option<Window> {
        self.sh.popup.get()
    }

    #[allow(clippy::too_many_arguments)]
    fn row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, sub: &str, sub_color: Option<Color>) -> Rect {
        let t = self.theme.clone();
        let h = if sub.is_empty() { 48.0 } else { 60.0 };
        let r = rect(x, y, w, h);
        if sub.is_empty() {
            c.text(label, rect(x, y, w * 0.55, h), Font::Body, t.t1, Align::Left);
        } else {
            c.text(label, rect(x, y + 9.0, w * 0.6, 20.0), Font::Body, t.t1, Align::Left);
            c.text(sub, rect(x, y + 31.0, w * 0.7, 18.0), Font::Small, sub_color.unwrap_or(t.t3), Align::Left);
        }
        c.hline(x, y + h, w, t.sep);
        r
    }

    fn toggle(&mut self, c: &mut Canvas, r: Rect, on: bool, name: &'static str) {
        let t = self.theme.clone();
        let sw = rect(r.right() - 44.0, r.y + (r.h - 24.0) / 2.0, 44.0, 24.0);
        c.fill_round(sw, 12.0, if on { t.accent } else { t.fill2 });
        let k = if on { sw.right() - 21.0 } else { sw.x + 3.0 };
        c.fill_round(rect(k, sw.y + 3.0, 18.0, 18.0), 9.0, Color::hex(0xffffff));
        self.hits.add(r, Hit::Toggle(name));
    }

    fn choices(&mut self, c: &mut Canvas, r: Rect, labels: &[&str], cur: usize, name: &'static str) {
        let t = self.theme.clone();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::Small) + 22.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 6.0;
        let bg = rect(r.right() - total, r.y + (r.h - 32.0) / 2.0, total, 32.0);
        c.fill_round(bg, 16.0, t.fill);
        let mut x = bg.x + 3.0;
        for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
            let b = rect(x, bg.y + 3.0, w, 26.0);
            let hit = Hit::Choice(name, i);
            if i == cur {
                c.fill_round(b, 13.0, t.accent);
            } else if self.hover == Some(hit) {
                c.fill_round(b, 13.0, t.hover);
            }
            c.text(l, b, Font::Small, if i == cur { Color::hex(0xffffff) } else { t.t1 }, Align::Center);
            self.hits.add(b, hit);
            x += w;
        }
    }

    fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, strong: bool, hit: Hit) {
        let t = self.theme.clone();
        let w = c.text_width(label, Font::SmallBold) + 30.0;
        let b = rect(r.right() - w, r.y + (r.h - 30.0) / 2.0, w, 30.0);
        let bg = if strong {
            t.accent.fade(0.22)
        } else if self.hover == Some(hit) {
            t.fill2
        } else {
            t.fill
        };
        c.fill_round(b, 15.0, bg);
        if strong {
            c.stroke_round(b, 15.0, t.accent, 1.5);
        }
        c.text(label, b, Font::SmallBold, if strong { t.accent } else { t.t1 }, Align::Center);
        self.hits.add(b, hit);
    }

    fn group(&self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        c.text(title, rect(x, y, w, 24.0), Font::SmallBold, self.theme.t3, Align::Left);
        y + 30.0
    }

    fn index_line(&self) -> String {
        let f = &self.sh.files;
        if f.building.load(Ordering::Relaxed) {
            return tr("Indexing your folders…").into();
        }
        let n = f.count.load(Ordering::Relaxed);
        let built = f.built.load(Ordering::Relaxed);
        let ago = crate::rank::now().saturating_sub(built) / 60;
        let when = if built == 0 {
            String::new()
        } else if ago < 1 {
            tr("updated just now").into()
        } else if ago < 120 {
            fill(tr("updated {n} min ago"), &[&ago.to_string()])
        } else {
            fill(tr("updated {n} h ago"), &[&(ago / 60).to_string()])
        };
        let count = fill(tr("{n} items"), &[&n.to_string()]);
        if when.is_empty() { count } else { format!("{count}, {when}") }
    }

    fn paint_content(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32) -> f32 {
        let set = self.sh.set.borrow().clone();
        let mut y = y;

        y = self.group(c, x, y, w, tr("Shortcut"));
        let (sub, color) = if self.capturing {
            (tr("Press the new shortcut, or Esc to cancel").to_string(), None)
        } else if let Some(n) = &self.note {
            (n.clone(), Some(self.theme.orange))
        } else if !self.sh.hotkey_ok.get() {
            (tr("Another app already uses this shortcut: choose another one").to_string(), Some(self.theme.red))
        } else {
            (tr("Opens the search box from anywhere").to_string(), None)
        };
        let r = self.row(c, x, y, w, tr("Open Plume Launcher"), &sub, color);
        let label = if self.capturing { tr("Press keys…").to_string() } else { Hotkey::parse(&set.hotkey).map_or(set.hotkey.clone(), |h| h.label()) };
        let capturing = self.capturing;
        self.pill(c, r, &label, capturing, Hit::Hotkey);
        y += r.h + 26.0;

        y = self.group(c, x, y, w, tr("Search in"));
        let rows: [(&'static str, &str, String, bool); 5] = [
            ("apps", tr("Apps"), tr("Start menu and Store apps").into(), set.apps),
            ("files", tr("Files and folders"), self.index_line(), set.files),
            ("settings", tr("Windows settings"), String::new(), set.settings),
            ("calc", tr("Calculator"), tr("Sums, percentages and units, like 15% of 80 or 5 km in mi").into(), set.calc),
            ("web", tr("Web search"), tr("When nothing else matches, search the web").into(), set.web),
        ];
        for (name, label, sub, on) in rows {
            let r = self.row(c, x, y, w, label, &sub, None);
            self.toggle(c, r, on, name);
            y += r.h + 1.0;
        }
        let r = self.row(c, x, y, w, tr("Search engine"), "", None);
        let cur = ENGINES.iter().position(|e| e.0 == set.engine).unwrap_or(0);
        let names: Vec<&str> = ENGINES.iter().map(|e| e.1).collect();
        self.choices(c, r, &names, cur, "engine");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("File index"), tr("Desktop, Documents, Downloads, Pictures, Music, Videos, OneDrive"), None);
        self.pill(c, r, tr("Rebuild"), false, Hit::Rebuild);
        y += r.h + 26.0;

        y = self.group(c, x, y, w, tr("General"));
        let r = self.row(c, x, y, w, tr("Theme"), "", None);
        let cur = ["system", "dark", "light"].iter().position(|t| *t == set.theme).unwrap_or(0);
        self.choices(c, r, &[tr("System"), tr("Dark"), tr("Light")], cur, "theme");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Language"), "", None);
        let cur = ["system", "en", "fr"].iter().position(|t| *t == set.lang).unwrap_or(0);
        self.choices(c, r, &[tr("System"), "English", "Français"], cur, "lang");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Start with Windows"), tr("Runs in the background, with an icon near the clock"), None);
        let on = sys::autostart();
        self.toggle(c, r, on, "autostart");
        y += r.h + 1.0;
        let n = self.sh.history.borrow().picks.len();
        let r = self.row(c, x, y, w, tr("History"), &fill(tr("{n} results remembered, to put your picks first"), &[&n.to_string()]), None);
        self.pill(c, r, tr("Clear"), false, Hit::ClearHistory);
        y += r.h + 22.0;
        c.text(&format!("Plume Launcher {}", plume_update::VERSION), rect(x, y, w, 20.0), Font::Small, self.theme.t4, Align::Left);
        y + 40.0
    }

    fn start_capture(&mut self, cx: &mut Cx) {
        self.capturing = true;
        self.note = None;
        // the current shortcut must reach this window, not call the box
        if let Some(p) = self.popup() {
            sys::unregister_hotkey(p.hwnd);
        }
        cx.repaint();
    }

    fn end_capture(&mut self, new: Option<Hotkey>, cx: &mut Cx) {
        self.capturing = false;
        if let Some(h) = new {
            self.sh.set.borrow_mut().hotkey = h.label();
            self.sh.set.borrow().save();
        }
        let hk = Hotkey::parse(&self.sh.set.borrow().hotkey);
        if let (Some(p), Some(hk)) = (self.popup(), hk) {
            let ok = sys::test_mode() && std::env::var_os("PLUME_LAUNCHER_HOTKEY").is_none() || sys::register_hotkey(p.hwnd, &hk);
            self.sh.hotkey_ok.set(ok);
        }
        cx.repaint();
    }

    /// A key pressed while waiting for the new shortcut.
    pub fn capture_key(&mut self, vk: u16, ctrl: bool, alt: bool, shift: bool, cx: &mut Cx) {
        // modifiers alone: wait for the key
        if matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5) {
            return;
        }
        if vk == 0x1B {
            self.end_capture(None, cx);
            return;
        }
        let win = unsafe { GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0 };
        let h = Hotkey { ctrl, alt, shift, win, vk: vk as u32 };
        if h.valid() {
            self.end_capture(Some(h), cx);
        } else {
            self.note = Some(tr("Add Ctrl, Alt or Win to the key").into());
            cx.repaint();
        }
    }

    pub fn click(&mut self, hit: Hit, cx: &mut Cx) {
        match hit {
            Hit::Close => self.win.close(),
            Hit::Hotkey => {
                if self.capturing {
                    self.end_capture(None, cx);
                } else {
                    self.start_capture(cx);
                }
            }
            Hit::Toggle("autostart") => {
                sys::set_autostart(!sys::autostart());
            }
            Hit::Toggle(name) => {
                {
                    let mut s = self.sh.set.borrow_mut();
                    let v = match name {
                        "apps" => &mut s.apps,
                        "files" => &mut s.files,
                        "settings" => &mut s.settings,
                        "calc" => &mut s.calc,
                        _ => &mut s.web,
                    };
                    *v = !*v;
                    s.save();
                }
                if name == "files" && self.sh.set.borrow().files {
                    self.sh.files.rebuild();
                }
            }
            Hit::Choice(name, i) => {
                {
                    let mut s = self.sh.set.borrow_mut();
                    match name {
                        "engine" => s.engine = ENGINES[i.min(ENGINES.len() - 1)].0.into(),
                        "theme" => s.theme = ["system", "dark", "light"][i.min(2)].into(),
                        _ => s.lang = ["system", "en", "fr"][i.min(2)].into(),
                    }
                    s.save();
                }
                if name == "lang" {
                    i18n::set(&self.sh.set.borrow().lang);
                    self.win.set_title(tr("Plume Launcher settings"));
                }
                if name == "theme" {
                    self.theme = theme_for(&self.sh.set.borrow());
                    self.win.set_dark(self.theme.dark);
                }
            }
            Hit::Rebuild => self.sh.files.rebuild(),
            Hit::ClearHistory => {
                *self.sh.history.borrow_mut() = Default::default();
                self.sh.save_history();
            }
        }
        cx.repaint();
    }
}

impl Handler for SettingsApp {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let t = self.theme.clone();
        c.clear(t.bg);
        self.hits.clear();
        // the content, under the title bar
        let max = (self.content_h - (cx.h - TITLE_H)).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
        c.clip(rect(0.0, TITLE_H, cx.w, cx.h - TITLE_H));
        let w = (cx.w - 2.0 * MARGIN).min(620.0);
        let x = (cx.w - w) / 2.0;
        let top = TITLE_H + 12.0 - self.scroll;
        let end = self.paint_content(c, x, top, w);
        self.content_h = end - top + 12.0;
        c.unclip();
        // the title bar
        c.fill(rect(0.0, 0.0, cx.w, TITLE_H), t.bg);
        if self.scroll > 0.5 {
            c.hline(0.0, TITLE_H, cx.w, t.sep);
        }
        c.text(tr("Plume Launcher settings"), rect(MARGIN - 6.0, 0.0, cx.w - 120.0, TITLE_H), Font::Title, if cx.active { t.t1 } else { t.t3 }, Align::Left);
        let close = rect(cx.w - 46.0, 0.0, 46.0, TITLE_H - 6.0);
        if self.hover == Some(Hit::Close) {
            c.fill(close, t.close_hover);
        }
        c.glyph('\u{E8BB}', close, if self.hover == Some(Hit::Close) { Color::hex(0xffffff) } else { t.t2 });
        // the title bar's hit is added last: it is above the scrolled content
        self.hits.add(close, Hit::Close);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::Key { vk, mods, .. } if self.capturing => {
                self.capture_key(vk, mods.ctrl, mods.alt, mods.shift, cx);
                cx.handled();
            }
            Event::Key { vk: 0x1B, .. } => self.win.close(),
            Event::MouseMove { p, .. } => {
                let h = if p.y < TITLE_H && p.x < self.win_w() - 46.0 { None } else { self.hits.at(p).copied() };
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
                let close = p.y < TITLE_H && p.x >= self.win_w() - 46.0;
                if close {
                    self.click(Hit::Close, cx);
                } else if p.y >= TITLE_H
                    && let Some(h) = self.hits.at(p).copied()
                {
                    self.click(h, cx);
                }
            }
            Event::Wheel { dy, .. } => {
                self.scroll -= dy * 60.0;
                cx.repaint();
            }
            Event::Active(on) => {
                if !on && self.capturing {
                    self.end_capture(None, cx);
                }
                cx.repaint();
            }
            Event::Timer(TIMER) | Event::Resized | Event::Scale => cx.repaint(),
            Event::SystemTheme => {
                self.theme = theme_for(&self.sh.set.borrow());
                cx.repaint();
            }
            Event::Close => {
                if self.capturing {
                    self.end_capture(None, cx);
                }
                self.sh.settings_win.set(None);
            }
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        if p.y < TITLE_H { if p.x >= self.win_w() - 46.0 { Area::Control } else { Area::Caption } } else { Area::Client }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        if p.y >= TITLE_H && self.hits.at(p).is_some() { Cursor::Hand } else { Cursor::Arrow }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl SettingsApp {
    fn win_w(&self) -> f32 {
        let mut r = windows::Win32::Foundation::RECT::default();
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(self.win.hwnd, &mut r);
        }
        (r.right - r.left) as f32 / self.win.scale()
    }
}
