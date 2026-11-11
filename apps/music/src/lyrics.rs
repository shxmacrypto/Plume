//! Lyrics: a .lrc file next to the song, the song's own tag, or LRCLIB (a free, open lyrics database, with
//! time-synced lyrics for most songs). Kept in the cache once found.

use crate::{
    library::{Track, data_dir},
    net,
};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lyrics {
    /// (time in seconds, line). Times are all 0 when the lyrics are not synced.
    pub lines: Vec<(f64, String)>,
    pub synced: bool,
    pub source: String,
}

impl Lyrics {
    /// The line sung at `t` (synced lyrics only).
    pub fn current(&self, t: f64) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let i = self.lines.partition_point(|(at, _)| *at <= t + 0.15);
        i.checked_sub(1)
    }
}

/// Reads LRC text ("[01:02.34]words"); plain text gives unsynced lines.
pub fn parse(text: &str) -> Lyrics {
    let mut lines = Vec::new();
    let mut offset = 0.0;
    for raw in text.lines() {
        let mut rest = raw.trim();
        if let Some(o) = rest.strip_prefix("[offset:").and_then(|r| r.strip_suffix(']')) {
            offset = o.trim().parse::<f64>().unwrap_or(0.0) / 1000.0;
            continue;
        }
        let mut times = Vec::new();
        while let Some(r) = rest.strip_prefix('[') {
            let Some(end) = r.find(']') else { break };
            let tag = &r[..end];
            match parse_time(tag) {
                Some(t) => times.push(t),
                None => {
                    // [ar:], [ti:]… : metadata, not a line
                    if tag.contains(':') && tag.chars().next().is_some_and(|c| c.is_alphabetic()) {
                        times.clear();
                        rest = "";
                        break;
                    }
                }
            }
            rest = r[end + 1..].trim_start();
        }
        if times.is_empty() {
            if !raw.trim().starts_with('[') {
                lines.push((f64::NAN, raw.trim().to_string()));
            }
            continue;
        }
        for t in times {
            lines.push(((t - offset).max(0.0), rest.trim().to_string()));
        }
    }
    let synced = !lines.is_empty() && lines.iter().all(|(t, _)| !t.is_nan());
    if synced {
        lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        for l in lines.iter_mut() {
            l.0 = 0.0;
        }
        // trailing empty lines
        while lines.last().is_some_and(|l| l.1.is_empty()) {
            lines.pop();
        }
    }
    Lyrics { lines, synced, source: String::new() }
}

fn parse_time(tag: &str) -> Option<f64> {
    let (m, s) = tag.split_once(':')?;
    let m: f64 = m.trim().parse().ok()?;
    let s: f64 = s.trim().replace(',', ".").parse().ok()?;
    Some(m * 60.0 + s)
}

fn cache_file(t: &Track) -> PathBuf {
    data_dir().join("lyrics").join(format!("{:016x}.lrc", crate::yt::fnv(&format!("{}\u{1}{}", t.artist.to_lowercase(), t.title.to_lowercase()))))
}

/// Finds the lyrics of a song (blocking: network). None: nowhere.
pub fn find(t: &Track) -> Option<Lyrics> {
    // 1. a .lrc next to the file
    if let Some(p) = &t.path {
        let lrc = p.with_extension("lrc");
        if let Ok(s) = std::fs::read_to_string(&lrc) {
            let mut l = parse(&s);
            l.source = "file".into();
            return Some(l);
        }
        // 2. the file's own lyrics tag
        if let Some(s) = embedded(p) {
            let mut l = parse(&s);
            l.source = "tag".into();
            return Some(l);
        }
    }
    // 3. found before
    let cache = cache_file(t);
    if let Ok(s) = std::fs::read_to_string(&cache) {
        if s.trim() == "[none]" {
            return None;
        }
        let mut l = parse(&s);
        l.source = "LRCLIB".into();
        return Some(l);
    }
    // 4. LRCLIB
    let found = lrclib(t);
    let _ = std::fs::create_dir_all(cache.parent()?);
    match &found {
        Some(text) => {
            let _ = std::fs::write(&cache, text);
        }
        None => {
            let _ = std::fs::write(&cache, "[none]");
        }
    }
    found.map(|s| {
        let mut l = parse(&s);
        l.source = "LRCLIB".into();
        l
    })
}

fn embedded(p: &std::path::Path) -> Option<String> {
    use lofty::{file::TaggedFileExt, tag::ItemKey};
    let f = lofty::read_from_path(p).ok()?;
    let tag = f.primary_tag().or_else(|| f.first_tag())?;
    tag.get_string(&ItemKey::Lyrics).map(String::from).filter(|s| !s.trim().is_empty())
}

fn clean_title(s: &str) -> String {
    // "Song (feat. X)", "Song - Remastered 2011": LRCLIB knows the bare title
    let mut s = s.to_string();
    for cut in [" (feat", " (ft", " [feat", " - Remaster", " (Remaster", " - From ", " (From "] {
        if let Some(i) = s.find(cut) {
            s.truncate(i);
        }
    }
    s.trim().to_string()
}

fn lrclib(t: &Track) -> Option<String> {
    let artist = t.artist.split(',').next().unwrap_or("").trim();
    let title = clean_title(&t.title);
    if title.is_empty() {
        return None;
    }
    let pick = |v: &serde_json::Value| -> Option<String> {
        v["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()).or(v["plainLyrics"].as_str().filter(|s| !s.trim().is_empty())).map(String::from)
    };
    let mut url = format!("https://lrclib.net/api/get?artist_name={}&track_name={}", net::enc(artist), net::enc(&title));
    if !t.album.is_empty() {
        url.push_str(&format!("&album_name={}", net::enc(&t.album)));
    }
    if t.dur > 0.0 {
        url.push_str(&format!("&duration={}", t.dur.round() as u64));
    }
    if let Ok(v) = net::get_json(&url)
        && let Some(s) = pick(&v)
    {
        return Some(s);
    }
    // a looser search: the closest length among the results
    let v = net::get_json(&format!("https://lrclib.net/api/search?track_name={}&artist_name={}", net::enc(&title), net::enc(artist))).ok()?;
    let list = v.as_array()?;
    let best = list
        .iter()
        .filter(|x| pick(x).is_some())
        .min_by_key(|x| if t.dur > 0.0 { ((x["duration"].as_f64().unwrap_or(0.0) - t.dur).abs() * 10.0) as u64 } else { 0 })?;
    if t.dur > 0.0 && (best["duration"].as_f64().unwrap_or(0.0) - t.dur).abs() > 15.0 {
        return None;
    }
    pick(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc() {
        let l = parse("[ar:Someone]\n[00:12.50]Hello\n[00:15.00][01:15.00]Again\n[00:20]Bye");
        assert!(l.synced);
        assert_eq!(l.lines, vec![(12.5, "Hello".into()), (15.0, "Again".into()), (20.0, "Bye".into()), (75.0, "Again".into())]);
        assert_eq!(l.current(14.0), Some(0));
        assert_eq!(l.current(5.0), None);
        assert_eq!(l.current(100.0), Some(3));
        let p = parse("Just words\nand more\n");
        assert!(!p.synced);
        assert_eq!(p.lines.len(), 2);
        assert_eq!(clean_title("Song (feat. X) - Remastered"), "Song");
    }

    #[test]
    #[ignore] // network
    fn live_lrclib() {
        let t = Track { title: "Blinding Lights".into(), artist: "The Weeknd".into(), album: "After Hours".into(), dur: 200.0, ..Default::default() };
        let s = lrclib(&t).expect("lyrics");
        assert!(parse(&s).synced);
    }
}
