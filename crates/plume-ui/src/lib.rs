//! Le kit d'interface natif de Plume : fenêtre sans cadre, dessin Direct2D, texte DirectWrite,
//! thème et icônes de Plume. Il ne sait rien de ce que l'application affiche : Plume FEx et Plume Mail l'utilisent.

pub mod canvas;
pub mod dialogs;
pub mod edit;
pub mod folder;
pub mod geom;
pub mod gfx;
pub mod icons;
pub mod theme;
pub mod window;

pub use canvas::{Align, Canvas};
pub use edit::EditStyle;
pub use geom::{Color, Pt, Rect, pt, rect};
pub use gfx::{Font, Gfx};
pub use theme::{Theme, m};
pub use window::{Area, Button, Cursor, Cx, Event, Handler, Mods, PaintCx, Sender, Window};

/// Zones cliquables de l'image en cours : chaque élément dessiné y inscrit son rectangle et son identifiant.
/// Le dernier inscrit est au-dessus.
pub struct Hits<T> {
    list: Vec<(Rect, T)>,
}

impl<T> Default for Hits<T> {
    fn default() -> Self {
        Hits { list: Vec::new() }
    }
}

impl<T> Hits<T> {
    pub fn clear(&mut self) {
        self.list.clear();
    }
    pub fn add(&mut self, r: Rect, id: T) {
        self.list.push((r, id));
    }
    pub fn at(&self, p: Pt) -> Option<&T> {
        self.list.iter().rev().find(|(r, _)| r.contains(p)).map(|(_, id)| id)
    }
    pub fn iter(&self) -> std::slice::Iter<'_, (Rect, T)> {
        self.list.iter()
    }
    /// Rectangle d'un élément (le plus haut qui correspond).
    pub fn rect_of(&self, f: impl Fn(&T) -> bool) -> Option<Rect> {
        self.list.iter().rev().find(|(_, id)| f(id)).map(|(r, _)| *r)
    }
}
