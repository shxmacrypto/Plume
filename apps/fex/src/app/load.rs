//! Le travail hors du fil de l'interface : lire un dossier, trier, chercher les icônes exactes.
//! Chaque résultat revient par `Sender` et porte le numéro de sa demande : une réponse dépassée est ignorée.

use super::{
    App,
    model::{Item, Location, sort_items},
};
use crate::{
    shell::{self, places},
    ui::Sender,
};
use fex_core::{
    SortKey,
    listing::{self, ATTR_HIDDEN, ATTR_SYSTEM},
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

#[derive(Clone, Copy, Debug)]
pub struct ListOpts {
    pub show_hidden: bool,
    pub show_system: bool,
    pub sort: SortKey,
    pub asc: bool,
}

#[derive(Debug)]
pub enum LoadError {
    Denied,
    NotFound,
    Other(String),
}

pub struct Loaded {
    pub tab: u64,
    pub generation: u64,
    pub result: Result<Listing, LoadError>,
    pub took_ms: f32,
    /// Vérification de fond : rien ne doit bouger à l'écran si le dossier n'a pas changé.
    pub silent: bool,
}

pub struct Listing {
    pub items: Vec<Item>,
    /// Empreinte du contenu (noms, tailles, dates, attributs) : la même tant que rien ne change.
    pub sig: u64,
    /// Même empreinte que la fois précédente : `items` est vide, il n'y a rien à refaire.
    pub unchanged: bool,
}

/// Les API du shell (icônes, noms affichés) demandent COM sur le fil qui les appelle.
pub fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

/// Faut-il montrer un élément portant ces attributs ?
pub fn visible(attrs: u32, o: &ListOpts) -> bool {
    if attrs & ATTR_HIDDEN == 0 {
        return true;
    }
    // masqué + système : « fichier protégé du système », réglage à part dans Windows
    if attrs & ATTR_SYSTEM != 0 { o.show_hidden && o.show_system } else { o.show_hidden }
}

/// Lit et trie un dossier (ou la liste des lecteurs). `prev` : empreinte de la lecture précédente ;
/// si le contenu n'a pas changé, la liste n'est ni triée ni renvoyée.
pub fn list(loc: &Location, o: &ListOpts, cancel: &AtomicBool, prev: Option<u64>) -> Result<Listing, LoadError> {
    let mut h = DefaultHasher::new();
    let mut items = match loc {
        Location::ThisPc => {
            com_init();
            let drives = places::drives();
            for d in &drives {
                d.root.hash(&mut h);
                d.label.hash(&mut h);
                d.total.hash(&mut h);
                // l'espace libre bouge sans cesse : au mégaoctet près
                (d.free >> 20).hash(&mut h);
            }
            drives.into_iter().map(Item::from_drive).collect()
        }
        Location::Path(p) => {
            let mut items = Vec::new();
            listing::read_dir(p, Some(cancel), &mut |raw| {
                if visible(raw.attrs, o) {
                    raw.name.hash(&mut h);
                    (raw.size, raw.mtime, raw.attrs).hash(&mut h);
                    items.push(Item::from_raw(raw));
                }
            })
            .map_err(|e| match e.kind() {
                io::ErrorKind::PermissionDenied => LoadError::Denied,
                io::ErrorKind::NotFound => LoadError::NotFound,
                _ => LoadError::Other(e.to_string()),
            })?;
            items
        }
        // une recherche se relit par l'index, un serveur par sa session : jamais ici
        Location::Search { .. } | Location::Remote { .. } => vec![],
    };
    let sig = h.finish();
    if prev == Some(sig) {
        return Ok(Listing { items: vec![], sig, unchanged: true });
    }
    if matches!(loc, Location::Path(_)) {
        sort_items(&mut items, o.sort, o.asc);
    }
    Ok(Listing { items, sig, unchanged: false })
}

/// Lance la lecture d'un dossier sur un autre fil ; `App::on_loaded` reçoit le résultat.
/// `silent` : vérification de fond, avec l'empreinte de ce qui est affiché.
pub fn spawn_list(sender: Sender, tab: u64, generation: u64, loc: Location, opts: ListOpts, cancel: Arc<AtomicBool>, silent: Option<u64>) {
    std::thread::spawn(move || {
        let t0 = Instant::now();
        let result = list(&loc, &opts, &cancel, silent);
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let loaded = Loaded { tab, generation, result, took_ms: t0.elapsed().as_secs_f32() * 1000.0, silent: silent.is_some() };
        sender.send(move |h, cx| {
            if let Some(app) = h.as_any().downcast_mut::<App>() {
                app.on_loaded(loaded, cx);
            }
        });
    });
}

/// Demande d'icônes exactes pour quelques fichiers d'un dossier.
pub struct IconJob {
    pub tab: u64,
    pub generation: u64,
    pub paths: Vec<(String, PathBuf)>,
}

/// Démarre le fil des icônes : il traite les demandes une à une, la plus récente d'abord quand elles s'accumulent.
pub fn icon_worker(sender: Sender) -> mpsc::Sender<IconJob> {
    let (tx, rx) = mpsc::channel::<IconJob>();
    std::thread::spawn(move || {
        com_init();
        while let Ok(mut job) = rx.recv() {
            // l'utilisateur a fait défiler la liste entre-temps : seules les dernières lignes demandées comptent
            let mut waiting: Vec<IconJob> = rx.try_iter().collect();
            if let Some(last) = waiting.pop() {
                job = last;
            }
            let found: Vec<(String, i32)> = job.paths.into_iter().filter_map(|(name, path)| shell::icons::exact(&path).map(|i| (name, i.icon))).collect();
            let (tab, generation) = (job.tab, job.generation);
            let ok = sender.send(move |h, cx| {
                if let Some(app) = h.as_any().downcast_mut::<App>() {
                    app.on_icons(tab, generation, found, cx);
                }
            });
            if !ok {
                break;
            }
        }
    });
    tx
}

/// Les lieux de la barre latérale (dossiers personnels, lecteurs), lus hors du fil de l'interface. `check` : les
/// dossiers retenus (récents, catégories) dont on veut savoir s'ils existent encore (un lecteur peut faire attendre).
pub fn spawn_places(sender: Sender, pinned: Vec<PathBuf>, check: Vec<PathBuf>) {
    std::thread::spawn(move || {
        com_init();
        let mut personal = places::personal();
        personal.extend(pinned.iter().filter(|p| p.is_dir()).filter_map(|p| places::place_of(p)));
        let drives = places::drives();
        let gone: Vec<PathBuf> = check.into_iter().filter(|p| !p.is_dir()).collect();
        sender.send(move |h, cx| {
            if let Some(app) = h.as_any().downcast_mut::<App>() {
                app.on_places(personal, drives, gone, cx);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_rules() {
        let o = |h, s| ListOpts { show_hidden: h, show_system: s, sort: SortKey::Name, asc: true };
        assert!(visible(0x20, &o(false, false)));
        assert!(!visible(ATTR_HIDDEN, &o(false, false)));
        assert!(visible(ATTR_HIDDEN, &o(true, false)));
        assert!(!visible(ATTR_HIDDEN | ATTR_SYSTEM, &o(true, false)));
        assert!(visible(ATTR_HIDDEN | ATTR_SYSTEM, &o(true, true)));
        assert!(!visible(ATTR_HIDDEN | ATTR_SYSTEM, &o(false, true)));
        // système sans masqué : visible (comme dans l'Explorateur)
        assert!(visible(ATTR_SYSTEM, &o(false, false)));
    }

    #[test]
    fn lists_a_real_folder_sorted() {
        let dir = std::env::temp_dir().join(format!("fex-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Zeta")).unwrap();
        std::fs::write(dir.join("b10.txt"), b"hello").unwrap();
        std::fs::write(dir.join("b2.txt"), b"hi").unwrap();
        let o = ListOpts { show_hidden: true, show_system: false, sort: SortKey::Name, asc: true };
        let cancel = AtomicBool::new(false);
        let l = list(&Location::Path(dir.clone()), &o, &cancel, None).unwrap();
        let names: Vec<&str> = l.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Zeta", "b2.txt", "b10.txt"]);
        assert_eq!(l.items[2].size, 5);
        assert!(!l.unchanged);
        // même contenu : même empreinte, rien à refaire ; un fichier de plus : tout est relu
        let again = list(&Location::Path(dir.clone()), &o, &cancel, Some(l.sig)).unwrap();
        assert!(again.unchanged && again.items.is_empty() && again.sig == l.sig);
        std::fs::write(dir.join("c.txt"), b"x").unwrap();
        let changed = list(&Location::Path(dir.clone()), &o, &cancel, Some(l.sig)).unwrap();
        assert!(!changed.unchanged && changed.items.len() == 4 && changed.sig != l.sig);
        assert!(matches!(list(&Location::Path(dir.join("nope")), &o, &cancel, None), Err(LoadError::NotFound)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
