//! Drawing the frame: title bar (with the quick-add box), toolbar, sidebar (mini month, calendars), and what floats
//! above the views: event details, menus, dialogs, the toast and the reminders.

use super::{App, ConfirmAct, DlgKind, Drag, Field, Id, OccRef, View, color, editor};
use crate::{
    date::Date,
    i18n::{self, tr},
    model::PALETTE,
    popup,
    tz::{self},
};
use plume_ui::{
    Align, Canvas, Color, Font, PaintCx, Rect, Theme,
    icons::{self, Icon, Part},
    m, rect,
};
use windows::{
    Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
        WICDecodeMetadataCacheOnLoad,
    },
    core::Interface,
};

pub const SIDEBAR_W: f32 = 248.0;
pub const TOOLBAR_H: f32 = 56.0;

pub const BELL: Icon = Icon { name: "BELL", vb: 16.0, parts: &[Part::P("M4 11V7.5a4 4 0 0 1 8 0V11l1.2 1.5H2.8z"), Part::P("M6.6 14h2.8")] };
pub const REPEAT: Icon =
    Icon { name: "REPEAT", vb: 16.0, parts: &[Part::P("M3 7.5V6.5A2 2 0 0 1 5 4.5h8M11 2.5l2 2-2 2"), Part::P("M13 8.5v1a2 2 0 0 1-2 2H3M5 13.5l-2-2 2-2")] };
pub const PLACE: Icon =
    Icon { name: "PLACE", vb: 16.0, parts: &[Part::P("M8 14s4.5-4.2 4.5-7.5a4.5 4.5 0 0 0-9 0C3.5 9.8 8 14 8 14z"), Part::C(8.0, 6.5, 1.6)] };
pub const CLOCK: Icon = Icon { name: "CLOCK", vb: 16.0, parts: &[Part::C(8.0, 8.0, 5.8), Part::P("M8 4.8V8l2.2 1.6")] };
pub const LINK: Icon = Icon {
    name: "LINK",
    vb: 16.0,
    parts: &[Part::P("M6.8 9.2a2.6 2.6 0 0 0 3.7 0l2-2a2.6 2.6 0 0 0-3.7-3.7l-.8.8"), Part::P("M9.2 6.8a2.6 2.6 0 0 0-3.7 0l-2 2a2.6 2.6 0 0 0 3.7 3.7l.8-.8")],
};
pub const NOTE: Icon = Icon { name: "NOTE", vb: 16.0, parts: &[Part::P("M3 4h10M3 8h10M3 12h6")] };

const LOGO: &[u8] = include_bytes!("../../../../assets/brand/plume-calendar-256.png");

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

/// The opaque color under a text field (the native field paints it).
pub fn field_bg(t: &Theme, f: Field) -> Color {
    let under = match f {
        Field::Quick => t.hdr2,
        _ => t.solid,
    };
    t.fill.over(under)
}

/// Soft fill and readable text for an event of color `c`.
pub fn event_colors(t: &Theme, c: Color) -> (Color, Color) {
    if t.dark { (c.fade(0.30).over(t.bg), c.mix(Color::hex(0xffffff), 0.62)) } else { (c.fade(0.18).over(t.bg), c.mix(Color::hex(0x000000), 0.5)) }
}

impl App {
    // ───────────────────────── small pieces ─────────────────────────

    pub(super) fn hot(&self, id: &Id) -> bool {
        self.hover.as_ref() == Some(id)
    }

    /// A pill button. style: 0 secondary, 1 accent, 2 red, 3 quiet (no fill until hovered).
    pub(super) fn btn(&mut self, c: &mut Canvas, r: Rect, label: &str, style: u8, id: Id) {
        let t = self.t.clone();
        let hot = self.hot(&id);
        let down = hot && self.pressed.as_ref() == Some(&id);
        let base = match style {
            1 => t.accent,
            2 => t.red,
            3 => Color::TRANSPARENT,
            _ => t.fill,
        };
        let fill = match (style, hot, down) {
            (_, _, true) if style == 1 || style == 2 => base.mix(Color::hex(0), 0.15),
            (_, _, true) => t.fill2,
            (0 | 3, true, _) => t.fill2,
            (_, true, _) => base.mix(Color::hex(0xffffff), 0.12),
            _ => base,
        };
        c.fill_round(r, r.h / 2.0, fill);
        let ink = match style {
            1 | 2 => Color::hex(0xffffff),
            _ => t.t1,
        };
        c.text(label, r.inset_xy(10.0, 0.0), Font::BodyBold, ink, Align::Center);
        self.hits.add(r, id);
    }

    pub(super) fn icon_btn(&mut self, c: &mut Canvas, r: Rect, ic: &Icon, id: Id) {
        let t = self.t.clone();
        if self.hot(&id) {
            c.fill_round(r, (r.h / 2.0).min(9.0), t.hover);
        }
        c.icon(ic, r.centered(16.0, 16.0), t.t2);
        self.hits.add(r, id);
    }

    pub(super) fn chip(&mut self, c: &mut Canvas, r: Rect, label: &str, on: bool, id: Id) {
        let t = self.t.clone();
        let fill = if on {
            t.accent.fade(if t.dark { 0.32 } else { 0.18 })
        } else if self.hot(&id) {
            t.fill2
        } else {
            t.fill
        };
        c.fill_round(r, r.h / 2.0, fill);
        if on {
            c.stroke_round(r, r.h / 2.0, t.accent.fade(0.7), 1.0);
        }
        c.text(label, r.inset_xy(8.0, 0.0), if on { Font::SmallBold } else { Font::Small }, if on { t.t1 } else { t.t2 }, Align::Center);
        self.hits.add(r, id);
    }

    /// A segmented control; returns its width.
    pub(super) fn seg(&mut self, c: &mut Canvas, x: f32, y: f32, h: f32, opts: &[(String, Id, bool)]) -> f32 {
        let t = self.t.clone();
        let widths: Vec<f32> = opts.iter().map(|(l, _, _)| c.text_width(l, Font::BodyBold) + 26.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 6.0;
        let r = rect(x, y, total, h);
        c.fill_round(r, h / 2.0, t.fill);
        let mut xx = x + 3.0;
        for ((label, id, on), w) in opts.iter().zip(widths) {
            let b = rect(xx, y + 3.0, w, h - 6.0);
            if *on {
                c.fill_round(b, b.h / 2.0, if t.dark { t.solid2 } else { t.solid });
                c.stroke_round(b, b.h / 2.0, t.ring, 1.0);
            } else if self.hot(id) {
                c.fill_round(b, b.h / 2.0, t.hover);
            }
            c.text(label, b, if *on { Font::BodyBold } else { Font::Body }, if *on { t.t1 } else { t.t2 }, Align::Center);
            self.hits.add(b, id.clone());
            xx += w;
        }
        total
    }

    pub(super) fn toggle(&mut self, c: &mut Canvas, r: Rect, on: bool, id: Id) {
        let t = self.t.clone();
        let tr_ = rect(r.x, r.y + (r.h - 22.0) / 2.0, 38.0, 22.0);
        c.fill_round(tr_, 11.0, if on { t.accent } else { t.fill2 });
        let k = if on { tr_.x + 18.0 } else { tr_.x + 2.0 };
        c.fill_round(rect(k, tr_.y + 2.0, 18.0, 18.0), 9.0, Color::hex(0xffffff));
        self.hits.add(r, id);
    }

    pub(super) fn toggle_row(&mut self, c: &mut Canvas, r: Rect, label: &str, on: bool, id: Id) {
        let t = self.t.clone();
        self.toggle(c, rect(r.x, r.y, 40.0, r.h), on, id.clone());
        c.text(label, rect(r.x + 48.0, r.y, r.w - 48.0, r.h), Font::Body, t.t1, Align::Left);
        self.hits.add(r, id);
    }

    /// A text field drawn by the window; the native field covers it while it is being typed in.
    pub(super) fn field_box(&mut self, c: &mut Canvas, r: Rect, f: Field, shown: &str, placeholder: &str, font: Font) {
        let t = self.t.clone();
        let open = self.edit_for == Some(f);
        let bg = t.fill.over(if f == Field::Quick { t.hdr2 } else { t.solid });
        c.fill_round(r, 9.0, bg);
        if open {
            c.stroke_round(r, 9.0, t.accent, 1.5);
            let inner = rect(r.x + 10.0, r.y + (r.h - 20.0) / 2.0, r.w - 20.0, 20.0);
            self.edit_rect = Some(inner);
        }
        if !open || self.edit_shown != Some(f) {
            if !open && self.hot(&Id::Field(f)) {
                c.stroke_round(r, 9.0, t.ring, 1.0);
            }
            if shown.is_empty() {
                c.text(placeholder, r.inset_xy(12.0, 0.0), font, t.t4, Align::Left);
            } else {
                c.text(shown, r.inset_xy(12.0, 0.0), font, t.t1, Align::Left);
            }
        }
        self.hits.add(r, Id::Field(f));
    }

    /// − and + beside a date or time.
    pub(super) fn stepper(&mut self, c: &mut Canvas, x: f32, y: f32, f: Field) {
        let t = self.t.clone();
        for (k, (dir, glyph)) in [(-1, "−"), (1, "+")].into_iter().enumerate() {
            let r = rect(x + k as f32 * 28.0, y + 2.0, 26.0, 28.0);
            let id = Id::EdStep(f, dir);
            if self.hot(&id) {
                c.fill_round(r, 8.0, t.hover);
            }
            c.text(glyph, r, Font::Title, t.t2, Align::Center);
            self.hits.add(r, id);
        }
    }

    /// A value that changes on click: "Calendar  ● Personal ›".
    pub(super) fn pick(&mut self, c: &mut Canvas, r: Rect, label: &str, value: &str, dot: Option<Color>, id: Id) {
        let t = self.t.clone();
        c.fill_round(r, 10.0, if self.hot(&id) { t.fill2 } else { t.fill });
        let lw = c.text_width(label, Font::Small) + 12.0;
        c.text(label, rect(r.x + 12.0, r.y, lw, r.h), Font::Small, t.t3, Align::Left);
        let mut x = r.x + 12.0 + lw;
        if let Some(d) = dot {
            c.fill_round(rect(x, r.y + r.h / 2.0 - 5.0, 10.0, 10.0), 5.0, d);
            x += 16.0;
        }
        c.text(value, rect(x, r.y, r.right() - x - 26.0, r.h), Font::BodyBold, t.t1, Align::Left);
        c.icon(&icons::CHEVRON_RIGHT, rect(r.right() - 24.0, r.y + r.h / 2.0 - 7.0, 14.0, 14.0), t.t3);
        self.hits.add(r, id);
    }

    pub(super) fn card(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        c.fill_round(r.offset(0.0, 6.0).inset(-2.0), 18.0, Color::hexa(0x000000, if t.dark { 0.35 } else { 0.10 }));
        c.fill_round(r, 16.0, t.solid);
        c.stroke_round(r, 16.0, t.ring, 1.0);
    }

    /// The veil under a modal; a light one (menus, details) closes on click.
    pub(super) fn paint_scrim(&mut self, c: &mut Canvas, cx: &PaintCx, light: bool) {
        let r = rect(0.0, m::TITLE_H, cx.w, cx.h - m::TITLE_H);
        if !light {
            c.fill(r, Color::hexa(0x000000, if self.t.dark { 0.45 } else { 0.25 }));
        }
        self.hits.add(r, Id::Scrim);
    }

    // ───────────────────────── frame ─────────────────────────

    pub(super) fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.hits.clear();
        self.edit_rect = None;
        self.refresh_occs();
        let t = self.t.clone();
        c.clear(t.bg);
        let mut body = rect(0.0, m::TITLE_H, cx.w, cx.h - m::TITLE_H);
        let side = body.cut_left(SIDEBAR_W);
        self.paint_sidebar(c, side);
        c.vline(side.right(), side.y, side.h, t.sep);
        if self.settings_open {
            self.paint_settings(c, body);
        } else {
            let bar = body.cut_top(TOOLBAR_H);
            self.paint_toolbar(c, bar);
            match self.view {
                View::Month => self.paint_month(c, body),
                View::Week | View::Day => self.paint_week(c, body, cx),
                View::Agenda => self.paint_agenda(c, body),
            }
        }
        let modal = self.editor.is_some() || self.dialog.is_some() || self.scope.is_some() || self.confirm.is_some();
        if !modal {
            if self.peek.is_some() || self.menu.is_some() {
                self.paint_scrim(c, cx, true);
            }
            self.paint_peek(c, cx);
            self.paint_menu(c, cx);
        }
        self.paint_editor(c, cx);
        self.paint_dialog(c, cx);
        self.paint_scope(c, cx);
        self.paint_confirm(c, cx);
        self.paint_alerts(c, cx);
        self.paint_toast(c, cx);
        self.paint_title(c, cx, modal);
    }

    fn paint_title(&mut self, c: &mut Canvas, cx: &PaintCx, modal: bool) {
        let t = self.t.clone();
        c.gradient_v(rect(0.0, 0.0, cx.w, m::TITLE_H), t.hdr1, t.hdr2);
        c.hline(0.0, m::TITLE_H - 1.0, cx.w, t.sep);
        let wic = self.gfx.wic.clone();
        c.image(1, rect(14.0, (m::TITLE_H - 20.0) / 2.0, 20.0, 20.0), 1.0, || png(&wic, LOGO));
        c.text("Plume Calendar", rect(42.0, 0.0, 160.0, m::TITLE_H), Font::BodyBold, if cx.active { t.t1 } else { t.t3 }, Align::Left);
        // the quick-add box
        let cap_x = cx.w - 3.0 * m::CAPTION_W;
        let qw = (cap_x - 220.0 - 24.0).clamp(160.0, 520.0);
        let qx = ((cx.w - qw) / 2.0).max(214.0).min(cap_x - qw - 12.0);
        let r = rect(qx, (m::TITLE_H - 30.0) / 2.0, qw, 30.0);
        if !modal || self.edit_for == Some(Field::Quick) {
            let quick = self.quick.clone();
            self.field_box(c, r, Field::Quick, &quick, tr("Add an event: “Lunch with Max tomorrow 12:30”"), Font::Body);
            if self.edit_for != Some(Field::Quick) {
                c.icon(&icons::PLUS, rect(r.right() - 26.0, r.y + 8.0, 14.0, 14.0), t.t3);
            }
        }
        let glyphs = [(Id::Min, '\u{E921}'), (Id::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Id::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
            let hot = self.hover.as_ref() == Some(&id) || self.pressed.as_ref() == Some(&id);
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

    /// "November 2026", "Nov 9 – 15, 2026"…
    pub fn period_title(&self) -> String {
        let (a, b) = self.range();
        match self.view {
            View::Month => i18n::month_year(self.anchor),
            View::Day => i18n::cap(&i18n::long_date_year(self.anchor)),
            View::Week | View::Agenda => {
                let last = b.add_days(-1);
                if a.m == last.m && a.y == last.y {
                    if i18n::french() {
                        format!("{} – {} {} {}", a.d, last.d, i18n::month(a.m), a.y)
                    } else {
                        format!("{} {} – {}, {}", i18n::month(a.m), a.d, last.d, a.y)
                    }
                } else if a.y == last.y {
                    format!("{} – {}, {}", i18n::short_date(a), i18n::short_date(last), a.y)
                } else {
                    format!("{} {} – {} {}", i18n::short_date(a), a.y, i18n::short_date(last), last.y)
                }
            }
        }
    }

    fn paint_toolbar(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        let y = r.y + (r.h - 32.0) / 2.0;
        let mut x = r.x + 18.0;
        let tw = c.text_width(tr("Today"), Font::BodyBold) + 30.0;
        self.btn(c, rect(x, y, tw, 32.0), tr("Today"), 0, Id::Today);
        x += tw + 10.0;
        self.icon_btn(c, rect(x, y, 32.0, 32.0), &icons::BACK, Id::Prev);
        x += 34.0;
        self.icon_btn(c, rect(x, y, 32.0, 32.0), &icons::FORWARD, Id::Next);
        x += 44.0;
        let title = self.period_title();
        // the view switch and the settings on the right
        let gear = rect(r.right() - 18.0 - 32.0, y, 32.0, 32.0);
        self.icon_btn(c, gear, &icons::GEAR, Id::Settings);
        let views = [View::Day, View::Week, View::Month, View::Agenda];
        let labels = [tr("Day"), tr("Week"), tr("Month"), tr("Agenda")];
        let opts: Vec<(String, Id, bool)> = views.iter().zip(labels).map(|(v, l)| (l.to_string(), Id::Tab(*v), self.view == *v)).collect();
        let sw: f32 = opts.iter().map(|(l, _, _)| c.text_width(l, Font::BodyBold) + 26.0).sum::<f32>() + 6.0;
        let sx = gear.x - 12.0 - sw;
        self.seg(c, sx, y, 32.0, &opts);
        c.text(&title, rect(x, r.y, (sx - x - 12.0).max(0.0), r.h), Font::Large, t.t1, Align::Left);
    }

    fn paint_sidebar(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        c.fill(r, if t.dark { t.bg2 } else { t.bg2.mix(t.bg, 0.4) });
        let x = r.x + 16.0;
        let w = r.w - 32.0;
        let mut y = r.y + 16.0;
        // new event
        let b = rect(x, y, w, 38.0);
        self.btn(c, b, "", 1, Id::NewEvent);
        let lw = c.text_width(tr("New event"), Font::BodyBold);
        let ix = b.x + (b.w - lw - 22.0) / 2.0;
        c.icon_stroke(&icons::PLUS, rect(ix, b.y + 12.0, 14.0, 14.0), Color::hex(0xffffff), 2.2);
        c.text(tr("New event"), rect(ix + 22.0, b.y, lw + 4.0, b.h), Font::BodyBold, Color::hex(0xffffff), Align::Left);
        y += 54.0;
        y = self.paint_mini(c, rect(x, y, w, 0.0));
        y += 18.0;
        // calendars
        c.text(tr("Calendars"), rect(x + 4.0, y, w - 40.0, 28.0), Font::SmallBold, t.t3, Align::Left);
        self.icon_btn(c, rect(r.right() - 16.0 - 28.0, y, 28.0, 28.0), &icons::PLUS, Id::CalAdd);
        y += 32.0;
        let n = self.store.cals.len();
        for i in 0..n {
            if y + 34.0 > r.bottom() {
                break;
            }
            let cal = self.store.cals[i].clone();
            let row = rect(x - 4.0, y, w + 8.0, 32.0);
            let hot = self.hot(&Id::CalToggle(i)) || self.hot(&Id::CalMore(i));
            if hot {
                c.fill_round(row, 8.0, t.hover);
            }
            let col = color(&cal.color);
            let bx = rect(x + 4.0, y + 8.0, 16.0, 16.0);
            if cal.visible {
                c.fill_round(bx, 5.0, col);
                c.icon_stroke(&icons::CHECK, bx.inset(2.0), Color::hex(0xffffff), 2.4);
            } else {
                c.stroke_round(bx.inset(0.75), 4.5, col, 1.5);
            }
            let mut name_w = w - 40.0;
            let busy = self.refreshing.contains(&cal.id);
            if cal.read_only() {
                name_w -= 34.0;
                let ic = rect(x + w - 44.0, y + 9.0, 14.0, 14.0);
                c.icon(
                    &LINK,
                    ic,
                    if cal.error.is_some() {
                        t.red
                    } else if busy {
                        t.accent
                    } else {
                        t.t3
                    },
                );
            }
            c.text(&cal.name, rect(x + 30.0, y, name_w - 4.0, 32.0), Font::Body, if cal.visible { t.t1 } else { t.t3 }, Align::Left);
            self.hits.add(row, Id::CalToggle(i));
            let more = rect(x + w - 24.0, y + 4.0, 24.0, 24.0);
            if hot {
                self.icon_btn(c, more, &icons::MORE, Id::CalMore(i));
            } else {
                self.hits.add(more, Id::CalMore(i));
            }
            y += 34.0;
        }
    }

    /// The small month in the sidebar; returns where it ends.
    fn paint_mini(&mut self, c: &mut Canvas, r: Rect) -> f32 {
        let t = self.t.clone();
        let mut y = r.y;
        c.text(&i18n::month_year(self.mini), rect(r.x + 4.0, y, r.w - 64.0, 28.0), Font::BodyBold, t.t1, Align::Left);
        self.icon_btn(c, rect(r.right() - 58.0, y, 28.0, 28.0), &icons::BACK, Id::MiniPrev);
        self.icon_btn(c, rect(r.right() - 28.0, y, 28.0, 28.0), &icons::FORWARD, Id::MiniNext);
        y += 32.0;
        let ws = self.set.week_start();
        let cw = r.w / 7.0;
        for k in 0..7 {
            let wd = (ws + k) % 7;
            c.text(&i18n::day_letter(wd), rect(r.x + k as f32 * cw, y, cw, 20.0), Font::Small, t.t3, Align::Center);
        }
        y += 22.0;
        let first = self.mini.week_start(ws);
        let today = self.today();
        let (va, vb) = self.range();
        for row in 0..6 {
            for col in 0..7 {
                let d = first.add_days(row * 7 + col);
                let cell = rect(r.x + col as f32 * cw, y + row as f32 * 28.0, cw, 28.0);
                let dot = cell.centered(26.0, 26.0);
                let id = Id::Mini(d.days());
                let in_view = d >= va && d < vb && self.view != View::Agenda && self.view != View::Month;
                if in_view {
                    c.fill_round(dot, 13.0, t.accent.fade(0.14));
                }
                if d == today {
                    c.fill_round(dot, 13.0, t.accent);
                } else if d == self.selected {
                    c.stroke_round(dot, 13.0, t.accent, 1.5);
                } else if self.hot(&id) {
                    c.fill_round(dot, 13.0, t.hover);
                }
                let ink = if d == today {
                    Color::hex(0xffffff)
                } else if d.m != self.mini.m {
                    t.t4
                } else {
                    t.t1
                };
                c.text(&d.d.to_string(), cell, if d == today { Font::SmallBold } else { Font::Small }, ink, Align::Center);
                self.hits.add(cell, id);
            }
        }
        y + 6.0 * 28.0
    }

    // ───────────────────────── floating ─────────────────────────

    /// "Wednesday, November 11 · 12:30 – 13:30".
    pub(super) fn when_text(&self, r: &OccRef) -> String {
        let z = self.zone();
        let h24 = self.h24();
        let a = z.to_wall(r.start);
        if r.all_day {
            let last = tz::date_at(&z, r.end - 1);
            if last == a.date {
                format!("{} · {}", i18n::long_date(a.date), tr("All day"))
            } else {
                format!("{} – {}", i18n::long_date(a.date), i18n::long_date(last))
            }
        } else {
            let b = z.to_wall(r.end);
            if b.date == a.date || (b.date == a.date.add_days(1) && b.secs == 0) {
                format!("{} · {} – {}", i18n::long_date(a.date), i18n::time(a.secs, h24), i18n::time(b.secs, h24))
            } else {
                format!("{} {} – {} {}", i18n::short_date(a.date), i18n::time(a.secs, h24), i18n::short_date(b.date), i18n::time(b.secs, h24))
            }
        }
    }

    fn paint_peek(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(p) = self.peek.as_ref() else { return };
        let (r, at) = (p.occ.clone(), p.at);
        let Some(ev) = self.store.events.get(r.cal).and_then(|l| l.get(r.ev)).cloned() else {
            self.peek = None;
            return;
        };
        let t = self.t.clone();
        let cal = self.store.cals[r.cal].clone();
        let col = color(ev.color.as_deref().unwrap_or(&cal.color));
        let w = 340.0;
        let inner = w - 40.0;
        let title_h = c.text_height(&ev.title, inner - 30.0, Font::Large).min(48.0);
        let master_rule = if ev.recurrence_id.is_some() {
            self.store.events[r.cal].iter().find(|e| e.uid == ev.uid && e.recurrence_id.is_none()).and_then(|m| m.rrule.clone())
        } else {
            ev.rrule.clone()
        };
        let mut lines: Vec<(&Icon, String)> = vec![(&CLOCK, self.when_text(&r))];
        if let Some(rule) = &master_rule {
            lines.push((&REPEAT, editor::describe(rule, tz::date_at(&self.zone(), r.start))));
        }
        if !ev.location.is_empty() {
            lines.push((&PLACE, ev.location.clone()));
        }
        if let Some(a) = ev.alarms.first() {
            lines.push((&BELL, i18n::reminder(*a)));
        }
        let notes_h = if ev.notes.is_empty() { 0.0 } else { c.text_height(&ev.notes, inner - 26.0, Font::Body).min(120.0) + 8.0 };
        let writable = !cal.read_only();
        let h = 20.0 + title_h + 12.0 + lines.len() as f32 * 26.0 + 28.0 + notes_h + if writable { 56.0 } else { 16.0 };
        let mut x = if at.right() + 10.0 + w < cx.w - 10.0 { at.right() + 10.0 } else { at.x - 10.0 - w };
        if x < SIDEBAR_W {
            x = (at.x + at.w / 2.0 - w / 2.0).clamp(SIDEBAR_W + 8.0, (cx.w - w - 10.0).max(SIDEBAR_W + 8.0));
        }
        let y = at.y.clamp(m::TITLE_H + 8.0, (cx.h - h - 10.0).max(m::TITLE_H + 8.0));
        let card = rect(x, y, w, h);
        self.card(c, card);
        self.hits.add(card, Id::Scroll);
        c.fill_round(rect(card.x + 20.0, card.y + 24.0, 4.0, title_h - 6.0), 2.0, col);
        c.clip(rect(card.x + 32.0, card.y + 18.0, inner - 40.0, title_h));
        c.text_block(&ev.title, rect(card.x + 32.0, card.y + 18.0, inner - 40.0, title_h), Font::Large, t.t1);
        c.unclip();
        self.icon_btn(c, rect(card.right() - 44.0, card.y + 14.0, 28.0, 28.0), &icons::CLOSE, Id::PeekClose);
        let mut yy = card.y + 20.0 + title_h + 10.0;
        for (ic, text) in lines {
            c.icon(ic, rect(card.x + 20.0, yy + 5.0, 14.0, 14.0), t.t3);
            c.text(&text, rect(card.x + 44.0, yy, inner - 24.0, 24.0), Font::Body, t.t2, Align::Left);
            yy += 26.0;
        }
        c.fill_round(rect(card.x + 23.0, yy + 8.0, 9.0, 9.0), 4.5, color(&cal.color));
        let cal_line = if cal.read_only() { format!("{} · {}", cal.name, tr("Read-only")) } else { cal.name.clone() };
        c.text(&cal_line, rect(card.x + 44.0, yy, inner - 24.0, 24.0), Font::Body, t.t3, Align::Left);
        yy += 28.0;
        if !ev.notes.is_empty() {
            c.icon(&NOTE, rect(card.x + 20.0, yy + 4.0, 14.0, 14.0), t.t3);
            c.clip(rect(card.x + 44.0, yy, inner - 26.0, notes_h));
            c.text_block(&ev.notes, rect(card.x + 44.0, yy + 2.0, inner - 26.0, notes_h), Font::Body, t.t2);
            c.unclip();
        }
        if writable {
            let by = card.bottom() - 50.0;
            self.btn(c, rect(card.right() - 20.0 - 96.0, by, 96.0, 34.0), tr("Edit"), 1, Id::PeekEdit);
            self.btn(c, rect(card.right() - 20.0 - 96.0 - 8.0 - 100.0, by, 100.0, 34.0), tr("Delete"), 0, Id::PeekDelete);
        }
    }

    fn paint_menu(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(menu) = self.menu.as_ref() else { return };
        let t = self.t.clone();
        let labels: Vec<String> = menu.items.iter().map(|(l, _)| l.clone()).collect();
        let colors = menu.colors_for;
        let at = menu.at;
        let w = 236.0;
        let h = 12.0 + labels.len() as f32 * 34.0 + if colors.is_some() { 74.0 } else { 0.0 };
        let x = at.x.min(cx.w - w - 8.0);
        let y = at.y.min(cx.h - h - 8.0).max(m::TITLE_H + 4.0);
        let card = rect(x, y, w, h);
        self.card(c, card);
        self.hits.add(card, Id::Scroll);
        let mut yy = y + 6.0;
        if let Some(i) = colors {
            let current = self.store.cals.get(i).map(|c| c.color.clone()).unwrap_or_default();
            for (k, p) in PALETTE.iter().enumerate() {
                let r = rect(x + 16.0 + (k % 5) as f32 * 42.0, yy + 6.0 + (k / 5) as f32 * 30.0, 24.0, 24.0);
                c.fill_round(r, 12.0, color(p));
                if *p == current {
                    c.stroke_round(r.inset(-3.0), 15.0, t.t1, 2.0);
                }
                self.hits.add(r, Id::MenuColor(k));
            }
            yy += 68.0;
            c.hline(x + 10.0, yy, w - 20.0, t.sep);
            yy += 6.0;
        }
        for (k, label) in labels.iter().enumerate() {
            let r = rect(x + 6.0, yy, w - 12.0, 32.0);
            let id = Id::MenuItem(k);
            if self.hot(&id) {
                c.fill_round(r, 8.0, t.hover);
            }
            let red = label == tr("Remove");
            c.text(label, r.inset_xy(12.0, 0.0), Font::Body, if red { t.red } else { t.t1 }, Align::Left);
            self.hits.add(r, id);
            yy += 34.0;
        }
    }

    fn paint_dialog(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(d) = self.dialog.as_ref() else { return };
        let t = self.t.clone();
        let (kind, text, error) = (d.kind, d.text.clone(), d.error.clone());
        let (title, desc, ok) = match kind {
            DlgKind::NewCalendar => (tr("New calendar"), None, tr("Add")),
            DlgKind::Subscribe => (
                tr("Subscribe to a calendar"),
                Some(tr("Paste the address of a calendar (.ics or webcal://): school, sports, public holidays… It stays read-only and refreshes every hour.")),
                tr("Subscribe"),
            ),
            DlgKind::Rename(_) => (tr("Rename"), None, tr("Save")),
        };
        let w = (cx.w - 40.0).min(480.0);
        let desc_h = desc.map(|s| c.text_height(s, w - 48.0, Font::Body) + 10.0).unwrap_or(0.0);
        let h = 172.0 + desc_h + if error.is_some() { 22.0 } else { 0.0 };
        self.paint_scrim(c, cx, false);
        let card = rect((cx.w - w) / 2.0, ((cx.h - h) / 2.0).max(m::TITLE_H + 12.0), w, h);
        self.card(c, card);
        let x = card.x + 24.0;
        let mut y = card.y + 20.0;
        c.text(title, rect(x, y, w - 48.0, 26.0), Font::Large, t.t1, Align::Left);
        y += 36.0;
        if let Some(s) = desc {
            c.text_block(s, rect(x, y, w - 48.0, desc_h), Font::Body, t.t2);
            y += desc_h;
        }
        let ph = if kind == DlgKind::Subscribe { "https://… .ics" } else { tr("Calendar name") };
        self.field_box(c, rect(x, y, w - 48.0, 36.0), Field::Dialog, &text, ph, Font::Body);
        y += 44.0;
        if let Some(e) = error {
            c.text(&e, rect(x, y, w - 48.0, 18.0), Font::Small, t.red, Align::Left);
        }
        let by = card.bottom() - 54.0;
        self.btn(c, rect(card.right() - 24.0 - 120.0, by, 120.0, 36.0), ok, 1, Id::DlgOk);
        self.btn(c, rect(card.right() - 24.0 - 250.0, by, 120.0, 36.0), tr("Cancel"), 0, Id::DlgCancel);
    }

    fn paint_scope(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(s) = self.scope.as_ref() else { return };
        let t = self.t.clone();
        let delete = s.delete;
        let text = if delete {
            tr("Delete only this event, or all the events of the series?")
        } else {
            tr("Change only this event, or all the events of the series?")
        };
        let w = (cx.w - 40.0).min(440.0);
        let th = c.text_height(text, w - 48.0, Font::Body);
        let h = 140.0 + th;
        self.paint_scrim(c, cx, false);
        let card = rect((cx.w - w) / 2.0, (cx.h - h) / 2.0, w, h);
        self.card(c, card);
        c.text(tr("Repeating event"), rect(card.x + 24.0, card.y + 20.0, w - 48.0, 26.0), Font::Large, t.t1, Align::Left);
        c.text_block(text, rect(card.x + 24.0, card.y + 56.0, w - 48.0, th), Font::Body, t.t2);
        let by = card.bottom() - 54.0;
        let bw = (w - 48.0 - 16.0) / 3.0;
        self.btn(c, rect(card.x + 24.0, by, bw, 36.0), tr("Cancel"), 3, Id::ScopeCancel);
        self.btn(c, rect(card.x + 24.0 + bw + 8.0, by, bw, 36.0), tr("This event"), 0, Id::ScopeThis);
        self.btn(c, rect(card.x + 24.0 + 2.0 * (bw + 8.0), by, bw, 36.0), tr("All events"), if delete { 2 } else { 1 }, Id::ScopeAll);
    }

    fn paint_confirm(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(k) = self.confirm.as_ref() else { return };
        let t = self.t.clone();
        let text = k.text.clone();
        let label = match k.act {
            ConfirmAct::RemoveCalendar(i) if self.store.cals.get(i).is_some_and(|c| c.read_only()) => tr("Remove"),
            ConfirmAct::RemoveCalendar(_) => tr("Delete"),
        };
        let w = (cx.w - 40.0).min(420.0);
        let th = c.text_height(&text, w - 48.0, Font::Body);
        let h = 110.0 + th;
        self.paint_scrim(c, cx, false);
        let card = rect((cx.w - w) / 2.0, (cx.h - h) / 2.0, w, h);
        self.card(c, card);
        c.text_block(&text, rect(card.x + 24.0, card.y + 24.0, w - 48.0, th), Font::Body, t.t1);
        let by = card.bottom() - 54.0;
        self.btn(c, rect(card.right() - 24.0 - 120.0, by, 120.0, 36.0), label, 2, Id::ConfirmOk);
        self.btn(c, rect(card.right() - 24.0 - 250.0, by, 120.0, 36.0), tr("Cancel"), 0, Id::ConfirmCancel);
    }

    fn paint_toast(&mut self, c: &mut Canvas, cx: &PaintCx) {
        let Some(toast) = self.toast.as_ref() else { return };
        let t = self.t.clone();
        let (text, undo) = (toast.text.clone(), toast.undo && self.undo.is_some());
        let uw = if undo { c.text_width(tr("Undo"), Font::BodyBold) + 30.0 } else { 0.0 };
        let tw = (c.text_width(&text, Font::Body) + 40.0 + uw).min(cx.w - SIDEBAR_W - 40.0);
        let r = rect(SIDEBAR_W + (cx.w - SIDEBAR_W - tw) / 2.0, cx.h - 64.0, tw, 40.0);
        c.fill_round(r.offset(0.0, 3.0), 20.0, Color::hexa(0, 0.25));
        c.fill_round(r, 20.0, t.solid2);
        c.stroke_round(r, 20.0, t.ring, 1.0);
        c.text(&text, rect(r.x + 18.0, r.y, r.w - 36.0 - uw, r.h), Font::Body, t.t1, Align::Left);
        if undo {
            let b = rect(r.right() - uw - 6.0, r.y + 5.0, uw, 30.0);
            self.btn(c, b, tr("Undo"), 3, Id::Undo);
        }
    }

    /// Reminders shown in the window (top right).
    fn paint_alerts(&mut self, c: &mut Canvas, cx: &PaintCx) {
        if self.alerts.is_empty() {
            return;
        }
        let t = self.t.clone();
        let w = 330.0;
        let n = self.alerts.len().min(4) as f32;
        let mut y = cx.h - 16.0 - n * 124.0 + 12.0 - if self.toast.is_some() { 56.0 } else { 0.0 };
        let now = self.now();
        let h24 = self.h24();
        for (i, d) in self.alerts.clone().iter().enumerate().take(4) {
            let card = rect(cx.w - w - 16.0, y, w, 112.0);
            self.card(c, card);
            self.hits.add(card, Id::AlertOpen(i));
            c.fill_round(rect(card.x + 16.0, card.y + 18.0, 4.0, 40.0), 2.0, color(&d.color));
            c.icon(&BELL, rect(card.right() - 34.0, card.y + 18.0, 16.0, 16.0), t.accent);
            c.text(&d.title, rect(card.x + 30.0, card.y + 14.0, w - 74.0, 22.0), Font::Title, t.t1, Align::Left);
            let mut sub = popup::when(d, now, h24);
            if !d.location.is_empty() {
                sub = format!("{sub} · {}", d.location);
            }
            c.text(&sub, rect(card.x + 30.0, card.y + 38.0, w - 50.0, 20.0), Font::Body, t.t2, Align::Left);
            let by = card.bottom() - 44.0;
            let sw = c.text_width(tr("Snooze 5 min"), Font::BodyBold) + 28.0;
            self.btn(c, rect(card.x + 16.0, by, sw, 32.0), tr("Snooze 5 min"), 0, Id::AlertSnooze(i));
            self.btn(c, rect(card.right() - 16.0 - 100.0, by, 100.0, 32.0), tr("Dismiss"), 1, Id::AlertDismiss(i));
            y += 124.0;
        }
    }

    /// While dragging a new event: its outline.
    pub(super) fn drag_range(&self) -> Option<(i64, i64)> {
        match self.drag {
            Drag::Create { a, b } if a != b => Some(if a < b { (a, b) } else { (b, a) }),
            _ => None,
        }
    }

    /// Days covered while creating or moving in the month grid.
    pub(super) fn drag_days(&self) -> Option<(Date, Date)> {
        match &self.drag {
            Drag::CreateDays { a, b } if a != b => Some((Date::from_days(*a.min(b)), Date::from_days(*a.max(b)))),
            Drag::MoveDays { occ, day, .. } => {
                let o = self.occs.get(*occ)?;
                let z = self.zone();
                let len = tz::date_at(&z, (o.end - 1).max(o.start)).days() - tz::date_at(&z, o.start).days();
                Some((Date::from_days(*day), Date::from_days(*day + len)))
            }
            _ => None,
        }
    }
}
