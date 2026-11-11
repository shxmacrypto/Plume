//! The Startup apps page: what opens by itself at sign-in, each with a switch.

use super::{App, Hit, draw, draw::ic};
use crate::{i18n::tr, model, sampler};
use plume_ui::{Align, Canvas, Font, Rect, icons, rect};

impl App {
    pub fn paint_startup(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let top = self.page_title(c, page, tr("Startup apps"), tr("Programs that open by themselves when you sign in. Turning one off keeps it installed."));
        let x = page.x + 24.0;
        let w = page.w - 48.0;
        // the refresh button, top right (under the window buttons)
        let rw = self.pill_w(c, tr("Refresh"), true);
        self.pill(c, rect(x + w - rw, 44.0, rw, 32.0), tr("Refresh"), Some(&icons::RELOAD), None, !self.startup_busy, Hit::StartRefresh);
        let area = rect(page.x, top + 6.0, page.w, page.h - top - 6.0);
        self.view_h = area.h;
        let Some(list) = self.startup.clone() else {
            c.text(tr("Reading the startup list…"), rect(x, area.y + 30.0, w, 24.0), Font::Body, p.t3, Align::Center);
            self.content_h = 0.0;
            return;
        };
        if self.startup_src.is_test() {
            c.text(tr("Test mode: a test registry key and folder."), rect(x, area.y - 4.0, w, 18.0), Font::Small, p.orange, Align::Left);
        }
        let row_h = 64.0;
        let scroll = self.scroll();
        let on = list.iter().filter(|e| e.enabled).count();
        let head = format!("{} · {} {}", list.len(), on, tr("on"));
        c.clip(area);
        let mut y = area.y + 16.0 - scroll;
        c.text(&head, rect(x + 4.0, y, w, 18.0), Font::SmallBold, p.t3, Align::Left);
        y += 26.0;
        if list.is_empty() {
            c.text(tr("Nothing starts with Windows."), rect(x, y + 20.0, w, 24.0), Font::Body, p.t3, Align::Center);
        }
        for (i, e) in list.iter().enumerate() {
            let r = rect(x, y, w, row_h - 8.0);
            if r.bottom() >= area.y && r.y <= area.bottom() {
                c.fill_round(r, 12.0, p.card);
                c.stroke_round(r, 12.0, p.line, 1.0);
                let ir = rect(r.x + 16.0, r.y + (r.h - 28.0) / 2.0, 28.0, 28.0);
                match &e.exe {
                    Some(exe) if self.icons.contains_key(&sampler::icon_key(exe)) => {
                        let key = sampler::icon_key(exe);
                        let px = self.icons.get(&key);
                        draw::pixels(c, key, ir, px);
                    }
                    _ => c.icon(&ic::WINDOW, ir.inset(4.0), p.t3),
                }
                let tx = ir.right() + 14.0;
                let tw = r.right() - tx - 200.0;
                c.text(&e.name, rect(tx, r.y + 8.0, tw, 22.0), Font::BodyBold, if e.enabled { p.t1 } else { p.t2 }, Align::Left);
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
                        sub.push_str(&crate::i18n::fill(tr("running, {}"), &[&model::size(g.mem)]));
                    }
                }
                c.text(&sub, rect(tx, r.y + 29.0, tw, 18.0), Font::Small, p.t3, Align::Left);
                // the switch
                let sw = rect(r.right() - 64.0, r.y + (r.h - 24.0) / 2.0, 44.0, 24.0);
                let hit = Hit::StartToggle(i);
                let hot = self.hovered(&hit);
                let track = if e.enabled { p.accent } else { p.t1.fade(if hot { 0.28 } else { 0.2 }) };
                c.fill_round(sw, 12.0, track);
                let kx = if e.enabled { sw.right() - 22.0 } else { sw.x + 2.0 };
                c.fill_round(rect(kx, sw.y + 2.0, 20.0, 20.0), 10.0, plume_ui::Color::hex(0xffffff));
                let state = if e.enabled { tr("On") } else { tr("Off") };
                c.text(state, rect(sw.x - 120.0, r.y, 110.0, r.h), Font::Small, if e.enabled { p.t2 } else { p.t3 }, Align::Right);
                self.hits.add(rect(r.x, r.y, sw.x - 130.0 - r.x, r.h), Hit::StartRow(i));
                self.hits.add(rect(sw.x - 6.0, r.y, sw.w + 12.0, r.h), hit);
            }
            y += row_h;
        }
        c.unclip();
        self.content_h = y + scroll - area.y + 16.0;
        self.scrollbar(c, area);
    }
}
