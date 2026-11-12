//! La barre latérale : ses lignes (dossiers du haut, Ce PC, récents, catégories, serveurs), ce qu'on y range à la
//! souris (ordre des dossiers du haut, des catégories et de leurs dossiers), et ses menus.

use super::{
    App, Drag, EditKind, NavKind, NavRow, Sec,
    actions::{cmd, with_app},
    model::{Location, leaf},
    settings::{CAT_COLORS, Category, RECENT_MAX},
};
use crate::{
    shell::menu::{self, Menu},
    ui::{Color, Cx, Pt, Rect, m, rect},
};
use std::path::{Path, PathBuf};

/// Hauteur d'un titre de section (« Récents », « Catégories »).
pub const SEC_H: f32 = 28.0;
/// Marge au-dessus de la première ligne.
pub const NAV_TOP: f32 = 8.0;

/// Nos entrées des menus de la barre latérale.
mod nav_cmd {
    pub const OPEN_MINI: u32 = 0x7201;
    pub const NEW_CAT_WITH: u32 = 0x7202;
    pub const REMOVE_FROM_CAT: u32 = 0x7203;
    pub const REMOVE_RECENT: u32 = 0x7204;
    pub const CLEAR_RECENT: u32 = 0x7205;
    pub const HIDE_RECENT: u32 = 0x7206;
    pub const NEW_CAT: u32 = 0x7207;
    pub const DELETE_CAT: u32 = 0x7209;
    pub const CAT_UP: u32 = 0x720A;
    pub const CAT_DOWN: u32 = 0x720B;
    /// Pastille : 0 aucune, puis les couleurs de `CAT_COLORS`.
    pub const COLOR: u32 = 0x7210;
    /// « Ajouter à une catégorie » : une entrée par catégorie.
    pub const ADD_TO_CAT: u32 = 0x7300;
}
pub use nav_cmd::{ADD_TO_CAT, NEW_CAT_WITH, OPEN_MINI};

/// Ce que donnerait un dépôt pendant qu'on range la barre latérale.
#[derive(Clone, Debug, PartialEq)]
pub enum NavAction {
    /// Place la ligne parmi les dossiers du haut, à cette position (épingle un dossier qui n'y était pas).
    Quick(usize),
    /// Déplace la catégorie tirée à cette position.
    Category(usize),
    /// Range le dossier dans la catégorie, à cette position.
    Folder(usize, usize),
    /// Ajoute le dossier à la fin de la catégorie (lâché sur son titre).
    Into(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavDrop {
    pub action: NavAction,
    /// Trait d'insertion (y dans la fenêtre), ou ligne mise en évidence.
    pub line: Option<f32>,
    pub into: Option<usize>,
}

/// Le nom des couleurs des pastilles, dans l'ordre de `CAT_COLORS`.
pub fn color_names(fr: bool) -> [&'static str; 7] {
    if fr { ["Orange", "Rouge", "Vert", "Bleu", "Violet", "Rose", "Gris"] } else { ["Orange", "Red", "Green", "Blue", "Purple", "Pink", "Grey"] }
}

pub fn cat_color(k: u8) -> Option<Color> {
    (k > 0).then(|| CAT_COLORS.get(k as usize - 1).map(|&c| Color::hex(c))).flatten()
}

/// Deux chemins de Windows désignent-ils le même dossier (sans tenir compte de la casse ni d'une barre finale) ?
pub fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.display().to_string().trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}

impl App {
    /// Les dossiers du haut (personnels et épinglés), dans l'ordre choisi par l'utilisateur.
    pub(super) fn quick_paths(&self) -> Vec<PathBuf> {
        self.nav_rows.iter().filter(|r| matches!(r.kind, NavKind::Quick(_))).filter_map(|r| r.path().map(Path::to_path_buf)).collect()
    }

    pub(super) fn rebuild_nav(&mut self) {
        let s = self.s;
        let mut rows: Vec<NavRow> = vec![];
        let mut y = 0.0;
        let mut add = |rows: &mut Vec<NavRow>, mut row: NavRow| {
            row.y = y + row.gap;
            y = row.y + row.h + 2.0;
            rows.push(row);
        };
        let folder_icon = self.icons.by_ext("", true).0;
        // ── les dossiers du haut, dans l'ordre choisi (ceux qui n'y sont pas encore gardent le leur, à la suite)
        let order = &self.set.quick_order;
        let mut places: Vec<(usize, &super::Place)> =
            self.places.iter().map(|p| (order.iter().position(|o| same_path(o, &p.path)).unwrap_or(usize::MAX), p)).collect();
        places.sort_by_key(|(k, _)| *k);
        for (k, (_, p)) in places.into_iter().enumerate() {
            add(&mut rows, NavRow::new(NavKind::Quick(k), Some(Location::Path(p.path.clone())), &p.name, p.icon));
        }
        // ── Ce PC et ses lecteurs
        let mut pc = NavRow::new(NavKind::Place, Some(Location::ThisPc), s.this_pc, -1);
        pc.gap = if rows.is_empty() { 0.0 } else { 12.0 };
        add(&mut rows, pc);
        for d in &self.drives {
            let mut r = NavRow::new(NavKind::Place, Some(Location::Path(d.root.clone())), &d.label, d.icon);
            r.indent = 14.0;
            add(&mut rows, r);
        }
        // ── les dossiers récents (ceux qui n'existent plus sont cachés)
        if self.set.show_recent {
            let shown: Vec<(usize, &PathBuf)> = self.set.recent.iter().enumerate().filter(|(_, p)| !self.is_gone(p)).take(RECENT_MAX).collect();
            if !shown.is_empty() {
                let mut head = NavRow::new(NavKind::Section(Sec::Recent), None, s.recent, -1);
                (head.gap, head.h) = (12.0, SEC_H);
                add(&mut rows, head);
                if self.set.recent_open {
                    for (k, p) in shown {
                        add(&mut rows, NavRow::new(NavKind::Recent(k), Some(Location::Path(p.clone())), &self.label_of(p), self.place_icon(p, folder_icon)));
                    }
                }
            }
        }
        // ── les catégories
        let mut head = NavRow::new(NavKind::Section(Sec::Categories), None, s.categories, -1);
        (head.gap, head.h) = (12.0, SEC_H);
        add(&mut rows, head);
        for (ci, cat) in self.set.categories.iter().enumerate() {
            add(&mut rows, NavRow::new(NavKind::CatHeader(ci), None, &cat.name, -1));
            if cat.open {
                for (fi, p) in cat.folders.iter().enumerate() {
                    let mut r = NavRow::new(NavKind::CatFolder(ci, fi), Some(Location::Path(p.clone())), &self.label_of(p), self.place_icon(p, folder_icon));
                    r.indent = 14.0;
                    r.dim = self.is_gone(p);
                    add(&mut rows, r);
                }
            }
        }
        // ── les serveurs SFTP déjà visités
        for (k, srv) in self.set.servers.iter().enumerate() {
            let mut r = NavRow::new(NavKind::Server, Some(Location::Remote { server: srv.clone(), path: String::new() }), srv, -1);
            r.gap = if k == 0 { 12.0 } else { 0.0 };
            add(&mut rows, r);
        }
        self.nav_h = y;
        self.nav_rows = rows;
    }

    /// L'icône d'un dossier de la barre : celle de Windows pour un dossier personnel ou un lecteur, sinon un dossier.
    fn place_icon(&self, p: &Path, folder: i32) -> i32 {
        if let Some(pl) = self.places.iter().find(|pl| same_path(&pl.path, p)) {
            return pl.icon;
        }
        if let Some(d) = self.drives.iter().find(|d| same_path(&d.root, p)) {
            return d.icon;
        }
        folder
    }

    pub(super) fn is_gone(&self, p: &Path) -> bool {
        self.gone.iter().any(|g| same_path(g, p))
    }

    /// Relit les lieux (dossiers personnels, épinglés, lecteurs) et vérifie que les dossiers retenus existent encore.
    pub(super) fn reload_places(&self) {
        let mut check: Vec<PathBuf> = self.set.recent.clone();
        check.extend(self.set.categories.iter().flat_map(|c| c.folders.iter().cloned()));
        super::load::spawn_places(self.win.sender(), self.set.pinned.clone(), check);
    }

    /// Rectangle d'une ligne de la barre latérale dans la fenêtre.
    pub(super) fn nav_rect(&self, i: usize) -> Option<Rect> {
        let nav = self.lay.nav;
        let row = self.nav_rows.get(i)?;
        Some(rect(nav.x + 6.0, nav.y + NAV_TOP + row.y - self.nav_scroll, nav.w - 12.0, row.h))
    }

    /// Fait défiler la barre latérale juste assez pour montrer la ligne `i`.
    fn nav_reveal(&mut self, i: usize) {
        let (Some(row), h) = (self.nav_rows.get(i), self.lay.nav.h) else { return };
        if row.y < self.nav_scroll {
            self.nav_scroll = row.y;
        } else if h > 0.0 && row.y + row.h + 2.0 * NAV_TOP > self.nav_scroll + h {
            self.nav_scroll = row.y + row.h + 2.0 * NAV_TOP - h;
        }
    }

    pub(super) fn nav_wheel(&mut self, dy: f32) {
        let max = (self.nav_h + 2.0 * NAV_TOP - self.lay.nav.h).max(0.0);
        self.nav_scroll = (self.nav_scroll - dy * 3.0 * m::NAV_ROW_H).clamp(0.0, max);
        self.win.repaint();
    }

    // ───────────────────────── dossiers récents ─────────────────────────

    /// Un dossier vient d'être ouvert : il passe en tête des récents.
    pub(super) fn note_recent(&mut self, p: &Path) {
        // les lecteurs ont déjà leur place sous Ce PC
        if p.parent().is_none() {
            return;
        }
        self.gone.retain(|g| !same_path(g, p));
        if self.set.recent.first().is_some_and(|r| r == p) {
            return;
        }
        self.set.recent.retain(|r| !same_path(r, p));
        self.set.recent.insert(0, p.to_path_buf());
        self.set.recent.truncate(RECENT_MAX);
        self.set.save();
        self.rebuild_nav();
    }

    fn set_recent(&mut self, f: impl FnOnce(&mut Vec<PathBuf>)) {
        f(&mut self.set.recent);
        self.set.save();
        self.rebuild_nav();
        self.win.repaint();
    }

    pub(super) fn show_recent(&mut self, on: bool) {
        self.set.show_recent = on;
        self.set.save();
        self.rebuild_nav();
        self.win.repaint();
    }

    // ───────────────────────── catégories ─────────────────────────

    /// Nouvelle catégorie (avec ces dossiers), dont le nom s'ouvre aussitôt à la saisie.
    pub(super) fn new_category(&mut self, folders: Vec<PathBuf>) {
        let base = self.s.new_category;
        let mut name = base.to_string();
        let mut n = 2;
        while self.set.categories.iter().any(|c| c.name == name) {
            name = format!("{base} {n}");
            n += 1;
        }
        let folders: Vec<PathBuf> = folders.into_iter().filter(|p| p.is_dir()).collect();
        self.set.categories.push(Category { name, color: 0, open: true, folders });
        self.set.save();
        self.rebuild_nav();
        self.start_cat_edit(self.set.categories.len() - 1);
    }

    /// Ouvre le champ de saisie sur le nom d'une catégorie.
    pub(super) fn start_cat_edit(&mut self, ci: usize) {
        self.end_edit(true);
        let Some(i) = self.nav_rows.iter().position(|r| r.kind == NavKind::CatHeader(ci)) else { return };
        self.nav_reveal(i);
        let Some(r) = self.cat_edit_rect(ci) else { return };
        let name = self.set.categories[ci].name.clone();
        let n = name.encode_utf16().count();
        self.edit = Some(EditKind::Category(ci));
        self.edit_rect = r;
        self.win.edit_show(r, &name, &self.edit_style(self.theme.solid, Some((0, n)), ""));
        self.win.repaint();
    }

    /// Place du champ de saisie du nom d'une catégorie (sur sa ligne).
    pub(super) fn cat_edit_rect(&self, ci: usize) -> Option<Rect> {
        let i = self.nav_rows.iter().position(|r| r.kind == NavKind::CatHeader(ci))?;
        let r = self.nav_rect(i)?;
        let x = r.x + if self.set.categories.get(ci).is_some_and(|c| c.color > 0) { 40.0 } else { 26.0 };
        Some(rect(x, r.y + 4.0, (r.right() - 8.0 - x).max(40.0), r.h - 8.0))
    }

    pub(super) fn commit_cat_name(&mut self, ci: usize, text: &str) {
        let name = text.trim();
        if let Some(c) = self.set.categories.get_mut(ci)
            && !name.is_empty()
        {
            c.name = name.to_string();
            self.set.save();
        }
        self.rebuild_nav();
    }

    /// Ajoute des dossiers à une catégorie (les fichiers n'y vont pas).
    pub(super) fn add_to_category(&mut self, ci: usize, paths: &[PathBuf]) {
        let Some(cat) = self.set.categories.get_mut(ci) else { return };
        let dirs: Vec<&PathBuf> = paths.iter().filter(|p| p.is_dir()).collect();
        if dirs.is_empty() {
            self.toast(self.s.only_folders.to_string());
            return;
        }
        for d in dirs {
            if !cat.folders.iter().any(|f| same_path(f, d)) {
                cat.folders.push(d.clone());
            }
        }
        cat.open = true;
        let name = cat.name.clone();
        self.set.save();
        self.rebuild_nav();
        self.toast(self.s.fill(self.s.added_to, &[&name]));
    }

    fn delete_category(&mut self, ci: usize) {
        if ci >= self.set.categories.len() {
            return;
        }
        if matches!(self.edit, Some(EditKind::Category(_))) {
            self.end_edit(false);
        }
        let gone = self.set.categories.remove(ci);
        self.set.save();
        self.rebuild_nav();
        self.toast(self.s.fill(self.s.category_deleted, &[&gone.name]));
    }

    fn move_category(&mut self, from: usize, to: usize) {
        let n = self.set.categories.len();
        if from >= n || to >= n || from == to {
            return;
        }
        let c = self.set.categories.remove(from);
        self.set.categories.insert(to, c);
        self.set.save();
        self.rebuild_nav();
    }

    fn toggle_category(&mut self, ci: usize) {
        if let Some(c) = self.set.categories.get_mut(ci) {
            c.open = !c.open;
            self.set.save();
            self.rebuild_nav();
        }
    }

    // ───────────────────────── clics ─────────────────────────

    /// Clic sur une ligne de la barre latérale.
    pub(super) fn nav_click(&mut self, i: usize) {
        let Some(row) = self.nav_rows.get(i) else { return };
        match row.kind {
            NavKind::Section(Sec::Recent) => {
                self.set.recent_open = !self.set.recent_open;
                self.set.save();
                self.rebuild_nav();
            }
            NavKind::Section(Sec::Categories) => {}
            NavKind::CatHeader(ci) => self.toggle_category(ci),
            _ if row.dim => self.toast(self.s.gone.to_string()),
            _ => {
                if let Some(loc) = row.loc.clone() {
                    self.navigate(loc);
                }
            }
        }
    }

    /// Les lignes que l'on peut tirer pour les ranger.
    pub(super) fn nav_draggable(&self, i: usize) -> bool {
        self.nav_rows.get(i).is_some_and(|r| matches!(r.kind, NavKind::Quick(_) | NavKind::Recent(_) | NavKind::CatHeader(_) | NavKind::CatFolder(..)))
    }

    /// La ligne de la barre latérale sous l'ordonnée `y` (la plus proche, dans la barre).
    fn nav_row_at_y(&self, y: f32) -> Option<usize> {
        let mut best = None;
        for i in 0..self.nav_rows.len() {
            let r = self.nav_rect(i)?;
            if y >= r.y - 1.0 && y < r.bottom() + 2.0 {
                return Some(i);
            }
            if y >= r.y {
                best = Some(i);
            }
        }
        best
    }

    /// Où irait la ligne `dragged` si on la lâchait au point `p`.
    pub(super) fn nav_drop_at(&self, dragged: usize, p: Pt) -> Option<NavDrop> {
        if !self.lay.nav.contains(p) {
            return None;
        }
        let from = self.nav_rows.get(dragged)?.kind;
        let j = self.nav_row_at_y(p.y)?;
        let to = self.nav_rows[j].kind;
        let r = self.nav_rect(j)?;
        let upper = p.y < r.y + r.h / 2.0;
        let edge = if upper { r.y - 1.0 } else { r.bottom() + 1.0 };
        let folder = matches!(from, NavKind::Quick(_) | NavKind::Recent(_) | NavKind::CatFolder(..));
        let line = |action| Some(NavDrop { action, line: Some(edge), into: None });
        match (from, to) {
            (NavKind::Quick(a), NavKind::Quick(b)) => {
                let at = if upper { b } else { b + 1 };
                (at != a && at != a + 1).then(|| line(NavAction::Quick(at))).flatten()
            }
            (_, NavKind::Quick(b)) if folder => line(NavAction::Quick(if upper { b } else { b + 1 })),
            (NavKind::CatFolder(c2, _), NavKind::CatHeader(c)) if c2 == c => None,
            (_, NavKind::CatHeader(c)) if folder => Some(NavDrop { action: NavAction::Into(c), line: None, into: Some(j) }),
            (NavKind::CatFolder(c2, f2), NavKind::CatFolder(c, f)) => {
                let at = if upper { f } else { f + 1 };
                (c2 != c || (at != f2 && at != f2 + 1)).then(|| line(NavAction::Folder(c, at))).flatten()
            }
            (_, NavKind::CatFolder(c, f)) if folder => line(NavAction::Folder(c, if upper { f } else { f + 1 })),
            (NavKind::CatHeader(a), NavKind::CatHeader(b) | NavKind::CatFolder(b, _)) => {
                // au-dessus du titre d'une catégorie : avant elle ; plus bas (sur elle ou ses dossiers) : après
                let before = upper && to == NavKind::CatHeader(b);
                let at = if before { b } else { b + 1 };
                if at == a || at == a + 1 {
                    return None;
                }
                let y = if before {
                    r.y - 1.0
                } else {
                    // le bas du bloc de la catégorie b
                    let last = self.nav_rows.iter().rposition(|x| matches!(x.kind, NavKind::CatHeader(c) | NavKind::CatFolder(c, _) if c == b)).unwrap_or(j);
                    self.nav_rect(last).map_or(edge, |q| q.bottom() + 1.0)
                };
                Some(NavDrop { action: NavAction::Category(at), line: Some(y), into: None })
            }
            _ => None,
        }
    }

    /// Lâche la ligne tirée : range la barre latérale.
    pub(super) fn nav_apply(&mut self, dragged: usize, d: NavDrop) {
        let Some(row) = self.nav_rows.get(dragged) else { return };
        let (kind, path) = (row.kind, row.path().map(Path::to_path_buf));
        match d.action {
            NavAction::Quick(at) => {
                let Some(path) = path else { return };
                let mut list = self.quick_paths();
                let at = match list.iter().position(|q| same_path(q, &path)) {
                    Some(k) => {
                        list.remove(k);
                        if k < at { at - 1 } else { at }
                    }
                    None => {
                        // un dossier récent ou d'une catégorie : il est épinglé là
                        if !self.set.pinned.iter().any(|q| same_path(q, &path)) {
                            self.set.pinned.push(path.clone());
                        }
                        at
                    }
                };
                list.insert(at.min(list.len()), path);
                self.set.quick_order = list;
                self.set.save();
                self.reload_places();
            }
            NavAction::Category(at) => {
                if let NavKind::CatHeader(a) = kind {
                    self.move_category(a, if a < at { at - 1 } else { at });
                }
            }
            NavAction::Folder(c, _) | NavAction::Into(c) if c < self.set.categories.len() => {
                let Some(path) = path else { return };
                let mut at = if let NavAction::Folder(_, at) = d.action { at } else { usize::MAX };
                // tiré d'une catégorie : il la quitte
                if let NavKind::CatFolder(c2, f2) = kind
                    && let Some(cat) = self.set.categories.get_mut(c2)
                    && f2 < cat.folders.len()
                {
                    cat.folders.remove(f2);
                    if c2 == c && f2 < at {
                        at -= 1;
                    }
                }
                let cat = &mut self.set.categories[c];
                if let Some(k) = cat.folders.iter().position(|q| same_path(q, &path)) {
                    cat.folders.remove(k);
                    if k < at {
                        at -= 1;
                    }
                }
                let at = at.min(cat.folders.len());
                cat.folders.insert(at, path);
                cat.open = true;
                self.set.save();
                self.rebuild_nav();
            }
            _ => {}
        }
        self.win.repaint();
    }

    // ───────────────────────── menus ─────────────────────────

    /// Le sous-menu « Ajouter à une catégorie » : les catégories, puis « Nouvelle catégorie… ».
    pub(super) fn categories_menu(&self) -> Menu {
        let mut sub = Menu::new();
        for (ci, c) in self.set.categories.iter().enumerate() {
            sub.item(ADD_TO_CAT + ci as u32, &c.name);
        }
        if !self.set.categories.is_empty() {
            sub.sep();
        }
        sub.item(NEW_CAT_WITH, self.s.new_category_dots);
        sub
    }

    /// Menu d'une ligne de la barre latérale (ou de son fond, `None`).
    pub(super) fn nav_menu(&mut self, i: Option<usize>, at: (i32, i32), cx: &mut Cx) {
        let s = self.s;
        let row = i.and_then(|i| self.nav_rows.get(i)).cloned();
        let kind = row.as_ref().map(|r| r.kind);
        if let (Some(NavKind::Server), Some(Location::Remote { server, .. })) = (kind, row.as_ref().and_then(|r| r.loc.clone())) {
            return self.remote_nav_menu(server, at, cx);
        }
        let mut m = Menu::new();
        match kind {
            None | Some(NavKind::Section(Sec::Categories)) => {
                m.item(nav_cmd::NEW_CAT, s.new_category_dots);
            }
            Some(NavKind::Section(Sec::Recent)) => {
                m.item(nav_cmd::CLEAR_RECENT, s.clear_recent).item(nav_cmd::HIDE_RECENT, s.hide_recent);
            }
            Some(NavKind::CatHeader(ci)) => {
                let cat = &self.set.categories[ci];
                let mut colors = Menu::new();
                colors.check(nav_cmd::COLOR, s.color_none, cat.color == 0);
                for (k, name) in color_names(s.lang == super::i18n::Lang::Fr).into_iter().enumerate() {
                    colors.check(nav_cmd::COLOR + 1 + k as u32, name, cat.color as usize == k + 1);
                }
                let n = self.set.categories.len();
                m.item(cmd::RENAME, s.rename).sub(s.color, colors).sep();
                m.add(nav_cmd::CAT_UP, s.move_up, false, ci > 0).add(nav_cmd::CAT_DOWN, s.move_down, false, ci + 1 < n);
                m.sep().item(nav_cmd::NEW_CAT, s.new_category_dots).item(nav_cmd::DELETE_CAT, s.delete_category);
            }
            Some(k) => {
                let path = row.as_ref().and_then(|r| r.path().map(Path::to_path_buf));
                m.item(cmd::OPEN, s.open).item(cmd::OPEN_TAB, s.open_new_tab);
                if let Some(p) = &path {
                    m.item(OPEN_MINI, s.open_mini).item(cmd::COPY_PATH, s.copy_path);
                    let is_drive = p.parent().is_none();
                    if !is_drive {
                        let pinned = self.set.pinned.iter().any(|q| same_path(q, p));
                        m.sep();
                        if pinned {
                            m.item(cmd::UNPIN, s.unpin);
                        } else if !matches!(k, NavKind::Quick(_)) {
                            m.item(cmd::PIN, s.pin);
                        }
                        m.sub(s.add_to_category, self.categories_menu());
                    }
                    if matches!(k, NavKind::CatFolder(..)) {
                        m.item(nav_cmd::REMOVE_FROM_CAT, s.remove_from_category);
                    }
                    if matches!(k, NavKind::Recent(_)) {
                        m.sep()
                            .item(nav_cmd::REMOVE_RECENT, s.remove_recent)
                            .item(nav_cmd::CLEAR_RECENT, s.clear_recent)
                            .item(nav_cmd::HIDE_RECENT, s.hide_recent);
                    }
                    m.sep().item(cmd::PROPERTIES, s.properties);
                }
            }
        }
        cx.after(move |win| {
            let picked = m.show(win, at, false);
            if let Some(id) = picked {
                with_app(win, |app, cx| {
                    app.nav_pick(id, row, at);
                    cx.repaint();
                });
            }
        });
    }

    fn nav_pick(&mut self, id: u32, row: Option<NavRow>, at: (i32, i32)) {
        let kind = row.as_ref().map(|r| r.kind);
        let loc = row.as_ref().and_then(|r| r.loc.clone());
        let path = row.as_ref().and_then(|r| r.path().map(Path::to_path_buf));
        // la barre a pu changer pendant que le menu était ouvert : la ligne est retrouvée par ce qu'elle montre
        match (id, kind) {
            (cmd::OPEN, _) => {
                if let Some(loc) = loc {
                    self.navigate(loc);
                }
            }
            (cmd::OPEN_TAB, _) => {
                if let Some(loc) = loc {
                    self.new_tab(loc, false);
                }
            }
            (OPEN_MINI, _) => {
                if let Some(p) = path {
                    let pt = self.win.from_screen(at.0, at.1);
                    self.open_panel(p, pt);
                }
            }
            (cmd::COPY_PATH, _) => {
                if let Some(p) = path {
                    self.copy_paths(&[p]);
                }
            }
            (cmd::PIN | cmd::UNPIN, _) => {
                if let Some(p) = path {
                    self.pin(p, id == cmd::PIN);
                }
            }
            (cmd::PROPERTIES, _) => {
                if let Some(p) = path {
                    self.later(move |win| {
                        menu::invoke(win, &[p], "properties", false);
                    });
                }
            }
            (NEW_CAT_WITH, _) => self.new_category(path.into_iter().collect()),
            (id, _) if (ADD_TO_CAT..ADD_TO_CAT + 0x100).contains(&id) => {
                if let Some(p) = path {
                    self.add_to_category((id - ADD_TO_CAT) as usize, &[p]);
                }
            }
            (nav_cmd::REMOVE_FROM_CAT, Some(NavKind::CatFolder(ci, _))) => {
                if let (Some(p), Some(cat)) = (path, self.set.categories.get_mut(ci)) {
                    cat.folders.retain(|q| !same_path(q, &p));
                    self.set.save();
                    self.rebuild_nav();
                }
            }
            (nav_cmd::REMOVE_RECENT, _) => {
                if let Some(p) = path {
                    self.set_recent(|r| r.retain(|q| !same_path(q, &p)));
                }
            }
            (nav_cmd::CLEAR_RECENT, _) => self.set_recent(Vec::clear),
            (nav_cmd::HIDE_RECENT, _) => self.show_recent(false),
            (nav_cmd::NEW_CAT, _) => self.new_category(vec![]),
            (cmd::RENAME, Some(NavKind::CatHeader(ci))) => self.start_cat_edit(ci),
            (nav_cmd::DELETE_CAT, Some(NavKind::CatHeader(ci))) => self.delete_category(ci),
            (nav_cmd::CAT_UP, Some(NavKind::CatHeader(ci))) if ci > 0 => self.move_category(ci, ci - 1),
            (nav_cmd::CAT_DOWN, Some(NavKind::CatHeader(ci))) => self.move_category(ci, ci + 1),
            (id, Some(NavKind::CatHeader(ci))) if (nav_cmd::COLOR..=nav_cmd::COLOR + CAT_COLORS.len() as u32).contains(&id) => {
                if let Some(c) = self.set.categories.get_mut(ci) {
                    c.color = (id - nav_cmd::COLOR) as u8;
                    self.set.save();
                    self.rebuild_nav();
                }
            }
            _ => {}
        }
    }

    /// Ce que fait le menu « Ajouter à une catégorie » choisi sur la sélection de la liste.
    pub(super) fn pick_category_for(&mut self, id: u32, paths: Vec<PathBuf>) {
        if id == NEW_CAT_WITH {
            self.new_category(paths);
        } else if id >= ADD_TO_CAT {
            self.add_to_category((id - ADD_TO_CAT) as usize, &paths);
        }
    }

    /// Bouton gauche enfoncé sur une ligne de la barre : un clic, ou le début d'un rangement si la souris bouge.
    pub(super) fn nav_down(&mut self, i: usize, p: Pt) {
        if self.nav_draggable(i) {
            self.drag = Drag::NavWait { start: p, row: i };
        }
    }

    /// Libellé court d'une ligne tirée (pour l'étiquette qui suit la souris).
    pub(super) fn nav_label(&self, i: usize) -> String {
        self.nav_rows.get(i).map_or_else(String::new, |r| match r.path() {
            Some(p) if r.label.is_empty() => leaf(p),
            _ => r.label.clone(),
        })
    }
}
