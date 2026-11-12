//! Drawing the main window.

use super::{App, DialogKind, FIND_H, Field, FindPart, Hit, STATUS_H, Status, TITLE_H, TOAST_SECS, TOOL_H, Tool, draw, draw::ic};
use crate::{
    editor::Colors,
    i18n::{fill, tr},
};
use plume_ui::{Align, Canvas, Color, Font, PaintCx, Rect, Theme, icons, icons::Icon, m, rect};

pub fn editor_colors(t: &Theme) -> Colors {
    let bg = if t.dark { t.bg2 } else { t.solid };
    Colors {
        bg,
        text: t.t1,
        gutter: bg,
        numbers: t.t4,
        numbers_cur: t.t2,
        line: t.hover.fade(0.55),
        sel: t.accent.fade(if t.dark { 0.38 } else { 0.26 }),
        caret: draw::accent_ink(t),
        found: t.orange.fade(0.22),
        found_cur: t.orange.fade(0.5),
        bar: t.t4.fade(0.7),
        bar_hot: t.t3,
    }
}

/// "1 234" with a thin space, "1,234" in English.
pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let sep = if crate::i18n::french() { '\u{202f}' } else { ',' };
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}

impl App {
    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    fn down(&self, h: &Hit) -> bool {
        self.hovered(h) && self.pressed.as_ref() == Some(h)
    }

    /// A round icon button; `on`: a toggle that is on.
    fn icon_btn(&mut self, c: &mut Canvas, icon: &Icon, r: Rect, hit: Hit, on: bool, enabled: bool) {
        let t = self.theme();
        let hot = self.hovered(&hit) && enabled;
        if on {
            c.fill_round(r, r.h / 2.0, draw::chosen(&t));
        }
        if hot {
            c.fill_round(r, r.h / 2.0, if self.down(&hit) { t.fill2 } else { t.hover });
        }
        let ink = if !enabled {
            t.t4
        } else if on {
            draw::accent_ink(&t)
        } else if hot {
            t.t1
        } else {
            t.t2
        };
        c.icon(icon, r.centered(16.0, 16.0), ink);
        self.hits.add(r, hit);
    }

    fn pill_btn(&mut self, c: &mut Canvas, r: Rect, label: &str, kind: u8, hit: Hit) {
        let t = self.theme();
        draw::pill(c, &t, r, label, kind, self.hovered(&hit), self.down(&hit), Font::BodyBold);
        self.hits.add(r, hit);
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let t = self.theme();
        self.hits.clear();
        self.field_rects.clear();
        c.clear(t.bg);
        let (w, h) = (cx.w, cx.h);
        if self.settings {
            self.paint_settings(c, rect(0.0, TITLE_H, w, h - TITLE_H));
        } else {
            let status = self.sh.borrow().set.status;
            let find_h = if self.find.open { FIND_H * if self.find.replace { 2.0 } else { 1.0 } } else { 0.0 };
            let top = TITLE_H + TOOL_H + find_h;
            let bottom = if status { h - STATUS_H } else { h };
            let area = rect(0.0, top, w, bottom - top);
            // the text first: the bars above it take the clicks they cover
            self.hits.add(area, Hit::Editor);
            let found = self.find.open;
            let cur = {
                let s = self.tab().ed.doc.sel;
                (!s.is_empty()).then(|| (s.start(), s.end()))
            };
            let colors = editor_colors(&t);
            let m = self.find.matcher.take();
            {
                let i = self.active.min(self.tabs.len() - 1);
                let ed = &mut self.tabs[i].ed;
                ed.focused = cx.active && !self.field_focus && self.dialog.is_none();
                let matcher = if found { m.as_ref().and_then(|r| r.as_ref().ok()) } else { None };
                ed.paint(c, area, &colors, matcher, cur);
            }
            self.find.matcher = m;
            c.hline(0.0, top, w, t.sep);
            self.paint_tools(c, w);
            if self.find.open {
                self.paint_find(c, rect(0.0, TITLE_H + TOOL_H, w, find_h));
            }
            if status {
                self.paint_status(c, rect(0.0, h - STATUS_H, w, STATUS_H));
            }
        }
        self.paint_title(c, w, cx);
        if self.dialog.is_some() {
            self.paint_dialog(c, w, h);
        }
        self.paint_tip(c);
        self.paint_toast(c, w, h);
    }

    fn paint_title(&mut self, c: &mut Canvas, w: f32, cx: &PaintCx) {
        let t = self.theme();
        draw::logo(c, rect(16.0, (TITLE_H - 18.0) / 2.0, 18.0, 18.0));
        let name = "Plume Notepad";
        let nw = c.text_width(name, Font::BodyBold);
        c.text(name, rect(42.0, 0.0, nw + 4.0, TITLE_H), Font::BodyBold, if cx.active { t.t1 } else { t.t3 }, Align::Left);
        // the tabs
        let x0 = 42.0 + nw + 18.0;
        let caps = w - 3.0 * m::CAPTION_W;
        let avail = (caps - 48.0 - 40.0 - x0).max(120.0);
        let n = self.tabs.len();
        let tw = (avail / n as f32).clamp(96.0, 210.0);
        let fit = ((avail / tw).floor() as usize).max(1);
        let first = if self.active >= fit { self.active + 1 - fit } else { 0 };
        let mut x = x0;
        for i in first..n.min(first + fit) {
            let r = rect(x, 6.0, tw - 4.0, 30.0);
            let active = i == self.active && !self.settings;
            let hot = self.hovered(&Hit::Tab(i)) || self.hovered(&Hit::TabClose(i));
            if active {
                c.fill_round(r, 9.0, t.fill.over(t.bg));
                c.stroke_round(r, 9.0, t.ring, 1.0);
            } else if hot {
                c.fill_round(r, 9.0, t.hover);
            }
            self.hits.add(r, Hit::Tab(i));
            let tab = &self.tabs[i];
            let (label, dirty) = (tab.name(), tab.dirty());
            c.text(
                &label,
                rect(r.x + 12.0, r.y, r.w - 40.0, r.h),
                if active { Font::BodyBold } else { Font::Body },
                if active { t.t1 } else { t.t2 },
                Align::Left,
            );
            let close = rect(r.right() - 26.0, r.y + 5.0, 20.0, 20.0);
            if hot || active {
                let ch = self.hovered(&Hit::TabClose(i));
                if ch {
                    c.fill_round(close, 10.0, t.fill2);
                }
                if dirty && !hot {
                    c.fill_round(close.centered(8.0, 8.0), 4.0, draw::accent_ink(&t));
                } else {
                    c.icon(&icons::CLOSE, close.centered(12.0, 12.0), if ch { t.t1 } else { t.t3 });
                }
                self.hits.add(close, Hit::TabClose(i));
            } else if dirty {
                c.fill_round(close.centered(8.0, 8.0), 4.0, t.t3);
            }
            x += tw;
        }
        let plus = rect(x + 2.0, 7.0, 28.0, 28.0);
        self.icon_btn(c, &icons::PLUS, plus, Hit::NewTab, false, true);
        // the window's buttons
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(caps + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
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

    fn paint_tools(&mut self, c: &mut Canvas, w: f32) {
        let (wrap, can_undo, can_redo) = {
            let s = self.sh.borrow();
            let d = &self.tab().ed.doc;
            (s.set.wrap, d.can_undo(), d.can_redo())
        };
        let y = TITLE_H + (TOOL_H - 34.0) / 2.0;
        let mut x = 12.0;
        let groups: [&[(Tool, &Icon)]; 4] = [
            &[(Tool::New, &ic::NEW), (Tool::Open, &icons::FOLDER), (Tool::Save, &ic::SAVE)],
            &[(Tool::Undo, &ic::UNDO), (Tool::Redo, &ic::REDO)],
            &[(Tool::Find, &icons::SEARCH), (Tool::Replace, &ic::REPLACE)],
            &[(Tool::Wrap, &ic::WRAP), (Tool::Pad, &ic::PAD)],
        ];
        let t = self.theme();
        for (gi, g) in groups.iter().enumerate() {
            if gi > 0 {
                c.vline(x + 5.0, y + 9.0, 16.0, t.sep);
                x += 12.0;
            }
            for (tool, icon) in g.iter() {
                let on = match tool {
                    Tool::Wrap => wrap,
                    Tool::Find => self.find.open && !self.find.replace,
                    Tool::Replace => self.find.open && self.find.replace,
                    _ => false,
                };
                let enabled = match tool {
                    Tool::Undo => can_undo,
                    Tool::Redo => can_redo,
                    _ => true,
                };
                self.icon_btn(c, icon, rect(x, y, 34.0, 34.0), Hit::Tool(*tool), on, enabled);
                x += 38.0;
            }
        }
        self.icon_btn(c, &icons::GEAR, rect(w - 12.0 - 34.0, y, 34.0, 34.0), Hit::Tool(Tool::Settings), false, true);
        self.icon_btn(c, &icons::MORE, rect(w - 12.0 - 72.0, y, 34.0, 34.0), Hit::Tool(Tool::More), false, true);
    }

    /// A text field drawn by the app (the native one sits on it while typing).
    fn field(&mut self, c: &mut Canvas, f: Field, r: Rect, value: &str, placeholder: &str, error: bool) {
        let t = self.theme();
        c.fill_round(r, r.h / 2.0, t.fill);
        if self.field == Some(f) && self.field_focus {
            c.stroke_round(r, r.h / 2.0, if error { t.red } else { t.accent }, 1.5);
        } else if error {
            c.stroke_round(r, r.h / 2.0, t.red, 1.5);
        }
        if self.field_shown != Some(f) {
            let inner = rect(r.x + 15.0, r.y, r.w - 30.0, r.h);
            if value.is_empty() {
                c.text(placeholder, inner, Font::Body, t.t3, Align::Left);
            } else {
                c.text(value, inner, Font::Body, t.t1, Align::Left);
            }
        }
        self.field_rects.push((f, r));
        self.hits.add(r, Hit::Field(f));
    }

    fn toggle_text(&mut self, c: &mut Canvas, r: Rect, label: &str, on: bool, hit: Hit) {
        let t = self.theme();
        let hot = self.hovered(&hit);
        c.fill_round(
            r,
            r.h / 2.0,
            if on {
                draw::chosen(&t)
            } else if hot {
                t.hover
            } else {
                Color::TRANSPARENT
            },
        );
        c.text(label, r, Font::SmallBold, if on { draw::accent_ink(&t) } else { t.t2 }, Align::Center);
        self.hits.add(r, hit);
    }

    fn paint_find(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.theme();
        c.hline(0.0, r.y, r.w, t.sep);
        let fw = (r.w * 0.36).clamp(220.0, 360.0);
        let y = r.y + (FIND_H - 34.0) / 2.0;
        let error = matches!(self.find.matcher, Some(Err(_)));
        let q = self.find.q.text.clone();
        self.field(c, Field::Find, rect(16.0, y, fw, 34.0), &q, tr("Find"), error);
        let mut x = 16.0 + fw + 8.0;
        let (case, word, re) = (self.find.q.case, self.find.q.word, self.find.q.regex);
        self.toggle_text(c, rect(x, y + 2.0, 36.0, 30.0), "Aa", case, Hit::Find(FindPart::Case));
        x += 38.0;
        self.toggle_text(c, rect(x, y + 2.0, 36.0, 30.0), "ab", word, Hit::Find(FindPart::Word));
        x += 38.0;
        self.toggle_text(c, rect(x, y + 2.0, 36.0, 30.0), ".*", re, Hit::Find(FindPart::Regex));
        x += 46.0;
        self.icon_btn(c, &ic::ARROW_UP, rect(x, y, 34.0, 34.0), Hit::Find(FindPart::Prev), false, !q.is_empty());
        x += 36.0;
        self.icon_btn(c, &ic::ARROW_DOWN, rect(x, y, 34.0, 34.0), Hit::Find(FindPart::Next), false, !q.is_empty());
        x += 44.0;
        // how many, or what is wrong with the expression
        let info = match &self.find.matcher {
            Some(Err(e)) => (e.clone(), t.red),
            Some(Ok(_)) => match self.match_count() {
                Some((0, _)) => (tr("No results").to_string(), t.t3),
                Some((n, 0)) => (if n == 1 { tr("1 result").to_string() } else { fill(tr("{n} results"), &[&thousands(n)]) }, t.t3),
                Some((n, i)) => (fill(tr("{i} of {n}"), &[&thousands(i), &thousands(n)]), t.t2),
                None => (String::new(), t.t3),
            },
            None => (String::new(), t.t3),
        };
        let right = r.w - 16.0 - 2.0 * 38.0;
        c.text(&info.0, rect(x, r.y, (right - x - 8.0).max(0.0), FIND_H), Font::Small, info.1, Align::Left);
        let replace = self.find.replace;
        self.icon_btn(c, &ic::REPLACE, rect(right, y, 34.0, 34.0), Hit::Find(FindPart::Mode), replace, true);
        self.icon_btn(c, &icons::CLOSE, rect(right + 38.0, y, 34.0, 34.0), Hit::Find(FindPart::Close), false, true);
        if replace {
            let y = r.y + FIND_H + (FIND_H - 34.0) / 2.0 - 4.0;
            let with = self.find.with.clone();
            self.field(c, Field::Replace, rect(16.0, y, fw, 34.0), &with, tr("Replace with"), false);
            let mut x = 16.0 + fw + 8.0;
            let one = tr("Replace");
            let ow = draw::pill_w(c, one);
            self.pill_btn(c, rect(x, y, ow, 34.0), one, 0, Hit::Find(FindPart::One));
            x += ow + 8.0;
            let all = tr("Replace all");
            let aw = draw::pill_w(c, all);
            self.pill_btn(c, rect(x, y, aw, 34.0), all, 0, Hit::Find(FindPart::All));
        }
    }

    fn status_item(&mut self, c: &mut Canvas, x: f32, y: f32, label: &str, hit: Hit, right: bool) -> f32 {
        let t = self.theme();
        let w = c.text_width(label, Font::Small) + 20.0;
        let r = rect(if right { x - w } else { x }, y + 3.0, w, STATUS_H - 6.0);
        if self.hovered(&hit) {
            c.fill_round(r, r.h / 2.0, t.hover);
        }
        c.text(label, r, Font::Small, t.t2, Align::Center);
        self.hits.add(r, hit);
        w
    }

    fn paint_status(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.theme();
        c.fill(r, t.bg);
        c.hline(0.0, r.y, r.w, t.sep);
        let (line, col, chars, sel, enc, eol) = {
            let tab = self.tab();
            let d = &tab.ed.doc;
            let car = d.sel.caret;
            let col = d.line(car.line)[..car.col].chars().count() + 1;
            let sel = if d.sel.is_empty() || d.chars() > 20_000_000 { 0 } else { d.selected().chars().count() };
            (car.line + 1, col, d.chars(), sel, tab.enc.label(), tab.eol.label())
        };
        let pos = fill(tr("Ln {l}, Col {c}"), &[&thousands(line), &thousands(col)]);
        let w = self.status_item(c, 8.0, r.y, &pos, Hit::Status(Status::Pos), false);
        let count = if sel > 0 {
            fill(tr("{n} characters ({s} selected)"), &[&thousands(chars), &thousands(sel)])
        } else {
            fill(tr("{n} characters"), &[&thousands(chars)])
        };
        c.text(&count, rect(8.0 + w + 6.0, r.y, 320.0, r.h), Font::Small, t.t3, Align::Left);
        let zoom = format!("{} %", (self.sh.borrow().set.zoom * 100.0).round());
        let mut x = r.w - 8.0;
        x -= self.status_item(c, x, r.y, &enc, Hit::Status(Status::Enc), true) + 4.0;
        x -= self.status_item(c, x, r.y, eol, Hit::Status(Status::Eol), true) + 4.0;
        self.status_item(c, x, r.y, &zoom, Hit::Status(Status::Zoom), true);
    }

    fn paint_dialog(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let t = self.theme();
        let Some(d) = &self.dialog else { return };
        let (title, text, buttons, goto) = (d.title.clone(), d.text.clone(), d.buttons.clone(), d.kind == DialogKind::Goto);
        c.fill(rect(0.0, 0.0, w, h), Color::hexa(0x000000, if t.dark { 0.45 } else { 0.28 }));
        self.hits.add(rect(0.0, 0.0, w, h), Hit::Scrim);
        let dw = 440.0f32.min(w - 48.0);
        let text_h = c.text_height(&text, dw - 48.0, Font::Body);
        let dh = 24.0 + 26.0 + 10.0 + text_h + if goto { 50.0 } else { 0.0 } + 24.0 + 36.0 + 24.0;
        let r = rect((w - dw) / 2.0, (h - dh) / 2.0, dw, dh);
        c.fill_round(r, 20.0, t.solid);
        c.stroke_round(r, 20.0, t.ring, 1.0);
        c.text(&title, rect(r.x + 24.0, r.y + 24.0, dw - 48.0, 26.0), Font::Large, t.t1, Align::Left);
        c.text_block(&text, rect(r.x + 24.0, r.y + 60.0, dw - 48.0, text_h), Font::Body, t.t2);
        if goto {
            let gt = self.goto_text.clone();
            self.field(c, Field::Goto, rect(r.x + 24.0, r.y + 60.0 + text_h + 12.0, dw - 48.0, 36.0), &gt, "", false);
        }
        let mut x = r.right() - 24.0;
        let by = r.bottom() - 24.0 - 36.0;
        for (i, (label, kind)) in buttons.iter().enumerate().rev() {
            let bw = draw::pill_w(c, label).max(88.0);
            x -= bw;
            self.pill_btn(c, rect(x, by, bw, 36.0), label, *kind, Hit::Dialog(i));
            x -= 8.0;
        }
    }

    fn paint_tip(&mut self, c: &mut Canvas) {
        let Some((Hit::Tool(tool), true)) = self.tip.clone() else { return };
        let Some(r) = self.hits.rect_of(|h| *h == Hit::Tool(tool)) else { return };
        let label = match tool {
            Tool::New => format!("{} (Ctrl+N)", tr("New tab")),
            Tool::Open => format!("{} (Ctrl+O)", tr("Open")),
            Tool::Save => format!("{} (Ctrl+S)", tr("Save")),
            Tool::Undo => format!("{} (Ctrl+Z)", tr("Undo")),
            Tool::Redo => format!("{} (Ctrl+Y)", tr("Redo")),
            Tool::Find => format!("{} (Ctrl+F)", tr("Find")),
            Tool::Replace => format!("{} (Ctrl+H)", tr("Replace")),
            Tool::Wrap => format!("{} (Alt+Z)", tr("Word wrap")),
            Tool::Pad => format!("{} (Ctrl+Alt+N)", tr("New sticky pad")),
            Tool::More => tr("More").to_string(),
            Tool::Settings => tr("Settings").to_string(),
        };
        let t = self.theme();
        let tw = c.text_width(&label, Font::Small) + 24.0;
        let (w, _) = c.size();
        let x = (r.x + r.w / 2.0 - tw / 2.0).clamp(8.0, w - tw - 8.0);
        draw::bubble(c, &t, rect(x, r.bottom() + 6.0, tw, 28.0), &label, 1.0, Font::Small);
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some((text, at)) = &self.toast else { return };
        let t = self.theme();
        let age = at.elapsed().as_secs_f32();
        let a = if age < 0.15 {
            age / 0.15
        } else if age > TOAST_SECS - 0.4 {
            ((TOAST_SECS - age) / 0.4).max(0.0)
        } else {
            1.0
        };
        let text = text.clone();
        let tw = (c.text_width(&text, Font::BodyBold) + 48.0).min(w - 32.0);
        let r = rect((w - tw) / 2.0, h - STATUS_H - 60.0 + (1.0 - a) * 8.0, tw, 40.0);
        draw::bubble(c, &t, r, &text, a, Font::BodyBold);
        self.hits.add(r, Hit::Toast);
    }
}
