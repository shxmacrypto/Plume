//! Le dessin de la fenêtre : onglets, barre d'outils, barre latérale, liste, barre d'état.
//! Tout est redessiné à chaque image à partir de l'état ; les zones cliquables sont inscrites au passage.

use super::{
    App, Col, Drag, EditKind, Id, LIST_TOP, Lay, PAD, ROW_H,
    model::{Item, Load, Location},
};
use crate::ui::{Align, Canvas, Color, Font, PaintCx, Rect, Theme, icons, m, pt, rect};
use fex_core::{
    SortKey,
    query::{Kind, Options},
};
use std::path::PathBuf;

/// Nom affiché d'un élément : sans extension si Windows est réglé pour les masquer.
pub fn display_name(it: &Item, hide_ext: bool) -> &str {
    if hide_ext && !it.is_dir() && it.drive.is_none() {
        let ext = it.ext();
        if !ext.is_empty() && ext.len() <= 10 {
            return &it.name[..it.name.len() - ext.len() - 1];
        }
    }
    &it.name
}

/// Un chemin trop long pour sa colonne garde sa fin, la partie utile : « …\Projets\Plume\src ».
fn fit_start<'a>(c: &mut Canvas, text: &'a str, max_w: f32) -> std::borrow::Cow<'a, str> {
    if c.text_width(text, Font::Body) <= max_w {
        return text.into();
    }
    let mut rest = text;
    while let Some(i) = rest.find('\\') {
        rest = &rest[i + 1..];
        let short = format!("…\\{rest}");
        if !rest.contains('\\') || c.text_width(&short, Font::Body) <= max_w {
            return short.into();
        }
    }
    text.into()
}

fn millis(ms: f32) -> String {
    if ms < 10.0 { format!("{ms:.1} ms") } else { format!("{ms:.0} ms") }
}

impl App {
    pub(super) fn paint_all(&mut self, c: &mut Canvas, cx: &mut PaintCx, dt: f32) {
        self.active = cx.active;
        self.hits.clear();
        self.layout(cx.w, cx.h);
        let t = self.theme.clone();
        c.clear(t.bg);
        self.paint_list(c, cx, &t, dt);
        if self.lay.preview.w > 0.0 {
            let pane = self.lay.preview;
            self.paint_preview(c, &t, pane);
        }
        self.paint_nav(c, &t);
        self.paint_status(c, &t);
        self.paint_head(c, cx, &t);
        self.paint_overlays(c, cx, &t);
        // le champ de saisie de la barre d'outils suit sa place quand la fenêtre change de taille
        let wanted = match &self.edit {
            Some(EditKind::Address) => Some(self.lay.addr_text),
            Some(EditKind::Search) => Some(self.lay.search_text),
            Some(EditKind::Password) => self.sheet_lay().map(|l| l.text),
            _ => None,
        };
        if let Some(r) = wanted
            && r != self.edit_rect
        {
            self.edit_rect = r;
            self.win.edit_place(r);
        }
    }

    fn layout(&mut self, w: f32, h: f32) {
        let top = m::TITLE_H + m::BAR_H;
        let mut body = rect(0.0, top, w, (h - top - m::STATUS_H).max(0.0));
        let nav_w = if self.set.nav_open && w >= 520.0 { self.set.nav_w.min(w - 300.0).max(120.0) } else { 0.0 };
        let nav = body.cut_left(nav_w);
        // le volet d'aperçu prend la droite, quand la fenêtre est assez large
        let pw = if self.set.preview && w - nav_w >= 640.0 { ((w - nav_w) * 0.32).clamp(260.0, 460.0) } else { 0.0 };
        let preview = body.cut_right(pw);
        let mut list = body;
        let searching = matches!(self.tabs[self.cur].loc, Location::Search { .. });
        let scope = list.cut_top(if searching { 38.0 } else { 0.0 });
        let header = list.cut_top(m::HEADER_H);
        let status = rect(0.0, h - m::STATUS_H, w, m::STATUS_H);
        let fy = m::TITLE_H + (m::BAR_H - m::FIELD_H) / 2.0;
        let left = PAD + 4.0 * (m::NB_W + 2.0) + 6.0;
        let more_x = w - PAD - m::NB_W;
        let sw = if w >= 560.0 { (w * 0.24).clamp(170.0, 300.0) } else { 0.0 };
        let search = rect(more_x - 6.0 - sw, fy, sw, m::FIELD_H);
        let addr = rect(left, fy, (search.x - 8.0 - left).max(60.0), m::FIELD_H);
        let rows = self.tabs[self.cur].rows() as f32;
        let max_scroll = (rows * ROW_H + LIST_TOP + 10.0 - list.h).max(0.0);
        self.lay = Lay {
            w,
            preview,
            nav,
            header,
            list,
            status,
            addr,
            addr_text: rect(addr.x + 34.0, addr.y, (addr.w - 44.0).max(10.0), addr.h),
            search,
            search_text: rect(search.x + 32.0, search.y, (search.w - 58.0).max(10.0), search.h),
            thumb: self.lay.thumb,
            track: self.lay.track,
            max_scroll,
            scope,
        };
    }

    /// Les colonnes de la liste : (colonne, x, largeur). Les moins utiles disparaissent quand la place manque.
    pub(crate) fn columns(&self) -> Vec<(Col, f32, f32)> {
        let l = self.lay.list;
        let (x0, total) = (l.x + 6.0, (l.w - 20.0).max(0.0));
        let c = self.set.cols;
        let mut fixed: Vec<(Col, f32)> = match &self.tabs[self.cur].loc {
            Location::ThisPc => vec![(Col::Type, 170.0), (Col::Total, 110.0), (Col::Free, 110.0)],
            Location::Search { .. } => {
                let folder = if self.set.col_folder > 0.0 { self.set.col_folder } else { (total * 0.34).clamp(120.0, 460.0) };
                vec![(Col::Folder, folder), (Col::Date, c[0]), (Col::Size, c[2])]
            }
            Location::Path(_) | Location::Remote { .. } => vec![(Col::Date, c[0]), (Col::Type, c[1]), (Col::Size, c[2])],
        };
        for drop in [Col::Type, Col::Free, Col::Date, Col::Total, Col::Folder, Col::Size] {
            if total - fixed.iter().map(|f| f.1).sum::<f32>() >= 190.0 {
                break;
            }
            fixed.retain(|f| f.0 != drop);
        }
        let name_w = (total - fixed.iter().map(|f| f.1).sum::<f32>()).max(0.0);
        let mut out = vec![(Col::Name, x0, name_w)];
        let mut x = x0 + name_w;
        for (col, w) in fixed {
            out.push((col, x, w));
            x += w;
        }
        out
    }

    fn paint_list(&mut self, c: &mut Canvas, cx: &mut PaintCx, t: &Theme, dt: f32) {
        let (list, header, max) = (self.lay.list, self.lay.header, self.lay.max_scroll);
        let dragging = matches!(self.drag, Drag::Thumb { .. });
        {
            // défilement fluide vers la position demandée
            let tab = &mut self.tabs[self.cur];
            tab.target = tab.target.clamp(0.0, max);
            let d = tab.target - tab.scroll;
            if d.abs() > 0.5 && !dragging {
                tab.scroll += d * (1.0 - (-dt * 22.0).exp());
                cx.animate = true;
            } else {
                tab.scroll = tab.target;
            }
            tab.scroll = tab.scroll.clamp(0.0, max);
        }
        let cols = self.columns();
        let s = self.s;
        self.paint_scope(c, t);
        let tab = &self.tabs[self.cur];
        let searching = matches!(tab.loc, Location::Search { .. });
        let sortable = searching || matches!(tab.loc, Location::Path(_) | Location::Remote { .. });
        let (sort_key, sort_asc) = if searching { tab.search_sort } else { (tab.sort, tab.asc) };
        // recherche dans un dossier : les dossiers des résultats s'affichent à partir de celui-ci
        let base: Option<PathBuf> = match &tab.loc {
            Location::Search { from: Some(origin), .. } if !self.set.search_everywhere => origin.parent().map(|p| p.to_path_buf()),
            _ => None,
        };

        // ── en-tête des colonnes
        for &(col, x, w) in &cols {
            let label = match col {
                Col::Name => s.col_name,
                Col::Date => s.col_date,
                Col::Type => s.col_type,
                Col::Size => s.col_size,
                Col::Total => s.col_total,
                Col::Free => s.col_free,
                Col::Folder => s.col_folder,
            };
            let key = match col {
                Col::Name => Some(SortKey::Name),
                Col::Date => Some(SortKey::Modified),
                Col::Type => Some(SortKey::Type),
                Col::Size => Some(SortKey::Size),
                Col::Folder => Some(SortKey::Path),
                _ => None,
            };
            let hot = sortable && self.hover == Some(Id::Header(col));
            let right = matches!(col, Col::Size | Col::Total | Col::Free);
            let sorted = sortable && key == Some(sort_key);
            let lw = c.text_width(label, Font::Small);
            let pad = if col == Col::Name { 8.0 } else { 10.0 };
            let tr = rect(x + pad, header.y, (w - pad - 10.0 - if sorted && right { 14.0 } else { 0.0 }).max(0.0), header.h);
            c.text(label, tr, Font::Small, if hot { t.t1 } else { t.t3 }, if right { Align::Right } else { Align::Left });
            if sorted {
                let ax = if right { x + w - 10.0 - 10.0 } else { (tr.x + lw + 5.0).min(x + w - 12.0) };
                c.icon(if sort_asc { &icons::SORT_UP } else { &icons::SORT_DOWN }, rect(ax, header.y + 10.0, 10.0, 10.0), t.t3);
            }
            if col != Col::Name {
                c.vline(x, header.y + 8.0, header.h - 16.0, t.sep);
            }
            if sortable && key.is_some() && !(searching && col == Col::Type) {
                self.hits.add(rect(x, header.y, w, header.h), Id::Header(col));
            }
        }
        // le bord gauche de chaque colonne réglable se tire (par-dessus les zones de clic des en-têtes)
        if sortable {
            for &(col, x, _) in cols.iter().filter(|c| c.0 != Col::Name) {
                self.hits.add(rect(x - 4.0, header.y, 8.0, header.h), Id::ColSplit(col));
            }
        }
        c.hline(header.x, header.bottom() - 1.0, header.w, t.sep);

        // ── lignes
        let rows = tab.rows();
        let scroll = c.snap(tab.scroll);
        let first = ((scroll - LIST_TOP) / ROW_H).floor().max(0.0) as usize;
        let last = (((scroll + list.h - LIST_TOP) / ROW_H).ceil().max(0.0) as usize).min(rows);
        let slow = matches!(tab.load, Load::Loading(since) if since.elapsed().as_millis() > 150);
        let dim = tab.stale && slow;
        let renaming = match &self.edit {
            Some(EditKind::Rename { tab: id, name }) if *id == tab.id => Some(name.as_str()),
            _ => None,
        };
        let sel_bg = t.selection(self.active);
        self.first_row = first;
        self.name_right.clear();
        let mut ask: Vec<(String, PathBuf)> = vec![];
        c.clip(list);
        for r in first..last {
            let it = tab.item(r);
            let y = c.snap(list.y + LIST_TOP + r as f32 * ROW_H - scroll);
            let bg = rect(list.x + 6.0, y + 1.0, (list.w - 20.0).max(0.0), ROW_H - 2.0);
            if it.sel {
                c.fill_round(bg, 6.0, sel_bg);
            } else if self.hover == Some(Id::Row(r)) && !tab.stale {
                c.fill_round(bg, 6.0, t.hover);
            }
            if self.keyboard && self.active && tab.focus == Some(r) {
                c.stroke_round(bg, 6.0, t.t3, 1.0);
            }
            if self.drop_hl == Some(Id::Row(r)) {
                c.fill_round(bg, 6.0, t.accent.fade(0.22));
                c.stroke_round(bg, 6.0, t.accent, 1.5);
            }
            let fade = if dim {
                0.4
            } else if it.is_hidden() {
                0.5
            } else {
                1.0
            };
            let (ext_icon, type_name) = self.icons.by_ext(it.ext(), it.is_dir());
            for &(col, x, w) in &cols {
                let cell = rect(x + 10.0, y, (w - 20.0).max(0.0), ROW_H);
                let dimmed = t.t2.fade(fade);
                match col {
                    Col::Name => {
                        let icon = if it.icon.get() >= 0 { it.icon.get() } else { ext_icon };
                        self.icons.draw(c, icon, rect(x + 8.0, y + (ROW_H - 16.0) / 2.0, 16.0, 16.0), fade, it.is_shortcut());
                        let name = display_name(it, self.set.hide_ext);
                        let (tx, tw) = (x + 32.0, (w - 40.0).max(0.0));
                        if renaming != Some(it.name.as_str()) {
                            c.text(name, rect(tx, y, tw, ROW_H), Font::Body, t.t1.fade(fade), Align::Left);
                        }
                        self.name_right.push(tx + c.text_width(name, Font::Body).min(tw));
                        if !it.asked.get() && it.wants_exact_icon() {
                            it.asked.set(true);
                            if let Some(p) = tab.path_of(r) {
                                ask.push((it.name.clone(), p));
                            }
                        }
                    }
                    Col::Date => c.text(&self.dates.get(it.mtime), cell, Font::Body, dimmed, Align::Left),
                    Col::Type => match &it.drive {
                        Some(d) => c.text(&d.type_name, cell, Font::Body, dimmed, Align::Left),
                        None => c.text(&type_name, cell, Font::Body, dimmed, Align::Left),
                    },
                    Col::Size => {
                        // un dossier n'a de taille que si l'index l'a calculée
                        if !it.is_dir() || it.sized {
                            c.text(&s.size(it.size), cell, Font::Body, dimmed, Align::Right);
                        }
                    }
                    Col::Total => {
                        if let Some(d) = &it.drive {
                            c.text(&s.size(d.total), cell, Font::Body, dimmed, Align::Right);
                        }
                    }
                    Col::Free => {
                        if let Some(d) = &it.drive {
                            c.text(&s.size(d.free), cell, Font::Body, dimmed, Align::Right);
                        }
                    }
                    Col::Folder => {
                        if let Some(f) = &it.folder {
                            let shown = base.as_ref().and_then(|b| f.strip_prefix(b).ok()).unwrap_or(f).display().to_string();
                            let fitted = fit_start(c, &shown, cell.w);
                            c.text(&fitted, cell, Font::Body, dimmed, Align::Left);
                        }
                    }
                }
            }
        }
        // un dépôt irait dans le dossier ouvert
        if self.drop_hl == Some(Id::List) {
            c.stroke_round(list.inset(3.0), 8.0, t.accent.fade(0.8), 1.5);
        }
        // cadre du champ de renommage (le champ lui-même est un contrôle de Windows posé par-dessus)
        if renaming.is_some() {
            let r = self.edit_rect;
            let frame = rect(r.x - 5.0, r.y - 1.0, r.w + 10.0, r.h + 2.0);
            c.fill_round(frame, 5.0, t.solid);
            c.stroke_round(frame, 5.0, t.accent, 1.5);
        }
        c.unclip();
        self.hits.add(list, Id::List);

        // ── dossier vide, erreur
        let message = match &tab.load {
            Load::Failed(msg) => Some(msg.as_str()),
            Load::Ready if rows == 0 => Some(if tab.filter.trim().is_empty() && !searching { s.empty } else { s.no_match }),
            Load::Loading(_) if rows == 0 && slow => Some(s.loading),
            _ => None,
        };
        if let Some(msg) = message {
            c.text(msg, rect(list.x + 24.0, list.y + 36.0, (list.w - 48.0).max(0.0), 24.0), Font::Body, t.t3, Align::Center);
        }

        // ── barre de défilement, par-dessus la liste
        if max > 0.0 && list.h > 40.0 {
            let track = rect(list.right() - 12.0, list.y + 2.0, 12.0, list.h - 4.0);
            let th = (track.h * list.h / (list.h + max)).clamp(28.0, track.h);
            let ty = track.y + (track.h - th) * (tab.scroll / max);
            let hot = dragging || matches!(self.hover, Some(Id::Thumb | Id::Track));
            let wd = if hot { 8.0 } else { 4.0 };
            c.fill_round(rect(track.right() - 2.0 - wd, ty + 1.0, wd, th - 2.0), wd / 2.0, if hot { t.t3 } else { t.t4 });
            self.lay.track = track;
            self.lay.thumb = rect(track.x, ty, track.w, th);
            self.hits.add(track, Id::Track);
            self.hits.add(self.lay.thumb, Id::Thumb);
        } else {
            self.lay.track = Rect::default();
            self.lay.thumb = Rect::default();
        }
        if matches!(tab.load, Load::Loading(_)) {
            cx.animate = true;
        }
        self.ask_icons(ask);
    }

    /// Bandeau au-dessus des résultats de recherche : chercher dans le dossier de départ, ou partout.
    fn paint_scope(&mut self, c: &mut Canvas, t: &Theme) {
        let bar = self.lay.scope;
        if bar.h <= 0.0 {
            return;
        }
        let s = self.s;
        let Location::Search { from, .. } = &self.tabs[self.cur].loc else { return };
        let label_w = c.text_width(s.search_in_label, Font::Small);
        c.text(s.search_in_label, rect(bar.x + 16.0, bar.y, label_w + 4.0, bar.h), Font::Small, t.t3, Align::Left);
        let everywhere = self.set.search_everywhere || from.is_none();
        let mut pills: Vec<(Id, String, bool)> = vec![];
        if let Some(p) = from {
            pills.push((Id::ScopeHere, self.label_of(p), !everywhere));
        }
        pills.push((Id::ScopeAll, s.everywhere.to_string(), everywhere));
        // à droite : le type d'éléments gardé et les façons de comparer en cours ; un clic ouvre leur menu
        let o = self.search_opts;
        let mut filter = match o.kind {
            Kind::All => s.filter_all,
            Kind::Folders => s.kind_folder,
            Kind::Files(k) => s.kinds.get(k).copied().unwrap_or(s.filter_all),
        }
        .to_string();
        for (on, tag) in [(o.case, "Aa"), (o.word, "[ab]"), (o.path, "a\\b"), (o.regex, ".*")] {
            if on {
                filter.push_str("  ");
                filter.push_str(tag);
            }
        }
        let filtered = o != Options::default();
        let fw = c.text_width(&filter, Font::Small) + 40.0;
        let fr = rect(bar.right() - 16.0 - fw, bar.y + 7.0, fw, bar.h - 14.0);
        let mut x = bar.x + 16.0 + label_w + 10.0;
        // fenêtre étroite : la première pastille (le nom du dossier) cède la place qui manque
        let widths: Vec<f32> = pills.iter().map(|(_, label, _)| c.text_width(label, Font::Small).min(260.0) + 22.0).collect();
        let room = fr.x - 12.0 - x - 8.0 * pills.len() as f32;
        let over = (widths.iter().sum::<f32>() - room).max(0.0);
        for (k, (id, label, on)) in pills.into_iter().enumerate() {
            let w = if k == 0 { (widths[k] - over).max(44.0) } else { widths[k] };
            let r = rect(x, bar.y + 7.0, w, bar.h - 14.0);
            self.pill(c, t, r, id, on);
            c.text(&label, r.inset_xy(11.0, 0.0), Font::Small, if on { t.t1 } else { t.t2 }, Align::Left);
            x += w + 8.0;
        }
        self.pill(c, t, fr, Id::Filter, filtered);
        c.text(&filter, rect(fr.x + 11.0, fr.y, fr.w - 36.0, fr.h), Font::Small, if filtered { t.t1 } else { t.t2 }, Align::Left);
        c.icon(&icons::CHEVRON_DOWN, rect(fr.right() - 23.0, fr.y + (fr.h - 12.0) / 2.0, 12.0, 12.0), t.t3);
    }

    /// Fond d'une pastille du bandeau de recherche, et sa zone de clic.
    fn pill(&mut self, c: &mut Canvas, t: &Theme, r: Rect, id: Id, on: bool) {
        if on {
            c.fill_round(r, 12.0, t.accent.fade(if t.dark { 0.30 } else { 0.18 }));
        } else if self.hover == Some(id) {
            c.fill_round(r, 12.0, t.fill);
        } else {
            c.stroke_round(r, 12.0, t.sep, 1.0);
        }
        self.hits.add(r, id);
    }

    fn paint_nav(&mut self, c: &mut Canvas, t: &Theme) {
        let nav = self.lay.nav;
        if nav.w <= 0.0 {
            return;
        }
        c.fill(nav, t.bg2);
        c.clip(nav);
        let cur = &self.tabs[self.cur].loc;
        let mut y = nav.y + 8.0;
        for (i, row) in self.nav_rows.iter().enumerate() {
            y += row.gap;
            let r = rect(nav.x + 6.0, y, nav.w - 12.0, m::NAV_ROW_H);
            // un serveur est « ouvert » quel que soit le dossier où l'on s'y trouve
            let selected = match (&row.loc, cur) {
                (Location::Remote { server: a, .. }, Location::Remote { server: b, .. }) => a == b,
                (a, b) => a == b,
            };
            if selected {
                c.fill_round(r, m::ROW_R, t.fill);
            } else if self.hover == Some(Id::Nav(i)) {
                c.fill_round(r, m::ROW_R, t.hover);
            }
            if self.drop_hl == Some(Id::Nav(i)) {
                c.fill_round(r, m::ROW_R, t.accent.fade(0.22));
                c.stroke_round(r, m::ROW_R, t.accent, 1.5);
            }
            let ir = rect(r.x + 8.0 + row.indent, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0);
            if row.icon >= 0 {
                self.icons.draw(c, row.icon, ir, 1.0, false);
            } else {
                let icon = match &row.loc {
                    Location::ThisPc => &icons::PC,
                    Location::Remote { .. } => &icons::SERVER,
                    _ => &icons::FOLDER,
                };
                c.icon(icon, ir, t.t2);
            }
            let tr = rect(ir.right() + 9.0, r.y, (r.right() - ir.right() - 15.0).max(0.0), r.h);
            c.text(&row.label, tr, Font::Body, if selected { t.t1 } else { t.t2 }, Align::Left);
            self.hits.add(r, Id::Nav(i));
            y += m::NAV_ROW_H + 2.0;
        }
        c.unclip();
        c.vline(nav.right() - 1.0, nav.y, nav.h, t.sep);
        self.hits.add(rect(nav.right() - 3.0, nav.y, 6.0, nav.h), Id::NavSplit);
    }

    fn paint_status(&mut self, c: &mut Canvas, t: &Theme) {
        let st = self.lay.status;
        let s = self.s;
        c.fill(st, t.bg2);
        c.hline(st.x, st.y, st.w, t.sep);
        let tab = &self.tabs[self.cur];
        let mut text = if tab.stale {
            s.loading.to_string()
        } else if matches!(tab.loc, Location::Search { .. }) {
            if tab.total > tab.rows() {
                s.fill(s.results_shown, &[&s.num(tab.rows() as u64), &s.num(tab.total as u64)])
            } else {
                s.count(tab.rows() as u64, s.results)
            }
        } else {
            s.count(tab.rows() as u64, s.item)
        };
        if tab.sel_count > 0 && !tab.stale {
            text.push_str("    ");
            text.push_str(&s.count(tab.sel_count as u64, s.selected));
            if !tab.sel_dirs && tab.loc != Location::ThisPc {
                text.push_str("  ·  ");
                text.push_str(&s.size(tab.sel_size));
            }
        }
        c.text(&text, rect(st.x + 14.0, st.y, (st.w - 150.0).max(0.0), st.h), Font::Small, t.t3, Align::Left);
        if let Some((label, _)) = &self.busy {
            c.text(label, rect(st.right() - 474.0, st.y, 460.0, st.h), Font::Small, t.t2, Align::Right);
        } else if !self.ix.is_ready() && !self.ix.sleeping() {
            // premier lancement : l'index des disques se construit
            let n = self.ix.progress.entries.load(std::sync::atomic::Ordering::Relaxed);
            if n > 0 {
                let text = format!("{}  {}", s.indexing, s.num(n));
                c.text(&text, rect(st.right() - 474.0, st.y, 460.0, st.h), Font::Small, t.t4, Align::Right);
            }
        } else if tab.load == Load::Ready && !matches!(tab.loc, Location::ThisPc) {
            c.text(&millis(tab.took_ms), rect(st.right() - 124.0, st.y, 110.0, st.h), Font::Small, t.t4, Align::Right);
        }
    }

    fn paint_head(&mut self, c: &mut Canvas, cx: &mut PaintCx, t: &Theme) {
        let (w, top) = (cx.w, m::TITLE_H + m::BAR_H);
        c.gradient_v(rect(0.0, 0.0, w, top), t.hdr1, t.hdr2);
        c.hline(0.0, top - 1.0, w, t.edge);

        // ── boutons de la fenêtre
        let cap_x = w - 3.0 * m::CAPTION_W;
        let glyphs = [(Id::Min, '\u{E921}'), (Id::Max, if cx.maximized { '\u{E923}' } else { '\u{E922}' }), (Id::Close, '\u{E8BB}')];
        for (k, (id, glyph)) in glyphs.into_iter().enumerate() {
            let r = rect(cap_x + k as f32 * m::CAPTION_W, 0.0, m::CAPTION_W, m::TITLE_H - 8.0);
            let hot = self.hover == Some(id) || self.pressed == Some(id);
            if hot {
                c.fill(r, if id == Id::Close { t.close_hover } else { t.hover });
            }
            let col = if id == Id::Close && hot {
                Color::hex(0xffffff)
            } else if cx.active {
                t.t1
            } else {
                t.t3
            };
            c.glyph(glyph, r, col);
            self.hits.add(r, id);
        }

        // ── une version plus récente de la suite : le bouton qui met tout à jour (dans Plume Suite)
        let cap_x = match self.update.clone() {
            Some(v) => {
                let label = self.s.update_to.replace("{v}", &v);
                let pw = c.text_width(&label, Font::SmallBold) + 26.0;
                let r = rect(cap_x - pw - 10.0, (m::TITLE_H - 26.0) / 2.0, pw, 26.0);
                let fill = if self.hover == Some(Id::Update) { t.accent.mix(Color::hex(0xffffff), 0.12) } else { t.accent };
                c.fill_round(r, 13.0, fill);
                c.text(&label, r, Font::SmallBold, Color::hex(0xffffff), Align::Center);
                self.hits.add(r, Id::Update);
                r.x - 6.0
            }
            None => cap_x,
        };

        // ── onglets
        let n = self.tabs.len();
        let avail = (cap_x - 48.0 - PAD - m::NB_H - 8.0).max(m::TAB_MIN_W);
        let tw = ((avail - (n as f32 - 1.0) * 4.0) / n as f32).clamp(m::TAB_MIN_W, m::TAB_W);
        let ty = (m::TITLE_H - m::TAB_H) / 2.0;
        let titles: Vec<String> = self.tabs.iter().map(|tab| self.title_of(&tab.loc)).collect();
        let mut x = PAD;
        for (i, title) in titles.iter().enumerate() {
            let r = rect(x, ty, tw, m::TAB_H);
            let active = i == self.cur;
            let hot = matches!(self.hover, Some(Id::Tab(j) | Id::TabClose(j)) if j == i);
            if active {
                c.fill_round(r, m::TAB_R, t.fill2);
                c.stroke_round(r, m::TAB_R, t.ring, 1.0);
            } else if hot {
                c.fill_round(r, m::TAB_R, t.hover);
            }
            if self.drop_hl == Some(Id::Tab(i)) {
                c.stroke_round(r, m::TAB_R, t.accent, 1.5);
            }
            let ic = match &self.tabs[i].loc {
                Location::ThisPc => &icons::PC,
                Location::Search { .. } => &icons::SEARCH,
                Location::Remote { .. } => &icons::SERVER,
                Location::Path(p) if p.parent().is_none() => &icons::DRIVE,
                Location::Path(_) => &icons::FOLDER,
            };
            c.icon(ic, rect(r.x + 10.0, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), if active { t.t1 } else { t.t3 });
            let show_close = (active || hot) && tw >= 96.0;
            let tr = rect(r.x + 34.0, r.y, (r.w - 34.0 - if show_close { 30.0 } else { 8.0 }).max(0.0), r.h);
            let col = match (active, cx.active) {
                (true, true) => t.t1,
                (true, false) | (false, true) => t.t2,
                (false, false) => t.t3,
            };
            c.text(title, tr, Font::Body, col, Align::Left);
            self.hits.add(r, Id::Tab(i));
            if show_close {
                let cr = rect(r.right() - 26.0, r.y + (r.h - 20.0) / 2.0, 20.0, 20.0);
                let close_hot = self.hover == Some(Id::TabClose(i));
                if close_hot {
                    c.fill_round(cr, 6.0, t.fill2);
                }
                c.icon(&icons::CLOSE, cr.centered(9.0, 9.0), if close_hot { t.t1 } else { t.t3 });
                self.hits.add(cr, Id::TabClose(i));
            }
            x += tw + 4.0;
        }
        let nb = rect(x + 2.0, ty, m::NB_H, m::NB_H);
        if self.hover == Some(Id::NewTab) {
            c.fill_round(nb, m::NB_R, if self.pressed == Some(Id::NewTab) { t.fill2 } else { t.hover });
        }
        c.icon(&icons::PLUS, nb.centered(14.0, 14.0), t.t2);
        self.hits.add(nb, Id::NewTab);

        // ── boutons de navigation
        let by = m::TITLE_H + (m::BAR_H - m::NB_H) / 2.0;
        let tab = &self.tabs[self.cur];
        let buttons = [
            (Id::Back, &icons::BACK, !tab.back.is_empty()),
            (Id::Forward, &icons::FORWARD, !tab.fwd.is_empty()),
            (Id::Up, &icons::UP, tab.loc.parent().is_some()),
            (Id::Refresh, &icons::RELOAD, true),
            (Id::More, &icons::MORE, true),
        ];
        for (k, (id, ic, enabled)) in buttons.into_iter().enumerate() {
            let bx = if id == Id::More { w - PAD - m::NB_W } else { PAD + k as f32 * (m::NB_W + 2.0) };
            let r = rect(bx, by, m::NB_W, m::NB_H);
            if enabled {
                if self.hover == Some(id) {
                    c.fill_round(r, m::NB_R, if self.pressed == Some(id) { t.fill2 } else { t.hover });
                }
                self.hits.add(r, id);
            }
            c.icon(ic, r.centered(16.0, 16.0), if enabled { t.t2 } else { t.t4 });
        }

        // ── champ d'adresse : fil d'Ariane, ou saisie
        let a = self.lay.addr;
        let editing = self.edit == Some(EditKind::Address);
        c.fill_round(a, m::FIELD_R, t.fill);
        if editing {
            c.stroke_round(a, m::FIELD_R, t.accent, 1.5);
        }
        self.hits.add(a, Id::Address);
        let loc = self.tabs[self.cur].loc.clone();
        let loc_icon = match &loc {
            Location::ThisPc => &icons::PC,
            Location::Search { .. } => &icons::SEARCH,
            Location::Remote { .. } => &icons::SERVER,
            Location::Path(p) if p.parent().is_none() => &icons::DRIVE,
            Location::Path(_) => &icons::FOLDER,
        };
        c.icon(loc_icon, rect(a.x + 10.0, a.y + (a.h - 16.0) / 2.0, 16.0, 16.0), t.t2);
        self.crumbs.clear();
        if !editing {
            let mut parts: Vec<(String, Location)> = vec![(self.s.this_pc.to_string(), Location::ThisPc)];
            match &loc {
                Location::ThisPc => {}
                Location::Path(_) => parts.extend(loc.chain().into_iter().map(|p| (self.label_of(&p), Location::Path(p)))),
                Location::Search { .. } => parts.push((self.title_of(&loc), loc.clone())),
                // le serveur, puis chaque dossier du chemin
                Location::Remote { server, path } => {
                    parts.push((server.clone(), Location::Remote { server: server.clone(), path: "/".into() }));
                    let mut at = String::new();
                    for part in path.split('/').filter(|p| !p.is_empty()) {
                        at = format!("{at}/{part}");
                        parts.push((part.to_string(), Location::Remote { server: server.clone(), path: at.clone() }));
                    }
                }
            }
            let widths: Vec<f32> = parts.iter().map(|(label, _)| c.text_width(label, Font::Body).min(220.0) + 14.0).collect();
            let avail = (a.w - 34.0 - 44.0).max(40.0);
            // trop long : les premiers éléments laissent la place à « … »
            let mut start = 0;
            let total =
                |from: usize| widths[from..].iter().sum::<f32>() + (widths.len() - from).saturating_sub(1) as f32 * 14.0 + if from > 0 { 34.0 } else { 0.0 };
            while start + 1 < parts.len() && total(start) > avail {
                start += 1;
            }
            let mut x = a.x + 31.0;
            let (cy, ch) = (a.y + 4.0, a.h - 8.0);
            if start > 0 {
                c.text("…", rect(x, cy, 20.0, ch), Font::Body, t.t3, Align::Center);
                c.icon(&icons::CHEVRON_RIGHT, rect(x + 22.0, a.y + (a.h - 10.0) / 2.0, 10.0, 10.0), t.t4);
                x += 34.0;
            }
            for k in start..parts.len() {
                let (label, target) = &parts[k];
                let wd = widths[k].min((a.right() - 10.0 - x).max(20.0));
                let r = rect(x, cy, wd, ch);
                let id = Id::Crumb(self.crumbs.len());
                let last = k + 1 == parts.len();
                if self.hover == Some(id) {
                    c.fill_round(r, 7.0, t.fill2);
                }
                c.text(label, r.inset_xy(7.0, 0.0), Font::Body, if last { t.t1 } else { t.t2 }, Align::Left);
                self.hits.add(r, id);
                self.crumbs.push(target.clone());
                x += wd;
                if !last {
                    c.icon(&icons::CHEVRON_RIGHT, rect(x + 2.0, a.y + (a.h - 10.0) / 2.0, 10.0, 10.0), t.t4);
                    x += 14.0;
                }
            }
        }

        // ── champ de recherche
        let sr = self.lay.search;
        if sr.w > 0.0 {
            let searching = self.edit == Some(EditKind::Search);
            c.fill_round(sr, m::FIELD_R, t.fill);
            if searching {
                c.stroke_round(sr, m::FIELD_R, t.accent, 1.5);
            }
            self.hits.add(sr, Id::Search);
            c.icon(&icons::SEARCH, rect(sr.x + 10.0, sr.y + (sr.h - 14.0) / 2.0, 14.0, 14.0), t.t3);
            let filter = &self.tabs[self.cur].filter;
            if !searching {
                if filter.is_empty() {
                    c.text(&self.search_hint(), self.lay.search_text, Font::Body, t.t3, Align::Left);
                } else {
                    c.text(filter, self.lay.search_text, Font::Body, t.t1, Align::Left);
                }
            }
            if !filter.is_empty() {
                let cr = rect(sr.right() - 26.0, sr.y + (sr.h - 20.0) / 2.0, 20.0, 20.0);
                let hot = self.hover == Some(Id::SearchClear);
                if hot {
                    c.fill_round(cr, 6.0, t.fill2);
                }
                c.icon(&icons::CLOSE, cr.centered(9.0, 9.0), if hot { t.t1 } else { t.t3 });
                self.hits.add(cr, Id::SearchClear);
            }
        }

        // ── lecture en cours qui dure : filet animé sous la barre d'outils
        let working = match (&self.busy, &self.tabs[self.cur].load) {
            (Some((_, since)), _) => Some(*since),
            (None, Load::Loading(since)) if since.elapsed().as_millis() > 150 => Some(*since),
            _ => None,
        };
        if let Some(since) = working {
            let phase = (since.elapsed().as_secs_f32() * 0.9).fract();
            let bw = w * 0.3;
            c.fill(rect(-bw + (w + bw) * phase, top - 2.0, bw, 2.0), t.accent);
            cx.animate = true;
        }
    }

    fn paint_overlays(&mut self, c: &mut Canvas, cx: &mut PaintCx, t: &Theme) {
        if let Drag::Lasso { x0, y0, .. } = &self.drag {
            let l = self.lay.list;
            let a = pt(l.x + x0, l.y + y0 - self.tabs[self.cur].scroll);
            let b = self.mouse;
            let r = rect(a.x.min(b.x), a.y.min(b.y), (a.x - b.x).abs(), (a.y - b.y).abs()).intersect(&l);
            c.fill(r, t.accent.fade(0.16));
            c.stroke_round(r, 0.0, t.accent.fade(0.8), 1.0);
        }
        if let Some((text, since)) = &self.toast {
            let age = since.elapsed().as_secs_f32();
            let alpha = (age / 0.12).min(1.0) * ((2.6 - age) / 0.25).clamp(0.0, 1.0);
            let tw = c.text_width(text, Font::Body).min(cx.w - 96.0);
            let r = rect((cx.w - tw) / 2.0 - 18.0, self.lay.status.y - 54.0, tw + 36.0, 38.0);
            c.fill_round(r, 19.0, t.solid.fade(alpha));
            c.stroke_round(r, 19.0, t.ring.fade(alpha), 1.0);
            c.text(text, r.inset_xy(18.0, 0.0), Font::Body, t.t1.fade(alpha), Align::Center);
            cx.animate = true;
        }
        self.paint_sheet(c, t);
    }
}
