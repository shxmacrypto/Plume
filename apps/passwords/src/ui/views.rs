//! The generator, the security report and the settings.

use super::{App, Hit, Modal, draw, draw::ic, strength_name};
use crate::{
    generator::{self, MAX_LEN, MAX_WORDS, MIN_LEN, MIN_WORDS},
    i18n::{fill, tr},
    settings::{AUTOLOCK, CLEAR, HOTKEYS},
    sys,
};
use plume_ui::{Align, Canvas, Color, Cx, Font, Rect, dialogs, icons, icons::Icon, rect};
use zeroize::Zeroize;

const SEPARATORS: [&str; 4] = ["-", ".", "_", " "];

/// What sits on the right of a settings row.
enum Ctl {
    Toggle(bool, Hit),
    Choices(Vec<String>, usize, &'static str),
    Button(&'static str, Option<&'static Icon>, Hit),
    Slider(f32, &'static str),
}

struct Row {
    label: String,
    sub: String,
    ctl: Ctl,
}

fn row(label: &str, sub: &str, ctl: Ctl) -> Row {
    Row { label: label.to_string(), sub: sub.to_string(), ctl }
}

impl App {
    fn page_title(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, sub: &str) -> f32 {
        let t = self.t.clone();
        c.text(title, rect(x, y, w, 32.0), Font::Hero, t.t1, Align::Left);
        if sub.is_empty() {
            return y + 52.0;
        }
        let h = c.text_height(sub, w, Font::Body);
        c.text_block(sub, rect(x, y + 36.0, w, h), Font::Body, t.t3);
        y + 56.0 + h
    }

    fn group(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        let t = self.t.clone();
        c.text(title, rect(x + 4.0, y, w - 4.0, 22.0), Font::Title, t.t1, Align::Left);
        y + 32.0
    }

    fn card(&mut self, c: &mut Canvas, r: Rect) {
        let t = self.t.clone();
        c.fill_round(r, 16.0, t.card);
        c.stroke_round(r, 16.0, t.sep, 1.0);
    }

    /// Width of a row's control.
    fn ctl_w(&mut self, c: &mut Canvas, ctl: &Ctl) -> f32 {
        match ctl {
            Ctl::Toggle(..) => 46.0,
            Ctl::Choices(labels, ..) => {
                let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
                self.segmented_w(c, &refs, Font::Small)
            }
            Ctl::Button(label, icon, _) => c.text_width(label, Font::BodyBold) + if icon.is_some() { 52.0 } else { 36.0 },
            Ctl::Slider(..) => 240.0,
        }
    }

    /// Rows in a card: the label (and a line under it) on the left, the control on the right. Returns the y under the
    /// card.
    fn rows_card(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, rows: Vec<Row>) -> f32 {
        let t = self.t.clone();
        let hs: Vec<f32> = rows.iter().map(|r| if r.sub.is_empty() { 52.0 } else { 64.0 }).collect();
        let total: f32 = hs.iter().sum::<f32>() + 8.0;
        self.card(c, rect(x, y, w, total));
        let (ix, iw) = (x + 20.0, w - 40.0);
        let mut ry = y + 4.0;
        let n = rows.len();
        for (k, (r, h)) in rows.into_iter().zip(hs).enumerate() {
            let cw = self.ctl_w(c, &r.ctl);
            let lw = (iw - cw - 20.0).max(iw * 0.35);
            if r.sub.is_empty() {
                c.text(&r.label, rect(ix, ry, lw, h), Font::Body, t.t1, Align::Left);
            } else {
                c.text(&r.label, rect(ix, ry + 10.0, lw, 22.0), Font::Body, t.t1, Align::Left);
                c.text(&r.sub, rect(ix, ry + 33.0, lw, 20.0), Font::Small, t.t3, Align::Left);
            }
            let line = rect(ix, ry, iw, h);
            match r.ctl {
                Ctl::Toggle(on, hit) => self.toggle(c, line, on, hit),
                Ctl::Choices(labels, cur, name) => {
                    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
                    self.segmented(c, line.right() - cw, ry + (h - 32.0) / 2.0, &refs, cur, Font::Small, |i| Hit::Choice(name, i));
                }
                Ctl::Button(label, Some(icon), hit) => {
                    self.icon_pill(c, line.right() - cw, ry + (h - 34.0) / 2.0, 34.0, icon, label, 0, hit);
                }
                Ctl::Button(label, None, hit) => self.pill(c, rect(line.right() - cw, ry + (h - 34.0) / 2.0, cw, 34.0), label, 0, hit),
                Ctl::Slider(v, name) => self.slider(c, rect(line.right() - cw + 9.0, ry + (h - 28.0) / 2.0, cw - 18.0, 28.0), v, name),
            }
            if k + 1 < n {
                c.hline(ix, ry + h, iw, t.sep);
            }
            ry += h;
        }
        y + total + 14.0
    }

    fn toggle(&mut self, c: &mut Canvas, r: Rect, on: bool, hit: Hit) {
        let t = self.t.clone();
        let sw = rect(r.right() - 46.0, r.y + (r.h - 26.0) / 2.0, 46.0, 26.0);
        c.fill_round(sw, 13.0, if on { t.accent } else { t.fill2 });
        let k = if on { sw.right() - 23.0 } else { sw.x + 3.0 };
        c.fill_round(rect(k, sw.y + 3.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
        self.hits.add(r, hit);
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
        c.clip(r);
        let mut y = r.y + 30.0 - self.page_scroll;
        y = self.page_title(c, x, y, w, tr("Generator"), tr("Strong passwords and passphrases, made on this PC."));
        let g = self.set.generator.clone();
        // the result
        let card = rect(x, y, w, 156.0);
        self.card(c, card);
        let pw = self.generated.clone();
        let size = if pw.chars().count() > 40 {
            15.0
        } else if pw.chars().count() > 26 {
            18.0
        } else {
            22.0
        };
        draw::text(c, &pw, rect(x + 24.0, y + 20.0, w - 48.0, 40.0), size, 600, t.t1, Align::Left, true);
        let (_, bits) = generator::strength(&pw);
        let gb = generator::generator_bits(&g);
        self.strength_bar(c, x + 24.0, y + 66.0, 300.0, &pw);
        c.text(
            &fill(tr("{n} bits of randomness"), &[&format!("{:.0}", gb.min(bits.max(gb)))]),
            rect(x + 340.0, y + 64.0, w - 364.0, 20.0),
            Font::Small,
            t.t3,
            Align::Left,
        );
        let by = y + 156.0 - 24.0 - 34.0;
        let bw = self.icon_pill(c, x + 24.0, by, 34.0, &icons::RELOAD, tr("New"), 0, Hit::Gen("new"));
        self.icon_pill(c, x + 24.0 + bw + 10.0, by, 34.0, &icons::COPY, tr("Copy"), 1, Hit::Gen("copy"));
        y += card.h + 14.0;
        // the options
        let mut rows = vec![row(tr("Kind"), "", Ctl::Choices(vec![tr("Password").into(), tr("Passphrase").into()], g.passphrase as usize, "genmode"))];
        if g.passphrase {
            let v = (g.words - MIN_WORDS) as f32 / (MAX_WORDS - MIN_WORDS) as f32;
            rows.push(row(&fill(tr("Words: {n}"), &[&g.words.to_string()]), "", Ctl::Slider(v, "words")));
            let cur = SEPARATORS.iter().position(|s| *s == g.separator).unwrap_or(0);
            rows.push(row(tr("Between the words"), "", Ctl::Choices(vec!["-".into(), ".".into(), "_".into(), tr("space").into()], cur, "sep")));
            rows.push(row(tr("Capital letters"), "", Ctl::Toggle(g.capitalize, Hit::Gen("capitalize"))));
            rows.push(row(tr("Add a digit"), "", Ctl::Toggle(g.number, Hit::Gen("number"))));
        } else {
            let v = (g.length - MIN_LEN) as f32 / (MAX_LEN - MIN_LEN) as f32;
            rows.push(row(&fill(tr("Length: {n}"), &[&g.length.to_string()]), "", Ctl::Slider(v, "length")));
            rows.push(row(tr("Capital letters (A-Z)"), "", Ctl::Toggle(g.upper, Hit::Gen("upper"))));
            rows.push(row(tr("Small letters (a-z)"), "", Ctl::Toggle(g.lower, Hit::Gen("lower"))));
            rows.push(row(tr("Digits (0-9)"), "", Ctl::Toggle(g.digits, Hit::Gen("digits"))));
            rows.push(row(tr("Symbols (!@#$…)"), "", Ctl::Toggle(g.symbols, Hit::Gen("symbols"))));
            rows.push(row(tr("Leave out look-alikes (0 O, 1 l I)"), "", Ctl::Toggle(g.no_lookalikes, Hit::Gen("lookalikes"))));
        }
        y = self.rows_card(c, x, y, w, rows) + 16.0;
        self.page_h = y + self.page_scroll - r.y;
        self.page_scroll = self.page_scroll.min((self.page_h - r.h).max(0.0));
        c.unclip();
    }

    // ───────────────────────── report ─────────────────────────

    /// A row of the report, inside its card: the item, its user name, and a word on the right.
    #[allow(clippy::too_many_arguments)]
    fn report_row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, id: &str, right: Option<(&str, Color)>, last: bool) {
        let t = self.t.clone();
        let Some(e) = self.entry(id).cloned() else { return };
        let row = rect(x + 6.0, y + 4.0, w - 12.0, 44.0);
        let hit = Hit::Open(id.to_string());
        if self.hovered(&hit) {
            c.fill_round(row, 10.0, t.hover);
        }
        let (ix, iw) = (x + 20.0, w - 40.0);
        c.text(&e.title, rect(ix, y, iw * 0.45, 52.0), Font::BodyBold, t.t1, Align::Left);
        c.text(&e.username, rect(ix + iw * 0.45, y, iw * 0.3, 52.0), Font::Body, t.t3, Align::Left);
        if let Some((word, col)) = right {
            c.text(word, rect(ix + iw * 0.75, y, iw * 0.25 - 22.0, 52.0), Font::SmallBold, col, Align::Right);
        }
        c.icon(&icons::CHEVRON_RIGHT, rect(ix + iw - 14.0, y + 19.0, 14.0, 14.0), t.t3);
        if !last {
            c.hline(ix, y + 52.0, iw, t.sep);
        }
        self.hits.add(row, hit);
        let mut e = e;
        e.wipe();
    }

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
            (no_2fa, tr("Logins without 2FA"), t.t2, &ic::CLOCK),
        ];
        let tw = (w - 28.0) / 3.0;
        for (k, (n, label, col, icon)) in tiles.into_iter().enumerate() {
            let tile = rect(x + k as f32 * (tw + 14.0), y, tw, 96.0);
            self.card(c, tile);
            c.icon(icon, rect(tile.x + 20.0, tile.y + 20.0, 18.0, 18.0), if n == 0 { t.green } else { col });
            c.text(label, rect(tile.x + 46.0, tile.y + 16.0, tw - 62.0, 26.0), Font::SmallBold, t.t2, Align::Left);
            draw::text(c, &n.to_string(), rect(tile.x + 20.0, tile.y + 46.0, tw - 40.0, 32.0), 26.0, 700, t.t1, Align::Left, false);
        }
        y += 96.0 + 28.0;
        if weak.is_empty() && reused.is_empty() {
            let card = rect(x, y, w, 64.0);
            self.card(c, card);
            c.icon(&ic::SHIELD, rect(x + 20.0, y + 22.0, 20.0, 20.0), t.green);
            c.text(tr("All good: no weak or reused passwords."), rect(x + 50.0, y, w - 70.0, 64.0), Font::Body, t.t1, Align::Left);
            y += 64.0 + 14.0;
        }
        if !weak.is_empty() {
            y = self.group(c, x, y, w, tr("Weak"));
            let h = weak.len() as f32 * 52.0;
            self.card(c, rect(x, y, w, h));
            let n = weak.len();
            for (k, (id, s)) in weak.iter().enumerate() {
                let word = strength_name(generator::Strength(*s));
                self.report_row(c, x, y + k as f32 * 52.0, w, id, Some((word, if *s == 0 { t.red } else { t.orange })), k + 1 == n);
            }
            y += h + 28.0;
        }
        if !reused.is_empty() {
            y = self.group(c, x, y, w, tr("Reused"));
            for group in &reused {
                c.text(&fill(tr("Same password, {n} items"), &[&group.len().to_string()]), rect(x + 4.0, y, w, 20.0), Font::Small, t.t3, Align::Left);
                y += 26.0;
                let h = group.len() as f32 * 52.0;
                self.card(c, rect(x, y, w, h));
                let n = group.len();
                for (k, id) in group.iter().enumerate() {
                    self.report_row(c, x, y + k as f32 * 52.0, w, id, None, k + 1 == n);
                }
                y += h + 14.0;
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
        let autolock: Vec<String> = AUTOLOCK
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
        let clear: Vec<String> = CLEAR.iter().map(|s| if *s >= 60 { format!("{} min", s / 60) } else { format!("{s} s") }).collect();
        let hello_sub = if self.hello_ok { tr("Face, fingerprint or PIN instead of the master password") } else { tr("Not set up on this PC") };
        let rows = vec![
            row(
                tr("Lock after"),
                tr("Without using Plume Passwords"),
                Ctl::Choices(autolock, AUTOLOCK.iter().position(|m| *m == s.autolock_min).unwrap_or(2), "autolock"),
            ),
            row(tr("Lock when Windows locks"), "", Ctl::Toggle(s.lock_on_windows_lock, Hit::Toggle("winlock"))),
            row(
                tr("Clear the clipboard after"),
                tr("Copied secrets also stay out of the clipboard history"),
                Ctl::Choices(clear, CLEAR.iter().position(|v| *v == s.clear_clipboard_s).unwrap_or(1), "clear"),
            ),
            row(tr("Unlock with Windows Hello"), hello_sub, Ctl::Toggle(self.hello_on, Hit::Toggle("hello"))),
            row(tr("Master password"), tr("It cannot be recovered if you forget it"), Ctl::Button(tr("Change…"), None, Hit::Action("password"))),
        ];
        y = self.rows_card(c, x, y, w, rows) + 14.0;

        y = self.group(c, x, y, w, tr("Auto-type"));
        let sub = if self.hotkey_ok || !s.autotype {
            tr("Types the user name, Tab and the password into the window in front")
        } else {
            tr("Another program uses this shortcut: choose another one")
        };
        let keys: Vec<String> = HOTKEYS.iter().map(|h| h.0.to_string()).collect();
        let rows = vec![
            row(tr("Auto-type shortcut"), sub, Ctl::Toggle(s.autotype, Hit::Toggle("autotype"))),
            row(tr("Shortcut"), "", Ctl::Choices(keys, s.hotkey, "hotkey")),
            row(tr("Press Enter after the password"), "", Ctl::Toggle(s.autotype_enter, Hit::Toggle("enter"))),
        ];
        y = self.rows_card(c, x, y, w, rows) + 14.0;

        y = self.group(c, x, y, w, tr("Your data"));
        let rows = vec![
            row(
                tr("Import passwords"),
                tr("A CSV file from Chrome, Edge, Firefox, Bitwarden or Plume Web"),
                Ctl::Button(tr("Import…"), Some(&ic::IMPORT), Hit::Action("import")),
            ),
            row(
                tr("Encrypted backup"),
                tr("A copy of the vault that opens with your master password"),
                Ctl::Button(tr("Export…"), Some(&ic::EXPORT), Hit::Action("export")),
            ),
            row(tr("Restore a backup"), tr("Adds the items of a backup to this vault"), Ctl::Button(tr("Restore…"), Some(&ic::IMPORT), Hit::Action("restore"))),
        ];
        y = self.rows_card(c, x, y, w, rows) + 14.0;

        y = self.group(c, x, y, w, tr("App"));
        let rows = vec![
            row(tr("Keep running near the clock"), tr("Closing the window keeps auto-type working"), Ctl::Toggle(s.keep_running, Hit::Toggle("keep"))),
            row(tr("Start with Windows"), "", Ctl::Toggle(self.autostart, Hit::Toggle("autostart"))),
            row(
                tr("Theme"),
                "",
                Ctl::Choices(
                    vec![tr("System").into(), tr("Dark").into(), tr("Light").into()],
                    ["system", "dark", "light"].iter().position(|v| *v == s.theme).unwrap_or(0),
                    "theme",
                ),
            ),
            row(
                tr("Language"),
                "",
                Ctl::Choices(
                    vec![tr("System").into(), "English".into(), "Français".into()],
                    ["system", "en", "fr"].iter().position(|v| *v == s.lang).unwrap_or(0),
                    "lang",
                ),
            ),
        ];
        y = self.rows_card(c, x, y, w, rows) + 10.0;
        c.text(&format!("Plume Passwords {}", plume_update::VERSION), rect(x + 4.0, y, w, 20.0), Font::Small, t.t3, Align::Left);
        y += 22.0;
        c.text(tr("Nothing is sent anywhere: the vault stays on this PC."), rect(x + 4.0, y, w, 20.0), Font::Small, t.t3, Align::Left);
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
