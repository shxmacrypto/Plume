//! La recherche instantanée : l'index de tous les disques et les recherches lancées à chaque frappe, hors du fil
//! de l'interface.
//!
//! Un fil de service tient l'index à jour sans droits d'administrateur :
//! 1. au lancement, il relit l'index enregistré la dernière fois (prêt en une fraction de seconde), puis demande
//!    au journal des modifications de NTFS ce qui a changé depuis, et ne relit que ces dossiers-là ;
//! 2. sans index enregistré (premier lancement) ou si le journal ne remonte pas assez loin, il parcourt les disques ;
//! 3. ensuite il suit le journal toutes les secondes et demie, et enregistre l'index de temps en temps ;
//! 4. quand plus personne ne cherche depuis une minute et demie, l'index quitte la mémoire ; il y revient en une
//!    demi-seconde à la recherche suivante (étapes 1 et 2).

use super::{App, model::Item};
use crate::ui::Sender;
use fex_core::{
    index::{self, DirSync, F_DELETED, F_DIR, F_HIDDEN, F_SYSTEM, Index, NO_PARENT, Progress},
    listing::{ATTR_DIRECTORY, ATTR_HIDDEN, ATTR_SYSTEM},
    query::{Clock, Found, Options, Order, Query},
    usn,
};
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives},
        System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL},
    },
    core::PCWSTR,
};

/// Nombre de résultats montrés dans la liste (le total, lui, est toujours compté).
pub const LIMIT: usize = 10_000;
/// Intervalle entre deux lectures du journal des modifications.
const WATCH_EVERY: Duration = Duration::from_millis(1500);
/// Dossiers remis à jour par passe (le verrou d'écriture sur l'index reste bref).
const SYNC_BATCH: usize = 256;
/// Sans recherche pendant ce temps, l'index quitte la mémoire.
const IDLE: Duration = Duration::from_secs(90);
/// Intervalle minimal entre deux enregistrements de l'index quand il change.
const SAVE_EVERY: Duration = Duration::from_secs(900);

pub struct SearchIndex {
    pub index: RwLock<Index>,
    /// L'index est prêt à répondre.
    ready: AtomicBool,
    pub progress: Progress,
    cancel: AtomicBool,
    /// Augmente à chaque changement de l'index : des résultats affichés peuvent alors être dépassés.
    pub version: AtomicU64,
    service: Mutex<Option<JoinHandle<()>>>,
    /// Dernière fois qu'une recherche a eu besoin de l'index.
    used: Mutex<Instant>,
    /// L'index a quitté la mémoire faute d'usage ; `wanted` demande son retour.
    asleep: AtomicBool,
    wanted: AtomicBool,
}

impl SearchIndex {
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    /// L'index a quitté la mémoire faute d'usage (il revient à la première recherche).
    pub fn sleeping(&self) -> bool {
        self.asleep.load(Ordering::Relaxed)
    }

    /// On cherche, ou on s'apprête à chercher : l'index doit être en mémoire, et y rester.
    pub fn touch(&self) {
        *self.used.lock().unwrap() = Instant::now();
        if self.sleeping() {
            self.wanted.store(true, Ordering::Relaxed);
        }
    }

    /// Arrête le fil de service et attend qu'il ait enregistré l'index (fermeture de l'application).
    pub fn shutdown(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(h) = self.service.lock().unwrap().take() {
            let _ = h.join();
        }
    }

    fn stopping(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Les disques à indexer : les disques fixes (pas les clés USB, les lecteurs réseau ni les CD).
/// La variable PLUME_FEX_INDEX_ROOTS les remplace par une liste de dossiers séparés par « ; » (essais) ;
/// vide, elle désactive l'index.
fn roots() -> Vec<PathBuf> {
    if let Some(v) = std::env::var_os("PLUME_FEX_INDEX_ROOTS") {
        return std::env::split_paths(&v).filter(|p| p.is_dir()).collect();
    }
    let mask = unsafe { GetLogicalDrives() };
    (0..26u32)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| format!("{}:\\", (b'A' + i as u8) as char))
        .filter(|root| {
            let w: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            // 3 = DRIVE_FIXED
            unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) == 3 }
        })
        .map(PathBuf::from)
        .collect()
}

// ───────────────────────── disques sans journal suivi ─────────────────────────

/// Disques que le journal ne suit pas : clés USB et cartes (DRIVE_REMOVABLE = 2), lecteurs réseau (DRIVE_REMOTE = 4).
/// Ils sont lus quand ils apparaissent, retirés quand ils disparaissent, et relus en entier de temps en temps.
/// PLUME_FEX_EXTRA_ROOTS (essais) : une liste de dossiers à la place ; avec PLUME_FEX_INDEX_ROOTS seul, aucun.
fn extra_roots() -> Vec<PathBuf> {
    if let Some(v) = std::env::var_os("PLUME_FEX_EXTRA_ROOTS") {
        return std::env::split_paths(&v).filter(|p| p.is_dir()).collect();
    }
    if std::env::var_os("PLUME_FEX_INDEX_ROOTS").is_some() {
        return vec![];
    }
    let mask = unsafe { GetLogicalDrives() };
    (0..26u32)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| format!("{}:\\", (b'A' + i as u8) as char))
        .filter(|root| {
            let w: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            matches!(unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) }, 2 | 4)
        })
        .map(PathBuf::from)
        // un lecteur de cartes vide, un partage injoignable : rien à lire
        .filter(|p| p.is_dir())
        .collect()
}

fn root_key(p: &Path) -> String {
    p.display().to_string().trim_end_matches('\\').to_lowercase()
}

fn ms_env(name: &str, default: Duration) -> Duration {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).map_or(default, Duration::from_millis)
}

/// Les disques sans journal que l'index contient déjà (ceux de la dernière fois) : (numéro de la racine, dernière lecture).
type Extras = std::collections::HashMap<String, (u32, Option<Instant>)>;

fn adopt_extras(s: &SearchIndex, plan: &Plan) -> Extras {
    let fixed: HashSet<String> = plan.roots.iter().map(|r| root_key(r)).collect();
    let ix = s.index.read().unwrap();
    ix.roots().into_iter().filter(|(_, name)| !fixed.contains(&name.to_lowercase())).map(|(id, name)| (name.to_lowercase(), (id, None))).collect()
}

/// Met l'index d'accord avec les disques sans journal présents. Vrai si quelque chose a changé.
fn follow_extras(s: &SearchIndex, extras: &mut Extras, every: Duration) -> bool {
    let present: std::collections::HashMap<String, PathBuf> = extra_roots().into_iter().map(|p| (root_key(&p), p)).collect();
    let mut changed = false;
    let gone: Vec<String> = extras.keys().filter(|k| !present.contains_key(*k)).cloned().collect();
    for k in gone {
        if let Some((id, _)) = extras.remove(&k) {
            s.index.write().unwrap().drop_root(id);
            changed = true;
        }
    }
    for (k, path) in present {
        let due = extras.get(&k).is_none_or(|(_, at)| at.is_none_or(|t| t.elapsed() >= every));
        if !due || s.stopping() {
            continue;
        }
        let id = match extras.get(&k) {
            Some((id, _)) => *id,
            None => {
                changed = true;
                s.index.write().unwrap().add_root(&path)
            }
        };
        let dirs = s.index.read().unwrap().dirs_under(id);
        changed |= index::walk(dirs, &mut |f| f(&mut s.index.write().unwrap()), &|| s.stopping());
        extras.insert(k, (id, Some(Instant::now())));
    }
    if changed {
        s.version.fetch_add(1, Ordering::Relaxed);
    }
    changed
}

/// Racine du volume d'un dossier indexé : « C:\ » pour « C:\Users\… ».
fn volume_of(root: &Path) -> PathBuf {
    let s = root.display().to_string();
    PathBuf::from(format!("{}\\", s.get(..2).unwrap_or("")))
}

/// Démarre le service de l'index. `dir` : dossier des données de Plume FEx (l'index y est enregistré).
/// `App::on_index_ready` est appelé dès qu'on peut chercher.
pub fn start(sender: Sender, dir: PathBuf) -> Arc<SearchIndex> {
    let shared = Arc::new(SearchIndex {
        index: RwLock::new(Index::default()),
        ready: AtomicBool::new(false),
        progress: Progress::default(),
        cancel: AtomicBool::new(false),
        version: AtomicU64::new(0),
        service: Mutex::new(None),
        used: Mutex::new(Instant::now()),
        asleep: AtomicBool::new(false),
        wanted: AtomicBool::new(false),
    });
    let s = shared.clone();
    let handle = std::thread::spawn(move || service(&s, sender, &dir));
    *shared.service.lock().unwrap() = Some(handle);
    shared
}

fn publish(s: &SearchIndex, ix: Index, sender: Sender) {
    *s.index.write().unwrap() = ix;
    s.ready.store(true, Ordering::Relaxed);
    s.version.fetch_add(1, Ordering::Relaxed);
    sender.send(|h, cx| {
        if let Some(app) = h.as_any().downcast_mut::<App>() {
            app.on_index_ready(cx);
        }
    });
}

fn save(s: &SearchIndex, file: &Path, roots: &[PathBuf], positions: &[Option<usn::Position>]) {
    let volumes: Vec<(String, u64, i64)> =
        roots.iter().zip(positions).map(|(r, p)| (r.display().to_string(), p.map_or(0, |p| p.journal), p.map_or(-1, |p| p.next))).collect();
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let ix = s.index.read().unwrap();
    let _ = ix.save(file, &volumes);
}

/// Reporte dans l'index ce que le journal du volume signale depuis `from`.
/// `Ok(None)` : la position n'est plus lisible, il faut reparcourir le disque.
fn catch_up(s: &SearchIndex, volume: &Path, from: usn::Position) -> std::io::Result<Option<(usn::Position, bool)>> {
    let mut touched = HashSet::new();
    let to = match usn::read(volume, from, &mut |parent| {
        touched.insert(parent);
    })? {
        usn::Read::Lost => return Ok(None),
        usn::Read::UpTo(p) => p,
    };
    if touched.is_empty() {
        return Ok(Some((to, false)));
    }
    // les dossiers touchés que l'index connaît ; les autres sont des dossiers nouveaux, que la relecture de leur
    // parent fera découvrir
    let mut todo: Vec<(u32, bool)> = {
        let ix = s.index.read().unwrap();
        touched.iter().filter_map(|f| ix.dir_ids.get(f).copied()).filter(|&d| ix.flags[d as usize] & F_DELETED == 0).map(|d| (d, false)).collect()
    };
    let mut changed = false;
    while !todo.is_empty() && !s.stopping() {
        let take = todo.len().min(SYNC_BATCH);
        // les dossiers sont lus sur le disque hors de tout verrou
        let paths: Vec<(u32, bool, PathBuf)> = {
            let ix = s.index.read().unwrap();
            todo.drain(..take).map(|(d, fresh)| (d, fresh, ix.path(d))).collect()
        };
        let mut denied = vec![];
        let batch: Vec<DirSync> = paths
            .into_iter()
            .filter_map(|(dir, fresh, path)| match index::list_for_sync(&path) {
                Ok(listing) => Some(DirSync { dir, listing, fresh }),
                Err(e) => {
                    // accès refusé : le dossier existe, mais on ne saura pas ce qu'il contient
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        denied.push(dir);
                    }
                    None
                }
            })
            .collect();
        let (new_dirs, c) = {
            let mut ix = s.index.write().unwrap();
            denied.into_iter().for_each(|d| ix.deny(d));
            ix.sync_dirs(&batch)
        };
        changed |= c;
        todo.extend(new_dirs.into_iter().map(|d| (d, true)));
    }
    if changed {
        s.version.fetch_add(1, Ordering::Relaxed);
    }
    Ok(Some((to, changed)))
}

/// Ce qu'il faut pour ouvrir et suivre l'index.
struct Plan {
    file: PathBuf,
    roots: Vec<PathBuf>,
    volumes: Vec<PathBuf>,
    names: Vec<String>,
    threads: usize,
}

/// Met l'index en mémoire : celui de la dernière fois, rattrapé par le journal, sinon un parcours complet.
/// Renvoie la position du journal de chaque volume, et dit si l'index diffère de celui du fichier.
/// None : arrêt demandé en cours de route.
fn open(s: &SearchIndex, plan: &Plan, sender: Sender) -> Option<(Vec<Option<usn::Position>>, bool)> {
    let mut positions: Vec<Option<usn::Position>> = vec![None; plan.roots.len()];
    let mut dirty = false;
    // 1. l'index de la dernière fois, s'il porte sur les mêmes disques et n'est pas encombré d'éléments supprimés
    let mut current = false;
    if let Ok((ix, saved)) = Index::load(&plan.file)
        && saved.iter().map(|v| &v.0).eq(plan.names.iter())
        && ix.deleted() * 5 < ix.len().max(1)
    {
        positions = saved.iter().map(|(_, journal, next)| (*next >= 0).then_some(usn::Position { journal: *journal, next: *next })).collect();
        publish(s, ix, sender);
        current = true;
        for (position, volume) in positions.iter_mut().zip(&plan.volumes) {
            let caught = position.and_then(|from| catch_up(s, volume, from).ok().flatten());
            match caught {
                Some((to, changed)) => {
                    *position = Some(to);
                    dirty |= changed;
                }
                // pas de journal, ou il ne remonte plus jusqu'à notre dernière lecture : on ne sait pas ce qui a changé
                None => current = false,
            }
        }
    }
    // 2. sinon, parcours complet des disques ; la position du journal est notée avant, pour rattraper ensuite
    //    ce qui aura changé pendant le parcours
    if !current && !s.stopping() {
        let before: Vec<Option<usn::Position>> = plan.volumes.iter().map(|v| usn::position(v).ok()).collect();
        let mut ix = index::build(&plan.roots, plan.threads, &s.progress, &s.cancel);
        if s.stopping() {
            return None;
        }
        ix.compute_rank(plan.threads);
        publish(s, ix, sender);
        positions = before;
        for (position, volume) in positions.iter_mut().zip(&plan.volumes) {
            *position = position.and_then(|from| catch_up(s, volume, from).ok().flatten()).map(|(to, _)| to);
        }
        save(s, &plan.file, &plan.roots, &positions);
        dirty = false;
    }
    (!s.stopping()).then_some((positions, dirty))
}

fn service(s: &SearchIndex, sender: Sender, dir: &Path) {
    let roots = roots();
    if roots.is_empty() {
        return;
    }
    // en retrait : l'indexation ne doit pas gêner ce que l'utilisateur fait pendant ce temps
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
    let plan = Plan {
        file: dir.join("index.bin"),
        volumes: roots.iter().map(|r| volume_of(r)).collect(),
        names: roots.iter().map(|r| r.display().to_string()).collect(),
        threads: std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 12),
        roots,
    };
    // PLUME_FEX_INDEX_IDLE_MS : délai de mise en sommeil (essais)
    let idle = std::env::var("PLUME_FEX_INDEX_IDLE_MS").ok().and_then(|v| v.parse().ok()).map_or(IDLE, Duration::from_millis);
    let Some((mut positions, mut dirty)) = open(s, &plan, sender) else { return };
    s.touch();
    let (mut last_save, mut last_watch) = (Instant::now(), Instant::now());
    // clés USB, cartes, lecteurs réseau : regardés toutes les 10 s, relus en entier toutes les 15 min
    let (extra_check, extra_every) = (ms_env("PLUME_FEX_EXTRA_CHECK_MS", Duration::from_secs(10)), ms_env("PLUME_FEX_EXTRA_EVERY_MS", Duration::from_secs(15 * 60)));
    let mut extras = adopt_extras(s, &plan);
    dirty |= follow_extras(s, &mut extras, extra_every);
    let mut last_extra = Instant::now();
    while !s.stopping() {
        std::thread::sleep(Duration::from_millis(100));
        // en sommeil : l'index n'est plus en mémoire ; il y revient (en une demi-seconde) dès qu'on veut chercher
        if s.sleeping() {
            if s.wanted.swap(false, Ordering::Relaxed) {
                match open(s, &plan, sender) {
                    Some((p, d)) => (positions, dirty) = (p, d),
                    None => return,
                }
                extras = adopt_extras(s, &plan);
                dirty |= follow_extras(s, &mut extras, extra_every);
                last_extra = Instant::now();
                s.asleep.store(false, Ordering::Relaxed);
                s.touch();
                last_watch = Instant::now();
            }
            continue;
        }
        if last_extra.elapsed() >= extra_check {
            last_extra = Instant::now();
            dirty |= follow_extras(s, &mut extras, extra_every);
        }
        // veille : le journal dit quels dossiers ont changé
        if last_watch.elapsed() >= WATCH_EVERY {
            last_watch = Instant::now();
            for (position, volume) in positions.iter_mut().zip(&plan.volumes) {
                if let Some(from) = *position
                    && !s.stopping()
                {
                    match catch_up(s, volume, from) {
                        Ok(Some((to, changed))) => {
                            *position = Some(to);
                            dirty |= changed;
                        }
                        // position perdue en cours de route (journal recréé) : ce volume ne sera plus suivi d'ici
                        // la prochaine ouverture de l'index, qui reparcourra le disque
                        Ok(None) => *position = None,
                        Err(_) => {}
                    }
                }
            }
        }
        // L'enregistrement (des dizaines de mégaoctets) n'a pas lieu à chaque changement : le fichier peut avoir
        // un peu de retard, le journal le rattrape à la prochaine ouverture.
        let stale = dirty && last_save.elapsed() > SAVE_EVERY;
        let unused = s.used.lock().unwrap().elapsed() > idle;
        if stale {
            save(s, &plan.file, &plan.roots, &positions);
            dirty = false;
            last_save = Instant::now();
        }
        // personne ne cherche plus depuis un moment : l'index quitte la mémoire
        if unused && !s.stopping() {
            s.ready.store(false, Ordering::Relaxed);
            s.asleep.store(true, Ordering::Relaxed);
            *s.index.write().unwrap() = Index::default();
        }
    }
    if dirty && !s.sleeping() && last_save.elapsed() > Duration::from_secs(120) {
        save(s, &plan.file, &plan.roots, &positions);
    }
}

pub struct Request {
    pub tab: u64,
    pub generation: u64,
    pub query: String,
    /// Dossier où chercher (None : partout).
    pub scope: Option<PathBuf>,
    pub order: Order,
    pub descending: bool,
    pub cancel: Arc<AtomicBool>,
    /// Réglages choisis dans le bandeau de recherche (type d'éléments, façon de comparer).
    pub options: Options,
}

pub struct Results {
    pub tab: u64,
    pub generation: u64,
    pub items: Vec<Item>,
    pub total: usize,
    pub took_ms: f32,
    /// Version de l'index au moment de la recherche.
    pub version: u64,
}

/// Les numéros des éléments trouvés dans l'index (déjà prêt), `limit` au plus, et leur nombre total.
fn find(ix: &Index, req: &Request, limit: usize) -> Found {
    let query = Query::parse_with(&req.query, &Clock::system(), &req.options);
    // un dossier de départ que l'index ne connaît pas (lecteur non indexé) : aucun résultat plutôt que tout le disque
    let scope = match &req.scope {
        Some(p) => match ix.find(p) {
            Some(id) => Some(id),
            None => return Found::default(),
        },
        None => None,
    };
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
    ix.search(&query, scope, req.order, req.descending, limit, threads, &req.cancel)
}

/// Cherche dans l'index (déjà prêt) et fabrique les lignes à afficher.
pub fn search(ix: &Index, req: &Request) -> (Vec<Item>, usize) {
    let found = find(ix, req, LIMIT);
    let mut folders: HashMap<u32, Arc<Path>> = HashMap::new();
    let items = found
        .ids
        .iter()
        .filter(|&&i| ix.parent[i as usize] != NO_PARENT)
        .map(|&i| {
            let f = ix.flags[i as usize];
            let parent = ix.parent[i as usize];
            let folder = folders.entry(parent).or_insert_with(|| Arc::from(ix.path(parent))).clone();
            let mut attrs = if f & F_DIR != 0 { ATTR_DIRECTORY } else { 0x20 };
            if f & F_HIDDEN != 0 {
                attrs |= ATTR_HIDDEN;
            }
            if f & F_SYSTEM != 0 {
                attrs |= ATTR_SYSTEM;
            }
            Item {
                name: ix.name(i).to_string(),
                attrs,
                size: ix.size_of(i),
                sized: false,
                mtime: ix.mtime_of(i),
                ctime: 0,
                sel: false,
                icon: Cell::new(-1),
                asked: Cell::new(false),
                drive: None,
                folder: Some(folder),
            }
        })
        .collect();
    (items, found.total)
}

/// La taille de chaque sous-dossier de `dir`, d'après l'index : (nom, octets). None : ce dossier n'est pas dans
/// l'index. `wake` : réveiller l'index s'il dort (sinon, on ne répond que s'il est déjà en mémoire).
pub fn folder_sizes(shared: &SearchIndex, dir: &Path, wake_up: bool) -> Option<Vec<(String, u64)>> {
    if wake_up {
        if roots().is_empty() {
            return None;
        }
        wake(shared, &shared.cancel);
    }
    if !shared.is_ready() {
        return None;
    }
    let ix = shared.index.read().unwrap();
    let id = ix.find(dir)?;
    Some(ix.folder_sizes(id).into_iter().map(|(i, size)| (ix.name(i).to_string(), size)).collect())
}

/// L'index dormait peut-être : on le réveille et on l'attend (une demi-seconde, en général).
fn wake(shared: &SearchIndex, cancel: &AtomicBool) {
    shared.touch();
    let waited = Instant::now();
    while !shared.is_ready() && !cancel.load(Ordering::Relaxed) && waited.elapsed() < Duration::from_secs(120) {
        shared.touch();
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Écrit tous les résultats d'une recherche (pas seulement ceux que la liste affiche) dans un fichier CSV, avec
/// les colonnes d'Everything : nom, dossier, taille, date de modification. Renvoie le nombre de lignes écrites.
pub fn export(shared: &SearchIndex, req: &Request, file: &Path) -> io::Result<usize> {
    wake(shared, &req.cancel);
    let ix = shared.index.read().unwrap();
    let found = find(&ix, req, usize::MAX);
    let mut out = io::BufWriter::new(std::fs::File::create(file)?);
    // la marque en tête dit aux tableurs que le fichier est en UTF-8
    out.write_all("\u{feff}Name,Path,Size,Date Modified\r\n".as_bytes())?;
    let quoted = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut folders: HashMap<u32, String> = HashMap::new();
    let mut rows = 0;
    for &i in found.ids.iter().filter(|&&i| ix.parent[i as usize] != NO_PARENT) {
        let parent = ix.parent[i as usize];
        let folder = folders.entry(parent).or_insert_with(|| quoted(&ix.path(parent).display().to_string()));
        let size = if ix.is_dir(i) { String::new() } else { ix.size_of(i).to_string() };
        write!(out, "{},{},{},{}\r\n", quoted(ix.name(i)), folder, size, super::format::iso(ix.mtime_of(i)))?;
        rows += 1;
    }
    out.flush()?;
    Ok(rows)
}

/// Lance une recherche sur un autre fil ; `App::on_search` reçoit les résultats.
pub fn spawn(shared: Arc<SearchIndex>, sender: Sender, req: Request) {
    std::thread::spawn(move || {
        wake(&shared, &req.cancel);
        let t0 = Instant::now();
        let version = shared.version.load(Ordering::Relaxed);
        let (items, total) = {
            let ix = shared.index.read().unwrap();
            search(&ix, &req)
        };
        if req.cancel.load(Ordering::Relaxed) {
            return;
        }
        let results = Results { tab: req.tab, generation: req.generation, items, total, took_ms: t0.elapsed().as_secs_f32() * 1000.0, version };
        sender.send(move |h, cx| {
            if let Some(app) = h.as_any().downcast_mut::<App>() {
                app.on_search(results, cx);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_become_rows_with_their_folder() {
        let mut ix = Index::default();
        let c = ix.push("C:", NO_PARENT, 0, 0, F_DIR);
        // 2026-11-10 12:00 UTC en FILETIME
        const T: u64 = 134_387_856_000_000_000;
        let docs = ix.push("Docs", c, 0, T, F_DIR);
        ix.push("plan.txt", docs, 120, T, 0);
        ix.push("Plans", docs, 0, T, F_DIR | F_HIDDEN);
        ix.push("other.md", c, 1, 1, 0);
        ix.compute_rank(2);
        let req = |q: &str, scope: Option<&str>| Request {
            tab: 1,
            generation: 1,
            query: q.into(),
            scope: scope.map(PathBuf::from),
            order: Order::Name,
            descending: false,
            cancel: Arc::new(AtomicBool::new(false)),
            options: Options::default(),
        };
        let (items, total) = search(&ix, &req("plan", None));
        assert_eq!(total, 2);
        assert_eq!(items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["plan.txt", "Plans"]);
        assert_eq!(items[0].folder.as_deref(), Some(Path::new(r"C:\Docs")));
        assert_eq!((items[0].size, items[0].mtime, items[0].is_dir()), (120, T, false));
        assert!(items[1].is_dir() && items[1].is_hidden());
        // dans un dossier ; dans un dossier inconnu de l'index ; la racine elle-même n'est jamais un résultat
        assert_eq!(search(&ix, &req("o", Some(r"C:\Docs"))).1, 0);
        assert_eq!(search(&ix, &req("md", Some(r"C:\"))).1, 1);
        assert_eq!(search(&ix, &req("plan", Some(r"D:\Nope"))).1, 0);
        assert!(search(&ix, &req("c:", None)).0.is_empty());
    }

    #[test]
    fn volumes() {
        assert_eq!(volume_of(Path::new(r"C:\")), PathBuf::from(r"C:\"));
        assert_eq!(volume_of(Path::new(r"d:\Temp\fixture")), PathBuf::from(r"d:\"));
    }
}
