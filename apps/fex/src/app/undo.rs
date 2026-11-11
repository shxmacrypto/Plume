//! Annuler (Ctrl+Z) : ce que Plume FEx a fait, le plus récent d'abord. Renommer, créer un dossier, coller (copier ou
//! déplacer), supprimer vers la Corbeille. Une suppression définitive (Maj+Suppr) ne s'annule pas.

use super::App;
use crate::shell::{ops, undo};
use std::path::PathBuf;
use windows::Win32::Foundation::HWND;

/// Ce qu'il faut pour défaire une opération.
pub enum Undo {
    /// L'élément s'appelle maintenant `now` ; il s'appelait `old_name`.
    Rename {
        now: PathBuf,
        old_name: String,
    },
    NewFolder(PathBuf),
    /// Un collage : (d'où venait l'élément, où il a été collé). `moved` : couper-coller (il a quitté sa place).
    Pasted {
        moved: bool,
        items: Vec<(PathBuf, PathBuf)>,
    },
    /// Les chemins d'origine d'éléments envoyés à la Corbeille.
    Recycled(Vec<PathBuf>),
}

/// Pas plus de cinquante opérations en mémoire.
const KEEP: usize = 50;

impl App {
    pub(super) fn remember(&mut self, u: Undo) {
        self.undo.push(u);
        if self.undo.len() > KEEP {
            self.undo.remove(0);
        }
    }

    /// Défait la dernière opération.
    pub(super) fn undo_last(&mut self) {
        let Some(u) = self.undo.pop() else {
            self.toast(self.s.nothing_to_undo.to_string());
            return;
        };
        let hwnd = self.win.hwnd.0 as isize;
        self.later(move |win| {
            let owner = HWND(hwnd as _);
            let ok = match u {
                Undo::Rename { now, old_name } => now.exists() && ops::rename(owner, &now, &old_name),
                // seulement s'il est encore vide : rien de ce qu'on y a mis depuis ne disparaît
                Undo::NewFolder(dir) => std::fs::remove_dir(&dir).is_ok(),
                Undo::Pasted { moved: false, items } => {
                    let made: Vec<PathBuf> = items.into_iter().map(|(_, to)| to).filter(|p| p.exists()).collect();
                    !made.is_empty() && undo::recycle(owner, &made)
                }
                Undo::Pasted { moved: true, items } => {
                    let back: Vec<(PathBuf, PathBuf, String)> = items
                        .into_iter()
                        .filter(|(from, to)| to.exists() && !from.exists())
                        .filter_map(|(from, to)| Some((to, from.parent()?.to_path_buf(), from.file_name()?.to_string_lossy().into_owned())))
                        .collect();
                    !back.is_empty() && undo::move_items(owner, &back)
                }
                Undo::Recycled(paths) => {
                    let n = paths.len();
                    undo::restore(&paths).len() == n
                }
            };
            super::actions::with_app(win, |app, _| {
                let s = app.s;
                app.toast(if ok { s.undone } else { s.undo_failed }.to_string());
                app.check_now();
            });
        });
    }
}
