//! The Apps page: the "what's slowing my PC" card, the search and the buttons, then every program with its
//! processes, sortable by any column, the busy cells tinted.

use super::{
    App, Hit, ROW_H, Row,
    draw::{self, ic},
};
use crate::{
    i18n::{fill, tr},
    model::{self, Col},
    sampler,
};
use plume_ui::{Align, Canvas, Color, Font, Rect, icons, rect};

/// Widths of the number columns.
fn col_w(col: Col) -> f32 {
    match col {
        Col::Name => 0.0,
        Col::Cpu => 92.0,
        Col::Mem => 104.0,
        Col::Disk => 100.0,
        Col::Gpu => 92.0,
        Col::Impact => 118.0,
    }
}

impl App {
    pub fn paint_apps(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let x = page.x + 24.0;
        let w = page.w - 48.0;
        let mut y = self.page_title(c, page, tr("Apps"), "");

        // what's slowing my PC
        if let Some(s) = self.summary.clone() {
            let card = rect(x, y, w, 74.0);
            c.fill_round(card, 14.0, p.card);
            c.stroke_round(card, 14.0, p.line, 1.0);
            let tone = match s.level {
                0 => p.green,
                1 => p.orange,
                _ => p.red,
            };
            let badge = rect(card.x + 16.0, card.y + 17.0, 40.0, 40.0);
            c.fill_round(badge, 20.0, tone.fade(0.16));
            c.icon(if s.level == 0 { &ic::OK } else { &ic::WARN }, badge.centered(20.0, 20.0), tone);
            let tx = badge.right() + 14.0;
            let show = s.key.is_some() && s.level > 0;
            let bw = if show { self.pill_w(c, tr("Show"), false) } else { 0.0 };
            let tw = card.right() - tx - bw - 30.0;
            draw::text(c, &s.title, rect(tx, card.y + 13.0, tw, 24.0), 15.0, 600, p.t1, Align::Left);
            c.text(&s.detail, rect(tx, card.y + 38.0, tw, 22.0), Font::Body, p.t2, Align::Left);
            if show {
                self.pill(c, rect(card.right() - 16.0 - bw, card.y + 20.0, bw, 34.0), tr("Show"), None, None, true, Hit::SummaryShow);
            }
            y += 74.0 + 16.0;
        }

        // the search and the buttons
        let bar_h = 36.0;
        let sw = (w * 0.42).clamp(220.0, 380.0);
        let s = rect(x, y, sw, bar_h);
        let focused = self.edit_for == Some("search");
        c.fill_round(s, 18.0, p.t1.fade(if focused { 0.1 } else { 0.06 }).over(p.bg));
        if focused {
            c.stroke_round(s, 18.0, p.accent.fade(0.7), 1.5);
        }
        c.icon(&icons::SEARCH, rect(s.x + 12.0, s.y + 10.0, 16.0, 16.0), p.t2);
        if !focused {
            let (text, ink) = if self.query.is_empty() { (tr("Search a program, a process or a PID").to_string(), p.t3) } else { (self.query.clone(), p.t1) };
            c.text(&text, rect(s.x + 36.0, s.y, s.w - 70.0, s.h), Font::Body, ink, Align::Left);
        }
        self.search_rect = Some(s);
        self.hits.add(s, Hit::SearchBox);
        if !self.query.is_empty() {
            self.icon_btn(c, &icons::CLOSE, rect(s.right() - 32.0, s.y + 4.0, 28.0, 28.0), Hit::SearchClear, p.t2);
        }
        let has_sel = self.selected().is_some();
        let ew = self.pill_w(c, tr("End task"), true);
        let dw = self.pill_w(c, tr("Details"), true);
        let er = rect(x + w - ew, y, ew, bar_h);
        self.pill(c, er, tr("End task"), Some(&ic::END), Some(p.red), has_sel, Hit::EndTask);
        self.pill(c, rect(er.x - 10.0 - dw, y, dw, bar_h), tr("Details"), Some(&ic::INFO), None, has_sel, Hit::Details);
        // how many, next to the search
        let count = fill(tr("{} programs, {} processes"), &[&self.groups.len().to_string(), &self.sys.procs.to_string()]);
        let cx0 = s.right() + 14.0;
        let cw = er.x - 10.0 - dw - 14.0 - cx0;
        if cw > 80.0 {
            c.text(&count, rect(cx0, y, cw, bar_h), Font::Small, p.t3, Align::Left);
        }
        y += bar_h + 14.0;

        // the column headers: total on top, name below
        let head = rect(x, y, w, 46.0);
        let cols = self.columns(head);
        for (col, r) in &cols {
            let hit = Hit::Header(*col);
            let hot = self.hovered(&hit);
            if hot {
                c.fill_round(*r, 8.0, p.t1.fade(0.05));
            }
            let sorted = self.set.sort == *col;
            let align = if *col == Col::Name { Align::Left } else { Align::Right };
            let inner = r.inset_xy(10.0, 0.0);
            let total = match col {
                Col::Cpu => Some(self.sys.cpu),
                Col::Mem => Some(self.sys.mem_pct()),
                Col::Disk => Some(self.sys.disk_active),
                Col::Gpu => self.sys.has_gpu.then_some(self.sys.gpu),
                _ => None,
            };
            if let Some(v) = total {
                draw::text(
                    c,
                    &model::percent(v),
                    rect(inner.x, r.y + 2.0, inner.w, 22.0),
                    15.0,
                    600,
                    if v >= 60.0 { super::paint::level_color(&p, v) } else { p.t1 },
                    align,
                );
            }
            let label_y = r.y + 24.0;
            let lw = c.text_width(col.label(), if sorted { Font::SmallBold } else { Font::Small });
            c.text(
                col.label(),
                rect(inner.x, label_y, inner.w, 18.0),
                if sorted { Font::SmallBold } else { Font::Small },
                if sorted { p.t1 } else { p.t2 },
                align,
            );
            if sorted {
                let arrow = if self.set.desc { &icons::SORT_DOWN } else { &icons::SORT_UP };
                let ax = if align == Align::Left { inner.x + lw + 4.0 } else { inner.right() - lw - 14.0 };
                c.icon(arrow, rect(ax, label_y + 4.0, 10.0, 10.0), p.t1);
            }
            self.hits.add(*r, hit);
        }
        y += 46.0;
        c.hline(x, y, w, p.line);
        y += 4.0;

        // the rows
        let list = rect(x - 8.0, y, w + 16.0, page.bottom() - y - 8.0);
        self.list_top = y;
        self.content_h = self.rows.len() as f32 * ROW_H + 8.0;
        self.view_h = list.h;
        let area_h = list.h;
        let scroll = self.scroll().min((self.content_h - area_h).max(0.0));
        self.scroll.insert(super::Page::Apps, scroll);
        c.clip(list);
        let first = (scroll / ROW_H) as usize;
        let last = (((scroll + area_h) / ROW_H).ceil() as usize + 1).min(self.rows.len());
        let sel_row = self.sel_row();
        for i in first..last {
            let ry = list.y + i as f32 * ROW_H - scroll;
            let row = rect(x - 4.0, ry, w + 8.0, ROW_H - 2.0);
            self.paint_row(c, i, row, &cols, sel_row == Some(i));
        }
        if self.rows.is_empty() {
            let msg = if self.samples == 0 {
                tr("Reading the programs…")
            } else if !self.query.is_empty() {
                tr("No program matches this search.")
            } else {
                tr("The list is paused.")
            };
            c.text(msg, rect(list.x, list.y + 30.0, list.w, 24.0), Font::Body, p.t3, Align::Center);
        }
        c.unclip();
        self.scrollbar(c, rect(list.x, list.y, list.w + 8.0, list.h));
    }

    /// Where each column sits in a band `r` (same x for the header and the rows).
    fn columns(&self, r: Rect) -> Vec<(Col, Rect)> {
        let fixed: f32 = Col::ALL.iter().map(|c| col_w(*c)).sum();
        let name_w = (r.w - fixed).max(160.0);
        let mut x = r.x;
        Col::ALL
            .iter()
            .map(|&col| {
                let cw = if col == Col::Name { name_w } else { col_w(col) };
                let q = rect(x, r.y, cw, r.h);
                x += cw;
                (col, q)
            })
            .collect()
    }

    fn paint_row(&mut self, c: &mut Canvas, i: usize, row: Rect, cols: &[(Col, Rect)], selected: bool) {
        let p = self.pal.clone();
        let r = self.rows[i];
        if let Row::Section(label, n) = r {
            let t = format!("{} ({n})", tr(label));
            c.text(&t, rect(row.x + 8.0, row.y + 6.0, row.w, row.h - 6.0), Font::SmallBold, p.t3, Align::Left);
            return;
        }
        let hit = Hit::Row(i);
        if selected {
            c.fill_round(row, 8.0, p.accent.fade(if p.dark { 0.2 } else { 0.14 }));
        } else if self.hovered(&hit) || matches!(&self.hover, Some(Hit::Chevron(j)) if *j == i) {
            c.fill_round(row, 8.0, p.t1.fade(0.045));
        }
        self.hits.add(row, hit);
        let (gi, pi) = match r {
            Row::Group(gi) => (gi, None),
            Row::Proc(gi, pi) => (gi, Some(pi)),
            Row::Section(..) => return,
        };
        let total = self.sys.mem_total.max(1);
        let (name, sub, cpu, mem, disk, gpu, impact, path) = {
            let g = &self.groups[gi];
            match pi {
                None => {
                    let n = g.procs.len();
                    (g.name.clone(), if n > 1 { format!("({n})") } else { String::new() }, g.cpu, g.mem, g.disk(), g.gpu, g.impact, g.path.clone())
                }
                Some(pi) => {
                    let q = &g.procs[pi];
                    (q.name.clone(), format!("PID {}", q.pid), q.cpu, q.mem, q.read + q.write, q.gpu, model::impact_score(q.cpu, q.gpu, q.read + q.write), None)
                }
            }
        };
        // the name: chevron, icon, name, count
        let nr = cols[0].1;
        let mut tx = row.x + 10.0;
        if pi.is_none() {
            if self.groups[gi].procs.len() > 1 {
                let open = self.expanded.contains(&self.groups[gi].key);
                let cr = rect(tx - 2.0, row.y + (row.h - 24.0) / 2.0, 24.0, 24.0);
                let chev = Hit::Chevron(i);
                if self.hovered(&chev) {
                    c.fill_round(cr, 6.0, p.t1.fade(0.1));
                }
                c.icon(if open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, cr.centered(14.0, 14.0), p.t2);
                self.hits.add(cr, chev);
            }
            tx += 26.0;
            let ir = rect(tx, row.y + (row.h - 20.0) / 2.0, 20.0, 20.0);
            match &path {
                Some(path) => {
                    let key = sampler::icon_key(path);
                    if self.icons.contains_key(&key) {
                        let px = self.icons.get(&key);
                        draw::pixels(c, key, ir, px);
                    } else {
                        c.icon(&ic::WINDOW, ir.inset(1.0), p.t3);
                    }
                }
                None => c.icon(&icons::GEAR, ir.inset(1.0), p.t3),
            }
            tx += 30.0;
        } else {
            tx += 56.0;
        }
        let name_w = nr.right() - tx - 12.0;
        let font = if pi.is_none() { Font::Body } else { Font::Small };
        let nw = c.text_width(&name, font).min(name_w);
        c.text(&name, rect(tx, row.y, name_w, row.h), font, if pi.is_none() { p.t1 } else { p.t2 }, Align::Left);
        if !sub.is_empty() && nw + 40.0 < name_w {
            c.text(&sub, rect(tx + nw + 6.0, row.y, name_w - nw - 6.0, row.h), Font::Small, p.t3, Align::Left);
        }
        // the numbers, tinted by how busy they are
        let heat = |v: f32| (v.clamp(0.0, 1.0) * 0.55).max(0.0);
        let level = model::impact_level(impact);
        let cells: [(f32, String); 5] = [
            (heat(cpu / 40.0), model::percent(cpu)),
            (heat(mem as f32 / (total as f32 * 0.2)), model::size(mem)),
            (heat((disk / (16.0 * 1_048_576.0)) as f32), model::rate(disk)),
            (heat(gpu / 40.0), if self.sys.has_gpu { model::percent(gpu) } else { "—".into() }),
            (heat(level as f32 / 5.0 - 0.15), model::impact_label(level).to_string()),
        ];
        for ((_, q), (h, text)) in cols[1..].iter().zip(cells) {
            let cell = rect(q.x + 3.0, row.y + 1.0, q.w - 6.0, row.h - 2.0);
            if h > 0.03 {
                c.fill_round(cell, 6.0, p.heat.fade(h));
            }
            c.text(&text, cell.inset_xy(8.0, 0.0), Font::Body, if h > 0.3 { p.t1 } else { p.t2 }, Align::Right);
        }
    }
}

#[allow(dead_code)]
const _C: Option<Color> = None;
