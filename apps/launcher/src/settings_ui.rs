//! The small settings window: the shortcut, the sources searched, the web search engine, the look, the start
//! with Windows, the file index and the history.

use crate::{
    i18n::{self, fill, tr},
    settings::ENGINES,
    sys::{self, Hotkey},
    ui::{Shared, theme_for},
};
use plume_ui::{Align, Area, Button, Canvas, Color, Cursor, Cx, Event, Font, Gfx, Handler, Hits, PaintCx, Pt, Rect, Theme, Window, m, rect, window};
use std::{any::Any, rc::Rc, sync::atomic::Ordering};
use windows::{
    Win32::{
        Graphics::Imaging::{
            GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
            WICDecodeMetadataCacheOnLoad,
        },
        UI::Input::KeyboardAndMouse::{GetKeyState, VK_LWIN, VK_RWIN},
    },
    core::Interface,
};

const TITLE_H: f32 = m::TITLE_H;
const MARGIN: f32 = 24.0;
/// Inner padding of a card.
const PAD: f32 = 20.0;
const TIMER: usize = 1;
const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-launcher-256.png");

/// A PNG of the program, decoded for drawing.
fn png(wic: &IWICImagingFactory, bytes: &'static [u8]) -> Option<IWICBitmapSource> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let decoder = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        conv.cast::<IWICBitmapSource>().ok()
    }
}

/// What sits on the right of a settings row.
enum Control {
    Toggle(&'static str, bool),
    Choice(&'static str, Vec<String>, usize),
    /// A button: label, what it does, and whether it has the focus (the shortcut being recorded).
    Pill(String, Hit, bool),
}

struct Row {
    label: String,
    sub: String,
    sub_color: Option<Color>,
    control: Control,
}

fn row(label: &str, sub: &str, control: Control) -> Row {
    Row { label: label.into(), sub: sub.into(), sub_color: None, control }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Min,
    Max,
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
    pressed: Option<Hit>,
    gfx: Rc<Gfx>,
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
        size: (560.0, 740.0),
        min: (460.0, 420.0),
        dark: theme.dark,
        background: theme.bg,
        activate: activate && !sys::test_mode(),
        placement: None,
    };
    let (s, g) = (sh.clone(), gfx.clone());
    if let Ok(w) = window::create(gfx, o, move |win| {
        Box::new(SettingsApp {
            win,
            sh: s,
            theme,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            gfx: g,
            capturing: false,
            note: None,
            scroll: 0.0,
            content_h: 0.0,
        })
    }) {
        sh.settings_win.set(Some(w));
        w.timer(TIMER, Some(1000));
    }
}

impl SettingsApp {
    fn popup(&self) -> Option<Window> {
        self.sh.popup.get()
    }

    /// The iOS-style switch, on the right of `r`.
    fn toggle(&mut self, c: &mut Canvas, r: Rect, on: bool, name: &'static str) {
        let t = self.theme.clone();
        let sw = rect(r.right() - 48.0, r.y + (r.h - 28.0) / 2.0, 48.0, 28.0);
        c.fill_round(sw, 14.0, if on { t.accent } else { t.fill2 });
        let k = if on { sw.right() - 25.0 } else { sw.x + 3.0 };
        c.fill_round(rect(k, sw.y + 3.0, 22.0, 22.0), 11.0, Color::hex(0xffffff));
        self.hits.add(r, Hit::Toggle(name));
    }

    /// The segmented control: a pill track, the chosen segment raised.
    fn choices(&mut self, c: &mut Canvas, r: Rect, labels: &[String], cur: usize, name: &'static str) {
        let t = self.theme.clone();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::SmallBold) + 24.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 8.0;
        let track = rect(r.right() - total, r.y + (r.h - 32.0) / 2.0, total, 32.0);
        c.fill_round(track, 16.0, t.fill);
        let mut x = track.x + 4.0;
        for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
            let b = rect(x, track.y + 4.0, w, 24.0);
            let hit = Hit::Choice(name, i);
            if i == cur {
                c.fill_round(b, 12.0, t.fill2);
                c.stroke_round(b, 12.0, t.ring, 1.0);
            } else if self.hover == Some(hit) {
                c.fill_round(b, 12.0, t.hover);
            }
            let (font, ink) = if i == cur { (Font::SmallBold, t.t1) } else { (Font::Small, t.t2) };
            c.text(l, b, font, ink, Align::Center);
            self.hits.add(b, hit);
            x += w;
        }
    }

    /// A pill button on the right of `r`. `focus`: drawn with the accent (the shortcut being recorded).
    fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, focus: bool, hit: Hit) {
        let t = self.theme.clone();
        let w = c.text_width(label, Font::BodyBold) + 32.0;
        let b = rect(r.right() - w, r.y + (r.h - 32.0) / 2.0, w, 32.0);
        let bg = if focus {
            t.accent.fade(0.16)
        } else if self.pressed == Some(hit) {
            t.fill2.mix(t.t1, 0.06)
        } else if self.hover == Some(hit) {
            t.fill2
        } else {
            t.fill
        };
        c.fill_round(b, 16.0, bg);
        if focus {
            c.stroke_round(b, 16.0, t.accent, 1.5);
        }
        c.text(label, b, Font::BodyBold, if focus { t.accent } else { t.t1 }, Align::Center);
        self.hits.add(b, hit);
    }

    /// A titled card of rows; returns where the next one goes.
    fn section(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, rows: Vec<Row>) -> f32 {
        let t = self.theme.clone();
        c.text(title, rect(x + 4.0, y, w, 24.0), Font::Title, t.t1, Align::Left);
        let top = y + 32.0;
        let heights: Vec<f32> = rows.iter().map(|r| if r.sub.is_empty() { 52.0 } else { 64.0 }).collect();
        let card = rect(x, top, w, heights.iter().sum());
        c.fill_round(card, 16.0, t.card);
        c.stroke_round(card, 16.0, t.sep, 1.0);
        let (ix, iw) = (x + PAD, w - 2.0 * PAD);
        let mut ry = top;
        let n = rows.len();
        for (k, (row, h)) in rows.into_iter().zip(heights).enumerate() {
            let r = rect(ix, ry, iw, h);
            let text_w = iw * 0.62;
            if row.sub.is_empty() {
                c.text(&row.label, rect(ix, ry, text_w, h), Font::Body, t.t1, Align::Left);
            } else {
                c.text(&row.label, rect(ix, ry + 12.0, text_w, 20.0), Font::Body, t.t1, Align::Left);
                c.text(&row.sub, rect(ix, ry + 34.0, iw * 0.7, 18.0), Font::Small, row.sub_color.unwrap_or(t.t3), Align::Left);
            }
            match row.control {
                Control::Toggle(name, on) => self.toggle(c, r, on, name),
                Control::Choice(name, labels, cur) => self.choices(c, r, &labels, cur, name),
                Control::Pill(label, hit, focus) => self.pill(c, r, &label, focus, hit),
            }
            if k + 1 < n {
                c.hline(ix, ry + h, iw, t.sep);
            }
            ry += h;
        }
        card.bottom() + 28.0
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
        let t = self.theme.clone();
        let set = self.sh.set.borrow().clone();
        let mut y = y;
        c.text(tr("Settings"), rect(x + 4.0, y, w, 34.0), Font::Hero, t.t1, Align::Left);
        c.text(tr("What the search box looks in, and how it opens"), rect(x + 4.0, y + 38.0, w, 20.0), Font::Body, t.t3, Align::Left);
        y += 80.0;

        let (sub, color) = if self.capturing {
            (tr("Press the new shortcut, or Esc to cancel").to_string(), None)
        } else if let Some(n) = &self.note {
            (n.clone(), Some(t.orange))
        } else if !self.sh.hotkey_ok.get() {
            (tr("Another app already uses this shortcut: choose another one").to_string(), Some(t.red))
        } else {
            (tr("Opens the search box from anywhere").to_string(), None)
        };
        let label = if self.capturing { tr("Press keys…").to_string() } else { Hotkey::parse(&set.hotkey).map_or(set.hotkey.clone(), |h| h.label()) };
        let mut shortcut = row(tr("Open Plume Launcher"), &sub, Control::Pill(label, Hit::Hotkey, self.capturing));
        shortcut.sub_color = color;
        y = self.section(c, x, y, w, tr("Shortcut"), vec![shortcut]);

        let engines: Vec<String> = ENGINES.iter().map(|e| e.1.to_string()).collect();
        let engine = ENGINES.iter().position(|e| e.0 == set.engine).unwrap_or(0);
        let rows = vec![
            row(tr("Apps"), tr("Start menu and Store apps"), Control::Toggle("apps", set.apps)),
            row(tr("Files and folders"), &self.index_line(), Control::Toggle("files", set.files)),
            row(tr("Windows settings"), "", Control::Toggle("settings", set.settings)),
            row(tr("Calculator"), tr("Sums, percentages and units, like 15% of 80 or 5 km in mi"), Control::Toggle("calc", set.calc)),
            row(tr("Web search"), tr("When nothing else matches, search the web"), Control::Toggle("web", set.web)),
            row(tr("Search engine"), "", Control::Choice("engine", engines, engine)),
            row(
                tr("File index"),
                tr("Desktop, Documents, Downloads, Pictures, Music, Videos, OneDrive"),
                Control::Pill(tr("Rebuild").into(), Hit::Rebuild, false),
            ),
        ];
        y = self.section(c, x, y, w, tr("Search in"), rows);

        let theme = ["system", "dark", "light"].iter().position(|t| *t == set.theme).unwrap_or(0);
        let lang = ["system", "en", "fr"].iter().position(|t| *t == set.lang).unwrap_or(0);
        let n = self.sh.history.borrow().picks.len();
        let rows = vec![
            row(tr("Theme"), "", Control::Choice("theme", vec![tr("System").into(), tr("Dark").into(), tr("Light").into()], theme)),
            row(tr("Language"), "", Control::Choice("lang", vec![tr("System").into(), "English".into(), "Français".into()], lang)),
            row(tr("Start with Windows"), tr("Runs in the background, with an icon near the clock"), Control::Toggle("autostart", sys::autostart())),
            row(
                tr("History"),
                &fill(tr("{n} results remembered, to put your picks first"), &[&n.to_string()]),
                Control::Pill(tr("Clear").into(), Hit::ClearHistory, false),
            ),
        ];
        y = self.section(c, x, y, w, tr("General"), rows);
        c.text(&format!("Plume Launcher {}", plume_update::VERSION), rect(x + 4.0, y - 8.0, w, 20.0), Font::Small, t.t3, Align::Left);
        y + 32.0
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
            Hit::Min => self.win.minimize(),
            Hit::Max => self.win.toggle_maximize(),
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
        let w = (cx.w - 2.0 * MARGIN).min(640.0);
        let x = (cx.w - w) / 2.0;
        let top = TITLE_H + 16.0 - self.scroll;
        let end = self.paint_content(c, x, top, w);
        self.content_h = end - top + 16.0;
        c.unclip();
        // the title bar: logo, name, the window's buttons
        c.gradient_v(rect(0.0, 0.0, cx.w, TITLE_H), t.hdr1, t.hdr2);
        if self.scroll > 0.5 {
            c.hline(0.0, TITLE_H, cx.w, t.sep);
        }
        let wic = self.gfx.wic.clone();
        c.image(1, rect(14.0, (TITLE_H - 18.0) / 2.0, 18.0, 18.0), 1.0, || png(&wic, LOGO));
        c.text("Plume Launcher", rect(40.0, 0.0, cx.w - 200.0, TITLE_H), Font::BodyBold, if cx.active { t.t1 } else { t.t3 }, Align::Left);
        let cap_x = cx.w - 3.0 * m::CAPTION_W;
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, TITLE_H - 8.0);
            let hot = self.hover == Some(id) || self.pressed == Some(id);
            if hot {
                c.fill(r, if id == Hit::Close { t.close_hover } else { t.hover });
            }
            let ink = match (id == Hit::Close && hot, cx.active) {
                (true, _) => Color::hex(0xffffff),
                (false, true) => t.t1,
                (false, false) => t.t3,
            };
            c.glyph(glyph, r, ink);
            // added last: above the scrolled content
            self.hits.add(r, id);
        }
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::Key { vk, mods, .. } if self.capturing => {
                self.capture_key(vk, mods.ctrl, mods.alt, mods.shift, cx);
                cx.handled();
            }
            Event::Key { vk: 0x1B, .. } => self.win.close(),
            Event::MouseMove { p, .. } => {
                let h = self.hit_at(p);
                if h != self.hover {
                    self.hover = h;
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                self.hover = None;
                self.pressed = None;
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Left, .. } => {
                if let Some(h) = self.hit_at(p) {
                    self.pressed = Some(h);
                    // the window's buttons act on release, like Windows'; the rest at once
                    if !matches!(h, Hit::Min | Hit::Max | Hit::Close) {
                        self.click(h, cx);
                    }
                    cx.repaint();
                }
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                let was = self.pressed.take();
                if let Some(h) = was.filter(|h| matches!(h, Hit::Min | Hit::Max | Hit::Close))
                    && self.hit_at(p) == Some(h)
                {
                    self.click(h, cx);
                }
                cx.repaint();
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
        if p.y >= TITLE_H {
            return Area::Client;
        }
        match self.hits.at(p) {
            Some(Hit::Max) => Area::MaxButton,
            Some(Hit::Min | Hit::Close) => Area::Control,
            _ => Area::Caption,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        if p.y >= TITLE_H && self.hits.at(p).is_some() { Cursor::Hand } else { Cursor::Arrow }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl SettingsApp {
    /// What is under the pointer: in the title bar only the window's buttons, below it the content.
    fn hit_at(&self, p: Pt) -> Option<Hit> {
        let h = self.hits.at(p).copied();
        if p.y < TITLE_H { h.filter(|h| matches!(h, Hit::Min | Hit::Max | Hit::Close)) } else { h }
    }
}
