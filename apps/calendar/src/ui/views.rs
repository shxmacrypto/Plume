//! The four views: month (a grid of six weeks), week and day (an hour grid with an all-day row and the now line),
//! agenda (what is coming, day by day).

use super::{App, Drag, Id, OccRef, View, color, paint::event_colors};
use crate::{
    date::{DAY, Date, Wall},
    i18n::{self, fill, tr},
    model::Occ,
    tz,
};
use plume_ui::{Align, Canvas, Color, Font, PaintCx, Rect, rect};

pub const HOUR_H: f32 = 48.0;
pub const GUTTER: f32 = 58.0;
pub const AGENDA_DAYS: i64 = 60;
const LINE_H: f32 = 21.0;

/// Lanes for bars spanning days: each item (first column, last column) gets the first lane free on all its columns.
fn lanes(spans: &[(usize, usize)], cols: usize) -> Vec<usize> {
    let mut used: Vec<Vec<bool>> = Vec::new();
    let mut out = Vec::new();
    for &(a, b) in spans {
        let mut lane = 0;
        loop {
            if lane == used.len() {
                used.push(vec![false; cols]);
            }
            if used[lane][a..=b].iter().all(|u| !u) {
                for u in &mut used[lane][a..=b] {
                    *u = true;
                }
                break;
            }
            lane += 1;
        }
        out.push(lane);
    }
    out
}

/// Side-by-side columns for overlapping timed events: (column, columns in its group) for each.
fn columns(items: &[(i64, i64)]) -> Vec<(usize, usize)> {
    let mut out = vec![(0, 1); items.len()];
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| (items[i].0, -(items[i].1)));
    let mut group: Vec<usize> = Vec::new();
    let mut ends: Vec<i64> = Vec::new();
    let mut group_end = i64::MIN;
    let flush = |group: &mut Vec<usize>, ends: &mut Vec<i64>, out: &mut Vec<(usize, usize)>| {
        let n = ends.len().max(1);
        for &g in group.iter() {
            out[g].1 = n;
        }
        group.clear();
        ends.clear();
    };
    for i in order {
        let (s, e) = items[i];
        let e = e.max(s + 900);
        if s >= group_end && !group.is_empty() {
            flush(&mut group, &mut ends, &mut out);
        }
        let col = match ends.iter().position(|&end| end <= s) {
            Some(c) => {
                ends[c] = e;
                c
            }
            None => {
                ends.push(e);
                ends.len() - 1
            }
        };
        out[i].0 = col;
        group.push(i);
        group_end = group_end.max(e);
        if group.len() == 1 {
            group_end = e;
        }
    }
    flush(&mut group, &mut ends, &mut out);
    out
}

impl App {
    fn occ_is_bar(&self, o: &Occ) -> bool {
        o.all_day || o.end - o.start >= DAY
    }

    /// An event as a filled bar (all-day, several days).
    fn bar(&mut self, c: &mut Canvas, r: Rect, i: usize, text: &str, dim: bool) {
        let t = self.t.clone();
        let o = &self.occs[i];
        let col = color(&self.store.color(o));
        let (fill, ink) = event_colors(&t, col);
        let hot = self.hot(&Id::Occ(i));
        let a = if dim { 0.4 } else { 1.0 };
        c.fill_round(r, 5.0, if hot { fill.mix(col, 0.25) } else { fill }.fade(a));
        c.fill_round(rect(r.x, r.y, 3.0, r.h), 1.5, col.fade(a));
        c.text(text, rect(r.x + 8.0, r.y, r.w - 10.0, r.h), Font::SmallBold, ink.fade(a), Align::Left);
        self.hits.add(r, Id::Occ(i));
    }

    // ───────────────────────── month ─────────────────────────

    pub(super) fn paint_month(&mut self, c: &mut Canvas, area: Rect) {
        let t = self.t.clone();
        let ws = self.set.week_start();
        let first = self.range().0;
        let wn = if self.set.week_numbers { 34.0 } else { 0.0 };
        let mut r = area.inset_xy(12.0, 0.0);
        r.h -= 12.0;
        let head = r.cut_top(28.0);
        let cw = (r.w - wn) / 7.0;
        let ch = r.h / 6.0;
        for k in 0..7 {
            let wd = (ws + k) % 7;
            c.text(&i18n::day_short(wd), rect(head.x + wn + k as f32 * cw + 10.0, head.y, cw - 10.0, head.h), Font::SmallBold, t.t3, Align::Left);
        }
        let grid = rect(r.x + wn, r.y, cw * 7.0, ch * 6.0);
        self.geo.month = grid;
        self.geo.cell_w = cw;
        self.geo.cell_h = ch;
        self.geo.month_first = Some(first);
        self.geo.days.clear();
        self.geo.grid = Rect::default();
        self.geo.allday = Rect::default();
        c.fill_round(grid, 12.0, t.card);
        c.stroke_round(grid, 12.0, t.ring, 1.0);
        let today = self.today();
        let z = self.zone();
        let drag = self.drag_days();
        let dragging = match self.drag {
            Drag::MoveDays { occ, .. } => Some(occ),
            _ => None,
        };
        let h24 = self.h24();
        for row in 0..6 {
            let y = grid.y + row as f32 * ch;
            if row > 0 {
                c.hline(grid.x, y, grid.w, t.sep);
            }
            let d0 = first.add_days(row as i64 * 7);
            if wn > 0.0 {
                c.text(&d0.add_days(3).iso_week().to_string(), rect(r.x, y + 6.0, wn - 6.0, 18.0), Font::Small, t.t4, Align::Center);
            }
            for col in 0..7 {
                let d = d0.add_days(col);
                let cell = rect(grid.x + col as f32 * cw, y, cw, ch);
                if col > 0 && row == 0 {
                    c.vline(cell.x, grid.y, grid.h, t.sep);
                }
                let id = Id::Cell(d.days());
                if let Some((a, b)) = drag
                    && d >= a
                    && d <= b
                {
                    c.fill(cell.inset(1.0), t.accent.fade(0.16));
                } else if d == self.selected {
                    c.fill(cell.inset(1.0), t.accent.fade(if t.dark { 0.10 } else { 0.07 }));
                } else if self.hot(&id) {
                    c.fill(cell.inset(1.0), t.hover.fade(0.5));
                }
                let num = rect(cell.x + 6.0, cell.y + 5.0, 24.0, 24.0);
                let label = if d.d == 1 { format!("{} {}", d.d, i18n::month_short(d.m)) } else { d.d.to_string() };
                if d == today {
                    let w = (c.text_width(&label, Font::SmallBold) + 14.0).max(24.0);
                    c.fill_round(rect(num.x, num.y, w, 24.0), 12.0, t.accent);
                    c.text(&label, rect(num.x, num.y, w, 24.0), Font::SmallBold, Color::hex(0xffffff), Align::Center);
                } else {
                    let ink = if d.m != self.anchor.m { t.t4 } else { t.t2 };
                    c.text(&label, rect(num.x + 6.0, num.y, cw - 16.0, 24.0), if d.d == 1 { Font::SmallBold } else { Font::Small }, ink, Align::Left);
                }
                self.hits.add(cell, id);
            }
            // events of the week
            let week_from = tz::day_start(&z, d0);
            let week_to = tz::day_start(&z, d0.add_days(7));
            let col_of = |tt: i64| -> i64 { tz::date_at(&z, tt).days() - d0.days() };
            let mut bars: Vec<(usize, usize, usize)> = Vec::new();
            let mut singles: Vec<Vec<usize>> = vec![Vec::new(); 7];
            for (i, o) in self.occs.iter().enumerate() {
                if o.end <= week_from || o.start >= week_to {
                    continue;
                }
                let a = col_of(o.start).clamp(0, 6) as usize;
                let b = col_of((o.end - 1).max(o.start)).clamp(0, 6) as usize;
                if self.occ_is_bar(o) || a != b {
                    bars.push((i, a, b));
                } else {
                    singles[a].push(i);
                }
            }
            let ln = lanes(&bars.iter().map(|x| (x.1, x.2)).collect::<Vec<_>>(), 7);
            let cap = (((ch - 32.0) / LINE_H).floor() as usize).max(1);
            // lines needed per column
            let mut need = [0usize; 7];
            for (k, (_, a, b)) in bars.iter().enumerate() {
                for n in need.iter_mut().take(*b + 1).skip(*a) {
                    *n = (*n).max(ln[k] + 1);
                }
            }
            let mut total = need;
            for col in 0..7 {
                total[col] = need[col] + singles[col].len();
            }
            let visible: Vec<usize> = (0..7).map(|col| if total[col] > cap { cap - 1 } else { cap }).collect();
            let top = y + 31.0;
            // bars, cut where a column has no room
            for (k, (i, a, b)) in bars.iter().enumerate() {
                let lane = ln[k];
                let mut col = *a;
                while col <= *b {
                    if lane >= visible[col] {
                        col += 1;
                        continue;
                    }
                    let start = col;
                    while col < *b && lane < visible[col + 1] {
                        col += 1;
                    }
                    let o = &self.occs[*i];
                    let ev = self.store.event(o);
                    let mut text = ev.title.clone();
                    if !o.all_day && start == *a && col_of(o.start) >= 0 {
                        text = format!("{} {}", i18n::time(z.to_wall(o.start).secs, h24), text);
                    }
                    let br = rect(grid.x + start as f32 * cw + 4.0, top + lane as f32 * LINE_H, (col - start + 1) as f32 * cw - 8.0, LINE_H - 3.0);
                    self.bar(c, br, *i, &text, dragging == Some(*i));
                    col += 1;
                }
            }
            // single-day events
            for col in 0..7 {
                let mut shown = 0;
                for (line, &i) in (need[col]..visible[col].max(need[col])).zip(singles[col].iter()) {
                    let o = self.occs[i].clone();
                    let ev = self.store.event(&o).clone();
                    let cc = color(&self.store.color(&o));
                    let rr = rect(grid.x + col as f32 * cw + 4.0, top + line as f32 * LINE_H, cw - 8.0, LINE_H - 3.0);
                    let dim = dragging == Some(i);
                    if self.hot(&Id::Occ(i)) {
                        c.fill_round(rr, 5.0, t.hover);
                    }
                    c.fill_round(rect(rr.x + 4.0, rr.y + rr.h / 2.0 - 3.5, 7.0, 7.0), 3.5, cc.fade(if dim { 0.4 } else { 1.0 }));
                    let tm = i18n::time(z.to_wall(o.start).secs, h24);
                    let tw = c.text_width(&tm, Font::Small) + 6.0;
                    c.text(&tm, rect(rr.x + 15.0, rr.y, tw, rr.h), Font::Small, t.t3, Align::Left);
                    c.text(&ev.title, rect(rr.x + 15.0 + tw, rr.y, rr.w - 17.0 - tw, rr.h), Font::Small, if dim { t.t3 } else { t.t1 }, Align::Left);
                    self.hits.add(rr, Id::Occ(i));
                    shown += 1;
                }
                let hidden = total[col] - need[col].min(visible[col]) - shown;
                if total[col] > cap && hidden > 0 {
                    let d = d0.add_days(col as i64);
                    let rr = rect(grid.x + col as f32 * cw + 4.0, top + (cap - 1) as f32 * LINE_H, cw - 8.0, LINE_H - 3.0);
                    let id = Id::More(d.days());
                    if self.hot(&id) {
                        c.fill_round(rr, 5.0, t.hover);
                    }
                    c.text(&fill(tr("{} more"), &[&hidden.to_string()]), rr.inset_xy(6.0, 0.0), Font::SmallBold, t.t3, Align::Left);
                    self.hits.add(rr, id);
                }
            }
        }
    }

    // ───────────────────────── week and day ─────────────────────────

    pub(super) fn paint_week(&mut self, c: &mut Canvas, area: Rect, cx: &mut PaintCx) {
        let t = self.t.clone();
        let (a, b) = self.range();
        let days: Vec<Date> = (0..(b.days() - a.days())).map(|k| a.add_days(k)).collect();
        let n = days.len();
        let z = self.zone();
        let h24 = self.h24();
        let today = self.today();
        let now = self.now();
        let mut r = area.inset_xy(12.0, 0.0);
        r.h -= 12.0;
        c.fill_round(r, 12.0, t.card);
        c.stroke_round(r, 12.0, t.ring, 1.0);
        let inner = r.inset(1.0);
        let col_x = inner.x + GUTTER;
        let col_w = (inner.w - GUTTER - 10.0) / n as f32;
        // head: day names and numbers
        let head = rect(inner.x, inner.y, inner.w, 58.0);
        if self.set.week_numbers {
            c.text(
                &format!("{} {}", tr("W"), a.add_days(3.min(n as i64 - 1)).iso_week()),
                rect(head.x, head.y, GUTTER, head.h),
                Font::SmallBold,
                t.t3,
                Align::Center,
            );
        }
        for (k, d) in days.iter().enumerate() {
            let x = col_x + k as f32 * col_w;
            let cell = rect(x, head.y, col_w, head.h);
            let id = Id::DayHead(d.days());
            if self.hot(&id) {
                c.fill_round(cell.inset(4.0), 10.0, t.hover);
            }
            let name = if n == 1 { i18n::cap(i18n::day(d.weekday())) } else { i18n::day_short(d.weekday()) };
            c.text(&name, rect(x, head.y + 6.0, col_w, 18.0), Font::Small, if *d == today { t.accent } else { t.t3 }, Align::Center);
            let num = rect(x + col_w / 2.0 - 15.0, head.y + 25.0, 30.0, 28.0);
            if *d == today {
                c.fill_round(num, 14.0, t.accent);
            }
            c.text(&d.d.to_string(), num, Font::Large, if *d == today { Color::hex(0xffffff) } else { t.t1 }, Align::Center);
            if n > 1 {
                self.hits.add(cell, id);
            }
        }
        // all-day row
        let day_from = |d: Date| tz::day_start(&z, d);
        let mut bars: Vec<(usize, usize, usize)> = Vec::new();
        for (i, o) in self.occs.iter().enumerate() {
            if !self.occ_is_bar(o) {
                continue;
            }
            let ca = (tz::date_at(&z, o.start).days() - a.days()).clamp(0, n as i64 - 1) as usize;
            let cb = (tz::date_at(&z, (o.end - 1).max(o.start)).days() - a.days()).clamp(0, n as i64 - 1) as usize;
            bars.push((i, ca, cb));
        }
        let ln = lanes(&bars.iter().map(|x| (x.1, x.2)).collect::<Vec<_>>(), n);
        let lanes_n = ln.iter().map(|l| l + 1).max().unwrap_or(0);
        let shown_lanes = lanes_n.min(3);
        let more = lanes_n > 3;
        let ad_h = (shown_lanes.max(1) as f32 * 22.0 + 8.0 + if more { 18.0 } else { 0.0 }).max(30.0);
        let allday = rect(inner.x, head.bottom(), inner.w, ad_h);
        c.hline(inner.x, allday.y, inner.w, t.sep);
        c.text(tr("All day"), rect(allday.x + 4.0, allday.y + 4.0, GUTTER - 8.0, 20.0), Font::Small, t.t3, Align::Center);
        let drag_days = self.drag_days();
        for (k, d) in days.iter().enumerate() {
            let cell = rect(col_x + k as f32 * col_w, allday.y, col_w, allday.h);
            if let Some((da, db)) = drag_days
                && *d >= da
                && *d <= db
            {
                c.fill(cell.inset(1.0), t.accent.fade(0.16));
            }
            self.hits.add(cell, Id::AllDay(d.days()));
        }
        let moving = match self.drag {
            Drag::MoveDays { occ, .. } | Drag::Move { occ, .. } | Drag::Resize { occ, .. } => Some(occ),
            _ => None,
        };
        for (k, (i, ca, cb)) in bars.iter().enumerate() {
            if ln[k] >= 3 {
                continue;
            }
            let br = rect(col_x + *ca as f32 * col_w + 3.0, allday.y + 4.0 + ln[k] as f32 * 22.0, (cb - ca + 1) as f32 * col_w - 6.0, 19.0);
            let title = self.store.event(&self.occs[*i]).title.clone();
            self.bar(c, br, *i, &title, moving == Some(*i));
        }
        if more {
            c.text(&fill(tr("{} more"), &[&(lanes_n - 3).to_string()]), rect(col_x + 6.0, allday.bottom() - 20.0, 200.0, 16.0), Font::Small, t.t3, Align::Left);
        }
        // the hour grid
        let grid = rect(inner.x, allday.bottom(), inner.w, inner.bottom() - allday.bottom());
        c.hline(inner.x, grid.y, inner.w, t.sep);
        let max = (24.0 * HOUR_H - grid.h).max(0.0);
        if let Some(s) = self.scroll_to.take() {
            self.scroll = s;
        }
        self.scroll = self.scroll.clamp(0.0, max);
        self.geo.grid = grid;
        self.geo.col_x = col_x;
        self.geo.col_w = col_w;
        self.geo.days = days.clone();
        self.geo.hour_h = HOUR_H;
        self.geo.allday = allday;
        self.geo.month_first = None;
        self.geo.month = Rect::default();
        let y0 = grid.y - self.scroll;
        c.clip(grid);
        for (k, d) in days.iter().enumerate() {
            let x = col_x + k as f32 * col_w;
            if *d == today && n > 1 {
                c.fill(rect(x, grid.y, col_w, grid.h), t.accent.fade(0.04));
            }
            self.hits.add(rect(x, grid.y, col_w, grid.h), Id::Grid(d.days()));
        }
        for h in 0..24 {
            let y = y0 + h as f32 * HOUR_H;
            if h > 0 {
                c.hline(col_x - 6.0, y, inner.right() - col_x - 4.0, t.sep);
                c.text(&i18n::hour_label(h, h24), rect(inner.x, y - 9.0, GUTTER - 12.0, 18.0), Font::Small, t.t3, Align::Right);
            }
            c.hline(col_x, y + HOUR_H / 2.0, inner.right() - col_x - 4.0, t.sep.fade(0.45));
        }
        for k in 0..n {
            c.vline(col_x + k as f32 * col_w, grid.y, grid.h, t.sep);
        }
        // timed events, per day
        let y_of = |w: Wall| y0 + w.secs as f32 / 3600.0 * HOUR_H;
        for (k, d) in days.iter().enumerate() {
            let x = col_x + k as f32 * col_w;
            let ds = day_from(*d);
            let de = day_from(d.add_days(1));
            let items: Vec<usize> = (0..self.occs.len())
                .filter(|&i| !self.occ_is_bar(&self.occs[i]) && self.occs[i].start < de && self.occs[i].end.max(self.occs[i].start + 1) > ds)
                .collect();
            let spans: Vec<(i64, i64)> = items.iter().map(|&i| (self.occs[i].start.max(ds), self.occs[i].end.min(de))).collect();
            let cols = columns(&spans);
            for (j, &i) in items.iter().enumerate() {
                let o = self.occs[i].clone();
                let (s, e) = spans[j];
                let top = y_of(z.to_wall(s));
                let bottom = if e >= de { y0 + 24.0 * HOUR_H } else { y_of(z.to_wall(e)) };
                let (cl, cn) = cols[j];
                let w = (col_w - 6.0) / cn as f32;
                let br = rect(x + 2.0 + cl as f32 * w, top + 1.0, w - 2.0, (bottom - top - 2.0).max(18.0));
                self.block(c, br, i, &o, moving == Some(i), e < de || e == o.end);
            }
            // the now line
            if *d == today && now >= ds && now < de {
                let y = y_of(z.to_wall(now));
                c.fill(rect(x, y - 1.0, col_w, 2.0), t.red);
                c.fill_round(rect(x - 5.0, y - 5.0, 10.0, 10.0), 5.0, t.red);
                cx.animate = false;
            }
        }
        // what a drag is about to make
        let ghost = match self.drag.clone() {
            Drag::Move { occ, start, .. } => self.occs.get(occ).map(|o| (start, start + (o.end - o.start), Some(occ))),
            Drag::Resize { occ, end } => self.occs.get(occ).map(|o| (o.start, end, Some(occ))),
            _ => self.drag_range().map(|(s, e)| (s, e, None)),
        };
        if let Some((s, e, occ)) = ghost {
            let sd = tz::date_at(&z, s);
            if let Some(k) = days.iter().position(|d| *d == sd) {
                let x = col_x + k as f32 * col_w;
                let top = y_of(z.to_wall(s));
                let ew = z.to_wall(e);
                let bottom = if ew.date > sd { y0 + 24.0 * HOUR_H } else { y_of(ew) };
                let br = rect(x + 2.0, top + 1.0, col_w - 6.0, (bottom - top - 2.0).max(18.0));
                let col = occ.and_then(|i| self.occs.get(i)).map(|o| color(&self.store.color(o))).unwrap_or(t.accent);
                let (fill_c, ink) = event_colors(&t, col);
                c.fill_round(br, 7.0, fill_c);
                c.stroke_round(br, 7.0, col, 1.5);
                let title = occ.and_then(|i| self.occs.get(i)).map(|o| self.store.event(o).title.clone()).unwrap_or_else(|| tr("New event").to_string());
                c.text(&title, rect(br.x + 8.0, br.y + 3.0, br.w - 12.0, 18.0), Font::SmallBold, ink, Align::Left);
                let times = format!("{} – {}", i18n::time(z.to_wall(s).secs, h24), i18n::time(ew.secs, h24));
                c.text(&times, rect(br.x + 8.0, br.y + 20.0, br.w - 12.0, 16.0), Font::Small, ink, Align::Left);
            }
        }
        c.unclip();
        // the scroll position
        if max > 0.0 {
            let th = (grid.h * grid.h / (24.0 * HOUR_H)).max(30.0);
            let ty = grid.y + (grid.h - th) * self.scroll / max;
            c.fill_round(rect(inner.right() - 6.0, ty, 4.0, th), 2.0, t.fill2);
        }
    }

    /// A timed event in the hour grid.
    fn block(&mut self, c: &mut Canvas, r: Rect, i: usize, o: &Occ, dim: bool, ends_here: bool) {
        let t = self.t.clone();
        let ev = self.store.event(o).clone();
        let col = color(&self.store.color(o));
        let (fill, ink) = event_colors(&t, col);
        let a = if dim { 0.35 } else { 1.0 };
        let hot = self.hot(&Id::Occ(i)) || self.hot(&Id::Resize(i));
        let selected = self.peek.as_ref().is_some_and(|p| p.occ == OccRef::of(o));
        c.fill_round(r, 7.0, if hot || selected { fill.mix(col, 0.22) } else { fill }.fade(a));
        c.fill_round(rect(r.x, r.y, 3.5, r.h), 1.75, col.fade(a));
        if selected {
            c.stroke_round(r, 7.0, col, 1.5);
        }
        let z = self.zone();
        let h24 = self.h24();
        let times = format!("{} – {}", i18n::time(z.to_wall(o.start).secs, h24), i18n::time(z.to_wall(o.end).secs, h24));
        c.clip(r);
        if r.h < 34.0 {
            let line = format!("{}, {}", ev.title, i18n::time(z.to_wall(o.start).secs, h24));
            c.text(&line, rect(r.x + 8.0, r.y, r.w - 10.0, r.h.min(20.0)), Font::SmallBold, ink.fade(a), Align::Left);
        } else {
            // two lines of title when the block is tall and wide enough, else one with "…"
            let title_h = if r.h >= 72.0 && r.w >= 110.0 { c.text_height(&ev.title, r.w - 12.0, Font::SmallBold).clamp(16.0, 34.0) } else { 16.0 };
            if title_h > 18.0 {
                c.clip(rect(r.x + 8.0, r.y + 3.0, r.w - 12.0, title_h));
                c.text_block(&ev.title, rect(r.x + 8.0, r.y + 3.0, r.w - 12.0, title_h), Font::SmallBold, ink.fade(a));
                c.unclip();
            } else {
                c.text(&ev.title, rect(r.x + 8.0, r.y + 3.0, r.w - 12.0, 16.0), Font::SmallBold, ink.fade(a), Align::Left);
            }
            let mut y = r.y + 4.0 + title_h;
            c.text(&times, rect(r.x + 8.0, y, r.w - 12.0, 16.0), Font::Small, ink.fade(0.8 * a), Align::Left);
            y += 16.0;
            if !ev.location.is_empty() && y + 16.0 < r.bottom() {
                c.text(&ev.location, rect(r.x + 8.0, y, r.w - 12.0, 16.0), Font::Small, ink.fade(0.7 * a), Align::Left);
            }
        }
        c.unclip();
        self.hits.add(r, Id::Occ(i));
        if ends_here && !self.store.cals[o.cal].read_only() {
            self.hits.add(rect(r.x, r.bottom() - 6.0, r.w, 7.0), Id::Resize(i));
        }
    }

    // ───────────────────────── agenda ─────────────────────────

    pub(super) fn paint_agenda(&mut self, c: &mut Canvas, area: Rect) {
        let t = self.t.clone();
        let z = self.zone();
        let h24 = self.h24();
        let today = self.today();
        let (a, _) = self.range();
        let mut r = area.inset_xy(12.0, 0.0);
        r.h -= 12.0;
        self.geo.agenda = r;
        self.geo.days.clear();
        self.geo.month_first = None;
        self.geo.grid = Rect::default();
        self.geo.month = Rect::default();
        let w = r.w.min(820.0);
        let x = r.x + (r.w - w) / 2.0;
        let mut y = r.y - self.agenda_scroll;
        c.clip(r);
        let mut any = false;
        for k in 0..AGENDA_DAYS {
            let d = a.add_days(k);
            let ds = tz::day_start(&z, d);
            let de = tz::day_start(&z, d.add_days(1));
            let items: Vec<usize> = (0..self.occs.len()).filter(|&i| self.occs[i].start < de && self.occs[i].end.max(self.occs[i].start + 1) > ds).collect();
            if items.is_empty() && d != today {
                continue;
            }
            any |= !items.is_empty();
            // the day's head
            let head = if d == today { format!("{} · {}", tr("Today"), i18n::long_date(d)) } else { i18n::cap(&i18n::long_date(d)) };
            c.text(&head, rect(x + 6.0, y + 10.0, w - 12.0, 24.0), Font::BodyBold, if d == today { t.accent } else { t.t1 }, Align::Left);
            y += 40.0;
            let card = rect(x, y, w, (items.len().max(1)) as f32 * 52.0 + 8.0);
            if card.bottom() > r.y && card.y < r.bottom() {
                c.fill_round(card, 12.0, t.card);
                c.stroke_round(card, 12.0, t.ring, 1.0);
            }
            y += 4.0;
            if items.is_empty() {
                c.text(tr("Nothing planned"), rect(x + 18.0, y, w - 36.0, 52.0), Font::Body, t.t3, Align::Left);
                y += 52.0;
            }
            for (j, &i) in items.iter().enumerate() {
                let row = rect(x + 4.0, y, w - 8.0, 52.0);
                if row.bottom() > r.y && row.y < r.bottom() {
                    let o = self.occs[i].clone();
                    let ev = self.store.event(&o).clone();
                    let col = color(&self.store.color(&o));
                    let id = Id::AgendaRow(i);
                    if self.hot(&id) {
                        c.fill_round(row, 9.0, t.hover);
                    }
                    if j > 0 {
                        c.hline(row.x + 110.0, row.y, row.w - 120.0, t.sep);
                    }
                    let (tl, tl2) = if o.all_day || (o.start <= ds && o.end >= de) {
                        (tr("All day").to_string(), String::new())
                    } else {
                        let sa = if o.start < ds { "…".to_string() } else { i18n::time(z.to_wall(o.start).secs, h24) };
                        let sb = if o.end > de { "…".to_string() } else { i18n::time(z.to_wall(o.end).secs, h24) };
                        (sa, sb)
                    };
                    c.text(&tl, rect(row.x + 14.0, row.y + 8.0, 86.0, 18.0), Font::BodyBold, t.t1, Align::Left);
                    c.text(&tl2, rect(row.x + 14.0, row.y + 27.0, 86.0, 16.0), Font::Small, t.t3, Align::Left);
                    c.fill_round(rect(row.x + 104.0, row.y + 10.0, 4.0, 32.0), 2.0, col);
                    let cal = self.store.cals[o.cal].name.clone();
                    let cw = c.text_width(&cal, Font::Small).min(160.0);
                    c.text(&ev.title, rect(row.x + 120.0, row.y + 8.0, row.w - 140.0 - cw, 20.0), Font::BodyBold, t.t1, Align::Left);
                    let mut sub = ev.location.clone();
                    if self.store.events[o.cal][o.ev].rrule.is_some() || o.rid.is_some() {
                        sub = if sub.is_empty() { "↻".into() } else { format!("↻  {sub}") };
                    }
                    c.text(&sub, rect(row.x + 120.0, row.y + 28.0, row.w - 140.0 - cw, 16.0), Font::Small, t.t3, Align::Left);
                    c.text(&cal, rect(row.right() - 14.0 - cw, row.y, cw, row.h), Font::Small, t.t3, Align::Right);
                    self.hits.add(row.intersect(&r), id);
                }
                y += 52.0;
            }
            y += 8.0;
        }
        if !any {
            c.text(&fill(tr("Nothing in the next {} days."), &[&AGENDA_DAYS.to_string()]), rect(x, y + 20.0, w, 24.0), Font::Body, t.t3, Align::Center);
            y += 60.0;
        }
        c.unclip();
        self.agenda_h = y + self.agenda_scroll - r.y + 20.0;
        let max = (self.agenda_h - r.h).max(0.0);
        if self.agenda_scroll > max {
            self.agenda_scroll = max;
        }
        let _ = View::Agenda;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_and_columns() {
        assert_eq!(lanes(&[(0, 2), (1, 3), (3, 4), (0, 0)], 7), vec![0, 1, 0, 1]);
        // three overlapping, then a lone one
        let c = columns(&[(0, 3600), (1800, 5400), (3000, 4000), (7200, 9000)]);
        assert_eq!(c, vec![(0, 3), (1, 3), (2, 3), (0, 1)]);
        // back to back: same column
        let c = columns(&[(0, 3600), (3600, 7200)]);
        assert_eq!(c, vec![(0, 1), (0, 1)]);
    }
}
