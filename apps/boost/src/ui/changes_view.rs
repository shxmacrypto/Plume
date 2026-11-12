//! Changes: everything Plume Boost changed, newest first, each with the values it will put back and an Undo button;
//! "Undo everything" at the top; the snapshots taken before each change (what they hold, Restore this snapshot) and
//! where Plume Backup keeps their copies; the restore points it made.

use super::{App, Hit, Page, changed_title, draw::ic, paint::ago};
use crate::{
    backup_link,
    i18n::{fill, tf, tr},
};
use plume_ui::{Align, Canvas, Font, Rect, rect};

impl App {
    pub fn paint_changes(&mut self, c: &mut Canvas, page: Rect) {
        let p = self.pal.clone();
        let top = self.page_title(c, page, tr("Changes"), tr("Everything Plume Boost changed, and the value each setting gets back on Undo."));
        let changed = self.changed();
        if changed.is_empty() && self.snaps_list.is_empty() {
            let r = rect(page.x, top, page.w, page.h - top);
            self.content_h = 0.0;
            self.empty_state(
                c,
                r,
                &ic::HISTORY,
                tr("Nothing changed yet"),
                tr("Applied tweaks show up here, ready to undo."),
                Some((tr("Choose tweaks"), Hit::Nav(Page::Tweaks))),
            );
            return;
        }
        let x = page.x + 24.0;
        let w = (page.w - 48.0).min(980.0);
        let busy = self.busy.is_some();
        let area = rect(page.x, top, page.w, page.h - top);
        self.view_h = area.h;
        let scroll = self.scroll();
        c.clip(area);
        let mut y = area.y - scroll;
        for (i, id) in changed.iter().enumerate() {
            let entries: Vec<_> = self.log.entries.iter().filter(|e| &e.tweak == id).cloned().collect();
            let when = entries.iter().map(|e| e.when).max().unwrap_or(0);
            let lines: Vec<String> = entries.iter().map(|e| e.before.describe()).collect();
            let inner = w - 48.0 - 120.0;
            let lh: Vec<f32> = lines.iter().map(|l| c.text_height(l, inner, Font::Small) + 4.0).collect();
            let card = rect(x, y, w, 58.0 + lh.iter().sum::<f32>() + 14.0);
            self.card(c, card);
            c.text(&changed_title(id), rect(card.x + 24.0, card.y + 16.0, inner, 22.0), Font::BodyBold, p.t1, Align::Left);
            let admin = entries.iter().any(|e| e.before.needs_admin());
            let meta = if admin { format!("{} · {}", ago(when), tr("needs administrator rights to undo")) } else { ago(when) };
            c.text(&meta, rect(card.x + 24.0, card.y + 36.0, inner, 18.0), Font::Small, p.t3, Align::Left);
            let mut ly = card.y + 58.0;
            for (l, h) in lines.iter().zip(&lh) {
                c.text_block(l, rect(card.x + 24.0, ly, inner, *h), Font::Small, p.t2);
                ly += h;
            }
            let bl = tr("Undo");
            let bw = self.pill_w(c, bl, false);
            self.pill(c, rect(card.right() - 24.0 - bw, card.y + 18.0, bw, 32.0), bl, None, None, !busy, Hit::UndoTweak(i));
            y = card.bottom() + 14.0;
        }
        if !changed.is_empty() {
            c.text(
                &fill(tr("{} settings changed in all. Removing Plume Boost puts them back too."), &[&self.log.entries.len().to_string()]),
                rect(x + 4.0, y + 2.0, w, 20.0),
                Font::Small,
                p.t4,
                Align::Left,
            );
            y += 34.0;
        }

        // snapshots
        y = self.group_title(c, x, y, w, tf("Snapshots", "Instantanés"));
        let installed = backup_link::exe().is_some();
        let status = if !self.set.backup {
            tf(
                "Kept on this PC only: copies with Plume Backup are off in Settings.",
                "Gardés sur ce PC seulement : les copies Plume Backup sont coupées dans Paramètres.",
            )
            .to_string()
        } else if installed {
            fill(
                tf(
                    "Taken before every change. Plume Backup keeps dated copies (plan \"{}\", in {} unless you moved it).",
                    "Pris avant chaque changement. Plume Backup en garde des copies datées (plan « {} », dans {} sauf si vous l'avez déplacé).",
                ),
                &[backup_link::PLAN, &backup_link::default_dest().display().to_string()],
            )
        } else {
            tf(
                "Taken before every change, kept on this PC. Plume Backup isn't installed, so there is no second copy.",
                "Pris avant chaque changement, gardés sur ce PC. Plume Backup n'est pas installé : pas de seconde copie.",
            )
            .to_string()
        };
        let sh = c.text_height(&status, w - 48.0 - 200.0, Font::Small) + 4.0;
        let n = self.snaps_list.len().min(12);
        let extra = if self.backup_line.is_empty() { 0.0 } else { 22.0 };
        let card = rect(x, y, w, 28.0 + sh + extra + n as f32 * 56.0 + if n == 0 { 30.0 } else { 0.0 });
        self.card(c, card);
        c.text_block(&status, rect(card.x + 24.0, card.y + 14.0, w - 48.0 - 200.0, sh), Font::Small, p.t3);
        if self.set.backup && !installed {
            let bl = tf("Install Plume Backup", "Installer Plume Backup");
            let bw = self.pill_w(c, bl, false);
            self.pill(c, rect(card.right() - 24.0 - bw, card.y + 12.0, bw, 32.0), bl, None, Some(p.accent), !busy, Hit::BackupInstall);
        }
        let mut sy = card.y + 18.0 + sh;
        if !self.backup_line.is_empty() {
            c.text(&self.backup_line, rect(card.x + 24.0, sy, w - 48.0, 20.0), Font::Small, p.t2, Align::Left);
            sy += 22.0;
        }
        if n == 0 {
            c.text(tf("None yet.", "Aucun pour l'instant."), rect(card.x + 24.0, sy + 4.0, w - 48.0, 20.0), Font::Body, p.t3, Align::Left);
        }
        let list = self.snaps_list.clone();
        for (i, (dir, m)) in list.iter().take(12).enumerate() {
            c.hline(card.x + 20.0, sy + 4.0, card.w - 40.0, p.line);
            let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            c.text(&format!("{name} · {}", ago(m.time)), rect(card.x + 24.0, sy + 8.0, w * 0.55, 22.0), Font::BodyBold, p.t1, Align::Left);
            let holds = fill(
                tf(
                    "{} · {} registry values · {} plans · {} services · {} graphics cards",
                    "{} · {} valeurs du Registre · {} modes · {} services · {} cartes graphiques",
                ),
                &[m.why.text(), &m.reg_values.to_string(), &m.plans.to_string(), &m.services.to_string(), &m.gpus.to_string()],
            );
            c.text(&holds, rect(card.x + 24.0, sy + 30.0, w - 48.0 - 280.0, 20.0), Font::Small, p.t3, Align::Left);
            let mut bx = card.right() - 24.0;
            for (label, hit, tone) in
                [(tf("Restore this snapshot", "Restaurer cet instantané"), Hit::SnapRestore(i), None), (tf("Open", "Ouvrir"), Hit::SnapOpen(i), None)]
            {
                let bw = self.pill_w(c, label, false);
                bx -= bw;
                self.pill(c, rect(bx, sy + 14.0, bw, 32.0), label, None, tone, !busy, hit);
                bx -= 8.0;
            }
            sy += 56.0;
        }
        y = card.bottom() + 14.0;
        if self.snaps_list.len() > 12 {
            c.text(
                &fill(
                    tf("{} older ones in the Snapshots folder.", "{} plus anciens dans le dossier des instantanés."),
                    &[&(self.snaps_list.len() - 12).to_string()],
                ),
                rect(x + 4.0, y, w, 20.0),
                Font::Small,
                p.t4,
                Align::Left,
            );
            y += 26.0;
        }

        // restore points
        if !self.log.restore_points.is_empty() {
            y = self.group_title(c, x, y + 10.0, w, tr("Restore points made by Plume Boost"));
            let n = self.log.restore_points.len().min(6);
            let card = rect(x, y, w, 20.0 + n as f32 * 26.0 + 30.0);
            self.card(c, card);
            let mut ry = card.y + 14.0;
            for (when, name) in self.log.restore_points.iter().rev().take(6) {
                c.text(&format!("{name} — {}", ago(*when)), rect(card.x + 24.0, ry, card.w - 48.0, 24.0), Font::Body, p.t1, Align::Left);
                ry += 26.0;
            }
            c.text(
                tr("Windows restores them from System › System protection › System Restore."),
                rect(card.x + 24.0, ry + 2.0, card.w - 48.0, 20.0),
                Font::Small,
                p.t3,
                Align::Left,
            );
            y = card.bottom() + 14.0;
        }
        y += 26.0;
        c.unclip();
        self.content_h = y + scroll - area.y;
        self.scrollbar(c, area);
        // drawn last: above whatever scrolled under the title line
        if !changed.is_empty() {
            // the button stays at the top right, over the title line
            let ul = tr("Undo everything");
            let uw = self.pill_w(c, ul, true);
            self.pill(c, rect(x + w - uw, plume_ui::m::TITLE_H + 2.0, uw, 36.0), ul, Some(&ic::HISTORY), Some(p.accent), !busy, Hit::UndoAll);
        }
    }
}
