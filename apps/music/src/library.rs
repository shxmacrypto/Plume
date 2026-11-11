//! The library: every song Plume Music knows (files on this PC, YouTube Music songs, SoundCloud tracks), the
//! playlists, likes and play counts. Kept in %LOCALAPPDATA%\Plume\Music\library.json (PLUME_MUSIC_DIR for tests).

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const AUDIO_EXT: [&str; 10] = ["mp3", "m4a", "mp4", "aac", "flac", "ogg", "oga", "wav", "alac", "aiff"];

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_MUSIC_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Music")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Track {
    /// "f:<path>", "yt:<video id>" or "sc:<page url>".
    pub key: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Seconds (0: unknown).
    pub dur: f64,
    /// A file on this PC (scanned, or downloaded).
    pub path: Option<PathBuf>,
    /// A YouTube video id to stream it.
    pub yt: Option<String>,
    /// A page yt-dlp can read (SoundCloud).
    pub url: Option<String>,
    /// Cover: an http(s) URL; files use their embedded picture.
    pub cover: Option<String>,
    pub year: u32,
    pub no: u32,
    pub added: u64,
    pub plays: u32,
    pub last: u64,
    pub liked: bool,
    /// ReplayGain of the file, in dB.
    pub gain: Option<f32>,
    pub explicit: bool,
}

impl Track {
    /// Something to play (a file, or a source yt-dlp can fetch).
    pub fn playable(&self) -> bool {
        self.path.as_ref().is_some_and(|p| p.is_file()) || self.yt.is_some() || self.url.is_some()
    }
    pub fn local(&self) -> bool {
        self.path.as_ref().is_some_and(|p| p.is_file())
    }
    pub fn album_key(&self) -> String {
        let artist = self.artist.split([',', '&']).next().unwrap_or("").trim().to_lowercase();
        format!("{}\u{1}{}", artist, self.album.to_lowercase())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub tracks: Vec<String>,
    /// Where it was imported from (Spotify, Apple Music, SoundCloud, YouTube link).
    pub source: Option<String>,
    pub cover: Option<String>,
    pub created: u64,
    pub pinned: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub tracks: HashMap<String, Track>,
    pub playlists: Vec<Playlist>,
    /// Folders scanned for music files.
    pub folders: Vec<PathBuf>,
    /// Recently played keys, newest first.
    pub recent: Vec<String>,
    /// Recent searches.
    pub searches: Vec<String>,
    #[serde(skip)]
    pub dirty: bool,
}

/// The user's Music folder (Windows' known folder), the default place to scan and download to.
pub fn music_folder() -> PathBuf {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_Music, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    };
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Music, KF_FLAG_DEFAULT, None) {
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as _));
            if let Some(s) = s {
                return PathBuf::from(s);
            }
        }
    }
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join("Music")
}

impl Library {
    fn file() -> PathBuf {
        data_dir().join("library.json")
    }

    pub fn load() -> Library {
        let mut lib: Library = std::fs::read(Self::file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        // songs only seen in a search or an autoplay last time, never kept: forgotten
        let keep: std::collections::HashSet<String> = lib.playlists.iter().flat_map(|p| p.tracks.iter().cloned()).chain(lib.recent.iter().cloned()).collect();
        lib.tracks.retain(|k, t| t.added != 0 || t.plays > 0 || t.liked || t.local() || keep.contains(k));
        if lib.folders.is_empty() && std::env::var_os("PLUME_MUSIC_DIR").is_none() {
            lib.folders.push(music_folder());
        }
        lib
    }

    pub fn save(&mut self) {
        let _ = std::fs::create_dir_all(data_dir());
        let tmp = Self::file().with_extension("tmp");
        if let Ok(b) = serde_json::to_vec(self)
            && std::fs::write(&tmp, b).is_ok()
        {
            let _ = std::fs::rename(&tmp, Self::file());
            self.dirty = false;
        }
    }

    /// Adds a song (or completes the one already known: new information wins, the user's own stays).
    pub fn upsert(&mut self, mut t: Track) -> String {
        let key = t.key.clone();
        match self.tracks.get_mut(&key) {
            Some(old) => {
                if !t.title.is_empty() {
                    old.title = t.title;
                }
                if !t.artist.is_empty() {
                    old.artist = t.artist;
                }
                if !t.album.is_empty() {
                    old.album = t.album;
                }
                if t.dur > 0.0 {
                    old.dur = t.dur;
                }
                if t.cover.is_some() {
                    old.cover = t.cover;
                }
                if t.path.is_some() {
                    old.path = t.path;
                }
                if t.yt.is_some() {
                    old.yt = t.yt;
                }
                if t.url.is_some() {
                    old.url = t.url;
                }
                if t.gain.is_some() {
                    old.gain = t.gain;
                }
            }
            None => {
                if t.added == 0 {
                    t.added = now();
                }
                self.tracks.insert(key.clone(), t);
            }
        }
        self.dirty = true;
        key
    }

    pub fn playlist(&self, id: &str) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.id == id)
    }

    pub fn playlist_mut(&mut self, id: &str) -> Option<&mut Playlist> {
        self.dirty = true;
        self.playlists.iter_mut().find(|p| p.id == id)
    }

    pub fn new_playlist(&mut self, name: &str) -> String {
        let id = format!("p{:x}{:04x}", now(), self.playlists.len());
        self.playlists.push(Playlist { id: id.clone(), name: name.to_string(), created: now(), ..Default::default() });
        self.dirty = true;
        id
    }

    pub fn played(&mut self, key: &str) {
        if let Some(t) = self.tracks.get_mut(key) {
            t.plays += 1;
            t.last = now();
        }
        self.recent.retain(|k| k != key);
        self.recent.insert(0, key.to_string());
        self.recent.truncate(200);
        self.dirty = true;
    }

    /// Songs of the library view: files and downloads, liked songs, and songs saved to a playlist.
    pub fn songs(&self) -> Vec<&Track> {
        let mut v: Vec<&Track> = self.tracks.values().filter(|t| t.local() || t.liked || self.in_playlist(&t.key)).collect();
        v.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()).then(a.album.cmp(&b.album)).then(a.no.cmp(&b.no)).then(a.title.cmp(&b.title)));
        v
    }

    fn in_playlist(&self, key: &str) -> bool {
        self.playlists.iter().any(|p| p.tracks.iter().any(|k| k == key))
    }

    /// Albums (artist, album, its songs in order), the most recently added first.
    pub fn albums(&self) -> Vec<(String, String, Vec<String>)> {
        let mut map: HashMap<String, (String, String, Vec<&Track>, u64)> = HashMap::new();
        for t in self.songs() {
            if t.album.is_empty() {
                continue;
            }
            let e = map.entry(t.album_key()).or_insert_with(|| (t.artist.clone(), t.album.clone(), Vec::new(), 0));
            e.3 = e.3.max(t.added);
            e.2.push(t);
        }
        let mut v: Vec<_> = map.into_values().collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.3));
        v.into_iter()
            .map(|(ar, al, mut ts, _)| {
                ts.sort_by_key(|t| (t.no, t.title.clone()));
                (ar, al, ts.into_iter().map(|t| t.key.clone()).collect())
            })
            .collect()
    }

    /// Artists and how many songs each has.
    pub fn artists(&self) -> Vec<(String, usize)> {
        let mut map: HashMap<String, (String, usize)> = HashMap::new();
        for t in self.songs() {
            let name = t.artist.split([',', '&']).next().unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            map.entry(name.to_lowercase()).or_insert_with(|| (name.clone(), 0)).1 += 1;
        }
        let mut v: Vec<_> = map.into_values().collect();
        v.sort_by_key(|a| a.0.to_lowercase());
        v
    }

    pub fn liked(&self) -> Vec<String> {
        let mut v: Vec<&Track> = self.tracks.values().filter(|t| t.liked).collect();
        v.sort_by_key(|t| std::cmp::Reverse(t.added));
        v.into_iter().map(|t| t.key.clone()).collect()
    }

    pub fn downloaded(&self) -> Vec<String> {
        let mut v: Vec<&Track> = self.tracks.values().filter(|t| t.local() && (t.yt.is_some() || t.url.is_some())).collect();
        v.sort_by_key(|t| std::cmp::Reverse(t.added));
        v.into_iter().map(|t| t.key.clone()).collect()
    }

    /// Simple local search: every word must appear in the title, artist or album.
    pub fn search(&self, q: &str) -> Vec<String> {
        let words: Vec<String> = q.to_lowercase().split_whitespace().map(String::from).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut v: Vec<&Track> = self
            .tracks
            .values()
            .filter(|t| {
                let hay = format!("{} {} {}", t.title, t.artist, t.album).to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .collect();
        v.sort_by(|a, b| b.plays.cmp(&a.plays).then(a.title.cmp(&b.title)));
        v.into_iter().take(50).map(|t| t.key.clone()).collect()
    }

    /// Files that disappeared: the song stays (with its YouTube source if it has one) but loses its file.
    pub fn forget_missing(&mut self) {
        let mut gone = Vec::new();
        for t in self.tracks.values_mut() {
            if t.path.as_ref().is_some_and(|p| !p.is_file()) {
                t.path = None;
                if !t.playable() && !t.liked {
                    gone.push(t.key.clone());
                }
            }
        }
        for k in gone {
            if !self.in_playlist(&k) {
                self.tracks.remove(&k);
            }
        }
    }
}

/// Reads a music file's tags (lofty). None: not audio, or unreadable.
pub fn read_file(path: &Path) -> Option<Track> {
    use lofty::{file::AudioFile, file::TaggedFileExt, prelude::Accessor, tag::ItemKey};
    let ext = path.extension()?.to_str()?.to_lowercase();
    if !AUDIO_EXT.contains(&ext.as_str()) {
        return None;
    }
    let f = lofty::read_from_path(path).ok()?;
    let dur = f.properties().duration().as_secs_f64();
    let tag = f.primary_tag().or_else(|| f.first_tag());
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    let mut t = Track { key: format!("f:{}", path.display()), path: Some(path.to_path_buf()), dur, ..Default::default() };
    if let Some(tag) = tag {
        t.title = tag.title().map(|s| s.to_string()).unwrap_or_default();
        t.artist = tag.artist().map(|s| s.to_string()).unwrap_or_default();
        t.album = tag.album().map(|s| s.to_string()).unwrap_or_default();
        t.year = tag.year().unwrap_or(0);
        t.no = tag.track().unwrap_or(0);
        t.gain = tag.get_string(&ItemKey::ReplayGainTrackGain).and_then(|g| g.trim().trim_end_matches("dB").trim().parse().ok());
        // songs downloaded by Plume Music remember their YouTube source in the comment
        if let Some(id) = tag.comment().and_then(|c| c.strip_prefix("plume:yt:").map(|s| s.trim().to_string()))
            && !id.is_empty()
        {
            t.yt = Some(id);
            t.key = format!("yt:{}", t.yt.as_deref().unwrap_or(""));
        }
    }
    if t.title.is_empty() {
        // "Artist - Title.mp3"
        match stem.split_once(" - ") {
            Some((a, b)) if t.artist.is_empty() => {
                t.artist = a.trim().to_string();
                t.title = b.trim().to_string();
            }
            _ => t.title = stem,
        }
    }
    Some(t)
}

/// Every music file under these folders (not following links, at most 20 levels deep).
pub fn scan(folders: &[PathBuf], mut each: impl FnMut(Track)) {
    fn walk(dir: &Path, depth: u32, each: &mut dyn FnMut(Track)) {
        if depth > 20 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let p = e.path();
            if ft.is_dir() {
                walk(&p, depth + 1, each);
            } else if ft.is_file()
                && let Some(t) = read_file(&p)
            {
                each(t);
            }
        }
    }
    for f in folders {
        walk(f, 0, &mut each);
    }
}

/// "3:07", "1:02:03".
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// A file name that Windows accepts.
pub fn safe_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    n.trim().trim_end_matches('.').chars().take(150).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_keeps_user_data() {
        let mut l = Library::default();
        l.upsert(Track { key: "yt:a".into(), title: "A".into(), ..Default::default() });
        l.tracks.get_mut("yt:a").unwrap().liked = true;
        l.upsert(Track { key: "yt:a".into(), artist: "B".into(), ..Default::default() });
        let t = &l.tracks["yt:a"];
        assert!(t.liked && t.title == "A" && t.artist == "B");
        assert_eq!(l.liked(), vec!["yt:a".to_string()]);
    }

    #[test]
    fn helpers() {
        assert_eq!(clock(187.4), "3:07");
        assert_eq!(clock(3723.0), "1:02:03");
        assert_eq!(safe_name("AC/DC: Back?"), "AC_DC_ Back_");
        let mut l = Library::default();
        l.upsert(Track { key: "f:x".into(), title: "Blinding Lights".into(), artist: "The Weeknd".into(), ..Default::default() });
        assert_eq!(l.search("weeknd light"), vec!["f:x".to_string()]);
        assert!(l.search("drake").is_empty());
    }
}
