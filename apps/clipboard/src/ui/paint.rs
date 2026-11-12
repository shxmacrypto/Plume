//! Painting the main window: sidebar, title row, the history, the snippets and their editor.

use super::{
    App, Field, Hit, LIST_TOP, Page, SIDEBAR_W,
    draw::{self, ic},
    thumb_key,
};
use crate::{
    detect::Secret,
    i18n::{fill, tr},
    image,
    store::{self, Filter, Item, Kind, MASK},
};
use plume_ui::{Align, Canvas, Color, EditStyle, Font, PaintCx, Rect, Theme, icons, icons::Icon, m, rect};

pub const LOGO: &[u8] = include_bytes!("../../../../assets/brand/plume-clipboard-256.png");
const LOGO_KEY: u64 = 1 << 61;

/// The cards of the history and the snippets: radius, padding, gap.
const CARD_R: f32 = 16.0;
const PAD: f32 = 20.0;
const CARD_GAP: f32 = 14.0;

pub fn row_h(kind: Kind) -> f32 {
    match kind {
        Kind::Text => 86.0,
        Kind::Link => 70.0,
        Kind::Image => 116.0,
        Kind::Files => 78.0,
    }
}

/// A card: Plume's card colour, a hairline around it, lighter under the mouse.
fn card(c: &mut Canvas, r: Rect, t: &Theme, hot: bool) {
    c.fill_round(r, CARD_R, t.card);
    if hot {
        c.fill_round(r, CARD_R, t.hover);
    }
    c.stroke_round(r.inset(0.5), CARD_R, t.sep, 1.0);
}

pub fn kind_icon(kind: Kind) -> &'static Icon {
    match kind {
        Kind::Text => &ic::TEXT,
        Kind::Link => &ic::LINK,
        Kind::Image => &ic::IMAGE,
        Kind::Files => &ic::FILES,
    }
}

/// "12 KB", "3.4 MB".
pub fn size_label(bytes: u64) -> String {
    if bytes < 1024 * 1024 { format!("{} KB", bytes.div_ceil(1024)) } else { format!("{:.1} MB", bytes as f64 / 1048576.0) }
}

/// What a sensitive item is called.
pub fn secret_label(s: Secret) -> &'static str {
    match s {
        Secret::Password => tr("Password"),
        Secret::Ip => tr("IP address"),
        Secret::Key => tr("Key"),
    }
}

pub fn secret_icon(s: Secret) -> &'static Icon {
    match s {
        Secret::Password => &ic::LOCK,
        Secret::Ip => &ic::GLOBE,
        Secret::Key => &ic::KEY,
    }
}

/// The line under an item: where it comes from, when, and how big it is. A sensitive item tells instead when
/// it goes (`secret_secs`: how long such items are kept), never its length.
pub fn meta(it: &Item, now: u64, secret_secs: u64) -> String {
    let mut parts = Vec::new();
    if !it.app.is_empty() {
        parts.push(it.app.trim_end_matches(".exe").to_string());
    }
    parts.push(draw::ago(it.at, now));
    if it.secret.is_some() {
        if it.pin.is_none() {
            let left = (it.at + secret_secs).saturating_sub(now);
            parts.push(fill(tr("hidden, removed in {n} min"), &[&left.div_ceil(60).max(1).to_string()]));
        } else {
            parts.push(tr("hidden").to_string());
        }
        return parts.join("  ·  ");
    }
    match it.kind {
        Kind::Text => parts.push(fill(tr("{n} characters"), &[&it.text.chars().count().to_string()])),
        Kind::Image => parts.push(size_label(it.bytes)),
        Kind::Files if it.files.len() > 1 => parts.push(fill(tr("{n} files"), &[&it.files.len().to_string()])),
        _ => {}
    }
    if it.count > 1 {
        parts.push(fill(tr("copied {n} times"), &[&it.count.to_string()]));
    }
    parts.join("  ·  ")
}

impl App {
    pub fn theme(&self) -> Theme {
        self.model.borrow().theme.clone()
    }

    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    pub fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, color: Color) {
        let t = self.theme();
        let hot = self.hovered(&hit);
        if hot {
            let down = self.pressed.as_ref() == Some(&hit);
            c.fill_round(r, r.h / 2.0, if down { t.fill2 } else { t.hover });
        }
        let s = 16.0;
        c.icon(icon, r.centered(s, s), if hot { t.t1 } else { color });
        self.hits.add(r, hit);
    }

    /// A round button: primary (accent), secondary (fill), or red.
    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, kind: u8, hit: Hit) {
        let t = self.theme();
        let hot = self.hovered(&hit);
        let down = hot && self.pressed.as_ref() == Some(&hit);
        let (fill, ink) = match kind {
            1 if down => (t.accent.mix(Color::hex(0x000000), 0.18), Color::hex(0xffffff)),
            1 => (if hot { t.accent.mix(Color::hex(0xffffff), 0.1) } else { t.accent }, Color::hex(0xffffff)),
            2 => (
                t.red.fade(if down {
                    0.32
                } else if hot {
                    0.24
                } else {
                    0.16
                }),
                t.red,
            ),
            _ if down => (t.fill2.over(t.bg).mix(t.t1, 0.06), t.t1),
            _ => (if hot { t.fill2 } else { t.fill }, t.t1),
        };
        c.fill_round(r, r.h / 2.0, fill);
        c.text(label, r, Font::BodyBold, ink, Align::Center);
        self.hits.add(r, hit);
    }

    pub fn pill_w(c: &mut Canvas, label: &str) -> f32 {
        c.text_width(label, Font::BodyBold) + 34.0
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        self.refresh_rows();
        let t = self.theme();
        self.hits.clear();
        self.field_rects.clear();
        c.clear(t.bg);
        let (w, h) = (cx.w, cx.h);
        let content = rect(SIDEBAR_W, 0.0, w - SIDEBAR_W, h);
        self.view_h = h - LIST_TOP;
        match self.page {
            Page::History(_) => self.paint_history(c, content),
            Page::Snippets => self.paint_snippets(c, content),
            Page::Settings => self.paint_settings(c, content),
        }
        self.paint_header(c, content);
        self.paint_sidebar(c, rect(0.0, 0.0, SIDEBAR_W, h), cx.active);
        self.paint_caption(c, w, cx);
        self.paint_dialog(c, w, h);
        self.paint_toast(c, w, h);
        self.place_edit();
    }

    /// The window's one text field, on the field being typed in.
    fn place_edit(&mut self) {
        let t = self.theme();
        let want = self.edit_for.and_then(|f| self.field_rects.iter().find(|(g, _)| *g == f).map(|(_, r)| (f, *r)));
        match want {
            Some((f, r)) => {
                let inner =
                    if f == Field::Search { rect(r.x + 36.0, r.y + 7.0, r.w - 46.0, r.h - 14.0) } else { rect(r.x + 12.0, r.y + 8.0, r.w - 24.0, r.h - 16.0) };
                if self.edit_shown != Some(f) {
                    let d = self.draft.clone().unwrap_or_default();
                    let text = match f {
                        Field::Search => self.query.clone(),
                        Field::Name => d.name,
                        Field::Trigger => d.trigger,
                        Field::Body => d.text,
                        Field::Ignored => self.ignored_text.clone(),
                    };
                    let style = EditStyle {
                        fg: t.t1,
                        bg: t.fill.over(t.bg),
                        size: 14.0,
                        select: None,
                        placeholder: match f {
                            Field::Search => tr("Search the history").to_string(),
                            Field::Trigger => ";sig".to_string(),
                            Field::Ignored => "app.exe".to_string(),
                            _ => String::new(),
                        },
                        password: false,
                        multiline: f == Field::Body,
                    };
                    self.win.edit_show(inner, &text, &style);
                    self.win.edit_focus();
                    self.edit_shown = Some(f);
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
        let t = self.theme();
        let cap_x = w - 3.0 * m::CAPTION_W;
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
            let hot = self.hovered(&id) || self.pressed.as_ref() == Some(&id);
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

    fn paint_sidebar(&mut self, c: &mut Canvas, r: Rect, active: bool) {
        let t = self.theme();
        c.fill(r, t.bg2);
        c.vline(r.right() - 0.5, 0.0, r.h, t.sep);
        // the title bar's part: the logo and the name
        c.image(LOGO_KEY, rect(16.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0), 1.0, || image::drawable(LOGO));
        c.text("Plume Clipboard", rect(42.0, 0.0, r.w - 50.0, m::TITLE_H), Font::BodyBold, if active { t.t1 } else { t.t3 }, Align::Left);
        let counts = {
            let m = self.model.borrow();
            [Filter::All, Filter::Text, Filter::Links, Filter::Images, Filter::Files, Filter::Pinned].map(|f| m.store.count(f))
        };
        let snippets = self.model.borrow().store.snippets.len();
        let nav: [(Page, &Icon, &str, Option<usize>); 8] = [
            (Page::History(Filter::All), &ic::ALL, tr("All"), Some(counts[0])),
            (Page::History(Filter::Text), &ic::TEXT, tr("Text"), Some(counts[1])),
            (Page::History(Filter::Links), &ic::LINK, tr("Links"), Some(counts[2])),
            (Page::History(Filter::Images), &ic::IMAGE, tr("Pictures"), Some(counts[3])),
            (Page::History(Filter::Files), &ic::FILES, tr("Files"), Some(counts[4])),
            (Page::History(Filter::Pinned), &icons::PIN, tr("Pinned"), Some(counts[5])),
            (Page::Snippets, &ic::SNIPPET, tr("Snippets"), Some(snippets)),
            (Page::Settings, &icons::GEAR, tr("Settings"), None),
        ];
        let mut y = m::TITLE_H + 12.0;
        for (i, (page, icon, label, n)) in nav.into_iter().enumerate() {
            if i == 6 {
                c.hline(24.0, y + 6.0, r.w - 48.0, t.sep);
                y += 12.0;
            }
            let row = rect(12.0, y, r.w - 24.0, 40.0);
            let hit = Hit::Nav(page);
            let on = self.page == page;
            if on {
                c.fill_round(row, 10.0, draw::selection(&t));
            } else if self.hovered(&hit) {
                c.fill_round(row, 10.0, t.hover);
            }
            c.icon(icon, rect(row.x + 12.0, row.y + 12.0, 16.0, 16.0), if on { draw::accent_ink(&t) } else { t.t2 });
            c.text(label, rect(row.x + 40.0, row.y, row.w - 80.0, row.h), if on { Font::BodyBold } else { Font::Body }, t.t1, Align::Left);
            if let Some(n) = n.filter(|n| *n > 0) {
                c.text(&n.to_string(), rect(row.right() - 52.0, row.y, 40.0, row.h), Font::Small, t.t3, Align::Right);
            }
            self.hits.add(row, hit);
            y += 42.0;
        }
        // the shortcut, and what is going on
        let (label, ok, paused) = {
            let m = self.model.borrow();
            (m.hotkey.map(|h| h.label()).unwrap_or_else(|| m.set.hotkey.clone()), m.hotkey_ok || crate::hotkey::off_for_test(), m.set.paused)
        };
        let card = rect(16.0, r.h - 108.0, r.w - 32.0, 92.0);
        let hit = Hit::OpenPanel;
        c.fill_round(card, 16.0, t.card);
        if self.hovered(&hit) {
            c.fill_round(card, 16.0, t.hover);
        }
        c.stroke_round(card.inset(0.5), 16.0, t.sep, 1.0);
        let icon_r = rect(card.x + 18.0, card.y + 20.0, 16.0, 16.0);
        if paused {
            c.icon_fill(&ic::PAUSE, icon_r, t.orange);
        } else {
            c.icon(&ic::KEYBOARD, icon_r, draw::accent_ink(&t));
        }
        c.text(&label, rect(card.x + 44.0, card.y + 14.0, card.w - 56.0, 28.0), Font::BodyBold, t.t1, Align::Left);
        let (line, color) = if paused {
            (tr("Paused: nothing new is kept"), t.orange)
        } else if !ok {
            (tr("Shortcut unavailable: pick another one in Settings"), t.red)
        } else {
            (tr("Opens the panel in any app"), t.t2)
        };
        draw::text_box(c, line, rect(card.x + 18.0, card.y + 46.0, card.w - 32.0, 36.0), 12.0, 400, color);
        self.hits.add(card, hit);
    }

    fn paint_header(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.theme();
        c.fill(rect(r.x, 0.0, r.w, LIST_TOP - 6.0), t.bg);
        let title = match self.page {
            Page::History(Filter::All) => tr("History"),
            Page::History(Filter::Text) => tr("Text"),
            Page::History(Filter::Links) => tr("Links"),
            Page::History(Filter::Images) => tr("Pictures"),
            Page::History(Filter::Files) => tr("Files"),
            Page::History(Filter::Pinned) => tr("Pinned"),
            Page::Snippets => tr("Snippets"),
            Page::Settings => tr("Settings"),
        };
        let x = r.x + 24.0;
        c.text(title, rect(x, 42.0, 340.0, 36.0), Font::Hero, t.t1, Align::Left);
        let n = self.rows.len().to_string();
        let sub = match self.page {
            Page::History(Filter::Pinned) => tr("Kept forever. The arrows change their order.").to_string(),
            Page::History(_) if !self.query.is_empty() => fill(tr("{n} found"), &[&n]),
            Page::History(_) => fill(tr("{n} items, newest first"), &[&n]),
            Page::Snippets => tr("Texts you type often. Type their trigger in the panel.").to_string(),
            Page::Settings => tr("Everything stays on this PC.").to_string(),
        };
        // beside the search (history) or the "New snippet" button
        let sub_w = if matches!(self.page, Page::History(_)) { (r.w - 48.0 - 470.0).max(260.0) } else { r.w - 48.0 - 180.0 };
        c.text(&sub, rect(x, 78.0, sub_w, 22.0), Font::Body, t.t3, Align::Left);
        let mut right = r.right() - 24.0;
        match self.page {
            Page::History(f) => {
                if f == Filter::All {
                    let confirm = self.confirm_clear.is_some();
                    let label = if confirm { tr("Click again to clear") } else { tr("Clear") };
                    let bw = Self::pill_w(c, label);
                    self.pill(c, rect(right - bw, 46.0, bw, 36.0), label, if confirm { 2 } else { 0 }, Hit::Clear);
                    right -= bw + 10.0;
                }
                let sw = (r.w - 420.0).clamp(180.0, 320.0);
                let sr = rect(right - sw, 46.0, sw, 36.0);
                let editing = self.edit_for == Some(Field::Search);
                c.fill_round(sr, 18.0, t.fill);
                if editing {
                    c.stroke_round(sr.inset(0.5), 18.0, t.accent.fade(0.7), 1.5);
                }
                c.icon(&icons::SEARCH, rect(sr.x + 13.0, sr.y + 10.0, 16.0, 16.0), t.t3);
                if !editing {
                    let (s, col) = if self.query.is_empty() { (tr("Search the history"), t.t3) } else { (self.query.as_str(), t.t1) };
                    c.text(s, rect(sr.x + 38.0, sr.y, sr.w - 48.0, sr.h), Font::Body, col, Align::Left);
                }
                self.hits.add(sr, Hit::Search);
                self.field_rects.push((Field::Search, sr));
            }
            Page::Snippets if self.draft.is_none() => {
                let label = tr("New snippet");
                let bw = Self::pill_w(c, label);
                self.pill(c, rect(right - bw, 46.0, bw, 36.0), label, 1, Hit::NewSnippet);
            }
            _ => {}
        }
    }

    /// An empty page: its icon, a title, one line, and the one button that helps.
    fn empty(&mut self, c: &mut Canvas, r: Rect, icon: &Icon, title: &str, sub: &str, button: (&str, Hit)) {
        let t = self.theme();
        let cy = r.y + (r.h * 0.34).min(180.0);
        let tile = rect(r.x + (r.w - 64.0) / 2.0, cy - 40.0, 64.0, 64.0);
        c.fill_round(tile, 18.0, t.fill);
        c.icon(icon, tile.centered(28.0, 28.0), t.t2);
        c.text(title, rect(r.x, cy + 36.0, r.w, 28.0), Font::Large, t.t1, Align::Center);
        c.text(sub, rect(r.x + 24.0, cy + 66.0, r.w - 48.0, 22.0), Font::Body, t.t3, Align::Center);
        let (label, hit) = button;
        let bw = Self::pill_w(c, label);
        self.pill(c, rect(r.x + (r.w - bw) / 2.0, cy + 104.0, bw, 36.0), label, 1, hit);
    }

    fn paint_history(&mut self, c: &mut Canvas, content: Rect) {
        let t = self.theme();
        let area = rect(content.x, LIST_TOP, content.w, content.h - LIST_TOP);
        let pinned_page = self.page == Page::History(Filter::Pinned);
        if self.rows.is_empty() {
            self.content_h = 0.0;
            let hk = self.model.borrow().hotkey.map(|h| h.label()).unwrap_or_default();
            if !self.query.is_empty() {
                self.empty(c, area, &icons::SEARCH, tr("Nothing found"), tr("Try other words."), (tr("Clear the search"), Hit::ClearSearch));
            } else if pinned_page {
                self.empty(
                    c,
                    area,
                    &icons::PIN,
                    tr("No pins yet"),
                    tr("Pin an item to keep it forever: right-click it, or its pin button."),
                    (tr("Show all"), Hit::Nav(Page::History(Filter::All))),
                );
            } else {
                let sub = fill(tr("Copy something, then press {k} to paste it again."), &[&hk]);
                self.empty(c, area, &ic::BOARD, tr("Nothing copied yet"), &sub, (tr("Open the panel"), Hit::OpenPanel));
            }
            return;
        }
        let model = self.model.clone();
        let m = model.borrow();
        let now = store::now();
        let (x, w) = (area.x + 24.0, area.w - 48.0);
        let mut y = area.y + 2.0 - self.scroll;
        c.clip(area);
        let rows = self.rows.clone();
        let n = rows.len();
        for (i, id) in rows.iter().enumerate() {
            let Some(it) = m.store.get(*id) else { continue };
            let rh = row_h(it.kind);
            if y + rh >= area.y && y <= area.bottom() {
                self.paint_row(c, it, rect(x, y, w, rh), &t, now, pinned_page && i > 0, pinned_page && i + 1 < n, &m.dir);
            }
            y += rh + CARD_GAP;
        }
        c.unclip();
        self.content_h = y + self.scroll - area.y + 16.0;
        self.scroll = self.scroll.min((self.content_h - area.h).max(0.0));
        self.scrollbar(c, area);
    }

    fn scrollbar(&mut self, c: &mut Canvas, area: Rect) {
        if self.content_h <= area.h {
            return;
        }
        let t = self.theme();
        let h = (area.h * area.h / self.content_h).max(30.0);
        let y = area.y + (area.h - h) * (self.scroll / (self.content_h - area.h).max(1.0));
        c.fill_round(rect(area.right() - 7.0, y, 4.0, h), 2.0, t.t1.fade(0.18));
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_row(&mut self, c: &mut Canvas, it: &Item, r: Rect, t: &Theme, now: u64, up: bool, down: bool, dir: &std::path::Path) {
        let id = it.id;
        let hot = matches!(&self.hover, Some(Hit::Row(i) | Hit::RowPin(i) | Hit::RowCopy(i) | Hit::RowDelete(i) | Hit::RowUp(i) | Hit::RowDown(i) | Hit::RowReveal(i)) if *i == id);
        card(c, r, t, hot);
        self.hits.add(r, Hit::Row(id));
        // the picture, or the kind's tile
        let mut x0 = r.x + PAD;
        if it.kind == Kind::Image {
            let (tw, th) = image::fit(it.w, it.h, 1000);
            let box_h = r.h - 32.0;
            let k = (148.0 / tw as f32).min(box_h / th as f32);
            let (dw, dh) = ((tw as f32 * k).max(8.0), (th as f32 * k).max(8.0));
            let frame = rect(r.x + 16.0, r.y + 16.0, 148.0, box_h);
            c.fill_round(frame, 10.0, t.fill);
            let q = frame.centered(dw, dh);
            let dir = dir.to_path_buf();
            c.image(thumb_key(id), q, 1.0, || store::read_image(&dir, id, true).and_then(|p| image::drawable(&p)));
            self.drawn.insert(id);
            x0 = frame.right() + 16.0;
        } else {
            let tile = rect(x0, r.y + 16.0, 40.0, 40.0);
            c.fill_round(tile, 10.0, t.fill);
            let icon = it.secret.map_or(kind_icon(it.kind), secret_icon);
            c.icon(icon, tile.centered(18.0, 18.0), t.t2);
            x0 = tile.right() + 16.0;
        }
        let tw = r.right() - x0 - 190.0;
        let shown = self.reveal.shown(id);
        match it.kind {
            // sensitive: its kind, and dots (the value only while the eye is held, or just after a click on it)
            _ if it.secret.is_some() => {
                let label = it.secret.map(secret_label).unwrap_or_default();
                c.text(label, rect(x0, r.y + 13.0, tw, 20.0), Font::BodyBold, t.t1, Align::Left);
                let (value, ink) = if shown { (it.preview_real(120), t.t1) } else { (MASK.to_string(), t.t2) };
                c.text(&value, rect(x0, r.y + 33.0, tw, 20.0), Font::Body, ink, Align::Left);
            }
            Kind::Text => {
                let s: String = it.text.chars().take(400).collect::<String>().replace('\t', "    ");
                draw::text_box(c, s.trim(), rect(x0, r.y + 15.0, tw, 38.0), 13.0, 400, t.t1);
            }
            Kind::Link => c.text(it.text.trim(), rect(x0, r.y + 14.0, tw, 22.0), Font::BodyBold, draw::accent_ink(t), Align::Left),
            Kind::Files => {
                c.text(&it.preview(200), rect(x0, r.y + 14.0, tw, 20.0), Font::BodyBold, t.t1, Align::Left);
                let folder = it.files.first().and_then(|f| std::path::Path::new(f).parent()).map(|p| p.display().to_string()).unwrap_or_default();
                c.text(&folder, rect(x0, r.y + 34.0, tw, 18.0), Font::Small, t.t2, Align::Left);
            }
            Kind::Image => {
                c.text(tr("Picture"), rect(x0, r.y + 16.0, tw, 22.0), Font::Title, t.t1, Align::Left);
                c.text(&format!("{} × {} px", it.w, it.h), rect(x0, r.y + 38.0, tw, 18.0), Font::Small, t.t2, Align::Left);
            }
        }
        // where it comes from, when
        let my = r.bottom() - 30.0;
        let mut mx = x0;
        if it.pin.is_some() {
            c.icon_fill(&icons::PIN, rect(mx, my + 3.0, 12.0, 12.0), draw::accent_ink(t));
            mx += 18.0;
        }
        let secs = self.model.borrow().set.secret_secs();
        c.text(&meta(it, now, secs), rect(mx, my, r.right() - mx - 190.0, 18.0), Font::Small, t.t3, Align::Left);
        // its buttons: the eye of a sensitive item always, the others on hover
        if hot || up || down || it.secret.is_some() {
            let mut bx = r.right() - 16.0;
            let by = r.y + (r.h - 32.0) / 2.0;
            let mut btn = |app: &mut App, c: &mut Canvas, icon: &Icon, hit: Hit, color: Color| {
                bx -= 34.0;
                app.icon_btn(c, icon, rect(bx, by, 32.0, 32.0), hit, color);
            };
            if it.secret.is_some() {
                let (icon, ink) = if shown { (&ic::EYE_OFF, draw::accent_ink(t)) } else { (&ic::EYE, t.t2) };
                btn(self, c, icon, Hit::RowReveal(id), ink);
            }
            if hot {
                btn(self, c, &icons::TRASH, Hit::RowDelete(id), t.t2);
                btn(self, c, &icons::COPY, Hit::RowCopy(id), t.t2);
                btn(self, c, &icons::PIN, Hit::RowPin(id), if it.pin.is_some() { draw::accent_ink(t) } else { t.t2 });
            }
            if down {
                btn(self, c, &ic::ARROW_DOWN, Hit::RowDown(id), t.t2);
            }
            if up {
                btn(self, c, &ic::ARROW_UP, Hit::RowUp(id), t.t2);
            }
        }
    }

    fn paint_snippets(&mut self, c: &mut Canvas, content: Rect) {
        let t = self.theme();
        let area = rect(content.x, LIST_TOP, content.w, content.h - LIST_TOP);
        if self.draft.is_some() {
            self.paint_editor(c, area);
            return;
        }
        let snippets = self.model.borrow().store.snippets.clone();
        if snippets.is_empty() {
            self.content_h = 0.0;
            self.empty(
                c,
                area,
                &ic::SNIPPET,
                tr("No snippets yet"),
                tr("Save a text you type often, with a trigger like ;sig."),
                (tr("New snippet"), Hit::NewSnippet),
            );
            return;
        }
        let (x, w) = (area.x + 24.0, area.w - 48.0);
        let mut y = area.y + 2.0 - self.scroll;
        c.clip(area);
        for s in &snippets {
            let r = rect(x, y, w, 88.0);
            let hit = Hit::Snippet(s.id);
            let hot = self.hovered(&hit);
            card(c, r, &t, hot);
            let tile = rect(r.x + PAD, r.y + 16.0, 40.0, 40.0);
            c.fill_round(tile, 10.0, t.fill);
            c.icon(&ic::SNIPPET, tile.centered(18.0, 18.0), t.t2);
            let x0 = tile.right() + 16.0;
            let nw = c.text_width(&s.name, Font::BodyBold).min(w * 0.5);
            c.text(&s.name, rect(x0, r.y + 15.0, nw + 2.0, 22.0), Font::BodyBold, t.t1, Align::Left);
            if !s.trigger.is_empty() {
                let pw = c.text_width(&s.trigger, Font::SmallBold) + 20.0;
                let pr = rect(x0 + nw + 10.0, r.y + 16.0, pw, 20.0);
                c.fill_round(pr, 10.0, draw::selection(&t));
                c.text(&s.trigger, pr, Font::SmallBold, draw::accent_ink(&t), Align::Center);
            }
            let preview = s.text.split_whitespace().collect::<Vec<_>>().join(" ");
            draw::text_box(c, &preview, rect(x0, r.y + 42.0, r.right() - x0 - PAD, 36.0), 13.0, 400, t.t2);
            self.hits.add(r, hit);
            y += r.h + CARD_GAP;
        }
        c.unclip();
        self.content_h = y + self.scroll - area.y + 16.0;
        self.scrollbar(c, area);
    }

    fn field(&mut self, c: &mut Canvas, f: Field, r: Rect, value: &str, placeholder: &str) {
        let t = self.theme();
        let on = self.edit_for == Some(f);
        c.fill_round(r, 10.0, t.fill);
        if on {
            c.stroke_round(r.inset(0.5), 10.0, t.accent.fade(0.7), 1.5);
        } else {
            let (s, col) = if value.is_empty() { (placeholder, t.t3) } else { (value, t.t1) };
            if f == Field::Body {
                draw::text_box(c, s, rect(r.x + 12.0, r.y + 9.0, r.w - 24.0, r.h - 18.0), 14.0, 400, col);
            } else {
                draw::text(c, s, rect(r.x + 12.0, r.y, r.w - 24.0, r.h), 14.0, 400, col, Align::Left);
            }
        }
        self.hits.add(r, Hit::Field(f));
        self.field_rects.push((f, r));
    }

    fn paint_editor(&mut self, c: &mut Canvas, area: Rect) {
        let t = self.theme();
        let d = self.draft.clone().unwrap_or_default();
        let x = area.x + 24.0;
        let w = (area.w - 48.0).min(720.0);
        let mut y = area.y + 4.0;
        let label = |c: &mut Canvas, s: &str, y: f32| c.text(s, rect(x, y, w, 18.0), Font::SmallBold, t.t2, Align::Left);
        label(c, tr("Name"), y);
        self.field(c, Field::Name, rect(x, y + 22.0, w, 40.0), &d.name, tr("Signature"));
        y += 76.0;
        label(c, tr("Trigger"), y);
        self.field(c, Field::Trigger, rect(x, y + 22.0, 220.0, 40.0), &d.trigger, ";sig");
        c.text(tr("Type it in the panel to find this snippet."), rect(x + 236.0, y + 22.0, w - 236.0, 40.0), Font::Small, t.t3, Align::Left);
        y += 76.0;
        label(c, tr("Text"), y);
        let body_h = (area.bottom() - y - 110.0).clamp(120.0, 320.0);
        self.field(c, Field::Body, rect(x, y + 22.0, w, body_h), &d.text, tr("What the snippet pastes"));
        y += body_h + 40.0;
        let save = tr("Save");
        let bw = Self::pill_w(c, save);
        self.pill(c, rect(x, y, bw, 36.0), save, 1, Hit::Save);
        let cancel = tr("Cancel");
        let cw = Self::pill_w(c, cancel);
        self.pill(c, rect(x + bw + 10.0, y, cw, 36.0), cancel, 0, Hit::Cancel);
        if d.id.is_some() {
            let del = tr("Delete");
            let dw = Self::pill_w(c, del);
            self.pill(c, rect(x + w - dw, y, dw, 36.0), del, 2, Hit::DeleteSnippet);
        }
        c.text(tr("Ctrl+Enter saves."), rect(x + bw + cw + 24.0, y, 200.0, 36.0), Font::Small, t.t3, Align::Left);
        self.content_h = 0.0;
    }

    /// The question before a sensitive item is pinned: a card over the dimmed window.
    fn paint_dialog(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(id) = self.dialog else { return };
        let t = self.theme();
        let kind = self.model.borrow().store.get(id).and_then(|i| i.secret);
        let Some(kind) = kind else {
            self.dialog = None;
            return;
        };
        let all = rect(0.0, 0.0, w, h);
        c.fill(all, Color::hex(0x000000).fade(if t.dark { 0.5 } else { 0.3 }));
        self.hits.add(all, Hit::Scrim);
        let card = rect((w - 420.0) / 2.0, (h - 200.0) / 2.0, 420.0, 200.0);
        c.fill_round(card, 20.0, t.solid);
        c.stroke_round(card.inset(0.5), 20.0, t.ring, 1.0);
        let x = card.x + 24.0;
        c.icon(secret_icon(kind), rect(x, card.y + 28.0, 20.0, 20.0), t.t2);
        c.text(tr("Pin this sensitive item?"), rect(x + 32.0, card.y + 22.0, card.w - 80.0, 32.0), Font::Large, t.t1, Align::Left);
        let body = fill(tr("This item ({kind}) will be kept forever instead of a short time. It stays hidden and sealed."), &[secret_label(kind)]);
        draw::text_box(c, &body, rect(x, card.y + 66.0, card.w - 48.0, 48.0), 13.0, 400, t.t2);
        let ok = tr("Pin it");
        let ow = Self::pill_w(c, ok);
        let by = card.bottom() - 24.0 - 36.0;
        self.pill(c, rect(card.right() - 24.0 - ow, by, ow, 36.0), ok, 1, Hit::DialogOk);
        let cancel = tr("Cancel");
        let cw = Self::pill_w(c, cancel);
        self.pill(c, rect(card.right() - 24.0 - ow - 10.0 - cw, by, cw, 36.0), cancel, 0, Hit::DialogCancel);
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((text, at)) = &self.toast else { return };
        let t = self.theme();
        let age = at.elapsed().as_secs_f32();
        let a = if age < 0.15 {
            age / 0.15
        } else if age > super::TOAST_SECS - 0.4 {
            ((super::TOAST_SECS - age) / 0.4).max(0.0)
        } else {
            1.0
        };
        let text = text.clone();
        let tw = c.text_width(&text, Font::BodyBold) + 48.0;
        let r = rect(SIDEBAR_W + (w - SIDEBAR_W - tw) / 2.0, h - 72.0 + (1.0 - a) * 8.0, tw, 40.0);
        c.fill_round(r, 20.0, t.solid2.fade(a));
        c.stroke_round(r.inset(0.5), 20.0, t.ring.fade(a), 1.0);
        c.text(&text, r, Font::BodyBold, t.t1.fade(a), Align::Center);
        self.hits.add(r, Hit::Toast);
    }
}
