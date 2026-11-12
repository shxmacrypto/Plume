//! Graphics card: the card's readings, then — when its driver allows it — clocks, voltage, power and fans within
//! the ranges the driver reports. Nothing is applied while the controls move: "Try" applies, then a 15-second
//! "Keep these settings?" question goes back by itself unless the user keeps them. Profiles, apply at sign-in, the
//! stability test, the monitor window, and what Plume Boost doesn't do (no frame counter in games).

use super::{App, Hit, draw::ic};
use crate::{
    gpu::{Card, Fan, Knob, show},
    i18n::{fill, tf},
};
use plume_ui::{Align, Canvas, Font, Rect, rect};

const ROW: f32 = 74.0;

fn knob_text(k: Knob, percent: bool) -> (&'static str, &'static str) {
    match k {
        Knob::CoreOffset => (
            tf("Core clock offset", "Décalage de fréquence du cœur"),
            tf(
                "Added to every graphics clock the card uses. Faster until it is unstable: go up in small steps and test.",
                "Ajouté à chaque fréquence graphique de la carte. Plus rapide jusqu'à l'instabilité : montez par petits pas et testez.",
            ),
        ),
        Knob::MemOffset => (
            tf("Memory clock offset", "Décalage de fréquence de la mémoire"),
            tf(
                "Faster video memory helps at high resolutions. Too far gives artefacts, or a silent slowdown on cards that correct errors.",
                "Une mémoire vidéo plus rapide aide en haute résolution. Trop loin : artefacts, ou ralentissement discret sur les cartes qui corrigent les erreurs.",
            ),
        ),
        Knob::CoreMax => (
            tf("Maximum graphics clock", "Fréquence graphique maximale"),
            tf("The highest clock the card may boost to.", "La fréquence la plus haute que la carte peut atteindre."),
        ),
        Knob::MemMax => (tf("Memory clock", "Fréquence de la mémoire"), tf("The video memory's clock.", "La fréquence de la mémoire vidéo.")),
        Knob::Voltage => (
            tf("Graphics voltage", "Tension graphique"),
            tf(
                "Lower is an undervolt: cooler and quieter, often just as fast. Too low crashes under load: test after each step.",
                "Plus bas, c'est un undervolt : plus frais et plus silencieux, souvent aussi rapide. Trop bas, ça plante en charge : testez à chaque pas.",
            ),
        ),
        Knob::VoltOffset => (
            tf("Voltage offset", "Décalage de tension"),
            tf("More voltage lets the clock go higher, with more heat.", "Plus de tension laisse monter la fréquence, avec plus de chaleur."),
        ),
        Knob::Power if percent => (
            tf("Power limit", "Limite de puissance"),
            tf(
                "Around the card's stock power. More lets it hold higher clocks; less is cooler and quieter.",
                "Autour de la puissance d'origine. Plus lui laisse tenir des fréquences plus hautes ; moins est plus frais et silencieux.",
            ),
        ),
        Knob::Power => (
            tf("Power limit", "Limite de puissance"),
            tf(
                "How much power the card may draw. More lets it hold higher clocks; less is cooler and quieter.",
                "La puissance que la carte peut tirer. Plus lui laisse tenir des fréquences plus hautes ; moins est plus frais et silencieux.",
            ),
        ),
        Knob::TempLimit => (
            tf("Temperature limit", "Limite de température"),
            tf("The card slows down to stay under this temperature.", "La carte ralentit pour rester sous cette température."),
        ),
    }
}

fn why_not_text(c: &Card) -> &'static str {
    match c.why_not.as_deref() {
        Some("integrated") => tf(
            "Integrated graphics share the processor's power and cooling: their clocks follow the power plan and the processor settings (Processor page). The maker's driver gives readings for them, not clock or power controls, so Plume Boost shows the readings.",
            "Une puce graphique intégrée partage l'alimentation et le refroidissement du processeur : ses fréquences suivent le mode d'alimentation et les réglages du processeur (page Processeur). Le pilote du fabricant donne ses mesures, pas de commande de fréquence ni de puissance : Plume Boost affiche les mesures.",
        ),
        Some("no-library") => tf(
            "Windows sees this card, but its maker's driver library didn't answer (a driver from Windows Update, or too old). Install the latest driver from NVIDIA, AMD or Intel to tune it here.",
            "Windows voit cette carte, mais la bibliothèque du pilote du fabricant n'a pas répondu (pilote de Windows Update, ou trop ancien). Installez le dernier pilote de NVIDIA, AMD ou Intel pour la régler ici.",
        ),
        _ => tf(
            "The driver answered, but reports nothing that can be changed on this card (some laptop graphics are locked by their maker).",
            "Le pilote a répondu, mais n'indique rien de modifiable sur cette carte (certaines puces de portables sont verrouillées par leur fabricant).",
        ),
    }
}

impl App {
    pub fn paint_gpu(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        self.sliders.clear();
        let top = self.page_title(
            c,
            page,
            tf("Graphics card", "Carte graphique"),
            tf(
                "Clocks, power and fans through the card's own driver, within the limits it reports. Tried first, kept only if you say so.",
                "Fréquences, puissance et ventilateurs via le pilote de la carte, dans les limites qu'il donne. Essayé d'abord, gardé seulement si vous le dites.",
            ),
        );
        let area = rect(page.x, top, page.w, page.h - top);
        self.view_h = area.h;
        let scroll = self.scroll();
        c.clip(area);
        let x = page.x + 24.0;
        let w = (page.w - 48.0).min(980.0);
        let mut y = area.y - scroll;
        let gap = 14.0;

        let Some(card) = self.gpu_card().cloned() else {
            c.unclip();
            self.content_h = 0.0;
            self.empty_state(
                c,
                area,
                &ic::GPU,
                tf("No graphics card found", "Aucune carte graphique trouvée"),
                tf("Windows lists no graphics card Plume Boost can read.", "Windows n'indique aucune carte que Plume Boost peut lire."),
                None,
            );
            return;
        };

        // which card, when there are several
        if self.cards.len() > 1 {
            let names: Vec<String> = self.cards.iter().map(|c| c.name.replace(" (imaginary)", "")).collect();
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let sel = self.cards.iter().position(|c| c.id == card.id);
            let r = rect(x, y, w, 40.0);
            self.cards_tabs(c, r, &refs, sel);
            y += 52.0;
        }

        // the card and the monitor button
        let r = rect(x, y, w, 76.0);
        self.card(c, r);
        c.icon(&ic::GPU, rect(r.x + 20.0, r.y + 20.0, 20.0, 20.0), p.accent);
        c.text(&card.name, rect(r.x + 52.0, r.y + 14.0, r.w - 300.0, 24.0), Font::Title, p.t1, Align::Left);
        let kind = if card.integrated { tf("integrated", "intégrée") } else { tf("dedicated", "dédiée") };
        c.text(
            &format!("{} · {kind} · {} {}", card.vendor.name(), tf("driver interface:", "interface du pilote :"), card.api),
            rect(r.x + 52.0, r.y + 40.0, r.w - 300.0, 20.0),
            Font::Small,
            p.t3,
            Align::Left,
        );
        let ml = if self.monitor.is_some() { tf("Close the monitor", "Fermer le moniteur") } else { tf("Open the monitor", "Ouvrir le moniteur") };
        let mw = self.pill_w(c, ml, true);
        self.pill(c, rect(r.right() - 20.0 - mw, r.y + 20.0, mw, 36.0), ml, Some(&ic::GAUGE), None, true, Hit::Monitor);
        y = r.bottom() + gap;

        // readings
        let t = if self.live.card == card.id { self.live.tel.unwrap_or_default() } else { Default::default() };
        let dash = || "—".to_string();
        let tiles: [(&str, String, String); 6] = [
            (tf("Graphics clock", "Fréquence graphique"), t.core_mhz.map(|v| format!("{v:.0} MHz")).unwrap_or_else(dash), String::new()),
            (tf("Memory clock", "Fréquence mémoire"), t.mem_mhz.map(|v| format!("{v:.0} MHz")).unwrap_or_else(dash), String::new()),
            (
                tf("Temperature", "Température"),
                t.temp_c.map(|v| format!("{v:.0} °C")).unwrap_or_else(dash),
                t.hotspot_c.map(|v| fill(tf("hotspot {} °C", "point chaud {} °C"), &[&format!("{v:.0}")])).unwrap_or_default(),
            ),
            (
                tf("Power", "Puissance"),
                t.power_w.map(|v| format!("{v:.0} W")).unwrap_or_else(dash),
                t.power_limit_w.map(|v| fill(tf("limit {} W", "limite {} W"), &[&format!("{v:.0}")])).unwrap_or_default(),
            ),
            (
                tf("Fan", "Ventilateur"),
                t.fan_pct.map(|v| format!("{v:.0} %")).or(t.fan_rpm.map(|v| format!("{v:.0} rpm"))).unwrap_or_else(dash),
                t.fan_rpm.filter(|_| t.fan_pct.is_some()).map(|v| format!("{v:.0} rpm")).unwrap_or_default(),
            ),
            (tf("Load", "Charge"), t.usage_pct.map(|v| format!("{v:.0} %")).unwrap_or_else(dash), String::new()),
        ];
        let cols = if w > 760.0 { 6 } else { 3 };
        let tw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        for (i, (k, v, sub)) in tiles.iter().enumerate() {
            let r = rect(x + (i % cols) as f32 * (tw + gap), y + (i / cols) as f32 * (88.0 + gap), tw, 88.0);
            self.card(c, r);
            c.text(k, rect(r.x + 16.0, r.y + 12.0, r.w - 28.0, 18.0), Font::Small, p.t3, Align::Left);
            c.text(v, rect(r.x + 16.0, r.y + 32.0, r.w - 28.0, 28.0), Font::Large, p.t1, Align::Left);
            c.text(sub, rect(r.x + 16.0, r.y + 62.0, r.w - 28.0, 18.0), Font::Small, p.t3, Align::Left);
        }
        y += (6 / cols) as f32 * (88.0 + gap) + 6.0;

        if !card.caps.tunable() {
            let text = why_not_text(&card);
            let h = c.text_height(text, w - 88.0, Font::Body) + 40.0;
            let r = rect(x, y, w, h);
            self.card(c, r);
            c.icon(&ic::CHIP, rect(r.x + 22.0, r.y + 20.0, 20.0, 20.0), p.t3);
            c.text_block(text, rect(r.x + 56.0, r.y + 20.0, w - 88.0, h - 30.0), Font::Body, p.t2);
            y = r.bottom() + gap + 10.0;
        } else {
            y = self.paint_tuning(c, &card, x, y, w);
        }

        // how it stays safe, and what isn't done
        y = self.group_title(c, x, y, w, tf("Safety, and what Plume Boost doesn't do", "Sécurité, et ce que Plume Boost ne fait pas"));
        let paras: [(&str, &str); 4] = [
            (
                tf("Inside the driver's limits", "Dans les limites du pilote"),
                tf(
                    "Every value stays within the range the card's driver reports. Each Try takes a snapshot first (kept with Plume Backup), and Changes can undo it.",
                    "Chaque valeur reste dans la plage donnée par le pilote. Chaque essai prend d'abord un instantané (copié par Plume Backup), et Changements peut l'annuler.",
                ),
            ),
            (
                tf("Kept only if you say so", "Gardé seulement si vous le dites"),
                tf(
                    "After Try, the settings go back by themselves after 15 seconds unless you press Keep, like Windows' display settings. For 10 minutes after a change, a driver reset or a restart without a clean shutdown puts the previous settings back at the next start, and pauses \"apply at sign-in\".",
                    "Après Essayer, les réglages reviennent d'eux-mêmes au bout de 15 secondes sauf si vous appuyez sur Garder, comme les paramètres d'affichage de Windows. Pendant 10 minutes après un changement, une réinitialisation du pilote ou un redémarrage sans arrêt correct remet les réglages précédents au démarrage suivant, et suspend « appliquer à la connexion ».",
                ),
            ),
            (
                tf("No frame counter inside games", "Pas de compteur d'images dans les jeux"),
                tf(
                    "An on-screen FPS counter works by loading code into each game. Anti-cheat systems can flag that, and it can crash games. Plume Boost injects nothing: the monitor is its own window. Use the game's own counter, or the Game Bar's performance widget (Win+G).",
                    "Un compteur d'images à l'écran fonctionne en chargeant du code dans chaque jeu. Les anti-triche peuvent le signaler, et cela peut faire planter des jeux. Plume Boost n'injecte rien : le moniteur est sa propre fenêtre. Utilisez le compteur du jeu, ou le widget Performances de la Game Bar (Win+G).",
                ),
            ),
            (
                tf("No fan curve on NVIDIA cards", "Pas de courbe de ventilateur sur les cartes NVIDIA"),
                tf(
                    "NVIDIA's driver keeps no curve of its own: a curve would need a program running all the time. Plume Boost offers a fixed speed or automatic there; AMD and Intel Arc keep the curve in their driver.",
                    "Le pilote NVIDIA ne garde pas de courbe : il faudrait un programme qui tourne en permanence. Plume Boost y propose une vitesse fixe ou l'automatique ; AMD et Intel Arc gardent la courbe dans leur pilote.",
                ),
            ),
        ];
        y = self.paras(c, x, y, w, &paras) + gap + 10.0;
        c.unclip();
        self.content_h = y + scroll - area.y;
        self.scrollbar(c, area);
    }

    /// A row of card names (one chosen).
    fn cards_tabs(&mut self, c: &mut Canvas, r: Rect, names: &[&str], sel: Option<usize>) {
        let p = self.pal.clone();
        let mut bx = r.x;
        for (i, n) in names.iter().enumerate() {
            let bw = c.text_width(n, Font::Body) + 36.0;
            let b = rect(bx, r.y, bw, 36.0);
            let on = sel == Some(i);
            let hit = Hit::Card(i);
            c.fill_round(
                b,
                18.0,
                if on {
                    p.accent.fade(0.16)
                } else if self.hovered(&hit) {
                    p.hover
                } else {
                    p.fill
                },
            );
            if on {
                c.stroke_round(b, 18.0, p.accent, 1.4);
            }
            c.text(n, b, if on { Font::BodyBold } else { Font::Body }, p.t1, Align::Center);
            self.hits.add(b, hit);
            bx += bw + 8.0;
        }
    }

    /// Titled paragraphs in a card; returns the bottom.
    pub fn paras(&mut self, c: &mut Canvas, x: f32, y: f32, w: f32, paras: &[(&str, &str)]) -> f32 {
        let p = self.pal.clone();
        let inner = w - 48.0;
        let hs: Vec<f32> = paras.iter().map(|(_, b)| 22.0 + c.text_height(b, inner, Font::Body) + 14.0).collect();
        let card = rect(x, y, w, hs.iter().sum::<f32>() + 28.0);
        self.card(c, card);
        let mut py = card.y + 18.0;
        for ((t, b), h) in paras.iter().zip(hs) {
            c.text(t, rect(card.x + 24.0, py, inner, 20.0), Font::BodyBold, p.t1, Align::Left);
            let bh = c.text_height(b, inner, Font::Body);
            c.text_block(b, rect(card.x + 24.0, py + 22.0, inner, bh), Font::Body, p.t2);
            py += h;
        }
        card.bottom()
    }

    fn paint_tuning(&mut self, c: &mut Canvas, card: &Card, x: f32, mut y: f32, w: f32) -> f32 {
        let p = self.pal.clone();
        let gap = 14.0;
        let draft = self.draft.clone().unwrap_or_default();
        let now = self.gpu_now.clone().unwrap_or_default();
        y = self.group_title(c, x, y, w, tf("Tuning", "Réglages"));
        let knobs = card.caps.knobs.clone();
        let fan_rows =
            if card.caps.fan_fixed.is_some() || card.caps.fan_curve.is_some() { 1.0 + if matches!(draft.fan, Fan::Curve(_)) { 1.6 } else { 0.0 } } else { 0.0 };
        let r = rect(x, y, w, (knobs.len() as f32 + fan_rows) * ROW);
        self.card(c, r);
        let mut ry = r.y;
        for (i, (k, range)) in knobs.iter().enumerate() {
            if i > 0 {
                c.hline(r.x + 20.0, ry, r.w - 40.0, p.line);
            }
            let (label, sub) = knob_text(*k, card.caps.power_percent);
            let v = draft.get(*k).unwrap_or(range.default);
            let tx = r.x + 20.0;
            let left = r.w * 0.5 - 20.0;
            c.text(label, rect(tx, ry + 12.0, left, 22.0), Font::BodyBold, p.t1, Align::Left);
            c.text_block(sub, rect(tx, ry + 34.0, left, 36.0), Font::Small, p.t3);
            // value, then − slider +
            let sx = r.x + r.w * 0.5;
            let val = show(*k, v, card.caps.power_percent);
            let changed = now.get(*k).is_some_and(|n| (n - v).abs() > 0.01);
            c.text(&val, rect(sx, ry + 10.0, 120.0, 22.0), Font::BodyBold, if changed { p.accent } else { p.t1 }, Align::Left);
            let range_txt = format!(
                "{} … {} · {} {}",
                show(*k, range.min, card.caps.power_percent),
                show(*k, range.max, card.caps.power_percent),
                tf("stock", "origine"),
                show(*k, range.default, card.caps.power_percent)
            );
            c.text(&range_txt, rect(sx + 120.0, ry + 10.0, r.right() - sx - 140.0, 22.0), Font::Small, p.t3, Align::Right);
            let line_y = ry + 40.0;
            self.pill(c, rect(sx, line_y, 32.0, 28.0), "−", None, None, true, Hit::KnobStep(*k, -1));
            self.pill(c, rect(r.right() - 20.0 - 32.0, line_y, 32.0, 28.0), "+", None, None, true, Hit::KnobStep(*k, 1));
            let track = rect(sx + 44.0, line_y, r.right() - 20.0 - 44.0 - sx - 44.0, 28.0);
            let f = |v: f64| ((v - range.min) / (range.max - range.min)).clamp(0.0, 1.0) as f32;
            self.slider(c, track, f(v), f(range.default), Hit::Slider(*k));
            self.sliders.push((*k, track));
            ry += ROW;
        }
        if fan_rows > 0.0 {
            if !knobs.is_empty() {
                c.hline(r.x + 20.0, ry, r.w - 40.0, p.line);
            }
            let tx = r.x + 20.0;
            c.text(tf("Fans", "Ventilateurs"), rect(tx, ry + 12.0, 300.0, 22.0), Font::BodyBold, p.t1, Align::Left);
            let sub = match &draft.fan {
                Fan::Auto => tf("The card's own automatic curve.", "La courbe automatique de la carte."),
                Fan::Fixed(_) => tf("The same speed whatever the temperature.", "La même vitesse quelle que soit la température."),
                Fan::Curve(_) => tf("Speed by temperature, kept by the driver itself.", "La vitesse selon la température, gardée par le pilote lui-même."),
            };
            c.text(sub, rect(tx, ry + 36.0, r.w * 0.5 - 40.0, 20.0), Font::Small, p.t3, Align::Left);
            let mut modes: Vec<(&str, usize)> = Vec::new();
            if card.vendor != crate::gpu::Vendor::Amd {
                modes.push((tf("Automatic", "Automatique"), 0));
            }
            if card.caps.fan_fixed.is_some() {
                modes.push((tf("Fixed", "Fixe"), 1));
            }
            if card.caps.fan_curve.is_some() {
                modes.push((tf("Curve", "Courbe"), 2));
            }
            let cur = match draft.fan {
                Fan::Auto => 0,
                Fan::Fixed(_) => 1,
                Fan::Curve(_) => 2,
            };
            let mut bx = r.right() - 20.0;
            for (label, m) in modes.iter().rev() {
                let bw = c.text_width(label, Font::Small) + 30.0;
                bx -= bw;
                let b = rect(bx, ry + 10.0, bw, 30.0);
                let on = cur == *m;
                let hit = Hit::FanMode(*m);
                c.fill_round(
                    b,
                    15.0,
                    if on {
                        p.fill2.over(p.card)
                    } else if self.hovered(&hit) {
                        p.hover
                    } else {
                        p.fill
                    },
                );
                if on {
                    c.stroke_round(b, 15.0, p.ring, 1.0);
                }
                c.text(label, b, if on { Font::SmallBold } else { Font::Small }, if on { p.t1 } else { p.t2 }, Align::Center);
                self.hits.add(b, hit);
                bx -= 4.0;
            }
            if let Fan::Fixed(pct) = draft.fan {
                let sx = r.x + r.w * 0.5;
                self.pill(c, rect(sx, ry + 42.0, 32.0, 26.0), "−", None, None, true, Hit::FanFixed(-1));
                c.text(&format!("{pct} %"), rect(sx + 40.0, ry + 42.0, 70.0, 26.0), Font::BodyBold, p.accent, Align::Center);
                self.pill(c, rect(sx + 118.0, ry + 42.0, 32.0, 26.0), "+", None, None, true, Hit::FanFixed(1));
            }
            if let Fan::Curve(pts) = &draft.fan {
                let cy = ry + ROW;
                let n = pts.len().max(1) as f32;
                let cw = (r.w - 40.0) / n;
                for (i, (tc, sp)) in pts.iter().enumerate() {
                    let cx_ = r.x + 20.0 + i as f32 * cw;
                    let b = rect(cx_ + 4.0, cy, cw - 8.0, ROW * 1.6 - 12.0);
                    c.fill_round(b, 12.0, p.fill);
                    c.text(&fill(tf("Point {}", "Point {}"), &[&(i + 1).to_string()]), rect(b.x, b.y + 6.0, b.w, 18.0), Font::Small, p.t3, Align::Center);
                    for (row, (txt, temp)) in [(format!("{tc} °C"), true), (format!("{sp} %"), false)].into_iter().enumerate() {
                        let yy = b.y + 28.0 + row as f32 * 36.0;
                        self.pill(c, rect(b.x + 6.0, yy, 28.0, 28.0), "−", None, None, true, Hit::FanPoint(i, temp, -1));
                        c.text(&txt, rect(b.x + 36.0, yy, b.w - 72.0, 28.0), Font::BodyBold, p.t1, Align::Center);
                        self.pill(c, rect(b.right() - 34.0, yy, 28.0, 28.0), "+", None, None, true, Hit::FanPoint(i, temp, 1));
                    }
                }
            }
        }
        y = r.bottom() + 12.0;

        // the buttons
        let changed = self.draft_changed();
        let busy = self.busy.is_some() || self.trying.is_some();
        let tl = tf("Try", "Essayer");
        let tw_ = self.pill_w(c, tl, true);
        self.pill(c, rect(x + w - tw_, y, tw_, 38.0), tl, Some(&ic::BOLT), Some(p.accent), changed && !busy, Hit::GpuTry);
        let mut bx = x + w - tw_ - 10.0;
        for (label, hit, on) in [
            (tf("Discard", "Annuler les modifications"), Hit::GpuDiscard, changed),
            (tf("Stock settings", "Réglages d'origine"), Hit::GpuStock, !busy),
            (tf("Stability test", "Test de stabilité"), Hit::GpuTest, !busy && self.stress.is_none()),
        ] {
            let bw = self.pill_w(c, label, false);
            bx -= bw;
            self.pill(c, rect(bx, y, bw, 38.0), label, None, None, on, hit);
            bx -= 10.0;
        }
        let hint = if changed {
            tf("Not applied yet: press Try.", "Pas encore appliqué : appuyez sur Essayer.")
        } else {
            tf("These are the card's settings now.", "Ce sont les réglages actuels de la carte.")
        };
        c.text(hint, rect(x + 4.0, y, (bx - x).max(80.0), 38.0), Font::Small, if changed { p.accent } else { p.t3 }, Align::Left);
        y += 38.0 + gap + 16.0;

        // profiles and sign-in
        y = self.group_title(c, x, y, w, tf("Profiles", "Profils"));
        let n = self.profiles.list.len();
        let notice = self.set.gpu_signin_blocked && !self.set.gpu_notice.is_empty();
        let h = 64.0 + n as f32 * 52.0 + if notice { 56.0 } else { 0.0 };
        let r = rect(x, y, w, h);
        self.card(c, r);
        c.text(
            tf(
                "Saves every card's settings as they are now. Load puts them in the controls; Try applies them.",
                "Enregistre les réglages actuels de chaque carte. Charger les met dans les commandes ; Essayer les applique.",
            ),
            rect(r.x + 20.0, r.y + 14.0, r.w - 240.0, 36.0),
            Font::Small,
            p.t3,
            Align::Left,
        );
        let sl = tf("Save as a profile", "Enregistrer comme profil");
        let sw = self.pill_w(c, sl, false);
        self.pill(c, rect(r.right() - 20.0 - sw, r.y + 14.0, sw, 34.0), sl, None, None, true, Hit::ProfileSave);
        let mut py = r.y + 60.0;
        if notice {
            let b = rect(r.x + 16.0, py, r.w - 32.0, 46.0);
            c.fill_round(b, 12.0, p.orange.fade(0.14));
            c.icon(&ic::WARN, rect(b.x + 12.0, b.y + 14.0, 18.0, 18.0), p.orange);
            c.text(
                &fill(tf("Paused at sign-in: {}", "Suspendu à la connexion : {}"), &[&self.set.gpu_notice]),
                rect(b.x + 40.0, b.y, b.w - 50.0, b.h),
                Font::Small,
                p.t1,
                Align::Left,
            );
            py += 56.0;
        }
        let signin = self.profiles.signin.clone();
        for i in 0..n {
            let prof = self.profiles.list[i].clone();
            c.hline(r.x + 20.0, py - 4.0, r.w - 40.0, p.line);
            c.text(&prof.name, rect(r.x + 20.0, py, 220.0, 44.0), Font::BodyBold, p.t1, Align::Left);
            let what = prof.cards.get(&card.id).map(|t| t.describe()).unwrap_or_else(|| tf("nothing for this card", "rien pour cette carte").into());
            c.text(&what, rect(r.x + 200.0, py, r.w - 620.0, 44.0), Font::Small, p.t3, Align::Left);
            let on = signin.as_deref() == Some(prof.name.as_str()) && !self.set.gpu_signin_blocked;
            let mut bx = r.right() - 20.0;
            for (label, hit, tone) in [
                (tf("Delete", "Supprimer"), Hit::ProfileDelete(i), None),
                (if on { tf("At sign-in ✓", "À la connexion ✓") } else { tf("At sign-in", "À la connexion") }, Hit::ProfileSignin(i), on.then_some(p.accent)),
                (tf("Load", "Charger"), Hit::ProfileLoad(i), None),
            ] {
                let bw = self.pill_w(c, label, false);
                bx -= bw;
                self.pill(c, rect(bx, py + 6.0, bw, 32.0), label, None, tone, true, hit);
                bx -= 8.0;
            }
            py += 52.0;
        }
        r.bottom() + gap + 10.0
    }
}
