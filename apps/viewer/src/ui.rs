//! The window of Plume Viewer: this PC's ID, its one-time code and their QR code, the permanent password, and what
//! happens (someone asks to connect, someone is connected). The work runs elsewhere: the link to the relay on its
//! thread, each session on its own; they report here through the window's sender.

use crate::{
    crypto,
    gate::{self, Action, Gate, Mode, Ready, Secrets},
    i18n::{Strings, fill},
    relay::{self, FromRelay, Relay},
    rtc::{self, SessionEvent},
    store::Store,
    sys,
};
use plume_ui::{Align, Area, Button, Canvas, Color, Cursor, Cx, EditStyle, Event, Font, Gfx, Handler, Hits, PaintCx, Pt, Rect, Theme, Window, m, rect};
use serde_json::json;
use std::{
    any::Any,
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::{
    Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
        WICDecodeMetadataCacheOnLoad,
    },
    core::Interface,
};

const TICK: usize = 1;
/// The person at the PC has this long to answer.
pub const ASK_FOR: Duration = Duration::from_secs(60);
const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-viewer-256.png");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Min,
    Max,
    Close,
    CopyNumber,
    CopyCode,
    NewCode,
    SetPassword,
    RemovePassword,
    SavePassword,
    CancelPassword,
    Autostart,
    OpenViewer,
    Allow,
    Refuse,
    Disconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Net {
    Connecting,
    Online,
    Offline,
}

pub struct Ask {
    pub from: String,
    pub name: String,
    pub since: Instant,
}

pub struct Live {
    pub name: String,
    pub mode: Mode,
    pub since: Instant,
    pub linked: bool,
    session: rtc::Session,
    /// The viewer of this session, on the relay, and whether it left before the link (a phone that went to sleep
    /// while the person at the PC accepted): the next viewer then takes its place instead of being told "busy".
    from: String,
    viewer_gone: bool,
    /// Which session this is: the events of a session that was replaced are ignored.
    run: u64,
}

pub struct App {
    pub win: Window,
    gfx: Rc<Gfx>,
    pub theme: Theme,
    pub s: &'static Strings,
    pub hits: Hits<Id>,
    pub hover: Option<Id>,
    pressed: Option<Id>,
    pub store: Store,
    pub code: String,
    relay: Relay,
    pub net: Net,
    pub gate: Gate,
    pw_key: Option<Vec<u8>>,
    pub asks: Vec<Ask>,
    pub live: Option<Live>,
    /// Sessions started so far (each one's number, see `Live::run`).
    runs: u64,
    pub editing: bool,
    edit_pending: bool,
    pub autostart: bool,
    pub toast: Option<(String, Instant)>,
    copied: Option<(Id, Instant)>,
    /// Read by the window hook: closing the window hides it instead (password set, or a session).
    pub keep: Rc<Cell<bool>>,
    /// What happened, for the tests and the logs.
    pub log: Vec<String>,
    /// Asked to quit for good (uninstall): the window really closes.
    pub quitting: bool,
}

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

/// 123456789 → "123 456 789"; abcdefgh → "abcd efgh".
pub fn spaced(s: &str, every: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && i % every == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

fn start_relay(win: &Window, store: &Store) -> Relay {
    let sender = win.sender();
    relay::start(&store.relay_url(), &store.id, &store.token, move |ev| {
        sender.send(move |h, cx| {
            if let Some(app) = with_app(h) {
                app.on_relay(ev, cx);
            }
        });
    })
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, theme: Theme, s: &'static Strings, keep: Rc<Cell<bool>>) -> App {
        let store = Store::load();
        let relay = start_relay(&win, &store);
        let pw_key = store.password_key();
        let mut app = App {
            win,
            gfx,
            theme,
            s,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            code: std::env::var("PLUME_VIEWER_CODE").ok().filter(|_| cfg!(feature = "test-hook")).unwrap_or_else(crypto::new_code),
            store,
            relay,
            net: Net::Connecting,
            gate: Gate::default(),
            pw_key,
            asks: Vec::new(),
            live: None,
            runs: 0,
            editing: false,
            edit_pending: false,
            autostart: sys::autostart(),
            toast: None,
            copied: None,
            keep,
            log: Vec::new(),
            quitting: false,
        };
        app.update_keep();
        app
    }

    pub fn site(&self) -> String {
        self.store.relay_url()
    }

    /// What the QR code holds: the viewer page, with the ID and code after # (never sent to any server).
    pub fn qr_text(&self) -> String {
        format!("{}/#{}.{}", self.site(), self.store.id, self.code)
    }

    fn say(&mut self, text: String, cx: &mut Cx) {
        self.toast = Some((text, Instant::now()));
        cx.timer(TICK, Some(500));
        cx.repaint();
    }

    fn note(&mut self, what: String) {
        if self.log.len() > 200 {
            self.log.remove(0);
        }
        self.log.push(what);
    }

    pub fn update_keep(&mut self) {
        self.keep.set(!self.quitting && (self.pw_key.is_some() || self.live.is_some() || !self.asks.is_empty()));
    }

    pub fn tray_tip(&self) -> String {
        fill(self.s.tray_tip, &[("id", &spaced(&self.store.id, 3))])
    }

    // ───────────────────────── relay and sessions ─────────────────────────

    pub fn on_relay(&mut self, ev: FromRelay, cx: &mut Cx) {
        match ev {
            FromRelay::Online => {
                self.net = Net::Online;
                self.note("online".into());
            }
            FromRelay::Offline(e) => {
                self.net = Net::Offline;
                self.note(format!("offline: {e}"));
            }
            FromRelay::Taken => {
                // the relay knows this ID with another key (the settings were lost): a new ID
                self.note("id taken".into());
                self.store.id = crypto::new_id();
                self.store.token = crypto::hex(&crypto::random(24));
                self.store.save();
                self.relay = start_relay(&self.win, &self.store);
                sys::tray_add(self.win.hwnd, &self.tray_tip());
            }
            FromRelay::Join(from) => self.note(format!("join {from}")),
            FromRelay::Gone(from) => {
                self.note(format!("gone {from}"));
                if let Some(l) = &mut self.live
                    && !l.linked
                    && l.from == from
                {
                    l.viewer_gone = true;
                }
                if self.gate.gone(&from) {
                    self.asks.retain(|a| a.from != from);
                }
                // a viewer that leaves the relay once linked is normal (it does not need it any more)
            }
            FromRelay::Msg(from, msg) => {
                self.note(format!("msg {from} {}", msg["t"].as_str().unwrap_or("?")));
                // the accepted viewer left before the link (or never linked: a link takes a few seconds), this one takes
                // its place
                if self.live.as_ref().is_some_and(|l| !l.linked && l.from != from && (l.viewer_gone || l.since.elapsed() > Duration::from_secs(10)))
                    && let Some(old) = self.live.take()
                {
                    self.note(format!("replaced {}", old.from));
                    old.session.end();
                    self.gate.busy = false;
                }
                let key = self.pw_key.clone();
                let salt = self.store.pw_salt.clone();
                let sec = Secrets { id: &self.store.id, code: &self.code, password: key.as_deref().map(|k| (salt.as_str(), k)) };
                let actions = self.gate.on_message(&from, &msg, &sec);
                self.apply(actions, cx);
            }
        }
        self.update_keep();
        cx.repaint();
    }

    fn apply(&mut self, actions: Vec<Action>, cx: &mut Cx) {
        for a in actions {
            match a {
                Action::Send { to, m, close } => {
                    if m["t"] == "refused" {
                        self.note(format!("refused {to} {}", m["why"].as_str().unwrap_or("")));
                    }
                    self.relay.send(&to, m, close);
                }
                Action::Ask { from, name } => {
                    self.note(format!("ask {name}"));
                    self.asks.push(Ask { from, name, since: Instant::now() });
                    sys::show(self.win.hwnd, true);
                    self.win.bring_to_front();
                    cx.timer(TICK, Some(500));
                }
                Action::Start(ready) => self.start_session(ready),
            }
        }
        self.update_keep();
        cx.repaint();
    }

    fn start_session(&mut self, ready: Ready) {
        self.note(format!("start {} {:?}", ready.name, ready.mode));
        let relay = self.relay.clone();
        let r = ready.clone();
        let sender = self.win.sender();
        self.runs += 1;
        let run = self.runs;
        let session = rtc::start(
            ready.sdp.clone(),
            move |res| match res {
                Ok(sdp) => relay.send(&r.from, gate::answer(&r, &sdp), false),
                // the offer could not be read: say so (it is not a busy PC)
                Err(_) => relay.send(&r.from, json!({ "t": "refused", "why": "offer" }), true),
            },
            move |ev| {
                sender.send(move |h, cx| {
                    if let Some(app) = with_app(h) {
                        app.on_session(run, ev, cx);
                    }
                });
            },
        );
        self.live = Some(Live { name: ready.name, mode: ready.mode, since: Instant::now(), linked: false, session, from: ready.from, viewer_gone: false, run });
    }

    pub fn on_session(&mut self, run: u64, ev: SessionEvent, cx: &mut Cx) {
        // a session that was replaced by another one: nothing to do
        if self.live.as_ref().map(|l| l.run) != Some(run) {
            return;
        }
        match ev {
            SessionEvent::Linked => {
                self.note("linked".into());
                if let Some(l) = &mut self.live {
                    l.linked = true;
                    l.since = Instant::now();
                }
                cx.timer(TICK, Some(500));
            }
            SessionEvent::Ended(why) => {
                self.note(format!("ended {why}"));
                if let Some(l) = self.live.take()
                    && l.mode == Mode::Code
                {
                    // a one-time code
                    self.code = crypto::new_code();
                }
                self.gate.busy = false;
                let s = self.s;
                self.say(s.ended.to_string(), cx);
            }
        }
        self.update_keep();
        cx.repaint();
    }

    fn answer_ask(&mut self, allow: bool, cx: &mut Cx) {
        if self.asks.is_empty() {
            return;
        }
        let a = self.asks.remove(0);
        self.note(format!("{} {}", if allow { "allow" } else { "deny" }, a.name));
        let actions = if allow { self.gate.accept(&a.from) } else { self.gate.deny(&a.from) };
        self.apply(actions, cx);
    }

    /// The window is about to go away for good: sessions end, the relay link closes.
    pub fn shutdown(&mut self) {
        if let Some(l) = &self.live {
            l.session.end_and_wait(Duration::from_secs(1));
        }
        self.relay.stop();
        sys::tray_remove(self.win.hwnd);
    }

    // ───────────────────────── actions ─────────────────────────

    fn act(&mut self, id: Id, cx: &mut Cx) {
        let s = self.s;
        match id {
            Id::Min => cx.after(|w| w.minimize()),
            Id::Max => cx.after(|w| w.toggle_maximize()),
            Id::Close => cx.after(|w| w.close()),
            Id::CopyNumber | Id::CopyCode => {
                let text = if id == Id::CopyNumber { self.store.id.clone() } else { self.code.clone() };
                if self.win.copy_text(&text) {
                    self.copied = Some((id, Instant::now()));
                    cx.timer(TICK, Some(500));
                }
            }
            Id::NewCode => {
                self.code = crypto::new_code();
                self.note("new code".into());
            }
            Id::SetPassword => {
                // the field opens at the next drawing, where its place is known
                self.editing = true;
                self.edit_pending = true;
            }
            Id::SavePassword => self.save_password(cx),
            Id::CancelPassword => {
                self.editing = false;
                cx.after(|w| w.edit_hide());
            }
            Id::RemovePassword => {
                self.store.set_password(None);
                self.pw_key = None;
                self.note("password removed".into());
                self.say(s.password_removed.into(), cx);
            }
            Id::Autostart => {
                let on = !self.autostart;
                if sys::set_autostart(on) {
                    self.autostart = on;
                }
            }
            Id::OpenViewer => {
                self.win.open(&format!("{}/", self.site()));
            }
            Id::Allow => self.answer_ask(true, cx),
            Id::Refuse => self.answer_ask(false, cx),
            Id::Disconnect => {
                if let Some(l) = &self.live {
                    l.session.end();
                    self.note("disconnect".into());
                }
            }
        }
        self.update_keep();
        cx.repaint();
    }

    fn save_password(&mut self, cx: &mut Cx) {
        let text = self.win.edit_text().unwrap_or_default();
        let s = self.s;
        if text.chars().count() < 8 {
            self.say(s.too_short.into(), cx);
            return;
        }
        if self.store.set_password(Some(&text)) {
            self.pw_key = self.store.password_key();
            self.note("password set".into());
            self.say(s.password_saved.into(), cx);
        }
        self.editing = false;
        cx.after(|w| {
            w.edit_set_text("");
            w.edit_hide();
        });
    }

    fn field_bg(&self) -> Color {
        self.theme.fill.over(self.theme.card)
    }

    // ───────────────────────── drawing ─────────────────────────

    fn button(&mut self, c: &mut Canvas, r: Rect, label: &str, style: u8, id: Id) {
        // style: 0 secondary, 1 accent, 2 red
        let t = self.theme.clone();
        let hot = self.hover == Some(id);
        let down = hot && self.pressed == Some(id);
        let base = match style {
            1 => t.accent,
            2 => t.red,
            _ => t.fill,
        };
        let fill = match (style, hot, down) {
            (_, _, true) => base.mix(Color::hex(0), 0.15),
            (0, true, _) => t.fill2,
            (_, true, _) => base.mix(Color::hex(0xffffff), 0.1),
            _ => base,
        };
        c.fill_round(r, r.h / 2.0, fill);
        c.text(label, r, Font::BodyBold, if style == 0 { t.t1 } else { Color::hex(0xffffff) }, Align::Center);
        self.hits.add(r, id);
    }

    fn pill(c: &mut Canvas, x: f32, y: f32, text: &str, ink: Color) -> f32 {
        let w = c.text_width(text, Font::SmallBold) + 22.0;
        let r = rect(x, y, w, 22.0);
        c.fill_round(r, 11.0, ink.fade(0.16));
        c.fill_round(rect(x + 9.0, y + 8.0, 6.0, 6.0), 3.0, ink);
        c.text(text, rect(x + 13.0, y, w - 13.0, 22.0), Font::SmallBold, ink, Align::Center);
        w
    }

    fn paint_head(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let t = self.theme.clone();
        c.gradient_v(rect(0.0, 0.0, cx.w, m::TITLE_H), t.hdr1, t.hdr2);
        let wic = self.gfx.wic.clone();
        c.image(1, rect(14.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0), 1.0, || png(&wic, LOGO));
        c.text(self.s.title, rect(40.0, 0.0, 200.0, m::TITLE_H), Font::BodyBold, if cx.active { t.t1 } else { t.t3 }, Align::Left);
        let cap_x = cx.w - 3.0 * m::CAPTION_W;
        let glyphs = [(Id::Min, '\u{E921}'), (Id::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Id::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
            let hot = self.hover == Some(id) || self.pressed == Some(id);
            if hot {
                c.fill(r, if id == Id::Close { t.close_hover } else { t.hover });
            }
            let ink = match (id == Id::Close && hot, cx.active) {
                (true, _) => Color::hex(0xffffff),
                (false, true) => t.t1,
                (false, false) => t.t3,
            };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    /// The QR code, black on white with its quiet zone (cameras need it).
    fn paint_qr(&mut self, c: &mut Canvas, r: Rect) {
        c.fill_round(r, 16.0, Color::hex(0xffffff));
        let Ok(code) = qrcode::QrCode::with_error_correction_level(self.qr_text().as_bytes(), qrcode::EcLevel::M) else { return };
        let n = code.width();
        // whole device pixels per module: no seams between them at 125 % or 150 %
        let k = c.scale();
        let cell = (((r.w - 24.0) * k / n as f32).floor().max(1.0)) / k;
        let size = cell * n as f32;
        let (ox, oy) = (c.snap(r.x + (r.w - size) / 2.0), c.snap(r.y + (r.h - size) / 2.0));
        let ink = Color::hex(0x0b1416);
        for y in 0..n {
            let mut x = 0;
            while x < n {
                if code[(x, y)] == qrcode::Color::Dark {
                    let start = x;
                    while x < n && code[(x, y)] == qrcode::Color::Dark {
                        x += 1;
                    }
                    c.fill(rect(ox + start as f32 * cell, oy + y as f32 * cell, (x - start) as f32 * cell, cell), ink);
                } else {
                    x += 1;
                }
            }
        }
    }

    fn paint_main(&mut self, c: &mut Canvas, x: f32, y: f32, cw: f32) -> f32 {
        let t = self.theme.clone();
        let s = self.s;
        let h = 252.0;
        let card = rect(x, y, cw, h);
        c.fill_round(card, 18.0, t.card);
        c.stroke_round(card, 18.0, t.sep, 1.0);
        let qr_size = 204.0;
        let qr = rect(card.right() - 24.0 - qr_size, y + 24.0, qr_size, qr_size);
        let left = x + 26.0;
        let text_w = qr.x - left - 20.0;
        // state of the link
        let (state, ink) = if self.gate.locked() {
            (s.locked, t.orange)
        } else {
            match self.net {
                Net::Online => (s.ready, t.green),
                Net::Connecting => (s.connecting, t.orange),
                Net::Offline => (s.offline, t.red),
            }
        };
        Self::pill(c, left, y + 24.0, state, ink);
        let copied = |id: Id, me: &Self| me.copied.is_some_and(|(k, at)| k == id && at.elapsed() < Duration::from_secs(2));
        // ID
        c.text(s.your_id, rect(left, y + 62.0, text_w, 18.0), Font::Small, t.t3, Align::Left);
        let id_text = spaced(&self.store.id, 3);
        c.text(&id_text, rect(left, y + 80.0, text_w, 34.0), Font::Hero, t.t1, Align::Left);
        let iw = c.text_width(&id_text, Font::Hero);
        let label = if copied(Id::CopyNumber, self) { s.copied } else { s.copy };
        let bw = c.text_width(label, Font::BodyBold) + 28.0;
        if left + iw + 16.0 + bw < qr.x - 10.0 {
            self.button(c, rect(left + iw + 16.0, y + 84.0, bw, 28.0), label, 0, Id::CopyNumber);
        }
        // code
        c.text(s.code, rect(left, y + 132.0, text_w, 18.0), Font::Small, t.t3, Align::Left);
        let code_text = spaced(&self.code, 4);
        c.text(&code_text, rect(left, y + 150.0, text_w, 34.0), Font::Hero, t.accent, Align::Left);
        let mut bx = left;
        let by = y + 200.0;
        for (label, id) in [(s.new_code, Id::NewCode), (if copied(Id::CopyCode, self) { s.copied } else { s.copy }, Id::CopyCode)] {
            let bw = c.text_width(label, Font::BodyBold) + 28.0;
            self.button(c, rect(bx, by, bw, 30.0), label, 0, id);
            bx += bw + 10.0;
        }
        self.paint_qr(c, qr);
        c.text(s.scan, rect(qr.x - 20.0, qr.bottom() + 2.0, qr.w + 40.0, 18.0), Font::Small, t.t3, Align::Center);
        h
    }

    fn paint_password(&mut self, c: &mut Canvas, x: f32, y: f32, cw: f32) -> f32 {
        let t = self.theme.clone();
        let s = self.s;
        let h = if self.editing { 136.0 } else { 92.0 };
        let card = rect(x, y, cw, h);
        c.fill_round(card, 18.0, t.card);
        c.stroke_round(card, 18.0, t.sep, 1.0);
        let left = x + 26.0;
        let on = self.pw_key.is_some();
        // buttons on the right
        let mut right = card.right() - 22.0;
        if !self.editing {
            let labels: Vec<(&str, u8, Id)> =
                if on { vec![(s.change, 0, Id::SetPassword), (s.remove, 0, Id::RemovePassword)] } else { vec![(s.set_password, 1, Id::SetPassword)] };
            for (label, style, id) in labels.into_iter().rev() {
                let bw = c.text_width(label, Font::BodyBold) + 32.0;
                self.button(c, rect(right - bw, y + 30.0, bw, 32.0), label, style, id);
                right -= bw + 10.0;
            }
        }
        let tw = right - left - 16.0;
        c.text(s.password, rect(left, y + 20.0, tw, 22.0), Font::Title, t.t1, Align::Left);
        c.text(if on { s.password_on } else { s.password_sub }, rect(left, y + 46.0, tw, 20.0), Font::Body, if on { t.green } else { t.t2 }, Align::Left);
        if self.editing {
            let row = y + 82.0;
            let mut r2 = card.right() - 22.0;
            for (label, style, id) in [(s.save, 1, Id::SavePassword), (s.cancel, 0, Id::CancelPassword)] {
                let bw = c.text_width(label, Font::BodyBold) + 32.0;
                self.button(c, rect(r2 - bw, row, bw, 34.0), label, style, id);
                r2 -= bw + 10.0;
            }
            let field = rect(left, row, (r2 - left - 6.0).min(360.0), 34.0);
            c.fill_round(field, 10.0, self.field_bg());
            c.stroke_round(field, 10.0, t.accent, 1.5);
            let inner = field.inset_xy(12.0, 7.0);
            if self.edit_pending {
                self.edit_pending = false;
                let st = EditStyle {
                    fg: t.t1,
                    bg: self.field_bg(),
                    size: 14.0,
                    select: None,
                    placeholder: s.password_hint.to_string(),
                    password: true,
                    multiline: false,
                };
                self.win.edit_show(inner, "", &st);
                self.win.edit_focus();
            } else {
                self.win.edit_place(inner);
            }
        }
        h
    }

    fn paint_more(&mut self, c: &mut Canvas, x: f32, y: f32, cw: f32) -> f32 {
        let t = self.theme.clone();
        let s = self.s;
        let h = 164.0;
        let card = rect(x, y, cw, h);
        c.fill_round(card, 18.0, t.card);
        c.stroke_round(card, 18.0, t.sep, 1.0);
        let left = x + 26.0;
        // start with Windows: a switch
        let sw = rect(card.right() - 22.0 - 44.0, y + 26.0, 44.0, 24.0);
        let on = self.autostart;
        c.fill_round(sw, 12.0, if on { t.accent } else { t.fill2 });
        let knob = if on { sw.right() - 21.0 } else { sw.x + 3.0 };
        c.fill_round(rect(knob, sw.y + 3.0, 18.0, 18.0), 9.0, Color::hex(0xffffff));
        let row = rect(x, y, cw, 76.0);
        self.hits.add(row, Id::Autostart);
        let tw = sw.x - left - 16.0;
        c.text(s.autostart, rect(left, y + 16.0, tw, 22.0), Font::Title, t.t1, Align::Left);
        c.text(s.autostart_sub, rect(left, y + 40.0, tw, 20.0), Font::Body, t.t2, Align::Left);
        c.hline(left, y + 82.0, cw - 52.0, t.sep);
        // another PC
        let label = s.open_viewer;
        let bw = c.text_width(label, Font::BodyBold) + 32.0;
        self.button(c, rect(card.right() - 22.0 - bw, y + 104.0, bw, 32.0), label, 0, Id::OpenViewer);
        let tw = card.right() - 22.0 - bw - left - 16.0;
        c.text(s.other, rect(left, y + 96.0, tw, 22.0), Font::Title, t.t1, Align::Left);
        c.text(s.other_sub, rect(left, y + 120.0, tw, 20.0), Font::Body, t.t2, Align::Left);
        h
    }

    fn paint_live(&mut self, c: &mut Canvas, x: f32, y: f32, cw: f32) -> f32 {
        let Some(l) = &self.live else { return 0.0 };
        let t = self.theme.clone();
        let s = self.s;
        let (text, linked, secs) = (fill(if l.linked { s.controlled } else { s.linking }, &[("name", &l.name)]), l.linked, l.since.elapsed().as_secs());
        let card = rect(x, y, cw, 64.0);
        let ink = if linked { t.red } else { t.orange };
        c.fill_round(card, 16.0, ink.fade(if t.dark { 0.18 } else { 0.1 }));
        c.stroke_round(card, 16.0, ink.fade(0.5), 1.0);
        let bw = c.text_width(s.disconnect, Font::BodyBold) + 36.0;
        self.button(c, rect(card.right() - 18.0 - bw, y + 15.0, bw, 34.0), s.disconnect, 2, Id::Disconnect);
        let tw = card.w - bw - 60.0;
        c.text(&text, rect(x + 22.0, y + 12.0, tw, 22.0), Font::Title, t.t1, Align::Left);
        if linked {
            c.text(&format!("{:02}:{:02}", secs / 60, secs % 60), rect(x + 22.0, y + 34.0, tw, 18.0), Font::Small, t.t2, Align::Left);
        }
        card.h + 16.0
    }

    fn paint_ask(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(a) = self.asks.first() else { return };
        let t = self.theme.clone();
        let s = self.s;
        let (title, left) = (fill(s.asks, &[("name", &a.name)]), ASK_FOR.saturating_sub(a.since.elapsed()).as_secs());
        c.fill(rect(0.0, m::TITLE_H, cx.w, cx.h - m::TITLE_H), Color::hex(0).fade(0.45));
        let w = (cx.w - 80.0).min(480.0);
        let card = rect((cx.w - w) / 2.0, (cx.h - 230.0) / 2.0 + 10.0, w, 230.0);
        c.fill_round(card, 20.0, t.solid2);
        c.stroke_round(card, 20.0, t.ring, 1.0);
        let wic = self.gfx.wic.clone();
        c.image(1, rect(card.x + (w - 52.0) / 2.0, card.y + 22.0, 52.0, 52.0), 1.0, || png(&wic, LOGO));
        c.text(&title, rect(card.x + 24.0, card.y + 86.0, w - 48.0, 24.0), Font::Large, t.t1, Align::Center);
        let sub_h = c.text_height(s.asks_sub, w - 64.0, Font::Body);
        c.text_block(s.asks_sub, rect(card.x + 32.0, card.y + 116.0, w - 64.0, sub_h), Font::Body, t.t2);
        let bw = (w - 48.0 - 12.0) / 2.0;
        let by = card.bottom() - 58.0;
        self.button(c, rect(card.x + 24.0, by, bw, 38.0), &format!("{} ({left})", s.refuse), 0, Id::Refuse);
        self.button(c, rect(card.x + 24.0 + bw + 12.0, by, bw, 38.0), s.allow, 1, Id::Allow);
    }

    fn paint_toast(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some((text, _)) = &self.toast else { return };
        let t = &self.theme;
        let tw = (c.text_width(text, Font::Body) + 40.0).min(cx.w - 40.0);
        let r = rect((cx.w - tw) / 2.0, cx.h - 64.0, tw, 38.0);
        c.fill_round(r, 19.0, t.solid2);
        c.stroke_round(r, 19.0, t.ring, 1.0);
        c.text(text, r.inset_xy(16.0, 0.0), Font::Body, t.t1, Align::Center);
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let t = self.theme.clone();
        self.hits.clear();
        c.clear(t.bg);
        let cw = (cx.w - 56.0).clamp(300.0, 820.0);
        let x = (cx.w - cw) / 2.0;
        let mut y = m::TITLE_H + 24.0;
        c.text(self.s.hero, rect(x, y, cw, 34.0), Font::Hero, t.t1, Align::Left);
        y += 38.0;
        let host = self.site().trim_start_matches("https://").trim_start_matches("http://").to_string();
        c.text(&fill(self.s.sub, &[("site", &host)]), rect(x, y, cw, 20.0), Font::Body, t.t3, Align::Left);
        y += 34.0;
        y += self.paint_live(c, x, y, cw);
        y += self.paint_main(c, x, y, cw) + 16.0;
        y += self.paint_password(c, x, y, cw) + 16.0;
        self.paint_more(c, x, y, cw);
        self.paint_head(c, cx);
        self.paint_toast(c, cx);
        self.paint_ask(c, cx);
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
            Event::MouseDown { p, button: Button::Left, .. } => {
                self.pressed = self.hits.at(p).copied();
                cx.capture(true);
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                let up = self.hits.at(p).copied();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    // while someone asks, only the answer counts
                    if self.asks.is_empty() || matches!(a, Id::Allow | Id::Refuse | Id::Min | Id::Max | Id::Close) {
                        self.act(a, cx);
                    }
                }
                cx.repaint();
            }
            Event::EditKey { vk: 0x0D, .. } => self.save_password(cx),
            Event::EditKey { vk: 0x1B, .. } => self.act(Id::CancelPassword, cx),
            Event::Key { vk: 0x1B, .. } if !self.asks.is_empty() => self.answer_ask(false, cx),
            Event::Timer(TICK) => {
                // expired questions are refused
                let expired: Vec<String> = self.asks.iter().filter(|a| a.since.elapsed() >= ASK_FOR).map(|a| a.from.clone()).collect();
                for from in expired {
                    self.asks.retain(|a| a.from != from);
                    let actions = self.gate.deny(&from);
                    self.apply(actions, cx);
                }
                if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(4)) {
                    self.toast = None;
                }
                let busy = self.toast.is_some()
                    || !self.asks.is_empty()
                    || self.live.is_some()
                    || self.copied.is_some_and(|(_, at)| at.elapsed() < Duration::from_secs(3));
                if !busy {
                    cx.timer(TICK, None);
                }
                self.update_keep();
                cx.repaint();
            }
            Event::Opened(text) if text == "quit" => {
                self.quitting = true;
                self.update_keep();
                cx.after(|w| w.close());
            }
            Event::Opened(_) => {
                sys::show(self.win.hwnd, true);
                self.win.bring_to_front();
                cx.repaint();
            }
            Event::Close => self.shutdown(),
            Event::Resized | Event::Scale | Event::Active(_) => cx.repaint(),
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Id::Max) => Area::MaxButton,
            Some(Id::Min | Id::Close) => Area::Control,
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
