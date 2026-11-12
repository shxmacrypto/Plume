//! Processor: the clocks as Windows reports them, and the processor settings Windows itself has in each power plan —
//! minimum and maximum state, turbo boost mode, energy performance preference (EPP) and core parking — each with what
//! it does and where it is documented. A choice applies at once to the active plan, after a snapshot, and Changes can
//! undo it. Then why Plume Boost doesn't undervolt or touch clock multipliers.

use super::{App, Hit, LIMIT_ROWS, choices_of, draw::ic, limit_index, paint::ghz, tweaks_view::boost_name};
use crate::{
    backend::guids,
    i18n::{fill, tf, tr},
};
use plume_ui::{Align, Canvas, Font, Rect, rect};

/// A setting's name and what it does (with its evidence).
fn setting_text(setting: &str) -> (&'static str, &'static str) {
    match setting {
        guids::PROC_MIN => (
            tr("Minimum processor state"),
            tf(
                "How slow the processor may go when idle. Higher wakes faster and uses more power. (PROCTHROTTLEMIN, Microsoft's processor power management options.)",
                "Jusqu'où le processeur peut ralentir au repos. Plus haut, il se réveille plus vite et consomme plus. (PROCTHROTTLEMIN, options de gestion du processeur de Microsoft.)",
            ),
        ),
        guids::PROC_MAX => (
            tr("Maximum processor state"),
            tf(
                "99 % turns turbo boost off on most Intel processors: cooler and quieter, slower under load. (PROCTHROTTLEMAX.)",
                "99 % coupe le turbo sur la plupart des processeurs Intel : plus frais et silencieux, plus lent en charge. (PROCTHROTTLEMAX.)",
            ),
        ),
        guids::BOOST_MODE => (
            tf("Turbo boost mode", "Mode turbo"),
            tf(
                "How eagerly the processor goes above its base clock. Efficient saves power on short bursts. (PERFBOOSTMODE.)",
                "Avec quel empressement le processeur dépasse sa fréquence de base. Efficace économise sur les courtes rafales. (PERFBOOSTMODE.)",
            ),
        ),
        guids::EPP => (
            tf("Energy performance preference", "Préférence performances / énergie"),
            tf(
                "For processors that choose their own clocks (Intel Speed Shift, AMD CPPC): 0 favours speed, 100 favours saving power. Measurable in clock and battery time. (PERFEPP.)",
                "Pour les processeurs qui choisissent leurs fréquences (Intel Speed Shift, AMD CPPC) : 0 favorise la vitesse, 100 l'économie. Mesurable en fréquence et en autonomie. (PERFEPP.)",
            ),
        ),
        _ => (
            tf("Core parking: minimum cores", "Parcage des cœurs : cœurs minimum"),
            tf(
                "The share of cores Windows keeps awake. 100 % parks none: quicker to respond, more idle power. On Ryzen X3D and Intel hybrid processors leave it to Windows: their scheduling relies on parking. (CPMINCORES.)",
                "La part des cœurs que Windows garde éveillés. 100 % n'en parque aucun : réponse plus rapide, plus de consommation au repos. Sur Ryzen X3D et processeurs hybrides Intel, laissez faire Windows : leur répartition s'appuie sur le parcage. (CPMINCORES.)",
            ),
        ),
    }
}

fn choice_label(setting: &str, v: u32) -> String {
    match setting {
        guids::BOOST_MODE => boost_name(v).to_string(),
        guids::PROC_MAX if v == 99 => tr("99 % (no turbo)").to_string(),
        guids::EPP => match v {
            0 => tf("0 speed", "0 vitesse").into(),
            100 => tf("100 saving", "100 économie").into(),
            v => v.to_string(),
        },
        guids::PARK_MIN if v == 10 => tf("10 % (default)", "10 % (défaut)").into(),
        guids::PARK_MIN if v == 100 => tf("100 % (none parked)", "100 % (aucun parqué)").into(),
        _ => format!("{v} %"),
    }
}

impl App {
    pub fn paint_cpu(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let top = self.page_title(
            c,
            page,
            tf("Processor", "Processeur"),
            tf(
                "Clocks as Windows reports them, and the processor settings of your power plan.",
                "Les fréquences données par Windows, et les réglages du processeur de votre mode d'alimentation.",
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
        let l = self.live.clone();

        // the processor
        let r = rect(x, y, w, 120.0);
        self.card(c, r);
        c.icon(&ic::CHIP, rect(r.x + 20.0, r.y + 20.0, 20.0, 20.0), p.accent);
        c.text(if l.cpu_name.is_empty() { "…" } else { &l.cpu_name }, rect(r.x + 50.0, r.y + 18.0, r.w - 70.0, 24.0), Font::Title, p.t1, Align::Left);
        let cols = [
            (tr("Now (all cores)"), l.cpu_mhz.map(ghz).unwrap_or("…".into())),
            (tr("Base clock"), l.base_mhz.map(ghz).unwrap_or("…".into())),
            (tr("Highest seen since opening"), l.peak_mhz.map(ghz).unwrap_or("…".into())),
            (tf("Load", "Charge"), l.cpu_pct.map(|v| format!("{v:.0} %")).unwrap_or("…".into())),
        ];
        let cw = (r.w - 40.0) / 4.0;
        for (i, (k, v)) in cols.iter().enumerate() {
            let cx_ = r.x + 20.0 + i as f32 * cw;
            c.text(k, rect(cx_, r.y + 56.0, cw - 10.0, 18.0), Font::Small, p.t3, Align::Left);
            c.text(v, rect(cx_, r.y + 76.0, cw - 10.0, 28.0), Font::Large, p.t1, Align::Left);
        }
        y = r.bottom() + gap + 10.0;

        // the plan's processor settings
        let plan = if l.plan.is_empty() { "…".to_string() } else { l.plan.clone() };
        y = self.group_title(c, x, y, w, &fill(tf("Processor settings of the plan \"{}\"", "Réglages du processeur du mode « {} »"), &[&plan]));
        c.text_block(
            tf(
                "Windows' own power settings. A choice applies at once after a snapshot, and Changes can undo it.",
                "Les réglages d'alimentation de Windows. Un choix s'applique aussitôt après un instantané, et Changements peut l'annuler.",
            ),
            rect(x + 4.0, y - 4.0, w, 20.0),
            Font::Small,
            p.t3,
        );
        y += 24.0;
        let rows: Vec<usize> = (0..LIMIT_ROWS.len()).filter(|&i| LIMIT_ROWS[i].1 || self.battery).collect();
        let row_h = 92.0;
        let sections = if self.battery { 2.0 } else { 1.0 };
        let card = rect(x, y, w, rows.len() as f32 * row_h + sections * 36.0);
        self.card(c, card);
        let mut ry = card.y;
        let mut last_ac = None;
        for &i in &rows {
            let (key, ac, setting) = LIMIT_ROWS[i];
            if last_ac != Some(ac) {
                c.text(
                    if ac { tr("Plugged in") } else { tr("On battery") },
                    rect(card.x + 20.0, ry + 10.0, 200.0, 22.0),
                    Font::SmallBold,
                    p.accent,
                    Align::Left,
                );
                ry += 36.0;
                last_ac = Some(ac);
            } else {
                c.hline(card.x + 20.0, ry, card.w - 40.0, p.line);
            }
            let cur = self.limits[if ac { 0 } else { 1 }][limit_index(setting)];
            let (label, sub) = setting_text(setting);
            let now = match cur {
                Some(v) if setting == guids::BOOST_MODE => boost_name(v).to_string(),
                Some(v) if setting == guids::EPP => v.to_string(),
                Some(v) => format!("{v} %"),
                None => tr("unknown").to_string(),
            };
            let r = rect(card.x + 20.0, ry, card.w - 40.0, row_h);
            let tw = r.w * 0.5;
            c.text(&format!("{label} — {now}"), rect(r.x, r.y + 10.0, tw, 20.0), Font::Body, p.t1, Align::Left);
            c.text_block(sub, rect(r.x, r.y + 32.0, tw, 44.0), Font::Small, p.t3);
            let ch = choices_of(setting);
            let labels: Vec<String> = ch.iter().map(|v| choice_label(setting, *v)).collect();
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            let sel = cur.and_then(|v| ch.iter().position(|x| *x == v));
            if cur.is_some() {
                self.segmented(c, rect(r.x + tw, r.y, r.w - tw, r.h), &refs, sel, key);
            }
            ry += row_h;
        }
        y = card.bottom() + gap;

        // shown in Windows' advanced power options
        let r = rect(x, y, w, 72.0);
        self.card(c, r);
        let (line, sub) = if self.attribs_shown {
            (
                tf("Shown in Windows' advanced power options", "Visibles dans les options d'alimentation avancées de Windows"),
                tf(
                    "Turbo mode, EPP and core parking appear in Control Panel › Power Options › Change advanced settings.",
                    "Mode turbo, EPP et parcage apparaissent dans Panneau de configuration › Options d'alimentation › paramètres avancés.",
                ),
            )
        } else {
            (
                tf("Hidden in Windows' advanced power options", "Masqués dans les options d'alimentation avancées de Windows"),
                tf(
                    "Windows hides turbo mode, EPP and core parking there. Showing them changes nothing else (administrator).",
                    "Windows y masque le mode turbo, l'EPP et le parcage. Les afficher ne change rien d'autre (administrateur).",
                ),
            )
        };
        c.text(line, rect(r.x + 20.0, r.y + 14.0, r.w - 260.0, 20.0), Font::BodyBold, p.t1, Align::Left);
        c.text(sub, rect(r.x + 20.0, r.y + 38.0, r.w - 260.0, 20.0), Font::Small, p.t3, Align::Left);
        let bl = if self.attribs_shown { tf("Hide them again", "Les masquer") } else { tf("Show them", "Les afficher") };
        let bw = self.pill_w(c, bl, false);
        self.pill(c, rect(r.right() - 20.0 - bw, r.y + 18.0, bw, 36.0), bl, None, None, self.busy.is_none(), Hit::CpuAttrib);
        y = r.bottom() + gap + 10.0;

        // what Plume Boost doesn't do, and why
        y = self.group_title(c, x, y, w, tf("Undervolting and overclocking the processor", "Undervolt et overclocking du processeur"));
        let paras: [(&str, &str); 3] = [
            (
                tf("Why Plume Boost doesn't undervolt or change multipliers", "Pourquoi Plume Boost ne fait ni undervolt ni changement de coefficient"),
                tf(
                    "Processor voltage and multipliers are written to model-specific registers (MSRs), which only kernel code can reach. Apps that do it install a driver such as WinRing0, and Windows now blocks those drivers as vulnerable (Microsoft's vulnerable driver blocklist, Defender): any program could use them to take over the PC. Plume Boost installs no driver. Intel has also locked undervolting on many recent laptops (Plundervolt fix), and the 2024 13th/14th-gen instability came from voltages that were too high.",
                    "La tension et les coefficients du processeur s'écrivent dans des registres spécifiques (MSR), que seul du code noyau atteint. Les applis qui le font installent un pilote comme WinRing0, et Windows bloque désormais ces pilotes comme vulnérables (liste de blocage de Microsoft, Defender) : n'importe quel programme pourrait s'en servir pour prendre le contrôle du PC. Plume Boost n'installe aucun pilote. Intel a aussi verrouillé l'undervolt sur beaucoup de portables récents (correctif Plundervolt), et l'instabilité des 13e/14e gén. de 2024 venait de tensions trop hautes.",
                ),
            ),
            (
                tf("Where it is done", "Où cela se fait"),
                tf(
                    "The BIOS/UEFI of your motherboard (Precision Boost Overdrive and Curve Optimizer on AMD, power limits on Intel), AMD Ryzen Master, Intel Extreme Tuning Utility on unlocked chips, or the laptop maker's own app. Change one thing at a time, test for stability, and watch temperatures.",
                    "Le BIOS/UEFI de la carte mère (Precision Boost Overdrive et Curve Optimizer sur AMD, limites de puissance sur Intel), AMD Ryzen Master, Intel Extreme Tuning Utility sur les puces débloquées, ou l'appli du fabricant du portable. Changez une chose à la fois, testez la stabilité, surveillez les températures.",
                ),
            ),
            (
                tf("What this page does instead", "Ce que fait cette page à la place"),
                tf(
                    "It sets what Windows exposes for every processor, in each power plan: how far it slows down, how high it may go, how eagerly it boosts, how it trades speed for power (EPP), and how many cores stay awake. All of it is measurable on this page and in Overview, and all of it can be undone.",
                    "Elle règle ce que Windows expose pour tout processeur, dans chaque mode : jusqu'où il ralentit, jusqu'où il monte, avec quel empressement il accélère, comment il arbitre vitesse et énergie (EPP), et combien de cœurs restent éveillés. Tout est mesurable ici et dans l'Aperçu, et tout s'annule.",
                ),
            ),
        ];
        y = self.paras(c, x, y, w, &paras) + 24.0;
        c.unclip();
        self.content_h = y + scroll - area.y;
        self.scrollbar(c, area);
    }
}
