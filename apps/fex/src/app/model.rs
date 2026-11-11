//! L'état d'un onglet : où l'on est, ce que le dossier contient, ce qui est sélectionné.

use crate::shell::places::Drive;
use fex_core::{
    SortKey,
    listing::{ATTR_DIRECTORY, ATTR_HIDDEN, ATTR_SYSTEM, RawEntry},
    sort::Fields,
};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    /// La liste des lecteurs.
    ThisPc,
    Path(PathBuf),
    /// Résultats d'une recherche lancée depuis `from` (None : tout le PC).
    Search {
        query: String,
        from: Option<PathBuf>,
    },
    /// Un dossier d'un serveur SFTP : le serveur (« utilisateur@hôte », suivi de « :port » s'il n'est pas
    /// l'habituel) et le chemin sur ce serveur (vide : le dossier d'accueil, connu une fois connecté).
    Remote {
        server: String,
        path: String,
    },
}

impl Location {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Location::Path(p) => Some(p),
            _ => None,
        }
    }

    /// Le dossier parent ; les racines des lecteurs ont « Ce PC » pour parent.
    pub fn parent(&self) -> Option<Location> {
        match self {
            Location::ThisPc => None,
            Location::Path(p) => Some(p.parent().map_or(Location::ThisPc, |q| Location::Path(q.to_path_buf()))),
            Location::Search { from, .. } => Some(from.clone().map_or(Location::ThisPc, Location::Path)),
            // au-dessus de la racine d'un serveur : « Ce PC », comme pour un lecteur
            Location::Remote { server, path } => Some(match path.trim_end_matches('/').rsplit_once('/') {
                Some((up, _)) => Location::Remote { server: server.clone(), path: if up.is_empty() { "/".into() } else { up.into() } },
                None => Location::ThisPc,
            }),
        }
    }

    /// L'adresse d'un dossier de serveur, telle qu'on la lit et qu'on la tape : « sftp://sam@hôte/dossier ».
    pub fn remote_url(server: &str, path: &str) -> String {
        format!("sftp://{server}{path}")
    }

    /// « sftp://utilisateur@hôte », avec ou sans port, avec ou sans chemin.
    pub fn parse_remote(text: &str) -> Option<Location> {
        let text = text.trim();
        let rest = text.get(..7).filter(|p| p.eq_ignore_ascii_case("sftp://")).map(|_| &text[7..])?;
        let (server, path) = match rest.find('/') {
            Some(i) => rest.split_at(i),
            None => (rest, ""),
        };
        let server = crate::shell::sftp::Server::parse(server)?.label();
        // pas de barre finale, sauf pour la racine
        let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
        Some(Location::Remote { server, path: path.to_string() })
    }

    /// Du lecteur jusqu'à ce dossier : `C:\`, `C:\Users`, `C:\Users\samue`.
    pub fn chain(&self) -> Vec<PathBuf> {
        match self {
            Location::Path(p) => {
                let mut v: Vec<PathBuf> = p.ancestors().map(Path::to_path_buf).collect();
                v.reverse();
                v
            }
            _ => vec![],
        }
    }
}

/// Dernier élément d'un chemin (« samue »), ou la racine sans barre finale (« C: », « \\serveur\partage »).
pub fn leaf(p: &Path) -> String {
    match p.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => p.display().to_string().trim_end_matches(['\\', '/']).to_string(),
    }
}

fn expand_env(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(a) = rest.find('%') {
        out.push_str(&rest[..a]);
        let after = &rest[a + 1..];
        match after.find('%') {
            Some(b) if b > 0 => match std::env::var(&after[..b]) {
                Ok(v) => {
                    out.push_str(&v);
                    rest = &after[b + 1..];
                }
                Err(_) => {
                    out.push('%');
                    rest = after;
                }
            },
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Comprend une adresse tapée par l'utilisateur : guillemets, `%VARIABLES%`, `~`, chemins relatifs au dossier
/// ouvert (`base`), barres obliques, « .. ». Le chemin renvoyé est absolu et normalisé ; il n'existe pas forcément.
pub fn parse_address(text: &str, base: Option<&Path>) -> Option<PathBuf> {
    let t = text.trim().trim_matches('"').trim();
    if t.is_empty() {
        return None;
    }
    let mut t = expand_env(t);
    if let Some(rest) = t.strip_prefix('~')
        && (rest.is_empty() || rest.starts_with(['\\', '/']))
        && let Ok(home) = std::env::var("USERPROFILE")
    {
        t = format!("{home}{rest}");
    }
    // « C: » seul désigne la racine du lecteur, pas son dossier courant
    if t.len() == 2 && t.ends_with(':') {
        t.push('\\');
    }
    let mut p = PathBuf::from(t);
    if p.is_relative() {
        p = base?.join(p);
    }
    let abs = std::path::absolute(&p).ok()?;
    // la recomposition retire les barres finales et les « . »
    Some(abs.components().collect())
}

pub struct Item {
    pub name: String,
    pub attrs: u32,
    pub size: u64,
    /// Dossier dont `size` est la taille de tout ce qu'il contient, calculée par l'index.
    pub sized: bool,
    pub mtime: u64,
    pub ctime: u64,
    pub sel: bool,
    /// Icône exacte (numéro dans la liste d'images du système) ; -1 : celle de l'extension suffit pour l'instant.
    pub icon: Cell<i32>,
    /// L'icône exacte a déjà été demandée.
    pub asked: Cell<bool>,
    /// Élément de « Ce PC ».
    pub drive: Option<Box<Drive>>,
    /// Résultat de recherche : le dossier qui contient l'élément.
    pub folder: Option<Arc<Path>>,
}

impl Item {
    pub fn from_raw(r: &RawEntry<'_>) -> Item {
        Item {
            name: String::from_utf16_lossy(r.name),
            attrs: r.attrs,
            size: r.size,
            sized: false,
            mtime: r.mtime,
            ctime: r.ctime,
            sel: false,
            icon: Cell::new(-1),
            asked: Cell::new(false),
            drive: None,
            folder: None,
        }
    }

    pub fn from_drive(d: Drive) -> Item {
        Item {
            name: d.label.clone(),
            attrs: ATTR_DIRECTORY,
            size: d.total,
            sized: false,
            mtime: 0,
            ctime: 0,
            sel: false,
            icon: Cell::new(d.icon),
            asked: Cell::new(true),
            drive: Some(Box::new(d)),
            folder: None,
        }
    }

    pub fn is_dir(&self) -> bool {
        self.attrs & ATTR_DIRECTORY != 0
    }
    pub fn is_hidden(&self) -> bool {
        self.attrs & ATTR_HIDDEN != 0
    }
    /// Extension sans le point ; vide pour un dossier ou un nom sans extension.
    pub fn ext(&self) -> &str {
        if self.is_dir() {
            return "";
        }
        match self.name.rfind('.') {
            Some(i) if i > 0 && i + 1 < self.name.len() => &self.name[i + 1..],
            _ => "",
        }
    }
    /// Raccourci : son icône porte la petite flèche.
    pub fn is_shortcut(&self) -> bool {
        let e = self.ext();
        e.eq_ignore_ascii_case("lnk") || e.eq_ignore_ascii_case("url")
    }
    /// Faut-il demander l'icône du fichier lui-même ? (programmes, raccourcis, dossiers personnalisés par desktop.ini)
    pub fn wants_exact_icon(&self) -> bool {
        if self.is_dir() {
            // Windows ne lit desktop.ini que dans les dossiers marqués lecture seule ou système
            self.attrs & (fex_core::listing::ATTR_READONLY | ATTR_SYSTEM) != 0
        } else {
            crate::shell::icons::has_own_icon(self.ext())
        }
    }
}

pub fn sort_items(items: &mut Vec<Item>, key: SortKey, asc: bool) {
    // Les dossiers d'abord, comme dans l'Explorateur. Sauf par taille, quand celle des dossiers est connue :
    // dossiers et fichiers se classent alors ensemble, du plus gros au plus petit.
    let dirs_first = !(key == SortKey::Size && items.iter().any(|it| it.sized));
    fex_core::sort_by(
        items,
        |it| Fields { name: &it.name, ext: it.ext(), dir: it.is_dir(), size: it.size, mtime: it.mtime, ctime: it.ctime },
        key,
        asc,
        dirs_first,
    );
}

/// Un pas de l'historique d'un onglet.
#[derive(Clone, Debug)]
pub struct Hist {
    pub loc: Location,
    pub scroll: f32,
    pub focus: Option<String>,
}

/// Ce qu'il faut retrouver une fois le dossier relu.
#[derive(Clone, Debug, Default)]
pub struct Restore {
    pub select: Vec<String>,
    pub focus: Option<String>,
    pub scroll: Option<f32>,
    /// Ouvrir le renommage sur l'élément `focus` (nouveau dossier).
    pub rename: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Load {
    Ready,
    Loading(Instant),
    Failed(String),
}

pub struct Tab {
    pub id: u64,
    pub loc: Location,
    pub back: Vec<Hist>,
    pub fwd: Vec<Hist>,
    pub items: Vec<Item>,
    /// Éléments affichés quand le filtre est actif (numéros dans `items`) ; None : tous.
    pub shown: Option<Vec<u32>>,
    /// Texte du champ de recherche.
    pub filter: String,
    pub sort: SortKey,
    pub asc: bool,
    pub load: Load,
    /// Les éléments affichés sont encore ceux du lieu précédent (la lecture du nouveau est en cours).
    pub stale: bool,
    /// Numéro de la lecture en cours : une réponse plus ancienne est ignorée.
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    pub scroll: f32,
    pub target: f32,
    /// Ligne qui a le clavier, et ligne de départ d'une sélection étendue.
    pub focus: Option<usize>,
    pub anchor: Option<usize>,
    pub sel_count: usize,
    pub sel_size: u64,
    /// Y a-t-il un dossier dans la sélection ? (sa taille n'est alors pas affichée)
    pub sel_dirs: bool,
    pub restore: Option<Restore>,
    /// Saisie au clavier dans la liste : « doc » saute à « Documents ».
    pub typed: String,
    pub typed_at: Instant,
    /// Durée de la dernière lecture.
    pub took_ms: f32,
    /// Empreinte du contenu affiché, et moment de la dernière vérification.
    pub sig: u64,
    pub checked: Instant,
    /// Recherche : nombre total de résultats (la liste n'en montre que les premiers), et tri des résultats.
    pub total: usize,
    pub search_sort: (SortKey, bool),
    /// Version de l'index quand les résultats affichés ont été calculés.
    pub index_version: u64,
}

impl Tab {
    pub fn new(id: u64, loc: Location) -> Tab {
        Tab {
            id,
            loc,
            back: vec![],
            fwd: vec![],
            items: vec![],
            shown: None,
            filter: String::new(),
            sort: SortKey::Name,
            asc: true,
            load: Load::Ready,
            stale: false,
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            scroll: 0.0,
            target: 0.0,
            focus: None,
            anchor: None,
            sel_count: 0,
            sel_size: 0,
            sel_dirs: false,
            restore: None,
            typed: String::new(),
            typed_at: Instant::now(),
            took_ms: 0.0,
            sig: 0,
            checked: Instant::now(),
            total: 0,
            search_sort: (SortKey::Name, true),
            index_version: 0,
        }
    }

    /// Nombre de lignes affichées.
    pub fn rows(&self) -> usize {
        self.shown.as_ref().map_or(self.items.len(), Vec::len)
    }

    /// Numéro dans `items` de la ligne `row`.
    pub fn idx(&self, row: usize) -> usize {
        self.shown.as_ref().map_or(row, |s| s[row] as usize)
    }

    pub fn item(&self, row: usize) -> &Item {
        &self.items[self.idx(row)]
    }

    pub fn item_mut(&mut self, row: usize) -> &mut Item {
        let i = self.idx(row);
        &mut self.items[i]
    }

    /// Chemin complet de l'élément d'une ligne.
    pub fn path_of(&self, row: usize) -> Option<PathBuf> {
        let it = self.item(row);
        if let Some(d) = &it.drive {
            return Some(d.root.clone());
        }
        if let Some(f) = &it.folder {
            return Some(f.join(&it.name));
        }
        self.loc.path().map(|p| p.join(&it.name))
    }

    pub fn row_of(&self, name: &str) -> Option<usize> {
        (0..self.rows()).find(|&r| self.item(r).name == name).or_else(|| (0..self.rows()).find(|&r| self.item(r).name.eq_ignore_ascii_case(name)))
    }

    pub fn recount(&mut self) {
        let (mut n, mut size, mut dirs) = (0, 0u64, false);
        for it in self.items.iter().filter(|it| it.sel) {
            n += 1;
            size += it.size;
            dirs |= it.is_dir();
        }
        self.sel_count = n;
        self.sel_size = size;
        self.sel_dirs = dirs;
    }

    pub fn clear_sel(&mut self) {
        if self.sel_count > 0 {
            self.items.iter_mut().for_each(|it| it.sel = false);
        }
        self.sel_count = 0;
        self.sel_size = 0;
        self.sel_dirs = false;
    }

    /// Sélectionne cette ligne seule ; elle prend le clavier.
    pub fn select_only(&mut self, row: usize) {
        self.clear_sel();
        if row < self.rows() {
            self.item_mut(row).sel = true;
            self.focus = Some(row);
            self.anchor = Some(row);
        }
        self.recount();
    }

    pub fn toggle(&mut self, row: usize) {
        if row < self.rows() {
            let it = self.item_mut(row);
            it.sel = !it.sel;
            self.focus = Some(row);
            self.anchor = Some(row);
        }
        self.recount();
    }

    /// Sélectionne de `a` à `b` compris. `keep` : le reste de la sélection est conservé.
    pub fn select_range(&mut self, a: usize, b: usize, keep: bool) {
        if !keep {
            self.clear_sel();
        }
        let (lo, hi) = (a.min(b), a.max(b).min(self.rows().saturating_sub(1)));
        for r in lo..=hi {
            if r < self.rows() {
                self.item_mut(r).sel = true;
            }
        }
        self.recount();
    }

    pub fn select_all(&mut self) {
        match &self.shown {
            None => self.items.iter_mut().for_each(|it| it.sel = true),
            Some(s) => {
                for &i in s {
                    self.items[i as usize].sel = true;
                }
            }
        }
        self.recount();
    }

    pub fn selected_rows(&self) -> Vec<usize> {
        (0..self.rows()).filter(|&r| self.item(r).sel).collect()
    }

    pub fn selected_names(&self) -> Vec<String> {
        self.selected_rows().into_iter().map(|r| self.item(r).name.clone()).collect()
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_rows().into_iter().filter_map(|r| self.path_of(r)).collect()
    }

    /// Recalcule les lignes affichées d'après le filtre (recherche dans le dossier ouvert).
    pub fn apply_filter(&mut self) {
        // la même forme que la recherche dans l'index : ni la casse ni les accents ne comptent
        let plain = |s: &str| String::from_utf8_lossy(&fex_core::query::plain(s)).into_owned();
        let needle = plain(self.filter.trim());
        let keep_focus = self.focus.filter(|&r| r < self.rows()).map(|r| self.idx(r));
        let keep_anchor = self.anchor.filter(|&r| r < self.rows()).map(|r| self.idx(r));
        self.shown = if needle.is_empty() || matches!(self.loc, Location::Search { .. }) {
            None
        } else {
            let words: Vec<&str> = needle.split_whitespace().collect();
            Some(
                self.items
                    .iter()
                    .enumerate()
                    .filter(|(_, it)| {
                        let name = plain(&it.name);
                        words.iter().all(|w| name.contains(w))
                    })
                    .map(|(i, _)| i as u32)
                    .collect(),
            )
        };
        // la ligne qui avait le clavier le garde si elle est toujours affichée
        let row_of_idx = |idx: Option<usize>| -> Option<usize> {
            let idx = idx?;
            match &self.shown {
                None => Some(idx),
                Some(s) => s.binary_search(&(idx as u32)).ok(),
            }
        };
        self.focus = row_of_idx(keep_focus);
        self.anchor = row_of_idx(keep_anchor);
        // ce qui n'est plus affiché n'est plus sélectionné
        if let Some(s) = &self.shown {
            let mut visible = vec![false; self.items.len()];
            s.iter().for_each(|&i| visible[i as usize] = true);
            for (i, it) in self.items.iter_mut().enumerate() {
                it.sel &= visible[i];
            }
        }
        self.recount();
    }

    /// Première ligne, à partir de `from` (en bouclant), dont le nom commence par `prefix`.
    pub fn find_prefix(&self, from: usize, prefix: &str) -> Option<usize> {
        let n = self.rows();
        let p = prefix.to_lowercase();
        (0..n).map(|k| (from + k) % n).find(|&r| self.item(r).name.to_lowercase().starts_with(&p))
    }

    /// Trie les éléments en gardant la sélection et la ligne active sur les mêmes fichiers.
    pub fn resort(&mut self) {
        if matches!(self.loc, Location::ThisPc) {
            return;
        }
        let focus = self.focus.filter(|&r| r < self.rows()).map(|r| self.item(r).name.clone());
        let anchor = self.anchor.filter(|&r| r < self.rows()).map(|r| self.item(r).name.clone());
        sort_items(&mut self.items, self.sort, self.asc);
        self.shown = None;
        self.apply_filter();
        self.focus = focus.and_then(|n| self.row_of(&n));
        self.anchor = anchor.and_then(|n| self.row_of(&n));
    }

    /// Ce qu'il faudra retrouver après une relecture du même dossier.
    pub fn snapshot(&self) -> Restore {
        Restore {
            select: self.selected_names(),
            focus: self.focus.filter(|&r| r < self.rows()).map(|r| self.item(r).name.clone()),
            scroll: Some(self.target),
            rename: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, dir: bool, size: u64) -> Item {
        Item {
            name: name.into(),
            attrs: if dir { ATTR_DIRECTORY } else { 0x20 },
            size,
            sized: false,
            mtime: 0,
            ctime: 0,
            sel: false,
            icon: Cell::new(-1),
            asked: Cell::new(false),
            drive: None,
            folder: None,
        }
    }

    fn tab() -> Tab {
        let mut t = Tab::new(1, Location::Path(PathBuf::from(r"C:\Test")));
        t.items = vec![item("Alpha", true, 0), item("beta.txt", false, 10), item("Gamma.TXT", false, 20), item("delta.png", false, 30)];
        t
    }

    #[test]
    fn locations() {
        let l = Location::Path(PathBuf::from(r"C:\Users\samue"));
        assert_eq!(l.parent(), Some(Location::Path(PathBuf::from(r"C:\Users"))));
        assert_eq!(Location::Path(PathBuf::from(r"C:\")).parent(), Some(Location::ThisPc));
        assert_eq!(Location::ThisPc.parent(), None);
        assert_eq!(l.chain(), vec![PathBuf::from(r"C:\"), PathBuf::from(r"C:\Users"), PathBuf::from(r"C:\Users\samue")]);
        assert_eq!(leaf(Path::new(r"C:\Users\samue")), "samue");
        assert_eq!(leaf(Path::new(r"C:\")), "C:");
        assert_eq!(leaf(Path::new(r"\\srv\share\")), r"\\srv\share");
    }

    #[test]
    fn addresses() {
        let base = Path::new(r"C:\Users\samue");
        assert_eq!(parse_address(r"C:\Windows\", None), Some(PathBuf::from(r"C:\Windows")));
        assert_eq!(parse_address(r#"  "C:\Program Files"  "#, None), Some(PathBuf::from(r"C:\Program Files")));
        assert_eq!(parse_address("c:", None), Some(PathBuf::from(r"c:\")));
        assert_eq!(parse_address("C:/Windows/System32/..", None), Some(PathBuf::from(r"C:\Windows")));
        assert_eq!(parse_address("Documents", Some(base)), Some(PathBuf::from(r"C:\Users\samue\Documents")));
        assert_eq!(parse_address("..", Some(base)), Some(PathBuf::from(r"C:\Users")));
        assert_eq!(parse_address("Documents", None), None);
        assert_eq!(parse_address("   ", Some(base)), None);
        unsafe { std::env::set_var("FEX_TEST_DIR", r"C:\Temp\x") };
        assert_eq!(parse_address(r"%FEX_TEST_DIR%\y", None), Some(PathBuf::from(r"C:\Temp\x\y")));
        assert_eq!(parse_address(r"C:\100%\a", None), Some(PathBuf::from(r"C:\100%\a")));
        assert_eq!(expand_env("%NOPE_NOT_SET%x"), "%NOPE_NOT_SET%x");
    }

    #[test]
    fn selection() {
        let mut t = tab();
        t.select_only(1);
        assert_eq!((t.sel_count, t.sel_size, t.focus, t.anchor), (1, 10, Some(1), Some(1)));
        t.select_range(1, 3, false);
        assert_eq!((t.sel_count, t.sel_size), (3, 60));
        t.toggle(2);
        assert_eq!(t.selected_names(), vec!["beta.txt", "delta.png"]);
        t.select_range(0, 0, true);
        assert_eq!(t.sel_count, 3);
        assert!(t.sel_dirs);
        t.select_all();
        assert_eq!(t.sel_count, 4);
        t.clear_sel();
        assert_eq!(t.sel_count, 0);
        assert_eq!(t.selected_paths(), Vec::<PathBuf>::new());
        t.select_only(3);
        assert_eq!(t.selected_paths(), vec![PathBuf::from(r"C:\Test\delta.png")]);
    }

    #[test]
    fn filter_and_type_ahead() {
        let mut t = tab();
        t.select_all();
        t.focus = Some(2);
        t.filter = "TXT".into();
        t.apply_filter();
        assert_eq!(t.rows(), 2);
        assert_eq!(t.item(0).name, "beta.txt");
        assert_eq!(t.item(1).name, "Gamma.TXT");
        assert_eq!(t.focus, Some(1));
        assert_eq!(t.sel_count, 2);
        t.filter.clear();
        t.apply_filter();
        assert_eq!(t.rows(), 4);
        assert_eq!(t.focus, Some(2));
        assert_eq!(t.find_prefix(0, "g"), Some(2));
        assert_eq!(t.find_prefix(3, "a"), Some(0));
        assert_eq!(t.find_prefix(0, "zz"), None);
        assert_eq!(t.row_of("GAMMA.txt"), Some(2));
    }

    #[test]
    fn resort_keeps_focus_and_selection() {
        let mut t = tab();
        t.select_only(3);
        t.sort = SortKey::Size;
        t.asc = false;
        t.resort();
        assert_eq!(t.item(0).name, "delta.png");
        assert_eq!(t.focus, Some(0));
        assert_eq!(t.selected_names(), vec!["delta.png"]);
        assert_eq!(t.items.last().unwrap().name, "Alpha");
    }

    #[test]
    fn extensions_and_icons() {
        assert_eq!(item("a.TXT", false, 0).ext(), "TXT");
        assert_eq!(item(".gitignore", false, 0).ext(), "");
        assert_eq!(item("folder.d", true, 0).ext(), "");
        assert!(item("x.lnk", false, 0).is_shortcut());
        assert!(item("setup.exe", false, 0).wants_exact_icon());
        assert!(!item("notes.txt", false, 0).wants_exact_icon());
        assert!(!item("plain", true, 0).wants_exact_icon());
    }
}
