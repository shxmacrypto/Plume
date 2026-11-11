//! The history: what was copied (newest first), the pinned items and the snippets. Kept in
//! %LOCALAPPDATA%\Plume\Clipboard (PLUME_CLIPBOARD_DIR for tests): `history.json`, or `history.dat` when the history
//! is encrypted (sealed by Windows for this user, DPAPI), and the pictures in `images\`.

use crate::crypto;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Text,
    Link,
    Image,
    Files,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub id: u64,
    pub kind: Kind,
    /// The text, the link, or the files' paths (one per line). Empty for a picture.
    pub text: String,
    /// The same text with its formatting, when the app gave it (pasted unless "plain text" is asked).
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub files: Vec<String>,
    /// A picture's size in pixels, and its PNG's size in bytes.
    pub w: u32,
    pub h: u32,
    pub bytes: u64,
    /// Identifies the content: the same copy twice is one item, moved to the top.
    pub hash: u64,
    /// Last copied (seconds since 1970).
    pub at: u64,
    /// The program it was copied from (its exe name).
    pub app: String,
    /// Pinned: its place among the pins (kept forever).
    pub pin: Option<u32>,
    pub count: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snippet {
    pub id: u64,
    pub name: String,
    /// Typed in the panel to find it, like ";sig".
    pub trigger: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Text,
    Links,
    Images,
    Files,
    Pinned,
}

/// A line of the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Snippet(u64),
    Item(u64),
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub items: Vec<Item>,
    pub snippets: Vec<Snippet>,
    pub next_id: u64,
}

/// FNV-1a: a hash that stays the same from one version of Plume to the next (it is saved).
pub fn hash(kind: Kind, data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ kind as u64;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_CLIPBOARD_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Plume").join("Clipboard")
}

/// Lowercase words of a search.
fn words(q: &str) -> Vec<String> {
    q.split_whitespace().map(str::to_lowercase).collect()
}

impl Item {
    pub fn text(text: &str, kind: Kind) -> Item {
        Item { kind, text: text.to_string(), hash: hash(kind, text.as_bytes()), count: 1, ..Default::default() }
    }

    pub fn files(files: Vec<String>) -> Item {
        let text = files.join("\n");
        Item { kind: Kind::Files, hash: hash(Kind::Files, text.to_lowercase().as_bytes()), text, files, count: 1, ..Default::default() }
    }

    pub fn image(w: u32, h: u32, pixels_hash: u64, bytes: u64) -> Item {
        Item { kind: Kind::Image, w, h, bytes, hash: pixels_hash, count: 1, ..Default::default() }
    }

    fn haystack(&self) -> String {
        let mut s = match self.kind {
            Kind::Image => format!("image picture {}x{} {}×{}", self.w, self.h, self.w, self.h),
            _ => self.text.clone(),
        };
        s.push('\n');
        s.push_str(&self.app);
        s.to_lowercase()
    }

    pub fn matches(&self, words: &[String]) -> bool {
        if words.is_empty() {
            return true;
        }
        let hay = self.haystack();
        words.iter().all(|w| hay.contains(w.as_str()))
    }

    fn fits(&self, f: Filter) -> bool {
        match f {
            Filter::All => true,
            Filter::Text => self.kind == Kind::Text,
            Filter::Links => self.kind == Kind::Link,
            Filter::Images => self.kind == Kind::Image,
            Filter::Files => self.kind == Kind::Files,
            Filter::Pinned => self.pin.is_some(),
        }
    }

    /// One line to show: the text with its spaces and lines folded, or the files' names.
    pub fn preview(&self, max: usize) -> String {
        let s = match self.kind {
            Kind::Files => {
                self.files.iter().map(|f| Path::new(f).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(f.clone())).collect::<Vec<_>>().join(", ")
            }
            Kind::Image => format!("{} × {}", self.w, self.h),
            _ => self.text.split_whitespace().collect::<Vec<_>>().join(" "),
        };
        if s.chars().count() > max { s.chars().take(max).collect::<String>() + "…" } else { s }
    }
}

impl Snippet {
    fn matches(&self, q: &str) -> bool {
        let q = q.trim().to_lowercase();
        if q.is_empty() {
            return false;
        }
        let t = self.trigger.to_lowercase();
        (!t.is_empty() && t.starts_with(&q)) || self.name.to_lowercase().contains(&q) || (q.len() >= 3 && self.text.to_lowercase().contains(&q))
    }
}

/// A trigger as it is kept: no spaces.
pub fn clean_trigger(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}

impl Store {
    pub fn get(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn snippet(&self, id: u64) -> Option<&Snippet> {
        self.snippets.iter().find(|s| s.id == id)
    }

    fn new_id(&mut self) -> u64 {
        self.next_id = self.next_id.max(self.items.iter().map(|i| i.id).chain(self.snippets.iter().map(|s| s.id)).max().unwrap_or(0)) + 1;
        self.next_id
    }

    /// Adds a copy. The same content copied again is moved to the top instead. Returns its id and whether it is new.
    pub fn add(&mut self, mut it: Item, now: u64) -> (u64, bool) {
        if let Some(pos) = self.items.iter().position(|i| i.hash == it.hash && i.kind == it.kind) {
            let mut old = self.items.remove(pos);
            old.at = now;
            old.count += 1;
            if !it.app.is_empty() {
                old.app = it.app;
            }
            // the newest formatting wins (the same text copied from another app)
            if it.html.is_some() || it.rtf.is_some() {
                old.html = it.html;
                old.rtf = it.rtf;
            }
            let id = old.id;
            self.items.insert(0, old);
            return (id, false);
        }
        it.id = self.new_id();
        it.at = now;
        it.count = it.count.max(1);
        let id = it.id;
        self.items.insert(0, it);
        (id, true)
    }

    /// Moves an item to the top (picked from the panel: it was just used).
    pub fn touch(&mut self, id: u64, now: u64) {
        if let Some(pos) = self.items.iter().position(|i| i.id == id) {
            let mut it = self.items.remove(pos);
            it.at = now;
            self.items.insert(0, it);
        }
    }

    /// Applies the limits: at most `max` items that are not pinned, none older than `days` (0: no limit).
    /// Returns what was removed (their pictures are to be deleted).
    pub fn prune(&mut self, max: usize, days: u32, now: u64) -> Vec<Item> {
        let mut removed = Vec::new();
        let oldest = if days == 0 { 0 } else { now.saturating_sub(days as u64 * 86_400) };
        let mut kept = 0usize;
        let mut keep = Vec::with_capacity(self.items.len());
        for it in std::mem::take(&mut self.items) {
            if it.pin.is_some() {
                keep.push(it);
            } else if kept < max && it.at >= oldest {
                kept += 1;
                keep.push(it);
            } else {
                removed.push(it);
            }
        }
        self.items = keep;
        removed
    }

    pub fn remove(&mut self, id: u64) -> Option<Item> {
        let pos = self.items.iter().position(|i| i.id == id)?;
        Some(self.items.remove(pos))
    }

    /// Empties the history; the pins stay.
    pub fn clear(&mut self) -> Vec<Item> {
        let (pins, rest): (Vec<Item>, Vec<Item>) = std::mem::take(&mut self.items).into_iter().partition(|i| i.pin.is_some());
        self.items = pins;
        rest
    }

    pub fn set_pin(&mut self, id: u64, on: bool) {
        let next = self.items.iter().filter_map(|i| i.pin).max().map_or(0, |m| m + 1);
        if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
            it.pin = if on { Some(it.pin.unwrap_or(next)) } else { None };
        }
    }

    /// The pins in their order.
    pub fn pins(&self) -> Vec<u64> {
        let mut p: Vec<(u32, u64)> = self.items.iter().filter_map(|i| i.pin.map(|o| (o, i.id))).collect();
        p.sort();
        p.into_iter().map(|(_, id)| id).collect()
    }

    /// Moves a pin up (`-1`) or down (`+1`) among the pins.
    pub fn move_pin(&mut self, id: u64, delta: i32) {
        let mut order = self.pins();
        let Some(pos) = order.iter().position(|x| *x == id) else { return };
        let to = pos as i32 + delta;
        if to < 0 || to >= order.len() as i32 {
            return;
        }
        order.swap(pos, to as usize);
        for (n, pid) in order.iter().enumerate() {
            if let Some(it) = self.items.iter_mut().find(|i| i.id == *pid) {
                it.pin = Some(n as u32);
            }
        }
    }

    /// The items of a page of the window: newest first, or the pins in their order.
    pub fn search(&self, q: &str, f: Filter) -> Vec<u64> {
        let w = words(q);
        if f == Filter::Pinned {
            return self.pins().into_iter().filter(|id| self.get(*id).is_some_and(|i| i.matches(&w))).collect();
        }
        self.items.iter().filter(|i| i.fits(f) && i.matches(&w)).map(|i| i.id).collect()
    }

    /// The lines of the panel: the snippets whose trigger or name match (an exact trigger first), then the pins,
    /// then the rest of the history.
    pub fn panel(&self, q: &str) -> Vec<Entry> {
        let mut out = Vec::new();
        let ql = q.trim().to_lowercase();
        let mut snips: Vec<&Snippet> = self.snippets.iter().filter(|s| s.matches(q)).collect();
        snips.sort_by_key(|s| (s.trigger.to_lowercase() != ql, s.name.to_lowercase()));
        out.extend(snips.into_iter().map(|s| Entry::Snippet(s.id)));
        // a trigger typed in full: the search is the trigger, not words to find in the history
        if ql.starts_with(';') && out.iter().any(|e| matches!(e, Entry::Snippet(id) if self.snippet(*id).is_some_and(|s| s.trigger.to_lowercase() == ql))) {
            return out;
        }
        let w = words(q);
        for id in self.pins() {
            if self.get(id).is_some_and(|i| i.matches(&w)) {
                out.push(Entry::Item(id));
            }
        }
        out.extend(self.items.iter().filter(|i| i.pin.is_none() && i.matches(&w)).map(|i| Entry::Item(i.id)));
        out
    }

    pub fn save_snippet(&mut self, id: Option<u64>, name: &str, trigger: &str, text: &str) -> u64 {
        let trigger = clean_trigger(trigger);
        match id.and_then(|id| self.snippets.iter().position(|s| s.id == id)) {
            Some(pos) => {
                let s = &mut self.snippets[pos];
                s.name = name.trim().to_string();
                s.trigger = trigger;
                s.text = text.to_string();
                s.id
            }
            None => {
                let id = self.new_id();
                self.snippets.push(Snippet { id, name: name.trim().to_string(), trigger, text: text.to_string() });
                id
            }
        }
    }

    pub fn remove_snippet(&mut self, id: u64) {
        self.snippets.retain(|s| s.id != id);
    }

    pub fn count(&self, f: Filter) -> usize {
        self.items.iter().filter(|i| i.fits(f)).count()
    }

    // ---- on disk ----

    pub fn load_from(dir: &Path) -> Store {
        let sealed = std::fs::read(dir.join("history.dat")).ok().and_then(|b| crypto::unprotect(&b));
        let plain = || std::fs::read(dir.join("history.json")).ok();
        sealed.or_else(plain).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save_to(&self, dir: &Path, encrypt: bool) -> bool {
        let _ = std::fs::create_dir_all(dir);
        let Ok(json) = serde_json::to_vec(self) else { return false };
        let (name, other, bytes) = if encrypt {
            let Some(b) = crypto::protect(&json) else { return false };
            ("history.dat", "history.json", b)
        } else {
            ("history.json", "history.dat", json)
        };
        let tmp = dir.join(format!("{name}.tmp"));
        let ok = std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, dir.join(name)).is_ok();
        if ok {
            let _ = std::fs::remove_file(dir.join(other));
        }
        ok
    }
}

/// A picture's files: the full one and its thumbnail, PNG (`.png`) or PNG sealed by DPAPI (`.dat`).
pub fn image_file(dir: &Path, id: u64, thumb: bool, encrypt: bool) -> PathBuf {
    let ext = if encrypt { "dat" } else { "png" };
    dir.join("images").join(if thumb { format!("{id}-t.{ext}") } else { format!("{id}.{ext}") })
}

pub fn write_image(dir: &Path, id: u64, thumb: bool, png: &[u8], encrypt: bool) -> bool {
    let _ = std::fs::create_dir_all(dir.join("images"));
    let data = if encrypt {
        match crypto::protect(png) {
            Some(d) => d,
            None => return false,
        }
    } else {
        png.to_vec()
    };
    let ok = std::fs::write(image_file(dir, id, thumb, encrypt), data).is_ok();
    if ok {
        let _ = std::fs::remove_file(image_file(dir, id, thumb, !encrypt));
    }
    ok
}

/// A picture's PNG, whichever way it is kept.
pub fn read_image(dir: &Path, id: u64, thumb: bool) -> Option<Vec<u8>> {
    if let Ok(b) = std::fs::read(image_file(dir, id, thumb, true)) {
        return crypto::unprotect(&b);
    }
    std::fs::read(image_file(dir, id, thumb, false)).ok()
}

pub fn delete_image(dir: &Path, id: u64) {
    for thumb in [false, true] {
        for enc in [false, true] {
            let _ = std::fs::remove_file(image_file(dir, id, thumb, enc));
        }
    }
}

/// Seals or unseals every picture (the "encrypt" setting changed).
pub fn reseal_images(dir: &Path, store: &Store, encrypt: bool) {
    for it in store.items.iter().filter(|i| i.kind == Kind::Image) {
        for thumb in [false, true] {
            if let Some(png) = read_image(dir, it.id, thumb) {
                write_image(dir, it.id, thumb, &png, encrypt);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(texts: &[&str]) -> Store {
        let mut s = Store::default();
        for (n, t) in texts.iter().enumerate() {
            s.add(Item::text(t, Kind::Text), 1000 + n as u64);
        }
        s
    }

    #[test]
    fn dedupe_moves_to_top() {
        let mut s = store_with(&["a", "b", "c"]);
        assert_eq!(s.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
        let (id, fresh) = s.add(Item { app: "notepad.exe".into(), ..Item::text("a", Kind::Text) }, 2000);
        assert!(!fresh);
        assert_eq!(s.items.len(), 3);
        assert_eq!(s.items[0].id, id);
        assert_eq!(s.items[0].text, "a");
        assert_eq!(s.items[0].count, 2);
        assert_eq!(s.items[0].at, 2000);
        assert_eq!(s.items[0].app, "notepad.exe");
        // same text but a link is another item
        let (_, fresh) = s.add(Item::text("a", Kind::Link), 2001);
        assert!(fresh);
    }

    #[test]
    fn limits_keep_pins() {
        let mut s = store_with(&["1", "2", "3", "4", "5"]);
        let oldest = s.items.last().unwrap().id;
        s.set_pin(oldest, true);
        let removed = s.prune(2, 0, 5000);
        assert_eq!(removed.len(), 2);
        assert_eq!(s.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["5", "4", "1"]);
        // days: all of them are old, only the pin stays
        let removed = s.prune(100, 1, 1000 + 2 * 86_400);
        assert_eq!(removed.len(), 2);
        assert_eq!(s.items.len(), 1);
        assert!(s.items[0].pin.is_some());
        // clear keeps the pins too
        s.add(Item::text("x", Kind::Text), 1);
        assert_eq!(s.clear().len(), 1);
        assert_eq!(s.items.len(), 1);
    }

    #[test]
    fn pins_reorder() {
        let mut s = store_with(&["a", "b", "c"]);
        let ids: Vec<u64> = s.items.iter().map(|i| i.id).collect(); // c, b, a
        for id in &ids {
            s.set_pin(*id, true);
        }
        assert_eq!(s.pins(), ids);
        s.move_pin(ids[2], -1);
        assert_eq!(s.pins(), vec![ids[0], ids[2], ids[1]]);
        s.move_pin(ids[0], -1); // already first
        assert_eq!(s.pins()[0], ids[0]);
        s.set_pin(ids[2], false);
        assert_eq!(s.pins(), vec![ids[0], ids[1]]);
        assert_eq!(s.search("", Filter::Pinned), vec![ids[0], ids[1]]);
    }

    #[test]
    fn search_words_and_filters() {
        let mut s = Store::default();
        s.add(Item { app: "code.exe".into(), ..Item::text("fn main() { println!(\"Hello\") }", Kind::Text) }, 1);
        s.add(Item::text("https://plume.example/docs", Kind::Link), 2);
        s.add(Item::files(vec![r"C:\Users\me\Report 2026.pdf".into()]), 3);
        s.add(Item::image(1920, 1080, 42, 1000), 4);
        assert_eq!(s.search("", Filter::All).len(), 4);
        assert_eq!(s.search("HELLO main", Filter::All).len(), 1);
        assert_eq!(s.search("hello docs", Filter::All).len(), 0);
        assert_eq!(s.search("code.exe", Filter::Text).len(), 1);
        assert_eq!(s.search("plume", Filter::Links).len(), 1);
        assert_eq!(s.search("plume", Filter::Text).len(), 0);
        assert_eq!(s.search("report", Filter::Files).len(), 1);
        assert_eq!(s.search("1920", Filter::Images).len(), 1);
        assert_eq!(s.count(Filter::Images), 1);
        assert_eq!(s.get(s.search("report", Filter::All)[0]).unwrap().preview(50), "Report 2026.pdf");
    }

    #[test]
    fn snippets_in_the_panel() {
        let mut s = store_with(&["old copy", "signature draft"]);
        let sig = s.save_snippet(None, "Signature", " ;sig ", "Best,\nSam");
        let addr = s.save_snippet(None, "Address", ";addr", "1 Main St");
        assert_eq!(s.snippet(sig).unwrap().trigger, ";sig");
        // empty search: no snippets, the history
        assert!(s.panel("").iter().all(|e| matches!(e, Entry::Item(_))));
        // the trigger typed in full: only the snippet
        assert_eq!(s.panel(";sig"), vec![Entry::Snippet(sig)]);
        // the start of a trigger: both, the history is searched too
        let p = s.panel(";");
        assert!(p.contains(&Entry::Snippet(sig)) && p.contains(&Entry::Snippet(addr)));
        // a word: the snippet by name, then the history
        let p = s.panel("signature");
        assert_eq!(p[0], Entry::Snippet(sig));
        assert_eq!(p.len(), 2);
        // edit and remove
        s.save_snippet(Some(sig), "Sig", ";s", "x");
        assert_eq!(s.snippet(sig).unwrap().name, "Sig");
        s.remove_snippet(addr);
        assert!(s.snippet(addr).is_none());
    }

    #[test]
    fn panel_pins_first() {
        let mut s = store_with(&["a", "b", "c"]);
        let a = s.items[2].id;
        s.set_pin(a, true);
        let p = s.panel("");
        assert_eq!(p[0], Entry::Item(a));
        assert_eq!(p.len(), 3);
        s.touch(a, 9999);
        assert_eq!(s.items[0].id, a);
    }

    #[test]
    fn saved_plain_and_sealed() {
        let dir = std::env::temp_dir().join(format!("plume-clipboard-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = store_with(&["secret-ish"]);
        s.save_snippet(None, "n", ";t", "body");
        assert!(s.save_to(&dir, true));
        assert!(dir.join("history.dat").exists() && !dir.join("history.json").exists());
        let raw = std::fs::read(dir.join("history.dat")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("secret-ish"));
        let back = Store::load_from(&dir);
        assert_eq!(back.items.len(), 1);
        assert_eq!(back.snippets[0].trigger, ";t");
        assert!(s.save_to(&dir, false));
        assert!(dir.join("history.json").exists() && !dir.join("history.dat").exists());
        assert_eq!(Store::load_from(&dir).items[0].text, "secret-ish");
        // pictures
        assert!(write_image(&dir, 7, false, b"png bytes", true));
        assert_eq!(read_image(&dir, 7, false).unwrap(), b"png bytes");
        assert!(write_image(&dir, 7, false, b"png bytes", false));
        assert!(!image_file(&dir, 7, false, true).exists());
        delete_image(&dir, 7);
        assert!(read_image(&dir, 7, false).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stable_hash() {
        assert_eq!(hash(Kind::Text, b"abc"), hash(Kind::Text, b"abc"));
        assert_ne!(hash(Kind::Text, b"abc"), hash(Kind::Link, b"abc"));
        assert_eq!(hash(Kind::Text, b""), 0xcbf2_9ce4_8422_2325);
    }
}
