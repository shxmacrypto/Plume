//! YouTube Music: search (its own web API, fast and rich: title, artists, album, length, cover), the best match for
//! an imported song, and the sound itself through yt-dlp (the AAC stream YouTube already serves, saved as it arrives,
//! so playback starts before the end of the download). yt-dlp and FFmpeg are shared with Plume Web, in
//! %LOCALAPPDATA%\Plume\tools, and downloaded only when first needed.

use crate::{
    audio::Growth,
    library::{Track, data_dir},
    net,
};
use serde_json::{Value, json};
use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

const NO_WINDOW: u32 = 0x0800_0000;
const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
/// Filters "songs" and "videos" of YouTube Music's search.
const SONGS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";
const VIDEOS: &str = "EgWKAQIQAWoKEAkQBRAKEAMQBA%3D%3D";
const CLIENT_VERSION: &str = "1.20241023.01.00";

pub fn tools_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_TOOLS_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(r"Plume\tools")
}
pub fn ytdlp() -> PathBuf {
    tools_dir().join("yt-dlp.exe")
}
pub fn ffmpeg() -> PathBuf {
    tools_dir().join("ffmpeg.exe")
}
pub fn have_ytdlp() -> bool {
    ytdlp().is_file()
}
pub fn have_ffmpeg() -> bool {
    ffmpeg().is_file()
}

pub fn cache_dir() -> PathBuf {
    data_dir().join("cache")
}

/// Downloads yt-dlp (and FFmpeg if asked). Blocking: call it on a thread.
pub fn install(with_ffmpeg: bool) -> Result<(), String> {
    let dir = tools_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if !have_ytdlp() {
        let data = net::get_tool(YTDLP_URL)?;
        if data.len() < 1_000_000 || &data[..2] != b"MZ" {
            return Err("yt-dlp: unexpected download".into());
        }
        let tmp = dir.join("yt-dlp.exe.part");
        std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, ytdlp()).map_err(|e| e.to_string())?;
    }
    if with_ffmpeg && !have_ffmpeg() {
        let data = net::get_tool(FFMPEG_URL)?;
        crate::zip::extract_only(&data, &dir, &["ffmpeg.exe", "ffprobe.exe"])?;
        if !have_ffmpeg() {
            return Err("ffmpeg.exe not found in the archive".into());
        }
    }
    Ok(())
}

/// YouTube changes often: yt-dlp updates itself, at most once a week (on a thread).
pub fn update_tools_weekly() {
    let stamp = tools_dir().join("yt-dlp.updated");
    let old = std::fs::metadata(&stamp).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_none_or(|e| e > Duration::from_secs(7 * 86400));
    if have_ytdlp() && old {
        std::thread::spawn(move || {
            let ok = cmd().arg("-U").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
            if ok {
                let _ = std::fs::write(stamp, b"");
            }
        });
    }
}

fn cmd() -> Command {
    let mut c = Command::new(ytdlp());
    c.creation_flags(NO_WINDOW).stdin(Stdio::null()).args(["--encoding", "utf-8", "--no-warnings", "--no-playlist"]);
    if have_ffmpeg() {
        c.arg("--ffmpeg-location").arg(tools_dir());
    }
    c
}

// ───────────────────────── search ─────────────────────────

fn innertube(endpoint: &str, body: Value) -> Result<Value, String> {
    let mut b = body;
    b["context"] = json!({ "client": { "clientName": "WEB_REMIX", "clientVersion": CLIENT_VERSION, "hl": "en", "gl": "US" } });
    let url = format!("https://music.youtube.com/youtubei/v1/{endpoint}?prettyPrint=false");
    let (status, data) = net::request(
        "POST",
        &url,
        &["Content-Type: application/json", "Origin: https://music.youtube.com", "Referer: https://music.youtube.com/"],
        Some(b.to_string().as_bytes()),
    )?;
    if status != 200 {
        return Err(format!("YouTube Music: HTTP {status}"));
    }
    serde_json::from_slice(&data).map_err(|e| e.to_string())
}

fn runs_text(v: &Value) -> String {
    v["runs"].as_array().map(|r| r.iter().filter_map(|x| x["text"].as_str()).collect::<String>()).unwrap_or_default()
}

/// "3:22" → 202.
pub fn parse_clock(s: &str) -> Option<f64> {
    let mut total = 0f64;
    let mut any = false;
    for p in s.trim().split(':') {
        total = total * 60.0 + p.trim().parse::<f64>().ok()?;
        any = true;
    }
    any.then_some(total)
}

/// A bigger version of a YouTube Music / Google picture (the search gives 120 px).
pub fn big_thumb(url: &str, px: u32) -> String {
    match url.rfind("=w") {
        Some(i) if url.contains("googleusercontent") => format!("{}=w{px}-h{px}-l90-rj", &url[..i]),
        _ => url.to_string(),
    }
}

/// Songs of YouTube Music for `q` (title, artists, album, length, cover).
pub fn search(q: &str) -> Result<Vec<Track>, String> {
    search_in(q, SONGS)
}

/// Music videos (some songs are on YouTube Music only as their video or "official audio").
pub fn search_videos(q: &str) -> Result<Vec<Track>, String> {
    search_in(q, VIDEOS)
}

fn search_in(q: &str, filter: &str) -> Result<Vec<Track>, String> {
    let v = innertube("search", json!({ "query": q, "params": filter }))?;
    let mut out = Vec::new();
    let sections = v
        .pointer("/contents/tabbedSearchResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    for s in sections {
        let Some(items) = s.pointer("/musicShelfRenderer/contents").and_then(|c| c.as_array()) else { continue };
        for it in items {
            let r = &it["musicResponsiveListItemRenderer"];
            let Some(id) = r.pointer("/playlistItemData/videoId").and_then(|x| x.as_str()) else { continue };
            let cols = r["flexColumns"].as_array().cloned().unwrap_or_default();
            let title = cols.first().map(|c| runs_text(&c["musicResponsiveListItemFlexColumnRenderer"]["text"])).unwrap_or_default();
            let runs = cols.get(1).and_then(|c| c["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"].as_array().cloned()).unwrap_or_default();
            let (mut artists, mut album, mut dur) = (Vec::new(), String::new(), 0.0);
            for run in &runs {
                let text = run["text"].as_str().unwrap_or("");
                let kind = run
                    .pointer("/navigationEndpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
                    .and_then(|k| k.as_str())
                    .unwrap_or("");
                match kind {
                    "MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL" => artists.push(text.to_string()),
                    "MUSIC_PAGE_TYPE_ALBUM" => album = text.to_string(),
                    _ => {
                        if let Some(d) = parse_clock(text).filter(|_| text.contains(':')) {
                            dur = d;
                        }
                    }
                }
            }
            if artists.is_empty() {
                // artists without a link (featurings): the first run
                if let Some(a) = runs.first().and_then(|r| r["text"].as_str()) {
                    artists.push(a.to_string());
                }
            }
            let thumbs = r.pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails").and_then(|t| t.as_array()).cloned().unwrap_or_default();
            let cover = thumbs.last().and_then(|t| t["url"].as_str()).map(|u| big_thumb(u, 544));
            let explicit = r["badges"].to_string().contains("EXPLICIT");
            out.push(Track {
                key: format!("yt:{id}"),
                title,
                artist: artists.join(", "),
                album,
                dur,
                yt: Some(id.to_string()),
                cover,
                explicit,
                ..Default::default()
            });
        }
    }
    Ok(out)
}

/// Search suggestions while typing.
pub fn suggest(q: &str) -> Vec<String> {
    let Ok(v) = innertube("music/get_search_suggestions", json!({ "input": q })) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(sections) = v["contents"].as_array() {
        for s in sections {
            for it in s.pointer("/searchSuggestionsSectionRenderer/contents").and_then(|c| c.as_array()).into_iter().flatten() {
                let t = runs_text(&it["searchSuggestionRenderer"]["suggestion"]);
                if !t.is_empty() {
                    out.push(t);
                }
            }
        }
    }
    out.truncate(8);
    out
}

/// The title without its "(feat. …)", "(with …)" part: those names are artists, not words of the title.
pub fn bare_title(s: &str) -> String {
    let mut out = s.to_string();
    for open in ['(', '['] {
        let close = if open == '(' { ')' } else { ']' };
        while let Some(i) = out.find(open) {
            let inner_start = i + 1;
            let Some(len) = out[inner_start..].find(close) else { break };
            let inner = out[inner_start..inner_start + len].trim().to_lowercase();
            if inner.starts_with("feat") || inner.starts_with("ft.") || inner.starts_with("with ") || inner.starts_with("prod") {
                out.replace_range(i..inner_start + len + 1, "");
            } else {
                break;
            }
        }
    }
    for cut in [" feat. ", " ft. ", " featuring "] {
        if let Some(i) = out.to_lowercase().find(cut) {
            out.truncate(i);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "feat" | "ft" | "the" | "a" | "remastered" | "remaster" | "version" | "official" | "audio"))
        .map(String::from)
        .collect()
}

/// How well a found song matches the wanted one (0..1): words of the title and artist, and the length.
pub fn score(want: &Track, got: &Track) -> f64 {
    let frac = |want: &[String], got: &[String]| if want.is_empty() { 0.0 } else { want.iter().filter(|w| got.contains(w)).count() as f64 / want.len() as f64 };
    let tw = words(&bare_title(&want.title));
    let gt = words(&got.title);
    let gw = words(&format!("{} {}", got.title, got.album));
    // the title counts; the album only helps (a song named like someone else's album is not that album's song)
    let title = frac(&tw, &gt).max(frac(&tw, &gw) * 0.5);
    // the performer counts; a name in the title ("made famous by …", a tribute) only half
    let aw = words(&want.artist);
    let ga = words(&got.artist);
    let by_artist = if aw.is_empty() { 0.5 } else { frac(&aw, &ga).max(frac(&aw, &gt) * 0.5) };
    // the featured singers count too (a label's video lists them all in its title)
    let feat: String = want.title.chars().skip(bare_title(&want.title).chars().count()).collect();
    let fw: Vec<String> = aw.iter().cloned().chain(words(&feat)).collect();
    let with_feat = if fw.is_empty() { 0.0 } else { frac(&fw, &ga).max(frac(&fw, &gt) * 0.5) };
    let artist = by_artist.max(with_feat);
    let (length, far) = if want.dur > 0.0 && got.dur > 0.0 {
        let d = (want.dur - got.dur).abs();
        let l = if d <= 3.0 {
            1.0
        } else if d <= 10.0 {
            0.7
        } else if d <= 30.0 {
            0.3
        } else {
            0.0
        };
        (l, d > 30.0)
    } else {
        (0.5, false)
    };
    // a live, karaoke or cover version, a tribute, only when asked for
    let unwanted = [
        "live",
        "karaoke",
        "instrumental",
        "cover",
        "remix",
        "sped",
        "slowed",
        "nightcore",
        "tribute",
        "famous",
        "popularized",
        "originally",
        "performed",
        "coverband",
        "lofi",
        "acoustic",
        "piano",
        "workout",
    ];
    let wt = words(&want.title);
    let gall = words(&format!("{} {} {}", got.title, got.album, got.artist));
    let mut penalty = unwanted.iter().filter(|u| gall.iter().any(|w| w == *u) && !wt.iter().any(|w| w == *u)).count() as f64 * 0.3;
    // "… by Rihanna" in the title of a song by someone else: a copy
    let gtl = got.title.to_lowercase().replace(['\'', '"'], "");
    if aw.iter().any(|a| gtl.contains(&format!("by {a}"))) && !want.title.to_lowercase().contains(" by ") {
        penalty += 0.6;
    }
    let s = title * 0.45 + artist * 0.35 + length * 0.2 - penalty;
    // half a minute longer or shorter: most likely another song (or another version)
    (if far { s * 0.5 } else { s }).clamp(0.0, 1.0)
}

fn best_of(want: &Track, results: Vec<Track>) -> Option<(f64, Track)> {
    results.into_iter().take(8).map(|t| (score(want, &t), t)).max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
}

/// The YouTube Music song for a song known by its title, artist and length (an imported one). None: nothing close.
/// Songs first; then the music videos (some songs are only there, like Drake's "Hotline Bling").
pub fn find_match(want: &Track) -> Option<Track> {
    let q = format!("{} {}", want.artist.split(',').next().unwrap_or(""), bare_title(&want.title));
    if let Some((s, t)) = search(q.trim()).ok().and_then(|r| best_of(want, r))
        && s >= 0.55
    {
        return Some(t);
    }
    let (s, mut t) = search_videos(q.trim()).ok().and_then(|r| best_of(want, r))?;
    if s < 0.6 {
        return None;
    }
    // a video: the names of the song, and a square cover (the service's, or Apple's)
    t.title = want.title.clone();
    t.artist = want.artist.clone();
    t.album = want.album.clone();
    t.cover = want.cover.clone().or_else(|| itunes_cover(want)).or(t.cover);
    Some(t)
}

/// The album cover of a song from Apple's public search (600 px).
pub fn itunes_cover(t: &Track) -> Option<String> {
    let q = net::enc(&format!("{} {}", t.artist.split(',').next().unwrap_or(""), bare_title(&t.title)));
    let v = net::get_json(&format!("https://itunes.apple.com/search?term={q}&entity=song&limit=5")).ok()?;
    let r = v["results"].as_array()?.iter().find(|r| {
        let got = Track {
            title: r["trackName"].as_str().unwrap_or("").into(),
            artist: r["artistName"].as_str().unwrap_or("").into(),
            dur: r["trackTimeMillis"].as_f64().unwrap_or(0.0) / 1000.0,
            ..Default::default()
        };
        score(t, &got) >= 0.6
    })?;
    r["artworkUrl100"].as_str().map(|u| u.replace("100x100bb", "600x600bb"))
}

/// Songs like this one (YouTube Music's radio of a song): for autoplay and "Song radio".
pub fn radio(id: &str) -> Result<Vec<Track>, String> {
    let v = innertube("next", json!({ "videoId": id, "playlistId": format!("RDAMVM{id}"), "isAudioOnly": true }))?;
    let items = v
        .pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs/0/tabRenderer/content/musicQueueRenderer/content/playlistPanelRenderer/contents")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for it in items {
        let r = &it["playlistPanelVideoRenderer"];
        let Some(vid) = r["videoId"].as_str() else { continue };
        let runs = r["longBylineText"]["runs"].as_array().cloned().unwrap_or_default();
        let (mut artists, mut album) = (Vec::new(), String::new());
        for run in &runs {
            let text = run["text"].as_str().unwrap_or("");
            match run
                .pointer("/navigationEndpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
                .and_then(|k| k.as_str())
            {
                Some("MUSIC_PAGE_TYPE_ARTIST") => artists.push(text.to_string()),
                Some("MUSIC_PAGE_TYPE_ALBUM") => album = text.to_string(),
                _ => {}
            }
        }
        if artists.is_empty()
            && let Some(a) = runs.first().and_then(|r| r["text"].as_str())
        {
            artists.push(a.to_string());
        }
        let cover = r["thumbnail"]["thumbnails"].as_array().and_then(|t| t.last()).and_then(|t| t["url"].as_str()).map(|u| big_thumb(u, 544));
        out.push(Track {
            key: format!("yt:{vid}"),
            title: runs_text(&r["title"]),
            artist: artists.join(", "),
            album,
            dur: parse_clock(&runs_text(&r["lengthText"])).unwrap_or(0.0),
            yt: Some(vid.to_string()),
            cover,
            ..Default::default()
        });
    }
    Ok(out)
}

// ───────────────────────── sound ─────────────────────────

/// Where a source's sound is kept: <cache>/<name>.m4a (or .mp3 for SoundCloud).
pub fn cache_name(t: &Track) -> Option<String> {
    if let Some(id) = &t.yt {
        return Some(format!("yt-{}", crate::library::safe_name(id)));
    }
    t.url.as_ref().map(|u| format!("sc-{:016x}", fnv(u)))
}

pub fn fnv(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// A finished copy in the cache, if any.
pub fn cached(t: &Track) -> Option<PathBuf> {
    let name = cache_name(t)?;
    ["m4a", "mp3", "aac", "ogg", "opus", "webm"]
        .iter()
        .map(|e| cache_dir().join(format!("{name}.{e}")))
        .find(|p| p.is_file() && !cache_dir().join(format!("{name}.downloading")).exists())
}

/// Starts fetching a song's sound into the cache. Returns the file and its progress as soon as the first bytes are
/// there (playback can start), or an error. Blocking for those first seconds: call it on a thread.
pub fn fetch(t: &Track) -> Result<(PathBuf, Arc<Growth>), String> {
    if let Some(p) = cached(t) {
        let g = Arc::new(Growth::default());
        g.done.store(true, Ordering::Release);
        return Ok((p, g));
    }
    if !have_ytdlp() {
        return Err("yt-dlp missing".into());
    }
    let name = cache_name(t).ok_or("nothing to fetch")?;
    let dir = cache_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (url, format, ext) = match (&t.yt, &t.url) {
        (Some(id), _) => (format!("https://music.youtube.com/watch?v={id}"), "140/bestaudio[ext=m4a]/bestaudio[acodec^=mp4a]", "m4a"),
        (None, Some(u)) => (u.clone(), "http_mp3_128/hls_mp3_128/bestaudio[ext=mp3]/bestaudio[acodec^=mp4a]/bestaudio", "mp3"),
        _ => return Err("nothing to fetch".into()),
    };
    let file = dir.join(format!("{name}.{ext}"));
    let marker = dir.join(format!("{name}.downloading"));
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::write(&marker, b"");
    let mut child = cmd()
        // no "fixup": the player reads YouTube's stream format as it is, and keeps the file open while it plays
        .args(["-f", format, "--no-part", "--no-mtime", "--no-continue", "--force-overwrites", "--fixup", "never", "-o"])
        .arg(&file)
        .arg("--")
        .arg(&url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let g = Arc::new(Growth::default());
    // wait for the first bytes (or the end, or an error)
    let start = Instant::now();
    loop {
        if std::fs::metadata(&file).is_ok_and(|m| m.len() > 64 * 1024) {
            break;
        }
        if let Ok(Some(st)) = child.try_wait() {
            if st.success() && file.is_file() {
                break;
            }
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                use std::io::Read;
                let _ = e.read_to_string(&mut err);
            }
            let _ = std::fs::remove_file(&marker);
            let line = err.lines().rev().find(|l| l.contains("ERROR")).unwrap_or("yt-dlp failed").replace("ERROR: ", "");
            return Err(line.chars().take(200).collect());
        }
        if start.elapsed() > Duration::from_secs(40) {
            let _ = child.kill();
            let _ = std::fs::remove_file(&marker);
            return Err("timed out".into());
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    // the rest arrives while the song plays
    let g2 = g.clone();
    let f2 = file.clone();
    std::thread::spawn(move || {
        let ok = child.wait().is_ok_and(|s| s.success()) && f2.is_file();
        if ok {
            let _ = std::fs::remove_file(&marker);
            g2.done.store(true, Ordering::Release);
        } else {
            g2.failed.store(true, Ordering::Release);
            let _ = std::fs::remove_file(&f2);
            let _ = std::fs::remove_file(&marker);
        }
        trim_cache();
    });
    Ok((file, g))
}

/// Cache size limit (MB), from the settings; the oldest songs go first.
pub static CACHE_MB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(2048);

pub fn cache_size() -> u64 {
    std::fs::read_dir(cache_dir()).map(|r| r.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0)
}

pub fn trim_cache() {
    let limit = CACHE_MB.load(Ordering::Relaxed) * 1024 * 1024;
    let Ok(rd) = std::fs::read_dir(cache_dir()) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.accessed().or_else(|_| m.modified()).ok()?, m.len(), e.path()))
        })
        .filter(|(_, _, p)| p.extension().is_none_or(|e| e != "downloading"))
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.0);
    for (_, len, p) in files {
        if total <= limit {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

pub fn clear_cache() {
    if let Ok(rd) = std::fs::read_dir(cache_dir()) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Saves a song for good in `folder` ("Artist - Title.m4a"), with its tags and cover. The sound comes from the
/// cache (fetched if needed). MP3: converted by FFmpeg. Returns the new file.
pub fn download(t: &Track, folder: &Path, mp3: bool, cover: Option<Vec<u8>>) -> Result<PathBuf, String> {
    let (src, g) = fetch(t)?;
    let start = Instant::now();
    while !g.done.load(Ordering::Acquire) {
        if g.failed.load(Ordering::Acquire) {
            return Err("download failed".into());
        }
        if start.elapsed() > Duration::from_secs(600) {
            return Err("timed out".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let base = crate::library::safe_name(&if t.artist.is_empty() {
        t.title.clone()
    } else {
        format!("{} - {}", t.artist.split(',').next().unwrap_or("").trim(), t.title)
    });
    let src_ext = src.extension().and_then(|e| e.to_str()).unwrap_or("m4a").to_string();
    let ext = if mp3 { "mp3".to_string() } else { src_ext.clone() };
    let mut dest = folder.join(format!("{base}.{ext}"));
    let mut n = 2;
    while dest.exists() {
        dest = folder.join(format!("{base} ({n}).{ext}"));
        n += 1;
    }
    if mp3 && src_ext != "mp3" {
        if !have_ffmpeg() {
            return Err("FFmpeg missing".into());
        }
        let st = Command::new(ffmpeg())
            .creation_flags(NO_WINDOW)
            .args(["-v", "error", "-y", "-i"])
            .arg(&src)
            .args(["-vn", "-c:a", "libmp3lame", "-b:a", "320k"])
            .arg(&dest)
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("FFmpeg could not convert".into());
        }
    } else if src_ext == "m4a" && have_ffmpeg() {
        // YouTube's stream (fragmented MP4) rewritten as a plain M4A, the sound untouched: every player reads it
        let st = Command::new(ffmpeg())
            .creation_flags(NO_WINDOW)
            .args(["-v", "error", "-y", "-i"])
            .arg(&src)
            .args(["-vn", "-c:a", "copy", "-movflags", "+faststart"])
            .arg(&dest)
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
        }
    } else {
        std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    }
    if let Err(e) = write_tags(&dest, t, cover) {
        eprintln!("tags: {e}");
    }
    Ok(dest)
}

/// Title, artist, album, cover, and where the song came from (to recognise it when the folder is scanned).
pub fn write_tags(path: &Path, t: &Track, cover: Option<Vec<u8>>) -> Result<(), String> {
    use lofty::{
        config::WriteOptions,
        file::{AudioFile, TaggedFileExt},
        picture::{MimeType, Picture, PictureType},
        prelude::Accessor,
        tag::Tag,
    };
    let mut f = lofty::read_from_path(path).map_err(|e| e.to_string())?;
    if f.primary_tag().is_none() {
        let tt = f.primary_tag_type();
        f.insert_tag(Tag::new(tt));
    }
    let tag = f.primary_tag_mut().ok_or("no tag")?;
    tag.set_title(t.title.clone());
    if !t.artist.is_empty() {
        tag.set_artist(t.artist.clone());
    }
    if !t.album.is_empty() {
        tag.set_album(t.album.clone());
    }
    if let Some(id) = &t.yt {
        tag.set_comment(format!("plume:yt:{id}"));
    }
    if let Some(data) = cover {
        tag.push_picture(Picture::new_unchecked(PictureType::CoverFront, Some(MimeType::Jpeg), None, data));
    }
    f.save_to_path(path, WriteOptions::default()).map_err(|e| e.to_string())
}

/// Songs of a playlist yt-dlp can read (SoundCloud sets, YouTube playlists): title, artist, length, page.
pub fn list(url: &str) -> Result<(String, Vec<Track>), String> {
    if !have_ytdlp() {
        return Err("yt-dlp missing".into());
    }
    let soundcloud = url.contains("soundcloud.com");
    let mut c = Command::new(ytdlp());
    c.creation_flags(NO_WINDOW).stdin(Stdio::null()).args(["--encoding", "utf-8", "--no-warnings", "-J"]);
    // SoundCloud's quick listing has no titles: its songs are read one by one (a second or two for a set)
    if !soundcloud {
        c.arg("--flat-playlist");
    }
    c.args(["--", url]);
    let out = c.stdout(Stdio::piped()).stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().rev().find(|l| l.contains("ERROR")).unwrap_or("yt-dlp error").replace("ERROR: ", "").chars().take(200).collect());
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let name = v["title"].as_str().unwrap_or("Playlist").to_string();
    // one song (not a set): itself
    let entries = match v["entries"].as_array() {
        Some(e) => e.clone(),
        None => vec![v.clone()],
    };
    let mut tracks = Vec::new();
    for e in &entries {
        let title = e["title"].as_str().unwrap_or("").to_string();
        let page = e["webpage_url"].as_str().or(e["url"].as_str()).unwrap_or("").to_string();
        let artist = e["uploader"].as_str().or(e["channel"].as_str()).or(e["artist"].as_str()).unwrap_or("").to_string();
        let dur = e["duration"].as_f64().unwrap_or(0.0);
        let thumb = e["thumbnails"].as_array().and_then(|t| t.last()).and_then(|t| t["url"].as_str()).map(String::from);
        if soundcloud {
            if page.is_empty() {
                continue;
            }
            tracks.push(Track { key: format!("sc:{page}"), title, artist, dur, url: Some(page), cover: thumb, ..Default::default() });
        } else if let Some(id) = e["id"].as_str() {
            let artist = artist.trim_end_matches(" - Topic").to_string();
            tracks.push(Track {
                key: format!("yt:{id}"),
                title,
                artist,
                dur,
                yt: Some(id.to_string()),
                cover: Some(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")),
                ..Default::default()
            });
        }
    }
    Ok((name, tracks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_and_thumbs() {
        assert_eq!(parse_clock("3:22"), Some(202.0));
        assert_eq!(parse_clock("1:02:03"), Some(3723.0));
        assert_eq!(parse_clock("abc"), None);
        assert_eq!(big_thumb("https://lh3.googleusercontent.com/x=w120-h120-l90-rj", 544), "https://lh3.googleusercontent.com/x=w544-h544-l90-rj");
    }

    #[test]
    fn matching() {
        let want = Track { title: "Blinding Lights".into(), artist: "The Weeknd".into(), dur: 200.0, ..Default::default() };
        let good = Track { title: "Blinding Lights".into(), artist: "The Weeknd".into(), album: "After Hours".into(), dur: 202.0, ..Default::default() };
        let live = Track { title: "Blinding Lights (Live)".into(), artist: "The Weeknd".into(), dur: 260.0, ..Default::default() };
        let other = Track { title: "Flashing Lights".into(), artist: "Kanye West".into(), dur: 238.0, ..Default::default() };
        assert!(score(&want, &good) > 0.9);
        assert!(score(&want, &live) < score(&want, &good));
        assert!(score(&want, &other) < 0.55);
        // "(feat. …)" names are not title words
        let feat = Track { title: "Champion (feat. Travis Scott)".into(), artist: "NAV, Travis Scott".into(), dur: 174.0, ..Default::default() };
        let got = Track { title: "Champion".into(), artist: "NAV & Travis Scott".into(), dur: 174.0, ..Default::default() };
        assert!(score(&feat, &got) > 0.9, "{}", score(&feat, &got));
        // an album named like the song, by someone else, a minute shorter: not it
        let other_album = Track {
            title: "Family Di Favorite".into(),
            artist: "Gurtaj, Babbu, Nav Prince".into(),
            album: "Champion".into(),
            dur: 143.0,
            ..Default::default()
        };
        assert!(score(&Track { dur: 195.0, ..feat.clone() }, &other_album) < 0.4);
        // copies that name the artist in their title are not the song
        let want = Track { title: "Disturbia".into(), artist: "Rihanna".into(), dur: 239.0, ..Default::default() };
        let copy = Track { title: "Disturbia (Original Version By 'Rihanna')".into(), artist: "The Coverband".into(), dur: 236.0, ..Default::default() };
        let real = Track { title: "Disturbia".into(), artist: "Rihanna".into(), dur: 239.0, ..Default::default() };
        assert!(score(&want, &copy) < 0.55, "{}", score(&want, &copy));
        assert!(score(&want, &real) > 0.9);
        let want = Track { title: "Call Me Maybe".into(), artist: "Carly Rae Jepsen".into(), dur: 193.0, ..Default::default() };
        let copy = Track {
            title: "Call Me Maybe (made popular by Carly Rae Jepsen) [vocal version]".into(),
            artist: "Party Tyme".into(),
            dur: 195.0,
            ..Default::default()
        };
        assert!(score(&want, &copy) < 0.55, "{}", score(&want, &copy));
        // a label's official video naming every singer
        let want = Track {
            title: "Bande organisée (feat. SCH, Kofs, JUL, Naps, Soso Maness, Elams, Solda & Houari)".into(),
            artist: "13 Organisé".into(),
            dur: 356.0,
            ..Default::default()
        };
        let video = Track {
            title: "Bande Organisée - SCH / Kofs / Jul / Naps / Soso Maness / Elams / Solda / Houari // Clip Officiel".into(),
            artist: "DORETDEPLATINE".into(),
            dur: 374.0,
            ..Default::default()
        };
        assert!(score(&want, &video) >= 0.6, "{}", score(&want, &video));
        assert_eq!(bare_title("Bande organisée (feat. SCH, Kofs, JUL)"), "Bande organisée");
        assert_eq!(bare_title("Song [with X] (Remix)"), "Song (Remix)");
        assert_eq!(bare_title("Song feat. X"), "Song");
    }

    #[test]
    #[ignore] // network
    fn live_search() {
        let r = search("the weeknd blinding lights").unwrap();
        assert!(r.iter().any(|t| t.title == "Blinding Lights" && t.artist.contains("Weeknd") && t.dur > 150.0), "{r:?}");
        // only a video on YouTube Music
        let hb = find_match(&Track { title: "Hotline Bling".into(), artist: "Drake".into(), dur: 267.0, ..Default::default() }).expect("Hotline Bling");
        assert!(hb.yt.is_some() && hb.title == "Hotline Bling" && hb.cover.as_deref().is_some_and(|c| c.contains("mzstatic")), "{hb:?}");
        let m = find_match(&Track { title: "Blinding Lights".into(), artist: "The Weeknd".into(), dur: 200.0, ..Default::default() }).unwrap();
        assert_eq!(m.title, "Blinding Lights");
        let r = radio(m.yt.as_deref().unwrap()).unwrap();
        assert!(r.len() >= 10 && r.iter().all(|t| t.yt.is_some() && !t.title.is_empty() && t.dur > 0.0), "{}", r.len());
    }
}
