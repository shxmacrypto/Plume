//! Drawing the window from the app's state: the title bar, the home page (QR code, send, devices, history), the
//! settings, the question when a device asks to connect, and the toast.

use super::{App, Id, View};
use crate::{
    hub::DevState,
    i18n::{fill, french, tr},
    settings::{Dir, Entry, Kind, State},
    util::human_size,
};
use plume_ui::{
    Align, Canvas, Color, EditStyle, Font, PaintCx, Rect,
    icons::{self, Icon, Part::*},
    m, rect,
};
use windows::{
    Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
        WICDecodeMetadataCacheOnLoad,
    },
    core::Interface,
};

const LOGO: &[u8] = include_bytes!("../../../../assets/brand/plume-drop-256.png");

const ARROW_UP: Icon = Icon { name: "DROP_ARROW_UP", vb: 16.0, parts: &[P("M8 13V3.5M3.8 7.7 8 3.5l4.2 4.2")] };
const ARROW_DOWN: Icon = Icon { name: "DROP_ARROW_DOWN", vb: 16.0, parts: &[P("M8 3v9.5M3.8 8.3 8 12.5l4.2-4.2")] };
const PHONE: Icon = Icon { name: "DROP_PHONE", vb: 16.0, parts: &[R(4.5, 1.8, 7.0, 12.4, 1.8), P("M7.2 11.8h1.6")] };
const LINK: Icon = Icon {
    name: "DROP_LINK",
    vb: 16.0,
    parts: &[P("M6.8 9.2a2.6 2.6 0 0 0 3.7 0l2-2a2.6 2.6 0 0 0-3.7-3.7l-.6.6"), P("M9.2 6.8a2.6 2.6 0 0 0-3.7 0l-2 2a2.6 2.6 0 0 0 3.7 3.7l.6-.6")],
};
const TEXT: Icon = Icon { name: "DROP_TEXT", vb: 16.0, parts: &[P("M3 4h10M3 8h10M3 12h6")] };

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

/// Local day and time of a moment (seconds since 1970): (year, month, day, hour, minute).
fn local(at: u64) -> (u16, u16, u16, u16, u16) {
    use windows::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
    };
    let ticks = (at + 11_644_473_600) * 10_000_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let (mut utc, mut loc) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        if FileTimeToSystemTime(&ft, &mut utc).is_err() || SystemTimeToTzSpecificLocalTime(None, &utc, &mut loc).is_err() {
            return (0, 0, 0, 0, 0);
        }
    }
    (loc.wYear, loc.wMonth, loc.wDay, loc.wHour, loc.wMinute)
}

/// "14:32" today, "Nov 9" (or "9 nov.") before.
pub fn when(at: u64) -> String {
    let (y, mo, d, h, mi) = local(at);
    let (ty, tmo, td, _, _) = local(crate::util::now());
    if (y, mo, d) == (ty, tmo, td) {
        return format!("{h:02}:{mi:02}");
    }
    const EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const FR: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    let k = (mo.clamp(1, 12) - 1) as usize;
    if french() { format!("{d} {}", FR[k]) } else { format!("{} {d}", EN[k]) }
}

/// Button styles.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Plain,
    Accent,
}

/// A pill button: primary (accent, white text) or secondary (fill, t1 text); lighter on hover, darker when pressed.
fn button(app: &mut App, c: &mut Canvas, r: Rect, label: &str, style: Style, id: Id) {
    let t = app.theme.clone();
    let hot = app.hover == Some(id);
    let down = hot && app.pressed == Some(id);
    let fill = match (style, hot, down) {
        (Style::Accent, _, true) => t.accent.mix(Color::hex(0), 0.18),
        (Style::Accent, true, _) => t.accent.mix(Color::hex(0xffffff), 0.1),
        (Style::Accent, ..) => t.accent,
        (Style::Plain, _, true) => t.fill2.mix(Color::hex(0), 0.12),
        (Style::Plain, true, _) => t.fill2,
        (Style::Plain, ..) => t.fill,
    };
    c.fill_round(r, r.h / 2.0, fill);
    let ink = if style == Style::Plain { t.t1 } else { Color::hex(0xffffff) };
    c.text(label, r, Font::BodyBold, ink, Align::Center);
    app.hits.add(r, id);
}

/// A button placed from its right edge; returns its left edge.
/// `at`: the right edge and the top.
fn button_left_of(app: &mut App, c: &mut Canvas, at: (f32, f32), h: f32, label: &str, style: Style, id: Id) -> f32 {
    let (right, y) = at;
    let w = c.text_width(label, Font::BodyBold) + 32.0;
    button(app, c, rect(right - w, y, w, h), label, style, id);
    right - w
}

fn card(app: &App, c: &mut Canvas, r: Rect) {
    c.fill_round(r, 18.0, app.theme.card);
    c.stroke_round(r, 18.0, app.theme.sep, 1.0);
}

fn pill(c: &mut Canvas, x: f32, y: f32, text: &str, ink: Color) -> f32 {
    let w = c.text_width(text, Font::SmallBold) + 22.0;
    let r = rect(x, y, w, 22.0);
    c.fill_round(r, 11.0, ink.fade(0.16));
    c.fill_round(rect(x + 9.0, y + 8.0, 6.0, 6.0), 3.0, ink);
    c.text(text, rect(x + 13.0, y, w - 13.0, 22.0), Font::SmallBold, ink, Align::Center);
    w
}

/// iOS-style switch, 48 x 28: on = accent.
fn switch(app: &mut App, c: &mut Canvas, r: Rect, on: bool, id: Id) {
    let t = &app.theme;
    c.fill_round(r, r.h / 2.0, if on { t.accent } else { t.fill2 });
    let k = r.h - 4.0;
    let x = if on { r.right() - 2.0 - k } else { r.x + 2.0 };
    c.fill_round(rect(x, r.y + 2.0, k, k), k / 2.0, Color::hex(0xffffff));
    app.hits.add(r, id);
}

/// A list row under the pointer: the hover fill, radius 10.
fn row_hover(app: &mut App, c: &mut Canvas, r: Rect, id: Id) {
    if app.hover == Some(id) {
        c.fill_round(r, 10.0, app.theme.hover);
    }
    app.hits.add(r, id);
}

/// An empty list: centred icon, title, one line and one primary button. `at`: left, top, width.
fn empty_state(app: &mut App, c: &mut Canvas, at: (f32, f32, f32), icon: &Icon, text: (&str, &str), action: (&str, Id)) {
    let (x, y, w) = at;
    let t = app.theme.clone();
    let circle = rect(x + (w - 48.0) / 2.0, y, 48.0, 48.0);
    c.fill_round(circle, 24.0, t.accent.fade(0.16));
    c.icon(icon, circle.centered(20.0, 20.0), t.accent);
    c.text(text.0, rect(x, y + 60.0, w, 24.0), Font::Title, t.t1, Align::Center);
    c.text(text.1, rect(x, y + 84.0, w, 20.0), Font::Body, t.t3, Align::Center);
    let bw = c.text_width(action.0, Font::BodyBold) + 36.0;
    button(app, c, rect(x + (w - bw) / 2.0, y + 116.0, bw, 36.0), action.0, Style::Accent, action.1);
}

/// Height an empty state takes.
const EMPTY_H: f32 = 160.0;

fn paint_head(app: &mut App, c: &mut Canvas, cx: &PaintCx) {
    let t = app.theme.clone();
    c.gradient_v(rect(0.0, 0.0, cx.w, m::TITLE_H), t.hdr1, t.hdr2);
    app.hits.add(rect(0.0, 0.0, cx.w, m::TITLE_H), Id::Title);
    let wic = app.gfx().wic.clone();
    c.image(1, rect(14.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0), 1.0, || png(&wic, LOGO));
    c.text("Plume Drop", rect(40.0, 0.0, 200.0, m::TITLE_H), Font::BodyBold, if cx.active { t.t1 } else { t.t3 }, Align::Left);
    let cap_x = cx.w - 3.0 * m::CAPTION_W;
    // settings, or back from them
    let (id, icon) = if app.view == View::Settings { (Id::Back, &icons::BACK) } else { (Id::Settings, &icons::GEAR) };
    let r = rect(cap_x - 44.0, 6.0, 34.0, m::TITLE_H - 14.0);
    if app.hover == Some(id) {
        c.fill_round(r, 8.0, t.hover);
    }
    c.icon(icon, r.centered(16.0, 16.0), t.t2);
    app.hits.add(r, id);
    let glyphs = [(Id::Min, '\u{E921}'), (Id::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Id::Close, '\u{E8BB}')];
    for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
        let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
        let hot = app.hover == Some(id);
        if hot {
            c.fill(r, if id == Id::Close { t.close_hover } else { t.hover });
        }
        let ink = match (id == Id::Close && hot, cx.active) {
            (true, _) => Color::hex(0xffffff),
            (false, true) => t.t1,
            (false, false) => t.t3,
        };
        c.glyph(glyph, r, ink);
        app.hits.add(r, id);
    }
}

/// The QR code, black on white with its quiet zone.
fn paint_qr(c: &mut Canvas, r: Rect, text: Option<&str>, faded: Color, edge: Color) {
    c.fill_round(r, 16.0, Color::hex(0xffffff));
    // the white square stays visible on a white card (light theme)
    c.stroke_round(r, 16.0, edge, 1.0);
    let Some(text) = text else {
        c.fill_round(r, 16.0, faded);
        return;
    };
    let Ok(code) = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M) else { return };
    let n = code.width();
    // whole device pixels per module: no seams at 125 % or 150 %
    let k = c.scale();
    let cell = (((r.w - 28.0) * k / n as f32).floor().max(1.0)) / k;
    let size = cell * n as f32;
    let (ox, oy) = (c.snap(r.x + (r.w - size) / 2.0), c.snap(r.y + (r.h - size) / 2.0));
    let ink = Color::hex(0x0b160e);
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

/// The card with the QR code. Returns its height.
fn paint_connect(app: &mut App, c: &mut Canvas, x: f32, y: f32, w: f32) -> f32 {
    let t = app.theme.clone();
    let pad = 24.0;
    let inner = w - 2.0 * pad;
    let link = app.link();
    let mut yy = y + 24.0;
    // state first, so it can be measured
    let (state, ink) = match (app.port, app.ip) {
        (None, _) => (tr("Can't start"), t.red),
        (_, None) => (tr("Not on a network"), t.orange),
        _ => (tr("Ready"), t.accent),
    };
    let title_h = 24.0;
    let sub = tr("Open the camera and point it at the code. Same Wi-Fi, nothing to install.");
    let sub_h = c.text_height(sub, inner, Font::Body);
    let qr = inner.min(250.0);
    let note = if app.ip.is_none() {
        tr("Connect this PC to Wi-Fi or Ethernet.")
    } else {
        tr("The first time, Windows Firewall may ask whether Plume Drop can use the network: allow it on private networks.")
    };
    let note_h = c.text_height(note, inner, Font::Small);
    let h = 24.0 + 22.0 + 12.0 + title_h + 4.0 + sub_h + 16.0 + qr + 16.0 + 18.0 + 22.0 + 12.0 + 32.0 + 16.0 + 1.0 + 12.0 + note_h + 24.0;
    card(app, c, rect(x, y, w, h));
    pill(c, x + pad, yy, state, ink);
    yy += 22.0 + 12.0;
    c.text(tr("Scan with your phone"), rect(x + pad, yy, inner, title_h), Font::Large, t.t1, Align::Left);
    yy += title_h + 4.0;
    c.text_block(sub, rect(x + pad, yy, inner, sub_h), Font::Body, t.t2);
    yy += sub_h + 16.0;
    paint_qr(c, rect(x + (w - qr) / 2.0, yy, qr, qr), link.as_deref(), t.fill2, t.sep);
    yy += qr + 16.0;
    c.text(tr("Address"), rect(x + pad, yy, inner, 18.0), Font::Small, t.t3, Align::Left);
    yy += 18.0;
    let shown = match (app.ip, app.port) {
        (Some(ip), Some(port)) => format!("http://{ip}:{port}"),
        _ => "—".to_string(),
    };
    c.text(&shown, rect(x + pad, yy, inner, 22.0), Font::Title, t.t1, Align::Left);
    yy += 22.0 + 12.0;
    let copy = if app.copied_recently() { tr("Copied") } else { tr("Copy link") };
    let bw = c.text_width(copy, Font::BodyBold) + 32.0;
    button(app, c, rect(x + pad, yy, bw, 32.0), copy, Style::Plain, Id::CopyLink);
    let nw = c.text_width(tr("New code"), Font::BodyBold) + 32.0;
    button(app, c, rect(x + pad + bw + 8.0, yy, nw, 32.0), tr("New code"), Style::Plain, Id::NewCode);
    yy += 32.0 + 16.0;
    c.hline(x + pad, yy, inner, t.sep);
    yy += 13.0;
    c.text_block(note, rect(x + pad, yy, inner, note_h), Font::Small, t.t3);
    h
}

/// Drop zone, or the text being written. Returns its height.
fn paint_send(app: &mut App, c: &mut Canvas, x: f32, y: f32, w: f32) -> f32 {
    let t = app.theme.clone();
    let pad = 24.0;
    if app.composing {
        let h = 216.0;
        card(app, c, rect(x, y, w, h));
        c.text(tr("Text or link for your phone"), rect(x + pad, y + 20.0, w - 2.0 * pad, 24.0), Font::Title, t.t1, Align::Left);
        let field = rect(x + pad, y + 52.0, w - 2.0 * pad, 100.0);
        let bg = t.fill.over(t.card);
        c.fill_round(field, 12.0, bg);
        c.stroke_round(field, 12.0, t.accent, 1.5);
        let inner = field.inset_xy(12.0, 9.0);
        if app.compose_pending {
            app.compose_pending = false;
            let st = EditStyle { fg: t.t1, bg, size: 14.0, select: None, placeholder: String::new(), password: false, multiline: true };
            app.win.edit_show(inner, "", &st);
            app.win.edit_focus();
        } else {
            app.win.edit_place(inner);
        }
        let by = y + h - 56.0;
        let left = button_left_of(app, c, (x + w - pad, by), 36.0, tr("Send"), Style::Accent, Id::ComposeSend);
        button_left_of(app, c, (left - 8.0, by), 36.0, tr("Cancel"), Style::Plain, Id::ComposeCancel);
        c.text("Ctrl+Enter", rect(x + pad, by, 160.0, 36.0), Font::Small, t.t3, Align::Left);
        return h;
    }
    let h = 168.0;
    card(app, c, rect(x, y, w, h));
    let circle = rect(x + pad, y + 24.0, 56.0, 56.0);
    c.fill_round(circle, 28.0, t.accent);
    c.icon_stroke(&ARROW_UP, circle.centered(26.0, 26.0), Color::hex(0xffffff), 2.2);
    let tx = circle.right() + 18.0;
    let tw = x + w - pad - tx;
    c.text(tr("Drop files here"), rect(tx, y + 28.0, tw, 26.0), Font::Large, t.t1, Align::Left);
    c.text(tr("Phones that are connected can save them."), rect(tx, y + 56.0, tw, 20.0), Font::Body, t.t3, Align::Left);
    let by = y + h - 60.0;
    let fw = c.text_width(tr("Send files…"), Font::BodyBold) + 36.0;
    button(app, c, rect(x + pad, by, fw, 36.0), tr("Send files…"), Style::Accent, Id::SendFiles);
    let sw = c.text_width(tr("Send text…"), Font::BodyBold) + 36.0;
    button(app, c, rect(x + pad + fw + 8.0, by, sw, 36.0), tr("Send text…"), Style::Plain, Id::SendText);
    h
}

fn paint_devices(app: &mut App, c: &mut Canvas, x: f32, y: f32, w: f32) -> f32 {
    let t = app.theme.clone();
    let pad = 24.0;
    let rows = app.devices.len();
    let row_h = 48.0;
    let h = 60.0 + if rows == 0 { EMPTY_H } else { rows as f32 * row_h } + 16.0;
    card(app, c, rect(x, y, w, h));
    c.text(tr("Devices"), rect(x + pad, y + 20.0, w - 2.0 * pad, 24.0), Font::Title, t.t1, Align::Left);
    let mut yy = y + 60.0;
    if rows == 0 {
        let text = (tr("No device yet"), tr("Scan the code with your phone's camera to connect it."));
        let label = if app.copied_recently() { tr("Copied") } else { tr("Copy link") };
        empty_state(app, c, (x + pad, yy + 4.0, w - 2.0 * pad), &PHONE, text, (label, Id::EmptyCopyLink));
        return h;
    }
    for i in 0..rows {
        let d = app.devices[i].clone();
        row_hover(app, c, rect(x + pad - 8.0, yy, w - 2.0 * pad + 16.0, row_h), Id::DevRow(i));
        let icon_r = rect(x + pad, yy + 8.0, 32.0, 32.0);
        let (status, ink) = match (d.state, d.connected) {
            (DevState::Pending, _) => (tr("Waiting for your OK"), t.orange),
            (_, true) => (tr("Connected"), t.accent),
            _ => (tr("Not connected"), t.t3),
        };
        c.fill_round(icon_r, 16.0, ink.fade(0.16));
        c.icon(&PHONE, icon_r.centered(18.0, 18.0), ink);
        let label = if !d.connected && d.known { tr("Forget") } else { tr("Disconnect") };
        let left = button_left_of(app, c, (x + w - pad, yy + 8.0), 32.0, label, Style::Plain, Id::Disconnect(i));
        let tx = icon_r.right() + 12.0;
        let tw = left - tx - 12.0;
        c.text(&d.name, rect(tx, yy + 5.0, tw, 20.0), Font::BodyBold, t.t1, Align::Left);
        c.text(status, rect(tx, yy + 25.0, tw, 18.0), Font::Small, ink, Align::Left);
        yy += row_h;
    }
    h
}

fn entry_icon(e: &Entry) -> &'static Icon {
    match (e.kind, e.dir) {
        (Kind::Link, _) => &LINK,
        (Kind::Text, _) => &TEXT,
        (Kind::File, Dir::In) => &ARROW_DOWN,
        (Kind::File, Dir::Out) => &ARROW_UP,
    }
}

/// The line under an entry's name, and its color.
fn entry_sub(e: &Entry, t: &plume_ui::Theme) -> (String, Color) {
    let size = human_size(e.size);
    let at = when(e.at);
    match (e.dir, e.state) {
        (_, State::Running) => (format!("{} / {}", human_size(e.done), size), t.t2),
        (Dir::In, State::Failed) => (format!("{} · {}", tr("Failed"), fill(tr("from {}"), &[&e.device])), t.red),
        (Dir::In, _) if e.kind == Kind::File => (format!("{size} · {} · {at}", fill(tr("from {}"), &[&e.device])), t.t3),
        (Dir::In, _) => (format!("{} · {at}", fill(tr("from {}"), &[&e.device])), t.t3),
        (Dir::Out, State::Shared) => {
            let mut s = if e.kind == Kind::File { format!("{} · {size}", tr("Offered")) } else { tr("Offered").to_string() };
            if e.saves > 0 && !e.device.is_empty() {
                s = format!("{s} · {}", fill(tr("saved by {}"), &[&e.device]));
            }
            (s, t.accent)
        }
        (Dir::Out, _) => {
            let mut s = if e.kind == Kind::File { format!("{} · {size} · {at}", tr("Sent")) } else { format!("{} · {at}", tr("Sent")) };
            if e.saves > 0 && !e.device.is_empty() {
                s = format!("{s} · {}", fill(tr("saved by {}"), &[&e.device]));
            }
            (s, t.t3)
        }
    }
}

fn paint_history(app: &mut App, c: &mut Canvas, x: f32, y: f32, w: f32) -> f32 {
    let t = app.theme.clone();
    let pad = 24.0;
    let rows = app.history.len().min(200);
    let row_h = 48.0;
    let h = 60.0 + if rows == 0 { EMPTY_H } else { rows as f32 * row_h } + 16.0;
    card(app, c, rect(x, y, w, h));
    c.text(tr("History"), rect(x + pad, y + 20.0, w - 2.0 * pad, 24.0), Font::Title, t.t1, Align::Left);
    if rows > 0 {
        button_left_of(app, c, (x + w - pad, y + 16.0), 32.0, tr("Clear"), Style::Plain, Id::ClearHistory);
    }
    let mut yy = y + 60.0;
    if rows == 0 {
        let text = (tr("Nothing yet"), tr("Files, text and links you send or receive show up here."));
        empty_state(app, c, (x + pad, yy + 4.0, w - 2.0 * pad), &ARROW_UP, text, (tr("Send files…"), Id::EmptySendFiles));
        return h;
    }
    for i in 0..rows {
        let e = app.history[i].clone();
        row_hover(app, c, rect(x + pad - 8.0, yy, w - 2.0 * pad + 16.0, row_h), Id::Row(e.id));
        let icon_r = rect(x + pad, yy + 8.0, 32.0, 32.0);
        let ink = match (e.state, e.dir) {
            (State::Failed, _) => t.red,
            (_, Dir::In) => t.accent,
            _ => t.t2,
        };
        c.fill_round(icon_r, 16.0, ink.fade(0.16));
        c.icon(entry_icon(&e), icon_r.centered(18.0, 18.0), ink);
        // actions, from the right
        let mut right = x + w - pad;
        let by = yy + 8.0;
        let acts: Vec<(&str, Id)> = match (e.dir, e.kind, e.state) {
            (_, _, State::Running) | (Dir::In, _, State::Failed) => vec![],
            (Dir::In, Kind::File, _) if e.path.is_some() => vec![(tr("Show in folder"), Id::Folder(e.id)), (tr("Open"), Id::Open(e.id))],
            (Dir::In, Kind::Link, _) => vec![(tr("Copy"), Id::Copy(e.id)), (tr("Open"), Id::Open(e.id))],
            (Dir::In, Kind::Text, _) => vec![(tr("Copy"), Id::Copy(e.id))],
            (Dir::Out, _, State::Shared) => vec![(tr("Stop"), Id::Stop(e.id))],
            (Dir::Out, Kind::File, _) if e.path.is_some() => vec![(tr("Show in folder"), Id::Folder(e.id))],
            _ => vec![],
        };
        for (label, id) in acts {
            right = button_left_of(app, c, (right, by), 32.0, label, Style::Plain, id) - 8.0;
        }
        let tx = icon_r.right() + 12.0;
        let tw = right - tx - 4.0;
        let name = if e.kind == Kind::File { e.name.clone() } else { e.name.lines().next().unwrap_or("").chars().take(200).collect() };
        c.text(&name, rect(tx, yy + 5.0, tw, 20.0), Font::BodyBold, t.t1, Align::Left);
        let (sub, sub_ink) = entry_sub(&e, &t);
        if e.state == State::Running {
            let bar = rect(tx, yy + 31.0, (tw * 0.55).min(220.0), 5.0);
            c.fill_round(bar, 2.5, t.fill2);
            let k = if e.size > 0 { (e.done as f32 / e.size as f32).clamp(0.0, 1.0) } else { 0.0 };
            c.fill_round(rect(bar.x, bar.y, (bar.w * k).max(5.0), bar.h), 2.5, t.accent);
            c.text(&sub, rect(bar.right() + 12.0, yy + 24.0, tw - bar.w - 12.0, 18.0), Font::Small, sub_ink, Align::Left);
        } else {
            c.text(&sub, rect(tx, yy + 25.0, tw, 18.0), Font::Small, sub_ink, Align::Left);
        }
        yy += row_h;
    }
    h
}

fn paint_home(app: &mut App, c: &mut Canvas, cx: &PaintCx, top: f32) -> f32 {
    let cw = (cx.w - 48.0).clamp(300.0, 1180.0);
    let x = (cx.w - cw) / 2.0;
    let mut y = top + 24.0;
    // the same gap between cards everywhere
    const GAP: f32 = 14.0;
    if cw >= 820.0 {
        let left_w = 332.0;
        let rx = x + left_w + GAP;
        let rw = cw - left_w - GAP;
        let lh = paint_connect(app, c, x, y, left_w);
        let mut ry = y;
        ry += paint_send(app, c, rx, ry, rw) + GAP;
        ry += paint_devices(app, c, rx, ry, rw) + GAP;
        ry += paint_history(app, c, rx, ry, rw);
        return (y + lh).max(ry) + 24.0;
    }
    y += paint_connect(app, c, x, y, cw) + GAP;
    y += paint_send(app, c, x, y, cw) + GAP;
    y += paint_devices(app, c, x, y, cw) + GAP;
    y += paint_history(app, c, x, y, cw);
    y + 24.0
}

/// One settings row: title and text on the left; the caller draws the control. Returns the height.
/// `at`: left, top and width.
fn setting_row(app: &App, c: &mut Canvas, at: (f32, f32, f32), title: &str, sub: &str, right_w: f32) -> f32 {
    let (x, y, w) = at;
    let t = &app.theme;
    let tw = w - 48.0 - right_w - 16.0;
    // a row with no text under its title is just its title, centered
    let sub_h = if sub.is_empty() { -4.0 } else { c.text_height(sub, tw, Font::Body) };
    let h = (20.0 + 22.0 + 4.0 + sub_h + 20.0).max(64.0);
    card(app, c, rect(x, y, w, h));
    let title_y = if sub.is_empty() { y + (h - 22.0) / 2.0 } else { y + 20.0 };
    c.text(title, rect(x + 24.0, title_y, tw, 22.0), Font::Title, t.t1, Align::Left);
    c.text_block(sub, rect(x + 24.0, y + 46.0, tw, sub_h), Font::Body, t.t3);
    h
}

fn paint_settings(app: &mut App, c: &mut Canvas, cx: &PaintCx, top: f32) -> f32 {
    let t = app.theme.clone();
    let cw = (cx.w - 48.0).clamp(300.0, 760.0);
    let x = (cx.w - cw) / 2.0;
    let mut y = top + 24.0;
    c.text(tr("Settings"), rect(x, y, cw, 34.0), Font::Hero, t.t1, Align::Left);
    y += 40.0;
    c.text(tr("Where files go, what happens to text and links, and the language."), rect(x, y, cw, 20.0), Font::Body, t.t3, Align::Left);
    y += 36.0;
    // received files
    let folder = app.settings.folder.to_string_lossy().to_string();
    let bw1 = c.text_width(tr("Change…"), Font::BodyBold) + 32.0;
    let bw2 = c.text_width(tr("Open folder"), Font::BodyBold) + 32.0;
    let h = setting_row(app, c, (x, y, cw), tr("Received files"), &folder, bw1 + bw2 + 10.0);
    let by = y + (h - 32.0) / 2.0;
    let left = button_left_of(app, c, (x + cw - 24.0, by), 32.0, tr("Change…"), Style::Plain, Id::ChangeFolder);
    button_left_of(app, c, (left - 8.0, by), 32.0, tr("Open folder"), Style::Plain, Id::OpenFolder);
    y += h + 14.0;
    // received text and links
    let h = setting_row(app, c, (x, y, cw), tr("Received text"), tr("Copy it to the clipboard"), 52.0);
    let on = app.settings.copy_text;
    switch(app, c, rect(x + cw - 24.0 - 48.0, y + (h - 28.0) / 2.0, 48.0, 28.0), on, Id::CopyTextSwitch);
    y += h + 14.0;
    let h = setting_row(app, c, (x, y, cw), tr("Received links"), tr("Open them in the browser"), 52.0);
    let on = app.settings.open_links;
    switch(app, c, rect(x + cw - 24.0 - 48.0, y + (h - 28.0) / 2.0, 48.0, 28.0), on, Id::OpenLinksSwitch);
    y += h + 14.0;
    // language
    let langs = [(tr("System"), "system"), ("English", "en"), ("Français", "fr")];
    let widths: Vec<f32> = langs.iter().map(|(l, _)| c.text_width(l, Font::BodyBold) + 28.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 8.0;
    let h = setting_row(app, c, (x, y, cw), tr("Language"), "", total);
    // segmented control: a pill track, the chosen segment raised
    let seg = rect(x + cw - 24.0 - total, y + (h - 36.0) / 2.0, total, 36.0);
    c.fill_round(seg, 18.0, t.fill);
    let mut sx = seg.x + 4.0;
    for (k, ((label, code), bw)) in langs.iter().zip(&widths).enumerate() {
        let r = rect(sx, seg.y + 4.0, *bw, 28.0);
        let on = app.settings.lang == *code;
        if on {
            c.fill_round(r, 14.0, t.fill2);
            c.stroke_round(r, 14.0, t.ring, 1.0);
        } else if app.hover == Some(Id::Lang(k as u8)) {
            c.fill_round(r, 14.0, t.hover);
        }
        c.text(label, r, Font::BodyBold, if on { t.t1 } else { t.t2 }, Align::Center);
        app.hits.add(r, Id::Lang(k as u8));
        sx += bw;
    }
    y += h + 14.0;
    let h = setting_row(
        app,
        c,
        (x, y, cw),
        tr("Security"),
        tr("Phones need the code in the QR code, and your OK the first time. New code makes every phone scan again."),
        0.0,
    );
    y += h + 14.0;
    let about = format!("Plume Drop {}", plume_update::VERSION);
    let h = setting_row(app, c, (x, y, cw), &about, tr("Phones talk straight to this PC on your Wi-Fi. Nothing goes through the internet, no account."), 0.0);
    y += h;
    y + 24.0
}

fn paint_ask(app: &mut App, c: &mut Canvas, cx: &PaintCx) {
    let Some(a) = app.asks.first() else { return };
    let t = app.theme.clone();
    let title = fill(tr("{} wants to connect"), &[&a.name]);
    let sub = tr("Allow it to send files to this PC and to save what you share. It is asked only once per device.");
    c.fill(rect(0.0, m::TITLE_H, cx.w, cx.h - m::TITLE_H), Color::hex(0).fade(0.45));
    let w = (cx.w - 80.0).min(480.0);
    let sub_h = c.text_height(sub, w - 64.0, Font::Body);
    let h = 24.0 + 52.0 + 12.0 + 26.0 + 12.0 + sub_h + 24.0 + 36.0 + 24.0;
    let card_r = rect((cx.w - w) / 2.0, (cx.h - h) / 2.0 + 10.0, w, h);
    c.fill_round(card_r, 20.0, t.solid2);
    c.stroke_round(card_r, 20.0, t.ring, 1.0);
    let icon_r = rect(card_r.x + (w - 52.0) / 2.0, card_r.y + 24.0, 52.0, 52.0);
    c.fill_round(icon_r, 26.0, t.accent.fade(0.16));
    c.icon(&PHONE, icon_r.centered(24.0, 24.0), t.accent);
    c.text(&title, rect(card_r.x + 24.0, card_r.y + 88.0, w - 48.0, 26.0), Font::Large, t.t1, Align::Center);
    c.text_block(sub, rect(card_r.x + 32.0, card_r.y + 126.0, w - 64.0, sub_h), Font::Body, t.t2);
    let bw = (w - 48.0 - 12.0) / 2.0;
    let by = card_r.bottom() - 60.0;
    button(app, c, rect(card_r.x + 24.0, by, bw, 36.0), tr("Refuse"), Style::Plain, Id::Refuse);
    button(app, c, rect(card_r.x + 24.0 + bw + 12.0, by, bw, 36.0), tr("Allow"), Style::Accent, Id::Allow);
}

fn paint_toast(app: &App, c: &mut Canvas, cx: &PaintCx) {
    let Some((text, _)) = &app.toast else { return };
    let t = &app.theme;
    let tw = (c.text_width(text, Font::Body) + 40.0).min(cx.w - 40.0);
    let r = rect((cx.w - tw) / 2.0, cx.h - 64.0, tw, 38.0);
    c.fill_round(r, 19.0, t.solid2);
    c.stroke_round(r, 19.0, t.ring, 1.0);
    c.text(text, r.inset_xy(16.0, 0.0), Font::Body, t.t1, Align::Center);
}

pub fn paint(app: &mut App, c: &mut Canvas, cx: &mut PaintCx) {
    let t = app.theme.clone();
    app.hits.clear();
    c.clear(t.bg);
    let view_h = cx.h - m::TITLE_H;
    app.scroll = app.scroll.clamp(0.0, app.max_scroll(view_h));
    let top = m::TITLE_H - app.scroll.round();
    c.clip(rect(0.0, m::TITLE_H, cx.w, view_h));
    let bottom = match app.view {
        View::Home => paint_home(app, c, cx, top),
        View::Settings => paint_settings(app, c, cx, top),
    };
    c.unclip();
    app.content_h = bottom - top;
    if app.view != View::Home && app.composing {
        app.win.edit_hide();
    }
    // a thin scroll bar when the page is taller than the window
    let max = app.max_scroll(view_h);
    if max > 0.0 {
        let k = view_h / app.content_h;
        let bar_h = (view_h * k).max(36.0);
        let bar_y = m::TITLE_H + (view_h - bar_h) * (app.scroll / max);
        c.fill_round(rect(cx.w - 7.0, bar_y + 2.0, 4.0, bar_h - 4.0), 2.0, t.fill2);
    }
    paint_head(app, c, cx);
    paint_toast(app, c, cx);
    paint_ask(app, c, cx);
}
