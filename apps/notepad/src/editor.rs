//! The text editing control, drawn on plume-ui: caret, selection with the keyboard and the mouse, undo, the
//! clipboard, scrolling, word wrap, line numbers and highlighted matches. Only the rows on screen are laid out, so
//! big files stay smooth. Used by the tabs of the main window and by the sticky pads.

use crate::{
    clip,
    layout::{Measure, shown},
    search::Matcher,
    text::{Doc, Kind, Pos, Sel, pos},
};
use plume_ui::{Canvas, Color, Mods, Pt, Rect, Window, rect};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Instant};
use windows::Win32::Graphics::{
    Direct2D::{Common::D2D1_COLOR_F, D2D1_DRAW_TEXT_OPTIONS, ID2D1RenderTarget, ID2D1SolidColorBrush},
    DirectWrite::DWRITE_TEXT_RANGE,
};
use windows_numerics::Vector2;

pub const BAR_W: f32 = 12.0;
const PAD_L: f32 = 10.0;
const PAD_T: f32 = 6.0;

/// The colours of an editor (the main window's and a pad's differ).
#[derive(Clone, Debug)]
pub struct Colors {
    pub bg: Color,
    pub text: Color,
    pub gutter: Color,
    pub numbers: Color,
    pub numbers_cur: Color,
    pub line: Color,
    pub sel: Color,
    pub caret: Color,
    pub found: Color,
    pub found_cur: Color,
    pub bar: Color,
    pub bar_hot: Color,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    None,
    /// Selecting text: by characters, by words (from the word double-clicked) or by lines.
    Text {
        words: Option<(Pos, Pos)>,
        lines: Option<usize>,
    },
    VBar {
        grab: f32,
    },
    HBar {
        grab: f32,
    },
}

type RowCache = HashMap<usize, Rc<Vec<usize>>>;

pub struct Editor {
    pub doc: Doc,
    pub m: Rc<Measure>,
    pub wrap: bool,
    pub numbers: bool,
    /// The first row on screen: a line and a row of that line.
    pub top: (usize, usize),
    /// Horizontal scroll, without word wrap.
    pub sx: f32,
    /// The x the caret keeps when going up and down.
    goal: Option<f32>,
    /// Where the editor was drawn last.
    pub area: Rect,
    drag: Drag,
    pub focused: bool,
    pub blink_on: bool,
    /// The caret on screen (DIP), for the input method window.
    pub caret_rect: Option<Rect>,
    widest: f32,
    cache: RefCell<((u64, u32, u32), RowCache)>,
    widths: RefCell<(u64, HashMap<usize, f32>)>,
    last_double: Option<(Instant, Pos)>,
    pub hover_bar: bool,
}

fn col(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}

impl Editor {
    pub fn new(doc: Doc, m: Rc<Measure>) -> Editor {
        Editor {
            doc,
            m,
            wrap: true,
            numbers: false,
            top: (0, 0),
            sx: 0.0,
            goal: None,
            area: rect(0.0, 0.0, 600.0, 400.0),
            drag: Drag::None,
            focused: true,
            blink_on: true,
            caret_rect: None,
            widest: 0.0,
            cache: RefCell::new(((u64::MAX, 0, 0), HashMap::new())),
            widths: RefCell::new((u64::MAX, HashMap::new())),
            last_double: None,
            hover_bar: false,
        }
    }

    // ---- geometry ----

    pub fn gutter_w(&self) -> f32 {
        if !self.numbers {
            return 0.0;
        }
        let digits = self.doc.line_count().to_string().len().max(3) as f32;
        (digits * self.m.adv('0') + 26.0).ceil()
    }

    /// Where the text goes (without the gutter and the scroll bars).
    pub fn text_rect(&self) -> Rect {
        let g = self.gutter_w();
        let a = self.area;
        let hbar = if self.wrap { 0.0 } else { BAR_W };
        rect(a.x + g + PAD_L, a.y + PAD_T, (a.w - g - PAD_L - BAR_W - 6.0).max(20.0), (a.h - PAD_T - hbar).max(10.0))
    }

    fn wrap_w(&self) -> f32 {
        if self.wrap { self.text_rect().w - 4.0 } else { 0.0 }
    }

    pub fn visible_rows(&self) -> usize {
        ((self.text_rect().h / self.m.line_h).floor() as usize).max(1)
    }

    /// Where the rows of a line start (cached until the text, the width or the font change).
    pub fn rows(&self, line: usize) -> Rc<Vec<usize>> {
        let ww = self.wrap_w();
        if ww <= 0.0 {
            return Rc::new(vec![0]);
        }
        let key = (self.doc.version, ww.to_bits(), self.m.size.to_bits());
        let mut c = self.cache.borrow_mut();
        if c.0 != key {
            c.0 = key;
            c.1.clear();
        }
        if let Some(r) = c.1.get(&line) {
            return r.clone();
        }
        let r = Rc::new(self.m.wrap(self.doc.line(line), ww));
        if c.1.len() > 4096 {
            c.1.clear();
        }
        c.1.insert(line, r.clone());
        r
    }

    fn line_width(&self, line: usize) -> f32 {
        let mut c = self.widths.borrow_mut();
        if c.0 != self.doc.version {
            c.0 = self.doc.version;
            c.1.clear();
        }
        *c.1.entry(line).or_insert_with(|| self.m.width(self.doc.line(line)))
    }

    /// The row of a line that holds `col`.
    fn row_in(rows: &[usize], col: usize) -> usize {
        rows.partition_point(|&s| s <= col).saturating_sub(1)
    }

    /// (line, row) of a position.
    pub fn vpos(&self, p: Pos) -> (usize, usize) {
        (p.line, Self::row_in(&self.rows(p.line), p.col))
    }

    fn next_row(&self, (l, r): (usize, usize)) -> Option<(usize, usize)> {
        if r + 1 < self.rows(l).len() {
            Some((l, r + 1))
        } else if l + 1 < self.doc.line_count() {
            Some((l + 1, 0))
        } else {
            None
        }
    }

    fn prev_row(&self, (l, r): (usize, usize)) -> Option<(usize, usize)> {
        if r > 0 {
            Some((l, r - 1))
        } else if l > 0 {
            Some((l - 1, self.rows(l - 1).len() - 1))
        } else {
            None
        }
    }

    /// `n` rows further (or back), stopping at the ends.
    pub fn offset_rows(&self, mut at: (usize, usize), n: isize) -> (usize, usize) {
        if n >= 0 {
            for _ in 0..n {
                match self.next_row(at) {
                    Some(p) => at = p,
                    None => break,
                }
            }
        } else {
            for _ in 0..(-n) {
                match self.prev_row(at) {
                    Some(p) => at = p,
                    None => break,
                }
            }
        }
        at
    }

    fn last_row(&self) -> (usize, usize) {
        let l = self.doc.line_count() - 1;
        (l, self.rows(l).len() - 1)
    }

    /// The furthest the first row can go: the end of the text on the last row of the screen.
    fn max_top(&self) -> (usize, usize) {
        self.offset_rows(self.last_row(), -(self.visible_rows() as isize - 1))
    }

    pub fn clamp_top(&mut self) {
        let lines = self.doc.line_count();
        if self.top.0 >= lines {
            self.top = (lines - 1, 0);
        }
        let n = self.rows(self.top.0).len();
        if self.top.1 >= n {
            self.top.1 = n - 1;
        }
        let max = self.max_top();
        if self.top > max {
            self.top = max;
        }
        if self.wrap {
            self.sx = 0.0;
        }
    }

    /// The x of a position from the start of its row.
    pub fn x_of(&self, p: Pos) -> f32 {
        let rows = self.rows(p.line);
        let s = rows[Self::row_in(&rows, p.col)];
        let line = self.doc.line(p.line);
        self.m.width(&line[s..p.col.min(line.len())])
    }

    /// Scrolls so that the caret is on screen.
    pub fn ensure_visible(&mut self) {
        let c = self.doc.sel.caret;
        let cr = self.vpos(c);
        let vis = self.visible_rows();
        if cr < self.top {
            self.top = cr;
        } else {
            let mut p = self.top;
            let mut k = 0;
            while p < cr && k < vis {
                match self.next_row(p) {
                    Some(n) => p = n,
                    None => break,
                }
                k += 1;
            }
            if k >= vis {
                self.top = self.offset_rows(cr, -(vis as isize - 1));
            }
        }
        if !self.wrap {
            let x = self.x_of(c);
            let tw = self.text_rect().w;
            if x < self.sx + 2.0 {
                self.sx = (x - tw / 3.0).max(0.0);
            } else if x > self.sx + tw - 12.0 {
                self.sx = x - tw * 2.0 / 3.0;
            }
        }
        self.clamp_top();
    }

    /// The position under a point of the window.
    pub fn hit(&self, p: Pt) -> Pos {
        let tr = self.text_rect();
        let i = ((p.y - tr.y) / self.m.line_h).floor() as isize;
        let (l, r) = self.offset_rows(self.top, i);
        let rows = self.rows(l);
        let line = self.doc.line(l);
        let (s, e) = (rows[r], rows.get(r + 1).copied().unwrap_or(line.len()));
        let x = p.x - tr.x + self.sx;
        if i < 0 && (l, r) == (0, 0) && x < 0.0 {
            return pos(0, 0);
        }
        let mut c = s + self.m.col_at(&line[s..e], x);
        // past the end of a wrapped row: on that row, before its last character
        if c == e && r + 1 < rows.len() && c > s {
            c = line[..c].char_indices().next_back().map_or(c, |(i, _)| i);
        }
        pos(l, c)
    }

    fn vbar(&self) -> Rect {
        let a = self.area;
        rect(a.right() - BAR_W, a.y, BAR_W, a.h - if self.wrap { 0.0 } else { BAR_W })
    }

    fn hbar(&self) -> Rect {
        let a = self.area;
        let g = self.gutter_w();
        rect(a.x + g, a.bottom() - BAR_W, a.w - g - BAR_W, BAR_W)
    }

    /// The thumb of the vertical bar, if the text is taller than the screen.
    fn vthumb(&self) -> Option<Rect> {
        let max = self.max_top();
        if max == (0, 0) {
            return None;
        }
        let track = self.vbar().inset_xy(2.0, 2.0);
        let total = self.doc.line_count().max(1) as f32;
        let vis = self.visible_rows() as f32;
        let h = (track.h * vis / (total + vis)).clamp(28.0, track.h);
        let frac = if max.0 == 0 { self.top.1 as f32 / max.1.max(1) as f32 } else { self.top.0 as f32 / max.0 as f32 };
        Some(rect(track.x, track.y + (track.h - h) * frac.clamp(0.0, 1.0), track.w, h))
    }

    fn content_w(&self) -> f32 {
        self.widest.max(self.sx + self.text_rect().w)
    }

    fn hthumb(&self) -> Option<Rect> {
        if self.wrap {
            return None;
        }
        let tw = self.text_rect().w;
        let cw = self.content_w();
        if cw <= tw + 1.0 {
            return None;
        }
        let track = self.hbar().inset_xy(2.0, 2.0);
        let w = (track.w * tw / cw).clamp(28.0, track.w);
        Some(rect(track.x + (track.w - w) * (self.sx / (cw - tw)).clamp(0.0, 1.0), track.y, w, track.h))
    }

    // ---- moving and selecting ----

    fn go(&mut self, p: Pos, extend: bool) {
        let p = self.doc.clamp(p);
        if extend {
            self.doc.sel.caret = p;
        } else {
            self.doc.sel = Sel::at(p);
        }
        self.doc.seal();
    }

    /// One row up or down, keeping the caret's x.
    fn vertical(&mut self, n: isize, extend: bool) {
        let c = self.doc.sel.caret;
        let from = self.vpos(c);
        let x = self.goal.unwrap_or_else(|| self.x_of(c));
        let to = self.offset_rows(from, n);
        let target = if to == from {
            if n < 0 { pos(0, 0) } else { self.doc.end() }
        } else {
            let rows = self.rows(to.0);
            let line = self.doc.line(to.0);
            let (s, e) = (rows[to.1], rows.get(to.1 + 1).copied().unwrap_or(line.len()));
            let mut col = s + self.m.col_at(&line[s..e], x);
            if col == e && to.1 + 1 < rows.len() && col > s {
                col = line[..col].char_indices().next_back().map_or(col, |(i, _)| i);
            }
            pos(to.0, col)
        };
        self.go(target, extend);
        self.goal = Some(x);
    }

    pub fn select_all(&mut self) {
        self.doc.sel = Sel { anchor: pos(0, 0), caret: self.doc.end() };
        self.doc.seal();
    }

    /// Inserts text where the caret is (typing, pasting, the date…).
    pub fn type_text(&mut self, s: &str, kind: Kind) {
        self.doc.insert(s, kind);
        self.goal = None;
        self.ensure_visible();
    }

    pub fn copy(&self, win: Window) -> bool {
        if self.doc.sel.is_empty() {
            return false;
        }
        clip::set(win, &self.doc.selected())
    }

    pub fn paste(&mut self, win: Window) -> bool {
        match clip::get(win) {
            Some(t) if !t.is_empty() => {
                let t = crate::codec::normalize(&t).0;
                self.doc.seal();
                self.type_text(&t, Kind::Other);
                true
            }
            _ => false,
        }
    }

    /// A key for the text. Returns true if it was the editor's.
    pub fn key(&mut self, vk: u16, m: Mods, win: Window) -> bool {
        let ctrl = m.ctrl && !m.alt;
        let shift = m.shift;
        let keep_goal = matches!(vk, 0x26 | 0x28 | 0x21 | 0x22);
        let sel = self.doc.sel;
        match vk {
            // arrows
            0x25 => {
                let p = if ctrl {
                    self.doc.word_left(sel.caret)
                } else if !sel.is_empty() && !shift {
                    sel.start()
                } else {
                    self.doc.prev(sel.caret)
                };
                self.go(p, shift);
            }
            0x27 => {
                let p = if ctrl {
                    self.doc.word_right(sel.caret)
                } else if !sel.is_empty() && !shift {
                    sel.end()
                } else {
                    self.doc.next(sel.caret)
                };
                self.go(p, shift);
            }
            0x26 if ctrl => {
                self.top = self.offset_rows(self.top, -1);
                self.clamp_top();
                return true;
            }
            0x28 if ctrl => {
                self.top = self.offset_rows(self.top, 1);
                self.clamp_top();
                return true;
            }
            0x26 => self.vertical(-1, shift),
            0x28 => self.vertical(1, shift),
            // page up, page down: the screen moves with the caret
            0x21 | 0x22 => {
                let n = self.visible_rows() as isize - 1;
                let n = if vk == 0x21 { -n } else { n };
                self.top = self.offset_rows(self.top, n);
                self.vertical(n, shift);
            }
            // home: first the indentation, then the start of the line
            0x24 => {
                let c = sel.caret;
                let p = if ctrl {
                    pos(0, 0)
                } else {
                    let ind = self.doc.indent_of(c.line).len();
                    pos(c.line, if c.col == ind { 0 } else { ind })
                };
                self.go(p, shift);
            }
            0x23 => {
                let c = sel.caret;
                let p = if ctrl { self.doc.end() } else { pos(c.line, self.doc.line(c.line).len()) };
                self.go(p, shift);
            }
            0x08 => self.doc.delete_back(ctrl),
            0x2E if shift && !ctrl => {
                if self.copy(win) {
                    self.doc.delete_back(false);
                }
            }
            0x2E => self.doc.delete_fwd(ctrl),
            0x2D if ctrl => {
                self.copy(win);
                return true;
            }
            0x2D if shift => {
                self.paste(win);
            }
            0x0D => {
                let indent = self.doc.indent_of(sel.start().line).to_string();
                let indent = &indent[..indent.len().min(sel.start().col)];
                self.doc.insert(&format!("\n{indent}"), Kind::Typing);
            }
            0x09 if !ctrl => {
                if sel.start().line != sel.end().line || shift {
                    self.doc.indent_lines(shift, "\t");
                } else {
                    self.doc.insert("\t", Kind::Typing);
                }
            }
            0x41 if ctrl => self.select_all(),
            0x43 if ctrl => {
                self.copy(win);
                return true;
            }
            0x58 if ctrl => {
                if self.copy(win) {
                    self.doc.delete_back(false);
                }
            }
            0x56 if ctrl => {
                self.paste(win);
            }
            0x5A if ctrl && shift => {
                self.doc.redo();
            }
            0x5A if ctrl => {
                self.doc.undo();
            }
            0x59 if ctrl => {
                self.doc.redo();
            }
            _ => return false,
        }
        if !keep_goal {
            self.goal = None;
        }
        self.blink_on = true;
        self.ensure_visible();
        true
    }

    /// A typed character.
    pub fn char(&mut self, ch: char) {
        let mut b = [0u8; 4];
        self.type_text(ch.encode_utf8(&mut b), Kind::Typing);
        self.blink_on = true;
    }

    // ---- the mouse ----

    pub fn on_bar(&self, p: Pt) -> bool {
        self.vbar().contains(p) || (!self.wrap && self.hbar().contains(p))
    }

    pub fn mouse_down(&mut self, p: Pt, clicks: u8, m: Mods) -> bool {
        if !self.area.contains(p) {
            return false;
        }
        if self.vbar().contains(p) {
            if let Some(t) = self.vthumb() {
                let grab = if t.contains(p) { p.y - t.y } else { t.h / 2.0 };
                self.drag = Drag::VBar { grab };
                self.mouse_move(p);
            }
            return true;
        }
        if !self.wrap && self.hbar().contains(p) {
            if let Some(t) = self.hthumb() {
                let grab = if t.contains(p) { p.x - t.x } else { t.w / 2.0 };
                self.drag = Drag::HBar { grab };
                self.mouse_move(p);
            }
            return true;
        }
        self.goal = None;
        self.blink_on = true;
        let at = self.hit(p);
        // the gutter selects whole lines
        if self.numbers && p.x < self.area.x + self.gutter_w() {
            let start = pos(at.line, 0);
            let end = if at.line + 1 < self.doc.line_count() { pos(at.line + 1, 0) } else { pos(at.line, self.doc.line(at.line).len()) };
            self.doc.sel = Sel { anchor: start, caret: end };
            self.drag = Drag::Text { words: None, lines: Some(at.line) };
            self.doc.seal();
            return true;
        }
        let triple = clicks == 1 && self.last_double.is_some_and(|(t, q)| t.elapsed().as_millis() < 500 && q.line == at.line);
        if triple {
            let end = if at.line + 1 < self.doc.line_count() { pos(at.line + 1, 0) } else { pos(at.line, self.doc.line(at.line).len()) };
            self.doc.sel = Sel { anchor: pos(at.line, 0), caret: end };
            self.drag = Drag::Text { words: None, lines: Some(at.line) };
            self.last_double = None;
        } else if clicks == 2 {
            let (a, b) = self.doc.word_at(at);
            self.doc.sel = Sel { anchor: a, caret: b };
            self.drag = Drag::Text { words: Some((a, b)), lines: None };
            self.last_double = Some((Instant::now(), at));
        } else {
            if m.shift {
                self.doc.sel.caret = at;
            } else {
                self.doc.sel = Sel::at(at);
            }
            self.drag = Drag::Text { words: None, lines: None };
        }
        self.doc.seal();
        true
    }

    /// Returns true if something changed on screen.
    pub fn mouse_move(&mut self, p: Pt) -> bool {
        let hot = self.on_bar(p);
        let mut changed = hot != self.hover_bar;
        self.hover_bar = hot;
        match self.drag {
            Drag::None => return changed,
            Drag::VBar { grab } => {
                let (Some(t), max) = (self.vthumb(), self.max_top()) else { return changed };
                let track = self.vbar().inset_xy(2.0, 2.0);
                let frac = ((p.y - grab - track.y) / (track.h - t.h).max(1.0)).clamp(0.0, 1.0);
                self.top = if max.0 == 0 { (0, (frac * max.1 as f32).round() as usize) } else { ((frac * max.0 as f32).round() as usize, 0) };
                self.clamp_top();
            }
            Drag::HBar { grab } => {
                let Some(t) = self.hthumb() else { return changed };
                let track = self.hbar().inset_xy(2.0, 2.0);
                let frac = ((p.x - grab - track.x) / (track.w - t.w).max(1.0)).clamp(0.0, 1.0);
                self.sx = frac * (self.content_w() - self.text_rect().w);
            }
            Drag::Text { words, lines } => {
                let tr = self.text_rect();
                // past the top or the bottom: the text scrolls
                if p.y < tr.y {
                    self.top = self.offset_rows(self.top, -1);
                } else if p.y > tr.bottom() {
                    self.top = self.offset_rows(self.top, 1);
                }
                self.clamp_top();
                if !self.wrap {
                    if p.x < tr.x && self.sx > 0.0 {
                        self.sx = (self.sx - 20.0).max(0.0);
                    } else if p.x > tr.right() {
                        self.sx += 20.0;
                    }
                }
                let at = self.hit(p);
                if let Some(l) = lines {
                    let (a, b) = (l.min(at.line), l.max(at.line));
                    let end = if b + 1 < self.doc.line_count() { pos(b + 1, 0) } else { pos(b, self.doc.line(b).len()) };
                    self.doc.sel = if at.line < l { Sel { anchor: end, caret: pos(a, 0) } } else { Sel { anchor: pos(a, 0), caret: end } };
                } else if let Some((wa, wb)) = words {
                    let (a, b) = self.doc.word_at(at);
                    self.doc.sel = if at < wa { Sel { anchor: wb, caret: a } } else { Sel { anchor: wa, caret: b.max(wb) } };
                } else {
                    self.doc.sel.caret = at;
                }
            }
        }
        changed = true;
        changed
    }

    pub fn mouse_up(&mut self) -> bool {
        let was = self.drag != Drag::None;
        self.drag = Drag::None;
        was
    }

    pub fn dragging(&self) -> bool {
        self.drag != Drag::None
    }

    /// The mouse wheel (in notches): rows, or columns with Shift.
    pub fn wheel(&mut self, dx: f32, dy: f32, shift: bool) {
        if (shift || dx != 0.0) && !self.wrap {
            let d = if dx != 0.0 { dx } else { -dy };
            self.sx = (self.sx + d * self.m.adv('M') * 6.0).clamp(0.0, (self.content_w() - self.text_rect().w).max(0.0));
            return;
        }
        let n = (-dy * 3.0).round() as isize;
        self.top = self.offset_rows(self.top, n);
        self.clamp_top();
    }

    // ---- drawing ----

    /// Draws a piece of a row from x `x0`, between the screen's `from` and `to` (x in the row).
    #[allow(clippy::too_many_arguments)]
    fn draw_seg(&self, rt: &ID2D1RenderTarget, brush: &ID2D1SolidColorBrush, seg: &str, x0: f32, y: f32, from: f32, to: f32) {
        let (Some(f), Some(dwf)) = (&self.m.format, &self.m.dw) else { return };
        let mut x = 0.0;
        let mut run: Vec<u16> = Vec::with_capacity(128);
        let mut run_x = 0.0;
        let flush = |run: &mut Vec<u16>, run_x: f32| {
            if run.is_empty() {
                return;
            }
            unsafe {
                if let Ok(l) = dwf.CreateTextLayout(run, f, 100_000.0, self.m.line_h) {
                    if let Some(t) = &self.m.typography {
                        let _ = l.SetTypography(t, DWRITE_TEXT_RANGE { startPosition: 0, length: run.len() as u32 });
                    }
                    rt.DrawTextLayout(Vector2 { X: x0 + run_x, Y: y + self.m.text_y }, &l, brush, D2D1_DRAW_TEXT_OPTIONS(4));
                }
            }
            run.clear();
        };
        for c in seg.chars() {
            let w = self.m.w_at(c, x);
            if x + w < from {
                x += w;
                continue;
            }
            if x > to {
                break;
            }
            if c == '\t' || run.len() > 400 {
                flush(&mut run, run_x);
            }
            if c != '\t' {
                if run.is_empty() {
                    run_x = x;
                }
                let mut b = [0u16; 2];
                run.extend_from_slice(shown(c).encode_utf16(&mut b));
            }
            x += w;
        }
        flush(&mut run, run_x);
    }

    /// Draws the editor in `area`. `found`: the search to highlight, and the match that is selected.
    pub fn paint(&mut self, c: &mut Canvas, area: Rect, k: &Colors, found: Option<&Matcher>, current: Option<(Pos, Pos)>) {
        self.area = area;
        self.clamp_top();
        c.fill(area, k.bg);
        let g = self.gutter_w();
        if g > 0.0 {
            c.fill(rect(area.x, area.y, g, area.h), k.gutter);
        }
        let tr = self.text_rect();
        let lh = self.m.line_h;
        let rt = c.target();
        let brushes = unsafe {
            (rt.CreateSolidColorBrush(&col(k.text), None), rt.CreateSolidColorBrush(&col(k.numbers), None), rt.CreateSolidColorBrush(&col(k.numbers_cur), None))
        };
        let (Ok(text_b), Ok(num_b), Ok(cur_b)) = brushes else { return };
        let sel = self.doc.sel;
        let (ss, se) = (sel.start(), sel.end());
        let caret = sel.caret;
        let clip = rect(area.x + g, area.y, area.w - g - BAR_W, area.h - if self.wrap { 0.0 } else { BAR_W });
        let (from, to) = (self.sx - 8.0, self.sx + tr.w + 8.0);
        let x0 = tr.x - self.sx;
        self.caret_rect = None;
        let mut at = Some(self.top);
        let mut y = tr.y;
        let mut matches: Option<(usize, Vec<(usize, usize)>)> = None;
        let space = self.m.adv(' ');
        while let Some((l, r)) = at {
            if y > area.bottom() {
                break;
            }
            let rows = self.rows(l);
            let line = self.doc.line(l);
            let (s, e) = (rows[r], rows.get(r + 1).copied().unwrap_or(line.len()));
            let seg = &line[s..e];
            let last_row = r + 1 == rows.len();
            c.clip(clip);
            if l == caret.line && sel.is_empty() && self.focused {
                c.fill(rect(clip.x, y, clip.w, lh), k.line);
            }
            // matches of the search
            if let Some(m) = found {
                if matches.as_ref().is_none_or(|(ml, _)| *ml != l) {
                    matches = Some((l, m.in_line(line)));
                }
                for &(ma, mb) in &matches.as_ref().expect("set above").1 {
                    if mb <= s || ma >= e {
                        continue;
                    }
                    let (a, b) = (ma.max(s), mb.min(e));
                    let xa = self.m.width(&line[s..a]);
                    let xb = self.m.width(&line[s..b]);
                    let is_cur = current == Some((pos(l, ma), pos(l, mb)));
                    c.fill_round(rect(x0 + xa - 1.0, y + 1.0, xb - xa + 2.0, lh - 2.0), 3.0, if is_cur { k.found_cur } else { k.found });
                }
            }
            // the selection on this row
            let row_a = pos(l, s);
            let row_b = pos(l, e);
            if !sel.is_empty() && ss <= row_b && se >= row_a {
                let a = if ss > row_a { ss.col } else { s };
                let b = if se < row_b { se.col } else { e };
                let xa = self.m.width(&line[s..a]);
                let mut xb = self.m.width(&line[s..b.max(a)]);
                // the line break is selected too: a little more
                if last_row && se.line > l {
                    xb += space;
                }
                if xb > xa {
                    c.fill(rect(x0 + xa, y, xb - xa, lh), k.sel);
                }
            }
            self.draw_seg(&rt, &text_b, seg, x0, y, from, to);
            if !self.wrap {
                self.widest = self.widest.max(self.line_width(l) + 40.0);
            }
            // the caret
            if l == caret.line && caret.col >= s && (caret.col < e || last_row) {
                let x = x0 + self.m.width(&line[s..caret.col.min(e)]);
                let cr = rect(x.round() - 1.0, y, 2.0, lh);
                self.caret_rect = Some(cr);
                if self.focused && self.blink_on {
                    c.fill(cr, k.caret);
                }
            }
            c.unclip();
            // its number
            if g > 0.0 && r == 0 {
                let n = (l + 1).to_string();
                let w = self.m.width(&n);
                c.clip(rect(area.x, area.y, g, area.h));
                self.draw_seg(&rt, if l == caret.line { &cur_b } else { &num_b }, &n, area.x + g - 14.0 - w, y, -1.0, 1e9);
                c.unclip();
            }
            y += lh;
            at = self.next_row((l, r));
        }
        // the scroll bars
        let thumb = |c: &mut Canvas, t: Rect| c.fill_round(t.inset_xy(2.0, 0.0), 4.0, if self.hover_bar || self.dragging() { k.bar_hot } else { k.bar });
        if let Some(t) = self.vthumb() {
            thumb(c, t);
        }
        if let Some(t) = self.hthumb() {
            thumb(c, rect(t.x, t.y + 2.0, t.w, t.h - 4.0).inset_xy(-2.0, 0.0));
        }
    }
}
