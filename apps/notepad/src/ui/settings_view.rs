//! The settings page.

use super::{App, Hit, draw};
use crate::{
    i18n::{fill, tr},
    model, pads, registry,
};
use plume_ui::{Align, Canvas, Cx, Font, Rect, icons, rect};

const ROW: f32 = 56.0;
const PAD: f32 = 22.0;

enum Row {
    Switch(&'static str, &'static str, Option<&'static str>, bool),
    Choice(&'static str, &'static str, Vec<&'static str>, usize),
    Button(&'static str, String, Option<String>, &'static str),
}

impl App {
    fn row_h(&self, c: &mut Canvas, r: &Row, w: f32) -> f32 {
        let sub = match r {
            Row::Switch(_, _, Some(s), _) => Some(tr(s).to_string()),
            Row::Button(_, _, s, _) => s.clone(),
            _ => None,
        };
        match sub {
            Some(s) => (ROW - 6.0).max(30.0 + c.text_height(&s, w, Font::Small) + 10.0),
            None => ROW,
        }
    }

    fn section(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, rows: Vec<Row>) -> f32 {
        let t = self.theme();
        c.text(title, rect(x + 4.0, y, w, 24.0), Font::Title, t.t1, Align::Left);
        let y = y + 32.0;
        let text_w = w - 2.0 * PAD - 230.0;
        let heights: Vec<f32> = rows.iter().map(|r| self.row_h(c, r, text_w)).collect();
        let h: f32 = heights.iter().sum::<f32>() + 8.0;
        let card = rect(x, y, w, h);
        c.fill_round(card, 16.0, t.card);
        c.stroke_round(card, 16.0, t.sep, 1.0);
        let mut ry = y + 4.0;
        for (i, (row, rh)) in rows.into_iter().zip(heights).enumerate() {
            if i > 0 {
                c.hline(x + PAD, ry, w - 2.0 * PAD, t.sep);
            }
            let r = rect(x + PAD, ry, w - 2.0 * PAD, rh);
            let label_box = |c: &mut Canvas, label: &str, sub: Option<&str>| {
                if let Some(sub) = sub {
                    c.text(label, rect(r.x, r.y + 9.0, text_w, 20.0), Font::Body, t.t1, Align::Left);
                    let sh = c.text_height(sub, text_w, Font::Small);
                    c.text_block(sub, rect(r.x, r.y + 30.0, text_w, sh), Font::Small, t.t3);
                } else {
                    c.text(label, rect(r.x, r.y, text_w + 120.0, r.h), Font::Body, t.t1, Align::Left);
                }
            };
            match row {
                Row::Switch(name, label, sub, on) => {
                    label_box(c, tr(label), sub.map(tr));
                    if name == "default" && self.default_busy {
                        c.text(tr("Waiting for Windows…"), r, Font::Small, t.t3, Align::Right);
                    } else {
                        draw::switch(c, &t, r, on);
                    }
                    self.hits.add(rect(r.right() - 60.0, r.y, 60.0, r.h), Hit::Toggle(name));
                }
                Row::Choice(name, label, labels, cur) => {
                    label_box(c, tr(label), None);
                    let hot = match &self.hover {
                        Some(Hit::Choice(n, i)) if *n == name => Some(*i),
                        _ => None,
                    };
                    let tl: Vec<&str> = labels.iter().map(|l| tr(l)).collect();
                    for (k, b) in draw::segments(c, &t, r, &tl, cur, hot).into_iter().enumerate() {
                        self.hits.add(b, Hit::Choice(name, k));
                    }
                }
                Row::Button(name, label, sub, button) => {
                    label_box(c, &label, sub.as_deref());
                    let bl = tr(button);
                    let bw = draw::pill_w(c, bl);
                    let b = rect(r.right() - bw, r.y + (r.h - 34.0) / 2.0, bw, 34.0);
                    let hit = Hit::Button(name);
                    draw::pill(c, &t, b, bl, 0, self.hovered(&hit), false, Font::BodyBold);
                    self.hits.add(b, hit);
                }
            }
            ry += rh;
        }
        y + h + 24.0
    }

    pub fn paint_settings(&mut self, c: &mut Canvas, area: Rect) {
        let t = self.theme();
        let s = self.sh.borrow().set.clone();
        let (run_on, npads) = (registry::run_on(&model::user_root()), self.sh.borrow().pads.list.len());
        let w = (area.w - 48.0).min(660.0);
        let x = area.x + (area.w - w) / 2.0;
        c.clip(area);
        let mut y = area.y + 20.0 - self.settings_scroll;
        let back = rect(x - 4.0, y, 36.0, 36.0);
        if self.hovered(&Hit::Back) {
            c.fill_round(back, 18.0, t.hover);
        }
        c.icon(&icons::BACK, back.centered(16.0, 16.0), t.t2);
        self.hits.add(back, Hit::Back);
        c.text(tr("Settings"), rect(x + 40.0, y, w, 36.0), Font::Hero, t.t1, Align::Left);
        y += 44.0;
        c.text(tr("Plume Notepad: plain text, tabs, and sticky pads."), rect(x + 40.0, y, w, 20.0), Font::Body, t.t3, Align::Left);
        y += 40.0;
        let idx = |v: &str, list: &[&str]| list.iter().position(|x| *x == v).unwrap_or(0);
        y = self.section(
            c,
            x,
            y,
            w,
            tr("Appearance"),
            vec![
                Row::Choice("theme", "Theme", vec!["System", "Dark", "Light"], idx(&s.theme, &["system", "dark", "light"])),
                Row::Choice("lang", "Language", vec!["System", "English", "Français"], idx(&s.lang, &["system", "en", "fr"])),
                Row::Choice("font", "Font", vec!["Fixed width", "Sans serif"], idx(&s.font, &["mono", "sans"])),
            ],
        );
        y = self.section(
            c,
            x,
            y,
            w,
            tr("Editor"),
            vec![
                Row::Switch("wrap", "Word wrap", None, s.wrap),
                Row::Switch("numbers", "Line numbers", None, s.numbers),
                Row::Switch("status", "Status bar", None, s.status),
            ],
        );
        y = self.section(
            c,
            x,
            y,
            w,
            tr("Windows Notepad"),
            vec![
                Row::Switch(
                    "default",
                    "Open Plume Notepad instead of Windows Notepad",
                    Some("For notepad.exe, the Run box and apps that open Notepad. Windows asks for an administrator's permission."),
                    self.default_on,
                ),
                Row::Button(
                    "default_apps",
                    tr("Default app for text files").to_string(),
                    Some(tr("Windows lets only you choose it: Plume Notepad is listed for .txt, .log, .ini, .md…").to_string()),
                    "Choose…",
                ),
            ],
        );
        y = self.section(
            c,
            x,
            y,
            w,
            tr("Sticky pads"),
            vec![
                Row::Switch("autostart", "Open my pads when I sign in", Some("Plume Notepad stays near the clock when its window is closed."), run_on),
                Row::Button("new_pad", tr("New pad").to_string(), Some(tr("Anywhere in Windows: Ctrl+Alt+N").to_string()), "New pad"),
                Row::Button("show_pads", if npads == 1 { tr("1 pad").to_string() } else { fill(tr("{n} pads"), &[&npads.to_string()]) }, None, "Show all"),
            ],
        );
        c.text(&format!("Plume Notepad {}", plume_update::VERSION), rect(x + 4.0, y, w, 20.0), Font::Small, t.t3, Align::Left);
        y += 40.0;
        self.settings_h = (y + self.settings_scroll - area.bottom()).max(0.0);
        c.unclip();
    }

    pub fn toggle(&mut self, name: &str, cx: &mut Cx) {
        match name {
            "wrap" => self.change_setting(|s| s.wrap = !s.wrap, cx),
            "numbers" => self.change_setting(|s| s.numbers = !s.numbers, cx),
            "status" => self.change_setting(|s| s.status = !s.status, cx),
            "default" => {
                let on = !self.default_on;
                self.set_default(on, cx);
            }
            "autostart" => {
                let on = !registry::run_on(&model::user_root());
                self.set_autostart(on, cx);
            }
            _ => {}
        }
    }

    pub fn choice(&mut self, name: &str, i: usize, cx: &mut Cx) {
        match name {
            "theme" => self.change_setting(|s| s.theme = ["system", "dark", "light"][i.min(2)].into(), cx),
            "lang" => self.change_setting(|s| s.lang = ["system", "en", "fr"][i.min(2)].into(), cx),
            "font" => self.change_setting(|s| s.font = ["mono", "sans"][i.min(1)].into(), cx),
            _ => {}
        }
    }

    pub fn button(&mut self, name: &str, cx: &mut Cx) {
        match name {
            "default_apps" => self.default_apps(cx),
            "new_pad" => self.new_pad(cx),
            "show_pads" => {
                let sh = self.sh.clone();
                let activate = !sh.borrow().noactivate;
                cx.after(move |_| pads::show_all(&sh, activate));
            }
            _ => {}
        }
    }
}
