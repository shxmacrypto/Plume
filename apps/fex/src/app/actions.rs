//! Ce que l'on fait aux fichiers et à la fenêtre : menus contextuels, renommer, supprimer, couper, copier,
//! coller, nouveau dossier, propriétés, réglages. Les opérations elles-mêmes sont confiées au shell de Windows.

use super::{
    App, EditKind, Id, LIST_TOP, ROW_H, i18n,
    model::{Location, Restore, leaf},
    search, search_order,
};
use crate::{
    shell::{
        self,
        archive::{self, Failure, Format},
        integration::Classes,
        menu::{self, Menu, Picked},
        ops,
    },
    ui::{Cx, Mods, Pt, Window, rect},
};
use fex_core::{
    SortKey,
    query::{KINDS, Kind, Options},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

/// Numéros de nos entrées de menu (au-dessus de ceux réservés au shell).
mod cmd {
    pub const OPEN: u32 = 0x7001;
    pub const OPEN_TAB: u32 = 0x7002;
    pub const COPY_PATH: u32 = 0x7003;
    pub const PIN: u32 = 0x7004;
    pub const UNPIN: u32 = 0x7005;
    pub const REFRESH: u32 = 0x7006;
    pub const PASTE: u32 = 0x7007;
    pub const NEW_FOLDER: u32 = 0x7008;
    pub const SORT_NAME: u32 = 0x7009;
    pub const SORT_DATE: u32 = 0x700A;
    pub const SORT_TYPE: u32 = 0x700B;
    pub const SORT_SIZE: u32 = 0x700C;
    pub const ASC: u32 = 0x700D;
    pub const DESC: u32 = 0x700E;
    pub const HIDDEN: u32 = 0x700F;
    pub const EXT: u32 = 0x7010;
    pub const TERMINAL: u32 = 0x7011;
    pub const PROPERTIES: u32 = 0x7012;
    pub const NEW_TAB: u32 = 0x7013;
    pub const DUP_TAB: u32 = 0x7014;
    pub const CLOSE_TAB: u32 = 0x7015;
    pub const CLOSE_OTHERS: u32 = 0x7016;
    pub const THEME_SYSTEM: u32 = 0x7017;
    pub const THEME_DARK: u32 = 0x7018;
    pub const THEME_LIGHT: u32 = 0x7019;
    pub const LANG_AUTO: u32 = 0x701A;
    pub const LANG_EN: u32 = 0x701B;
    pub const LANG_FR: u32 = 0x701C;
    pub const ABOUT: u32 = 0x701D;
    pub const SELECT_ALL: u32 = 0x701E;
    pub const EDIT_ADDRESS: u32 = 0x701F;
    pub const COPY_ADDRESS: u32 = 0x7020;
    pub const OPEN_LOCATION: u32 = 0x7021;
    pub const EXTRACT_HERE: u32 = 0x7022;
    pub const EXTRACT_TO: u32 = 0x7023;
    pub const COMPRESS: u32 = 0x7024;
    pub const COMPRESS_LOCKED: u32 = 0x7025;
    pub const CONTEXT_ENTRY: u32 = 0x7026;
    pub const DEFAULT_EXPLORER: u32 = 0x7027;
    pub const FILTER_ALL: u32 = 0x7028;
    pub const FILTER_FOLDERS: u32 = 0x7029;
    pub const MATCH_CASE: u32 = 0x702A;
    pub const MATCH_WORD: u32 = 0x702B;
    pub const MATCH_PATH: u32 = 0x702C;
    pub const MATCH_REGEX: u32 = 0x702D;
    pub const EXPORT: u32 = 0x702E;
    pub const FOLDER_SIZES: u32 = 0x702F;
    pub const CONNECT: u32 = 0x7030;
    /// Suivi des types de fichiers, dans l'ordre de `fex_core::query::KINDS`.
    pub const FILTER_KIND: u32 = 0x7040;
}

const K_RETURN: u16 = 0x0D;
const K_DELETE: u16 = 0x2E;
const K_APPS: u16 = 0x5D;
const K_F2: u16 = 0x71;
const K_F10: u16 = 0x79;

/// Redonne la main à l'application depuis un travail lancé par `Cx::after`.
pub(super) fn with_app(win: &Window, f: impl FnOnce(&mut App, &mut Cx)) {
    win.with(|h, cx| {
        if let Some(app) = h.as_any().downcast_mut::<App>() {
            f(app, cx);
        }
    });
}

impl App {
    /// Lance un travail qui peut ouvrir des boîtes de dialogue, une fois l'événement en cours terminé.
    fn later(&self, f: impl FnOnce(&Window) + Send + 'static) {
        self.win.sender().send(move |_, cx| cx.after(f));
    }

    // ───────────────────────── menus contextuels ─────────────────────────

    pub(super) fn context_menu(&mut self, id: Option<Id>, p: Pt, mods: Mods, cx: &mut Cx) {
        let at = self.win.to_screen(p);
        match id {
            Some(Id::Row(r)) => self.row_menu(r, at, mods.shift, cx),
            Some(Id::List) => self.background_menu(at, cx),
            Some(Id::Tab(i) | Id::TabClose(i)) => self.tab_menu(i, at, cx),
            Some(Id::Nav(i)) => self.nav_menu(i, at, cx),
            Some(Id::Crumb(i)) => {
                let Some(Location::Path(path)) = self.crumbs.get(i).cloned() else { return };
                let mut m = Menu::new();
                m.item(cmd::OPEN_TAB, self.s.open_new_tab).item(cmd::COPY_PATH, self.s.copy_path);
                cx.after(move |win| {
                    let picked = m.show(win, at, false);
                    with_app(win, |app, _| match picked {
                        Some(cmd::OPEN_TAB) => app.new_tab(Location::Path(path), false),
                        Some(cmd::COPY_PATH) => app.copy_paths(&[path]),
                        _ => {}
                    });
                });
            }
            Some(Id::Address) => {
                let mut m = Menu::new();
                m.item(cmd::COPY_ADDRESS, self.s.copy_address).item(cmd::EDIT_ADDRESS, self.s.edit_address);
                cx.after(move |win| {
                    let picked = m.show(win, at, false);
                    with_app(win, |app, _| match picked {
                        Some(cmd::COPY_ADDRESS) => {
                            if let Some(p) = app.tab().loc.path().map(|p| p.display().to_string()) {
                                ops::copy_text(app.win.hwnd, &p);
                                app.toast(app.s.copied_path.to_string());
                            }
                        }
                        Some(cmd::EDIT_ADDRESS) => app.start_address_edit(),
                        _ => {}
                    });
                });
            }
            _ => {}
        }
    }

    /// Menu d'un fichier ou de la sélection : celui de l'Explorateur, avec quelques entrées à nous en tête.
    fn row_menu(&mut self, row: usize, at: (i32, i32), extended: bool, cx: &mut Cx) {
        if matches!(self.tab().loc, Location::Remote { .. }) {
            return self.remote_row_menu(row, at, cx);
        }
        let s = self.s;
        let tab = self.tab();
        if row >= tab.rows() || tab.stale {
            return;
        }
        let paths = tab.selected_paths();
        let Some(first) = tab.path_of(row) else { return };
        if paths.is_empty() {
            return;
        }
        let it = tab.item(row);
        let single_dir = paths.len() == 1 && it.is_dir();
        let mut top: Vec<(u32, String)> = vec![];
        if single_dir {
            top.push((cmd::OPEN_TAB, s.open_new_tab.to_string()));
            if it.drive.is_none() {
                let pinned = self.set.pinned.contains(&first);
                top.push(if pinned { (cmd::UNPIN, s.unpin.to_string()) } else { (cmd::PIN, s.pin.to_string()) });
            }
        }
        if matches!(tab.loc, Location::Search { .. }) && paths.len() == 1 {
            top.push((cmd::OPEN_LOCATION, s.open_location.to_string()));
        }
        if paths.len() == 1 && !it.is_dir() && archive::is_archive(&it.name) {
            top.push((cmd::EXTRACT_HERE, s.extract_here.to_string()));
            top.push((cmd::EXTRACT_TO, s.fill(s.extract_to, &[archive::stem(&it.name)])));
        }
        if it.drive.is_none() {
            top.push((cmd::COMPRESS, s.compress.to_string()));
            top.push((cmd::COMPRESS_LOCKED, s.compress_password.to_string()));
        }
        top.push((cmd::COPY_PATH, s.copy_path.to_string()));
        cx.after(move |win| {
            let picked = menu::shell_menu(win, &paths, at, extended, &top);
            with_app(win, |app, _| match picked {
                Picked::Own(cmd::EXTRACT_HERE) => app.extract(first, false, None),
                Picked::Own(cmd::EXTRACT_TO) => app.extract(first, true, None),
                Picked::Own(cmd::COMPRESS) => app.compress(paths),
                Picked::Own(cmd::COMPRESS_LOCKED) => app.ask_new_password(paths),
                Picked::Own(cmd::OPEN_TAB) => app.new_tab(Location::Path(first), false),
                Picked::Own(cmd::PIN) => app.pin(first, true),
                Picked::Own(cmd::UNPIN) => app.pin(first, false),
                Picked::Own(cmd::COPY_PATH) => app.copy_paths(&paths),
                Picked::Own(cmd::OPEN_LOCATION) => app.open_location(&first),
                Picked::Rename => app.start_rename(),
                Picked::Done(_) => app.check_now(),
                _ => {}
            });
        });
    }

    /// Menu du fond de la liste : ce qui concerne le dossier ouvert.
    fn background_menu(&mut self, at: (i32, i32), cx: &mut Cx) {
        if matches!(self.tab().loc, Location::Remote { .. }) {
            return self.remote_background_menu(at, cx);
        }
        let s = self.s;
        let tab = self.tab();
        let in_folder = matches!(tab.loc, Location::Path(_));
        let mut m = Menu::new();
        m.item(cmd::REFRESH, s.refresh);
        if in_folder {
            let mut sort = Menu::new();
            sort.check(cmd::SORT_NAME, s.col_name, tab.sort == SortKey::Name)
                .check(cmd::SORT_DATE, s.col_date, tab.sort == SortKey::Modified)
                .check(cmd::SORT_TYPE, s.col_type, tab.sort == SortKey::Type)
                .check(cmd::SORT_SIZE, s.col_size, tab.sort == SortKey::Size)
                .sep()
                .check(cmd::ASC, s.ascending, tab.asc)
                .check(cmd::DESC, s.descending, !tab.asc);
            m.sub(s.sort_by, sort);
            m.sep().item(cmd::PASTE, s.paste).item(cmd::NEW_FOLDER, s.new_folder);
        }
        m.item(cmd::SELECT_ALL, s.select_all);
        m.sep().check(cmd::HIDDEN, s.show_hidden, self.set.show_hidden);
        if in_folder {
            m.sep().item(cmd::TERMINAL, s.open_terminal).item(cmd::PROPERTIES, s.properties);
        }
        cx.after(move |win| {
            if let Some(id) = m.show(win, at, false) {
                with_app(win, |app, cx| app.command(id, cx));
            }
        });
    }

    fn tab_menu(&mut self, i: usize, at: (i32, i32), cx: &mut Cx) {
        let s = self.s;
        let mut m = Menu::new();
        m.item(cmd::NEW_TAB, s.new_tab).item(cmd::DUP_TAB, s.duplicate_tab).sep().item(cmd::CLOSE_TAB, s.close_tab);
        m.add(cmd::CLOSE_OTHERS, s.close_others, false, self.tabs.len() > 1);
        let id = self.tabs[i].id;
        cx.after(move |win| {
            let picked = m.show(win, at, false);
            with_app(win, |app, cx| {
                let Some(i) = app.tabs.iter().position(|t| t.id == id) else { return };
                match picked {
                    Some(cmd::NEW_TAB) => app.new_tab(Location::ThisPc, true),
                    Some(cmd::DUP_TAB) => {
                        let loc = app.tabs[i].loc.clone();
                        app.cur = i;
                        app.new_tab(loc, true);
                    }
                    Some(cmd::CLOSE_TAB) => app.close_tab(i, cx),
                    Some(cmd::CLOSE_OTHERS) => {
                        app.end_edit(false);
                        let keep = app.tabs.remove(i);
                        app.tabs.iter().for_each(|t| t.cancel.store(true, std::sync::atomic::Ordering::Relaxed));
                        app.tabs = vec![keep];
                        app.cur = 0;
                        app.update_title();
                        app.check_now();
                    }
                    _ => {}
                }
                cx.repaint();
            });
        });
    }

    fn nav_menu(&mut self, i: usize, at: (i32, i32), cx: &mut Cx) {
        let s = self.s;
        let Some(row) = self.nav_rows.get(i) else { return };
        let loc = row.loc.clone();
        if let Location::Remote { server, .. } = loc {
            return self.remote_nav_menu(server, at, cx);
        }
        let path = loc.path().map(|p| p.to_path_buf());
        let pinned = path.as_ref().is_some_and(|p| self.set.pinned.contains(p));
        let mut m = Menu::new();
        m.item(cmd::OPEN, s.open).item(cmd::OPEN_TAB, s.open_new_tab);
        if path.is_some() {
            m.item(cmd::COPY_PATH, s.copy_path);
            if pinned {
                m.sep().item(cmd::UNPIN, s.unpin);
            }
            m.sep().item(cmd::PROPERTIES, s.properties);
        }
        cx.after(move |win| {
            let picked = m.show(win, at, false);
            with_app(win, |app, _| match (picked, path) {
                (Some(cmd::OPEN), _) => app.navigate(loc),
                (Some(cmd::OPEN_TAB), _) => app.new_tab(loc, false),
                (Some(cmd::COPY_PATH), Some(p)) => app.copy_paths(&[p]),
                (Some(cmd::UNPIN), Some(p)) => app.pin(p, false),
                (Some(cmd::PROPERTIES), Some(p)) => app.later(move |win| {
                    menu::invoke(win, &[p], "properties", false);
                }),
                _ => {}
            });
        });
    }

    /// Menu « … » de la barre d'outils : affichage et réglages.
    pub(super) fn more_menu(&mut self, cx: &mut Cx) {
        let s = self.s;
        let Some(button) = self.hits.rect_of(|id| *id == Id::More) else { return };
        let at = self.win.to_screen(crate::ui::pt(button.right(), button.bottom() + 4.0));
        let mut theme = Menu::new();
        theme.check(cmd::THEME_SYSTEM, s.theme_system, self.set.theme == "system");
        theme.check(cmd::THEME_DARK, s.theme_dark, self.set.theme == "dark");
        theme.check(cmd::THEME_LIGHT, s.theme_light, self.set.theme == "light");
        let lang = self.set.lang.to_ascii_lowercase();
        let mut language = Menu::new();
        language.check(cmd::LANG_AUTO, s.theme_system, lang != "en" && lang != "fr");
        language.check(cmd::LANG_EN, "English", lang == "en");
        language.check(cmd::LANG_FR, "Français", lang == "fr");
        let mut m = Menu::new();
        m.item(cmd::NEW_TAB, s.new_tab)
            .item(cmd::CONNECT, s.sftp_connect)
            .sep()
            .check(cmd::HIDDEN, s.show_hidden, self.set.show_hidden)
            .check(cmd::EXT, s.show_ext, !self.set.hide_ext)
            .check(cmd::FOLDER_SIZES, s.folder_sizes, self.set.folder_sizes)
            .sep()
            .sub(s.theme, theme)
            .sub(s.language, language);
        // Plume FEx dans Windows : entrée du clic droit, explorateur par défaut (jamais pendant un essai)
        if let (Some(classes), Ok(exe)) = (Classes::user(), std::env::current_exe()) {
            m.sep().check(cmd::CONTEXT_ENTRY, s.ctx_entry, classes.has_entry(&exe)).check(cmd::DEFAULT_EXPLORER, s.default_explorer, classes.is_default(&exe));
        }
        m.sep().item(cmd::ABOUT, s.about);
        cx.after(move |win| {
            if let Some(id) = m.show(win, at, true) {
                with_app(win, |app, cx| app.command(id, cx));
            }
        });
    }

    /// Menu du bandeau de recherche : le type d'éléments à garder, les façons de comparer, l'export.
    pub(super) fn filter_menu(&mut self, cx: &mut Cx) {
        let s = self.s;
        let Some(button) = self.hits.rect_of(|id| *id == Id::Filter) else { return };
        let at = self.win.to_screen(crate::ui::pt(button.right(), button.bottom() + 4.0));
        let o = self.search_opts;
        let mut m = Menu::new();
        m.check(cmd::FILTER_ALL, s.filter_all, o.kind == Kind::All);
        // l'ordre d'Everything : audio, compressé, document, exécutable, dossier, image, vidéo
        for (k, (kind, _)) in KINDS.iter().enumerate() {
            if *kind == "pic" {
                m.check(cmd::FILTER_FOLDERS, s.kind_folder, o.kind == Kind::Folders);
            }
            m.check(cmd::FILTER_KIND + k as u32, s.kinds[k], o.kind == Kind::Files(k));
        }
        m.sep()
            .check(cmd::MATCH_CASE, s.match_case, o.case)
            .check(cmd::MATCH_WORD, s.match_word, o.word)
            .check(cmd::MATCH_PATH, s.match_path, o.path)
            .check(cmd::MATCH_REGEX, s.match_regex, o.regex)
            .sep()
            .item(cmd::EXPORT, s.export_results);
        cx.after(move |win| {
            if let Some(id) = m.show(win, at, true) {
                with_app(win, |app, cx| app.command(id, cx));
            }
        });
    }

    /// Change les réglages du bandeau de recherche et relance la recherche en cours.
    fn set_search_opts(&mut self, o: Options) {
        if self.search_opts != o {
            self.search_opts = o;
            if matches!(self.tab().loc, Location::Search { .. }) {
                self.run_search(self.cur);
            }
        }
    }

    /// Enregistre tous les résultats de la recherche en cours dans un fichier CSV choisi par l'utilisateur.
    fn export_results(&mut self, cx: &mut Cx) {
        let tab = self.tab();
        let Location::Search { query, from } = &tab.loc else { return };
        let (key, asc) = tab.search_sort;
        let req = search::Request {
            tab: tab.id,
            generation: tab.generation,
            query: query.clone(),
            scope: if self.set.search_everywhere { None } else { from.clone() },
            order: search_order(key),
            descending: !asc,
            cancel: Arc::new(AtomicBool::new(false)),
            options: self.search_opts,
        };
        let (shared, s, hwnd) = (self.ix.clone(), self.s, self.win.hwnd);
        cx.after(move |win| {
            let Some(file) = shell::dialogs::save_as(hwnd, s.export_title, "Plume FEx.csv", "csv", &[(s.csv_files, "*.csv")]) else { return };
            let sender = win.sender();
            std::thread::spawn(move || {
                let text = match search::export(&shared, &req, &file) {
                    Ok(rows) => s.fill(s.exported, &[&s.count(rows as u64, s.results)]),
                    Err(_) => s.export_failed.to_string(),
                };
                sender.send(move |h, _| {
                    if let Some(app) = h.as_any().downcast_mut::<App>() {
                        app.toast(text);
                    }
                });
            });
        });
    }

    /// Exécute une de nos commandes de menu.
    pub(super) fn command(&mut self, id: u32, cx: &mut Cx) {
        let o = self.search_opts;
        match id {
            cmd::FILTER_ALL => self.set_search_opts(Options { kind: Kind::All, ..o }),
            cmd::FILTER_FOLDERS => self.set_search_opts(Options { kind: Kind::Folders, ..o }),
            id if (cmd::FILTER_KIND..cmd::FILTER_KIND + KINDS.len() as u32).contains(&id) => {
                self.set_search_opts(Options { kind: Kind::Files((id - cmd::FILTER_KIND) as usize), ..o })
            }
            cmd::MATCH_CASE => self.set_search_opts(Options { case: !o.case, ..o }),
            cmd::MATCH_WORD => self.set_search_opts(Options { word: !o.word, ..o }),
            cmd::MATCH_PATH => self.set_search_opts(Options { path: !o.path, ..o }),
            cmd::MATCH_REGEX => self.set_search_opts(Options { regex: !o.regex, ..o }),
            cmd::EXPORT => self.export_results(cx),
            cmd::FOLDER_SIZES => self.set_folder_sizes(!self.set.folder_sizes),
            cmd::CONNECT => self.ask_server(),
            cmd::REFRESH => self.refresh(),
            cmd::SORT_NAME => self.set_sort(Some(SortKey::Name), None),
            cmd::SORT_DATE => self.set_sort(Some(SortKey::Modified), None),
            cmd::SORT_TYPE => self.set_sort(Some(SortKey::Type), None),
            cmd::SORT_SIZE => self.set_sort(Some(SortKey::Size), None),
            cmd::ASC => self.set_sort(None, Some(true)),
            cmd::DESC => self.set_sort(None, Some(false)),
            cmd::PASTE => self.paste(),
            cmd::NEW_FOLDER => self.new_folder(),
            cmd::SELECT_ALL => self.tab_mut().select_all(),
            cmd::HIDDEN => self.set_show_hidden(!self.set.show_hidden),
            cmd::EXT => {
                self.set.hide_ext = !self.set.hide_ext;
                self.set.save();
            }
            cmd::TERMINAL => {
                if let Some(dir) = self.tab().loc.path().map(|p| p.to_path_buf()) {
                    let hwnd = self.win.hwnd;
                    cx.after(move |_| {
                        shell::open_terminal(hwnd, &dir);
                    });
                }
            }
            cmd::PROPERTIES => {
                if let Some(dir) = self.tab().loc.path().map(|p| p.to_path_buf()) {
                    cx.after(move |win| {
                        menu::invoke(win, &[dir], "properties", false);
                    });
                }
            }
            cmd::NEW_TAB => self.new_tab(Location::ThisPc, true),
            cmd::THEME_SYSTEM | cmd::THEME_DARK | cmd::THEME_LIGHT => {
                self.set.theme = match id {
                    cmd::THEME_DARK => "dark",
                    cmd::THEME_LIGHT => "light",
                    _ => "system",
                }
                .to_string();
                self.set.own_theme = true;
                self.set.save();
                self.apply_theme();
            }
            cmd::LANG_AUTO | cmd::LANG_EN | cmd::LANG_FR => {
                self.set.lang = match id {
                    cmd::LANG_EN => "en",
                    cmd::LANG_FR => "fr",
                    _ => "auto",
                }
                .to_string();
                self.set.own_lang = true;
                self.set.save();
                self.s = i18n::strings(self.set.language());
                self.rebuild_nav();
                self.update_title();
            }
            cmd::ABOUT => self.toast(format!("Plume FEx {}", env!("CARGO_PKG_VERSION"))),
            cmd::CONTEXT_ENTRY | cmd::DEFAULT_EXPLORER => self.windows_integration(id == cmd::DEFAULT_EXPLORER),
            _ => {}
        }
        cx.repaint();
    }

    /// Inscrit ou retire Plume FEx dans Windows : `default` bascule l'option « explorateur par défaut », sinon
    /// c'est l'entrée du clic droit. Tout s'écrit dans le registre de l'utilisateur et se défait.
    fn windows_integration(&mut self, default: bool) {
        let (Some(classes), Ok(exe)) = (Classes::user(), std::env::current_exe()) else { return };
        let s = self.s;
        let was_default = classes.is_default(&exe);
        let message = if default && !was_default {
            match classes.make_default(&exe, s.open_in_fex) {
                Some(backup) => {
                    // une sauvegarde déjà gardée (réglage activé puis réinscrit) reste la bonne
                    self.set.explorer_backup.get_or_insert(backup);
                    s.default_on
                }
                None => s.registry_failed,
            }
        } else if default || was_default {
            // on quitte le rôle d'explorateur par défaut (demandé, ou parce que l'entrée du clic droit s'en va)
            classes.restore(&self.set.explorer_backup.take().unwrap_or_default());
            if !default {
                classes.set_entry(&exe, s.open_in_fex, false);
            }
            s.default_off
        } else {
            let on = !classes.has_entry(&exe);
            if classes.set_entry(&exe, s.open_in_fex, on) { "" } else { s.registry_failed }
        };
        self.set.save();
        if !message.is_empty() {
            self.toast(message.to_string());
        }
    }

    fn set_sort(&mut self, key: Option<SortKey>, asc: Option<bool>) {
        let tab = self.tab_mut();
        tab.sort = key.unwrap_or(tab.sort);
        tab.asc = asc.unwrap_or(tab.asc);
        tab.resort();
        if let Some(f) = self.tab().focus {
            self.reveal(f);
        }
    }

    // ───────────────────────── actions sur la sélection ─────────────────────────

    fn copy_paths(&mut self, paths: &[PathBuf]) {
        if !paths.is_empty() && ops::copy_text(self.win.hwnd, &ops::paths_text(paths)) {
            self.toast(self.s.copied_path.to_string());
        }
    }

    /// Ajoute ou retire un dossier de la barre latérale.
    fn pin(&mut self, path: PathBuf, on: bool) {
        self.set.pinned.retain(|p| *p != path);
        if on {
            self.set.pinned.push(path);
        }
        self.set.save();
        super::load::spawn_places(self.win.sender(), self.set.pinned.clone());
    }

    /// Ouvre le dossier d'un résultat de recherche, en y sélectionnant l'élément.
    fn open_location(&mut self, path: &std::path::Path) {
        let Some(parent) = path.parent() else { return };
        let name = super::model::leaf(path);
        self.go(Location::Path(parent.to_path_buf()), Some(Restore { select: vec![name.clone()], focus: Some(name), scroll: None, rename: false }), true);
    }

    /// Commande du shell (« copy », « cut », « delete », « properties ») sur la sélection.
    fn verb(&mut self, verb: &'static str, shift: bool) {
        let tab = self.tab();
        if tab.stale {
            return;
        }
        let paths = tab.selected_paths();
        if paths.is_empty() {
            return;
        }
        self.later(move |win| {
            menu::invoke(win, &paths, verb, shift);
            with_app(win, |app, _| app.check_now());
        });
    }

    /// Colle le presse-papiers dans le dossier ouvert.
    pub(super) fn paste(&mut self) {
        let Some(dir) = self.tab().loc.path().map(|p| p.to_path_buf()) else { return };
        self.later(move |win| {
            menu::invoke(win, &[dir], "paste", false);
            with_app(win, |app, _| app.check_now());
        });
    }

    pub(super) fn new_folder(&mut self) {
        let Some(dir) = self.tab().loc.path().map(|p| p.to_path_buf()) else { return };
        self.end_edit(true);
        match ops::new_folder(&dir, self.s.new_folder) {
            Ok(name) => {
                let i = self.cur;
                let tab = self.tab_mut();
                tab.filter.clear();
                tab.restore = Some(Restore { select: vec![name.clone()], focus: Some(name), scroll: None, rename: true });
                self.load(i);
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    // ───────────────────────── archives ─────────────────────────

    /// Lance un travail de fond sur le dossier `dir` ; à la fin, l'élément dont il renvoie le nom y est sélectionné.
    /// `asking` : l'archive en cours d'extraction, pour demander son mot de passe s'il manque ou s'il est faux.
    fn job(&mut self, label: String, dir: PathBuf, asking: Option<(PathBuf, bool)>, work: impl FnOnce() -> Result<String, Failure> + Send + 'static) {
        self.jobs += 1;
        self.busy = Some((label, Instant::now()));
        self.win.repaint();
        let sender = self.win.sender();
        std::thread::spawn(move || {
            let result = work();
            sender.send(move |h, _| {
                if let Some(app) = h.as_any().downcast_mut::<App>() {
                    app.job_done(&dir, asking, result);
                }
            });
        });
    }

    fn job_done(&mut self, dir: &Path, asking: Option<(PathBuf, bool)>, result: Result<String, Failure>) {
        self.jobs = self.jobs.saturating_sub(1);
        if self.jobs == 0 {
            self.busy = None;
        }
        match (result, asking) {
            (Ok(name), _) => {
                if let Some(i) = self.tabs.iter().position(|t| t.loc.path() == Some(dir)) {
                    self.tabs[i].restore = Some(Restore { select: vec![name.clone()], focus: Some(name), scroll: None, rename: false });
                    self.load(i);
                }
            }
            // archive protégée : la boîte de dialogue demande le mot de passe, puis l'extraction reprend
            (Err(Failure::NeedPassword), Some((archive, to_folder))) => self.ask_password(archive, to_folder, false),
            (Err(Failure::WrongPassword), Some((archive, to_folder))) => self.ask_password(archive, to_folder, true),
            (Err(Failure::NeedWinRar), _) => self.toast(self.s.need_winrar.to_string()),
            (Err(Failure::Message(message)), _) => self.toast(message),
            (Err(_), None) => self.toast(self.s.pw_wrong.to_string()),
        }
        self.win.repaint();
    }

    /// Extrait une archive à côté d'elle : dans un dossier à son nom, ou directement (sans rien écraser).
    pub(super) fn extract(&mut self, archive: PathBuf, to_folder: bool, password: Option<String>) {
        let Some(dir) = archive.parent().map(Path::to_path_buf) else { return };
        let name = leaf(&archive);
        let label = self.s.fill(self.s.extracting, &[&name]);
        let folder = ops::free_name(&dir, archive::stem(&name));
        let (d, a) = (dir.clone(), archive.clone());
        self.job(label, dir, Some((archive, to_folder)), move || {
            if to_folder {
                archive::extract_new(&a, &d.join(&folder), password.as_deref()).map(|_| folder)
            } else {
                archive::extract_here(&a, &d, password.as_deref()).map(|_| name)
            }
        });
    }

    /// Les éléments à compresser (ceux du même dossier que le premier), leur dossier, et un nom d'archive libre.
    fn archive_plan(paths: &[PathBuf], ext: &str) -> Option<(PathBuf, Vec<String>, String)> {
        let dir = paths.first()?.parent()?.to_path_buf();
        let names: Vec<String> = paths.iter().filter(|p| p.parent() == Some(dir.as_path())).map(|p| leaf(p)).collect();
        let base = if names.len() == 1 { archive::stem(&names[0]).to_string() } else { leaf(&dir) };
        let mut out = format!("{base}.{ext}");
        let mut n = 2;
        while dir.join(&out).exists() {
            out = format!("{base} ({n}).{ext}");
            n += 1;
        }
        Some((dir, names, out))
    }

    /// Compresse des éléments d'un même dossier dans un fichier ZIP à côté d'eux.
    pub(super) fn compress(&mut self, paths: Vec<PathBuf>) {
        let Some((dir, names, out)) = Self::archive_plan(&paths, "zip") else { return };
        let label = self.s.fill(self.s.compressing, &[&out]);
        let d = dir.clone();
        self.job(label, dir, None, move || archive::compress(&d.join(&out), &d, &names).map(|_| out).map_err(Failure::Message));
    }

    /// Compresse des éléments d'un même dossier dans une archive protégée par mot de passe.
    pub(super) fn compress_locked(&mut self, paths: Vec<PathBuf>, password: String, format: Format) {
        let ext = if format == Format::Rar { "rar" } else { "zip" };
        let Some((dir, names, out)) = Self::archive_plan(&paths, ext) else { return };
        let label = self.s.fill(self.s.compressing, &[&out]);
        let d = dir.clone();
        self.job(label, dir, None, move || archive::compress_locked(&d.join(&out), &d, &names, &password, format).map(|_| out).map_err(Failure::Message));
    }

    // ───────────────────────── renommer ─────────────────────────

    /// Ouvre le champ de saisie sur le nom de l'élément sélectionné.
    pub fn start_rename(&mut self) {
        self.end_edit(true);
        let tab = self.tab();
        if tab.stale || tab.loc == Location::ThisPc {
            return;
        }
        let row = tab.focus.filter(|&f| f < tab.rows() && tab.item(f).sel).or_else(|| tab.selected_rows().first().copied());
        let Some(row) = row else { return };
        self.reveal(row);
        let (_, x, w) = self.columns()[0];
        let l = self.lay.list;
        let tab = self.tab();
        let it = tab.item(row);
        let name = it.name.clone();
        // la sélection couvre le nom sans l'extension, comme dans l'Explorateur
        let ext = it.ext().len();
        let stem = if ext > 0 { name.len() - ext - 1 } else { name.len() };
        let (text, select) = if self.set.hide_ext && ext > 0 {
            (name[..stem].to_string(), name[..stem].encode_utf16().count())
        } else {
            (name.clone(), name[..stem].encode_utf16().count())
        };
        let y = l.y + LIST_TOP + row as f32 * ROW_H - tab.scroll;
        let r = rect(x + 30.0, y + 3.0, (w - 40.0).max(40.0), ROW_H - 6.0);
        self.edit = Some(EditKind::Rename { tab: tab.id, name });
        self.edit_rect = r;
        self.win.edit_show(r, &text, &self.edit_style(self.theme.solid, Some((0, select)), ""));
        self.win.repaint();
    }

    pub(super) fn commit_rename(&mut self, tab_id: u64, old: &str, typed: &str) {
        let Some(i) = self.tabs.iter().position(|t| t.id == tab_id) else { return };
        let tab = &self.tabs[i];
        let Some(row) = tab.row_of(old) else { return };
        let it = tab.item(row);
        let mut new = typed.trim().to_string();
        // extension masquée : elle n'était pas dans le champ, on la remet
        if self.set.hide_ext && !it.ext().is_empty() {
            new = format!("{new}.{}", it.ext());
        }
        if typed.trim().is_empty() || new == old {
            return;
        }
        if !ops::valid_name(&new) {
            self.toast(self.s.fill(self.s.invalid_name, &[ops::FORBIDDEN]));
            return;
        }
        if matches!(tab.loc, Location::Remote { .. }) {
            return self.remote_rename(old, &new);
        }
        let Some(path) = tab.path_of(row) else { return };
        let hwnd = self.win.hwnd.0 as isize;
        self.later(move |win| {
            let ok = ops::rename(windows::Win32::Foundation::HWND(hwnd as _), &path, &new);
            with_app(win, |app, _| {
                let Some(i) = app.tabs.iter().position(|t| t.id == tab_id) else { return };
                if ok {
                    let tab = &mut app.tabs[i];
                    tab.restore = Some(Restore { select: vec![new.clone()], focus: Some(new), scroll: Some(tab.target), rename: false });
                    app.load(i);
                }
            });
        });
    }

    // ───────────────────────── clavier ─────────────────────────

    /// Touches des opérations sur les fichiers. Renvoie vrai si la touche a été comprise.
    pub(super) fn key_more(&mut self, vk: u16, mods: Mods, cx: &mut Cx) -> bool {
        let Mods { ctrl, shift, alt } = mods;
        let letter = |c: char| vk == c as u16;
        // sur un serveur : supprimer demande confirmation (pas de corbeille), coller envoie des fichiers
        if matches!(self.tab().loc, Location::Remote { .. }) {
            match vk {
                K_F2 => self.start_rename(),
                K_DELETE => self.remote_ask_delete(),
                _ if letter('V') && ctrl => self.remote_paste(),
                _ if letter('N') && ctrl && shift => self.remote_new_folder(),
                K_APPS => self.keyboard_menu(cx),
                K_F10 if shift => self.keyboard_menu(cx),
                // copier, couper, propriétés : rien de tel ici pour l'instant
                _ if (letter('C') || letter('X')) && ctrl => {}
                K_RETURN if alt => {}
                _ => return false,
            }
            cx.repaint();
            return true;
        }
        match vk {
            K_F2 => self.start_rename(),
            K_DELETE => self.verb("delete", shift),
            _ if letter('C') && ctrl && shift => {
                let paths = self.tab().selected_paths();
                self.copy_paths(&paths);
            }
            _ if letter('C') && ctrl => self.verb("copy", false),
            _ if letter('X') && ctrl => self.verb("cut", false),
            _ if letter('V') && ctrl => self.paste(),
            _ if letter('N') && ctrl && shift => self.new_folder(),
            K_RETURN if alt => {
                if self.tab().sel_count > 0 {
                    self.verb("properties", false);
                } else {
                    self.command(cmd::PROPERTIES, cx);
                }
            }
            K_APPS => self.keyboard_menu(cx),
            K_F10 if shift => self.keyboard_menu(cx),
            _ => return false,
        }
        cx.repaint();
        true
    }

    /// Touche Menu : le menu contextuel s'ouvre sur la ligne active.
    fn keyboard_menu(&mut self, cx: &mut Cx) {
        let l = self.lay.list;
        let tab = self.tab();
        match tab.focus.filter(|&f| f < tab.rows() && tab.item(f).sel) {
            Some(row) => {
                self.reveal(row);
                let y = l.y + LIST_TOP + row as f32 * ROW_H - self.tab().scroll + ROW_H;
                let at = self.win.to_screen(crate::ui::pt(l.x + 44.0, y.clamp(l.y, l.bottom())));
                self.row_menu(row, at, false, cx);
            }
            None => {
                let at = self.win.to_screen(crate::ui::pt(l.x + 24.0, l.y + 24.0));
                self.background_menu(at, cx);
            }
        }
    }
}
