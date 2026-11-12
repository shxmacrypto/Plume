//! The Settings page: language, theme, how often the numbers refresh, how the list is laid out, and what Plume Task
//! costs itself. Grouped cards of rows, a segmented control or a switch on the right of each row.

use super::{App, Hit, startup_view::switch};
use crate::{
    i18n::{fill, tr},
    model,
};
use plume_ui::{Align, Canvas, Cx, Font, Rect, rect};

const LANGS: [(&str, &str); 3] = [("system", "Same as Windows"), ("en", "English"), ("fr", "Français")];
const THEMES: [(&str, &str); 3] = [("system", "Same as Windows"), ("dark", "Dark"), ("light", "Light")];
const SPEEDS: [(u32, &str); 4] = [(500, "Fast (½ s)"), (1000, "Normal (1 s)"), (2000, "Slow (2 s)"), (0, "Paused")];

const ROW: f32 = 64.0;
const PAD: f32 = 20.0;

impl App {
    pub fn paint_settings(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let top = self.page_title(c, page, tr("Settings"), "");
        let x = page.x + 24.0;
        let w = (page.w - 48.0).min(820.0);
        let area = rect(page.x, top, page.w, page.h - top);
        self.view_h = area.h;
        let scroll = self.scroll();
        c.clip(area);
        let mut y = area.y - scroll;
        let lang = LANGS.iter().position(|l| l.0 == self.set.lang).unwrap_or(0);
        let theme = THEMES.iter().position(|t| t.0 == self.set.theme).unwrap_or(0);
        let speed = SPEEDS.iter().position(|s| s.0 == self.set.interval).unwrap_or(1);

        // appearance
        y = self.group_title(c, x, y, w, tr("Appearance"));
        let card = self.group_card(c, x, y, w, 2);
        let langs = LANGS.map(|l| if l.0 == "system" { tr(l.1) } else { l.1 });
        let r = self.row(c, card, 0, tr("Language"), "");
        self.segmented(c, r, &langs, lang, "lang");
        let r = self.row(c, card, 1, tr("Theme"), "");
        self.segmented(c, r, &THEMES.map(|t| tr(t.1)), theme, "theme");
        y = card.bottom() + 14.0;

        // the numbers
        y = self.group_title(c, x, y, w, tr("Numbers"));
        let card = self.group_card(c, x, y, w, 2);
        let r = self.row(c, card, 0, tr("Refresh"), tr("Slower uses even less of the processor."));
        self.segmented(c, r, &SPEEDS.map(|s| tr(s.1)), speed, "speed");
        let r = self.row(c, card, 1, tr("Apps apart from background processes"), tr("Programs with a window come first, in their own section."));
        let sw = rect(r.right() - 48.0, r.y + (r.h - 28.0) / 2.0, 48.0, 28.0);
        switch(c, &p, sw, self.set.sections);
        self.hits.add(rect(sw.x - 8.0, r.y, sw.w + 8.0, r.h), Hit::Toggle("sections"));
        y = card.bottom() + 14.0;

        // about
        y = self.group_title(c, x, y, w, tr("About"));
        let card = rect(x, y, w, 104.0);
        c.fill_round(card, 18.0, p.card);
        c.stroke_round(card, 18.0, p.line, 1.0);
        c.text(
            &format!("Plume Task {}", plume_update::VERSION),
            rect(card.x + PAD, card.y + 16.0, card.w - 2.0 * PAD, 22.0),
            Font::BodyBold,
            p.t1,
            Align::Left,
        );
        let own = fill(tr("Plume Task itself uses {} of the processor (average of the last minute)."), &[&model::percent(self.own_cpu_avg())]);
        c.text(&own, rect(card.x + PAD, card.y + 42.0, card.w - 2.0 * PAD, 20.0), Font::Body, p.t2, Align::Left);
        c.text(
            tr("Everything stays on this PC: Plume Task sends nothing online."),
            rect(card.x + PAD, card.y + 66.0, card.w - 2.0 * PAD, 20.0),
            Font::Body,
            p.t3,
            Align::Left,
        );
        y = card.bottom() + 24.0;
        c.unclip();
        self.content_h = y + scroll - area.y;
        self.scrollbar(c, area);
    }

    fn group_title(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        c.text(title, rect(x + 4.0, y, w, 24.0), Font::Title, self.pal.t1, Align::Left);
        y + 24.0 + 8.0
    }

    /// A card holding `rows` rows, separated by hairlines.
    fn group_card(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, rows: usize) -> Rect {
        let p = self.pal.clone();
        let card = rect(x, y, w, rows as f32 * ROW);
        c.fill_round(card, 18.0, p.card);
        c.stroke_round(card, 18.0, p.line, 1.0);
        for i in 1..rows {
            c.hline(card.x + PAD, card.y + i as f32 * ROW, card.w - 2.0 * PAD, p.line);
        }
        card
    }

    /// Row `i` of a card: its label (and a line under it) on the left; returns the area for the control.
    fn row(&mut self, c: &mut Canvas, card: Rect, i: usize, label: &str, sub: &str) -> Rect {
        let p = self.pal.clone();
        let r = rect(card.x + PAD, card.y + i as f32 * ROW, card.w - 2.0 * PAD, ROW);
        if sub.is_empty() {
            c.text(label, rect(r.x, r.y, r.w * 0.4, r.h), Font::Body, p.t1, Align::Left);
        } else {
            c.text(label, rect(r.x, r.y + 12.0, r.w * 0.5, 20.0), Font::Body, p.t1, Align::Left);
            c.text(sub, rect(r.x, r.y + 32.0, r.w * 0.5, 20.0), Font::Small, p.t3, Align::Left);
        }
        r
    }

    /// A segmented control on the right of a row: a pill track, the chosen segment raised.
    fn segmented(&mut self, c: &mut Canvas, r: Rect, labels: &[&str], cur: usize, key: &'static str) {
        let p = self.pal.clone();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::Small) + 28.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 8.0;
        let track = rect(r.right() - total, r.y + (r.h - 36.0) / 2.0, total, 36.0);
        c.fill_round(track, 18.0, p.fill);
        let mut x = track.x + 4.0;
        for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
            let b = rect(x, track.y + 4.0, w, 28.0);
            let hit = Hit::Choice(key, i);
            if i == cur {
                c.fill_round(b, 14.0, p.fill2.over(p.card));
                c.stroke_round(b, 14.0, p.ring, 1.0);
            } else if self.hovered(&hit) {
                c.fill_round(b, 14.0, p.hover);
            }
            c.text(l, b, if i == cur { Font::SmallBold } else { Font::Small }, if i == cur { p.t1 } else { p.t2 }, Align::Center);
            self.hits.add(b, hit);
            x += w;
        }
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
