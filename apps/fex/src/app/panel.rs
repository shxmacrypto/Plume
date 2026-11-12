//! Les mini-panneaux : Ctrl+clic droit sur un dossier l'ouvre dans une petite carte flottante au-dessus de la barre
//! latérale. On y entre dans les sous-dossiers par son propre fil d'Ariane, et on fait glisser des fichiers entre
//! elle et la liste (ou un autre mini-panneau). Plusieurs peuvent être ouverts à la fois ; le dernier est devant.

use super::{
    App, Drag, Id,
    actions::with_app,
    load::{self, ListOpts, Listing, LoadError},
    model::{Item, Location},
};
use crate::{
    shell::{self, menu::Picked},
    ui::{Align, Button, Canvas, Color, Cx, Font, Mods, Pt, Rect, Theme, icons, m, rect},
};
use fex_core::SortKey;
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

/// Largeur d'un mini-panneau : celle de la barre latérale, entre ces deux bornes.
const PANEL_W: f32 = 300.0;
const MIN_W: f32 = 196.0;
const PANEL_H: f32 = 340.0;
const HEAD_H: f32 = 44.0;
const CRUMB_H: f32 = 30.0;
pub const PROW_H: f32 = 28.0;
const RADIUS: f32 = 16.0;

#[derive(Debug, PartialEq)]
pub enum PLoad {
    Loading,
    Ready,
    Failed(String),
}

pub struct Panel {
    /// Numéro unique : les zones de clic le désignent, quel que soit l'ordre d'empilement.
    pub id: usize,
    pub dir: PathBuf,
    pub items: Vec<Item>,
    pub load: PLoad,
    sig: u64,
    generation: u64,
    pub scroll: f32,
    /// Place de la carte dans la fenêtre.
    pub rect: Rect,
    pub anchor: Option<usize>,
    pub checked: Instant,
}

/// Les parties de la carte : en-tête (titre, fermer), fil d'Ariane, liste.
pub struct PanelLay {
    pub head: Rect,
    pub close: Rect,
    pub crumbs: Rect,
    pub list: Rect,
}

impl Panel {
    pub fn lay(&self) -> PanelLay {
        let r = self.rect;
        let head = rect(r.x, r.y, r.w, HEAD_H);
        let close = rect(r.right() - 38.0, r.y + (HEAD_H - 26.0) / 2.0, 26.0, 26.0);
        let crumbs = rect(r.x + 8.0, r.y + HEAD_H, r.w - 16.0, CRUMB_H);
        let list = rect(r.x + 6.0, crumbs.bottom() + 6.0, r.w - 12.0, (r.bottom() - crumbs.bottom() - 12.0).max(0.0));
        PanelLay { head, close, crumbs, list }
    }

    pub fn max_scroll(&self) -> f32 {
        (self.items.len() as f32 * PROW_H - self.lay().list.h + 4.0).max(0.0)
    }

    pub fn path_of(&self, row: usize) -> Option<PathBuf> {
        self.items.get(row).map(|it| self.dir.join(&it.name))
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.items.iter().filter(|it| it.sel).map(|it| self.dir.join(&it.name)).collect()
    }

    pub fn select_only(&mut self, row: usize) {
        self.items.iter_mut().enumerate().for_each(|(k, it)| it.sel = k == row);
        self.anchor = Some(row);
    }

    pub fn clear_sel(&mut self) {
        self.items.iter_mut().for_each(|it| it.sel = false);
    }
}

impl App {
    pub(super) fn panel_ix(&self, id: usize) -> Option<usize> {
        self.panels.iter().position(|p| p.id == id)
    }

    pub(super) fn panel(&self, id: usize) -> Option<&Panel> {
        self.panels.iter().find(|p| p.id == id)
    }

    pub(super) fn panel_mut(&mut self, id: usize) -> Option<&mut Panel> {
        self.panels.iter_mut().find(|p| p.id == id)
    }

    /// Ouvre un dossier dans un nouveau mini-panneau, près du point `at` (sur la barre latérale si elle est là).
    pub(super) fn open_panel(&mut self, dir: PathBuf, at: Pt) {
        let (w, h) = (self.lay.w, self.lay.status.bottom());
        let top = m::TITLE_H + m::BAR_H + 8.0;
        let ph = PANEL_H.min((h - top - m::STATUS_H - 16.0).max(160.0));
        let k = self.panels.len() as f32;
        // petit, posé sur la barre latérale : la liste reste libre pour y prendre ou y lâcher des fichiers
        let nav = self.lay.nav;
        let pw = if nav.w > 0.0 { (nav.w - 14.0).clamp(MIN_W, PANEL_W) } else { PANEL_W };
        let x = if nav.w > 0.0 { nav.x + 7.0 + 8.0 * k } else { at.x - 24.0 + 22.0 * k };
        let y = (at.y - 50.0 + 26.0 * k).clamp(top, (h - m::STATUS_H - 8.0 - ph).max(top));
        let x = x.clamp(8.0, (w - pw - 8.0).max(8.0));
        let id = self.next_panel;
        self.next_panel += 1;
        self.panels.push(Panel {
            id,
            dir,
            items: vec![],
            load: PLoad::Loading,
            sig: 0,
            generation: 0,
            scroll: 0.0,
            rect: rect(x, y, pw, ph),
            anchor: None,
            checked: Instant::now(),
        });
        self.load_panel(id, false);
        self.win.repaint();
    }

    pub(super) fn close_panel(&mut self, id: usize) {
        self.panels.retain(|p| p.id != id);
        if matches!(self.drop_hl, Some(Id::Panel(_) | Id::PanelRow(..))) {
            self.drop_hl = None;
        }
        self.win.repaint();
    }

    /// Met un mini-panneau devant les autres.
    pub(super) fn raise_panel(&mut self, id: usize) {
        if let Some(k) = self.panel_ix(id)
            && k + 1 != self.panels.len()
        {
            let p = self.panels.remove(k);
            self.panels.push(p);
        }
    }

    /// Le mini-panneau montre un autre dossier.
    pub(super) fn panel_go(&mut self, id: usize, dir: PathBuf) {
        let Some(p) = self.panel_mut(id) else { return };
        if p.dir != dir {
            p.dir = dir;
            p.items.clear();
            p.scroll = 0.0;
            p.anchor = None;
            p.sig = 0;
            p.load = PLoad::Loading;
        }
        self.load_panel(id, false);
        self.win.repaint();
    }

    /// (Re)lit le dossier d'un mini-panneau sur un autre fil. `silent` : rien ne change s'il n'a pas bougé.
    pub(super) fn load_panel(&mut self, id: usize, silent: bool) {
        let opts = ListOpts { show_hidden: self.set.show_hidden, show_system: self.set.show_system, sort: SortKey::Name, asc: true };
        let sender = self.win.sender();
        let Some(p) = self.panel_mut(id) else { return };
        p.generation += 1;
        p.checked = Instant::now();
        let (dir, generation, prev) = (p.dir.clone(), p.generation, silent.then_some(p.sig));
        std::thread::spawn(move || {
            load::com_init();
            let result = load::list(&Location::Path(dir.clone()), &opts, &AtomicBool::new(false), prev);
            // les icônes propres (programmes, raccourcis) des premiers éléments
            if let Ok(l) = &result {
                for it in l.items.iter().filter(|it| it.wants_exact_icon()).take(300) {
                    if let Some(i) = shell::icons::exact(&dir.join(&it.name)) {
                        it.icon.set(i.icon);
                    }
                }
            }
            sender.send(move |h, cx| {
                if let Some(app) = h.as_any().downcast_mut::<App>() {
                    app.on_panel_loaded(id, generation, result);
                    cx.repaint();
                }
            });
        });
    }

    fn on_panel_loaded(&mut self, id: usize, generation: u64, result: Result<Listing, LoadError>) {
        let s = self.s;
        let Some(p) = self.panel_mut(id) else { return };
        if p.generation != generation {
            return;
        }
        match result {
            Ok(l) if l.unchanged => {}
            Ok(l) => {
                let keep: Vec<String> = p.items.iter().filter(|it| it.sel).map(|it| it.name.clone()).collect();
                p.items = l.items;
                p.items.iter_mut().for_each(|it| it.sel = keep.contains(&it.name));
                p.sig = l.sig;
                p.load = PLoad::Ready;
                p.scroll = p.scroll.min(p.max_scroll());
            }
            Err(e) => {
                p.items.clear();
                p.load = PLoad::Failed(match e {
                    LoadError::Denied => s.denied.to_string(),
                    LoadError::NotFound => s.gone.to_string(),
                    LoadError::Other(msg) => format!("{} {msg}", s.cant_open),
                });
            }
        }
    }

    /// Relit tous les mini-panneaux (après une opération sur des fichiers, au retour sur la fenêtre).
    pub(super) fn refresh_panels(&mut self) {
        let ids: Vec<usize> = self.panels.iter().map(|p| p.id).collect();
        for id in ids {
            self.load_panel(id, true);
        }
    }

    /// Minuteur : un mini-panneau ouvert se remet à jour de temps en temps.
    pub(super) fn poll_panels(&mut self) {
        let due: Vec<usize> = self.panels.iter().filter(|p| p.load != PLoad::Loading && p.checked.elapsed().as_millis() > 1500).map(|p| p.id).collect();
        for id in due {
            self.load_panel(id, true);
        }
    }

    /// Le dossier qu'il faudrait ouvrir dans un mini-panneau pour la zone `id` (Ctrl+clic droit), s'il y en a un.
    pub(super) fn panel_target(&self, id: Option<Id>) -> Option<PathBuf> {
        match id? {
            Id::Row(r) => {
                let tab = self.tab();
                (r < tab.rows() && tab.item(r).is_dir() && !tab.stale).then(|| tab.path_of(r)).flatten()
            }
            Id::Nav(i) => self.nav_rows.get(i).and_then(|n| n.path()).map(Path::to_path_buf),
            Id::Crumb(i) => self.crumbs.get(i).and_then(|l| l.path()).map(Path::to_path_buf),
            Id::PanelRow(pid, r) => {
                let p = self.panel(pid)?;
                p.items.get(r).filter(|it| it.is_dir()).map(|it| p.dir.join(&it.name))
            }
            _ => None,
        }
    }

    /// Ouvre l'élément d'une ligne d'un mini-panneau : y entre (dossier) ou le lance (fichier).
    pub(super) fn panel_open_row(&mut self, id: usize, row: usize, cx: &mut Cx) {
        let Some(p) = self.panel(id) else { return };
        let Some(it) = p.items.get(row) else { return };
        let path = p.dir.join(&it.name);
        if it.is_dir() {
            self.panel_go(id, path);
        } else {
            cx.after(move |win| {
                shell::open(win.hwnd, &path);
            });
        }
    }

    /// Clic sur une ligne d'un mini-panneau.
    pub(super) fn panel_select(&mut self, id: usize, row: usize, ctrl: bool, shift: bool) {
        let Some(p) = self.panel_mut(id) else { return };
        if row >= p.items.len() {
            return;
        }
        if shift {
            let a = p.anchor.unwrap_or(row).min(p.items.len() - 1);
            let (lo, hi) = (a.min(row), a.max(row));
            p.items.iter_mut().enumerate().for_each(|(k, it)| it.sel = (lo..=hi).contains(&k) || (ctrl && it.sel));
        } else if ctrl {
            p.items[row].sel = !p.items[row].sel;
            p.anchor = Some(row);
        } else if !p.items[row].sel {
            p.select_only(row);
        }
    }

    pub(super) fn panel_wheel(&mut self, id: usize, dy: f32) {
        let lines = self.wheel_lines;
        if let Some(p) = self.panel_mut(id) {
            p.scroll = (p.scroll - dy * lines * PROW_H).clamp(0.0, p.max_scroll());
        }
        self.win.repaint();
    }

    /// Le mini-panneau suit la souris (tiré par son en-tête), sans sortir de la fenêtre.
    pub(super) fn panel_move(&mut self, id: usize, x: f32, y: f32) {
        let (w, h) = (self.lay.w, self.lay.status.bottom());
        let top = m::TITLE_H + m::BAR_H;
        if let Some(p) = self.panel_mut(id) {
            p.rect.x = x.clamp(-p.rect.w + 80.0, w - 80.0);
            p.rect.y = y.clamp(top, (h - 60.0).max(top));
        }
        self.win.repaint();
    }

    // ───────────────────────── dessin ─────────────────────────

    pub(super) fn paint_panels(&mut self, c: &mut Canvas, t: &Theme) {
        for k in 0..self.panels.len() {
            self.paint_panel(c, t, k);
        }
    }

    fn paint_panel(&mut self, c: &mut Canvas, t: &Theme, k: usize) {
        let s = self.s;
        let p = &self.panels[k];
        let (id, r, lay) = (p.id, p.rect, p.lay());
        // ombre portée en quelques couches, puis la carte
        let shade = if t.dark { 1.0 } else { 0.55 };
        for (grow, a) in [(14.0, 0.05), (8.0, 0.07), (3.0, 0.10)] {
            c.fill_round(rect(r.x - grow, r.y - grow + 6.0, r.w + 2.0 * grow, r.h + 2.0 * grow), RADIUS + grow, Color::hexa(0x000000, a * shade));
        }
        c.fill_round(r, RADIUS, t.card.over(t.bg));
        c.stroke_round(r, RADIUS, t.ring, 1.0);
        self.hits.add(r, Id::Panel(id));
        self.hits.add(lay.head, Id::PanelHead(id));

        // ── en-tête : dossier, titre, fermer
        let folder_icon = self.icons.by_ext("", true).0;
        let ir = rect(r.x + 16.0, lay.head.y + (HEAD_H - 18.0) / 2.0, 18.0, 18.0);
        self.icons.draw(c, folder_icon, ir, 1.0, false);
        let title = self.label_of(&p.dir);
        c.text(&title, rect(ir.right() + 10.0, lay.head.y, (lay.close.x - ir.right() - 16.0).max(0.0), HEAD_H), Font::BodyBold, t.t1, Align::Left);
        let hot = self.hover == Some(Id::PanelClose(id));
        if hot {
            c.fill_round(lay.close, 13.0, t.fill2);
        }
        c.icon(&icons::CLOSE, lay.close.centered(9.0, 9.0), if hot { t.t1 } else { t.t3 });
        self.hits.add(lay.close, Id::PanelClose(id));

        // ── fil d'Ariane du panneau : le dossier ouvert et ses parents, les premiers cèdent la place à « … »
        let chain = Location::Path(p.dir.clone()).chain();
        let labels: Vec<String> = chain.iter().map(|q| self.label_of(q)).collect();
        let widths: Vec<f32> = labels.iter().map(|l| c.text_width(l, Font::Small).min(140.0) + 12.0).collect();
        let avail = lay.crumbs.w - 4.0;
        let mut start = 0;
        let total =
            |from: usize| widths[from..].iter().sum::<f32>() + (widths.len() - from).saturating_sub(1) as f32 * 12.0 + if from > 0 { 26.0 } else { 0.0 };
        while start + 1 < labels.len() && total(start) > avail {
            start += 1;
        }
        let (cy, ch) = (lay.crumbs.y + 3.0, CRUMB_H - 6.0);
        let mut x = lay.crumbs.x;
        if start > 0 {
            c.text("…", rect(x, cy, 16.0, ch), Font::Small, t.t3, Align::Center);
            c.icon(&icons::CHEVRON_RIGHT, rect(x + 16.0, lay.crumbs.y + (CRUMB_H - 9.0) / 2.0, 9.0, 9.0), t.t4);
            x += 26.0;
        }
        for (ci, label) in labels.iter().enumerate().skip(start) {
            let wd = widths[ci].min((lay.crumbs.right() - x).max(16.0));
            let cr = rect(x, cy, wd, ch);
            let last = ci + 1 == labels.len();
            let cid = Id::PanelCrumb(id, ci);
            if self.hover == Some(cid) || self.drop_hl == Some(cid) {
                c.fill_round(cr, 7.0, if self.drop_hl == Some(cid) { t.accent.fade(0.25) } else { t.fill2 });
            }
            c.text(label, cr.inset_xy(6.0, 0.0), Font::Small, if last { t.t1 } else { t.t3 }, Align::Left);
            self.hits.add(cr, cid);
            x += wd;
            if !last {
                c.icon(&icons::CHEVRON_RIGHT, rect(x + 1.0, lay.crumbs.y + (CRUMB_H - 9.0) / 2.0, 9.0, 9.0), t.t4);
                x += 12.0;
            }
        }
        c.hline(r.x + 12.0, lay.crumbs.bottom() + 2.0, r.w - 24.0, t.sep);

        // ── la liste
        let p = &self.panels[k];
        let l = lay.list;
        let sel_bg = t.selection(self.active);
        c.clip(l);
        let first = (p.scroll / PROW_H).floor().max(0.0) as usize;
        let last = (((p.scroll + l.h) / PROW_H).ceil() as usize).min(p.items.len());
        for row in first..last {
            let it = &p.items[row];
            let y = c.snap(l.y + row as f32 * PROW_H - p.scroll);
            let bg = rect(l.x, y + 1.0, l.w - 4.0, PROW_H - 2.0);
            let rid = Id::PanelRow(id, row);
            if it.sel {
                c.fill_round(bg, 6.0, sel_bg);
            } else if self.hover == Some(rid) {
                c.fill_round(bg, 6.0, t.hover);
            }
            if self.drop_hl == Some(rid) {
                c.fill_round(bg, 6.0, t.accent.fade(0.22));
                c.stroke_round(bg, 6.0, t.accent, 1.5);
            }
            let fade = if it.is_hidden() { 0.5 } else { 1.0 };
            let icon = if it.icon.get() >= 0 { it.icon.get() } else { self.icons.by_ext(it.ext(), it.is_dir()).0 };
            self.icons.draw(c, icon, rect(bg.x + 8.0, y + (PROW_H - 16.0) / 2.0, 16.0, 16.0), fade, it.is_shortcut());
            let name = super::view::display_name(it, self.set.hide_ext);
            c.text(name, rect(bg.x + 32.0, y, (bg.w - 40.0).max(0.0), PROW_H), Font::Body, t.t1.fade(fade), Align::Left);
            self.hits.add(bg.intersect(&l), rid);
        }
        c.unclip();
        let msg = match &p.load {
            PLoad::Failed(msg) => Some(msg.as_str()),
            PLoad::Ready if p.items.is_empty() => Some(s.empty),
            _ => None,
        };
        if let Some(msg) = msg {
            c.icon(&icons::FOLDER, rect(l.x + (l.w - 20.0) / 2.0, l.y + 30.0, 20.0, 20.0), t.t3);
            c.text(msg, rect(l.x + 12.0, l.y + 58.0, l.w - 24.0, 22.0), Font::Body, t.t3, Align::Center);
        }
        // barre de défilement discrète
        let max = p.max_scroll();
        if max > 0.0 {
            let th = (l.h * l.h / (l.h + max)).clamp(24.0, l.h);
            let ty = l.y + (l.h - th) * (p.scroll / max);
            c.fill_round(rect(l.right() - 4.0, ty + 1.0, 3.0, th - 2.0), 1.5, t.t4);
        }
        // tout le panneau recevrait un dépôt
        if self.drop_hl == Some(Id::Panel(id)) {
            c.stroke_round(r.inset(1.0), RADIUS - 1.0, t.accent, 2.0);
        }
    }

    /// Bouton enfoncé sur un mini-panneau : il passe devant, puis selon la zone.
    pub(super) fn panel_down(&mut self, id: Option<Id>, p: Pt, button: Button, clicks: u8, mods: Mods, cx: &mut Cx) {
        let Some(id) = id else { return };
        let (Id::Panel(pid) | Id::PanelHead(pid) | Id::PanelClose(pid) | Id::PanelRow(pid, _) | Id::PanelCrumb(pid, _)) = id else { return };
        self.raise_panel(pid);
        match button {
            Button::Left => {
                self.pressed = Some(id);
                match id {
                    Id::PanelHead(_) => {
                        if let Some(pn) = self.panel(pid) {
                            self.drag = Drag::PanelMove { id: pid, dx: p.x - pn.rect.x, dy: p.y - pn.rect.y };
                        }
                    }
                    Id::PanelRow(_, r) if clicks == 2 && !mods.ctrl && !mods.shift => self.panel_open_row(pid, r, cx),
                    Id::PanelRow(_, r) => {
                        self.panel_select(pid, r, mods.ctrl, mods.shift);
                        self.drag = Drag::PanelItem { start: p, id: pid, row: r };
                    }
                    Id::Panel(_) => {
                        if let Some(pn) = self.panel_mut(pid) {
                            pn.clear_sel();
                        }
                    }
                    _ => {}
                }
            }
            Button::Middle => {
                if let Some(dir) = self.panel_target(Some(id)) {
                    self.new_tab(Location::Path(dir), false);
                }
            }
            Button::Right => {
                if let Id::PanelRow(_, r) = id {
                    self.panel_row_menu(pid, r, p, mods.shift, cx);
                }
            }
            _ => {}
        }
        cx.repaint();
    }

    /// Menu d'un élément d'un mini-panneau : celui de l'Explorateur, et « Ouvrir dans un mini-panneau » pour un dossier.
    fn panel_row_menu(&mut self, pid: usize, row: usize, p: Pt, extended: bool, cx: &mut Cx) {
        let s = self.s;
        let Some(pn) = self.panel_mut(pid) else { return };
        if pn.items.get(row).is_some_and(|it| !it.sel) {
            pn.select_only(row);
        }
        let paths = pn.selected_paths();
        let single_dir = paths.len() == 1 && pn.items.get(row).is_some_and(|it| it.is_dir());
        if paths.is_empty() {
            return;
        }
        let mut top: Vec<(u32, String)> = vec![];
        if single_dir {
            top.push((super::side::OPEN_MINI, s.open_mini.to_string()));
        }
        let at = self.win.to_screen(p);
        cx.after(move |win| {
            let picked = shell::menu::shell_menu(win, &paths, at, extended, &top);
            with_app(win, |app, _| match picked {
                Picked::Own(super::side::OPEN_MINI) => {
                    if let Some(dir) = paths.first() {
                        app.open_panel(dir.clone(), p);
                    }
                }
                Picked::Done(_) => {
                    app.check_now();
                    app.refresh_panels();
                }
                _ => {}
            });
        });
    }

    /// Le mini-panneau sous le point `p` (le plus en avant).
    pub(super) fn panel_at(&self, p: Pt) -> Option<usize> {
        self.panels.iter().rev().find(|q| q.rect.contains(p)).map(|q| q.id)
    }
}
