//! The Performance page: one card per part of the PC (processor, memory, disk, network, graphics) with a live graph
//! of the last minute, and the time since Windows started.

use super::{App, HIST, draw, draw::ic};
use crate::{
    i18n::{fill, tr},
    model,
};
use plume_ui::{Align, Canvas, Color, Font, Rect, icons::Icon, rect};
use std::collections::VecDeque;

struct Card<'a> {
    icon: &'a Icon,
    title: &'a str,
    value: String,
    sub: String,
    color: Color,
    lines: Vec<(&'a VecDeque<f32>, Color)>,
    max: f32,
    stats: Vec<(String, String)>,
}

impl App {
    pub fn paint_perf(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let s = self.sys.clone();
        let sub = fill(
            tr("Windows has been running for {}  ·  {} processes  ·  {} threads"),
            &[&model::uptime(s.uptime_s), &s.procs.to_string(), &group_digits(s.threads as u64)],
        );
        let top = self.page_title(c, page, tr("Performance"), &sub);
        let area = rect(page.x, top, page.w, page.h - top);
        self.view_h = area.h;
        let scroll = self.scroll();
        let net_max = self.hist.net_in.iter().chain(self.hist.net_out.iter()).copied().fold(125_000.0f32, f32::max) * 1.15;
        let h = &self.hist;
        let cards = [
            Card {
                icon: &ic::CHIP,
                title: tr("Processor"),
                value: model::percent(s.cpu),
                sub: s.cpu_name.clone(),
                color: p.blue,
                lines: vec![(&h.cpu, p.blue)],
                max: 100.0,
                stats: vec![
                    (tr("Logical processors").into(), s.ncpu.to_string()),
                    (tr("Handles").into(), group_digits(s.handles as u64)),
                    (tr("Up time").into(), model::uptime(s.uptime_s)),
                ],
            },
            Card {
                icon: &ic::MEMORY,
                title: tr("Memory"),
                value: model::percent(s.mem_pct()),
                sub: fill(tr("{} of {} in use"), &[&model::size(s.mem_used), &model::size(s.mem_total)]),
                color: p.teal,
                lines: vec![(&h.mem, p.teal)],
                max: 100.0,
                stats: vec![
                    (tr("Available").into(), model::size(s.mem_total.saturating_sub(s.mem_used))),
                    (tr("Committed").into(), format!("{} / {}", model::size(s.commit), model::size(s.commit_limit))),
                    (tr("Cached").into(), model::size(s.cached)),
                ],
            },
            Card {
                icon: &ic::DISK,
                title: tr("Disk"),
                value: model::percent(s.disk_active),
                sub: tr("Share of the time the disks are busy").into(),
                color: p.green,
                lines: vec![(&h.disk, p.green)],
                max: 100.0,
                stats: vec![(tr("Read").into(), model::rate(s.disk_read)), (tr("Write").into(), model::rate(s.disk_write))],
            },
            Card {
                icon: &ic::NET,
                title: tr("Network"),
                value: model::bits(s.net_in + s.net_out),
                sub: tr("All connected adapters").into(),
                color: p.orange,
                lines: vec![(&h.net_in, p.orange), (&h.net_out, p.blue)],
                max: net_max,
                stats: vec![(tr("Receive").into(), model::bits(s.net_in)), (tr("Send").into(), model::bits(s.net_out))],
            },
            Card {
                icon: &ic::GPU,
                title: tr("Graphics"),
                value: if s.has_gpu { model::percent(s.gpu) } else { "—".into() },
                sub: if s.gpu_name.is_empty() { tr("Graphics card").into() } else { s.gpu_name.clone() },
                color: p.red,
                lines: vec![(&h.gpu, p.red)],
                max: 100.0,
                stats: if s.gpu_mem > 0 { vec![(tr("Dedicated memory").into(), model::size(s.gpu_mem))] } else { vec![] },
            },
        ];
        // two cards per row when there is room
        let x0 = page.x + 24.0;
        let w = page.w - 48.0;
        let colw = if w > 760.0 { (w - 16.0) / 2.0 } else { w };
        let per_row = if colw < w { 2 } else { 1 };
        let card_h = 236.0;
        c.clip(area);
        let mut y = area.y - scroll + 4.0;
        for (i, card) in cards.iter().enumerate() {
            let col = i % per_row;
            if i > 0 && col == 0 {
                y += card_h + 16.0;
            }
            let r = rect(x0 + col as f32 * (colw + 16.0), y, colw, card_h);
            if r.bottom() >= area.y && r.y <= area.bottom() {
                paint_card(c, &p, r, card);
            }
        }
        c.unclip();
        self.content_h = y + card_h + 24.0 + scroll - area.y;
        self.scrollbar(c, area);
    }
}

fn paint_card(c: &mut Canvas, p: &super::Pal, r: Rect, k: &Card) {
    c.fill_round(r, 14.0, p.card);
    c.stroke_round(r, 14.0, p.line, 1.0);
    let ir = rect(r.x + 18.0, r.y + 18.0, 30.0, 30.0);
    c.fill_round(ir, 9.0, k.color.fade(0.16));
    c.icon(k.icon, ir.centered(16.0, 16.0), k.color);
    draw::text(c, k.title, rect(ir.right() + 12.0, r.y + 14.0, r.w - 200.0, 22.0), 15.0, 600, p.t1, Align::Left);
    c.text(&k.sub, rect(ir.right() + 12.0, r.y + 34.0, r.w - 190.0, 18.0), Font::Small, p.t3, Align::Left);
    draw::text(c, &k.value, rect(r.right() - 170.0, r.y + 14.0, 152.0, 36.0), 22.0, 700, p.t1, Align::Right);
    // the graph, on a faint grid
    let g = rect(r.x + 18.0, r.y + 62.0, r.w - 36.0, 106.0);
    c.fill_round(g.inset(-1.0), 8.0, p.t1.fade(0.025));
    for k in 1..4 {
        c.hline(g.x, g.y + g.h * k as f32 / 4.0, g.w, p.line);
    }
    for k in 1..6 {
        c.vline(g.x + g.w * k as f32 / 6.0, g.y, g.h, p.line.fade(0.6));
    }
    for (vals, col) in &k.lines {
        draw::graph(c, g, vals, HIST, k.max, *col, k.lines.len() == 1);
    }
    c.text(tr("60 seconds"), rect(g.x, g.bottom() + 2.0, 120.0, 16.0), Font::Small, p.t3, Align::Left);
    c.text("0", rect(g.right() - 40.0, g.bottom() + 2.0, 40.0, 16.0), Font::Small, p.t3, Align::Right);
    // the numbers below
    let n = k.stats.len().max(1) as f32;
    let sw = (r.w - 36.0) / n;
    for (i, (label, value)) in k.stats.iter().enumerate() {
        let sx = r.x + 18.0 + i as f32 * sw;
        let dot = k.lines.len() > 1 && i < k.lines.len();
        let lx = if dot {
            c.fill_round(rect(sx, r.bottom() - 42.0, 8.0, 8.0), 4.0, k.lines[i].1);
            sx + 14.0
        } else {
            sx
        };
        c.text(label, rect(lx, r.bottom() - 48.0, sw - 18.0, 18.0), Font::Small, p.t3, Align::Left);
        c.text(value, rect(sx, r.bottom() - 30.0, sw - 6.0, 20.0), Font::BodyBold, p.t1, Align::Left);
    }
}

/// 123456 → "123,456" (French: "123 456").
pub fn group_digits(n: u64) -> String {
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

#[cfg(test)]
mod tests {
    #[test]
    fn digits() {
        assert_eq!(super::group_digits(1234567), "1,234,567");
        assert_eq!(super::group_digits(12), "12");
    }
}
