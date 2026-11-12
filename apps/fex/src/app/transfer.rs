//! Glisser des fichiers à l'intérieur de la fenêtre : de la liste ou d'un mini-panneau vers un dossier (une ligne,
//! le fond de la liste, un mini-panneau, la barre latérale, un onglet, le fil d'Ariane) ou vers une catégorie.
//! Tant que la souris reste dans la fenêtre, Plume FEx fait le travail lui-même, par le shell de Windows (mêmes
//! dialogues que l'Explorateur), et Ctrl+Z le défait. Si elle sort de la fenêtre, le glisser passe à Windows.
//! Comme dans l'Explorateur : sur le même disque on déplace, sinon on copie ; Ctrl copie, Maj déplace.

use super::{App, Drag, Id, NavKind, actions::with_app, model::Location, undo::Undo};
use crate::{
    shell,
    ui::{Align, Canvas, Cx, Font, Mods, Pt, Theme, icons, rect},
};
use std::path::{Component, Path, PathBuf};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{CLSCTX_ALL, CoCreateInstance, IBindCtx},
        UI::Shell::{
            FOF_ALLOWUNDO, FOFX_SHOWELEVATIONPROMPT, FileOperation, IFileOperation, IFileOperationProgressSink, IShellItem, SHCreateItemFromParsingName,
        },
    },
    core::PCWSTR,
};

/// Ce qui recevrait les fichiers lâchés.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Dir(PathBuf),
    /// Une catégorie de la barre latérale : les dossiers y sont ajoutés (rien ne bouge sur le disque).
    Category(usize),
    /// L'onglet montre un serveur : les fichiers y sont envoyés.
    Remote,
}

/// Le lecteur (ou le partage réseau) d'un chemin.
fn volume(p: &Path) -> Option<String> {
    match p.components().next()? {
        Component::Prefix(x) => Some(x.as_os_str().to_string_lossy().to_lowercase()),
        _ => None,
    }
}

/// Copier plutôt que déplacer ? Ctrl : copier ; Maj : déplacer ; sinon on déplace sur le même disque.
pub fn wants_copy(paths: &[PathBuf], dir: &Path, mods: Mods) -> bool {
    if mods.ctrl {
        return true;
    }
    if mods.shift {
        return false;
    }
    let to = volume(dir);
    paths.iter().any(|p| volume(p) != to)
}

/// Copie des éléments dans un dossier, par le shell (dialogues de conflit, Corbeille et annulation de Windows).
fn copy_items(owner: HWND, items: &[PathBuf], dir: &Path) -> bool {
    let item = |p: &Path| -> windows::core::Result<IShellItem> {
        let w = shell::wide(p);
        unsafe { SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None::<&IBindCtx>) }
    };
    unsafe {
        let go = || -> windows::core::Result<bool> {
            let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
            op.SetOperationFlags(FOF_ALLOWUNDO | FOFX_SHOWELEVATIONPROMPT)?;
            op.SetOwnerWindow(owner)?;
            let to = item(dir)?;
            for p in items {
                op.CopyItem(&item(p)?, &to, PCWSTR::null(), None::<&IFileOperationProgressSink>)?;
            }
            op.PerformOperations()?;
            Ok(!op.GetAnyOperationsAborted()?.as_bool())
        };
        go().unwrap_or(false)
    }
}

impl App {
    /// Le glisser de fichiers commence (la souris a bougé, bouton enfoncé).
    pub(super) fn start_files_drag(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        self.panel_close_on_up = false;
        self.pressed = None;
        self.drag = Drag::Files { paths, target: None, copy: false };
    }

    /// Ce qui recevrait les fichiers `dragged` lâchés au point `p` : la zone à mettre en évidence et la cible.
    /// `dragged` vide : un glisser venu d'ailleurs (Windows), seuls les dossiers comptent.
    pub(super) fn target_at(&self, p: Pt, dragged: &[PathBuf]) -> Option<(Id, Target)> {
        let tab = self.tab();
        let here = |hl: Id| match &tab.loc {
            Location::Path(d) => Some((hl, Target::Dir(d.clone()))),
            Location::Remote { path, .. } if !path.is_empty() => Some((hl, Target::Remote)),
            _ => None,
        };
        let id = self.hit(p)?;
        let found = match id {
            Id::Row(r) if tab.item(r).is_dir() && !matches!(tab.loc, Location::Remote { .. }) => {
                let path = tab.path_of(r)?;
                if dragged.contains(&path) { here(Id::List) } else { Some((id, Target::Dir(path))) }
            }
            Id::Row(_) | Id::List => here(Id::List),
            Id::Nav(i) => {
                let row = self.nav_rows.get(i)?;
                match row.kind {
                    NavKind::CatHeader(c) => Some((id, Target::Category(c))),
                    _ if row.dim => None,
                    _ => row.path().map(|q| (id, Target::Dir(q.to_path_buf()))),
                }
            }
            Id::Tab(i) => self.tabs.get(i).and_then(|t| t.loc.path()).map(|q| (id, Target::Dir(q.to_path_buf()))),
            Id::Crumb(i) => self.crumbs.get(i).and_then(|l| l.path()).map(|q| (id, Target::Dir(q.to_path_buf()))),
            Id::PanelRow(pid, r) => {
                let panel = self.panel(pid)?;
                let it = panel.items.get(r)?;
                let path = panel.dir.join(&it.name);
                if it.is_dir() && !dragged.contains(&path) { Some((id, Target::Dir(path))) } else { Some((Id::Panel(pid), Target::Dir(panel.dir.clone()))) }
            }
            Id::PanelCrumb(pid, c) => {
                let panel = self.panel(pid)?;
                Location::Path(panel.dir.clone()).chain().get(c).map(|q| (id, Target::Dir(q.clone())))
            }
            Id::Panel(pid) | Id::PanelHead(pid) | Id::PanelClose(pid) => self.panel(pid).map(|panel| (Id::Panel(pid), Target::Dir(panel.dir.clone()))),
            _ => None,
        }?;
        // déjà là (lâché dans son propre dossier), ou un dossier dans lui-même : rien à faire ici
        if let (_, Target::Dir(d)) = &found
            && !dragged.is_empty()
            && (dragged.iter().all(|q| q.parent() == Some(d.as_path())) || dragged.iter().any(|q| d.starts_with(q)))
        {
            return None;
        }
        Some(found)
    }

    /// La souris bouge pendant un glisser de fichiers.
    pub(super) fn files_move(&mut self, p: Pt, mods: Mods, cx: &mut Cx) {
        let Drag::Files { paths, .. } = &self.drag else { return };
        // sortie de la fenêtre : Windows prend le relais (vers l'Explorateur, le Bureau, une autre application)
        if p.x < 0.0 || p.y < 0.0 || p.x >= self.lay.w || p.y >= self.lay.status.bottom() {
            let paths = paths.clone();
            self.drag = Drag::None;
            self.drop_hl = None;
            cx.capture(false);
            cx.after(move |win| {
                shell::dnd::drag(win, &paths);
                with_app(win, |app, _| {
                    app.drop_at(None);
                    app.check_now();
                    app.refresh_panels();
                });
            });
            cx.repaint();
            return;
        }
        let found = self.target_at(p, paths);
        let copy = match &found {
            Some((_, Target::Dir(d))) => wants_copy(paths, d, mods),
            _ => false,
        };
        let hl = found.as_ref().map(|f| f.0);
        if let Drag::Files { target, copy: c, .. } = &mut self.drag {
            *target = found.map(|f| f.1);
            *c = copy;
        }
        self.drop_hl = hl;
        cx.repaint();
    }

    /// Le bouton est relâché : les fichiers vont où la souris les a menés.
    pub(super) fn files_drop(&mut self, p: Pt, mods: Mods, paths: Vec<PathBuf>) {
        self.drop_hl = None;
        let Some((_, target)) = self.target_at(p, &paths) else { return };
        match target {
            Target::Dir(dir) => {
                let copy = wants_copy(&paths, &dir, mods);
                self.transfer(paths, dir, copy);
            }
            Target::Category(c) => self.add_to_category(c, &paths),
            Target::Remote => self.remote_upload(paths),
        }
    }

    /// Déplace ou copie des éléments dans un dossier ; ce qui a été fait se défait avec Ctrl+Z.
    pub(super) fn transfer(&mut self, paths: Vec<PathBuf>, dir: PathBuf, copy: bool) {
        let paths: Vec<PathBuf> = paths.into_iter().filter(|p| p.parent() != Some(dir.as_path()) && !dir.starts_with(p)).collect();
        if paths.is_empty() {
            return;
        }
        // ce qui n'existe pas encore à l'arrivée : c'est ce que Ctrl+Z pourra défaire
        let items: Vec<(PathBuf, PathBuf, bool)> = paths
            .iter()
            .filter_map(|p| {
                let to = dir.join(p.file_name()?);
                let fresh = !to.exists();
                Some((p.clone(), to, fresh))
            })
            .collect();
        self.later(move |win| {
            let owner = win.hwnd;
            if copy {
                copy_items(owner, &paths, &dir);
            } else {
                let moves: Vec<(PathBuf, PathBuf, String)> =
                    paths.iter().filter_map(|p| Some((p.clone(), dir.clone(), p.file_name()?.to_string_lossy().into_owned()))).collect();
                shell::undo::move_items(owner, &moves);
            }
            with_app(win, |app, cx| {
                let done: Vec<(PathBuf, PathBuf)> =
                    items.into_iter().filter(|(from, to, fresh)| *fresh && to.exists() && (copy || !from.exists())).map(|(f, t, _)| (f, t)).collect();
                if !done.is_empty() {
                    app.remember(Undo::Pasted { moved: !copy, items: done });
                }
                app.check_now();
                app.refresh_panels();
                cx.repaint();
            });
        });
    }

    /// L'étiquette qui suit la souris pendant un glisser : ce qui se passera au lâcher.
    pub(super) fn paint_drag_label(&mut self, c: &mut Canvas, t: &Theme) {
        let s = self.s;
        let (text, active) = match &self.drag {
            Drag::Files { paths, target, copy } => match target {
                Some(Target::Dir(d)) => (s.fill(if *copy { s.copy_to } else { s.move_to }, &[&self.label_of(d)]), true),
                Some(Target::Category(ci)) => (self.set.categories.get(*ci).map_or_else(String::new, |cat| s.fill(s.add_to, &[&cat.name])), true),
                Some(Target::Remote) => (s.fill(s.copy_to, &[&self.title_of(&self.tab().loc)]), true),
                None => (s.count(paths.len() as u64, s.item), false),
            },
            Drag::NavMove { row, .. } => (self.nav_label(*row), true),
            _ => return,
        };
        if text.is_empty() {
            return;
        }
        let tw = c.text_width(&text, Font::Small).min(320.0);
        let r = rect(self.mouse.x + 16.0, self.mouse.y + 14.0, tw + 40.0, 28.0);
        c.fill_round(r, 14.0, t.solid2);
        c.stroke_round(r, 14.0, t.ring, 1.0);
        let ic = if matches!(self.drag, Drag::Files { .. }) { &icons::FILE } else { &icons::FOLDER };
        c.icon(ic, rect(r.x + 11.0, r.y + 7.0, 14.0, 14.0), if active { t.accent } else { t.t3 });
        c.text(&text, rect(r.x + 30.0, r.y, tw + 4.0, r.h), Font::Small, if active { t.t1 } else { t.t2 }, Align::Left);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_on_the_same_drive_copy_across() {
        let none = Mods::default();
        let a = vec![PathBuf::from(r"C:\Users\Sam\a.txt")];
        assert!(!wants_copy(&a, Path::new(r"C:\Temp"), none));
        assert!(wants_copy(&a, Path::new(r"D:\Backup"), none));
        assert!(wants_copy(&a, Path::new(r"c:\temp"), Mods { ctrl: true, ..none }));
        assert!(!wants_copy(&a, Path::new(r"D:\Backup"), Mods { shift: true, ..none }));
        // la casse de la lettre du lecteur ne compte pas
        assert!(!wants_copy(&a, Path::new(r"c:\Temp"), none));
    }
}
