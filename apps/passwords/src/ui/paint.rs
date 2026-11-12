//! Painting: the title bar, the lock screen, the sidebar, the list, the details, the form, dialogs and messages.

use super::{
    App, F, Hit, LIST_W, Modal, ROW_H, SIDEBAR_W, Screen, View,
    draw::{self, ic},
    form_fields, kind_name, strength_name,
};
use crate::{
    generator,
    i18n::{fill, tr},
    totp::{self, Totp},
    vault::{Entry, Kind},
};
use plume_ui::{Align, Canvas, Color, EditStyle, Font, PaintCx, Rect, icons, icons::Icon, m, rect};
use windows::{
    Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
        WICDecodeMetadataCacheOnLoad,
    },
    core::Interface,
};

const LOGO: &[u8] = include_bytes!("../../../../assets/brand/plume-passwords-256.png");
/// A row of the details: label, value, monospaced, buttons.
type DetailRow = (String, String, bool, Vec<(&'static Icon, Hit)>);
const DOTS: &str = "••••••••••";

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

/// yyyy-mm-dd of a Unix time (UTC is close enough for "edited on").
pub fn date(secs: u64) -> String {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mth <= 2);
    format!("{y}-{mth:02}-{d:02}")
}

/// "4111111111111111" → "4111 1111 1111 1111".
pub fn card_groups(n: &str) -> String {
    let digits: String = n.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    digits.chars().collect::<Vec<_>>().chunks(4).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join(" ")
}

pub fn card_masked(n: &str) -> String {
    let digits: String = n.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 { DOTS.to_string() } else { format!("•••• •••• •••• {}", &digits[digits.len() - 4..]) }
}

impl App {
    /// Opaque (the native text field paints over it): the theme's field fill over a card.
    pub fn field_bg(&self) -> Color {
        self.t.fill.over(self.t.card.over(self.t.bg))
    }

    /// Text and icons on the accent: near-black on a light accent (slate in dark mode), white otherwise.
    pub fn ink_on_accent(&self) -> Color {
        let a = self.t.accent;
        if a.r * 0.299 + a.g * 0.587 + a.b * 0.114 > 0.55 { self.t.bg.mix(Color::hex(0), 0.6) } else { Color::hex(0xffffff) }
    }

    /// Width of a segmented control.
    pub fn segmented_w(&mut self, c: &mut Canvas, labels: &[&str], font: Font) -> f32 {
        labels.iter().map(|l| c.text_width(l, bold(font)) + 24.0).sum::<f32>() + 6.0
    }

    /// A segmented control: a pill track, the chosen segment raised. Returns its width.
    #[allow(clippy::too_many_arguments)]
    pub fn segmented(&mut self, c: &mut Canvas, x: f32, y: f32, labels: &[&str], cur: usize, font: Font, hit: impl Fn(usize) -> Hit) -> f32 {
        let t = self.t.clone();
        let total = self.segmented_w(c, labels, font);
        let track = rect(x, y, total, 32.0);
        c.fill_round(track, 16.0, t.fill);
        let mut sx = x + 3.0;
        for (i, l) in labels.iter().enumerate() {
            let sw = c.text_width(l, bold(font)) + 24.0;
            let b = rect(sx, y + 3.0, sw, 26.0);
            let h = hit(i);
            if i == cur {
                c.fill_round(b, 13.0, if t.dark { t.fill2 } else { t.solid });
                c.stroke_round(b, 13.0, t.ring, 1.0);
            } else if self.hovered(&h) {
                c.fill_round(b, 13.0, t.hover);
            }
            c.text(l, b, if i == cur { bold(font) } else { font }, if i == cur { t.t1 } else { t.t2 }, Align::Center);
            self.hits.add(b, h);
            sx += sw;
        }
        total
    }

    /// A round button: primary (accent), quiet, or red.
    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, style: u8, hit: Hit) {
        let t = self.t.clone();
        let hot = self.hovered(&hit);
        let down = hot && self.pressed.as_ref() == Some(&hit);
        let base = match style {
            1 => t.accent,
            2 => t.red,
            _ => t.fill,
        };
        let fill = match (style, hot, down) {
            (_, _, true) if style == 0 => t.fill2.mix(t.t1, 0.06),
            (_, _, true) => base.mix(Color::hex(0), 0.15),
            (0, true, _) => t.fill2,
            (_, true, _) => base.mix(Color::hex(0xffffff), 0.12),
            _ => base,
        };
        c.fill_round(r, r.h / 2.0, fill);
        c.text(label, r, Font::BodyBold, self.pill_ink(style), Align::Center);
        self.hits.add(r, hit);
    }

    pub fn pill_ink(&self, style: u8) -> Color {
        match style {
            0 => self.t.t1,
            2 => Color::hex(0xffffff),
            _ => self.ink_on_accent(),
        }
    }

    /// A pill with an icon before its label; returns its width.
    #[allow(clippy::too_many_arguments)]
    pub fn icon_pill(&mut self, c: &mut Canvas, x: f32, y: f32, h: f32, icon: &Icon, label: &str, style: u8, hit: Hit) -> f32 {
        let w = c.text_width(label, Font::BodyBold) + 52.0;
        let r = rect(x, y, w, h);
        self.pill(c, r, "", style, hit);
        let ink = self.pill_ink(style);
        c.icon(icon, rect(x + 16.0, y + (h - 16.0) / 2.0, 16.0, 16.0), ink);
        c.text(label, rect(x + 38.0, y, w - 44.0, h), Font::BodyBold, ink, Align::Left);
        w
    }

    pub fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, color: Color) {
        let hot = self.hovered(&hit);
        if hot {
            c.fill_round(r, r.h / 2.0, self.t.hover.mix(self.t.t1, 0.06));
        }
        let s = 16.0;
        c.icon(icon, rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s), if hot { self.t.t1 } else { color });
        self.hits.add(r, hit);
    }

    /// A text field: its frame, its text (or dots, or the hint) when the native field is not over it. `right` keeps
    /// room on the right for buttons inside the frame. Returns the frame.
    pub fn field(&mut self, c: &mut Canvas, r: Rect, f: F, hint: &str, right: f32) -> Rect {
        let t = self.t.clone();
        let focused = self.edit_for == Some(f);
        c.fill_round(r, 10.0, self.field_bg());
        c.stroke_round(r, 10.0, if focused { t.accent } else { t.sep }, if focused { 1.5 } else { 1.0 });
        let lead = if f == F::Search { 34.0 } else { 12.0 };
        if f == F::Search {
            c.icon(&icons::SEARCH, rect(r.x + 12.0, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), t.t3);
        }
        let inner = if f == F::Notes {
            rect(r.x + 12.0, r.y + 9.0, r.w - 24.0 - right, r.h - 18.0)
        } else {
            rect(r.x + lead, r.y + (r.h - 22.0) / 2.0, r.w - lead - 10.0 - right, 22.0)
        };
        self.hits.add(r, Hit::Field(f));
        self.fields.push((f, inner));
        if self.edit_shown != Some(f) || !focused {
            let v = self.value(f);
            if v.is_empty() {
                c.text(hint, inner, Font::Body, t.t4, Align::Left);
            } else if self.masked(f) {
                c.text(DOTS, inner, Font::Body, t.t1, Align::Left);
            } else if f == F::Notes {
                c.clip(inner);
                c.text_block(&v, inner, Font::Body, t.t1);
                c.unclip();
            } else {
                c.text(&v, inner, Font::Body, t.t1, Align::Left);
            }
        }
        r
    }

    /// Four bars and a word.
    pub fn strength_bar(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, pw: &str) {
        if pw.is_empty() {
            return;
        }
        let t = self.t.clone();
        let (s, _) = generator::strength(pw);
        let col = match s.0 {
            0 => t.red,
            1 => t.orange,
            2 => Color::hex(0xeab308),
            _ => t.green,
        };
        let label = strength_name(s);
        let lw = c.text_width(label, Font::SmallBold) + 10.0;
        let bw = ((w - lw - 12.0) / 4.0).max(10.0);
        for i in 0..4u8 {
            let on = i < s.0.max(1) || (s.0 == 4 && i < 4);
            c.fill_round(rect(x + i as f32 * (bw + 4.0), y + 6.0, bw, 5.0), 2.5, if on { col } else { t.fill });
        }
        c.text(label, rect(x + 4.0 * (bw + 4.0) + 6.0, y, lw, 17.0), Font::SmallBold, col, Align::Left);
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let t = self.t.clone();
        self.paints += 1;
        self.hits.clear();
        self.fields.clear();
        c.clear(t.bg);
        let (w, h) = (cx.w, cx.h);
        match self.screen {
            Screen::Main => self.paint_main(c, w, h),
            _ => self.paint_lock(c, w, h),
        }
        self.paint_caption(c, w, cx);
        self.paint_menu(c);
        self.paint_modal(c, w, h);
        self.paint_toast(c, w, h);
        self.place_edit();
    }

    /// The single native text field of the window goes over the field being edited.
    fn place_edit(&mut self) {
        let want = self.edit_for.and_then(|f| self.fields.iter().find(|(g, _)| *g == f).map(|(_, r)| (f, *r)));
        match want {
            Some((f, r)) => {
                if self.edit_shown != Some(f) {
                    let style = EditStyle {
                        fg: self.t.t1,
                        bg: self.field_bg(),
                        size: 13.5,
                        select: None,
                        placeholder: String::new(),
                        password: self.masked(f),
                        multiline: f == F::Notes,
                    };
                    let v = Zeroizing(self.value(f));
                    self.win.edit_show(r, &v.0, &style);
                    self.win.edit_focus();
                    self.edit_shown = Some(f);
                } else {
                    self.win.edit_place(r);
                }
            }
            None => {
                if self.edit_shown.is_some() {
                    self.win.edit_set_text("");
                    self.win.edit_hide();
                    self.edit_shown = None;
                }
            }
        }
    }

    fn paint_caption(&mut self, c: &mut Canvas, w: f32, cx: &PaintCx) {
        let t = self.t.clone();
        c.gradient_v(rect(0.0, 0.0, w, m::TITLE_H), t.hdr1, t.hdr2);
        let wic = self.gfx.wic.clone();
        c.image(1, rect(14.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0), 1.0, || png(&wic, LOGO));
        c.text("Plume Passwords", rect(40.0, 0.0, 200.0, m::TITLE_H), Font::BodyBold, if cx.active { t.t1 } else { t.t3 }, Align::Left);
        let cap_x = w - 3.0 * m::CAPTION_W;
        if self.screen == Screen::Main {
            let label = tr("Lock");
            let bw = c.text_width(label, Font::BodyBold) + 46.0;
            let r = rect(cap_x - bw - 10.0, 5.0, bw, 32.0);
            self.pill(c, r, "", 0, Hit::Lock);
            c.icon(&ic::LOCK, rect(r.x + 13.0, r.y + 8.0, 16.0, 16.0), t.t1);
            c.text(label, rect(r.x + 34.0, r.y, bw - 38.0, r.h), Font::BodyBold, t.t1, Align::Left);
        }
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
            let hot = self.hovered(&id);
            if hot {
                c.fill(r, if id == Hit::Close { t.close_hover } else { t.hover });
            }
            let ink = match (id == Hit::Close && hot, cx.active) {
                (true, _) => Color::hex(0xffffff),
                (false, true) => t.t1,
                (false, false) => t.t3,
            };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    // ───────────────────────── lock screen ─────────────────────────

    fn paint_lock(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let t = self.t.clone();
        c.gradient_v(rect(0.0, 0.0, w, h), t.bg, t.bg2);
        let create = self.screen == Screen::Create;
        let cw = 420.0f32.min(w - 40.0);
        let sub =
            if create { tr("Choose a master password. It opens your vault and cannot be recovered: keep it safe.") } else { tr("Enter your master password.") };
        let sub_h = c.text_height(sub, cw - 64.0, Font::Body);
        let ch = 268.0
            + sub_h
            + if create { 76.0 } else { 0.0 }
            + if self.hello_on && !create { 50.0 } else { 0.0 }
            + if self.error.is_empty() { 0.0 } else { 26.0 };
        let card = rect((w - cw) / 2.0, (m::TITLE_H + (h - m::TITLE_H - ch) / 2.0).max(m::TITLE_H + 12.0), cw, ch);
        c.fill_round(card, 18.0, t.card);
        c.stroke_round(card, 18.0, t.sep, 1.0);
        let x = card.x + 32.0;
        let fw = cw - 64.0;
        let mut y = card.y + 28.0;
        let wic = self.gfx.wic.clone();
        c.image(2, rect(card.x + (cw - 64.0) / 2.0, y, 64.0, 64.0), 1.0, || png(&wic, LOGO));
        y += 76.0;
        c.text(if create { tr("Create your vault") } else { tr("Unlock your vault") }, rect(x, y, fw, 30.0), Font::Hero, t.t1, Align::Center);
        y += 38.0;
        c.text_block(sub, rect(x, y, fw, sub_h), Font::Body, t.t2);
        y += sub_h + 16.0;
        let r = self.field(c, rect(x, y, fw, 40.0), F::Master, tr("Master password"), 34.0);
        self.icon_btn(c, if self.reveal_master { &ic::EYE_OFF } else { &ic::EYE }, rect(r.right() - 38.0, r.y + 5.0, 30.0, 30.0), Hit::Reveal(F::Master), t.t3);
        y += 48.0;
        if create {
            let pw = self.pw1.clone();
            self.strength_bar(c, x, y, fw, &pw);
            y += 24.0;
            self.field(c, rect(x, y, fw, 40.0), F::Master2, tr("Type it again"), 0.0);
            y += 52.0;
        }
        if !self.error.is_empty() {
            c.text(&self.error.clone(), rect(x, y - 4.0, fw, 22.0), Font::Body, t.red, Align::Center);
            y += 26.0;
        }
        let label = match self.busy {
            Some(b) => b,
            None if create => tr("Create vault"),
            None => tr("Unlock"),
        };
        self.pill(c, rect(x, y + 2.0, fw, 36.0), label, if self.busy.is_some() { 0 } else { 1 }, Hit::Submit);
        y += 50.0;
        if self.hello_on && !create {
            let label = tr("Unlock with Windows Hello");
            let bw = c.text_width(label, Font::BodyBold) + 52.0;
            self.icon_pill(c, x + (fw - bw) / 2.0, y, 36.0, &ic::FINGER, label, 0, Hit::HelloUnlock);
        }
        c.text(tr("Your vault is encrypted and never leaves this PC."), rect(x, card.bottom() + 14.0, fw, 20.0), Font::Small, t.t3, Align::Center);
    }

    // ───────────────────────── the vault ─────────────────────────

    fn paint_main(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let t = self.t.clone();
        self.paint_sidebar(c, rect(0.0, m::TITLE_H, SIDEBAR_W, h - m::TITLE_H));
        let rest = rect(SIDEBAR_W, m::TITLE_H, w - SIDEBAR_W, h - m::TITLE_H);
        match self.view.clone() {
            View::Generator => self.paint_generator(c, rest),
            View::Report => self.paint_report(c, rest),
            View::Settings => self.paint_settings(c, rest),
            _ => {
                let list = rect(rest.x, rest.y, LIST_W, rest.h);
                let detail = rect(rest.x + LIST_W, rest.y, rest.w - LIST_W, rest.h);
                self.paint_detail_pane(c, detail);
                self.paint_list(c, list);
                c.vline(list.right(), list.y, list.h, t.sep);
            }
        }
    }

    fn nav_row(&mut self, c: &mut Canvas, r: Rect, icon: &Icon, label: &str, count: Option<usize>, v: View) {
        let t = self.t.clone();
        let hit = Hit::Nav(v.clone());
        let on = self.view == v && self.pick.is_none();
        if on {
            c.fill_round(r, 10.0, t.accent.fade(0.16));
        } else if self.hovered(&hit) {
            c.fill_round(r, 10.0, t.hover);
        }
        c.icon(icon, rect(r.x + 12.0, r.y + (r.h - 18.0) / 2.0, 18.0, 18.0), if on { t.accent } else { t.t2 });
        let lw = r.w - 52.0 - if count.is_some_and(|n| n > 0) { 36.0 } else { 0.0 };
        c.text(label, rect(r.x + 40.0, r.y, lw, r.h), if on { Font::BodyBold } else { Font::Body }, t.t1, Align::Left);
        if let Some(n) = count.filter(|n| *n > 0) {
            c.text(&n.to_string(), rect(r.right() - 44.0, r.y, 34.0, r.h), Font::Small, t.t3, Align::Right);
        }
        self.hits.add(r, hit);
    }

    fn paint_sidebar(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        c.fill(r, t.bg2);
        c.vline(r.right() - 0.5, r.y, r.h, t.sep);
        let x = r.x + 12.0;
        let rw = r.w - 24.0;
        let mut y = r.y + 12.0;
        let rows: [(&Icon, &str, View); 6] = [
            (&ic::GRID, tr("All items"), View::All),
            (&icons::STAR, tr("Favorites"), View::Favorites),
            (&ic::KEY, tr("Logins"), View::Kind(Kind::Login)),
            (&ic::CLOCK, tr("2FA codes"), View::Kind(Kind::Totp)),
            (&ic::NOTE, tr("Secure notes"), View::Kind(Kind::Note)),
            (&ic::CARD, tr("Cards"), View::Kind(Kind::Card)),
        ];
        for (icon, label, v) in rows {
            let n = self.count(&v);
            self.nav_row(c, rect(x, y, rw, 36.0), icon, label, Some(n), v);
            y += 38.0;
        }
        y += 14.0;
        c.text(tr("FOLDERS"), rect(x + 12.0, y, rw - 44.0, 20.0), Font::SmallBold, t.t3, Align::Left);
        self.icon_btn(c, &icons::PLUS, rect(x + rw - 32.0, y - 6.0, 32.0, 32.0), Hit::AddFolder, t.t2);
        y += 26.0;
        let bottom = r.bottom() - 3.0 * 38.0 - 16.0;
        let folders = self.folders();
        if folders.is_empty() {
            c.text(tr("No folders yet"), rect(x + 12.0, y, rw - 12.0, 26.0), Font::Small, t.t3, Align::Left);
        }
        for f in folders {
            if y + 36.0 > bottom {
                break;
            }
            let n = self.count(&View::Folder(f.clone()));
            self.nav_row(c, rect(x, y, rw, 36.0), &icons::FOLDER, &f, Some(n), View::Folder(f.clone()));
            y += 38.0;
        }
        let mut y = r.bottom() - 3.0 * 38.0 - 10.0;
        c.hline(x, y - 7.0, rw, t.sep);
        for (icon, label, v) in
            [(&ic::DICE, tr("Generator"), View::Generator), (&ic::SHIELD, tr("Security report"), View::Report), (&icons::GEAR, tr("Settings"), View::Settings)]
        {
            self.nav_row(c, rect(x, y, rw, 36.0), icon, label, None, v);
            y += 38.0;
        }
    }

    fn badge(&mut self, c: &mut Canvas, r: Rect, e: &Entry) {
        let col = draw::badge_color(&e.title);
        c.fill_round(r, r.w / 2.0, col);
        let icon = match e.kind {
            Kind::Totp => Some(&ic::CLOCK),
            Kind::Note => Some(&ic::NOTE),
            Kind::Card => Some(&ic::CARD),
            Kind::Login => None,
        };
        match icon {
            Some(i) => {
                let s = (r.w * 0.46).round();
                c.icon(i, rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s), Color::hex(0xffffff));
            }
            None => {
                let letter: String = e.title.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into());
                draw::text(c, &letter, r, r.w * 0.44, 700, Color::hex(0xffffff), Align::Center, false);
            }
        }
    }

    fn paint_list(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        let x = r.x + 16.0;
        let mut y = r.y + 16.0;
        self.field(c, rect(x, y, r.w - 32.0 - 44.0, 36.0), F::Search, tr("Search"), 0.0);
        let nb = rect(r.right() - 16.0 - 36.0, y, 36.0, 36.0);
        self.pill(c, nb, "", 1, Hit::New);
        c.icon(&icons::PLUS, rect(nb.x + 10.0, nb.y + 10.0, 16.0, 16.0), self.ink_on_accent());
        self.new_btn = nb;
        y += 50.0;
        if let Some((title, _)) = self.pick.clone() {
            let card = rect(x, y, r.w - 32.0, 74.0);
            c.fill_round(card, 14.0, t.accent.fade(0.16));
            c.icon(&ic::KEYBOARD, rect(card.x + 12.0, card.y + 12.0, 16.0, 16.0), t.accent);
            c.text(tr("Choose the login to type into:"), rect(card.x + 36.0, card.y + 6.0, card.w - 46.0, 26.0), Font::SmallBold, t.t1, Align::Left);
            c.text(&title, rect(card.x + 36.0, card.y + 28.0, card.w - 46.0, 18.0), Font::Small, t.t2, Align::Left);
            let lw = c.text_width(tr("Cancel"), Font::SmallBold) + 20.0;
            let cr = rect(card.right() - lw - 8.0, card.y + 48.0, lw, 22.0);
            c.text(tr("Cancel"), cr, Font::SmallBold, t.t1, Align::Center);
            self.hits.add(cr, Hit::CancelPick);
            y += 84.0;
        }
        let area = rect(r.x, y, r.w, r.bottom() - y);
        let vis = self.visible();
        let content = vis.len() as f32 * ROW_H + 10.0;
        self.list_scroll = self.list_scroll.min((content - area.h).max(0.0));
        if vis.is_empty() {
            // an empty vault: the details side says what to do
            if !self.vault.data.entries.is_empty() || !self.query.is_empty() {
                let msg = if self.query.is_empty() { tr("Nothing here yet.") } else { tr("Nothing matches your search.") };
                c.text(msg, rect(r.x + 16.0, y + 24.0, r.w - 32.0, 22.0), Font::Body, t.t3, Align::Center);
            }
            return;
        }
        c.clip(area);
        let now = crate::vault::now();
        let first = (self.list_scroll / ROW_H) as usize;
        for (k, &i) in vis.iter().enumerate().skip(first) {
            let ry = area.y + k as f32 * ROW_H - self.list_scroll;
            if ry > area.bottom() {
                break;
            }
            let e = self.vault.data.entries[i].clone();
            let row = rect(r.x + 8.0, ry, r.w - 16.0, ROW_H - 4.0);
            let hit = Hit::Row(e.id.clone());
            if self.sel.as_deref() == Some(&e.id) {
                c.fill_round(row, 10.0, t.accent.fade(0.16));
            } else if self.hovered(&hit) {
                c.fill_round(row, 10.0, t.hover);
            }
            self.badge(c, rect(row.x + 10.0, row.y + (row.h - 32.0) / 2.0, 32.0, 32.0), &e);
            let tx = row.x + 54.0;
            let star = if e.favorite { 22.0 } else { 0.0 };
            let code = if e.totp.is_empty() { None } else { Totp::parse(&e.totp) };
            let code_w = if code.is_some() { 74.0 } else { 0.0 };
            let tw = row.right() - tx - 10.0 - star - code_w;
            c.text(&e.title, rect(tx, row.y + 5.0, tw, 20.0), Font::BodyBold, t.t1, Align::Left);
            let sub = match e.kind {
                Kind::Login if !e.username.is_empty() => e.username.clone(),
                Kind::Login => crate::csv::host_of(&e.url),
                Kind::Totp => {
                    if e.username.is_empty() {
                        kind_name(Kind::Totp).to_string()
                    } else {
                        e.username.clone()
                    }
                }
                Kind::Note => kind_name(Kind::Note).to_string(),
                Kind::Card => card_masked(&e.card_number),
            };
            c.text(&sub, rect(tx, row.y + 24.0, tw, 18.0), Font::Small, t.t3, Align::Left);
            if let Some(code) = code {
                let s = totp::grouped(&code.code_at(now));
                draw::text(c, &s, rect(row.right() - 10.0 - star - code_w, row.y, code_w, row.h), 14.0, 600, t.t1, Align::Right, true);
            }
            if e.favorite {
                c.icon_fill(&icons::STAR, rect(row.right() - 24.0, row.y + (row.h - 14.0) / 2.0, 14.0, 14.0), Color::hex(0xeab308));
            }
            self.hits.add(row, hit);
            let mut e = e;
            e.wipe();
        }
        c.unclip();
    }

    fn paint_detail_pane(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        c.clip(r);
        if self.form.is_some() {
            self.paint_form(c, r);
        } else if let Some(e) = self.selected().cloned() {
            self.paint_detail(c, r, &e);
            let mut e = e;
            e.wipe();
        } else {
            let empty = self.vault.data.entries.is_empty();
            let (title, line) = if empty {
                (tr("Your vault is empty"), tr("Add your first login, or import a CSV file in Settings."))
            } else {
                (tr("Nothing selected"), tr("Choose an item in the list, or add a new one."))
            };
            let cy = r.y + r.h / 2.0 - 70.0;
            c.icon(&ic::KEY, rect(r.x + (r.w - 40.0) / 2.0, cy, 40.0, 40.0), t.t3);
            c.text(title, rect(r.x + 24.0, cy + 54.0, r.w - 48.0, 26.0), Font::Large, t.t1, Align::Center);
            c.text(line, rect(r.x + 24.0, cy + 84.0, r.w - 48.0, 22.0), Font::Body, t.t3, Align::Center);
            let label = tr("New login");
            let bw = c.text_width(label, Font::BodyBold) + 52.0;
            self.icon_pill(c, r.x + (r.w - bw) / 2.0, cy + 122.0, 36.0, &icons::PLUS, label, 1, Hit::NewKind(Kind::Login));
        }
        c.unclip();
    }

    /// A row of the details: label, value, and its buttons on the right. Returns its height.
    #[allow(clippy::too_many_arguments)]
    fn detail_row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, value: &str, mono: bool, actions: &[(&'static Icon, Hit)]) -> f32 {
        let t = self.t.clone();
        let h = 62.0;
        c.text(label, rect(x, y + 10.0, w, 18.0), Font::Small, t.t3, Align::Left);
        let aw = actions.len() as f32 * 36.0;
        if mono {
            draw::text(c, value, rect(x, y + 28.0, w - aw - 8.0, 24.0), 15.0, 400, t.t1, Align::Left, true);
        } else {
            c.text(value, rect(x, y + 28.0, w - aw - 8.0, 24.0), Font::Body, t.t1, Align::Left);
        }
        let mut bx = x + w - aw;
        for (icon, hit) in actions {
            self.icon_btn(c, icon, rect(bx + 2.0, y + 16.0, 32.0, 32.0), hit.clone(), t.t2);
            bx += 36.0;
        }
        h
    }

    fn paint_detail(&mut self, c: &mut Canvas, r: Rect, e: &Entry) {
        let t = self.t.clone();
        let w = (r.w - 64.0).min(680.0);
        let x = r.x + 32.0;
        let mut y = r.y + 26.0 - self.page_scroll;
        // header
        self.badge(c, rect(x, y, 52.0, 52.0), e);
        let mut bx = x + w;
        bx -= 36.0;
        self.icon_btn(c, &icons::TRASH, rect(bx, y + 9.0, 34.0, 34.0), Hit::Delete, t.t2);
        let ew = c.text_width(tr("Edit"), Font::BodyBold) + 32.0;
        bx -= ew + 8.0;
        self.pill(c, rect(bx, y + 9.0, ew, 34.0), tr("Edit"), 0, Hit::Edit);
        bx -= 42.0;
        let star = rect(bx, y + 9.0, 34.0, 34.0);
        if e.favorite {
            self.hits.add(star, Hit::Fav);
            if self.hovered(&Hit::Fav) {
                c.fill_round(star, 17.0, t.hover);
            }
            c.icon_fill(&icons::STAR, rect(star.x + 9.0, star.y + 9.0, 16.0, 16.0), Color::hex(0xeab308));
        } else {
            self.icon_btn(c, &icons::STAR, star, Hit::Fav, t.t2);
        }
        let tw = bx - x - 74.0;
        c.text(&e.title, rect(x + 66.0, y + 2.0, tw, 28.0), Font::Large, t.t1, Align::Left);
        let mut sub = kind_name(e.kind).to_string();
        if !e.folder.is_empty() {
            sub = format!("{sub}  ·  {}", e.folder);
        }
        c.text(&sub, rect(x + 66.0, y + 30.0, tw, 18.0), Font::Small, t.t3, Align::Left);
        y += 74.0;

        // the fields
        let mut rows: Vec<DetailRow> = Vec::new();
        let shown = |k: &str| self.shown.contains(k);
        match e.kind {
            Kind::Login => {
                if !e.username.is_empty() {
                    rows.push((tr("User name").into(), e.username.clone(), false, vec![(&icons::COPY, Hit::Copy("username"))]));
                }
                if !e.password.is_empty() {
                    let v = if shown("password") { e.password.clone() } else { DOTS.into() };
                    rows.push((
                        tr("Password").into(),
                        v,
                        true,
                        vec![(if shown("password") { &ic::EYE_OFF } else { &ic::EYE }, Hit::Show("password")), (&icons::COPY, Hit::Copy("password"))],
                    ));
                }
                if !e.url.is_empty() {
                    rows.push((tr("Website").into(), e.url.clone(), false, vec![(&ic::LINK, Hit::OpenUrl), (&icons::COPY, Hit::Copy("url"))]));
                }
            }
            Kind::Totp => {
                if !e.username.is_empty() {
                    rows.push((tr("Account").into(), e.username.clone(), false, vec![(&icons::COPY, Hit::Copy("username"))]));
                }
            }
            Kind::Card => {
                if !e.card_holder.is_empty() {
                    rows.push((tr("Cardholder").into(), e.card_holder.clone(), false, vec![(&icons::COPY, Hit::Copy("holder"))]));
                }
                if !e.card_number.is_empty() {
                    let v = if shown("number") { card_groups(&e.card_number) } else { card_masked(&e.card_number) };
                    rows.push((
                        tr("Card number").into(),
                        v,
                        true,
                        vec![(if shown("number") { &ic::EYE_OFF } else { &ic::EYE }, Hit::Show("number")), (&icons::COPY, Hit::Copy("number"))],
                    ));
                }
                if !e.card_expiry.is_empty() {
                    rows.push((tr("Expires").into(), e.card_expiry.clone(), true, vec![(&icons::COPY, Hit::Copy("expiry"))]));
                }
                if !e.card_cvv.is_empty() {
                    let v = if shown("cvv") { e.card_cvv.clone() } else { "•••".into() };
                    rows.push((
                        tr("Security code").into(),
                        v,
                        true,
                        vec![(if shown("cvv") { &ic::EYE_OFF } else { &ic::EYE }, Hit::Show("cvv")), (&icons::COPY, Hit::Copy("cvv"))],
                    ));
                }
            }
            Kind::Note => {}
        }
        let code = if e.totp.is_empty() { None } else { Totp::parse(&e.totp) };
        let notes_h = if e.notes.is_empty() { 0.0 } else { c.text_height(&e.notes, w - 40.0 - 36.0, Font::Body) + 44.0 };
        let card_h = rows.len() as f32 * 62.0 + if code.is_some() { 84.0 } else { 0.0 } + notes_h + 8.0;
        if card_h > 8.0 {
            let card = rect(x, y, w, card_h);
            c.fill_round(card, 16.0, t.card);
            c.stroke_round(card, 16.0, t.sep, 1.0);
            let ix = x + 20.0;
            let iw = w - 32.0;
            let mut ry = y + 4.0;
            let n = rows.len();
            for (k, (label, value, mono, actions)) in rows.into_iter().enumerate() {
                let mut value = value;
                ry += self.detail_row(c, ix, ry, iw, &label, &value, mono, &actions);
                zeroize::Zeroize::zeroize(&mut value);
                if k + 1 < n || code.is_some() || notes_h > 0.0 {
                    c.hline(ix, ry, iw - 8.0, t.sep);
                }
                // the strength of the password, under it
                if label == tr("Password") {
                    let (s, _) = generator::strength(&e.password);
                    let name = strength_name(s);
                    let col = match s.0 {
                        0 => t.red,
                        1 => t.orange,
                        2 => Color::hex(0xeab308),
                        _ => t.green,
                    };
                    let lw = c.text_width(name, Font::SmallBold) + 18.0;
                    let pr = rect(ix + iw - 8.0 - 72.0 - lw, ry - 46.0, lw, 20.0);
                    c.fill_round(pr, 10.0, col.fade(0.16));
                    c.text(name, pr, Font::SmallBold, col, Align::Center);
                }
            }
            if let Some(code) = &code {
                let now = crate::vault::now();
                c.text(tr("2FA code"), rect(ix, ry + 10.0, iw, 18.0), Font::Small, t.t3, Align::Left);
                let s = totp::grouped(&code.code_at(now));
                draw::text(c, &s, rect(ix, ry + 30.0, iw - 60.0, 34.0), 26.0, 700, t.t1, Align::Left, true);
                let left = code.left(now);
                let frac = left as f32 / code.period as f32;
                let bar = rect(ix, ry + 70.0, 160.0, 4.0);
                c.fill_round(bar, 2.0, t.fill);
                c.fill_round(rect(bar.x, bar.y, bar.w * frac, 4.0), 2.0, if left <= 5 { t.orange } else { t.accent });
                c.text(&fill(tr("{n} s"), &[&left.to_string()]), rect(bar.right() + 8.0, ry + 62.0, 60.0, 18.0), Font::Small, t.t3, Align::Left);
                self.icon_btn(c, &icons::COPY, rect(ix + iw - 36.0 + 2.0, ry + 26.0, 32.0, 32.0), Hit::Copy("code"), t.t2);
                ry += 84.0;
                if notes_h > 0.0 {
                    c.hline(ix, ry, iw - 8.0, t.sep);
                }
            }
            if notes_h > 0.0 {
                c.text(tr("Notes"), rect(ix, ry + 10.0, iw, 18.0), Font::Small, t.t3, Align::Left);
                c.text_block(&e.notes, rect(ix, ry + 30.0, iw - 44.0, notes_h - 40.0), Font::Body, t.t1);
                self.icon_btn(c, &icons::COPY, rect(ix + iw - 36.0 + 2.0, ry + 16.0, 32.0, 32.0), Hit::Copy("notes"), t.t2);
            }
            y += card_h + 18.0;
        }
        if !e.tags.is_empty() {
            let mut tx = x;
            for tag in &e.tags {
                let tw = c.text_width(tag, Font::Small) + 22.0;
                c.fill_round(rect(tx, y, tw, 24.0), 12.0, t.fill);
                c.text(tag, rect(tx, y, tw, 24.0), Font::Small, t.t2, Align::Center);
                tx += tw + 6.0;
            }
            y += 36.0;
        }
        if e.kind == Kind::Login && !e.password.is_empty() {
            self.icon_pill(c, x, y, 36.0, &ic::KEYBOARD, tr("Auto-type"), if self.pick.is_some() { 1 } else { 0 }, Hit::AutoType);
            let hint = if self.set.autotype {
                fill(tr("Or press {key} in the other window."), &[self.set.hotkey().0])
            } else {
                tr("Types the user name, Tab and the password into the window you used last.").to_string()
            };
            let pw = c.text_width(tr("Auto-type"), Font::BodyBold) + 52.0;
            c.text(&hint, rect(x + pw + 12.0, y, w - pw - 12.0, 36.0), Font::Small, t.t3, Align::Left);
            y += 50.0;
        }
        c.text(&fill(tr("Edited {d}"), &[&date(e.modified)]), rect(x, y, w, 18.0), Font::Small, t.t4, Align::Left);
        y += 30.0;
        self.page_h = y + self.page_scroll - r.y;
        self.page_scroll = self.page_scroll.min((self.page_h - r.h).max(0.0));
    }

    fn paint_form(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        let Some(fm) = &self.form else { return };
        let (kind, new, error, reveal) = (fm.e.kind, fm.new, fm.error.clone(), fm.reveal);
        let pw = fm.e.password.clone();
        let w = (r.w - 64.0).min(620.0);
        let x = r.x + 32.0;
        let mut y = r.y + 24.0 - self.page_scroll;
        let title = match (new, kind) {
            (false, _) => tr("Edit"),
            (true, Kind::Login) => tr("New login"),
            (true, Kind::Totp) => tr("New 2FA code"),
            (true, Kind::Note) => tr("New secure note"),
            (true, Kind::Card) => tr("New card"),
        };
        c.text(title, rect(x, y, w, 30.0), Font::Large, t.t1, Align::Left);
        y += 40.0;
        if new {
            const KINDS: [Kind; 4] = [Kind::Login, Kind::Totp, Kind::Note, Kind::Card];
            let labels: Vec<&str> = KINDS.iter().map(|k| kind_name(*k)).collect();
            let cur = KINDS.iter().position(|k| *k == kind).unwrap_or(0);
            self.segmented(c, x, y, &labels, cur, Font::Body, |i| Hit::Kind(KINDS[i]));
            y += 48.0;
        }
        for &f in form_fields(kind) {
            let (label, hint) = match (f, kind) {
                (F::Title, _) => (tr("Name"), tr("Example: Mail")),
                (F::User, Kind::Totp) => (tr("Account"), tr("The account the code is for")),
                (F::User, _) => (tr("User name"), tr("Email or user name")),
                (F::Pass, _) => (tr("Password"), ""),
                (F::Url, _) => (tr("Website"), "https://"),
                (F::Totp, _) => (tr("2FA key"), tr("Secret key or otpauth:// address")),
                (F::Folder, _) => (tr("Folder"), tr("None")),
                (F::Tags, _) => (tr("Tags"), tr("Separated by commas")),
                (F::Notes, _) => (tr("Notes"), ""),
                (F::Holder, _) => (tr("Cardholder"), tr("Name on the card")),
                (F::Number, _) => (tr("Card number"), ""),
                (F::Expiry, _) => (tr("Expires"), "MM/YY"),
                (F::Cvv, _) => (tr("Security code"), "CVV"),
                _ => ("", ""),
            };
            c.text(label, rect(x, y, w, 18.0), Font::SmallBold, t.t2, Align::Left);
            y += 22.0;
            match f {
                F::Pass => {
                    let fr = self.field(c, rect(x, y, w, 38.0), f, "", 72.0);
                    self.icon_btn(c, if reveal { &ic::EYE_OFF } else { &ic::EYE }, rect(fr.right() - 72.0, fr.y + 4.0, 30.0, 30.0), Hit::Reveal(F::Pass), t.t3);
                    self.icon_btn(c, &ic::DICE, rect(fr.right() - 38.0, fr.y + 4.0, 30.0, 30.0), Hit::Generate, t.t3);
                    y += 44.0;
                    self.strength_bar(c, x, y, w.min(360.0), &pw);
                    y += 26.0;
                }
                F::Number | F::Cvv => {
                    let fr = self.field(c, rect(x, y, if f == F::Cvv { 160.0 } else { w }, 38.0), f, hint, 38.0);
                    self.icon_btn(c, if reveal { &ic::EYE_OFF } else { &ic::EYE }, rect(fr.right() - 38.0, fr.y + 4.0, 30.0, 30.0), Hit::Reveal(f), t.t3);
                    y += 50.0;
                }
                F::Expiry => {
                    self.field(c, rect(x, y, 160.0, 38.0), f, hint, 0.0);
                    y += 50.0;
                }
                F::Totp => {
                    let label = tr("From a QR picture…");
                    let bw = c.text_width(label, Font::BodyBold) + 52.0;
                    self.field(c, rect(x, y, w - bw - 10.0, 38.0), f, hint, 0.0);
                    self.icon_pill(c, x + w - bw, y + 1.0, 36.0, &ic::IMAGE, label, 0, Hit::QrImage);
                    y += 50.0;
                }
                F::Folder => {
                    self.field(c, rect(x, y, w, 38.0), f, hint, 0.0);
                    y += 44.0;
                    let cur = self.form.as_ref().map(|fm| fm.e.folder.clone()).unwrap_or_default();
                    let mut cx_ = x;
                    for name in self.folders().into_iter().take(8) {
                        let cw = c.text_width(&name, Font::Small) + 22.0;
                        if cx_ + cw > x + w {
                            break;
                        }
                        let hit = Hit::FolderChip(name.clone());
                        let on = cur == name;
                        let col = if on {
                            t.accent.fade(0.3)
                        } else if self.hovered(&hit) {
                            t.fill2
                        } else {
                            t.fill
                        };
                        c.fill_round(rect(cx_, y, cw, 24.0), 12.0, col);
                        c.text(&name, rect(cx_, y, cw, 24.0), Font::Small, t.t1, Align::Center);
                        self.hits.add(rect(cx_, y, cw, 24.0), hit);
                        cx_ += cw + 6.0;
                    }
                    if cx_ > x {
                        y += 34.0;
                    }
                }
                F::Notes => {
                    self.field(c, rect(x, y, w, 110.0), f, hint, 0.0);
                    y += 122.0;
                }
                _ => {
                    self.field(c, rect(x, y, w, 38.0), f, hint, 0.0);
                    y += 50.0;
                }
            }
        }
        if !error.is_empty() {
            c.text(&error, rect(x, y, w, 22.0), Font::Body, t.red, Align::Left);
            y += 30.0;
        }
        let sw = c.text_width(tr("Save"), Font::BodyBold) + 44.0;
        self.pill(c, rect(x, y, sw, 36.0), tr("Save"), 1, Hit::Save);
        let cw = c.text_width(tr("Cancel"), Font::BodyBold) + 40.0;
        self.pill(c, rect(x + sw + 10.0, y, cw, 36.0), tr("Cancel"), 0, Hit::Cancel);
        y += 60.0;
        self.page_h = y + self.page_scroll - r.y;
        self.page_scroll = self.page_scroll.min((self.page_h - r.h).max(0.0));
    }

    // ───────────────────────── on top ─────────────────────────

    fn paint_menu(&mut self, c: &mut Canvas) {
        if !self.menu_new || self.screen != Screen::Main {
            return;
        }
        let t = self.t.clone();
        let nb = self.new_btn;
        let mw = 200.0;
        let card = rect(nb.right() - mw, nb.bottom() + 6.0, mw, 4.0 * 38.0 + 12.0);
        c.fill_round(card.offset(0.0, 2.0), 14.0, Color::hex(0).fade(0.18));
        c.fill_round(card, 14.0, t.solid);
        c.stroke_round(card, 14.0, t.ring, 1.0);
        let mut y = card.y + 6.0;
        for (k, icon) in [(Kind::Login, &ic::KEY), (Kind::Totp, &ic::CLOCK), (Kind::Note, &ic::NOTE), (Kind::Card, &ic::CARD)] {
            let r = rect(card.x + 6.0, y, mw - 12.0, 38.0);
            let hit = Hit::NewKind(k);
            if self.hovered(&hit) {
                c.fill_round(r, 10.0, t.hover);
            }
            c.icon(icon, rect(r.x + 10.0, r.y + 11.0, 16.0, 16.0), t.t2);
            c.text(kind_name(k), rect(r.x + 36.0, r.y, r.w - 40.0, r.h), Font::Body, t.t1, Align::Left);
            self.hits.add(r, hit);
            y += 38.0;
        }
    }

    fn paint_modal(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(modal) = self.modal.clone() else { return };
        let t = self.t.clone();
        c.fill(rect(0.0, m::TITLE_H, w, h - m::TITLE_H), Color::hex(0).fade(0.45));
        self.hits.add(rect(0.0, m::TITLE_H, w, h - m::TITLE_H), Hit::Backdrop);
        let name_of = |id: &str| self.entry(id).map(|e| e.title.clone()).unwrap_or_default();
        let (title, text, ok, danger, fields): (String, String, &str, bool, Vec<(F, &str)>) = match &modal {
            Modal::Delete(id) => {
                (fill(tr("Delete \u{201c}{name}\u{201d}?"), &[&name_of(id)]), tr("It cannot be brought back.").into(), tr("Delete"), true, vec![])
            }
            Modal::NewFolder => (tr("New folder").into(), String::new(), tr("Create"), false, vec![(F::ModalA, tr("Name"))]),
            Modal::DeleteFolder(name) => (
                fill(tr("Remove the folder \u{201c}{name}\u{201d}?"), &[name]),
                tr("Its items stay in the vault, without a folder.").into(),
                tr("Remove"),
                true,
                vec![],
            ),
            Modal::ChangePassword => (
                tr("Change the master password").into(),
                tr("Your vault is sealed again with the new password. Backups keep their old password.").into(),
                tr("Change"),
                false,
                vec![(F::ModalA, tr("Current master password")), (F::ModalB, tr("New master password")), (F::ModalC, tr("Type it again"))],
            ),
            Modal::Restore(path) => (
                tr("Restore from a backup").into(),
                fill(
                    tr("Items of {file} that are not in your vault are added; newer versions replace older ones."),
                    &[&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()],
                ),
                tr("Restore"),
                false,
                vec![(F::ModalA, tr("The backup's master password"))],
            ),
        };
        let mw = 460.0f32.min(w - 40.0);
        let text_h = if text.is_empty() { 0.0 } else { c.text_height(&text, mw - 56.0, Font::Body) + 12.0 };
        let strength = matches!(modal, Modal::ChangePassword);
        let mh = 72.0 + text_h + fields.len() as f32 * 48.0 + if strength { 22.0 } else { 0.0 } + if self.modal_error.is_empty() { 0.0 } else { 28.0 } + 70.0;
        let card = rect((w - mw) / 2.0, (h - mh) / 2.0, mw, mh);
        c.fill_round(card, 20.0, t.solid);
        c.stroke_round(card, 20.0, t.ring, 1.0);
        self.hits.add(card, Hit::Backdrop);
        let x = card.x + 28.0;
        let fw = mw - 56.0;
        let mut y = card.y + 24.0;
        c.text(&title, rect(x, y, fw, 26.0), Font::Large, t.t1, Align::Left);
        y += 40.0;
        if !text.is_empty() {
            c.text_block(&text, rect(x, y, fw, text_h), Font::Body, t.t2);
            y += text_h;
        }
        let secret = matches!(modal, Modal::ChangePassword | Modal::Restore(_));
        for (k, (f, hint)) in fields.iter().enumerate() {
            let r = self.field(c, rect(x, y, fw, 38.0), *f, hint, if secret && k == 0 { 34.0 } else { 0.0 });
            if secret && k == 0 {
                self.icon_btn(c, if self.reveal_modal { &ic::EYE_OFF } else { &ic::EYE }, rect(r.right() - 36.0, r.y + 4.0, 30.0, 30.0), Hit::Reveal(*f), t.t3);
            }
            y += 48.0;
            if strength && *f == F::ModalB {
                let pw = self.modal_b.clone();
                self.strength_bar(c, x, y - 6.0, fw, &pw);
                y += 22.0;
            }
        }
        if !self.modal_error.is_empty() {
            c.text(&self.modal_error.clone(), rect(x, y, fw, 22.0), Font::Body, t.red, Align::Left);
            y += 28.0;
        }
        y += 10.0;
        let ow = c.text_width(ok, Font::BodyBold) + 44.0;
        let cw = c.text_width(tr("Cancel"), Font::BodyBold) + 40.0;
        self.pill(c, rect(card.right() - 28.0 - ow, y, ow, 36.0), ok, if danger { 2 } else { 1 }, Hit::ModalOk);
        self.pill(c, rect(card.right() - 28.0 - ow - 10.0 - cw, y, cw, 36.0), tr("Cancel"), 0, Hit::ModalCancel);
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((text, _)) = self.toast.clone() else { return };
        let t = self.t.clone();
        let tw = (c.text_width(&text, Font::Body) + 44.0).min(w - 40.0);
        let lines = if c.text_width(&text, Font::Body) + 44.0 > w - 40.0 { 2.0 } else { 1.0 };
        let th = 22.0 * lines + 18.0;
        let r = rect((w - tw) / 2.0, h - th - 24.0, tw, th);
        let radius = if lines > 1.0 { 18.0 } else { th / 2.0 };
        c.fill_round(r, radius, t.solid2);
        c.stroke_round(r, radius, t.ring, 1.0);
        if lines > 1.0 {
            c.text_block(&text, rect(r.x + 20.0, r.y + 9.0, r.w - 40.0, r.h - 18.0), Font::Body, t.t1);
        } else {
            c.text(&text, r.inset_xy(20.0, 0.0), Font::Body, t.t1, Align::Center);
        }
    }
}

/// The semibold variant of a font.
pub fn bold(f: Font) -> Font {
    match f {
        Font::Body => Font::BodyBold,
        Font::Small => Font::SmallBold,
        f => f,
    }
}

/// A string wiped when dropped.
struct Zeroizing(String);

impl Drop for Zeroizing {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}
