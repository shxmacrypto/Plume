//! The settings page.

use super::{App, Field, Hit, LIST_TOP, draw};
use crate::{
    i18n::{fill, tr},
    settings::{DAYS, IMAGE_MB, MAX_ITEMS},
};
use plume_ui::{Align, Canvas, Color, Font, Rect, icons, rect};

impl App {
    fn group(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        let t = self.theme();
        draw::text(c, title, rect(x, y, w, 28.0), 17.0, 700, t.t1, Align::Left);
        y + 36.0
    }

    /// A row: its label (and a line under it) on the left; returns the row's rect for the control on the right.
    fn row(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, label: &str, sub: &str) -> Rect {
        let t = self.theme();
        let h = if sub.is_empty() { 50.0 } else { 64.0 };
        if sub.is_empty() {
            c.text(label, rect(x, y, w * 0.5, h), Font::Body, t.t1, Align::Left);
        } else {
            c.text(label, rect(x, y + 9.0, w * 0.5, 22.0), Font::Body, t.t1, Align::Left);
            c.text(sub, rect(x, y + 33.0, w * 0.56, 20.0), Font::Small, t.t3, Align::Left);
        }
        c.hline(x, y + h, w, t.sep);
        rect(x, y, w, h)
    }

    fn switch(&mut self, c: &mut Canvas, r: Rect, on: bool, name: &'static str) {
        let t = self.theme();
        let sw = rect(r.right() - 46.0, r.y + (r.h - 26.0) / 2.0, 46.0, 26.0);
        c.fill_round(sw, 13.0, if on { t.accent } else { t.t1.fade(0.22) });
        let k = if on { sw.right() - 23.0 } else { sw.x + 3.0 };
        c.fill_round(rect(k, sw.y + 3.0, 20.0, 20.0), 10.0, Color::hex(0xffffff));
        self.hits.add(r, Hit::Toggle(name));
    }

    /// A segmented choice on the right of a row.
    fn choices(&mut self, c: &mut Canvas, r: Rect, labels: &[&str], cur: usize, name: &'static str) {
        let t = self.theme();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::Small) + 24.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 6.0;
        let bg = rect(r.right() - total, r.y + (r.h - 34.0) / 2.0, total, 34.0);
        c.fill_round(bg, 17.0, t.t1.fade(0.07));
        let mut x = bg.x + 3.0;
        for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
            let b = rect(x, bg.y + 3.0, w, 28.0);
            let hit = Hit::Choice(name, i);
            if i == cur {
                c.fill_round(b, 14.0, t.accent);
            } else if self.hovered(&hit) {
                c.fill_round(b, 14.0, t.t1.fade(0.08));
            }
            let ink = if i != cur {
                t.t1
            } else if t.dark {
                Color::hex(0x1a0420)
            } else {
                Color::hex(0xffffff)
            };
            c.text(l, b, Font::Small, ink, Align::Center);
            self.hits.add(b, hit);
            x += w;
        }
    }

    pub fn paint_settings(&mut self, c: &mut Canvas, content: Rect) {
        let t = self.theme();
        let area = rect(content.x, LIST_TOP, content.w, content.h - LIST_TOP);
        let x = area.x + 28.0;
        let w = (area.w - 56.0).min(780.0);
        let mut y = area.y + 4.0 - self.scroll;
        let set = self.model.borrow().set.clone();
        let (autostart, hk, hk_ok) = {
            let m = self.model.borrow();
            (m.autostart, m.hotkey.map(|h| h.label()), m.hotkey_ok)
        };
        c.clip(area);

        // the shortcut
        y = self.group(c, x, y, w, tr("Shortcut"));
        let r = self.row(c, x, y, w, tr("Open the panel"), tr("Search it, then Enter pastes into the app you were in."));
        let label = if self.capturing { tr("Press the new keys…").to_string() } else { hk.clone().unwrap_or_else(|| set.hotkey.clone()) };
        let bw = c.text_width(&label, Font::BodyBold) + 40.0;
        let hr = rect(r.right() - bw, r.y + 14.0, bw, 36.0);
        let hit = Hit::Hotkey;
        c.fill_round(
            hr,
            10.0,
            if self.capturing {
                t.accent.fade(0.2)
            } else if self.hovered(&hit) {
                t.fill2
            } else {
                t.fill
            },
        );
        if self.capturing {
            c.stroke_round(hr.inset(0.5), 10.0, t.accent, 1.5);
        }
        c.text(&label, hr, Font::BodyBold, t.t1, Align::Center);
        self.hits.add(hr, hit);
        if self.hotkey_error || (!hk_ok && !self.capturing && hk.is_some()) {
            c.text(tr("Another app uses it: try other keys."), rect(r.right() - bw - 300.0, r.y + 14.0, 290.0, 36.0), Font::Small, t.red, Align::Right);
        }
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Paste after picking"), tr("Off: the item is only put back on the clipboard."));
        self.switch(c, r, set.paste, "paste");
        y += r.h + 26.0;

        // the history
        y = self.group(c, x, y, w, tr("History"));
        let r = self.row(c, x, y, w, tr("Keep up to"), tr("Pinned items do not count."));
        let labels: Vec<String> = MAX_ITEMS.iter().map(|n| fill(tr("{n} items"), &[&n.to_string()])).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let cur = MAX_ITEMS.iter().position(|n| *n == set.max_items).unwrap_or(1);
        self.choices(c, r, &refs, cur, "max_items");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Keep for"), "");
        let cur = DAYS.iter().position(|n| *n == set.days).unwrap_or(2);
        self.choices(c, r, &[tr("1 day"), tr("7 days"), tr("30 days"), tr("1 year"), tr("Always")], cur, "days");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Largest picture"), "");
        let labels: Vec<String> = IMAGE_MB.iter().map(|n| format!("{n} MB")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let cur = IMAGE_MB.iter().position(|n| *n == set.max_image_mb).unwrap_or(1);
        self.choices(c, r, &refs, cur, "image_mb");
        y += r.h + 1.0;
        for (name, label, sub, on) in [
            ("images", tr("Keep pictures"), "", set.images),
            ("files", tr("Keep copied files"), tr("Their paths: the files themselves stay where they are."), set.files),
            ("encrypt", tr("Encrypt the history"), tr("Windows locks it to your account (DPAPI)."), set.encrypt),
            ("paused", tr("Pause"), tr("Nothing new is kept until you turn this off."), set.paused),
        ] {
            let r = self.row(c, x, y, w, label, sub);
            self.switch(c, r, on, name);
            y += r.h + 1.0;
        }
        let r = self.row(c, x, y, w, tr("Clear the history"), tr("Pinned items and snippets stay."));
        let confirm = self.confirm_clear.is_some();
        let label = if confirm { tr("Click again to clear") } else { tr("Clear") };
        let bw = Self::pill_w(c, label);
        self.pill(c, rect(r.right() - bw, r.y + 14.0, bw, 36.0), label, 2, Hit::Clear);
        y += r.h + 26.0;

        // privacy
        y = self.group(c, x, y, w, tr("Privacy"));
        let note = rect(x, y, w, 64.0);
        c.fill_round(note, 12.0, t.fill);
        c.icon(&draw::ic::SHIELD, rect(note.x + 14.0, note.y + 14.0, 18.0, 18.0), t.green);
        draw::text_box(
            c,
            tr("What apps mark as private is never kept: password managers mark the passwords they copy. Everything stays on this PC, nothing goes online."),
            rect(note.x + 44.0, note.y + 12.0, note.w - 58.0, 44.0),
            12.5,
            400,
            t.t2,
        );
        y += note.h + 10.0;
        let r = self.row(c, x, y, w, tr("Ignored apps"), tr("What you copy in these apps is never kept."));
        y += r.h + 10.0;
        let mut cx_ = x;
        let ignored = set.ignored.clone();
        for (i, app) in ignored.iter().enumerate() {
            let tw = c.text_width(app, Font::Small) + 44.0;
            if cx_ + tw > x + w {
                cx_ = x;
                y += 36.0;
            }
            let chip = rect(cx_, y, tw, 30.0);
            c.fill_round(chip, 15.0, t.fill);
            c.text(app, rect(chip.x + 12.0, chip.y, tw - 36.0, 30.0), Font::Small, t.t1, Align::Left);
            let xr = rect(chip.right() - 28.0, chip.y + 3.0, 24.0, 24.0);
            let hit = Hit::IgnoredRemove(i);
            if self.hovered(&hit) {
                c.fill_round(xr, 12.0, t.t1.fade(0.12));
            }
            c.icon(&icons::CLOSE, xr.centered(9.0, 9.0), t.t2);
            self.hits.add(xr, hit);
            cx_ += tw + 8.0;
        }
        if !ignored.is_empty() {
            y += 42.0;
        }
        let value = self.ignored_text.clone();
        let fr = rect(x, y, 240.0, 38.0);
        let on = self.edit_for == Some(Field::Ignored);
        c.fill_round(fr, 10.0, t.fill);
        if on {
            c.stroke_round(fr.inset(0.5), 10.0, t.accent.fade(0.7), 1.5);
        } else {
            let (s, col) = if value.is_empty() { ("app.exe", t.t3) } else { (value.as_str(), t.t1) };
            c.text(s, rect(fr.x + 12.0, fr.y, fr.w - 24.0, fr.h), Font::Body, col, Align::Left);
        }
        self.hits.add(fr, Hit::Field(Field::Ignored));
        if fr.y >= LIST_TOP {
            self.field_rects.push((Field::Ignored, fr));
        }
        let add = tr("Add");
        let aw = Self::pill_w(c, add);
        self.pill(c, rect(fr.right() + 10.0, y + 1.0, aw, 36.0), add, 0, Hit::IgnoredAdd);
        y += 64.0;

        // general
        y = self.group(c, x, y, w, tr("General"));
        let r = self.row(c, x, y, w, tr("Start with Windows"), tr("In the background, with its icon near the clock."));
        self.switch(c, r, autostart, "autostart");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Theme"), "");
        let cur = ["system", "dark", "light"].iter().position(|s| *s == set.theme).unwrap_or(0);
        self.choices(c, r, &[tr("System"), tr("Dark"), tr("Light")], cur, "theme");
        y += r.h + 1.0;
        let r = self.row(c, x, y, w, tr("Language"), "");
        let cur = ["system", "en", "fr"].iter().position(|s| *s == set.lang).unwrap_or(0);
        self.choices(c, r, &[tr("System"), "English", "Français"], cur, "lang");
        y += r.h + 26.0;
        c.text(&format!("Plume Clipboard {}", plume_update::VERSION), rect(x, y, w, 20.0), Font::Small, t.t3, Align::Left);
        y += 40.0;

        c.unclip();
        self.content_h = y + self.scroll - area.y;
        self.scroll = self.scroll.min((self.content_h - area.h).max(0.0));
    }
}
