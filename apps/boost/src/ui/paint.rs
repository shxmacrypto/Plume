//! The frame of the window: sidebar, caption buttons, dialogs, the message at the bottom, and the widgets the pages
//! share (pill buttons, switches, tick boxes, segmented controls, small badges, the scrollbar, empty states).

use super::{App, Hit, Modal, Page, SIDEBAR_W, Why, changed_title, draw, draw::ic};
use crate::{
    i18n::{fill, tf, tr},
    tweaks::{After, by_id},
};
use plume_ui::{Align, Canvas, Color, Font, PaintCx, Rect, icons, icons::Icon, m, rect};

/// One line of a dialog's body.
enum Line {
    Text(String, Color),
    Bullet(String, Color),
    Switch(String, String, bool),
    /// A progress bar (0..1) with a caption.
    Progress(f32, String),
}

impl App {
    pub fn hovered(&self, h: &Hit) -> bool {
        self.hover.as_ref() == Some(h)
    }

    /// A round button: `tone` None = quiet (fill), Some(color) = filled with that color. Disabled: faint, not clickable.
    #[allow(clippy::too_many_arguments)]
    pub fn pill(&mut self, c: &mut Canvas, r: Rect, label: &str, icon: Option<&Icon>, tone: Option<Color>, enabled: bool, hit: Hit) {
        let p = self.pal.clone();
        let hot = enabled && self.hovered(&hit);
        let down = hot && self.pressed.as_ref() == Some(&hit);
        let (fill, ink) = match tone {
            Some(t) if enabled => (
                if down {
                    t.mix(Color::hex(0), 0.16)
                } else if hot {
                    t.mix(Color::hex(0xffffff), 0.12)
                } else {
                    t
                },
                if t == p.accent { p.on_accent } else { Color::hex(0xffffff) },
            ),
            _ if enabled => (
                if down {
                    p.fill2.mix(p.t1, 0.06)
                } else if hot {
                    p.fill2
                } else {
                    p.fill
                },
                p.t1,
            ),
            _ => (p.fill, p.t4),
        };
        c.fill_round(r, r.h / 2.0, fill);
        match icon {
            Some(icn) => {
                let tw = c.text_width(label, Font::BodyBold);
                let x = r.x + (r.w - tw - 24.0) / 2.0;
                c.icon(icn, rect(x, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), ink);
                c.text(label, rect(x + 24.0, r.y, tw + 2.0, r.h), Font::BodyBold, ink, Align::Left);
            }
            None => c.text(label, r, Font::BodyBold, ink, Align::Center),
        }
        if enabled {
            self.hits.add(r, hit);
        }
    }

    pub fn pill_w(&self, c: &mut Canvas, label: &str, icon: bool) -> f32 {
        c.text_width(label, Font::BodyBold) + if icon { 56.0 } else { 36.0 }
    }

    /// An iOS-style switch, on = accent.
    pub fn switch(&mut self, c: &mut Canvas, r: Rect, on: bool, hit: Hit) {
        let p = self.pal.clone();
        c.fill_round(r, r.h / 2.0, if on { p.accent } else { p.fill2 });
        let k = r.h - 4.0;
        let x = if on { r.right() - 2.0 - k } else { r.x + 2.0 };
        c.fill_round(rect(x, r.y + 2.0, k, k), k / 2.0, Color::hex(0xffffff));
        self.hits.add(r.inset(-6.0), hit);
    }

    /// A tick box (rounded square), ticked = accent with a white tick.
    pub fn tickbox(&mut self, c: &mut Canvas, r: Rect, on: bool, enabled: bool, hit: Hit) {
        let p = self.pal.clone();
        if on && enabled {
            c.fill_round(r, 6.0, p.accent);
            c.icon_stroke(&ic::TICK, r.inset(3.0), p.on_accent, 2.4);
        } else {
            c.fill_round(r, 6.0, if enabled && self.hovered(&hit) { p.fill2 } else { p.fill });
            c.stroke_round(r, 6.0, if enabled { p.t3 } else { p.t4.fade(0.5) }, 1.4);
        }
        if enabled {
            self.hits.add(r.inset(-8.0), hit);
        }
    }

    /// A segmented control right-aligned in `r`: a pill track, the chosen segment raised. `cur` None: none chosen.
    pub fn segmented(&mut self, c: &mut Canvas, r: Rect, labels: &[&str], cur: Option<usize>, key: &'static str) {
        let p = self.pal.clone();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::Small) + 28.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + 8.0;
        let track = rect(r.right() - total, r.y + (r.h - 36.0) / 2.0, total, 36.0);
        c.fill_round(track, 18.0, p.fill);
        let mut x = track.x + 4.0;
        for (i, (l, w)) in labels.iter().zip(widths).enumerate() {
            let b = rect(x, track.y + 4.0, w, 28.0);
            let hit = Hit::Choice(key, i);
            let on = cur == Some(i);
            if on {
                c.fill_round(b, 14.0, p.fill2.over(p.card));
                c.stroke_round(b, 14.0, p.ring, 1.0);
            } else if self.hovered(&hit) {
                c.fill_round(b, 14.0, p.hover);
            }
            c.text(l, b, if on { Font::SmallBold } else { Font::Small }, if on { p.t1 } else { p.t2 }, Align::Center);
            self.hits.add(b, hit);
            x += w;
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// A small badge (icon + word), drawn from `x` leftwards when `right` is true; returns its width.
    pub fn badge(&mut self, c: &mut Canvas, x: f32, y: f32, icon: &Icon, label: &str, color: Color, right: bool) -> f32 {
        let p = self.pal.clone();
        let w = c.text_width(label, Font::Small) + 34.0;
        let r = rect(if right { x - w } else { x }, y, w, 24.0);
        c.fill_round(r, 12.0, color.fade(0.14).over(p.card));
        c.icon(icon, rect(r.x + 9.0, r.y + 5.0, 14.0, 14.0), color);
        c.text(label, rect(r.x + 26.0, r.y, w - 30.0, 24.0), Font::Small, color.mix(p.t1, 0.25), Align::Left);
        w
    }

    /// A slider: a track, the part up to the value in the accent, a tick at the stock value, a round knob.
    pub fn slider(&mut self, c: &mut Canvas, r: Rect, t: f32, stock: f32, hit: Hit) {
        let p = self.pal.clone();
        let ty = r.y + r.h / 2.0 - 3.0;
        c.fill_round(rect(r.x, ty, r.w, 6.0), 3.0, p.fill2);
        c.fill_round(rect(r.x, ty, r.w * t, 6.0), 3.0, p.accent);
        c.fill(rect(r.x + r.w * stock - 1.0, ty - 5.0, 2.0, 16.0), p.t3);
        let k = 18.0;
        let kx = r.x + r.w * t - k / 2.0;
        let hot = self.hovered(&hit) || matches!(hit, Hit::Slider(n) if self.drag_knob == Some(n));
        c.fill_round(rect(kx, r.y + (r.h - k) / 2.0, k, k), k / 2.0, Color::hex(0xffffff));
        c.stroke_round(rect(kx, r.y + (r.h - k) / 2.0, k, k), k / 2.0, if hot { p.accent } else { p.t3 }, if hot { 2.0 } else { 1.2 });
        self.hits.add(r, hit);
    }

    pub fn card(&self, c: &mut Canvas, r: Rect) {
        c.fill_round(r, 18.0, self.pal.card);
        c.stroke_round(r, 18.0, self.pal.line, 1.0);
    }

    pub fn group_title(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, title: &str) -> f32 {
        c.text(title, rect(x + 4.0, y, w, 24.0), Font::Title, self.pal.t1, Align::Left);
        y + 24.0 + 8.0
    }

    pub fn scrollbar(&mut self, c: &mut Canvas, area: Rect) {
        let max = (self.content_h - area.h).max(0.0);
        if max <= 0.5 {
            return;
        }
        let th = (area.h * area.h / self.content_h).max(36.0);
        let ty = area.y + (area.h - th) * (self.scroll() / max);
        let hot = self.hovered(&Hit::Scrollbar) || self.drag_scroll.is_some();
        c.fill_round(rect(area.right() - 8.0, ty, 5.0, th), 2.5, self.pal.t3.fade(if hot { 0.7 } else { 0.45 }));
        self.hits.add(rect(area.right() - 12.0, area.y, 12.0, area.h), Hit::Scrollbar);
    }

    /// The title of a page, at the top left; returns where the content starts.
    pub fn page_title(&mut self, c: &mut Canvas, page: Rect, title: &str, sub: &str) -> f32 {
        let p = self.pal.clone();
        c.text(title, rect(page.x + 24.0, m::TITLE_H, page.w - 200.0, 36.0), Font::Hero, p.t1, Align::Left);
        if !sub.is_empty() {
            c.text(sub, rect(page.x + 24.0, m::TITLE_H + 38.0, page.w - 48.0, 20.0), Font::Body, p.t3, Align::Left);
            return m::TITLE_H + 74.0;
        }
        m::TITLE_H + 52.0
    }

    /// An empty page: an icon, a title, one line, and one primary button.
    pub fn empty_state(&mut self, c: &mut Canvas, r: Rect, icon: &Icon, title: &str, line: &str, button: Option<(&str, Hit)>) {
        let p = self.pal.clone();
        let total = 44.0 + 12.0 + 24.0 + 6.0 + 20.0 + if button.is_some() { 20.0 + 36.0 } else { 0.0 };
        let mut y = r.y + ((r.h - total) / 2.0).clamp(16.0, 140.0);
        let b = rect(r.x + (r.w - 44.0) / 2.0, y, 44.0, 44.0);
        c.fill_round(b, 22.0, p.fill);
        c.icon(icon, b.centered(20.0, 20.0), p.t2);
        y += 56.0;
        c.text(title, rect(r.x, y, r.w, 24.0), Font::Title, p.t1, Align::Center);
        y += 30.0;
        c.text(line, rect(r.x + 16.0, y, r.w - 32.0, 20.0), Font::Body, p.t3, Align::Center);
        y += 40.0;
        if let Some((label, hit)) = button {
            let bw = self.pill_w(c, label, false);
            self.pill(c, rect(r.x + (r.w - bw) / 2.0, y, bw, 36.0), label, None, Some(p.accent), true, hit);
        }
    }

    pub fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        let p = self.pal.clone();
        self.hits.clear();
        c.clear(p.bg);
        let (w, h) = (cx.w, cx.h);
        let page = rect(SIDEBAR_W, 0.0, w - SIDEBAR_W, h);
        self.view_h = h;
        match self.page {
            Page::Overview => self.paint_overview(c, page),
            Page::Tweaks => self.paint_tweaks(c, page),
            Page::Cpu => self.paint_cpu(c, page),
            Page::Gpu => self.paint_gpu(c, page),
            Page::Changes => self.paint_changes(c, page),
            Page::Settings => self.paint_settings(c, page),
        }
        self.paint_sidebar(c, rect(0.0, 0.0, SIDEBAR_W, h), cx.active);
        self.paint_caption(c, w, cx);
        self.paint_modal(c, w, h);
        self.paint_toast(c, w, h);
    }

    fn paint_caption(&mut self, c: &mut Canvas, w: f32, cx: &PaintCx) {
        let p = self.pal.clone();
        let x0 = w - 3.0 * m::CAPTION_W;
        let glyphs = [(Hit::Min, '\u{E921}'), (Hit::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Hit::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(x0 + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
            let hot = self.hovered(&id) || self.pressed.as_ref() == Some(&id);
            if hot {
                c.fill(r, if id == Hit::Close { p.close_hover } else { p.hover });
            }
            let ink = match (id == Hit::Close && hot, cx.active) {
                (true, _) => Color::hex(0xffffff),
                (false, true) => p.t1,
                (false, false) => p.t3,
            };
            c.glyph(glyph, r, ink);
            self.hits.add(r, id);
        }
    }

    fn paint_sidebar(&mut self, c: &mut Canvas, r: Rect, active: bool) {
        let p = self.pal.clone();
        c.fill(r, p.side);
        c.vline(r.right(), r.y, r.h, p.line);
        let logo = rect(r.x + 14.0, (m::TITLE_H - 18.0) / 2.0, 18.0, 18.0);
        let wic = c.gfx.wic.clone();
        c.image(7, logo, 1.0, || draw::png(&wic, crate::LOGO));
        c.text("Plume Boost", rect(r.x + 40.0, 0.0, r.w - 52.0, m::TITLE_H), Font::BodyBold, if active { p.t1 } else { p.t3 }, Align::Left);
        let mut y = m::TITLE_H + 16.0;
        let changes = self.changed().len();
        let navs: [(&Icon, &str, Page); 5] = [
            (&ic::GAUGE, tr("Overview"), Page::Overview),
            (&ic::SLIDERS, tr("Tweaks"), Page::Tweaks),
            (&ic::CHIP, tf("Processor", "Processeur"), Page::Cpu),
            (&ic::GPU, tf("Graphics card", "Carte graphique"), Page::Gpu),
            (&ic::HISTORY, tr("Changes"), Page::Changes),
        ];
        for (icon, label, pg) in navs {
            let count = if pg == Page::Changes && changes > 0 { Some(changes) } else { None };
            self.nav_row(c, rect(r.x + 12.0, y, r.w - 24.0, 40.0), icon, label, pg, count);
            y += 44.0;
        }
        // the numbers that matter most, always in sight
        y += 12.0;
        c.hline(r.x + 24.0, y, r.w - 48.0, p.line);
        y += 16.0;
        let l = self.live.clone();
        let rows: [(&str, String); 3] = [
            (tr("Power plan"), if l.plan.is_empty() { "…".into() } else { l.plan.clone() }),
            (tr("Processor"), l.cpu_mhz.map(ghz).unwrap_or_else(|| "…".into())),
            (tr("Timer"), l.timer.map(|t| ms(t.0)).unwrap_or_else(|| "…".into())),
        ];
        for (k, v) in rows {
            c.text(k, rect(r.x + 24.0, y, r.w - 48.0, 18.0), Font::Small, p.t3, Align::Left);
            c.text(&v, rect(r.x + 24.0, y + 18.0, r.w - 48.0, 20.0), Font::BodyBold, p.t1, Align::Left);
            y += 46.0;
        }
        // where Plume Boost stands: administrator or not, test mode
        let mut by = r.bottom() - 56.0 - 12.0;
        if self.be.is_test() {
            by -= 30.0;
            let w = c.text_width(tr("Test mode: imaginary PC"), Font::Small) + 36.0;
            let b = rect(r.x + 24.0, by, w.min(r.w - 48.0), 26.0);
            c.fill_round(b, 13.0, p.orange.fade(0.16));
            c.icon(&ic::FLASK, rect(b.x + 9.0, b.y + 5.0, 16.0, 16.0), p.orange);
            c.text(tr("Test mode: imaginary PC"), rect(b.x + 30.0, b.y, b.w - 34.0, b.h), Font::Small, p.t1, Align::Left);
        }
        by -= 30.0;
        let (label, col) = if self.admin { (tr("Administrator"), p.accent) } else { (tr("Standard rights"), p.t3) };
        let w = c.text_width(label, Font::Small) + 36.0;
        let b = rect(r.x + 24.0, by, w, 26.0);
        c.fill_round(b, 13.0, col.fade(0.16));
        c.icon(&ic::SHIELD, rect(b.x + 9.0, b.y + 5.0, 16.0, 16.0), col);
        c.text(label, rect(b.x + 30.0, b.y, b.w - 34.0, b.h), Font::Small, p.t1, Align::Left);
        self.nav_row(c, rect(r.x + 12.0, r.bottom() - 56.0, r.w - 24.0, 40.0), &icons::GEAR, tr("Settings"), Page::Settings, None);
    }

    fn nav_row(&mut self, c: &mut Canvas, row: Rect, icon: &Icon, label: &str, pg: Page, count: Option<usize>) {
        let p = self.pal.clone();
        let active = self.page == pg;
        let hit = Hit::Nav(pg);
        if active {
            c.fill_round(row, 10.0, p.accent.fade(0.16));
        } else if self.hovered(&hit) {
            c.fill_round(row, 10.0, p.hover);
        }
        let ink = if active { p.accent.mix(p.t1, if p.on_accent == Color::hex(0xffffff) { 0.0 } else { 0.15 }) } else { p.t1 };
        c.icon(icon, rect(row.x + 12.0, row.y + 11.0, 18.0, 18.0), if active { p.accent } else { p.t2 });
        c.text(
            label,
            rect(row.x + 40.0, row.y, row.w - if count.is_some() { 80.0 } else { 48.0 }, row.h),
            if active { Font::BodyBold } else { Font::Body },
            ink,
            Align::Left,
        );
        if let Some(n) = count {
            let s = n.to_string();
            let w = c.text_width(&s, Font::SmallBold) + 14.0;
            let b = rect(row.right() - 10.0 - w, row.y + 10.0, w, 20.0);
            c.fill_round(b, 10.0, p.fill2);
            c.text(&s, b, Font::SmallBold, p.t1, Align::Center);
        }
        self.hits.add(row, hit);
    }

    fn paint_modal(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let Some(modal) = self.modal.clone() else { return };
        let p = self.pal.clone();
        c.fill(rect(0.0, 0.0, w, h), Color::hex(0).fade(0.5));
        self.hits.add(rect(0.0, 0.0, w, h), Hit::Inert);
        let mut lines: Vec<Line> = Vec::new();
        // buttons between OK and Cancel
        let mut mid: Vec<(String, Hit)> = Vec::new();
        let (icon, icol, title, ok, ok_tone, cancel): (&Icon, Color, String, String, Color, Option<&str>) = match &modal {
            Modal::BackupMissing { .. } => {
                lines.push(Line::Text(
                    tf(
                        "Plume Boost keeps a copy of your settings with Plume Backup: before each change it saves them, and Plume Backup keeps dated copies elsewhere on your disk.",
                        "Plume Boost garde une copie de vos réglages avec Plume Backup : avant chaque changement il les enregistre, et Plume Backup en garde des copies datées ailleurs sur le disque.",
                    )
                    .into(),
                    p.t2,
                ));
                lines.push(Line::Text(
                    tf(
                        "Plume Backup isn't installed. Install it now (from the Plume Suite installer next to Plume Boost), or go on: the snapshot is still saved on this PC.",
                        "Plume Backup n'est pas installé. Installez-le maintenant (avec l'installateur de Plume Suite à côté de Plume Boost), ou continuez : l'instantané est quand même enregistré sur ce PC.",
                    )
                    .into(),
                    p.t3,
                ));
                mid.push((tf("Apply without Plume Backup", "Appliquer sans Plume Backup").into(), Hit::BackupWithout));
                (
                    &ic::SHIELD,
                    p.accent,
                    tf("Keep a copy with Plume Backup?", "Garder une copie avec Plume Backup ?").into(),
                    tf("Install Plume Backup", "Installer Plume Backup").into(),
                    p.accent,
                    Some(tr("Cancel")),
                )
            }
            Modal::Keep => {
                let secs = self.keep_secs_left().unwrap_or(0);
                lines.push(Line::Text(
                    tf(
                        "The graphics card uses the new settings now. If the screen or a game misbehaves, do nothing: they go back by themselves.",
                        "La carte graphique utilise les nouveaux réglages. Si l'écran ou un jeu se comporte mal, ne faites rien : ils reviennent d'eux-mêmes.",
                    )
                    .into(),
                    p.t2,
                ));
                if let Some(r) = self.trying.as_ref().and_then(|t| t.result.clone()) {
                    lines.push(Line::Bullet(stress_line(&r), if r.throttled() { p.orange } else { p.green }));
                }
                lines.push(Line::Progress(
                    secs as f32 / super::flows::KEEP_SECS as f32,
                    fill(tf("Going back in {} s", "Retour dans {} s"), &[&secs.to_string()]),
                ));
                mid.push((tf("Stability test", "Test de stabilité").into(), Hit::GpuTest));
                (
                    &ic::GPU,
                    p.accent,
                    tf("Keep these settings?", "Garder ces réglages ?").into(),
                    tf("Keep", "Garder").into(),
                    p.accent,
                    Some(tf("Revert", "Revenir")),
                )
            }
            Modal::Stress => {
                let (done, total, tel) = self.stress.as_ref().map(|s| (s.done, s.total, s.tel)).unwrap_or((0.0, 60.0, None));
                lines.push(Line::Text(
                    tf(
                        "A compute load on the graphics card, checked against the processor's own answers. Wrong results or a driver reset mean the settings are not stable.",
                        "Une charge de calcul sur la carte graphique, vérifiée par les réponses du processeur. Des résultats faux ou une réinitialisation du pilote signifient que les réglages ne sont pas stables.",
                    )
                    .into(),
                    p.t2,
                ));
                lines.push(Line::Progress(
                    (done / total.max(1.0)) as f32,
                    fill(tf("{} s of {} s", "{} s sur {} s"), &[&format!("{done:.0}"), &format!("{total:.0}")]),
                ));
                if let Some(t) = tel {
                    let v = |x: Option<f64>, u: &str| x.map(|x| format!("{x:.0} {u}")).unwrap_or("—".into());
                    lines.push(Line::Text(format!("{} · {} · {} · {}", v(t.core_mhz, "MHz"), v(t.temp_c, "°C"), v(t.power_w, "W"), v(t.usage_pct, "%")), p.t1));
                }
                (&ic::FLASK, p.accent, tf("Stability test", "Test de stabilité").into(), tf("Stop", "Arrêter").into(), p.accent, None)
            }
            Modal::StressDone { result, reverted } => {
                lines.push(Line::Bullet(stress_line(result), if result.passed() && !result.throttled() { p.green } else { p.orange }));
                if *reverted {
                    lines.push(Line::Text(tf("The card is back on its previous settings.", "La carte est revenue à ses réglages précédents.").into(), p.t2));
                }
                let ok = result.passed();
                (
                    if ok { &ic::OK } else { &ic::WARN },
                    if ok { p.green } else { p.orange },
                    if ok { tf("Stable for the whole test", "Stable pendant tout le test") } else { tf("Not stable", "Pas stable") }.into(),
                    tr("OK").into(),
                    p.accent,
                    None,
                )
            }
            Modal::Restore { lines: ls, admin, .. } => {
                if ls.is_empty() {
                    lines.push(Line::Text(
                        tf("Nothing differs: the PC is already as in this snapshot.", "Rien ne diffère : le PC est déjà comme dans cet instantané.").into(),
                        p.t2,
                    ));
                } else {
                    lines.push(Line::Text(
                        fill(
                            tf("{} settings go back to their value in this snapshot:", "{} réglages reprennent leur valeur de cet instantané :"),
                            &[&ls.len().to_string()],
                        ),
                        p.t2,
                    ));
                    for l in ls.iter().take(10) {
                        lines.push(Line::Bullet(l.clone(), p.t1));
                    }
                    if ls.len() > 10 {
                        lines.push(Line::Text(fill(tf("…and {} more.", "…et {} de plus."), &[&(ls.len() - 10).to_string()]), p.t3));
                    }
                }
                lines.push(Line::Text(
                    tf("A snapshot of the settings as they are now is taken first.", "Un instantané des réglages actuels est pris d'abord.").into(),
                    p.t3,
                ));
                if *admin {
                    lines.push(Line::Text(tr("Needs administrator rights: Plume Boost will ask Windows first.").into(), p.t3));
                }
                (
                    &ic::HISTORY,
                    p.accent,
                    tf("Restore this snapshot?", "Restaurer cet instantané ?").into(),
                    tf("Restore", "Restaurer").into(),
                    p.accent,
                    Some(tr("Cancel")),
                )
            }
            Modal::GpuReverted { reason } => {
                lines.push(Line::Text(reason.clone(), p.t1));
                lines.push(Line::Text(
                    tf(
                        "Plume Boost put the graphics card back on the settings it had before, and paused applying a profile at sign-in. Change the settings on the Graphics card page, then choose a profile for sign-in again.",
                        "Plume Boost a remis la carte graphique sur ses réglages précédents et suspendu l'application d'un profil à la connexion. Modifiez les réglages dans la page Carte graphique, puis rechoisissez un profil pour la connexion.",
                    )
                    .into(),
                    p.t3,
                ));
                (&ic::WARN, p.orange, tf("Graphics settings put back", "Réglages graphiques remis").into(), tr("OK").into(), p.accent, None)
            }
            Modal::Waiver => {
                lines.push(Line::Text(
                    tf(
                        "Intel asks you to accept this before its driver lets an app change clocks, voltage or power: overclocking can shorten the card's life, may void its warranty, and Intel isn't responsible for damage it causes.",
                        "Intel demande d'accepter ceci avant que son pilote laisse une appli changer fréquences, tension ou puissance : l'overclocking peut réduire la durée de vie de la carte, peut annuler sa garantie, et Intel n'est pas responsable des dommages causés.",
                    )
                    .into(),
                    p.t1,
                ));
                lines.push(Line::Text(
                    tf(
                        "Plume Boost still keeps every value inside the card's limits, tries before keeping, and can undo everything.",
                        "Plume Boost garde toutes les valeurs dans les limites de la carte, essaie avant de garder, et peut tout annuler.",
                    )
                    .into(),
                    p.t3,
                ));
                (
                    &ic::WARN,
                    p.orange,
                    tf("Intel's overclocking waiver", "Décharge d'overclocking d'Intel").into(),
                    tf("I accept", "J'accepte").into(),
                    p.accent,
                    Some(tr("Cancel")),
                )
            }
            Modal::Confirm { ids, restore } => {
                for id in ids {
                    if let Some(t) = by_id(id) {
                        let tail = match t.after {
                            After::Restart => format!(" — {}", tr("after a restart")),
                            After::SignOut => format!(" — {}", tr("after signing out")),
                            After::Game => format!(" — {}", tr("when games start again")),
                            After::Now => String::new(),
                        };
                        lines.push(Line::Bullet(format!("{}{tail}", t.title.get()), p.t1));
                    }
                }
                lines.push(Line::Text(tr("The value each setting has now is saved first: Changes can put it back at any time.").into(), p.t3));
                let sub = if self.admin {
                    tr("Windows keeps it in System › System protection. Takes up to a minute.")
                } else {
                    tr("Needs administrator rights: Plume Boost will ask Windows first.")
                };
                lines.push(Line::Switch(tr("Create a System Restore point first").into(), sub.into(), *restore));
                let n = ids.len();
                (
                    &ic::BOLT,
                    p.accent,
                    if n == 1 { tr("Apply 1 change?").to_string() } else { fill(tr("Apply {} changes?"), &[&n.to_string()]) },
                    tr("Apply").into(),
                    p.accent,
                    Some(tr("Cancel")),
                )
            }
            Modal::Elevate(why) => {
                match why {
                    Why::Apply(ids) => {
                        lines.push(Line::Text(tr("These change the whole PC, so Windows asks for administrator rights:").into(), p.t2));
                        for id in ids {
                            if let Some(t) = by_id(id) {
                                lines.push(Line::Bullet(t.title.get().into(), p.t1));
                            }
                        }
                        lines.push(Line::Text(tr("Plume Boost opens again as administrator with the same ticks. Press Apply there.").into(), p.t3));
                    }
                    Why::RestorePoint => {
                        lines.push(Line::Text(tr("Creating a System Restore point needs administrator rights.").into(), p.t2));
                        lines.push(Line::Text(
                            tr("Restart Plume Boost as administrator, or turn off the restore point switch: every change can be undone from Changes anyway.")
                                .into(),
                            p.t3,
                        ));
                    }
                    Why::Undo => {
                        lines.push(Line::Text(
                            tr("Some of the settings to put back belong to the whole PC, so Windows asks for administrator rights.").into(),
                            p.t2,
                        ));
                        lines.push(Line::Text(tr("Plume Boost opens again as administrator. Undo from there.").into(), p.t3));
                    }
                    Why::Power => {
                        lines.push(Line::Text(tr("Windows refused to change this power setting without administrator rights.").into(), p.t2));
                    }
                }
                (&ic::SHIELD, p.accent, tr("Administrator rights needed").into(), tr("Restart as administrator").into(), p.accent, Some(tr("Cancel")))
            }
            Modal::RestoreFailed { reason, .. } => {
                lines.push(Line::Text(reason.clone(), p.t1));
                lines.push(Line::Text(tr("Windows also skips a new point when one was made in the last 24 hours. Apply without one? Every change can still be undone from Changes.").into(), p.t3));
                (&ic::WARN, p.orange, tr("No restore point was created").into(), tr("Apply anyway").into(), p.accent, Some(tr("Cancel")))
            }
            Modal::UndoAll => {
                let n = self.log.entries.len();
                lines.push(Line::Text(
                    fill(tr("Plume Boost puts back the {} settings it changed, newest first, to the values they had before."), &[&n.to_string()]),
                    p.t2,
                ));
                for id in self.changed().iter().take(8) {
                    lines.push(Line::Bullet(changed_title(id), p.t1));
                }
                lines.push(Line::Text(tr("Some may need a restart or a sign-out to be fully back.").into(), p.t3));
                (&ic::HISTORY, p.accent, tr("Undo everything?").into(), tr("Undo everything").into(), p.accent, Some(tr("Cancel")))
            }
            Modal::Done { applied, already, failed, after } => {
                for id in applied {
                    if let Some(t) = by_id(id) {
                        lines.push(Line::Bullet(t.title.get().into(), p.t1));
                    }
                }
                if *already > 0 {
                    lines.push(Line::Text(fill(tr("{} ticked were already on."), &[&already.to_string()]), p.t3));
                }
                for (t, why) in failed {
                    lines.push(Line::Bullet(format!("{t}: {why}"), p.red));
                }
                let next = match after {
                    Some(After::Restart) => tr("Restart Windows to finish: some changes are used only after a restart."),
                    Some(After::SignOut) => tr("Sign out and back in to finish."),
                    Some(After::Game) => tr("Start your games again to use the new settings."),
                    _ => tr("Everything is in use already."),
                };
                lines.push(Line::Text(next.into(), p.t2));
                lines.push(Line::Text(tr("Overview shows the numbers before and after. Changes lists every change, ready to undo.").into(), p.t3));
                let title = if applied.is_empty() && !failed.is_empty() {
                    tr("Nothing was changed").to_string()
                } else if applied.len() == 1 {
                    tr("1 change applied").to_string()
                } else {
                    fill(tr("{} changes applied"), &[&applied.len().to_string()])
                };
                (
                    if failed.is_empty() { &ic::OK } else { &ic::WARN },
                    if failed.is_empty() { p.green } else { p.orange },
                    title,
                    tr("OK").into(),
                    p.accent,
                    None,
                )
            }
        };
        // measure
        let mw = (w - 80.0).min(560.0);
        let x = (w - mw) / 2.0;
        let inner = mw - 48.0;
        let heights: Vec<f32> = lines
            .iter()
            .map(|l| match l {
                Line::Text(s, _) => c.text_height(s, inner, Font::Body) + 10.0,
                Line::Bullet(s, _) => c.text_height(s, inner - 18.0, Font::Body) + 6.0,
                Line::Switch(..) => 64.0,
                Line::Progress(..) => 44.0,
            })
            .collect();
        let body: f32 = heights.iter().sum();
        let mh = (28.0 + 30.0 + 14.0 + body + 18.0 + 36.0 + 24.0).min(h - 40.0);
        let y = ((h - mh) / 2.0).max(20.0);
        let b = rect(x, y, mw, mh);
        c.fill_round(b.offset(0.0, 8.0).inset(-2.0), 22.0, Color::hex(0).fade(0.14));
        c.fill_round(b, 20.0, p.solid);
        c.stroke_round(b, 20.0, p.ring, 1.0);
        self.hits.add(b, Hit::Inert);
        c.icon(icon, rect(x + 24.0, y + 33.0, 22.0, 22.0), icol);
        c.text(&title, rect(x + 56.0, y + 28.0, inner - 32.0, 32.0), Font::Large, p.t1, Align::Left);
        let mut ly = y + 72.0;
        let bottom = b.bottom() - 24.0 - 36.0 - 12.0;
        c.clip(rect(x, ly, mw, (bottom - ly).max(0.0)));
        for (l, lh) in lines.iter().zip(heights) {
            match l {
                Line::Text(s, col) => c.text_block(s, rect(x + 24.0, ly, inner, lh), Font::Body, *col),
                Line::Bullet(s, col) => {
                    c.fill_round(rect(x + 28.0, ly + 7.0, 5.0, 5.0), 2.5, *col);
                    c.text_block(s, rect(x + 42.0, ly, inner - 18.0, lh), Font::Body, *col);
                }
                Line::Switch(label, sub, on) => {
                    let r = rect(x + 24.0, ly + 6.0, inner, 52.0);
                    c.fill_round(r, 14.0, p.fill.over(p.solid));
                    c.text(label, rect(r.x + 16.0, r.y + 7.0, r.w - 90.0, 20.0), Font::Body, p.t1, Align::Left);
                    c.text(sub, rect(r.x + 16.0, r.y + 26.0, r.w - 90.0, 18.0), Font::Small, p.t3, Align::Left);
                    self.switch(c, rect(r.right() - 16.0 - 44.0, r.y + 13.0, 44.0, 26.0), *on, Hit::ModalSwitch);
                }
                Line::Progress(k, caption) => {
                    let bar = rect(x + 24.0, ly + 8.0, inner, 8.0);
                    c.fill_round(bar, 4.0, p.fill2);
                    c.fill_round(rect(bar.x, bar.y, bar.w * k.clamp(0.0, 1.0), 8.0), 4.0, p.accent);
                    c.text(caption, rect(x + 24.0, ly + 20.0, inner, 20.0), Font::Small, p.t3, Align::Left);
                }
            }
            ly += lh;
        }
        c.unclip();
        let by = b.bottom() - 24.0 - 36.0;
        let ow = self.pill_w(c, &ok, false);
        self.pill(c, rect(b.right() - 24.0 - ow, by, ow, 36.0), &ok, None, Some(ok_tone), true, Hit::ModalOk);
        let mut bx = b.right() - 24.0 - ow - 10.0;
        for (label, hit) in mid {
            let mw = self.pill_w(c, &label, false);
            bx -= mw;
            self.pill(c, rect(bx, by, mw, 36.0), &label, None, None, true, hit);
            bx -= 10.0;
        }
        if let Some(cl) = cancel {
            let cw = self.pill_w(c, cl, false);
            self.pill(c, rect(bx - cw, by, cw, 36.0), cl, None, None, true, Hit::ModalCancel);
        }
    }

    fn paint_toast(&mut self, c: &mut Canvas, w: f32, h: f32) {
        let p = self.pal.clone();
        let (t, busy) = match (&self.busy, &self.toast) {
            (Some(b), _) => (tr(b).to_string(), true),
            (None, Some((t, _))) => (t.clone(), false),
            _ => return,
        };
        let tw = (c.text_width(&t, Font::Body) + if busy { 64.0 } else { 40.0 }).min(w - SIDEBAR_W - 48.0);
        let x = SIDEBAR_W + (w - SIDEBAR_W - tw) / 2.0;
        let r = rect(x, h - 64.0, tw, 40.0);
        c.fill_round(r.offset(0.0, 4.0), 20.0, Color::hex(0).fade(0.18));
        c.fill_round(r, 20.0, p.solid2);
        c.stroke_round(r, 20.0, p.ring, 1.0);
        if busy {
            c.icon(&ic::BOLT, rect(r.x + 18.0, r.y + 12.0, 16.0, 16.0), p.accent);
            c.text(&t, rect(r.x + 42.0, r.y, r.w - 58.0, r.h), Font::Body, p.t1, Align::Left);
        } else {
            c.text(&t, r.inset_xy(16.0, 0.0), Font::Body, p.t1, Align::Center);
            self.hits.add(r, Hit::Toast);
        }
    }
}

/// One line about a stability test.
pub fn stress_line(r: &crate::gpu::StressResult) -> String {
    let v = |x: Option<f64>, u: &str| x.map(|x| format!("{x:.0} {u}")).unwrap_or("—".into());
    if let Some(e) = &r.failed_to_start {
        return fill(tf("The test couldn't start: {}", "Le test n'a pas pu démarrer : {}"), &[e]);
    }
    if r.device_removed {
        return fill(
            tf("The driver stopped responding after {} s (reset by Windows).", "Le pilote a cessé de répondre après {} s (réinitialisé par Windows)."),
            &[&format!("{:.0}", r.secs)],
        );
    }
    if r.errors > 0 {
        return fill(
            tf("{} wrong results in {} s: not stable.", "{} résultats faux en {} s : pas stable."),
            &[&r.errors.to_string(), &format!("{:.0}", r.secs)],
        );
    }
    if r.stopped {
        return fill(tf("Stopped after {} s, no error so far.", "Arrêté après {} s, aucune erreur jusque-là."), &[&format!("{:.0}", r.secs)]);
    }
    let mut s = fill(
        tf("{} s without error · up to {} · {} · {}", "{} s sans erreur · jusqu'à {} · {} · {}"),
        &[&format!("{:.0}", r.secs), &v(r.max_clock, "MHz"), &v(r.max_temp, "°C"), &v(r.max_power, "W")],
    );
    if r.throttled() {
        s.push_str(tf(" · the clock dropped under load (heat or power limit)", " · la fréquence a baissé en charge (chaleur ou limite de puissance)"));
    }
    s
}

pub fn ghz(mhz: f32) -> String {
    format!("{:.2} GHz", mhz / 1000.0)
}

pub fn ms(v: f32) -> String {
    if v < 1.0 { format!("{v:.2} ms") } else { format!("{v:.1} ms") }
}

pub fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1_073_741_824.0)
}

/// "just now", "5 min ago"…
pub fn ago(secs: u64) -> String {
    let d = crate::engine::now().saturating_sub(secs);
    if d < 60 {
        tr("just now").into()
    } else if d < 3600 {
        fill(tr("{} min ago"), &[&(d / 60).to_string()])
    } else if d < 86_400 {
        fill(tr("{} h ago"), &[&(d / 3600).to_string()])
    } else {
        fill(tr("{} days ago"), &[&(d / 86_400).to_string()])
    }
}
