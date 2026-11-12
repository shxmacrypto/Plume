//! Overview: the numbers as they are now (all read from Windows), the wake-up delay on demand, the numbers before and
//! after the last batch, and the three profiles to start from. No score: only things Windows can measure.

use super::{
    App, Hit,
    draw::ic,
    paint::{gb, ghz, ms},
};
use crate::{
    i18n::{fill, tr},
    tweaks::Profile,
};
use plume_ui::{Align, Canvas, Font, Rect, rect};

impl App {
    pub fn paint_overview(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let top =
            self.page_title(c, page, tr("Overview"), tr("Real numbers from this PC, read every second. Nothing changes until you tick it and press Apply."));
        let area = rect(page.x, top, page.w, page.h - top);
        self.view_h = area.h;
        let scroll = self.scroll();
        c.clip(area);
        let x = page.x + 24.0;
        let w = (page.w - 48.0).min(980.0);
        let mut y = area.y - scroll;
        let l = self.live.clone();

        // four tiles
        let gap = 14.0;
        let cols = if w > 760.0 { 4 } else { 2 };
        let tw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        let tiles: [(&str, String, String); 4] = [
            (tr("Power plan"), if l.plan.is_empty() { "…".into() } else { l.plan.clone() }, tr("Windows' active plan").into()),
            (
                tr("Processor clock"),
                l.cpu_mhz.map(ghz).unwrap_or_else(|| "…".into()),
                match (l.base_mhz, l.peak_mhz) {
                    (Some(b), Some(pk)) => fill(tr("base {} · peak {}"), &[&ghz(b), &ghz(pk)]),
                    (Some(b), None) => fill(tr("base {}"), &[&ghz(b)]),
                    _ => tr("average of all cores").into(),
                },
            ),
            (
                tr("Timer resolution"),
                l.timer.map(|t| ms(t.0)).unwrap_or_else(|| "…".into()),
                l.timer.map(|t| fill(tr("finest {} · coarsest {}"), &[&ms(t.1), &ms(t.2)])).unwrap_or_default(),
            ),
            (
                tr("Memory in use"),
                if l.mem_total > 0 { gb(l.mem_used) } else { "…".into() },
                if l.mem_total > 0 { fill(tr("of {} · {} %"), &[&gb(l.mem_total), &format!("{:.0}", l.mem_pct())]) } else { String::new() },
            ),
        ];
        for (i, (k, v, sub)) in tiles.iter().enumerate() {
            let (cx_, cy_) = (i % cols, i / cols);
            let r = rect(x + cx_ as f32 * (tw + gap), y + cy_ as f32 * (104.0 + gap), tw, 104.0);
            self.card(c, r);
            c.text(k, rect(r.x + 20.0, r.y + 16.0, r.w - 40.0, 18.0), Font::Small, p.t3, Align::Left);
            let big = if c.text_width(v, Font::Hero) <= r.w - 40.0 { Font::Hero } else { Font::Large };
            c.text(v, rect(r.x + 20.0, r.y + 36.0, r.w - 40.0, 32.0), big, p.t1, Align::Left);
            c.text(sub, rect(r.x + 20.0, r.y + 72.0, r.w - 40.0, 18.0), Font::Small, p.t3, Align::Left);
        }
        y += (4 / cols) as f32 * (104.0 + gap);

        // wake-up delay
        let r = rect(x, y, w, 112.0);
        self.card(c, r);
        c.text(tr("Wake-up delay"), rect(r.x + 24.0, r.y + 18.0, r.w - 220.0, 22.0), Font::Title, p.t1, Align::Left);
        c.text_block(
            tr("How late a 1 ms timer really wakes a program. Deep idle states and power saving make it later; High performance plans make it shorter."),
            rect(r.x + 24.0, r.y + 44.0, r.w * 0.55, 40.0),
            Font::Small,
            p.t3,
        );
        let label = if self.measuring { tr("Measuring…") } else { tr("Measure") };
        let bw = self.pill_w(c, label, true);
        self.pill(c, rect(r.right() - 24.0 - bw, r.y + 38.0, bw, 36.0), label, Some(&ic::GAUGE), None, !self.measuring, Hit::Measure);
        if let Some(wk) = self.wake {
            let vx = r.x + r.w * 0.58;
            c.text(&format!("{:.0} µs", wk.median_us), rect(vx, r.y + 26.0, 140.0, 32.0), Font::Hero, p.t1, Align::Left);
            c.text(
                &fill(tr("typical · {} µs at worst 1 in 100"), &[&format!("{:.0}", wk.p99_us)]),
                rect(vx, r.y + 62.0, r.right() - vx - bw - 40.0, 18.0),
                Font::Small,
                p.t3,
                Align::Left,
            );
        }
        y = r.bottom() + gap;

        // before and after the last batch
        if let Some((a, b)) = self.snaps.clone() {
            y = self.group_title(c, x, y + 6.0, w, tr("Before and after the last Apply"));
            let rows: Vec<(&str, String, String)> = vec![
                (tr("Power plan"), a.plan.clone(), b.plan.clone()),
                (tr("Processor clock"), a.cpu_mhz.map(ghz).unwrap_or("—".into()), b.cpu_mhz.map(ghz).unwrap_or("—".into())),
                (tr("Timer resolution"), a.timer.map(|t| ms(t.0)).unwrap_or("—".into()), b.timer.map(|t| ms(t.0)).unwrap_or("—".into())),
                (tr("Memory in use"), gb(a.mem_used), gb(b.mem_used)),
                (
                    tr("Wake-up delay (typical)"),
                    a.wake.map(|v| format!("{:.0} µs", v.median_us)).unwrap_or("—".into()),
                    b.wake.map(|v| format!("{:.0} µs", v.median_us)).unwrap_or("—".into()),
                ),
            ];
            let rh = 40.0;
            let r = rect(x, y, w, 44.0 + rows.len() as f32 * rh + 44.0);
            self.card(c, r);
            let (c1, c2, c3) = (r.x + 24.0, r.x + r.w * 0.42, r.x + r.w * 0.71);
            c.text(tr("Before"), rect(c2, r.y + 14.0, 200.0, 20.0), Font::SmallBold, p.t3, Align::Left);
            c.text(tr("After"), rect(c3, r.y + 14.0, 200.0, 20.0), Font::SmallBold, p.t3, Align::Left);
            let mut ry = r.y + 44.0;
            for (k, before, after) in rows {
                c.hline(r.x + 20.0, ry, r.w - 40.0, p.line);
                c.text(k, rect(c1, ry, c2 - c1 - 10.0, rh), Font::Body, p.t2, Align::Left);
                c.text(&before, rect(c2, ry, c3 - c2 - 10.0, rh), Font::Body, p.t1, Align::Left);
                c.text(&after, rect(c3, ry, r.right() - c3 - 20.0, rh), if after != before { Font::BodyBold } else { Font::Body }, p.t1, Align::Left);
                ry += rh;
            }
            let note = if self.be.is_test() {
                tr(
                    "Test mode: the plan comes from the imaginary PC; clocks, timer, memory and delay are this PC's real numbers, which Plume Boost did not change.",
                )
            } else {
                tr("Measured just before and just after. Clocks and memory move on their own; the wake-up delay shows what a power plan changes.")
            };
            c.text(note, rect(r.x + 24.0, ry + 8.0, r.w - 48.0, 28.0), Font::Small, p.t3, Align::Left);
            y = r.bottom() + gap;
        }

        // profiles
        y = self.group_title(c, x, y + 6.0, w, tr("Start from a profile"));
        let pw = (w - 2.0 * gap) / 3.0;
        for (i, prof) in Profile::ALL.into_iter().enumerate() {
            let r = rect(x + i as f32 * (pw + gap), y, pw, 92.0);
            let hit = Hit::StartProfile(i);
            self.card(c, r);
            if self.hovered(&hit) {
                c.fill_round(r, 18.0, p.hover);
            }
            let icon = match prof {
                Profile::Gaming => &ic::GAMEPAD,
                Profile::Balanced => &ic::GAUGE,
                Profile::Battery => &ic::BATTERY,
            };
            c.icon(icon, rect(r.x + 20.0, r.y + 20.0, 20.0, 20.0), p.accent);
            c.text(prof.title().get(), rect(r.x + 50.0, r.y + 18.0, r.w - 70.0, 24.0), Font::Title, p.t1, Align::Left);
            c.text_block(prof.line().get(), rect(r.x + 20.0, r.y + 50.0, r.w - 40.0, 36.0), Font::Small, p.t3);
            self.hits.add(r, hit);
        }
        y += 92.0 + gap;

        // rights
        let r = rect(x, y, w, 64.0);
        self.card(c, r);
        let (line, sub) = if self.admin {
            (tr("Running as administrator."), tr("Changes to the whole PC can be applied directly."))
        } else {
            (tr("Running with standard rights."), tr("Changes to the whole PC (marked Admin) will ask Windows for administrator rights first."))
        };
        c.icon(&ic::SHIELD, rect(r.x + 22.0, r.y + 22.0, 20.0, 20.0), if self.admin { p.accent } else { p.t3 });
        c.text(line, rect(r.x + 54.0, r.y + 12.0, r.w - 80.0, 20.0), Font::BodyBold, p.t1, Align::Left);
        c.text(sub, rect(r.x + 54.0, r.y + 32.0, r.w - 80.0, 20.0), Font::Small, p.t3, Align::Left);
        y = r.bottom() + 24.0;

        c.unclip();
        self.content_h = y + scroll - area.y;
        self.scrollbar(c, area);
    }
}
