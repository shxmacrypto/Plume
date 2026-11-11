//! Glisser-déposer avec le reste de Windows. Faire glisser : le shell fournit les données et l'image des fichiers.
//! Recevoir : on dit seulement quel dossier est sous le pointeur, et c'est la cible de dépôt du shell pour ce
//! dossier qui décide (copier, déplacer, raccourci, menu du bouton droit) et fait le travail, comme dans l'Explorateur.

use super::menu::ui_object;
use crate::ui::{Pt, Window};
use std::{cell::RefCell, path::PathBuf};
use windows::{
    Win32::{
        Foundation::POINTL,
        System::{
            Com::IDataObject,
            Ole::{DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl, RegisterDragDrop, RevokeDragDrop},
            SystemServices::MODIFIERKEYS_FLAGS,
        },
    },
    core::{Ref, Result, implement},
};

// Dernier glisser demandé pendant un essai (il n'a pas lieu : voir `drag`).
#[cfg(feature = "test-hook")]
thread_local! { pub static LAST_DRAG: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) }; }

/// Fait glisser des fichiers d'un même dossier. Bloque jusqu'au dépôt ou à l'abandon : à appeler depuis `Cx::after`.
pub fn drag(win: &Window, paths: &[PathBuf]) {
    // Avec la prise d'essai, rien n'est réellement glissé : sans bouton enfoncé, Windows déposerait aussitôt
    // les fichiers sur la fenêtre qui se trouve sous le vrai pointeur de l'utilisateur.
    #[cfg(feature = "test-hook")]
    {
        let _ = win;
        LAST_DRAG.with(|d| *d.borrow_mut() = paths.to_vec());
    }
    #[cfg(not(feature = "test-hook"))]
    unsafe {
        use windows::Win32::{
            System::Ole::{DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, IDropSource},
            UI::Shell::SHDoDragDrop,
        };
        let Some(data) = ui_object::<IDataObject>(win.hwnd, paths) else { return };
        let _ = SHDoDragDrop(Some(win.hwnd), &data, None::<&IDropSource>, DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK);
    }
}

/// `at(Some(point))` : le dossier qui recevrait un dépôt à ce point de la fenêtre (None : pas ici) ; l'application
/// en profite pour le mettre en évidence. `at(None)` : le glisser est fini ou sorti de la fenêtre.
type At = Box<dyn Fn(&Window, Option<Pt>) -> Option<PathBuf>>;

/// `remote(None)` : un dépôt irait-il vers un serveur (l'onglet affiché en montre un) ? `remote(Some(fichiers))` :
/// les y envoyer. Faux : le dépôt suit son cours habituel, vers un dossier de ce PC.
type Remote = Box<dyn Fn(&Window, Option<&[PathBuf]>) -> bool>;

#[implement(IDropTarget)]
struct Target {
    win: Window,
    at: At,
    remote: Remote,
    /// Appelé après un dépôt.
    dropped: Box<dyn Fn(&Window)>,
    data: RefCell<Option<IDataObject>>,
    /// Dossier survolé et sa cible de dépôt du shell.
    current: RefCell<Option<(PathBuf, IDropTarget)>>,
}

impl Target {
    /// Suit le pointeur : change de dossier cible au besoin, puis laisse le shell choisir l'effet.
    fn follow(&self, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        // vers un serveur : une copie, sans passer par le shell
        if (self.remote)(&self.win, None) {
            if let Some((_, old)) = self.current.borrow_mut().take() {
                let _ = unsafe { old.DragLeave() };
            }
            unsafe { *effect = DROPEFFECT_COPY };
            return Ok(());
        }
        let wanted = (self.at)(&self.win, Some(self.win.from_screen(pt.x, pt.y)));
        let mut current = self.current.borrow_mut();
        if current.as_ref().map(|c| &c.0) != wanted.as_ref() {
            if let Some((_, old)) = current.take() {
                let _ = unsafe { old.DragLeave() };
            }
            if let Some(path) = wanted
                && let Some(target) = unsafe { ui_object::<IDropTarget>(self.win.hwnd, std::slice::from_ref(&path)) }
            {
                let data = self.data.borrow().clone();
                let mut allowed = unsafe { *effect };
                if unsafe { target.DragEnter(data.as_ref(), keys, *pt, &mut allowed) }.is_ok() {
                    *current = Some((path, target));
                }
            }
        }
        match current.as_ref() {
            Some((_, target)) => unsafe { target.DragOver(keys, *pt, effect) },
            None => {
                unsafe { *effect = DROPEFFECT_NONE };
                Ok(())
            }
        }
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, data: Ref<'_, IDataObject>, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        *self.data.borrow_mut() = data.cloned();
        self.follow(keys, pt, effect)
    }

    fn DragOver(&self, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        self.follow(keys, pt, effect)
    }

    fn DragLeave(&self) -> Result<()> {
        if let Some((_, target)) = self.current.borrow_mut().take() {
            let _ = unsafe { target.DragLeave() };
        }
        *self.data.borrow_mut() = None;
        (self.at)(&self.win, None);
        Ok(())
    }

    fn Drop(&self, data: Ref<'_, IDataObject>, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        if (self.remote)(&self.win, None) {
            let files = data.as_ref().map(files_of).unwrap_or_default();
            let taken = !files.is_empty() && (self.remote)(&self.win, Some(&files));
            unsafe { *effect = if taken { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
            *self.data.borrow_mut() = None;
            return Ok(());
        }
        let allowed = unsafe { *effect };
        let _ = self.follow(keys, pt, effect);
        let current = self.current.borrow_mut().take();
        let result = match current {
            Some((_, target)) => unsafe {
                *effect = allowed;
                target.Drop(data.as_ref(), keys, *pt, effect)
            },
            None => {
                unsafe { *effect = DROPEFFECT_NONE };
                Ok(())
            }
        };
        *self.data.borrow_mut() = None;
        (self.at)(&self.win, None);
        (self.dropped)(&self.win);
        result
    }
}

/// Les fichiers que porte un glisser-déposer.
fn files_of(data: &IDataObject) -> Vec<PathBuf> {
    use windows::Win32::{
        System::{
            Com::{DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL},
            Ole::{CF_HDROP, ReleaseStgMedium},
        },
        UI::Shell::HDROP,
    };
    let format = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    unsafe {
        let Ok(mut medium) = data.GetData(&format) else { return vec![] };
        let files = super::ops::dropped_files(HDROP(medium.u.hGlobal.0));
        ReleaseStgMedium(&mut medium);
        files
    }
}

/// La fenêtre accepte désormais les fichiers qu'on y dépose.
pub fn register(
    win: Window,
    at: impl Fn(&Window, Option<Pt>) -> Option<PathBuf> + 'static,
    remote: impl Fn(&Window, Option<&[PathBuf]>) -> bool + 'static,
    dropped: impl Fn(&Window) + 'static,
) {
    let target: IDropTarget =
        Target { win, at: Box::new(at), remote: Box::new(remote), dropped: Box::new(dropped), data: RefCell::new(None), current: RefCell::new(None) }.into();
    unsafe {
        let _ = RegisterDragDrop(win.hwnd, &target);
    }
}

pub fn revoke(win: Window) {
    unsafe {
        let _ = RevokeDragDrop(win.hwnd);
    }
}
