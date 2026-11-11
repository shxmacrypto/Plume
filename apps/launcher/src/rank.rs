//! What a result is, how well it matches what is typed, and what was picked before (frecency: often and lately
//! picked results come first, and a query remembers the result picked for it). Pure logic, tested below.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Kind {
    App,
    File,
    Folder,
    Setting,
    Calc,
    Web,
}

/// One line of the results.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub kind: Kind,
    pub title: String,
    /// The line under the title: a folder, a description.
    pub sub: String,
    /// What Enter opens: a shortcut or file path, "shell:AppsFolder\…", an "ms-settings:" address, the value to copy
    /// (calculator), the search address (web).
    pub target: String,
    /// What the shell draws the icon of (a path or a shell parsing name); empty: one of Plume's own icons.
    pub icon: String,
}

impl Item {
    /// The key of a result in the history.
    pub fn key(&self) -> String {
        format!("{:?}:{}", self.kind, self.target.to_lowercase())
    }
    /// Remembered in the history? (A calculation or a web search is not a place to come back to.)
    pub fn remembered(&self) -> bool {
        !matches!(self.kind, Kind::Calc | Kind::Web)
    }
}

/// Lower case, no accents: how names and queries are compared (the same folding as Plume FEx's search).
pub fn fold(s: &str) -> String {
    String::from_utf8_lossy(&fex_core::query::plain(s)).into_owned()
}

fn words(name: &str) -> Vec<&str> {
    name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect()
}

/// How well `name` matches `q` (both folded). None: not at all. Higher is better (up to 1000).
pub fn score(q: &str, name: &str) -> Option<i32> {
    let q = q.trim();
    if q.is_empty() {
        return Some(0);
    }
    if name.is_empty() {
        return None;
    }
    let extra = (name.chars().count() as i32 - q.chars().count() as i32).clamp(0, 60);
    if name == q {
        return Some(1000);
    }
    if name.starts_with(q) {
        return Some(900 - extra);
    }
    let ws = words(name);
    let qs = words(q);
    // a word of the name starts with what is typed: "code" → "Visual Studio Code"
    if let Some(i) = ws.iter().position(|w| w.starts_with(q)) {
        return Some(820 - i as i32 * 6 - extra / 2);
    }
    // several words typed, each starting a word of the name, in any order: "studio vis"
    if qs.len() > 1 && qs.iter().all(|q| ws.iter().any(|w| w.starts_with(q))) {
        return Some(780 - extra / 2);
    }
    // initials: "vsc" → "Visual Studio Code"
    let initials: String = ws.iter().filter_map(|w| w.chars().next()).collect();
    if q.chars().count() >= 2 && !q.contains(' ') && initials.starts_with(q) {
        return Some(720 - (initials.len() as i32 - q.len() as i32).clamp(0, 20));
    }
    if let Some(pos) = name.find(q) {
        return Some(600 - (pos as i32).min(80) - extra / 3);
    }
    if qs.len() > 1 && qs.iter().all(|q| name.contains(q)) {
        return Some(520 - extra / 3);
    }
    // the letters, in order, close together, from the start of a word: "ntpd" → "notepad"
    let qc: Vec<char> = q.chars().filter(|c| !c.is_whitespace()).collect();
    if qc.len() < 3 {
        return None;
    }
    let nc: Vec<char> = name.chars().collect();
    let mut best = None;
    for start in 0..nc.len() {
        if nc[start] != qc[0] || (start > 0 && nc[start - 1].is_alphanumeric()) {
            continue;
        }
        let (mut i, mut last, mut runs) = (1usize, start, 0i32);
        for (k, c) in nc.iter().enumerate().skip(start + 1) {
            if i == qc.len() {
                break;
            }
            if *c == qc[i] {
                if last + 1 == k {
                    runs += 1;
                }
                last = k;
                i += 1;
            }
        }
        let span = last - start + 1;
        if i < qc.len() || span > qc.len() * 2 {
            continue;
        }
        best = best.max(Some(300 + runs * 12 - (span - qc.len()) as i32 * 4 - (start as i32).min(40)));
    }
    best
}

/// A little preference between kinds when two results match as well.
pub fn kind_bonus(k: Kind) -> i32 {
    match k {
        Kind::App => 60,
        Kind::Setting => 10,
        Kind::Folder => -10,
        Kind::File => -30,
        Kind::Calc | Kind::Web => 0,
    }
}

// ───────────────────────── history ─────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pick {
    pub count: u32,
    /// Seconds since 1970.
    pub last: u64,
    pub item: Item,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct History {
    pub picks: HashMap<String, Pick>,
    /// The result picked for a query (folded), the last time it was typed.
    pub queries: HashMap<String, String>,
}

const MAX_PICKS: usize = 400;
const MAX_QUERIES: usize = 600;

impl History {
    /// How often and how lately a result was picked (0: never).
    pub fn frecency(&self, key: &str, now: u64) -> f32 {
        let Some(p) = self.picks.get(key) else { return 0.0 };
        let days = now.saturating_sub(p.last) / 86_400;
        let weight = match days {
            0 => 1.0,
            1..=3 => 0.8,
            4..=13 => 0.6,
            14..=30 => 0.4,
            31..=90 => 0.2,
            _ => 0.1,
        };
        p.count.min(40) as f32 * weight
    }

    /// What the history adds to a result's score for the query `q` (folded).
    pub fn boost(&self, q: &str, key: &str, now: u64) -> i32 {
        let mut b = (self.frecency(key, now) * 18.0).min(320.0) as i32;
        let q = q.trim();
        if !q.is_empty() {
            if self.queries.get(q).is_some_and(|k| k == key) {
                b += 450;
            } else if q.len() >= 2 && self.queries.iter().any(|(typed, k)| k == key && typed.starts_with(q)) {
                b += 180;
            }
        }
        b
    }

    /// The result was opened after typing `q`.
    pub fn record(&mut self, q: &str, item: &Item, now: u64) {
        if !item.remembered() {
            return;
        }
        let key = item.key();
        let p = self.picks.entry(key.clone()).or_insert(Pick { count: 0, last: now, item: item.clone() });
        p.count = p.count.saturating_add(1);
        p.last = now;
        p.item = item.clone();
        let q = fold(q.trim());
        if !q.is_empty() {
            self.queries.insert(q, key);
        }
        if self.picks.len() > MAX_PICKS {
            // the weakest leave first
            let mut keys: Vec<(f32, u64, String)> = self.picks.iter().map(|(k, p)| (self.frecency(k, now), p.last, k.clone())).collect();
            keys.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            for (_, _, k) in keys.into_iter().take(self.picks.len() - MAX_PICKS) {
                self.picks.remove(&k);
            }
            self.queries.retain(|_, k| self.picks.contains_key(k));
        }
        if self.queries.len() > MAX_QUERIES {
            let picks = &self.picks;
            let mut qs: Vec<(u64, String)> = self.queries.iter().map(|(q, k)| (picks.get(k).map_or(0, |p| p.last), q.clone())).collect();
            qs.sort();
            for (_, q) in qs.into_iter().take(self.queries.len() - MAX_QUERIES) {
                self.queries.remove(&q);
            }
        }
    }

    /// Forgets a result (it no longer exists).
    pub fn forget(&mut self, key: &str) {
        self.picks.remove(key);
        self.queries.retain(|_, k| k != key);
    }

    /// The results picked most often and lately, for the empty box.
    pub fn recent(&self, n: usize, now: u64) -> Vec<Item> {
        let mut v: Vec<(&String, &Pick)> = self.picks.iter().collect();
        v.sort_by(|a, b| self.frecency(b.0, now).total_cmp(&self.frecency(a.0, now)).then(b.1.last.cmp(&a.1.last)));
        v.into_iter().take(n).map(|(_, p)| p.item.clone()).collect()
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(q: &str, name: &str) -> Option<i32> {
        score(&fold(q), &fold(name))
    }

    fn item(kind: Kind, title: &str, target: &str) -> Item {
        Item { kind, title: title.into(), sub: String::new(), target: target.into(), icon: String::new() }
    }

    #[test]
    fn match_order() {
        let exact = s("code", "code").unwrap();
        let prefix = s("note", "Notepad").unwrap();
        let word = s("code", "Visual Studio Code").unwrap();
        let initials = s("vsc", "Visual Studio Code").unwrap();
        let inside = s("pad", "Notepad").unwrap();
        let fuzzy = s("ntpd", "Notepad").unwrap();
        assert!(
            exact > prefix && prefix > word && word > initials && initials > inside && inside > fuzzy,
            "{exact} {prefix} {word} {initials} {inside} {fuzzy}"
        );
        // a shorter name wins among prefixes
        assert!(s("pho", "Photos").unwrap() > s("pho", "Photoshop Elements Organizer").unwrap());
    }

    #[test]
    fn accents_and_case() {
        assert!(s("parametres", "Paramètres").is_some());
        assert!(s("ÉCRAN", "ecran").is_some());
        assert_eq!(s("", "anything"), Some(0));
    }

    #[test]
    fn no_match() {
        assert_eq!(s("xyz", "Notepad"), None);
        assert_eq!(s("zz", "Zoom Zone"), Some(720));
        // letters in order and close together match; too far apart, they do not
        assert!(s("ntpd", "Notepad").is_some());
        assert_eq!(s("nad", "Notepad and something else entirely"), None);
        // from the start of a word only: "bud" is not in "OBS Studio", "calc" not in "Minecraft Launcher"
        assert_eq!(s("bud", "OBS Studio"), None);
        assert_eq!(s("calc", "Minecraft Launcher"), None);
        assert_eq!(s("note", "Quelles sont les nouveautés"), None);
        assert_eq!(s("ntd", "nxxxxxxxxxxxxxxxxxxxxxxtxxxxxxxxxxxxxd"), None);
    }

    #[test]
    fn several_words() {
        assert!(s("studio vis", "Visual Studio Code").is_some());
        assert!(s("visual code", "Visual Studio Code").unwrap() > s("isu tud", "Visual Studio Code").unwrap_or(0));
    }

    #[test]
    fn frecency_and_queries() {
        let mut h = History::default();
        let now = 1_000_000_000;
        let a = item(Kind::App, "Firefox", "c:\\firefox.lnk");
        let b = item(Kind::App, "File Explorer", "shell:appsfolder\\explorer");
        assert_eq!(h.boost("f", &a.key(), now), 0);
        h.record("f", &a, now);
        h.record("fi", &a, now);
        h.record("fil", &b, now);
        // picked twice beats picked once
        assert!(h.frecency(&a.key(), now) > h.frecency(&b.key(), now));
        // the query remembers its pick
        assert!(h.boost("fil", &b.key(), now) > h.boost("fil", &a.key(), now));
        // and a shorter query typed on the way finds it too
        assert!(h.boost("fi", &a.key(), now) >= 450);
        // old picks weigh less
        assert!(h.frecency(&a.key(), now + 100 * 86_400) < h.frecency(&a.key(), now));
        assert_eq!(h.recent(5, now).first().map(|i| i.title.as_str()), Some("Firefox"));
        // calculations are not remembered
        h.record("2+2", &item(Kind::Calc, "4", "4"), now);
        assert_eq!(h.picks.len(), 2);
        h.forget(&a.key());
        assert_eq!(h.recent(5, now).len(), 1);
        assert!(!h.queries.values().any(|k| *k == a.key()));
    }

    #[test]
    fn history_stays_small() {
        let mut h = History::default();
        for i in 0..(MAX_PICKS + 20) {
            h.record(&format!("q{i}"), &item(Kind::File, "f", &format!("c:\\f{i}")), 1000 + i as u64);
        }
        assert_eq!(h.picks.len(), MAX_PICKS);
        assert!(h.queries.len() <= MAX_PICKS);
        // the oldest left
        assert!(!h.picks.contains_key("File:c:\\f0"));
    }
}
