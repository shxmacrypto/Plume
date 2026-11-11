//! The Map page: where the scan is, a treemap of the folder (each block as big as what it holds, its biggest
//! folders opened one level), and the list of its folders and files, or of the biggest files under it.

use super::{App, Hit, ListMode, ROW_H, paint::HUES};
use crate::{
    clean::human,
    i18n::{count, fill, tr},
    scan::{DENIED, Order},
};
use plume_ui::{Align, Canvas, Color, Font, Rect, icons, rect};
use std::sync::atomic::Ordering;

fn worst(row: &[f64], sum: f64, side: f64) -> f64 {
    let (mut lo, mut hi) = (f64::MAX, 0.0f64);
    for &v in row {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo <= 0.0 || sum <= 0.0 {
        return f64::MAX;
    }
    let (s2, w2) = (sum * sum, side * side);
    (w2 * hi / s2).max(s2 / (w2 * lo))
}

/// Squarified treemap: rectangles for `values` (biggest first) filling `area`, each as big as its value, as square as
/// possible. A zero value gets an empty rectangle.
pub fn squarify(values: &[f64], area: Rect) -> Vec<Rect> {
    let mut out = vec![Rect::default(); values.len()];
    let total: f64 = values.iter().filter(|v| **v > 0.0).sum();
    if total <= 0.0 || area.is_empty() {
        return out;
    }
    let k = (area.w as f64 * area.h as f64) / total;
    let a: Vec<f64> = values.iter().map(|v| v.max(0.0) * k).collect();
    let (mut x, mut y, mut w, mut h) = (area.x as f64, area.y as f64, area.w as f64, area.h as f64);
    let n = a.iter().rposition(|v| *v > 0.0).map_or(0, |i| i + 1);
    let mut i = 0;
    while i < n {
        let side = w.min(h);
        let mut j = i + 1;
        let mut sum = a[i];
        let mut best = worst(&a[i..j], sum, side);
        while j < n {
            let s = sum + a[j];
            let ws = worst(&a[i..=j], s, side);
            if ws > best {
                break;
            }
            best = ws;
            sum = s;
            j += 1;
        }
        if w >= h {
            let cw = if h > 0.0 { sum / h } else { 0.0 };
            let mut yy = y;
            for k in i..j {
                let bh = if cw > 0.0 { a[k] / cw } else { 0.0 };
                out[k] = rect(x as f32, yy as f32, cw as f32, bh as f32);
                yy += bh;
            }
            x += cw;
            w -= cw;
        } else {
            let rh = if w > 0.0 { sum / w } else { 0.0 };
            let mut xx = x;
            for k in i..j {
                let bw = if rh > 0.0 { a[k] / rh } else { 0.0 };
                out[k] = rect(xx as f32, y as f32, bw as f32, rh as f32);
                xx += bw;
            }
            y += rh;
            h -= rh;
        }
        i = j;
    }
    out
}

/// A piece of the path: the folder's name, the scanned folder's own name (or "C:" for a drive).
fn crumb(t: &crate::scan::Tree, id: u32) -> String {
    if id != 0 {
        return t.name(id).to_string();
    }
    match t.root.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => t.root.to_string_lossy().trim_end_matches('\\').to_string(),
    }
}

impl App {
    pub fn paint_map(&mut self, c: &mut Canvas, body: Rect) {
        let p = self.pal.clone();
        let x0 = body.x + 24.0;
        let w = body.w - 48.0;
        if let Some(prog) = self.scanning.clone() {
            let target = self.target.as_ref().map(|t| t.to_string_lossy().to_string()).unwrap_or_default();
            let y = self.heading(c, x0, body.y + 28.0, w, tr("Scanning…"), &target);
            let files = prog.files.load(Ordering::Relaxed);
            let bytes = prog.bytes.load(Ordering::Relaxed);
            let r = rect(x0, y + 10.0, w.min(560.0), 120.0);
            c.fill_round(r, 16.0, p.card);
            c.text(&human(bytes), rect(r.x + 22.0, r.y + 18.0, r.w - 44.0, 36.0), Font::Hero, p.accent, Align::Left);
            c.text(&fill(tr("{n} files so far"), &[&count(files)]), rect(r.x + 22.0, r.y + 58.0, r.w - 44.0, 20.0), Font::Body, p.t2, Align::Left);
            // a moving stripe: busy
            let t = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() % 1600) as f32 / 1600.0;
            let track = rect(r.x + 22.0, r.y + 92.0, r.w - 44.0, 6.0);
            c.fill_round(track, 3.0, p.t1.fade(0.1));
            let sw = track.w * 0.3;
            let sx = track.x + (track.w + sw) * t - sw;
            c.clip(track);
            c.fill_round(rect(sx, track.y, sw, track.h), 3.0, p.accent);
            c.unclip();
            self.pill(c, x0, r.bottom() + 16.0, tr("Stop"), false, Hit::CancelScan);
            return;
        }
        let Some(t) = self.tree.clone() else {
            let y = self.heading(c, x0, body.y + 28.0, w, tr("Map"), tr("Scan a drive or a folder to see what fills it."));
            self.pill(c, x0, y + 4.0, tr("Choose a drive"), true, Hit::GoScan);
            return;
        };
        // the path, clickable piece by piece
        let top = body.y + 18.0;
        self.icon_btn(c, &icons::UP, rect(x0, top, 34.0, 32.0), Hit::Up);
        let mut x = x0 + 44.0;
        let right_tools = 380.0;
        let chain = t.chain(self.cur);
        let max_x = x0 + w - right_tools;
        // when too long, only the end of the path
        let mut start = 0;
        loop {
            let total: f32 = chain[start..].iter().map(|&id| c.text_width(&crumb(&t, id), Font::BodyBold) + 26.0).sum();
            if x + total <= max_x || start + 1 >= chain.len() {
                break;
            }
            start += 1;
        }
        if start > 0 {
            c.text("…", rect(x, top, 18.0, 32.0), Font::BodyBold, p.t3, Align::Left);
            x += 18.0;
        }
        for (k, &id) in chain[start..].iter().enumerate() {
            let name = crumb(&t, id);
            let tw = c.text_width(&name, Font::BodyBold).min(max_x - x - 10.0).max(20.0);
            let r = rect(x - 6.0, top, tw + 12.0, 32.0);
            let hit = Hit::Crumb(id);
            if self.hovered(&hit) {
                c.fill_round(r, 8.0, p.t1.fade(0.08));
            }
            let last = start + k + 1 == chain.len();
            c.text(&name, rect(x, top, tw, 32.0), Font::BodyBold, if last { p.t1 } else { p.t2 }, Align::Left);
            self.hits.add(r, hit);
            x += tw + 8.0;
            if !last {
                c.icon(&icons::CHEVRON_RIGHT, rect(x, top + 9.0, 14.0, 14.0), p.t3);
                x += 18.0;
            }
        }
        let mut rx = x0 + w;
        rx = self.pill_right(c, rx, top, tr("Rescan"), false, Hit::Rescan) - 8.0;
        rx = self.pill_right(c, rx, top, tr("Plume FEx"), false, Hit::Fex) - 8.0;
        self.pill_right(c, rx, top, tr("Explorer"), false, Hit::Explorer);

        let node = *t.node(self.cur);
        let mut sub = format!("{}  ·  {}", t.path(self.cur).display(), fill(tr("{size} in {n} files"), &[&human(node.size), &count(node.files)]));
        if self.cur == 0 {
            let took = if t.millis < 1000 { format!("{} ms", t.millis) } else { format!("{:.1} s", t.millis as f32 / 1000.0) };
            sub.push_str(&fill(tr(" · read in {t}"), &[&took]));
        }
        if node.flags & DENIED != 0 || t.node(0).flags & crate::scan::UNSURE != 0 && self.cur == 0 {
            sub.push_str(tr(" · some folders could not be read"));
        }
        c.text(&sub, rect(x0 + 44.0, top + 34.0, w - 44.0, 20.0), Font::Small, p.t3, Align::Left);

        let list_w = (w * 0.4).clamp(340.0, 460.0);
        let area = rect(x0, top + 64.0, w - list_w - 16.0, body.bottom() - top - 64.0 - 20.0);
        self.paint_treemap(c, area);
        let list = rect(area.right() + 16.0, area.y, list_w, area.h);
        self.paint_list(c, list);
    }

    fn block_color(&self, k: usize) -> Color {
        let base = Color::hex(HUES[k % HUES.len()]);
        if self.pal.dark { base.fade(0.62).over(self.pal.card) } else { base.fade(0.55).over(self.pal.card) }
    }

    fn paint_treemap(&mut self, c: &mut Canvas, area: Rect) {
        let p = self.pal.clone();
        let Some(t) = self.tree.clone() else { return };
        c.fill_round(area, 16.0, p.card);
        let inner = area.inset(8.0);
        let mut kids = t.sorted_children(self.cur, Order::Size, true);
        kids.retain(|&k| t.node(k).size > 0);
        kids.truncate(400);
        if kids.is_empty() {
            c.text(tr("This folder holds nothing that takes space."), inner, Font::Body, p.t3, Align::Center);
            return;
        }
        let vals: Vec<f64> = kids.iter().map(|&k| t.node(k).size as f64).collect();
        let rects = squarify(&vals, inner);
        let sel = self.sel;
        let hover_id = match &self.hover {
            Some(Hit::Block(id)) => Some(*id),
            Some(Hit::Row(i)) => self.rows.get(*i).copied(),
            _ => None,
        };
        for (k, (&id, r)) in kids.iter().zip(rects.iter()).enumerate() {
            let b = r.inset(1.5);
            if b.w < 2.0 || b.h < 2.0 {
                continue;
            }
            let col = self.block_color(k);
            let n = *t.node(id);
            let rad = 6.0f32.min(b.w / 3.0).min(b.h / 3.0);
            c.fill_round(b, rad, if n.is_dir() { col } else { col.mix(p.card, 0.35) });
            self.hits.add(b, Hit::Block(id));
            let label_h = 20.0;
            let show_label = b.w > 54.0 && b.h > 30.0;
            if show_label {
                let ink = if p.dark { Color::hex(0xffffff) } else { Color::hex(0x111114) };
                let name = t.name(id);
                let size = human(n.size);
                let sw = c.text_width(&size, Font::Small) + 8.0;
                if b.w > sw + 50.0 {
                    c.text(name, rect(b.x + 8.0, b.y + 2.0, b.w - sw - 14.0, label_h), Font::SmallBold, ink, Align::Left);
                    c.text(&size, rect(b.right() - sw - 6.0, b.y + 2.0, sw, label_h), Font::Small, ink.fade(0.8), Align::Right);
                } else {
                    c.text(name, rect(b.x + 6.0, b.y + 2.0, b.w - 10.0, label_h), Font::SmallBold, ink, Align::Left);
                }
            }
            // one level deeper inside big folders
            if n.is_dir() && b.w > 60.0 && b.h > 50.0 {
                let room = rect(b.x + 4.0, b.y + label_h + 2.0, b.w - 8.0, b.h - label_h - 6.0);
                let mut sub = t.sorted_children(id, Order::Size, true);
                sub.retain(|&s| t.node(s).size > 0);
                sub.truncate(120);
                let sv: Vec<f64> = sub.iter().map(|&s| t.node(s).size as f64).collect();
                let srects = squarify(&sv, room);
                for (&sid, sr) in sub.iter().zip(srects.iter()) {
                    let sb = sr.inset(1.0);
                    if sb.w < 3.0 || sb.h < 3.0 {
                        continue;
                    }
                    let sn = t.node(sid);
                    let shade = if sn.is_dir() {
                        Color::hex(0x000000).fade(if p.dark { 0.22 } else { 0.1 })
                    } else {
                        Color::hex(0xffffff).fade(if p.dark { 0.08 } else { 0.25 })
                    };
                    c.fill_round(sb, 3.0f32.min(sb.w / 3.0), shade.over(col));
                    if sb.w > 70.0 && sb.h > 22.0 {
                        let ink = if p.dark { Color::hex(0xffffff).fade(0.85) } else { Color::hex(0x111114).fade(0.85) };
                        c.text(t.name(sid), rect(sb.x + 6.0, sb.y + 1.0, sb.w - 10.0, 18.0), Font::Small, ink, Align::Left);
                    }
                    if hover_id == Some(sid) || sel == Some(sid) {
                        c.stroke_round(sb, 3.0, Color::hex(0xffffff), 2.0);
                    }
                    self.hits.add(sb, Hit::Block(sid));
                }
            }
            if hover_id == Some(id) || sel == Some(id) {
                c.stroke_round(b, rad, if p.dark { Color::hex(0xffffff) } else { Color::hex(0x111114) }, 2.0);
            }
        }
    }

    fn paint_list(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        let Some(t) = self.tree.clone() else { return };
        c.fill_round(r, 16.0, p.card);
        // contents or biggest files
        let seg = rect(r.x + 12.0, r.y + 12.0, r.w - 24.0, 32.0);
        c.fill_round(seg, 16.0, p.t1.fade(0.06));
        let half = seg.w / 2.0;
        for (k, (mode, label)) in [(ListMode::Contents, tr("In this folder")), (ListMode::Files, tr("Biggest files"))].into_iter().enumerate() {
            let sr = rect(seg.x + k as f32 * half + 3.0, seg.y + 3.0, half - 6.0, seg.h - 6.0);
            if self.mode == mode {
                c.fill_round(sr, 13.0, if p.dark { Color::hex(0x34343c) } else { Color::hex(0xffffff) });
            }
            c.text(label, sr, Font::BodyBold, if self.mode == mode { p.t1 } else { p.t2 }, Align::Center);
            self.hits.add(sr, Hit::Mode(mode));
        }
        // column titles
        let head = rect(r.x + 12.0, seg.bottom() + 8.0, r.w - 24.0, 26.0);
        let arrow = |on: bool, desc: bool| if on { if desc { " ↓" } else { " ↑" } } else { "" };
        let name_label = format!("{}{}", tr("Name"), arrow(self.order == Order::Name, self.desc));
        let size_label = format!("{}{}", tr("Size"), arrow(self.order == Order::Size, self.desc));
        let date_label = format!("{}{}", tr("Changed"), arrow(self.order == Order::Date, self.desc));
        let nr = rect(head.x + 30.0, head.y, 120.0, head.h);
        c.text(&name_label, nr, Font::SmallBold, if self.order == Order::Name { p.t1 } else { p.t3 }, Align::Left);
        self.hits.add(nr, Hit::Sort(Order::Name));
        let szr = rect(head.right() - 90.0, head.y, 90.0, head.h);
        c.text(&size_label, szr, Font::SmallBold, if self.order == Order::Size { p.t1 } else { p.t3 }, Align::Right);
        self.hits.add(szr, Hit::Sort(Order::Size));
        let dr = rect(head.right() - 190.0, head.y, 90.0, head.h);
        c.text(&date_label, dr, Font::SmallBold, if self.order == Order::Date { p.t1 } else { p.t3 }, Align::Right);
        self.hits.add(dr, Hit::Sort(Order::Date));
        c.hline(head.x, head.bottom(), head.w, p.line);
        // rows
        let rows_r = rect(r.x + 6.0, head.bottom() + 4.0, r.w - 12.0, r.bottom() - head.bottom() - 10.0);
        self.list_rect = rows_r;
        let max = (self.rows.len() as f32 * ROW_H - rows_r.h).max(0.0);
        self.list_scroll = self.list_scroll.min(max);
        if self.rows.is_empty() {
            c.text(tr("Empty folder"), rows_r, Font::Body, p.t3, Align::Center);
            return;
        }
        // color of each child as in the map
        let order_by_size: Vec<u32> = {
            let mut v = t.sorted_children(self.cur, Order::Size, true);
            v.retain(|&k| t.node(k).size > 0);
            v
        };
        let parent_size = t.node(self.cur).size.max(1);
        let first = (self.list_scroll / ROW_H).floor() as usize;
        let now = crate::clean::now_filetime();
        c.clip(rows_r);
        for i in first..self.rows.len() {
            let y = rows_r.y + i as f32 * ROW_H - self.list_scroll;
            if y > rows_r.bottom() {
                break;
            }
            let id = self.rows[i];
            let n = *t.node(id);
            let row = rect(rows_r.x, y, rows_r.w, ROW_H - 2.0);
            let hit = Hit::Row(i);
            let hovered = self.hovered(&hit) || matches!(self.hover, Some(Hit::Block(b)) if b == id);
            if self.sel == Some(id) {
                c.fill_round(row, 8.0, p.accent.fade(0.18));
            } else if hovered {
                c.fill_round(row, 8.0, p.t1.fade(0.06));
            }
            if let Some(k) = order_by_size.iter().position(|&x| x == id) {
                c.fill_round(rect(row.x + 8.0, row.y + 12.0, 8.0, 8.0), 2.5, self.block_color(k));
            }
            c.icon(
                if n.is_dir() { &icons::FOLDER } else { &icons::FILE },
                rect(row.x + 24.0, row.y + 8.0, 16.0, 16.0),
                if n.is_dir() { p.accent } else { p.t2 },
            );
            let name_w = row.w - 48.0 - 190.0;
            let name = if self.mode == ListMode::Files {
                t.name(id).to_string()
            } else if n.flags & DENIED != 0 {
                format!("{}  ⚠", t.name(id))
            } else {
                t.name(id).to_string()
            };
            c.text(&name, rect(row.x + 48.0, row.y, name_w, row.h), Font::Body, p.t1, Align::Left);
            // a bar: share of the folder
            let k = n.size as f32 / parent_size as f32;
            let track = rect(row.x + 48.0, row.bottom() - 5.0, name_w, 3.0);
            c.fill_round(track, 1.5, p.t1.fade(0.06));
            c.fill_round(rect(track.x, track.y, (name_w * k).max(2.0), 3.0), 1.5, p.accent.fade(0.55));
            c.text(&crate::i18n::ago(n.mtime, now), rect(row.right() - 196.0, row.y, 90.0, row.h), Font::Small, p.t3, Align::Right);
            c.text(&human(n.size), rect(row.right() - 96.0, row.y, 88.0, row.h), Font::BodyBold, p.t2, Align::Right);
            self.hits.add(row.intersect(&rows_r), hit);
        }
        c.unclip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treemap_fills_the_area() {
        let area = rect(10.0, 20.0, 400.0, 300.0);
        let vals = [500.0, 250.0, 120.0, 80.0, 40.0, 10.0, 0.0];
        let rs = squarify(&vals, area);
        let total: f64 = vals.iter().sum();
        for (v, r) in vals.iter().zip(rs.iter()) {
            let want = v / total * 120_000.0;
            let got = r.w as f64 * r.h as f64;
            assert!((want - got).abs() < 1.0, "{v}: {got} vs {want}");
            if *v > 0.0 {
                assert!(r.x >= 9.99 && r.y >= 19.99 && r.right() <= 410.01 && r.bottom() <= 320.01, "{r:?}");
            }
        }
        // no overlap between the two biggest
        let (a, b) = (rs[0], rs[1]);
        assert!(a.intersect(&b).w * a.intersect(&b).h < 0.01);
        // squarish: the biggest block is not a sliver
        assert!(a.w.min(a.h) / a.w.max(a.h) > 0.4);
        assert!(squarify(&[], area).is_empty());
        assert!(squarify(&[0.0, 0.0], area).iter().all(|r| r.is_empty()));
    }
}
