//! The Settings page: language, theme, how often the numbers refresh, how the list is laid out, and what Plume Task
//! costs itself.

use super::{App, Hit};
use crate::{
    i18n::{fill, tr},
    model,
};
use plume_ui::{Align, Canvas, Cx, Font, Rect, rect};

const LANGS: [(&str, &str); 3] = [("system", "Same as Windows"), ("en", "English"), ("fr", "Français")];
const THEMES: [(&str, &str); 3] = [("system", "Same as Windows"), ("dark", "Dark"), ("light", "Light")];
const SPEEDS: [(u32, &str); 4] = [(500, "Fast (½ s)"), (1000, "Normal (1 s)"), (2000, "Slow (2 s)"), (0, "Paused")];

impl App {
    pub fn paint_settings(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let top = self.page_title(c, page, tr("Settings"), "");
        let x = page.x + 24.0;
        let w = (page.w - 48.0).min(760.0);
        let area = rect(page.x, top, page.w, page.h - top);
        self.view_h = area.h;
        let scroll = self.scroll();
        c.clip(area);
        let mut y = area.y + 4.0 - scroll;
        let lang = LANGS.iter().position(|l| l.0 == self.set.lang).unwrap_or(0);
        let theme = THEMES.iter().position(|t| t.0 == self.set.theme).unwrap_or(0);
        let speed = SPEEDS.iter().position(|s| s.0 == self.set.interval).unwrap_or(1);
        y = self.choice_card(c, x, y, w, tr("Language"), "", "lang", &LANGS.map(|l| if l.0 == "system" { tr(l.1) } else { l.1 }), lang);
        y = self.choice_card(c, x, y, w, tr("Theme"), "", "theme", &THEMES.map(|t| tr(t.1)), theme);
        y = self.choice_card(
            c,
            x,
            y,
            w,
            tr("Refresh"),
            tr("How often the numbers are read again. Slower uses even less of the processor."),
            "speed",
            &SPEEDS.map(|s| tr(s.1)),
            speed,
        );
        // the list's layout
        let r = rect(x, y, w, 70.0);
        c.fill_round(r, 14.0, p.card);
        c.stroke_round(r, 14.0, p.line, 1.0);
        c.text(tr("Apps apart from background processes"), rect(r.x + 20.0, r.y + 12.0, r.w - 120.0, 22.0), Font::BodyBold, p.t1, Align::Left);
        c.text(tr("Programs with a window come first, in their own section."), rect(r.x + 20.0, r.y + 36.0, r.w - 120.0, 18.0), Font::Small, p.t3, Align::Left);
        let on = self.set.sections;
        let sw = rect(r.right() - 64.0, r.y + 23.0, 44.0, 24.0);
        c.fill_round(sw, 12.0, if on { p.accent } else { p.t1.fade(0.2) });
        c.fill_round(rect(if on { sw.right() - 22.0 } else { sw.x + 2.0 }, sw.y + 2.0, 20.0, 20.0), 10.0, plume_ui::Color::hex(0xffffff));
        self.hits.add(rect(sw.x - 6.0, r.y, sw.w + 12.0, r.h), Hit::Toggle("sections"));
        y += 70.0 + 14.0;
        // about
        let r = rect(x, y, w, 96.0);
        c.fill_round(r, 14.0, p.card);
        c.stroke_round(r, 14.0, p.line, 1.0);
        c.text(&format!("Plume Task {}", plume_update::VERSION), rect(r.x + 20.0, r.y + 12.0, r.w - 40.0, 22.0), Font::BodyBold, p.t1, Align::Left);
        let own = fill(tr("Plume Task itself uses {} of the processor (average of the last minute)."), &[&model::percent(self.own_cpu_avg())]);
        c.text(&own, rect(r.x + 20.0, r.y + 38.0, r.w - 40.0, 18.0), Font::Small, p.t2, Align::Left);
        c.text(
            tr("Everything stays on this PC: Plume Task sends nothing online."),
            rect(r.x + 20.0, r.y + 60.0, r.w - 40.0, 18.0),
            Font::Small,
            p.t3,
            Align::Left,
        );
        y += 96.0 + 20.0;
        c.unclip();
        self.content_h = y + scroll - area.y;
        self.scrollbar(c, area);
    }

    #[allow(clippy::too_many_arguments)]
    fn choice_card(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str, sub: &str, key: &'static str, options: &[&str], current: usize) -> f32 {
        let p = self.pal.clone();
        let h = if sub.is_empty() { 96.0 } else { 116.0 };
        let r = rect(x, y, w, h);
        c.fill_round(r, 14.0, p.card);
        c.stroke_round(r, 14.0, p.line, 1.0);
        c.text(title, rect(r.x + 20.0, r.y + 12.0, r.w - 40.0, 22.0), Font::BodyBold, p.t1, Align::Left);
        let mut oy = r.y + 44.0;
        if !sub.is_empty() {
            c.text(sub, rect(r.x + 20.0, r.y + 34.0, r.w - 40.0, 18.0), Font::Small, p.t3, Align::Left);
            oy += 20.0;
        }
        let mut ox = r.x + 20.0;
        for (i, label) in options.iter().enumerate() {
            let bw = c.text_width(label, Font::Body) + 30.0;
            let b = rect(ox, oy, bw, 34.0);
            let hit = Hit::Choice(key, i);
            let on = i == current;
            let fill = if on {
                p.accent.fade(0.22)
            } else if self.hovered(&hit) {
                p.t1.fade(0.1)
            } else {
                p.t1.fade(0.05)
            };
            c.fill_round(b, 17.0, fill);
            if on {
                c.stroke_round(b, 17.0, p.accent, 1.5);
            }
            c.text(label, b, if on { Font::BodyBold } else { Font::Body }, p.t1, Align::Center);
            self.hits.add(b, hit);
            ox += bw + 8.0;
        }
        y + h + 14.0
    }

    pub fn settings_act(&mut self, hit: Hit, cx: &mut Cx) {
        match hit {
            Hit::Choice("lang", i) => self.set.lang = LANGS[i.min(2)].0.into(),
            Hit::Choice("theme", i) => self.set.theme = THEMES[i.min(2)].0.into(),
            Hit::Choice("speed", i) => self.set.interval = SPEEDS[i.min(3)].0,
            Hit::Toggle("sections") => self.set.sections = !self.set.sections,
            _ => return,
        }
        self.settings_changed(cx);
    }
}
