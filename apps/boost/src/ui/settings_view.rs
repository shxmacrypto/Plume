//! Settings: language, theme, the restore point offered before applying, and what Plume Boost is.

use super::{App, Hit};
use crate::i18n::tr;
use plume_ui::{Align, Canvas, Cx, Font, Rect, rect};

const LANGS: [(&str, &str); 3] = [("system", "Same as Windows"), ("en", "English"), ("fr", "Français")];
const THEMES: [(&str, &str); 3] = [("system", "Same as Windows"), ("dark", "Dark"), ("light", "Light")];
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

        y = self.group_title(c, x, y, w, tr("Appearance"));
        let card = self.group_card(c, x, y, w, 2);
        let langs = LANGS.map(|l| if l.0 == "system" { tr(l.1) } else { l.1 });
        let r = self.row(c, card, 0, tr("Language"), "");
        self.segmented(c, r, &langs, Some(lang), "lang");
        let r = self.row(c, card, 1, tr("Theme"), "");
        self.segmented(c, r, &THEMES.map(|t| tr(t.1)), Some(theme), "theme");
        y = card.bottom() + 14.0;

        y = self.group_title(c, x, y, w, tr("Safety"));
        let card = self.group_card(c, x, y, w, 2);
        let r = self.row(
            c,
            card,
            0,
            tr("Offer a System Restore point before applying"),
            tr("The Apply dialog starts with its switch on. A restore point needs administrator rights."),
        );
        let on = self.set.restore_point;
        self.switch(c, rect(r.right() - 48.0, r.y + (r.h - 28.0) / 2.0, 48.0, 28.0), on, Hit::Toggle("restore"));
        let r = self.row(
            c,
            card,
            1,
            crate::i18n::tf("Keep a copy of each snapshot with Plume Backup", "Garder une copie de chaque instantané avec Plume Backup"),
            crate::i18n::tf(
                "Snapshots are always saved on this PC; Plume Backup keeps dated copies elsewhere.",
                "Les instantanés sont toujours enregistrés sur ce PC ; Plume Backup en garde des copies datées ailleurs.",
            ),
        );
        let on = self.set.backup;
        self.switch(c, rect(r.right() - 48.0, r.y + (r.h - 28.0) / 2.0, 48.0, 28.0), on, Hit::Toggle("backup"));
        y = card.bottom() + 14.0;

        y = self.group_title(c, x, y, w, tr("About"));
        let card = rect(x, y, w, 128.0);
        self.card(c, card);
        c.text(
            &format!("Plume Boost {}", plume_update::VERSION),
            rect(card.x + PAD, card.y + 16.0, card.w - 2.0 * PAD, 22.0),
            Font::BodyBold,
            p.t1,
            Align::Left,
        );
        let lines = [
            tr("Only Windows' own settings, each one explained and reversible. Nothing is applied until you tick it and press Apply."),
            tr("Everything stays on this PC: Plume Boost sends nothing online."),
            if self.be.is_test() {
                tr("Test mode: an imaginary PC kept in a file. This PC is not changed.")
            } else {
                tr("Removing Plume Boost puts back everything it changed.")
            },
        ];
        let mut ly = card.y + 42.0;
        for l in lines {
            c.text(l, rect(card.x + PAD, ly, card.w - 2.0 * PAD, 20.0), Font::Body, p.t2, Align::Left);
            ly += 24.0;
        }
        y = card.bottom() + 24.0;
        c.unclip();
        self.content_h = y + scroll - area.y;
        self.scrollbar(c, area);
    }

    fn group_card(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, rows: usize) -> Rect {
        let p = self.pal.clone();
        let card = rect(x, y, w, rows as f32 * ROW);
        self.card(c, card);
        for i in 1..rows {
            c.hline(card.x + PAD, card.y + i as f32 * ROW, card.w - 2.0 * PAD, p.line);
        }
        card
    }

    fn row(&mut self, c: &mut Canvas, card: Rect, i: usize, label: &str, sub: &str) -> Rect {
        let p = self.pal.clone();
        let r = rect(card.x + PAD, card.y + i as f32 * ROW, card.w - 2.0 * PAD, ROW);
        if sub.is_empty() {
            c.text(label, rect(r.x, r.y, r.w * 0.4, r.h), Font::Body, p.t1, Align::Left);
        } else {
            c.text(label, rect(r.x, r.y + 12.0, r.w - 80.0, 20.0), Font::Body, p.t1, Align::Left);
            c.text(sub, rect(r.x, r.y + 32.0, r.w - 80.0, 20.0), Font::Small, p.t3, Align::Left);
        }
        r
    }

    pub fn settings_act(&mut self, hit: Hit, cx: &mut Cx) {
        match hit {
            Hit::Choice("lang", i) => self.set.lang = LANGS[i.min(2)].0.into(),
            Hit::Choice("theme", i) => self.set.theme = THEMES[i.min(2)].0.into(),
            Hit::Toggle("restore") => self.set.restore_point = !self.set.restore_point,
            Hit::Toggle("backup") => self.set.backup = !self.set.backup,
            _ => return,
        }
        self.settings_changed(cx);
    }
}
