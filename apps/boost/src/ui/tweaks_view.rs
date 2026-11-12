//! Tweaks: the three profiles, then every tweak by category — a tick box, its name and one line, what it needs
//! (administrator rights, a restart…), whether it is on now; a click opens what it does, gains, costs, the evidence
//! and the exact settings it writes. The bar at the bottom applies the ticked ones or undoes everything.

use super::{App, Hit, draw::ic};
use crate::{
    backend::guids,
    engine::{Status, setting_name, start_name},
    i18n::{fill, tr},
    tweaks::{After, Cat, Op, Profile, TWEAKS, Tweak, V},
};
use plume_ui::{Align, Canvas, Font, Rect, rect};

const ROW: f32 = 66.0;
const BAR: f32 = 84.0;

/// The settings a tweak writes, in plain technical words (shown in its details).
pub fn ops_text(t: &Tweak, plan_name: impl Fn(&str) -> String) -> Vec<String> {
    t.ops
        .iter()
        .map(|op| match *op {
            Op::Reg { hive, path, name, val, .. } => {
                let v = match val {
                    V::D(d) => d.to_string(),
                    V::S(s) => format!("\"{s}\""),
                    V::B(b) => b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" "),
                };
                format!("{}\\{}\\{} = {}", hive.short(), path, name, v)
            }
            Op::DxFlag { name, val } => format!(r"HKCU\Software\Microsoft\DirectX\UserGpuPreferences\DirectXUserGlobalSettings: {name}={val};"),
            Op::Plan(g) => format!("{}: {}", tr("Active power plan"), plan_name(g)),
            Op::Power { setting, ac, val } => {
                let v = if setting == guids::BOOST_MODE { boost_name(val).to_string() } else { format!("{val} %") };
                format!("{} ({}) = {}", setting_name(setting), if ac { tr("plugged in") } else { tr("on battery") }, v)
            }
            Op::Service { name, start } => format!("{} {name}: {}", tr("Service"), start_name(start)),
        })
        .collect()
}

pub fn boost_name(v: u32) -> &'static str {
    match v {
        0 => tr("Off"),
        1 => tr("On"),
        2 => tr("Aggressive"),
        3 => tr("Efficient"),
        4 => tr("Efficient aggressive"),
        _ => tr("Other"),
    }
}

impl App {
    fn detail_blocks(&self, t: &Tweak) -> Vec<(&'static str, String)> {
        let be = self.be.clone();
        let ops = ops_text(t, |g| be.plan_name(g).unwrap_or_else(|| g.to_string()));
        vec![
            (tr("What it does"), t.what.get().to_string()),
            (tr("What you gain"), t.gain.get().to_string()),
            (tr("What it costs"), t.cost.get().to_string()),
            (tr("Evidence"), t.evidence.get().to_string()),
            (crate::i18n::tf("How to check it", "Comment le vérifier"), t.check.get().to_string()),
            (tr("What Plume Boost writes"), ops.join("\n")),
        ]
    }

    pub fn paint_tweaks(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let top = self.page_title(c, page, tr("Tweaks"), tr("Tick what you want, then Apply. Every change can be undone from Changes."));
        let area = rect(page.x, top, page.w, page.h - top - BAR);
        self.view_h = area.h;
        let scroll = self.scroll();
        c.clip(area);
        let x = page.x + 24.0;
        let w = (page.w - 48.0).min(980.0);
        let mut y = area.y - scroll;
        let gap = 14.0;

        // profiles
        let pw = (w - 2.0 * gap) / 3.0;
        for (i, prof) in Profile::ALL.into_iter().enumerate() {
            let r = rect(x + i as f32 * (pw + gap), y, pw, 76.0);
            let on = self.set.profile == prof.key();
            let hit = Hit::Profile(i);
            if on {
                c.fill_round(r, 18.0, p.accent.fade(0.16).over(p.bg));
                c.stroke_round(r, 18.0, p.accent, 1.6);
            } else {
                self.card(c, r);
                if self.hovered(&hit) {
                    c.fill_round(r, 18.0, p.hover);
                }
            }
            let icon = match prof {
                Profile::Gaming => &ic::GAMEPAD,
                Profile::Balanced => &ic::GAUGE,
                Profile::Battery => &ic::BATTERY,
            };
            c.icon(icon, rect(r.x + 18.0, r.y + 17.0, 18.0, 18.0), if on { p.accent } else { p.t2 });
            c.text(prof.title().get(), rect(r.x + 46.0, r.y + 14.0, r.w - 60.0, 24.0), Font::BodyBold, p.t1, Align::Left);
            c.text(prof.line().get(), rect(r.x + 18.0, r.y + 44.0, r.w - 36.0, 20.0), Font::Small, p.t3, Align::Left);
            self.hits.add(r, hit);
        }
        y += 76.0 + 10.0;
        let hint = if self.set.profile.is_empty() {
            if self.ticked.is_empty() {
                tr("Pick a profile, or tick tweaks one by one.").to_string()
            } else {
                tr("Your own choice: a profile was changed by hand.").to_string()
            }
        } else {
            tr("A profile only ticks boxes: change any of them before applying.").to_string()
        };
        c.text(&hint, rect(x + 4.0, y, w, 20.0), Font::Small, p.t3, Align::Left);
        y += 20.0 + 18.0;

        // the tweaks, by category
        for cat in Cat::ALL {
            let items: Vec<usize> = (0..TWEAKS.len()).filter(|&i| TWEAKS[i].cat == cat).collect();
            if items.is_empty() {
                continue;
            }
            c.text(cat.title().get(), rect(x + 4.0, y, w, 24.0), Font::Title, p.t1, Align::Left);
            c.text(cat.note().get(), rect(x + 4.0, y + 24.0, w, 20.0), Font::Small, p.t3, Align::Left);
            y += 52.0;
            // heights first (details open or not)
            let inner = w - 56.0 - 24.0;
            let heights: Vec<f32> = items
                .iter()
                .map(|&i| {
                    let t = &TWEAKS[i];
                    if self.expanded.contains(t.id) {
                        let extra: f32 = self.detail_blocks(t).iter().map(|(_, s)| 20.0 + c.text_height(s, inner, Font::Body) + 12.0).sum();
                        ROW + extra + 8.0
                    } else {
                        ROW
                    }
                })
                .collect();
            let card = rect(x, y, w, heights.iter().sum());
            self.card(c, card);
            let mut ry = y;
            for (k, (&i, rh)) in items.iter().zip(heights).enumerate() {
                if k > 0 {
                    c.hline(card.x + 20.0, ry, card.w - 40.0, p.line);
                }
                // only rows in sight are drawn and clickable
                if ry + rh >= area.y && ry <= area.bottom() {
                    self.tweak_row(c, i, rect(card.x, ry, card.w, rh), inner);
                }
                ry += rh;
            }
            y = card.bottom() + 24.0;
        }
        // what isn't offered, and why
        use crate::i18n::tf;
        c.text(tf("No measurable effect: not offered", "Aucun effet mesurable : non proposé"), rect(x + 4.0, y, w, 24.0), Font::Title, p.t1, Align::Left);
        c.text(
            tf(
                "Popular \"optimizer\" switches, and why Plume Boost leaves them out. Every tweak above has a documented effect you can check.",
                "Des réglages « d'optimiseur » courants, et pourquoi Plume Boost les laisse de côté. Chaque réglage ci-dessus a un effet documenté et vérifiable.",
            ),
            rect(x + 4.0, y + 24.0, w, 20.0),
            Font::Small,
            p.t3,
            Align::Left,
        );
        y += 52.0;
        let inner = w - 48.0;
        let hs: Vec<f32> = crate::tweaks::NO_EFFECT.iter().map(|(_, b)| 22.0 + c.text_height(b.get(), inner, Font::Small) + 12.0).collect();
        let card = rect(x, y, w, hs.iter().sum::<f32>() + 24.0);
        self.card(c, card);
        let mut ny = card.y + 14.0;
        for ((t, b), h) in crate::tweaks::NO_EFFECT.iter().zip(hs) {
            c.text(t.get(), rect(card.x + 24.0, ny, inner, 20.0), Font::BodyBold, p.t2, Align::Left);
            let bh = c.text_height(b.get(), inner, Font::Small);
            c.text_block(b.get(), rect(card.x + 24.0, ny + 22.0, inner, bh), Font::Small, p.t3);
            ny += h;
        }
        y = card.bottom() + 24.0;
        c.unclip();
        self.content_h = y + scroll - area.y;
        self.scrollbar(c, area);
        self.paint_apply_bar(c, rect(page.x, page.bottom() - BAR, page.w, BAR));
    }

    fn tweak_row(&mut self, c: &mut Canvas, i: usize, r: Rect, inner: f32) {
        let p = self.pal.clone();
        let t = &TWEAKS[i];
        let st = self.statuses.get(i).copied().unwrap_or(Status::Off);
        let avail = st != Status::Unavailable;
        let open = self.expanded.contains(t.id);
        let head = rect(r.x, r.y, r.w, ROW);
        let hit = Hit::Expand(i);
        if self.hovered(&hit) {
            c.fill_round(head.inset(4.0), 12.0, p.hover);
        }
        self.hits.add(head, hit);
        let ticked = self.ticked.contains(&t.id);
        self.tickbox(c, rect(r.x + 20.0, r.y + (ROW - 22.0) / 2.0, 22.0, 22.0), ticked, avail, Hit::Tick(i));
        // right side: chevron, state, then what it needs
        let mut rx = r.right() - 16.0;
        c.icon(if open { &ic::CHEVRON_UP } else { &ic::CHEVRON }, rect(rx - 18.0, r.y + (ROW - 18.0) / 2.0, 18.0, 18.0), p.t3);
        rx -= 30.0;
        let (label, col) = match st {
            Status::On => (tr("On"), p.green),
            Status::Off => (tr("Off"), p.t3),
            Status::Partly => (tr("Partly"), p.orange),
            Status::Unavailable => (tr("Not on this PC"), p.t3),
        };
        let sw = c.text_width(label, Font::SmallBold) + 26.0;
        let sr = rect(rx - sw, r.y + (ROW - 24.0) / 2.0, sw, 24.0);
        c.fill_round(sr, 12.0, if st == Status::On { p.green.fade(0.16) } else { p.fill });
        c.fill_round(rect(sr.x + 10.0, sr.y + 9.0, 6.0, 6.0), 3.0, col);
        c.text(label, rect(sr.x + 20.0, sr.y, sw - 22.0, 24.0), Font::SmallBold, p.t1, Align::Left);
        rx = sr.x - 8.0;
        let by = r.y + (ROW - 24.0) / 2.0;
        match t.after {
            After::Restart => rx -= self.badge(c, rx, by, &ic::RESTART, tr("Restart"), p.orange, true) + 6.0,
            After::SignOut => rx -= self.badge(c, rx, by, &ic::SIGNOUT, tr("Sign out"), p.orange, true) + 6.0,
            After::Game => rx -= self.badge(c, rx, by, &ic::GAMEPAD, tr("Restart the game"), p.t2, true) + 6.0,
            After::Now => {}
        }
        if t.needs_admin() {
            rx -= self.badge(c, rx, by, &ic::SHIELD, tr("Admin"), p.accent, true) + 6.0;
        }
        let tx = r.x + 56.0;
        let tw = (rx - tx - 12.0).max(80.0);
        c.text(t.title.get(), rect(tx, r.y + 12.0, tw, 22.0), Font::BodyBold, if avail { p.t1 } else { p.t3 }, Align::Left);
        c.text(t.summary.get(), rect(tx, r.y + 34.0, tw, 20.0), Font::Small, p.t3, Align::Left);
        if open {
            let mut y = r.y + ROW;
            let dx = tx;
            for (label, text) in self.detail_blocks(t) {
                c.text(label, rect(dx, y, inner, 20.0), Font::SmallBold, if label == tr("What Plume Boost writes") { p.t3 } else { p.accent }, Align::Left);
                y += 20.0;
                let h = c.text_height(&text, inner, Font::Body);
                let mono = label == tr("What Plume Boost writes");
                c.text_block(&text, rect(dx, y, inner, h), if mono { Font::Small } else { Font::Body }, if mono { p.t3 } else { p.t1 });
                y += h + 12.0;
            }
        }
    }

    fn paint_apply_bar(&mut self, c: &mut Canvas, r: Rect) {
        let p = self.pal.clone();
        c.fill(r, p.bg);
        c.hline(r.x, r.y, r.w, p.line);
        let todo = self.to_apply();
        let n = todo.len();
        let admin = todo.iter().filter(|id| crate::tweaks::by_id(id).is_some_and(|t| t.needs_admin())).count();
        let restart = todo.iter().filter(|id| crate::tweaks::by_id(id).is_some_and(|t| matches!(t.after, After::Restart | After::SignOut))).count();
        let x = r.x + 24.0;
        let title = match n {
            0 if self.ticked.is_empty() => tr("Nothing ticked").to_string(),
            0 => tr("Everything ticked is on").to_string(),
            1 => tr("1 change to apply").to_string(),
            n => fill(tr("{} changes to apply"), &[&n.to_string()]),
        };
        c.text(&title, rect(x, r.y + 18.0, r.w * 0.5, 22.0), Font::BodyBold, p.t1, Align::Left);
        let mut parts: Vec<String> = Vec::new();
        if admin > 0 {
            parts.push(fill(tr("{} need administrator rights"), &[&admin.to_string()]));
        }
        if restart > 0 {
            parts.push(fill(tr("{} need a restart or sign-out"), &[&restart.to_string()]));
        }
        if parts.is_empty() {
            parts.push(tr("The current value of each setting is saved first.").to_string());
        }
        c.text(&parts.join(" · "), rect(x, r.y + 42.0, r.w * 0.55, 20.0), Font::Small, p.t3, Align::Left);
        let busy = self.busy.is_some();
        let label = if n > 1 { fill(tr("Apply {}"), &[&n.to_string()]) } else { tr("Apply").to_string() };
        let aw = self.pill_w(c, &label, true);
        let ar = rect(r.right() - 24.0 - aw, r.y + (r.h - 36.0) / 2.0, aw, 36.0);
        self.pill(c, ar, &label, Some(&ic::BOLT), Some(p.accent), n > 0 && !busy, Hit::Apply);
        let ul = tr("Undo everything");
        let uw = self.pill_w(c, ul, true);
        self.pill(c, rect(ar.x - 10.0 - uw, ar.y, uw, 36.0), ul, Some(&ic::HISTORY), None, !self.log.entries.is_empty() && !busy, Hit::UndoAll);
    }
}
