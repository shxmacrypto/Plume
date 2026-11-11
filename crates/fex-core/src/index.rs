//! L'index : tous les fichiers et dossiers des disques, en mémoire, dans des tableaux serrés.
//!
//! Les noms sont rangés bout à bout dans une seule zone, `names`. Chaque nom finit par un saut de ligne : une
//! recherche de texte sur toute la zone ne peut donc pas chevaucher deux noms, et elle parcourt des dizaines de
//! mégaoctets en quelques millisecondes. Un élément ne garde que le numéro de son dossier parent ; son chemin se
//! reconstruit à la demande. Tout est compté au plus juste : environ 55 octets par fichier, nom compris.
//!
//! Le premier index se construit en parcourant les dossiers sur plusieurs fils (sans droits d'administrateur).

use crate::{
    listing::{self, ATTR_DIRECTORY, ATTR_HIDDEN, ATTR_OFFLINE, ATTR_READONLY, ATTR_RECALL_ON_DATA_ACCESS, ATTR_REPARSE, ATTR_SYSTEM},
    sort::NameKeys,
    wide::long_path,
};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub const F_DIR: u8 = 1;
pub const F_HIDDEN: u8 = 2;
pub const F_SYSTEM: u8 = 4;
/// Lien symbolique ou jonction : indexé, mais on n'y descend pas.
pub const F_LINK: u8 = 8;
/// Supprimé depuis la construction de l'index (ignoré par la recherche, retiré au prochain compactage).
pub const F_DELETED: u8 = 16;
/// Dossier dont le contenu n'a pas pu être lu (accès refusé) : on ne sait pas ce qu'il contient.
pub const F_DENIED: u8 = 32;
pub const F_READONLY: u8 = 64;
/// Fichier qui n'est pas sur le disque (dans le nuage seulement, par exemple).
pub const F_OFFLINE: u8 = 128;

pub const NO_PARENT: u32 = u32::MAX;
/// Secondes entre 1601 (origine des dates de Windows) et 1970.
const EPOCH_GAP: u64 = 11_644_473_600;
/// Nombre de repères de l'ordre alphabétique gardés pour un grand index.
const SAMPLES: usize = 32_768;

/// Date FILETIME → secondes depuis 1970 (0 : inconnue ou hors de portée).
fn seconds(filetime: u64) -> u32 {
    (filetime / 10_000_000).saturating_sub(EPOCH_GAP).min(u32::MAX as u64) as u32
}

#[derive(Default)]
pub struct Index {
    /// Noms en UTF-8, chacun suivi de `\n`.
    pub names: Vec<u8>,
    /// Début du nom de chaque élément dans la zone ; une case de plus que d'éléments.
    pub off: Vec<u32>,
    pub parent: Vec<u32>,
    /// Taille en octets ; `u32::MAX` renvoie à `big` (fichiers de 4 Go et plus, rares).
    size: Vec<u32>,
    big: HashMap<u32, u64>,
    /// Date de modification, en secondes depuis 1970 (0 : inconnue).
    mtime: Vec<u32>,
    pub flags: Vec<u8>,
    /// Place de chaque élément dans l'ordre alphabétique de tous les noms (les résultats sortent déjà triés).
    pub rank: Vec<u32>,
    /// Un élément tous les `step` dans l'ordre alphabétique, au moment de la construction : des repères pour
    /// donner une place aux éléments ajoutés ensuite (tous les éléments pour un petit index, 32 768 repères au plus
    /// pour un grand).
    sample: Vec<u32>,
    step: u32,
    /// Dossiers : identifiant NTFS → numéro de l'élément. Le journal des modifications désigne les dossiers ainsi.
    pub dir_ids: HashMap<u64, u32>,
}

/// Un élément d'un dossier tel qu'on vient de le lire sur le disque.
#[derive(Clone, Debug)]
pub struct Listed {
    pub name: String,
    pub size: u64,
    pub mtime: u64,
    pub flags: u8,
    pub file_id: u64,
}

/// Le contenu réel d'un dossier de l'index, pour le remettre à jour.
pub struct DirSync {
    pub dir: u32,
    pub listing: Vec<Listed>,
    /// Dossier tout juste ajouté à l'index : aucun de ses éléments n'y figure encore.
    pub fresh: bool,
}

/// Lit un dossier sous la forme attendue par [`Index::sync_dirs`]. Erreur : dossier disparu ou illisible.
pub fn list_for_sync(path: &Path) -> io::Result<Vec<Listed>> {
    let mut out = vec![];
    listing::read_dir(path, None, &mut |raw| {
        out.push(Listed { name: String::from_utf16_lossy(raw.name), size: raw.size, mtime: raw.mtime, flags: flags_of(raw.attrs), file_id: raw.file_id });
    })?;
    Ok(out)
}

/// Avancement de la construction, lisible depuis un autre fil.
#[derive(Default)]
pub struct Progress {
    pub entries: AtomicU64,
    pub folders: AtomicU64,
}

/// Met un nom en minuscules sans changer sa longueur en octets : un caractère dont la minuscule n'a pas la
/// même taille en UTF-8 est laissé tel quel. La requête passe par la même fonction.
pub fn fold_into(name: &str, out: &mut Vec<u8>) {
    if name.is_ascii() {
        out.extend(name.bytes().map(|b| b.to_ascii_lowercase()));
        return;
    }
    let mut buf = [0u8; 4];
    for c in name.chars() {
        let mut low = c.to_lowercase();
        match (low.next(), low.next()) {
            (Some(l), None) if l.len_utf8() == c.len_utf8() => out.extend_from_slice(l.encode_utf8(&mut buf).as_bytes()),
            _ => out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes()),
        }
    }
}

pub fn fold(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    fold_into(s, &mut out);
    out
}

impl Index {
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    /// Mémoire occupée par les tableaux, en octets.
    pub fn bytes(&self) -> usize {
        self.names.capacity()
            + (self.off.capacity() + self.parent.capacity() + self.size.capacity() + self.mtime.capacity() + self.rank.capacity() + self.sample.capacity()) * 4
            + self.flags.capacity()
            + self.dir_ids.capacity() * 17
            + self.big.capacity() * 17
    }

    /// Rend au système la place prise d'avance par les tableaux pendant la construction.
    pub fn shrink(&mut self) {
        self.names.shrink_to_fit();
        self.off.shrink_to_fit();
        self.parent.shrink_to_fit();
        self.size.shrink_to_fit();
        self.mtime.shrink_to_fit();
        self.flags.shrink_to_fit();
        self.rank.shrink_to_fit();
        self.dir_ids.shrink_to_fit();
        self.big.shrink_to_fit();
    }

    /// Taille d'un fichier en octets.
    pub fn size_of(&self, i: u32) -> u64 {
        match self.size[i as usize] {
            u32::MAX => self.big.get(&i).copied().unwrap_or(u32::MAX as u64),
            s => s as u64,
        }
    }

    fn set_size(&mut self, i: u32, size: u64) {
        if size >= u32::MAX as u64 {
            self.size[i as usize] = u32::MAX;
            self.big.insert(i, size);
        } else {
            self.size[i as usize] = size as u32;
        }
    }

    /// Date de modification en FILETIME (à la seconde près ; 0 : inconnue).
    pub fn mtime_of(&self, i: u32) -> u64 {
        match self.mtime[i as usize] {
            0 => 0,
            s => (s as u64 + EPOCH_GAP) * 10_000_000,
        }
    }

    pub fn name(&self, i: u32) -> &str {
        let (a, b) = (self.off[i as usize] as usize, self.off[i as usize + 1] as usize - 1);
        // les noms viennent de `String` : ils sont en UTF-8 valide
        unsafe { std::str::from_utf8_unchecked(&self.names[a..b]) }
    }

    /// Le nom en minuscules, écrit dans `buf` (les recherches ignorent la casse sans garder de copie des noms).
    pub fn folded<'a>(&self, i: u32, buf: &'a mut Vec<u8>) -> &'a [u8] {
        buf.clear();
        fold_into(self.name(i), buf);
        buf
    }

    /// Le nom de l'élément vaut-il `folded` (déjà en minuscules), à la casse près ?
    pub fn name_is(&self, i: u32, folded: &[u8]) -> bool {
        let name = self.name(i);
        name.len() == folded.len() && if name.is_ascii() { name.as_bytes().eq_ignore_ascii_case(folded) } else { fold(name) == folded }
    }

    pub fn is_dir(&self, i: u32) -> bool {
        self.flags[i as usize] & F_DIR != 0
    }

    /// Ajoute un élément et renvoie son numéro. Un enfant est toujours ajouté après son dossier parent.
    pub fn push(&mut self, name: &str, parent: u32, size: u64, mtime: u64, flags: u8) -> u32 {
        let id = self.parent.len() as u32;
        if self.off.is_empty() {
            self.off.push(0);
        }
        // un saut de ligne dans un nom (possible sur NTFS, rarissime) casserait la séparation des noms
        if name.contains('\n') {
            self.names.extend_from_slice(name.replace('\n', " ").as_bytes());
        } else {
            self.names.extend_from_slice(name.as_bytes());
        }
        self.names.push(b'\n');
        self.off.push(self.names.len() as u32);
        self.parent.push(parent);
        self.size.push(0);
        self.set_size(id, size);
        self.mtime.push(seconds(mtime));
        self.flags.push(flags);
        id
    }

    /// Chemin complet d'un élément : « C:\Users\samue\notes.txt ».
    pub fn path(&self, i: u32) -> PathBuf {
        let mut parts = vec![];
        let mut cur = i;
        while cur != NO_PARENT {
            parts.push(cur);
            cur = self.parent[cur as usize];
        }
        let mut s = String::with_capacity(parts.iter().map(|&p| self.name(p).len() + 1).sum());
        for (k, &p) in parts.iter().rev().enumerate() {
            let name = self.name(p);
            s.push_str(name);
            // la racine d'un disque s'appelle « C: » : la barre suit toujours, même seule (« C:\ »)
            if k + 1 < parts.len() || name.ends_with(':') {
                s.push('\\');
            }
        }
        PathBuf::from(s)
    }

    /// Chemin du dossier qui contient l'élément.
    pub fn parent_path(&self, i: u32) -> Option<PathBuf> {
        let p = self.parent[i as usize];
        (p != NO_PARENT).then(|| self.path(p))
    }

    /// Numéro de l'élément qui correspond à ce chemin (comparaison sans tenir compte de la casse).
    pub fn find(&self, path: &Path) -> Option<u32> {
        let wanted = fold(path.to_str()?.trim_end_matches('\\'));
        for root in (0..self.len() as u32).filter(|&i| self.parent[i as usize] == NO_PARENT) {
            let name = fold(self.name(root));
            let rest = match wanted.strip_prefix(name.as_slice()) {
                Some([]) => return Some(root),
                Some([b'\\', rest @ ..]) => rest,
                _ => continue,
            };
            let mut cur = root;
            let mut found = true;
            for part in rest.split(|&b| b == b'\\').filter(|p| !p.is_empty()) {
                let child =
                    (cur + 1..self.len() as u32).find(|&i| self.parent[i as usize] == cur && self.flags[i as usize] & F_DELETED == 0 && self.name_is(i, part));
                match child {
                    Some(c) => cur = c,
                    None => {
                        found = false;
                        break;
                    }
                }
            }
            if found {
                return Some(cur);
            }
        }
        None
    }

    /// Calcule l'ordre alphabétique de tous les noms (`rank`), sur plusieurs fils. C'est l'ordre de Windows,
    /// le même que dans la liste des fichiers : accents à leur place, « photo2 » avant « photo10 ».
    pub fn compute_rank(&mut self, threads: usize) {
        /// Un morceau de l'index : ses numéros triés, et leurs clés de tri rangées dans une seule zone.
        struct Part {
            first: u32,
            ids: Vec<u32>,
            keys: Vec<u8>,
            off: Vec<u32>,
        }
        impl Part {
            fn key(&self, id: u32) -> &[u8] {
                let k = (id - self.first) as usize;
                &self.keys[self.off[k] as usize..self.off[k + 1] as usize]
            }
        }
        let n = self.len();
        let threads = threads.clamp(1, 32);
        let chunk = n.div_ceil(threads).max(1);
        let this = &*self;
        let mut parts: Vec<Part> = vec![];
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..n)
                .step_by(chunk)
                .map(|a| {
                    let b = (a + chunk).min(n);
                    s.spawn(move || {
                        let mut maker = NameKeys::new();
                        let mut part = Part {
                            first: a as u32,
                            ids: (a as u32..b as u32).collect(),
                            keys: Vec::with_capacity((b - a) * 48),
                            off: Vec::with_capacity(b - a + 1),
                        };
                        part.off.push(0);
                        for i in a..b {
                            maker.key_into(this.name(i as u32), &mut part.keys);
                            part.off.push(part.keys.len() as u32);
                        }
                        let mut ids = std::mem::take(&mut part.ids);
                        ids.sort_unstable_by(|&x, &y| part.key(x).cmp(part.key(y)).then(x.cmp(&y)));
                        part.ids = ids;
                        part
                    })
                })
                .collect();
            parts = handles.into_iter().filter_map(|h| h.join().ok()).collect();
        });
        // fusion des morceaux triés
        let mut next = vec![0usize; parts.len()];
        let mut heap = std::collections::BinaryHeap::with_capacity(parts.len());
        for (k, p) in parts.iter().enumerate() {
            if let Some(&id) = p.ids.first() {
                heap.push(std::cmp::Reverse((p.key(id), id, k)));
            }
        }
        let mut rank = vec![0u32; n];
        let step = (n / SAMPLES).max(1);
        let mut sample = Vec::with_capacity(n / step + 1);
        let mut pos = 0u32;
        while let Some(std::cmp::Reverse((_, id, k))) = heap.pop() {
            rank[id as usize] = pos;
            if (pos as usize).is_multiple_of(step) {
                sample.push(id);
            }
            pos += 1;
            next[k] += 1;
            if let Some(&id) = parts[k].ids.get(next[k]) {
                heap.push(std::cmp::Reverse((parts[k].key(id), id, k)));
            }
        }
        self.rank = rank;
        self.sample = sample;
        self.step = step as u32;
        self.shrink();
    }
}

struct Work {
    /// Chemin du dossier, préfixé `\\?\` et terminé par zéro.
    path: Vec<u16>,
    id: u32,
}

struct Queue {
    work: Vec<Work>,
    /// Dossiers en attente ou en cours de lecture : zéro quand tout est fini.
    pending: usize,
}

/// Ce qu'un fil a lu dans un dossier, avant de le verser dans l'index.
#[derive(Default)]
struct Batch {
    names: String,
    ends: Vec<u32>,
    size: Vec<u64>,
    mtime: Vec<u64>,
    flags: Vec<u8>,
    /// Sous-dossiers : (rang dans le lot, nom en UTF-16, identifiant NTFS, à parcourir ?).
    dirs: Vec<(u32, Vec<u16>, u64, bool)>,
}

fn flags_of(attrs: u32) -> u8 {
    let mut f = 0;
    if attrs & ATTR_DIRECTORY != 0 {
        f |= F_DIR;
    }
    if attrs & ATTR_HIDDEN != 0 {
        f |= F_HIDDEN;
    }
    if attrs & ATTR_SYSTEM != 0 {
        f |= F_SYSTEM;
    }
    if attrs & ATTR_REPARSE != 0 {
        f |= F_LINK;
    }
    if attrs & ATTR_READONLY != 0 {
        f |= F_READONLY;
    }
    if attrs & (ATTR_OFFLINE | ATTR_RECALL_ON_DATA_ACCESS) != 0 {
        f |= F_OFFLINE;
    }
    f
}

/// Construit l'index des dossiers `roots` (des racines de disques, en général) sur `threads` fils.
/// Les dossiers illisibles sont ignorés ; on ne descend pas dans les liens et les jonctions.
/// L'ordre alphabétique (`compute_rank`) reste à calculer.
pub fn build(roots: &[PathBuf], threads: usize, progress: &Progress, cancel: &AtomicBool) -> Index {
    let mut index = Index::default();
    let mut first = vec![];
    for root in roots {
        let name = root.display().to_string();
        let id = index.push(name.trim_end_matches('\\'), NO_PARENT, 0, 0, F_DIR);
        if let Some(fid) = listing::dir_id(root) {
            index.dir_ids.insert(fid, id);
        }
        first.push(Work { path: long_path(root), id });
    }
    let pending = first.len();
    let (global, queue, wake) = (Mutex::new(index), Mutex::new(Queue { work: first, pending }), Condvar::new());
    std::thread::scope(|s| {
        for _ in 0..threads.clamp(1, 64) {
            s.spawn(|| {
                let mut batch = Batch::default();
                // un tampon de lecture par fil, réutilisé pour tous ses dossiers
                let mut buf = vec![0u64; 64 * 1024 / 8];
                loop {
                    let work = {
                        let mut q = queue.lock().unwrap();
                        loop {
                            if let Some(w) = q.work.pop() {
                                break Some(w);
                            }
                            if q.pending == 0 || cancel.load(Ordering::Relaxed) {
                                break None;
                            }
                            q = wake.wait(q).unwrap();
                        }
                    };
                    let Some(work) = work else { break };
                    batch.names.clear();
                    batch.ends.clear();
                    batch.size.clear();
                    batch.mtime.clear();
                    batch.flags.clear();
                    batch.dirs.clear();
                    let read = listing::read_dir_buf(&work.path, Some(cancel), &mut buf, &mut |raw| {
                        let f = flags_of(raw.attrs);
                        if f & F_DIR != 0 {
                            batch.dirs.push((batch.ends.len() as u32, raw.name.to_vec(), raw.file_id, f & F_LINK == 0));
                        }
                        batch.names.extend(char::decode_utf16(raw.name.iter().copied()).map(|c| c.unwrap_or('\u{fffd}')));
                        batch.ends.push(batch.names.len() as u32);
                        batch.size.push(raw.size);
                        batch.mtime.push(raw.mtime);
                        batch.flags.push(f);
                    });
                    let base = {
                        let mut g = global.lock().unwrap();
                        // accès refusé : le dossier reste dans l'index, marqué comme non lu
                        if read.is_err_and(|e| e.kind() == io::ErrorKind::PermissionDenied) {
                            g.flags[work.id as usize] |= F_DENIED;
                        }
                        let base = g.len() as u32;
                        let mut start = 0usize;
                        for k in 0..batch.ends.len() {
                            let end = batch.ends[k] as usize;
                            g.push(&batch.names[start..end], work.id, batch.size[k], batch.mtime[k], batch.flags[k]);
                            start = end;
                        }
                        for (k, _, fid, _) in &batch.dirs {
                            g.dir_ids.insert(*fid, base + k);
                        }
                        base
                    };
                    progress.entries.fetch_add(batch.ends.len() as u64, Ordering::Relaxed);
                    progress.folders.fetch_add(1, Ordering::Relaxed);
                    let mut q = queue.lock().unwrap();
                    batch.dirs.retain(|d| d.3);
                    let added = batch.dirs.len();
                    for (k, name, _, _) in batch.dirs.drain(..) {
                        // chemin du sous-dossier : celui du dossier (sans son zéro final), une barre, le nom, un zéro
                        let mut path = Vec::with_capacity(work.path.len() + name.len() + 1);
                        path.extend_from_slice(&work.path[..work.path.len() - 1]);
                        if path.last() != Some(&(b'\\' as u16)) {
                            path.push(b'\\' as u16);
                        }
                        path.extend_from_slice(&name);
                        path.push(0);
                        q.work.push(Work { path, id: base + k });
                        q.pending += 1;
                    }
                    q.pending -= 1;
                    let finished = q.pending == 0;
                    drop(q);
                    // un fil réveillé par dossier ajouté (celui-ci en prendra un lui-même) ; tous à la fin
                    if finished {
                        wake.notify_all();
                    } else {
                        for _ in 1..added.min(8) {
                            wake.notify_one();
                        }
                    }
                }
                // un fil qui s'arrête réveille les autres : ils verront qu'il n'y a plus rien à faire
                wake.notify_all();
            });
        }
    });
    let mut index = global.into_inner().unwrap();
    index.shrink();
    index
}

// ───────────────────────── mise à jour ─────────────────────────

impl Index {
    /// Ajoute un élément à un index déjà rangé : il reçoit une place approchée dans l'ordre alphabétique.
    fn add(&mut self, name: &str, parent: u32, size: u64, mtime: u64, flags: u8, keys: &mut NameKeys) -> u32 {
        // Les tableaux d'un index construit sont taillés au plus juste. Pour un fichier de plus, on les agrandit
        // d'un petit pas, pas du double (ce que `Vec` ferait de lui-même, et qui doublerait la mémoire réservée).
        fn room<T>(v: &mut Vec<T>, extra: usize, step: usize) {
            if v.capacity() - v.len() < extra {
                v.reserve_exact(step.max(extra));
            }
        }
        room(&mut self.names, name.len() + 1, 1 << 20);
        room(&mut self.off, 2, 1 << 14);
        room(&mut self.parent, 1, 1 << 14);
        room(&mut self.size, 1, 1 << 14);
        room(&mut self.mtime, 1, 1 << 14);
        room(&mut self.flags, 1, 1 << 14);
        room(&mut self.rank, 1, 1 << 14);
        let ranked = self.rank.len() == self.len();
        let id = self.push(name, parent, size, mtime, flags);
        if ranked {
            let key = keys.key(name);
            let mut other = Vec::new();
            let place = self.sample.partition_point(|&e| {
                other.clear();
                keys.key_into(self.name(e), &mut other);
                other.as_slice() < &*key
            });
            self.rank.push((place * self.step.max(1) as usize).min(u32::MAX as usize) as u32);
        }
        id
    }

    /// Remet des dossiers de l'index en accord avec leur contenu réel : éléments ajoutés, supprimés, modifiés.
    /// Renvoie les sous-dossiers nouveaux (leur contenu reste à lire) et dit si quelque chose a changé.
    pub fn sync_dirs(&mut self, batch: &[DirSync]) -> (Vec<u32>, bool) {
        let mut keys = NameKeys::new();
        // les éléments déjà connus de chaque dossier du lot, en une seule passe sur l'index
        let wanted: HashMap<u32, usize> = batch.iter().enumerate().filter(|(_, d)| !d.fresh).map(|(k, d)| (d.dir, k)).collect();
        let mut known: Vec<HashMap<Vec<u8>, u32>> = vec![HashMap::new(); batch.len()];
        if let Some(&first) = wanted.keys().min() {
            for i in first + 1..self.len() as u32 {
                if self.flags[i as usize] & F_DELETED == 0
                    && let Some(&k) = wanted.get(&self.parent[i as usize])
                {
                    known[k].insert(fold(self.name(i)), i);
                }
            }
        }
        let (mut new_dirs, mut changed, mut first_removed_dir) = (vec![], false, u32::MAX);
        for (k, d) in batch.iter().enumerate() {
            if d.dir as usize >= self.len() || self.flags[d.dir as usize] & F_DELETED != 0 {
                continue;
            }
            // le dossier vient d'être lu : s'il était marqué illisible, il ne l'est plus
            if self.flags[d.dir as usize] & F_DENIED != 0 {
                self.flags[d.dir as usize] &= !F_DENIED;
                changed = true;
            }
            for e in &d.listing {
                let existing = known[k].remove(&fold(&e.name));
                match existing {
                    Some(i) if self.flags[i as usize] & F_DIR == e.flags & F_DIR && self.name(i) == e.name => {
                        if self.size_of(i) != e.size || self.mtime[i as usize] != seconds(e.mtime) || self.flags[i as usize] & !F_DENIED != e.flags {
                            self.set_size(i, e.size);
                            let i = i as usize;
                            self.mtime[i] = seconds(e.mtime);
                            self.flags[i] = (self.flags[i] & F_DENIED) | e.flags;
                            changed = true;
                        }
                        if e.flags & F_DIR != 0 {
                            self.dir_ids.insert(e.file_id, i);
                        }
                    }
                    other => {
                        // nouveau venu ; ou même nom à la casse près, ou fichier devenu dossier : l'ancien s'efface
                        if let Some(i) = other {
                            self.flags[i as usize] |= F_DELETED;
                            if self.flags[i as usize] & F_DIR != 0 {
                                first_removed_dir = first_removed_dir.min(i);
                            }
                        }
                        let id = self.add(&e.name, d.dir, e.size, e.mtime, e.flags, &mut keys);
                        if e.flags & F_DIR != 0 {
                            self.dir_ids.insert(e.file_id, id);
                            if e.flags & F_LINK == 0 {
                                new_dirs.push(id);
                            }
                        }
                        changed = true;
                    }
                }
            }
            // ce que le disque ne montre plus a été supprimé, déplacé ou renommé
            for (_, i) in known[k].drain() {
                self.flags[i as usize] |= F_DELETED;
                if self.flags[i as usize] & F_DIR != 0 {
                    first_removed_dir = first_removed_dir.min(i);
                }
                changed = true;
            }
        }
        // un dossier disparu emporte tout ce qu'il contenait (les enfants suivent toujours leur parent dans l'index)
        if first_removed_dir != u32::MAX {
            for i in first_removed_dir as usize + 1..self.len() {
                let p = self.parent[i];
                if p != NO_PARENT && self.flags[p as usize] & F_DELETED != 0 {
                    self.flags[i] |= F_DELETED;
                }
            }
        }
        (new_dirs, changed)
    }

    /// La taille de chaque sous-dossier direct de `dir` : la somme de tous les fichiers qu'il contient, à tous
    /// les niveaux. Renvoie (numéro du sous-dossier, octets).
    pub fn folder_sizes(&self, dir: u32) -> Vec<(u32, u64)> {
        let n = self.len();
        // pour chaque élément, le sous-dossier direct de `dir` dont il fait partie (sa place dans `out`, plus un ;
        // 0 : aucun). Un enfant suit toujours son dossier parent : une seule passe suffit.
        let mut top = vec![0u32; n];
        let mut out: Vec<(u32, u64)> = vec![];
        for i in dir as usize + 1..n {
            let p = self.parent[i];
            if p == NO_PARENT || self.flags[i] & F_DELETED != 0 {
                continue;
            }
            let is_dir = self.flags[i] & F_DIR != 0;
            let t = if p != dir {
                top[p as usize]
            } else if is_dir {
                out.push((i as u32, 0));
                out.len() as u32
            } else {
                0
            };
            if t != 0 {
                top[i] = t;
                if !is_dir {
                    out[t as usize - 1].1 += self.size_of(i as u32);
                }
            }
        }
        out
    }

    /// Marque un dossier comme illisible : la lecture de son contenu a été refusée.
    pub fn deny(&mut self, dir: u32) {
        if let Some(f) = self.flags.get_mut(dir as usize) {
            *f |= F_DENIED;
        }
    }

    /// Ajoute un disque branché après la construction (clé USB, lecteur réseau) : sa racine seule ; son contenu se
    /// lit ensuite avec [`Index::walk`]. Renvoie le numéro de la racine.
    pub fn add_root(&mut self, root: &Path) -> u32 {
        let mut keys = NameKeys::new();
        let name = root.display().to_string();
        let id = self.add(name.trim_end_matches('\\'), NO_PARENT, 0, 0, F_DIR, &mut keys);
        if let Some(fid) = listing::dir_id(root) {
            self.dir_ids.insert(fid, id);
        }
        id
    }

    /// Un disque débranché : sa racine et tout ce qu'elle contient sortent des recherches.
    pub fn drop_root(&mut self, root: u32) {
        let Some(f) = self.flags.get_mut(root as usize) else { return };
        *f |= F_DELETED;
        // un enfant suit toujours son dossier parent : une passe suffit
        for i in root as usize + 1..self.len() {
            let p = self.parent[i];
            if p != NO_PARENT && self.flags[p as usize] & F_DELETED != 0 {
                self.flags[i] |= F_DELETED;
            }
        }
    }

    /// Les racines de l'index (non supprimées) : (numéro, nom tel que « E: »).
    pub fn roots(&self) -> Vec<(u32, String)> {
        (0..self.len() as u32).filter(|&i| self.parent[i as usize] == NO_PARENT && self.flags[i as usize] & F_DELETED == 0).map(|i| (i, self.name(i).to_string())).collect()
    }

    /// Les dossiers (non supprimés) sous `root`, `root` compris, dans l'ordre de l'index.
    pub fn dirs_under(&self, root: u32) -> Vec<u32> {
        let mut inside = vec![false; self.len()];
        let mut out = vec![];
        if (root as usize) < self.len() && self.flags[root as usize] & F_DELETED == 0 {
            inside[root as usize] = true;
            out.push(root);
        }
        for i in root as usize + 1..self.len() {
            let p = self.parent[i];
            if p != NO_PARENT && inside[p as usize] && self.flags[i] & F_DELETED == 0 {
                inside[i] = true;
                if self.flags[i] & F_DIR != 0 && self.flags[i] & F_LINK == 0 {
                    out.push(i as u32);
                }
            }
        }
        out
    }

    /// Nombre d'éléments supprimés encore présents dans les tableaux.
    pub fn deleted(&self) -> usize {
        self.flags.iter().filter(|&&f| f & F_DELETED != 0).count()
    }
}

/// Ce qui donne l'index à un travail, le temps d'une mise à jour.
pub type Lend<'a> = dyn FnMut(&mut dyn FnMut(&mut Index)) + 'a;

/// Relit `dirs` sur le disque et met l'index d'accord, puis lit les sous-dossiers nouveaux, jusqu'au bout.
/// `with` donne l'index (pris le temps d'une mise à jour seulement : les recherches continuent entre deux).
/// `stop` interrompt. Renvoie vrai si quelque chose a changé.
pub fn walk(dirs: Vec<u32>, with: &mut Lend, stop: &dyn Fn() -> bool) -> bool {
    let mut todo: Vec<(u32, bool)> = dirs.into_iter().map(|d| (d, false)).collect();
    let mut changed = false;
    while !todo.is_empty() && !stop() {
        let chunk: Vec<(u32, bool)> = todo.drain(..todo.len().min(256)).collect();
        let mut paths = vec![];
        with(&mut |ix| paths = chunk.iter().map(|&(d, fresh)| (d, fresh, ix.path(d))).collect());
        let (mut batch, mut denied) = (vec![], vec![]);
        for (dir, fresh, path) in paths {
            match list_for_sync(&path) {
                Ok(listing) => batch.push(DirSync { dir, listing, fresh }),
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => denied.push(dir),
                Err(_) => {}
            }
        }
        with(&mut |ix| {
            denied.iter().for_each(|&d| ix.deny(d));
            let (new_dirs, c) = ix.sync_dirs(&batch);
            changed |= c;
            todo.extend(new_dirs.into_iter().map(|d| (d, true)));
        });
    }
    changed
}

// ───────────────────────── fichier ─────────────────────────

const MAGIC: &[u8; 8] = b"PFEXIDX5";

fn put<T: Copy>(w: &mut impl Write, v: &[T]) -> io::Result<()> {
    w.write_all(&(v.len() as u64).to_le_bytes())?;
    // SAFETY: T est un entier simple (u8, u32, u64) : ses octets sont tous lisibles
    w.write_all(unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) })
}

fn get<T: Copy + Default>(r: &mut impl Read) -> io::Result<Vec<T>> {
    let mut len = [0u8; 8];
    r.read_exact(&mut len)?;
    let n = u64::from_le_bytes(len) as usize;
    // garde-fou contre un fichier abîmé : pas de tableau de plus de 4 Go
    if n.saturating_mul(size_of::<T>()) > 4 << 30 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut v = vec![T::default(); n];
    // SAFETY: mêmes entiers simples, n'importe quels octets y sont valides
    r.read_exact(unsafe { std::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, n * size_of::<T>()) })?;
    Ok(v)
}

/// Où en était la lecture du journal d'un volume quand l'index a été enregistré : (racine, journal, position).
pub type Volume = (String, u64, i64);

impl Index {
    /// Écrit l'index dans un fichier, avec la position du journal de chaque volume.
    /// Le fichier est d'abord écrit à côté puis renommé : jamais d'index à moitié écrit.
    pub fn save(&self, path: &Path, volumes: &[Volume]) -> io::Result<()> {
        let tmp = path.with_extension("tmp");
        {
            let mut w = io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&tmp)?);
            w.write_all(MAGIC)?;
            put(&mut w, &self.names)?;
            put(&mut w, &self.off)?;
            put(&mut w, &self.parent)?;
            put(&mut w, &self.size)?;
            put(&mut w, &self.mtime)?;
            put(&mut w, &self.flags)?;
            put(&mut w, &self.rank)?;
            put(&mut w, &self.sample)?;
            w.write_all(&(self.step as u64).to_le_bytes())?;
            let (big_ids, big_sizes): (Vec<u32>, Vec<u64>) = self.big.iter().map(|(a, b)| (*a, *b)).unzip();
            put(&mut w, &big_ids)?;
            put(&mut w, &big_sizes)?;
            let (fids, ids): (Vec<u64>, Vec<u32>) = self.dir_ids.iter().map(|(a, b)| (*a, *b)).unzip();
            put(&mut w, &fids)?;
            put(&mut w, &ids)?;
            w.write_all(&(volumes.len() as u64).to_le_bytes())?;
            for (root, journal, next) in volumes {
                put(&mut w, root.as_bytes())?;
                w.write_all(&journal.to_le_bytes())?;
                w.write_all(&next.to_le_bytes())?;
            }
            w.flush()?;
        }
        std::fs::rename(&tmp, path)
    }

    /// Relit un index enregistré par [`Index::save`]. Tout y est vérifié : un fichier abîmé donne une erreur.
    pub fn load(path: &Path) -> io::Result<(Index, Vec<Volume>)> {
        let bad = || io::Error::from(io::ErrorKind::InvalidData);
        let mut r = io::BufReader::with_capacity(1 << 20, std::fs::File::open(path)?);
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(bad());
        }
        let mut ix = Index {
            names: get(&mut r)?,
            off: get(&mut r)?,
            parent: get(&mut r)?,
            size: get(&mut r)?,
            mtime: get(&mut r)?,
            flags: get(&mut r)?,
            rank: get(&mut r)?,
            sample: get(&mut r)?,
            ..Default::default()
        };
        let mut step = [0u8; 8];
        r.read_exact(&mut step)?;
        ix.step = u64::from_le_bytes(step).clamp(1, u32::MAX as u64) as u32;
        let (big_ids, big_sizes): (Vec<u32>, Vec<u64>) = (get(&mut r)?, get(&mut r)?);
        let (fids, ids): (Vec<u64>, Vec<u32>) = (get(&mut r)?, get(&mut r)?);
        let mut word = [0u8; 8];
        r.read_exact(&mut word)?;
        let mut volumes = vec![];
        for _ in 0..u64::from_le_bytes(word).min(64) {
            let root = String::from_utf8(get::<u8>(&mut r)?).map_err(|_| bad())?;
            r.read_exact(&mut word)?;
            let journal = u64::from_le_bytes(word);
            r.read_exact(&mut word)?;
            volumes.push((root, journal, i64::from_le_bytes(word)));
        }
        // cohérence : mêmes longueurs partout, noms en UTF-8 terminés par un saut de ligne, parents avant enfants
        let n = ix.parent.len();
        let sizes_ok = ix.off.len() == n + 1
            && ix.size.len() == n
            && ix.mtime.len() == n
            && ix.flags.len() == n
            && ix.rank.len() == n
            && fids.len() == ids.len()
            && big_ids.len() == big_sizes.len();
        if !sizes_ok || ix.off.first() != Some(&0) || ix.off.last().copied() != Some(ix.names.len() as u32) || std::str::from_utf8(&ix.names).is_err() {
            return Err(bad());
        }
        for i in 0..n {
            let (a, b) = (ix.off[i] as usize, ix.off[i + 1] as usize);
            let parent_ok = ix.parent[i] == NO_PARENT || (ix.parent[i] as usize) < i;
            if a >= b || b > ix.names.len() || ix.names[b - 1] != b'\n' || !parent_ok {
                return Err(bad());
            }
        }
        if ix.sample.iter().chain(&ids).chain(&big_ids).any(|&i| i as usize >= n) {
            return Err(bad());
        }
        ix.dir_ids = fids.into_iter().zip(ids).collect();
        ix.big = big_ids.into_iter().zip(big_sizes).collect();
        Ok((ix, volumes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::tests::TempDir;

    fn tree(tag: &str) -> TempDir {
        let d = TempDir::new(tag);
        std::fs::create_dir_all(d.0.join("Projets").join("Plume").join("src")).unwrap();
        std::fs::create_dir_all(d.0.join("Photos")).unwrap();
        std::fs::write(d.0.join("Projets").join("Plume").join("src").join("main.rs"), b"fn main() {}").unwrap();
        std::fs::write(d.0.join("Projets").join("Plume").join("README.md"), b"# Plume").unwrap();
        std::fs::write(d.0.join("Photos").join("Été 2026.JPG"), b"jpeg").unwrap();
        std::fs::write(d.0.join("notes.txt"), b"hello").unwrap();
        d
    }

    #[test]
    fn folding_keeps_byte_length() {
        assert_eq!(fold("ReadMe.TXT"), b"readme.txt");
        assert_eq!(fold("Été"), "été".as_bytes());
        // « İ » (2 octets) a une minuscule de 3 octets : laissé tel quel
        let s = "İstanbul";
        assert_eq!(fold(s).len(), s.len());
        assert_eq!(fold("ǅ").len(), "ǅ".len());
    }

    #[test]
    fn builds_an_index_of_a_tree() {
        let d = tree("index-build");
        let (progress, cancel) = (Progress::default(), AtomicBool::new(false));
        let mut ix = build(std::slice::from_ref(&d.0), 4, &progress, &cancel);
        ix.compute_rank(3);
        // la racine + 4 dossiers + 4 fichiers
        assert_eq!(ix.len(), 9);
        assert_eq!(progress.entries.load(Ordering::Relaxed), 8);
        assert_eq!(ix.off.len(), ix.len() + 1);
        let main = (0..ix.len() as u32).find(|&i| ix.name(i) == "main.rs").unwrap();
        assert_eq!(ix.path(main), d.0.join("Projets").join("Plume").join("src").join("main.rs"));
        assert_eq!(ix.parent_path(main), Some(d.0.join("Projets").join("Plume").join("src")));
        assert_eq!(ix.size_of(main), 12);
        assert!(ix.mtime_of(main) > 130_000_000_000_000_000);
        assert!(!ix.is_dir(main) && ix.is_dir(ix.parent[main as usize]));
        // un enfant vient toujours après son parent
        assert!((1..ix.len()).all(|i| ix.parent[i] < i as u32));
        assert_eq!(ix.find(&d.0.join("projets").join("PLUME")), Some(ix.parent[ix.parent[main as usize] as usize]));
        assert_eq!(ix.find(&d.0), Some(0));
        assert_eq!(ix.find(&d.0.join("nope")), None);
        let photo = (0..ix.len() as u32).find(|&i| ix.name(i) == "Été 2026.JPG").unwrap();
        assert_eq!(ix.folded(photo, &mut Vec::new()), "été 2026.jpg".as_bytes());
        assert!(ix.name_is(photo, "été 2026.jpg".as_bytes()) && !ix.name_is(photo, "ete 2026.jpg".as_bytes()));
        // ordre alphabétique de Windows : une place par élément, « Été » rangé à E et non après Z
        let mut by_rank: Vec<u32> = (1..ix.len() as u32).collect();
        by_rank.sort_by_key(|&i| ix.rank[i as usize]);
        let names: Vec<&str> = by_rank.iter().map(|&i| ix.name(i)).collect();
        assert_eq!(names, ["Été 2026.JPG", "main.rs", "notes.txt", "Photos", "Plume", "Projets", "README.md", "src"]);
        let mut all: Vec<u32> = ix.rank.clone();
        all.sort_unstable();
        assert!(all.iter().enumerate().all(|(k, &r)| r == k as u32));
    }

    fn live_names(ix: &Index) -> Vec<String> {
        let mut v: Vec<String> = (0..ix.len() as u32).filter(|&i| ix.flags[i as usize] & F_DELETED == 0).map(|i| ix.path(i).display().to_string()).collect();
        v.sort();
        v
    }

    /// Relit sur le disque les dossiers donnés, puis les nouveaux sous-dossiers découverts, jusqu'à épuisement.
    fn resync(ix: &mut Index, dirs: &[u32]) -> bool {
        let mut todo: Vec<(u32, bool)> = dirs.iter().map(|&d| (d, false)).collect();
        let mut changed = false;
        while !todo.is_empty() {
            let batch: Vec<DirSync> =
                todo.drain(..).filter_map(|(dir, fresh)| list_for_sync(&ix.path(dir)).ok().map(|listing| DirSync { dir, listing, fresh })).collect();
            let (new_dirs, c) = ix.sync_dirs(&batch);
            changed |= c;
            todo = new_dirs.into_iter().map(|d| (d, true)).collect();
        }
        changed
    }

    #[test]
    fn drives_come_and_go() {
        let (a, b) = (tree("index-extra-a"), tree("index-extra-b"));
        let (progress, cancel) = (Progress::default(), AtomicBool::new(false));
        let mut ix = build(std::slice::from_ref(&a.0), 2, &progress, &cancel);
        ix.compute_rank(2);
        // un disque branché : sa racine, puis tout son contenu
        let root = ix.add_root(&b.0);
        let mut with = |f: &mut dyn FnMut(&mut Index)| f(&mut ix);
        assert!(walk(vec![root], &mut with, &|| false));
        assert_eq!(ix.roots().len(), 2);
        let main = ix.find(&b.0.join("Projets").join("Plume").join("src").join("main.rs")).unwrap();
        assert_eq!(ix.size_of(main), 12);
        assert_eq!(ix.dirs_under(root).len(), 5);
        // relu sans changement : rien ne bouge ; un fichier ajouté : il arrive
        let dirs = ix.dirs_under(root);
        let mut with = |f: &mut dyn FnMut(&mut Index)| f(&mut ix);
        assert!(!walk(dirs, &mut with, &|| false));
        std::fs::write(b.0.join("Photos").join("new.jpg"), b"abc").unwrap();
        let dirs = ix.dirs_under(root);
        let mut with = |f: &mut dyn FnMut(&mut Index)| f(&mut ix);
        assert!(walk(dirs, &mut with, &|| false));
        assert!(ix.find(&b.0.join("Photos").join("new.jpg")).is_some());
        // débranché : il disparaît, l'autre reste
        ix.drop_root(root);
        assert!(ix.find(&b.0.join("Photos").join("new.jpg")).is_none());
        assert_eq!(ix.roots().len(), 1);
        assert!(ix.find(&a.0.join("notes.txt")).is_some());
    }

    #[test]
    fn sync_follows_the_disk() {
        let d = tree("index-sync");
        let (progress, cancel) = (Progress::default(), AtomicBool::new(false));
        let mut ix = build(std::slice::from_ref(&d.0), 3, &progress, &cancel);
        ix.compute_rank(2);
        let projets = ix.find(&d.0.join("Projets")).unwrap();
        // rien n'a changé : rien ne bouge (NTFS met à jour avec un léger retard les dates des dossiers tout
        // juste remplis : une première relecture peut encore les corriger, la suivante ne trouve plus rien)
        resync(&mut ix, &[0, projets]);
        assert!(!resync(&mut ix, &[0, projets]));
        // un fichier créé, un modifié, un supprimé, un dossier entier ajouté, un autre renommé
        std::fs::write(d.0.join("nouveau.txt"), b"abc").unwrap();
        std::fs::write(d.0.join("notes.txt"), b"hello world, longer").unwrap();
        std::fs::remove_file(d.0.join("Photos").join("Été 2026.JPG")).unwrap();
        std::fs::create_dir_all(d.0.join("Archive").join("2025")).unwrap();
        std::fs::write(d.0.join("Archive").join("2025").join("bilan.pdf"), b"pdf").unwrap();
        std::fs::rename(d.0.join("Projets").join("Plume"), d.0.join("Projets").join("PlumeSuite")).unwrap();
        let photos = ix.find(&d.0.join("Photos")).unwrap();
        assert!(resync(&mut ix, &[0, projets, photos]));
        let mut fresh = build(std::slice::from_ref(&d.0), 2, &progress, &cancel);
        fresh.compute_rank(2);
        assert_eq!(live_names(&ix), live_names(&fresh));
        // l'ancien dossier et son contenu sont marqués supprimés ; le nouveau a été relu en entier
        assert_eq!(ix.find(&d.0.join("Projets").join("Plume")), None);
        let main = ix.find(&d.0.join("Projets").join("PlumeSuite").join("src").join("main.rs")).unwrap();
        assert_eq!(ix.size_of(main), 12);
        let notes = ix.find(&d.0.join("notes.txt")).unwrap();
        assert_eq!(ix.size_of(notes), 19);
        assert!(ix.deleted() >= 5);
        // chaque élément a sa place alphabétique, et les dossiers nouveaux sont retrouvables par leur identifiant
        assert_eq!(ix.rank.len(), ix.len());
        let archive = ix.find(&d.0.join("Archive")).unwrap();
        assert!(ix.dir_ids.values().any(|&v| v == archive));
        // un nouvel élément est rangé près de ses voisins alphabétiques
        let nouveau = ix.find(&d.0.join("nouveau.txt")).unwrap();
        assert!(ix.rank[nouveau as usize] >= ix.rank[main as usize] && ix.rank[nouveau as usize] <= ix.rank[ix.find(&d.0.join("Projets")).unwrap() as usize]);
    }

    #[test]
    fn saves_and_loads() {
        let d = tree("index-save");
        let (progress, cancel) = (Progress::default(), AtomicBool::new(false));
        let mut ix = build(&[d.0.join("Projets"), d.0.join("Photos")], 2, &progress, &cancel);
        ix.compute_rank(2);
        let file = d.0.join("index.bin");
        let volumes = vec![("C:\\".to_string(), 0x1234_5678_9abc_def0, 987_654_321i64)];
        ix.save(&file, &volumes).unwrap();
        let (back, vols) = Index::load(&file).unwrap();
        assert_eq!(vols, volumes);
        assert_eq!((back.names == ix.names, back.off == ix.off, back.parent == ix.parent, back.big == ix.big), (true, true, true, true));
        assert_eq!(
            (back.size == ix.size, back.mtime == ix.mtime, back.flags == ix.flags, back.rank == ix.rank, back.sample == ix.sample && back.step == ix.step),
            (true, true, true, true, true)
        );
        assert_eq!(back.dir_ids, ix.dir_ids);
        // un fichier tronqué ou étranger est refusé, sans plantage
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, &bytes[..bytes.len() / 2]).unwrap();
        assert!(Index::load(&file).is_err());
        std::fs::write(&file, b"not an index at all").unwrap();
        assert!(Index::load(&file).is_err());
        let mut broken = bytes.clone();
        let at = broken.len() / 3;
        broken[at..at + 64].iter_mut().for_each(|b| *b = 0xff);
        std::fs::write(&file, &broken).unwrap();
        let _ = Index::load(&file);
        assert!(Index::load(&d.0.join("missing.bin")).is_err());
    }

    #[test]
    fn folder_sizes_add_up_every_level() {
        let mut ix = Index::default();
        let c = ix.push("C:", NO_PARENT, 0, 0, F_DIR);
        let a = ix.push("A", c, 0, 0, F_DIR);
        let b = ix.push("B", c, 0, 0, F_DIR);
        ix.push("root.txt", c, 7, 0, 0);
        ix.push("a1", a, 100, 0, 0);
        let deep = ix.push("Deep", a, 0, 0, F_DIR);
        let empty = ix.push("Empty", c, 0, 0, F_DIR);
        ix.push("d1", deep, 20, 0, 0);
        ix.push("big", deep, 5_000_000_000, 0, 0);
        ix.push("gone", deep, 999, 0, F_DELETED);
        ix.push("b1", b, 3, 0, 0);
        assert_eq!(ix.folder_sizes(c), [(a, 5_000_000_120), (b, 3), (empty, 0)]);
        assert_eq!(ix.folder_sizes(a), [(deep, 5_000_000_020)]);
        assert_eq!(ix.folder_sizes(deep), []);
    }

    #[test]
    fn drive_root_paths() {
        let mut ix = Index::default();
        let c = ix.push("C:", NO_PARENT, 0, 0, F_DIR);
        let users = ix.push("Users", c, 0, 0, F_DIR);
        let f = ix.push("a.txt", users, 1, 2, 0);
        assert_eq!(ix.path(c), PathBuf::from(r"C:\"));
        assert_eq!(ix.path(users), PathBuf::from(r"C:\Users"));
        assert_eq!(ix.path(f), PathBuf::from(r"C:\Users\a.txt"));
        // tailles : un gros fichier (4 Go et plus) garde sa taille exacte ; dates : à la seconde près
        let big = ix.push("film.mkv", users, 9_000_000_000, 134_387_856_001_234_567, 0);
        assert_eq!((ix.size_of(big), ix.size_of(f)), (9_000_000_000, 1));
        assert_eq!((ix.mtime_of(big), ix.mtime_of(c)), (134_387_856_000_000_000, 0));
        assert_eq!(ix.find(Path::new(r"c:\users\A.TXT")), Some(f));
        assert_eq!(ix.find(Path::new(r"C:\")), Some(c));
    }
}
