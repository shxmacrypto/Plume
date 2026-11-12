//! The Startup apps page: what opens by itself at sign-in, in one card, each with a switch.

use super::{App, Hit, draw, draw::ic};
use crate::{
    i18n::{fill, tr},
    model, sampler,
};
use plume_ui::{Align, Canvas, Color, Font, Rect, icons, m, rect};

const ROW: f32 = 48.0;

impl App {
    pub fn paint_startup(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let top = self.page_title(c, page, tr("Startup apps"), tr("Programs that open by themselves when you sign in. Turning one off keeps it installed."));
        let x = page.x + 24.0;
        let w = page.w - 48.0;
        // the refresh button, on the title's line
        let rw = self.pill_w(c, tr("Refresh"), true);
        self.pill(c, rect(x + w - rw, m::TITLE_H + 2.0, rw, 32.0), tr("Refresh"), Some(&icons::RELOAD), None, !self.startup_busy, Hit::StartRefresh);
        let area = rect(page.x, top, page.w, page.h - top);
        self.view_h = area.h;
        let Some(list) = self.startup.clone() else {
            self.content_h = 0.0;
            self.empty_state(c, area, &ic::ROCKET, tr("Reading the startup list…"), tr("It takes a second."), None);
            return;
        };
        if list.is_empty() {
            self.content_h = 0.0;
            self.empty_state(
                c,
                area,
                &ic::ROCKET,
                tr("Nothing starts with Windows"),
                tr("Programs set to open at sign-in will show here."),
                Some((tr("Refresh"), Hit::StartRefresh)),
            );
            return;
        }
        let scroll = self.scroll();
        let on = list.iter().filter(|e| e.enabled).count();
        c.clip(area);
        let mut y = area.y - scroll;
        // the section title, and the test notice when tests point the page at test data
        let head = fill(tr("{} programs, {} on"), &[&list.len().to_string(), &on.to_string()]);
        c.text(&head, rect(x + 4.0, y, w / 2.0, 24.0), Font::Title, p.t1, Align::Left);
        if self.startup_src.is_test() {
            c.text(tr("Test mode: a test registry key and folder."), rect(x + w / 2.0, y, w / 2.0 - 4.0, 24.0), Font::Small, p.orange, Align::Right);
        }
        y += 24.0 + 12.0;
        let card = rect(x, y, w, list.len() as f32 * ROW + 16.0);
        c.fill_round(card, 18.0, p.card);
        c.stroke_round(card, 18.0, p.line, 1.0);
        y += 8.0;
        for (i, e) in list.iter().enumerate() {
            let r = rect(x + 8.0, y, w - 16.0, ROW);
            if r.bottom() >= area.y && r.y <= area.bottom() {
                if i > 0 {
                    c.hline(r.x + 52.0, r.y, r.w - 64.0, p.line);
                }
                let row_hit = Hit::StartRow(i);
                let hit = Hit::StartToggle(i);
                if self.hovered(&row_hit) || self.hovered(&hit) {
                    c.fill_round(r.inset_xy(0.0, 2.0), 10.0, p.hover);
                }
                let ir = rect(r.x + 12.0, r.y + (r.h - 24.0) / 2.0, 24.0, 24.0);
                match &e.exe {
                    Some(exe) if self.icons.contains_key(&sampler::icon_key(exe)) => {
                        let key = sampler::icon_key(exe);
                        let px = self.icons.get(&key);
                        draw::pixels(c, key, ir, px);
                    }
                    _ => c.icon(&ic::WINDOW, ir.inset(3.0), p.t2),
                }
                let tx = ir.right() + 16.0;
                let tw = r.right() - tx - 180.0;
                c.text(&e.name, rect(tx, r.y + 5.0, tw, 20.0), Font::BodyBold, if e.enabled { p.t1 } else { p.t2 }, Align::Left);
                let mut sub = String::new();
                if !e.publisher.is_empty() {
                    sub.push_str(&e.publisher);
                    sub.push_str("  ·  ");
                }
                sub.push_str(e.kind.place());
                // already running: how much memory it holds now
                if let Some(exe) = &e.exe {
                    let k = exe.to_lowercase();
                    if let Some(g) = self.groups.iter().find(|g| g.key == k) {
                        sub.push_str("  ·  ");
                        sub.push_str(&fill(tr("running, {}"), &[&model::size(g.mem)]));
                    }
                }
                c.text(&sub, rect(tx, r.y + 25.0, tw, 18.0), Font::Small, p.t3, Align::Left);
                // the switch
                let sw = rect(r.right() - 64.0, r.y + (r.h - 28.0) / 2.0, 48.0, 28.0);
                switch(c, &p, sw, e.enabled);
                let state = if e.enabled { tr("On") } else { tr("Off") };
                c.text(state, rect(sw.x - 112.0, r.y, 100.0, r.h), Font::Small, p.t3, Align::Right);
                self.hits.add(rect(r.x, r.y, sw.x - 120.0 - r.x, r.h), row_hit);
                self.hits.add(rect(sw.x - 8.0, r.y, sw.w + 16.0, r.h), hit);
            }
            y += ROW;
        }
        y += 8.0 + 24.0;
        c.unclip();
        self.content_h = y + scroll - area.y;
        self.scrollbar(c, area);
    }
}

/// An iOS-style switch: a pill, the accent when on, a white knob.
pub fn switch(c: &mut Canvas, p: &super::Pal, r: Rect, on: bool) {
    c.fill_round(r, r.h / 2.0, if on { p.accent } else { p.fill2 });
    let k = r.h - 4.0;
    let kx = if on { r.right() - 2.0 - k } else { r.x + 2.0 };
    c.fill_round(rect(kx, r.y + 2.0, k, k).offset(0.0, 1.0), k / 2.0, Color::hex(0).fade(0.12));
    c.fill_round(rect(kx, r.y + 2.0, k, k), k / 2.0, Color::hex(0xffffff));
}
