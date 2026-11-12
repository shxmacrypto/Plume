//! The frame of the window: sidebar, caption buttons, menus, dialogs, messages, and the small widgets the pages
//! share (buttons, pills, the scrollbar).

use super::{
    App, Hit, Modal, Page, SIDEBAR_W,
    draw::{self, ic},
};
use crate::{
    i18n::{fill, tr},
    model,
    sys::Why,
};
use plume_ui::{Align, Canvas, Color, EditStyle, Font, PaintCx, Rect, icons, icons::Icon, m, rect};

impl App {
    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    pub fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, color: Color) {
        let hot = self.hovered(&hit);
        if hot {
            c.fill_round(r, r.h / 2.0, self.pal.hover);
        }
        let s = (r.w * 0.5).round();
        c.icon(icon, rect(r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0, s, s), if hot { self.pal.t1 } else { color });
        self.hits.add(r, hit);
    }

    /// A round button: `tone` None quiet, Some(color) filled with that color. Disabled: drawn faint, not clickable.
    #[allow(clippy::too_many_arguments)]
    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, icon: Option<&Icon>, tone: Option<Color>, enabled: bool, hit: Hit) {
        let p = self.pal.clone();
        let hot = enabled && self.hovered(&hit);
        let down = hot && self.pressed.as_ref() == Some(&hit);
        let (fill, ink) = match tone {
            Some(t) if enabled => (
                if down {
                    t.mix(Color::hex(0), 0.18)
                } else if hot {
                    t.mix(Color::hex(0xffffff), 0.1)
                } else {
                    t
                },
                Color::hex(0xffffff),
            ),
            _ if enabled => (
                if down {
                    p.fill2.mix(p.t1, 0.06)
                } else if hot {
                    p.fill2
                } else {
                    p.fill
                },
                p.t1,
            ),
            _ => (p.fill, p.t3),
        };
        c.fill_round(r, r.h / 2.0, fill);
        match icon {
            Some(ic) => {
                let tw = c.text_width(label, Font::BodyBold);
                let x = r.x + (r.w - tw - 22.0) / 2.0;
                c.icon(ic, rect(x, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), ink);
                c.text(label, rect(x + 22.0, r.y, tw + 2.0, r.h), Font::BodyBold, ink, Align::Left);
            }
            None => c.text(label, r, Font::BodyBold, ink, Align::Center),
        }
        if enabled {
            self.hits.add(r, hit);
        }
    }

    /// Width a pill needs for its label.
    pub fn pill_w(&self, c: &mut Canvas, label: &str, icon: bool) -> f32 {
        c.text_width(label, Font::BodyBold) + if icon { 54.0 } else { 32.0 }
    }

    /// The vertical scrollbar of a scrolled area (`area`: where the content shows).
    pub fn scrollbar(&mut self, c: &mut Canvas, area: Rect) {
        let max = (self.content_h - area.h).max(0.0);
        if max <= 0.5 {
            return;
        }
        let th = (area.h * area.h / self.content_h).max(36.0);
        let ty = area.y + (area.h - th) * (self.scroll() / max);
        let hot = self.hovered(&Hit::Scrollbar) || self.drag_scroll.is_some();
        c.fill_round(rect(area.right() - 8.0, ty, 5.0, th), 2.5, self.pal.t3.fade(if hot { 0.7 } else { 0.45 }));
        self.hits.add(rect(area.right() - 12.0, area.y, 12.0, area.h), Hit::Scrollbar);
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let p = self.pal.clone();
        self.hits.clear();
        c.clear(p.bg);
        let (w, h) = (cx.w, cx.h);
        let page = rect(SIDEBAR_W, 0.0, w - SIDEBAR_W, h);
        self.view_h = h;
        match self.page {
            Page::Apps => self.paint_apps(c, page),
            Page::Perf => self.paint_perf(c, page),
            Page::Startup => self.paint_startup(c, page),
            Page::Settings => self.paint_settings(c, page),
        }
        self.paint_sidebar(c, rect(0.0, 0.0, SIDEBAR_W, h), cx.active);
        self.paint_caption(c, w, cx);
        self.paint_menu(c, w, h);
        self.paint_modal(c, w, h);
        self.paint_toast(c, w, h);
        self.place_edit();
    }

    /// The single text field of the window: the search box of the Apps page.
    fn place_edit(&mut self) {
        let p = self.pal.clone();
        let want = match (self.edit_for, self.search_rect) {
            (Some("search"), Some(r)) if self.page == super::Page::Apps && self.modal.is_none() => Some(r),
            _ => None,
        };
        match want {
            Some(r) => {
                let inner = rect(r.x + 36.0, r.y + 5.0, r.w - 66.0, r.h - 10.0);
                if self.edit_shown != self.edit_for {
                    let style = EditStyle {
                        fg: p.t1,
                        bg: p.fill2.over(p.bg),
                        size: 13.0,
                        select: Some((0, self.query.encode_utf16().count())),
                        placeholder: tr("Search a program, a process or a PID").to_string(),
                        password: false,
                        multiline: false,
                    };
                    self.win.edit_show(inner, &self.query, &style);
                    self.win.edit_focus();
                    self.edit_shown = self.edit_for;
                } else {
                    self.win.edit_place(inner);
                }
            }
            None => {
                if self.edit_shown.is_some() {
                    self.win.edit_hide();
                    self.edit_shown = None;
                }
            }
        }
    }

    fn paint_caption(&mut self, c: &mut Canvas, w: f32, cx: &PaintCx) {
        let p = self.pal.clone();
        let x0 = w - 3.0 * m::CAPTION_W;
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(x0 + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
            let hot = self.hovered(&id) || self.pressed.as_ref() == Some(&id);
            if hot {
                c.fill(r, if id == Hit::Close { p.close_hover } else { p.hover });
            }
            let ink = match (id == Hit::Close && hot, cx.active) {
                (true, _) => Color::hex(0xffffff),
                (false, true) => p.t1,
                (false, false) => p.t3,
            };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    fn paint_sidebar(&mut self, c: &mut Canvas, r: Rect, active: bool) {
        let p = self.pal.clone();
        c.fill(r, p.side);
        c.vline(r.right(), r.y, r.h, p.line);
        // the title bar: the logo and the name, like every Plume window
        let logo = rect(r.x + 14.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0);
        let wic = c.gfx.wic.clone();
        c.image(7, logo, 1.0, || draw::png(&wic, crate::LOGO));
        c.text("Plume Task", rect(r.x + 40.0, 0.0, r.w - 52.0, m::TITLE_H), Font::BodyBold, if active { p.t1 } else { p.t3 }, Align::Left);
        let mut y = m::TITLE_H + 16.0;
        let navs: [(&Icon, &str, Page); 3] =
            [(&ic::APPS, tr("Apps"), Page::Apps), (&ic::PULSE, tr("Performance"), Page::Perf), (&ic::ROCKET, tr("Startup apps"), Page::Startup)];
        for (icon, label, pg) in navs {
            self.nav_row(c, rect(r.x + 12.0, y, r.w - 24.0, 40.0), icon, label, pg);
            y += 44.0;
        }
        // live numbers, always in sight
        let s = self.sys.clone();
        y += 12.0;
        c.hline(r.x + 24.0, y, r.w - 48.0, p.line);
        y += 16.0;
        let mem_line = fill(tr("{} of {}"), &[&model::size(s.mem_used), &model::size(s.mem_total)]);
        let procs_line = fill(tr("{} processes"), &[&s.procs.to_string()]);
        let gauges: [(&str, f32, String); 2] = [(tr("Processor"), s.cpu, procs_line), (tr("Memory"), s.mem_pct(), mem_line)];
        for (label, v, value) in gauges {
            c.text(label, rect(r.x + 24.0, y, 120.0, 20.0), Font::Small, p.t2, Align::Left);
            c.text(&model::percent(v), rect(r.right() - 104.0, y, 80.0, 20.0), Font::SmallBold, p.t1, Align::Right);
            let bar = rect(r.x + 24.0, y + 24.0, r.w - 48.0, 6.0);
            c.fill_round(bar, 3.0, p.fill);
            let col = level_color(&p, v);
            c.fill_round(rect(bar.x, bar.y, bar.w * (v / 100.0).clamp(0.0, 1.0), bar.h), 3.0, col);
            c.text(&value, rect(r.x + 24.0, y + 32.0, r.w - 48.0, 20.0), Font::Small, p.t3, Align::Left);
            y += 64.0;
        }
        // settings at the bottom
        self.nav_row(c, rect(r.x + 12.0, r.bottom() - 56.0, r.w - 24.0, 40.0), &icons::GEAR, tr("Settings"), Page::Settings);
    }

    fn nav_row(&mut self, c: &mut Canvas, row: Rect, icon: &Icon, label: &str, pg: Page) {
        let p = self.pal.clone();
        let active = self.page == pg;
        let hit = Hit::Nav(pg);
        if active {
            c.fill_round(row, 10.0, p.accent.fade(0.16));
        } else if self.hovered(&hit) {
            c.fill_round(row, 10.0, p.hover);
        }
        c.icon(icon, rect(row.x + 12.0, row.y + 11.0, 18.0, 18.0), if active { p.accent } else { p.t2 });
        c.text(
            label,
            rect(row.x + 40.0, row.y, row.w - 48.0, row.h),
            if active { Font::BodyBold } else { Font::Body },
            if active { p.accent } else { p.t1 },
            Align::Left,
        );
        self.hits.add(row, hit);
    }

    /// The title of a page, at the top left.
    pub fn page_title(&mut self, c: &mut Canvas, page: Rect, title: &str, sub: &str) -> f32 {
        let p = self.pal.clone();
        c.text(title, rect(page.x + 24.0, m::TITLE_H, page.w - 200.0, 36.0), Font::Hero, p.t1, Align::Left);
        if !sub.is_empty() {
            c.text(sub, rect(page.x + 24.0, m::TITLE_H + 38.0, page.w - 48.0, 20.0), Font::Body, p.t3, Align::Left);
            return m::TITLE_H + 74.0;
        }
        m::TITLE_H + 52.0
    }

    fn paint_menu(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(menu) = &self.menu else { return };
        let p = self.pal.clone();
        let labels: Vec<String> = menu.items.iter().map(|i| i.0.clone()).collect();
        let (mx, my) = (menu.x, menu.y);
        let mw = labels.iter().map(|l| c.text_width(l, Font::Body)).fold(170.0f32, f32::max) + 40.0;
        let mh = labels.len() as f32 * 36.0 + 12.0;
        let x = mx.min(w - mw - 8.0).max(8.0);
        let y = if my + mh > h - 8.0 { (my - mh).max(8.0) } else { my };
        let b = rect(x, y, mw, mh);
        c.fill_round(b.offset(0.0, 6.0).inset(-2.0), 16.0, Color::hex(0).fade(0.22));
        c.fill_round(b, 14.0, p.solid);
        c.stroke_round(b, 14.0, p.ring, 1.0);
        for (i, label) in labels.iter().enumerate() {
            let row = rect(x + 6.0, y + 6.0 + i as f32 * 36.0, mw - 12.0, 36.0);
            let hit = Hit::MenuItem(i);
            if self.hovered(&hit) {
                c.fill_round(row, 10.0, p.hover);
            }
            let ink = if i == 0 { p.red } else { p.t1 };
            c.text(label, rect(row.x + 12.0, row.y, row.w - 20.0, row.h), Font::Body, ink, Align::Left);
            self.hits.add(row, hit);
        }
    }

    fn paint_modal(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(modal) = self.modal.clone() else { return };
        let p = self.pal.clone();
        c.fill(rect(0.0, 0.0, w, h), Color::hex(0).fade(0.5));
        // a dialog swallows clicks around it
        self.hits.add(rect(0.0, 0.0, w, h), Hit::Inert);
        let mw = (w - 80.0).min(560.0);
        let x = (w - mw) / 2.0;
        let inner = mw - 48.0;
        match modal {
            Modal::End { name, targets } => {
                let ok = targets.iter().filter(|t| t.ok.is_ok()).count();
                let blocked: Vec<Why> = targets.iter().filter_map(|t| t.ok.err()).collect();
                let why = blocked.iter().find(|w| **w != Why::Gone).or(blocked.first()).copied();
                let reason = why.map(|w| {
                    tr(match w {
                        Why::Elevated => "It runs as administrator. To end it, open Plume Task as administrator.",
                        Why::System => "It is part of Windows, which protects it.",
                        Why::Service => "It is a Windows service. Services are stopped from Windows' Services app.",
                        Why::Gone => "It has already closed.",
                    })
                });
                let (title, body) = if ok == 0 {
                    (fill(tr("Plume Task can't end {}"), &[&name]), reason.unwrap_or_default().to_string())
                } else if blocked.is_empty() {
                    let body = if targets.len() > 1 {
                        fill(tr("Its {} processes will close. Anything not saved in it will be lost."), &[&targets.len().to_string()])
                    } else {
                        tr("It will close. Anything not saved in it will be lost.").to_string()
                    };
                    (fill(tr("End {}?"), &[&name]), body)
                } else {
                    let body = fill(
                        tr("{} of its {} processes will close; the others can't be ended. {}"),
                        &[&ok.to_string(), &targets.len().to_string(), reason.unwrap_or("")],
                    );
                    (fill(tr("End {}?"), &[&name]), body)
                };
                let bh = c.text_height(&body, inner, Font::Body);
                let mh = 28.0 + 30.0 + 12.0 + bh + 28.0 + 36.0 + 24.0;
                let y = ((h - mh) / 2.0).max(20.0);
                let b = rect(x, y, mw, mh);
                self.dialog_box(c, b);
                let icon = if ok == 0 { &ic::WARN } else { &ic::END };
                c.icon(icon, rect(x + 24.0, y + 32.0, 22.0, 22.0), if ok == 0 { p.orange } else { p.red });
                c.text(&title, rect(x + 54.0, y + 28.0, inner - 28.0, 30.0), Font::Large, p.t1, Align::Left);
                c.text_block(&body, rect(x + 24.0, y + 70.0, inner, bh), Font::Body, p.t2);
                let by = b.bottom() - 24.0 - 36.0;
                if ok == 0 {
                    let cw = self.pill_w(c, tr("Close"), false);
                    self.pill(c, rect(b.right() - 24.0 - cw, by, cw, 36.0), tr("Close"), None, Some(p.accent), true, Hit::ModalCancel);
                } else {
                    let ew = self.pill_w(c, tr("End task"), false);
                    let cw = self.pill_w(c, tr("Cancel"), false);
                    self.pill(c, rect(b.right() - 24.0 - ew, by, ew, 36.0), tr("End task"), None, Some(p.red), true, Hit::ModalOk);
                    self.pill(c, rect(b.right() - 24.0 - ew - 10.0 - cw, by, cw, 36.0), tr("Cancel"), None, None, true, Hit::ModalCancel);
                }
            }
            Modal::Details { title, fields, path } => {
                let kw = 130.0;
                let vw = inner - kw;
                let heights: Vec<f32> = fields.iter().map(|(_, v)| c.text_height(v, vw, Font::Body).min(110.0)).collect();
                let body_h: f32 = heights.iter().map(|h| h + 10.0).sum();
                let mh = (28.0 + 34.0 + 14.0 + body_h + 24.0 + 36.0 + 24.0).min(h - 40.0);
                let y = ((h - mh) / 2.0).max(20.0);
                let b = rect(x, y, mw, mh);
                self.dialog_box(c, b);
                c.icon(&ic::INFO, rect(x + 24.0, y + 33.0, 22.0, 22.0), p.accent);
                c.text(&title, rect(x + 54.0, y + 28.0, inner - 28.0, 32.0), Font::Large, p.t1, Align::Left);
                let mut fy = y + 76.0;
                let bottom = b.bottom() - 24.0 - 36.0 - 16.0;
                c.clip(rect(x, fy, mw, (bottom - fy).max(0.0)));
                for ((k, v), vh) in fields.iter().zip(heights) {
                    if fy > bottom {
                        break;
                    }
                    c.text(k, rect(x + 24.0, fy, kw - 10.0, 20.0), Font::Small, p.t3, Align::Left);
                    c.clip(rect(x + 24.0 + kw, fy, vw, vh));
                    c.text_block(v, rect(x + 24.0 + kw, fy + 1.0, vw, vh), Font::Body, p.t1);
                    c.unclip();
                    fy += vh + 10.0;
                }
                c.unclip();
                let by = b.bottom() - 24.0 - 36.0;
                let cw = self.pill_w(c, tr("Close"), false);
                self.pill(c, rect(b.right() - 24.0 - cw, by, cw, 36.0), tr("Close"), None, Some(p.accent), true, Hit::ModalCancel);
                if path.is_some() {
                    let lw = self.pill_w(c, tr("Open file location"), true);
                    self.pill(
                        c,
                        rect(b.right() - 24.0 - cw - 10.0 - lw, by, lw, 36.0),
                        tr("Open file location"),
                        Some(&icons::FOLDER),
                        None,
                        true,
                        Hit::ModalAlt,
                    );
                }
            }
        }
    }

    fn dialog_box(&mut self, c: &mut Canvas, b: Rect) {
        let p = self.pal.clone();
        c.fill_round(b.offset(0.0, 8.0).inset(-2.0), 22.0, Color::hex(0).fade(0.14));
        c.fill_round(b, 20.0, p.solid);
        c.stroke_round(b, 20.0, p.ring, 1.0);
        self.hits.add(b, Hit::Inert);
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((t, _)) = self.toast.clone() else { return };
        let p = self.pal.clone();
        let tw = (c.text_width(&t, Font::Body) + 40.0).min(w - 48.0);
        let r = rect((w - tw) / 2.0, h - 64.0, tw, 40.0);
        c.fill_round(r.offset(0.0, 4.0), 20.0, Color::hex(0).fade(0.18));
        c.fill_round(r, 20.0, p.solid2);
        c.stroke_round(r, 20.0, p.ring, 1.0);
        c.text(&t, r.inset_xy(16.0, 0.0), Font::Body, p.t1, Align::Center);
        self.hits.add(r, Hit::Toast);
    }
}

impl App {
    /// An empty page or list: an icon, a title, one line, and (when there is something to do) one primary button.
    pub fn empty_state(&mut self, c: &mut Canvas, r: Rect, icon: &Icon, title: &str, line: &str, button: Option<(&str, Hit)>) {
        let p = self.pal.clone();
        let total = 44.0 + 12.0 + 24.0 + 6.0 + 20.0 + if button.is_some() { 20.0 + 36.0 } else { 0.0 };
        let mut y = r.y + ((r.h - total) / 2.0).clamp(16.0, 120.0);
        let badge = rect(r.x + (r.w - 44.0) / 2.0, y, 44.0, 44.0);
        c.fill_round(badge, 22.0, p.fill);
        c.icon(icon, badge.centered(20.0, 20.0), p.t2);
        y += 44.0 + 12.0;
        c.text(title, rect(r.x, y, r.w, 24.0), Font::Title, p.t1, Align::Center);
        y += 24.0 + 6.0;
        c.text(line, rect(r.x + 16.0, y, r.w - 32.0, 20.0), Font::Body, p.t3, Align::Center);
        y += 20.0 + 20.0;
        if let Some((label, hit)) = button {
            let bw = self.pill_w(c, label, false);
            self.pill(c, rect(r.x + (r.w - bw) / 2.0, y, bw, 36.0), label, None, Some(p.accent), true, hit);
        }
    }
}

/// Green, orange or red for a load between 0 and 100.
pub fn level_color(p: &super::Pal, v: f32) -> Color {
    if v >= 85.0 {
        p.red
    } else if v >= 60.0 {
        p.orange
    } else {
        p.accent
    }
}
