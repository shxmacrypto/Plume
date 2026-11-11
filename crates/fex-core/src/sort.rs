//! Tri « comme l'Explorateur » : les noms sont comparés selon la langue de l'utilisateur, sans tenir compte
//! de la casse, et les nombres comptent comme des nombres (« photo2 » avant « photo10 »).
//! Windows fournit une clé de tri par nom (`LCMapStringEx`) : on la calcule une fois, puis on compare des octets.

use crate::listing::Entry;
use std::cmp::Ordering;
use windows::{
    Win32::{
        Foundation::LPARAM,
        Globalization::{LCMAP_SORTKEY, LCMapStringEx, LINGUISTIC_IGNORECASE, SORT_DIGITSASNUMBERS},
    },
    core::PCWSTR,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    Created,
    /// Par extension, puis par nom.
    Type,
    /// Résultats de recherche : par dossier (dans un dossier, tous les éléments ont le même : c'est le tri par nom).
    Path,
}

/// Fabrique les clés de tri de noms en réutilisant ses tampons.
#[derive(Default)]
pub struct NameKeys {
    src: Vec<u16>,
    dst: Vec<u16>,
}

impl NameKeys {
    pub fn new() -> Self {
        NameKeys { src: Vec::with_capacity(260), dst: vec![0; 1024] }
    }

    /// Clé de tri de `name` : deux clés se comparent octet par octet.
    pub fn key(&mut self, name: &str) -> Box<[u8]> {
        let mut out = Vec::new();
        self.key_into(name, &mut out);
        out.into_boxed_slice()
    }

    /// Ajoute la clé de tri de `name` au bout de `out` (pour ranger beaucoup de clés dans une seule zone).
    pub fn key_into(&mut self, name: &str, out: &mut Vec<u8>) {
        self.src.clear();
        self.src.extend(name.encode_utf16());
        if self.src.is_empty() {
            return;
        }
        let flags = LCMAP_SORTKEY | LINGUISTIC_IGNORECASE.0 | SORT_DIGITSASNUMBERS.0;
        for _ in 0..2 {
            // pour une clé de tri, la taille du tampon de sortie se compte en octets
            let n = unsafe { LCMapStringEx(PCWSTR::null(), flags, &self.src, Some(&mut self.dst[..]), None, None, LPARAM(0)) };
            if n > 0 {
                let bytes = unsafe { std::slice::from_raw_parts(self.dst.as_ptr() as *const u8, n as usize) };
                out.extend_from_slice(bytes);
                return;
            }
            // tampon trop petit : Windows donne la taille nécessaire
            let need = unsafe { LCMapStringEx(PCWSTR::null(), flags, &self.src, None, None, None, LPARAM(0)) };
            if need <= 0 {
                break;
            }
            self.dst.resize(need as usize, 0);
        }
        // secours (ne devrait pas arriver) : minuscules, octet par octet
        out.extend_from_slice(name.to_lowercase().as_bytes());
    }
}

/// Ce que le tri lit d'un élément.
pub struct Fields<'a> {
    pub name: &'a str,
    /// Extension sans le point (vide pour un dossier).
    pub ext: &'a str,
    pub dir: bool,
    pub size: u64,
    pub mtime: u64,
    pub ctime: u64,
}

/// Trie n'importe quelle liste d'éléments décrits par `fields`. Les dossiers passent avant les fichiers quand
/// `dirs_first` est vrai ; un tri décroissant inverse toute la liste, dossiers compris, comme dans l'Explorateur.
/// À égalité, le nom départage.
pub fn sort_by<T>(items: &mut Vec<T>, fields: impl Fn(&T) -> Fields<'_>, key: SortKey, ascending: bool, dirs_first: bool) {
    let mut keys = NameKeys::new();
    // (clé du nom, clé de l'extension, élément)
    let mut rows: Vec<_> = std::mem::take(items)
        .into_iter()
        .map(|e| {
            let f = fields(&e);
            let name = keys.key(f.name);
            let ext = if key == SortKey::Type { keys.key(f.ext) } else { Box::default() };
            (name, ext, e)
        })
        .collect();
    rows.sort_unstable_by(|a, b| {
        let (x, y) = (fields(&a.2), fields(&b.2));
        let group = if dirs_first { y.dir.cmp(&x.dir) } else { Ordering::Equal };
        let main = match key {
            SortKey::Name | SortKey::Path => Ordering::Equal,
            SortKey::Size => x.size.cmp(&y.size),
            SortKey::Modified => x.mtime.cmp(&y.mtime),
            SortKey::Created => x.ctime.cmp(&y.ctime),
            SortKey::Type => a.1.cmp(&b.1),
        };
        let o = group.then(main).then_with(|| a.0.cmp(&b.0));
        if ascending { o } else { o.reverse() }
    });
    *items = rows.into_iter().map(|r| r.2).collect();
}

/// Trie des entrées de dossier (voir [`sort_by`]).
pub fn sort_entries(items: &mut Vec<Entry>, key: SortKey, ascending: bool, dirs_first: bool) {
    sort_by(items, |e| Fields { name: &e.name, ext: e.ext(), dir: e.is_dir(), size: e.size, mtime: e.mtime, ctime: e.ctime }, key, ascending, dirs_first);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::ATTR_DIRECTORY;

    fn file(name: &str, size: u64, mtime: u64) -> Entry {
        Entry { name: name.into(), size, mtime, ..Default::default() }
    }
    fn dir(name: &str) -> Entry {
        Entry { name: name.into(), attrs: ATTR_DIRECTORY, ..Default::default() }
    }
    fn names(l: &[Entry]) -> Vec<&str> {
        l.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn natural_order_like_explorer() {
        let mut l = vec![file("photo10.jpg", 1, 1), file("Photo2.jpg", 1, 1), file("photo1.jpg", 1, 1), file("b.txt", 1, 1), file("A.txt", 1, 1)];
        sort_entries(&mut l, SortKey::Name, true, true);
        assert_eq!(names(&l), ["A.txt", "b.txt", "photo1.jpg", "Photo2.jpg", "photo10.jpg"]);
    }

    #[test]
    fn accents_sort_next_to_their_letter() {
        let mut l = vec![file("zebre", 0, 0), file("école", 0, 0), file("eau", 0, 0), file("fin", 0, 0)];
        sort_entries(&mut l, SortKey::Name, true, true);
        assert_eq!(names(&l), ["eau", "école", "fin", "zebre"]);
    }

    #[test]
    fn folders_first_then_reversed() {
        let mut l = vec![file("b", 5, 2), dir("Zed"), file("a", 9, 1), dir("alpha")];
        sort_entries(&mut l, SortKey::Name, true, true);
        assert_eq!(names(&l), ["alpha", "Zed", "a", "b"]);
        sort_entries(&mut l, SortKey::Name, false, true);
        assert_eq!(names(&l), ["b", "a", "Zed", "alpha"], "décroissant : toute la liste s'inverse");
        sort_entries(&mut l, SortKey::Name, true, false);
        assert_eq!(names(&l), ["a", "alpha", "b", "Zed"]);
    }

    #[test]
    fn by_size_date_and_type() {
        let mut l = vec![file("b.txt", 5, 30), file("a.zip", 9, 10), file("c.txt", 1, 20), dir("d")];
        sort_entries(&mut l, SortKey::Size, true, true);
        assert_eq!(names(&l), ["d", "c.txt", "b.txt", "a.zip"]);
        sort_entries(&mut l, SortKey::Modified, false, true);
        assert_eq!(names(&l), ["b.txt", "c.txt", "a.zip", "d"]);
        sort_entries(&mut l, SortKey::Type, true, true);
        assert_eq!(names(&l), ["d", "b.txt", "c.txt", "a.zip"]);
    }
}
