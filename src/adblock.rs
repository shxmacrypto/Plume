//! Bloqueur de pubs / traqueurs : ~3 600 domaines (liste Peter Lowe + ajouts), recherche
//! par suffixe dans un HashSet (quelques nanosecondes par requête), liste blanche par site.
use std::collections::HashSet;
use std::sync::{
    OnceLock, RwLock,
    atomic::{AtomicBool, Ordering},
};

static LIST: &str = include_str!("../assets/adlist.txt");
static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
pub static ON: AtomicBool = AtomicBool::new(true);
/// Sites où l'utilisateur a désactivé le bloqueur.
pub static ALLOW: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Le domaine (ou un domaine parent) est-il dans la liste ?
pub fn listed(host: &str) -> bool {
    if !ON.load(Ordering::Relaxed) {
        return false;
    }
    let set = SET.get_or_init(|| LIST.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect());
    let mut h = host;
    loop {
        if set.contains(h) {
            return true;
        }
        match h.split_once('.') {
            Some((_, rest)) if rest.contains('.') => h = rest,
            _ => return false,
        }
    }
}

/// Le bloqueur est-il désactivé sur ce site ?
pub fn allowed(page_host: &str) -> bool {
    ALLOW.read().is_ok_and(|a| a.iter().any(|s| s == page_host))
}

pub fn count() -> usize {
    LIST.lines().filter(|l| !l.trim().is_empty()).count()
}
