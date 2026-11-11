//! The generator, the security report and the settings.

use super::{App, Hit, Modal, draw, draw::ic, strength_name};
use crate::{
    generator::{self, MAX_LEN, MAX_WORDS, MIN_LEN, MIN_WORDS},
    i18n::{fill, tr},
    settings::{AUTOLOCK, CLEAR, HOTKEYS},
    sys,
};
use plume_ui::{Align, Canvas, Color, Cx, Font, Rect, dialogs, icons, rect};
use zeroize::Zeroize;

const SEPARATORS: [&str; 4] = ["-", ".", "_", " "];

impl App {
    fn page_title(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, sub: &str) -> f32 {
        let t = self.t.clone();
        c.text(title, rect(x, y, w, 32.0), Font::Hero, t.t1, Align::Left);
        if sub.is_empty() {
            return y + 46.0;
        }
        let h = c.text_height(sub, w, Font::Body);
        c.text_block(sub, rect(x, y + 36.0, w, h), Font::Body, t.t3);
        y + 48.0 + h
    }

    fn group(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        let t = self.t.clone();
        c.text(title, rect(x, y, w, 22.0), Font::Title, t.t1, Align::Left);
        y + 32.0
    }

    /// A settings row: its label (and a line under it) on the left; returns the row for the control on the right.
    fn row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, sub: &str) -> Rect {
        let t = self.t.clone();
        let h = if sub.is_empty() { 48.0 } else { 62.0 };
        let lw = w * 0.56;
        if sub.is_empty() {
            c.text(label, rect(x, y, lw, h), Font::Body, t.t1, Align::Left);
        } else {
            c.text(label, rect(x, y + 9.0, lw, 22.0), Font::Body, t.t1, Align::Left);
            c.text(sub, rect(x, y + 32.0, lw, 20.0), Font::Small, t.t3, Align::Left);
        }
        c.hline(x, y + h, w, t.sep);
        rect(x, y, w, h)
    }

    fn toggle(&mut self, c: &mut Canvas, r: Rect, on: bool, hit: Hit) {
        let t = self.t.clone();
        let sw = rect(r.right() - 46.0, r.y + (r.h - 26.0) / 2.0, 46.0, 26.0);
        c.fill_round(sw, 13.0, if on { t.accent } else { t.fill2 });
        let k = if on { sw.right() - 23.0 } else { sw.x + 3.0 };
        c.fill_round(rect(k, sw.y + 3.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
        self.hits.add(r, hit);
    }

    /// Choices side by side on the right of a row.
    fn choices(&mut self, c: &mut Canvas, r: Rect, labels: &[&str], cur: usize, name: &'static str) {
        let t = self.t.clone();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::Small) + 24.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 6.0;
        let mut x = r.right() - total;
        let bg = rect(x, r.y + (r.h - 32.0) / 2.0, total, 32.0);
        c.fill_round(bg, 16.0, t.fill);
        x += 3.0;
        for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
            let b = rect(x, bg.y + 3.0, w, 26.0);
            let hit = Hit::Choice(name, i);
            if i == cur {
                c.fill_round(b, 13.0, t.accent);
            } else if self.hovered(&hit) {
                c.fill_round(b, 13.0, t.hover);
            }
            c.text(l, b, Font::Small, if i == cur { Color::hex(0xffffff) } else { t.t1 }, Align::Center);
            self.hits.add(b, hit);
            x += w;
        }
    }

    fn slider(&mut self, c: &mut Canvas, r: Rect, v: f32, name: &'static str) {
        let t = self.t.clone();
        let y = r.y + r.h / 2.0;
        c.fill_round(rect(r.x, y - 2.5, r.w, 5.0), 2.5, t.fill2);
        let w = r.w * v.clamp(0.0, 1.0);
        c.fill_round(rect(r.x, y - 2.5, w, 5.0), 2.5, t.accent);
        c.fill_round(rect(r.x + w - 9.0, y - 9.0, 18.0, 18.0), 9.0, Color::hex(0xffffff));
        c.stroke_round(rect(r.x + w - 9.0, y - 9.0, 18.0, 18.0), 9.0, t.sep, 1.0);
        self.hits.add(rect(r.x - 9.0, r.y, r.w + 18.0, r.h), Hit::Slider(name));
    }

    pub fn set_slider(&mut self, name: &str, v: f32) {
        let g = &mut self.set.generator;
        match name {
            "length" => g.length = MIN_LEN + ((MAX_LEN - MIN_LEN) as f32 * v).round() as u32,
            "words" => g.words = MIN_WORDS + ((MAX_WORDS - MIN_WORDS) as f32 * v).round() as u32,
            _ => return,
        }
        self.regenerate();
    }

    pub fn regenerate(&mut self) {
        self.generated.zeroize();
        *self.generated = generator::generate(&self.set.generator);
    }

    // ───────────────────────── generator ─────────────────────────

    pub fn paint_generator(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        let w = (r.w - 80.0).min(680.0);
        let x = r.x + 40.0;
        let mut y = r.y + 30.0;
        y = self.page_title(c, x, y, w, tr("Generator"), tr("Strong passwords and passphrases, made on this PC."));
        let g = self.set.generator.clone();
        // the result
        let card = rect(x, y, w, 132.0);
        c.fill_round(card, 18.0, t.card);
        c.stroke_round(card, 18.0, t.sep, 1.0);
        let pw = self.generated.clone();
        let size = if pw.chars().count() > 40 {
            15.0
        } else if pw.chars().count() > 26 {
            18.0
        } else {
            22.0
        };
        draw::text(c, &pw, rect(x + 24.0, y + 18.0, w - 48.0, 44.0), size, 600, t.t1, Align::Left, true);
        let (s, bits) = generator::strength(&pw);
        let _ = s;
        let gb = generator::generator_bits(&g);
        self.strength_bar(c, x + 24.0, y + 70.0, 300.0, &pw);
        c.text(
            &fill(tr("{n} bits of randomness"), &[&format!("{:.0}", gb.min(bits.max(gb)))]),
            rect(x + 340.0, y + 68.0, w - 364.0, 20.0),
            Font::Small,
            t.t3,
            Align::Left,
        );
        let mut bx = x + 24.0;
        let by = y + 92.0 - 2.0;
        bx += self.icon_pill(c, bx, by, 34.0, &icons::RELOAD, tr("New"), 0, Hit::Gen("new")) + 8.0;
        self.icon_pill(c, bx, by, 34.0, &icons::COPY, tr("Copy"), 1, Hit::Gen("copy"));
        y += card.h + 24.0;
        // the options
        let row = self.row(c, x, y, w, tr("Kind"), "");
        self.choices(c, row, &[tr("Password"), tr("Passphrase")], g.passphrase as usize, "genmode");
        y += row.h;
        if g.passphrase {
            let row = self.row(c, x, y, w, &fill(tr("Words: {n}"), &[&g.words.to_string()]), "");
            self.slider(c, rect(row.right() - 260.0, row.y + 10.0, 250.0, 28.0), (g.words - MIN_WORDS) as f32 / (MAX_WORDS - MIN_WORDS) as f32, "words");
            y += row.h;
            let row = self.row(c, x, y, w, tr("Between the words"), "");
            let cur = SEPARATORS.iter().position(|s| *s == g.separator).unwrap_or(0);
            self.choices(c, row, &["-", ".", "_", tr("space")], cur, "sep");
            y += row.h;
            for (name, label, on) in [("capitalize", tr("Capital letters"), g.capitalize), ("number", tr("Add a digit"), g.number)] {
                let row = self.row(c, x, y, w, label, "");
                self.toggle(c, row, on, Hit::Gen(name));
                y += row.h;
            }
        } else {
            let row = self.row(c, x, y, w, &fill(tr("Length: {n}"), &[&g.length.to_string()]), "");
            self.slider(c, rect(row.right() - 260.0, row.y + 10.0, 250.0, 28.0), (g.length - MIN_LEN) as f32 / (MAX_LEN - MIN_LEN) as f32, "length");
            y += row.h;
            for (name, label, on) in [
                ("upper", tr("Capital letters (A-Z)"), g.upper),
                ("lower", tr("Small letters (a-z)"), g.lower),
                ("digits", tr("Digits (0-9)"), g.digits),
                ("symbols", tr("Symbols (!@#$…)"), g.symbols),
                ("lookalikes", tr("Leave out look-alikes (0 O, 1 l I)"), g.no_lookalikes),
            ] {
                let row = self.row(c, x, y, w, label, "");
                self.toggle(c, row, on, Hit::Gen(name));
                y += row.h;
            }
        }
        let _ = strength_name;
    }

    // ───────────────────────── report ─────────────────────────

    pub fn paint_report(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        if self.report.is_none() {
            self.compute_report();
        }
        let (weak, reused) = self.report.clone().unwrap_or_default();
        let w = (r.w - 80.0).min(720.0);
        let x = r.x + 40.0;
        c.clip(r);
        let mut y = r.y + 30.0 - self.page_scroll;
        y = self.page_title(c, x, y, w, tr("Security report"), tr("Passwords that are easy to guess, and passwords used for more than one site."));
        let reused_n: usize = reused.iter().map(|g| g.len()).sum();
        let no_2fa = self.vault.data.entries.iter().filter(|e| e.kind == crate::vault::Kind::Login && e.totp.is_empty()).count();
        let tiles = [
            (weak.len(), tr("Weak passwords"), t.orange, &ic::WARN),
            (reused_n, tr("Reused passwords"), t.red, &ic::REPEAT),
            (no_2fa, tr("Logins without 2FA"), t.t3, &ic::CLOCK),
        ];
        let tw = (w - 24.0) / 3.0;
        for (k, (n, label, col, icon)) in tiles.into_iter().enumerate() {
            let tr_ = rect(x + k as f32 * (tw + 12.0), y, tw, 92.0);
            c.fill_round(tr_, 16.0, t.card);
            c.stroke_round(tr_, 16.0, t.sep, 1.0);
            c.icon(icon, rect(tr_.x + 18.0, tr_.y + 18.0, 18.0, 18.0), if n == 0 { t.green } else { col });
            draw::text(c, &n.to_string(), rect(tr_.x + 18.0, tr_.y + 40.0, tw - 36.0, 32.0), 26.0, 700, t.t1, Align::Left, false);
            c.text(label, rect(tr_.x + 46.0, tr_.y + 14.0, tw - 60.0, 26.0), Font::SmallBold, t.t2, Align::Left);
        }
        y += 116.0;
        if weak.is_empty() && reused.is_empty() {
            c.icon(&ic::SHIELD, rect(x, y + 4.0, 20.0, 20.0), t.green);
            c.text(tr("All good: no weak or reused passwords."), rect(x + 30.0, y, w - 30.0, 28.0), Font::Body, t.t1, Align::Left);
            y += 40.0;
        }
        if !weak.is_empty() {
            y = self.group(c, x, y, w, tr("Weak"));
            for (id, s) in &weak {
                let Some(e) = self.entry(id).cloned() else { continue };
                let row = rect(x, y, w, 48.0);
                let hit = Hit::Open(id.clone());
                if self.hovered(&hit) {
                    c.fill_round(row, 10.0, t.hover);
                }
                c.text(&e.title, rect(x + 14.0, y, w * 0.5, 48.0), Font::BodyBold, t.t1, Align::Left);
                c.text(&e.username, rect(x + w * 0.5, y, w * 0.3, 48.0), Font::Small, t.t3, Align::Left);
                let name = strength_name(generator::Strength(*s));
                c.text(name, rect(x + w * 0.8, y, w * 0.2 - 14.0, 48.0), Font::SmallBold, if *s == 0 { t.red } else { t.orange }, Align::Right);
                self.hits.add(row, hit);
                y += 50.0;
                let mut e = e;
                e.wipe();
            }
            y += 16.0;
        }
        if !reused.is_empty() {
            y = self.group(c, x, y, w, tr("Reused"));
            for (k, group) in reused.iter().enumerate() {
                c.text(&fill(tr("Same password, {n} items"), &[&group.len().to_string()]), rect(x, y, w, 22.0), Font::SmallBold, t.t3, Align::Left);
                y += 26.0;
                for id in group {
                    let Some(e) = self.entry(id).cloned() else { continue };
                    let row = rect(x, y, w, 44.0);
                    let hit = Hit::Open(id.clone());
                    if self.hovered(&hit) {
                        c.fill_round(row, 10.0, t.hover);
                    }
                    c.text(&e.title, rect(x + 14.0, y, w * 0.5, 44.0), Font::BodyBold, t.t1, Align::Left);
                    c.text(&e.username, rect(x + w * 0.5, y, w * 0.5 - 14.0, 44.0), Font::Small, t.t3, Align::Left);
                    self.hits.add(row, hit);
                    y += 46.0;
                    let mut e = e;
                    e.wipe();
                }
                if k + 1 < reused.len() {
                    y += 10.0;
                }
            }
        }
        self.page_h = y + 30.0 + self.page_scroll - r.y;
        self.page_scroll = self.page_scroll.min((self.page_h - r.h).max(0.0));
        c.unclip();
    }

    // ───────────────────────── settings ─────────────────────────

    pub fn paint_settings(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        let w = (r.w - 80.0).min(720.0);
        let x = r.x + 40.0;
        c.clip(r);
        let mut y = r.y + 30.0 - self.page_scroll;
        y = self.page_title(c, x, y, w, tr("Settings"), "");
        let s = self.set.clone();

        y = self.group(c, x, y, w, tr("Security"));
        let row = self.row(c, x, y, w, tr("Lock after"), tr("Without using Plume Passwords"));
        let labels: Vec<String> = AUTOLOCK
            .iter()
            .map(|m| {
                if *m == 0 {
                    tr("Never").to_string()
                } else if *m >= 60 {
                    format!("{} h", m / 60)
                } else {
                    format!("{m} min")
                }
            })
            .collect();
        let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        self.choices(c, row, &refs, AUTOLOCK.iter().position(|m| *m == s.autolock_min).unwrap_or(2), "autolock");
        y += row.h;
        let row = self.row(c, x, y, w, tr("Lock when Windows locks"), "");
        self.toggle(c, row, s.lock_on_windows_lock, Hit::Toggle("winlock"));
        y += row.h;
        let row = self.row(c, x, y, w, tr("Clear the clipboard after"), tr("Copied secrets also stay out of the clipboard history"));
        let labels: Vec<String> = CLEAR.iter().map(|s| if *s >= 60 { format!("{} min", s / 60) } else { format!("{s} s") }).collect();
        let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        self.choices(c, row, &refs, CLEAR.iter().position(|v| *v == s.clear_clipboard_s).unwrap_or(1), "clear");
        y += row.h;
        let hello_sub = if self.hello_ok { tr("Face, fingerprint or PIN instead of the master password") } else { tr("Not set up on this PC") };
        let row = self.row(c, x, y, w, tr("Unlock with Windows Hello"), hello_sub);
        self.toggle(c, row, self.hello_on, Hit::Toggle("hello"));
        y += row.h;
        let row = self.row(c, x, y, w, tr("Master password"), tr("It cannot be recovered if you forget it"));
        let label = tr("Change…");
        let bw = c.text_width(label, Font::BodyBold) + 36.0;
        self.pill(c, rect(row.right() - bw, row.y + (row.h - 34.0) / 2.0, bw, 34.0), label, 0, Hit::Action("password"));
        y += row.h + 28.0;

        y = self.group(c, x, y, w, tr("Auto-type"));
        let sub = if self.hotkey_ok || !s.autotype {
            tr("Types the user name, Tab and the password into the window in front")
        } else {
            tr("Another program uses this shortcut: choose another one")
        };
        let row = self.row(c, x, y, w, tr("Auto-type shortcut"), sub);
        self.toggle(c, row, s.autotype, Hit::Toggle("autotype"));
        y += row.h;
        let row = self.row(c, x, y, w, tr("Shortcut"), "");
        let keys: Vec<&str> = HOTKEYS.iter().map(|h| h.0).collect();
        self.choices(c, row, &keys, s.hotkey, "hotkey");
        y += row.h;
        let row = self.row(c, x, y, w, tr("Press Enter after the password"), "");
        self.toggle(c, row, s.autotype_enter, Hit::Toggle("enter"));
        y += row.h + 28.0;

        y = self.group(c, x, y, w, tr("Your data"));
        for (label, sub, button, id, icon) in [
            (tr("Import passwords"), tr("A CSV file from Chrome, Edge, Firefox, Bitwarden or Plume Web"), tr("Import…"), "import", &ic::IMPORT),
            (tr("Encrypted backup"), tr("A copy of the vault that opens with your master password"), tr("Export…"), "export", &ic::EXPORT),
            (tr("Restore a backup"), tr("Adds the items of a backup to this vault"), tr("Restore…"), "restore", &ic::IMPORT),
        ] {
            let row = self.row(c, x, y, w, label, sub);
            let bw = c.text_width(button, Font::BodyBold) + 52.0;
            self.icon_pill(c, row.right() - bw, row.y + (row.h - 34.0) / 2.0, 34.0, icon, button, 0, Hit::Action(id));
            y += row.h;
        }
        y += 28.0;

        y = self.group(c, x, y, w, tr("App"));
        let row = self.row(c, x, y, w, tr("Keep running near the clock"), tr("Closing the window keeps auto-type working"));
        self.toggle(c, row, s.keep_running, Hit::Toggle("keep"));
        y += row.h;
        let row = self.row(c, x, y, w, tr("Start with Windows"), "");
        self.toggle(c, row, self.autostart, Hit::Toggle("autostart"));
        y += row.h;
        let row = self.row(c, x, y, w, tr("Theme"), "");
        let cur = ["system", "dark", "light"].iter().position(|v| *v == s.theme).unwrap_or(0);
        self.choices(c, row, &[tr("System"), tr("Dark"), tr("Light")], cur, "theme");
        y += row.h;
        let row = self.row(c, x, y, w, tr("Language"), "");
        let cur = ["system", "en", "fr"].iter().position(|v| *v == s.lang).unwrap_or(0);
        self.choices(c, row, &[tr("System"), "English", "Français"], cur, "lang");
        y += row.h + 24.0;
        c.text(&format!("Plume Passwords {}", plume_update::VERSION), rect(x, y, w, 20.0), Font::Small, t.t3, Align::Left);
        y += 22.0;
        c.text(tr("Nothing is sent anywhere: the vault stays on this PC."), rect(x, y, w, 20.0), Font::Small, t.t3, Align::Left);
        y += 40.0;
        self.page_h = y + self.page_scroll - r.y;
        self.page_scroll = self.page_scroll.min((self.page_h - r.h).max(0.0));
        c.unclip();
    }

    pub fn views_act(&mut self, hit: Hit, cx: &mut Cx) {
        match hit {
            Hit::Gen("new") => self.regenerate(),
            Hit::Gen("copy") => {
                let p = self.generated.clone();
                self.copy(&p, "generated", cx);
            }
            Hit::Gen(name) => {
                let g = &mut self.set.generator;
                match name {
                    "upper" => g.upper = !g.upper,
                    "lower" => g.lower = !g.lower,
                    "digits" => g.digits = !g.digits,
                    "symbols" => g.symbols = !g.symbols,
                    "lookalikes" => g.no_lookalikes = !g.no_lookalikes,
                    "capitalize" => g.capitalize = !g.capitalize,
                    "number" => g.number = !g.number,
                    _ => {}
                }
                self.set.save();
                self.regenerate();
            }
            Hit::Choice("genmode", i) => {
                self.set.generator.passphrase = i == 1;
                self.set.save();
                self.regenerate();
            }
            Hit::Choice("sep", i) => {
                self.set.generator.separator = SEPARATORS[i.min(3)].into();
                self.set.save();
                self.regenerate();
            }
            Hit::Choice("autolock", i) => {
                self.set.autolock_min = AUTOLOCK[i.min(AUTOLOCK.len() - 1)];
                self.settings_changed(cx);
            }
            Hit::Choice("clear", i) => {
                self.set.clear_clipboard_s = CLEAR[i.min(CLEAR.len() - 1)];
                self.settings_changed(cx);
            }
            Hit::Choice("hotkey", i) => {
                self.set.hotkey = i.min(HOTKEYS.len() - 1);
                self.settings_changed(cx);
            }
            Hit::Choice("theme", i) => {
                self.set.theme = ["system", "dark", "light"][i.min(2)].into();
                self.settings_changed(cx);
            }
            Hit::Choice("lang", i) => {
                self.set.lang = ["system", "en", "fr"][i.min(2)].into();
                self.settings_changed(cx);
            }
            Hit::Toggle("winlock") => {
                self.set.lock_on_windows_lock = !self.set.lock_on_windows_lock;
                self.settings_changed(cx);
            }
            Hit::Toggle("autotype") => {
                self.set.autotype = !self.set.autotype;
                self.settings_changed(cx);
            }
            Hit::Toggle("enter") => {
                self.set.autotype_enter = !self.set.autotype_enter;
                self.settings_changed(cx);
            }
            Hit::Toggle("keep") => {
                self.set.keep_running = !self.set.keep_running;
                self.settings_changed(cx);
            }
            Hit::Toggle("autostart") => {
                let on = !self.autostart;
                if sys::set_autostart(on) {
                    self.autostart = on;
                }
            }
            Hit::Toggle("hello") => {
                let on = !self.hello_on;
                self.set_hello(on, cx);
            }
            Hit::Action("password") => self.open_modal(Modal::ChangePassword, cx),
            Hit::Action("import") => {
                if let Some(p) = dialogs::pick_files(self.win.hwnd, tr("A CSV file of passwords")).into_iter().next() {
                    self.import_csv(&p, cx);
                }
            }
            Hit::Action("export") => {
                let name = format!("Plume Passwords backup {}.ppw", super::paint::date(crate::vault::now()));
                if let Some(p) = dialogs::save_as(self.win.hwnd, tr("Save the backup"), &name, "ppw", &[(tr("Plume Passwords backup"), "*.ppw")]) {
                    self.export_backup(&p, cx);
                }
            }
            Hit::Action("restore") => {
                if let Some(p) = dialogs::pick_files(self.win.hwnd, tr("A Plume Passwords backup")).into_iter().next() {
                    self.open_modal(Modal::Restore(p), cx);
                }
            }
            _ => {}
        }
        cx.repaint();
    }
}
