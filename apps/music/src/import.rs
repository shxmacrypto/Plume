//! Playlists from a link: Spotify (playlist, album, track), Apple Music (playlist, album, song), SoundCloud (set,
//! track) and YouTube / YouTube Music (playlist). Public pages only, no account. Spotify and Apple songs are then
//! found on YouTube Music (title, artist and length must match); SoundCloud plays from SoundCloud.

use crate::{library::Track, net, yt};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Spotify,
    Apple,
    Deezer,
    SoundCloud,
    YouTube,
}

pub fn service(url: &str) -> Option<Service> {
    let u = url.trim().to_lowercase();
    if !u.starts_with("https://") && !u.starts_with("http://") {
        return None;
    }
    if u.contains("open.spotify.com/") {
        Some(Service::Spotify)
    } else if u.contains("music.apple.com/") {
        Some(Service::Apple)
    } else if u.contains("deezer.com/") || u.contains("deezer.page.link/") || u.contains("dzr.page.link/") {
        Some(Service::Deezer)
    } else if u.contains("soundcloud.com/") {
        Some(Service::SoundCloud)
    } else if u.contains("youtube.com/") || u.contains("youtu.be/") {
        Some(Service::YouTube)
    } else {
        None
    }
}

/// The Spotify songs in a text: the links Spotify copies with Ctrl+A, Ctrl+C in a playlist (every song, not only the
/// first 100 that a shared playlist link shows), or spotify:track: URIs. In order, without repeats.
pub fn spotify_track_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for marker in ["open.spotify.com/track/", "spotify:track:"] {
        let mut rest = text;
        while let Some(i) = rest.find(marker) {
            let after = &rest[i + marker.len()..];
            let id: String = after.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if id.len() == 22 && !out.contains(&id) {
                out.push(id.clone());
            }
            rest = &after[id.len()..];
        }
    }
    // keep the order of the text (both kinds mixed)
    out.sort_by_key(|id| text.find(id.as_str()).unwrap_or(usize::MAX));
    out
}

/// A Spotify song from its public page: title, artists, length.
pub fn spotify_track(id: &str) -> Option<Track> {
    let html = net::get_text(&format!("https://open.spotify.com/embed/track/{id}")).ok()?;
    let start = html.find(r#"<script id="__NEXT_DATA__" type="application/json">"#)?;
    let rest = &html[start..];
    let json = &rest[rest.find('>')? + 1..rest.find("</script>")?];
    let v: Value = serde_json::from_str(json).ok()?;
    let e = v.pointer("/props/pageProps/state/data/entity")?;
    let title = e["name"].as_str().or(e["title"].as_str())?.to_string();
    let artist = e["artists"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    Some(Track {
        title,
        artist,
        dur: e["duration"].as_f64().unwrap_or(0.0) / 1000.0,
        explicit: e["isExplicit"].as_bool().unwrap_or(false),
        ..Default::default()
    })
}

/// What a link holds: its name, its cover, and its songs (Spotify/Apple songs not matched yet).
pub struct Found {
    pub name: String,
    pub cover: Option<String>,
    pub tracks: Vec<Track>,
    pub service: Service,
}

pub fn read(url: &str) -> Result<Found, String> {
    let url = url.trim();
    match service(url).ok_or("This link is not from Spotify, Apple Music, Deezer, SoundCloud or YouTube.")? {
        Service::Spotify => spotify(url),
        Service::Apple => apple(url),
        Service::Deezer => deezer(url),
        s @ (Service::SoundCloud | Service::YouTube) => {
            let (name, tracks) = yt::list(url)?;
            if tracks.is_empty() {
                return Err("No songs found at this link.".into());
            }
            let cover = tracks.first().and_then(|t| t.cover.clone());
            Ok(Found { name, cover, tracks, service: s })
        }
    }
}

/// open.spotify.com/(intl-xx/)?(playlist|album|track)/<id> → the public embed page, which lists the songs.
fn spotify(url: &str) -> Result<Found, String> {
    let path = url.split("open.spotify.com/").nth(1).ok_or("bad Spotify link")?;
    let parts: Vec<&str> = path.split(['/', '?', '#']).filter(|p| !p.is_empty() && !p.starts_with("intl-")).collect();
    let (kind, id) = match parts.as_slice() {
        [k, id, ..] if matches!(*k, "playlist" | "album" | "track") => (*k, *id),
        _ => return Err("Use a Spotify playlist, album or song link.".into()),
    };
    let html = net::get_text(&format!("https://open.spotify.com/embed/{kind}/{id}"))?;
    let start = html.find(r#"<script id="__NEXT_DATA__" type="application/json">"#).ok_or("Spotify: page not understood")?;
    let rest = &html[start..];
    let json = &rest[rest.find('>').ok_or("Spotify")? + 1..rest.find("</script>").ok_or("Spotify")?];
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let e = v.pointer("/props/pageProps/state/data/entity").ok_or("Spotify: playlist not found (is it public?)")?;
    let name = e["name"].as_str().or(e["title"].as_str()).unwrap_or("Spotify").to_string();
    let cover = e["coverArt"]["sources"]
        .as_array()
        .and_then(|s| s.iter().max_by_key(|x| x["width"].as_u64().unwrap_or(0)))
        .and_then(|x| x["url"].as_str())
        .map(String::from);
    let mut tracks = Vec::new();
    let list = e["trackList"].as_array().cloned().unwrap_or_default();
    if list.is_empty() && kind == "track" {
        tracks.push(Track {
            title: e["title"].as_str().or(e["name"].as_str()).unwrap_or("").into(),
            artist: e["artists"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
            dur: e["duration"].as_f64().unwrap_or(0.0) / 1000.0,
            cover: cover.clone(),
            ..Default::default()
        });
    }
    for t in list {
        let title = t["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() {
            continue;
        }
        tracks.push(Track {
            title,
            artist: t["subtitle"].as_str().unwrap_or("").replace('\u{a0}', " "),
            album: if kind == "album" { name.clone() } else { String::new() },
            dur: t["duration"].as_f64().unwrap_or(0.0) / 1000.0,
            explicit: t["isExplicit"].as_bool().unwrap_or(false),
            cover: if kind == "album" { cover.clone() } else { None },
            ..Default::default()
        });
    }
    if tracks.is_empty() {
        return Err("Spotify: no songs found (is the playlist public?)".into());
    }
    Ok(Found { name, cover, tracks, service: Service::Spotify })
}

/// The kind and number in a Deezer address ("playlist", "123").
fn deezer_ref(s: &str) -> Option<(String, String)> {
    for kind in ["playlist", "album", "track"] {
        let marker = format!("/{kind}/");
        let mut rest = s;
        while let Some(i) = rest.find(&marker) {
            let after = &rest[i + marker.len()..];
            let id: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !id.is_empty() && rest[..i].contains("deezer.com") {
                return Some((kind.to_string(), id));
            }
            rest = after;
        }
    }
    None
}

fn deezer_track(t: &Value, album: Option<&str>, cover: Option<&str>) -> Option<Track> {
    let title = t["title"].as_str().or(t["title_short"].as_str())?.to_string();
    Some(Track {
        title,
        artist: t["artist"]["name"].as_str().unwrap_or("").to_string(),
        album: t["album"]["title"].as_str().or(album).unwrap_or("").to_string(),
        dur: t["duration"].as_f64().unwrap_or(0.0),
        explicit: t["explicit_lyrics"].as_bool().unwrap_or(false),
        cover: t["album"]["cover_xl"].as_str().or(cover).map(String::from),
        ..Default::default()
    })
}

/// Deezer has a free public API: a playlist, an album or a song, every song of it.
fn deezer(url: &str) -> Result<Found, String> {
    // a short sharing link (deezer.page.link/…) leads to the real address
    let found = deezer_ref(url).or_else(|| net::get_text(url).ok().and_then(|page| deezer_ref(&page.replace("\\/", "/"))));
    let (kind, id) = found.ok_or("Use a Deezer playlist, album or song link.")?;
    let api = |path: &str| -> Result<Value, String> {
        let v = net::get_json(&format!("https://api.deezer.com/{path}"))?;
        if let Some(e) = v["error"]["message"].as_str() {
            return Err(format!("Deezer: {e}"));
        }
        Ok(v)
    };
    let mut tracks = Vec::new();
    let (name, cover) = match kind.as_str() {
        "track" => {
            let v = api(&format!("track/{id}"))?;
            tracks.extend(deezer_track(&v, None, None));
            (v["title"].as_str().unwrap_or("Deezer").to_string(), v["album"]["cover_xl"].as_str().map(String::from))
        }
        "album" => {
            let v = api(&format!("album/{id}"))?;
            let (al, cover) = (v["title"].as_str().unwrap_or("").to_string(), v["cover_xl"].as_str().map(String::from));
            let mut next = Some(format!("album/{id}/tracks?limit=100"));
            while let Some(path) = next.take() {
                let page = api(&path)?;
                for t in page["data"].as_array().into_iter().flatten() {
                    tracks.extend(deezer_track(t, Some(&al), cover.as_deref()));
                }
                next = page["next"].as_str().and_then(|n| n.split("api.deezer.com/").nth(1)).map(String::from);
            }
            (al, cover)
        }
        _ => {
            let v = api(&format!("playlist/{id}"))?;
            let name = v["title"].as_str().unwrap_or("Deezer").to_string();
            let cover = v["picture_xl"].as_str().map(String::from);
            let mut next = Some(format!("playlist/{id}/tracks?limit=100"));
            let mut pages = 0;
            while let Some(path) = next.take() {
                let page = api(&path)?;
                for t in page["data"].as_array().into_iter().flatten() {
                    tracks.extend(deezer_track(t, None, None));
                }
                pages += 1;
                next = page["next"].as_str().and_then(|n| n.split("api.deezer.com/").nth(1)).map(String::from).filter(|_| pages < 100);
            }
            (name, cover)
        }
    };
    if tracks.is_empty() {
        return Err("Deezer: no songs found (is the playlist public?)".into());
    }
    Ok(Found { name, cover, tracks, service: Service::Deezer })
}

/// Apple Music pages carry their content as JSON ("serialized-server-data").
fn apple(url: &str) -> Result<Found, String> {
    let html = net::get_text(url)?;
    let marker = r#"id="serialized-server-data">"#;
    let start = html.find(marker).ok_or("Apple Music: page not understood")? + marker.len();
    let end = start + html[start..].find("</script>").ok_or("Apple Music")?;
    let v: Value = serde_json::from_str(&html[start..end]).map_err(|e| e.to_string())?;
    let mut tracks = Vec::new();
    collect_apple(&v, &mut tracks);
    // the page's own title: "Name - Playlist - Apple Music"
    let name = html
        .split("<title>")
        .nth(1)
        .and_then(|t| t.split("</title>").next())
        .map(|t| html_unescape(t.split(" - ").next().unwrap_or(t).split(" by ").next().unwrap_or(t).trim()))
        .unwrap_or_else(|| "Apple Music".into());
    let cover = html.split(r#"property="og:image" content=""#).nth(1).and_then(|t| t.split('"').next()).map(String::from);
    let mut seen = std::collections::HashSet::new();
    tracks.retain(|t| seen.insert((t.title.clone(), t.artist.clone())));
    // the share picture has white borders: the first song's square artwork is the cover
    let cover = tracks.first().and_then(|t| t.cover.clone()).or(cover);
    if tracks.is_empty() {
        return Err("Apple Music: no songs found (is the playlist public?)".into());
    }
    Ok(Found { name, cover, tracks, service: Service::Apple })
}

fn collect_apple(v: &Value, out: &mut Vec<Track>) {
    match v {
        Value::Object(o) => {
            if let (Some(title), Some(artist), Some(dur)) =
                (o.get("title").and_then(|x| x.as_str()), o.get("artistName").and_then(|x| x.as_str()), o.get("duration").and_then(|x| x.as_f64()))
            {
                let album = o.get("tertiaryLinks").and_then(|l| l.get(0)).and_then(|l| l["title"].as_str()).unwrap_or("");
                let album = album.trim_end_matches(" - Single").trim_end_matches(" - EP").to_string();
                let cover = o
                    .get("artwork")
                    .and_then(|a| a.pointer("/dictionary/url"))
                    .and_then(|u| u.as_str())
                    .map(|u| u.replace("{w}", "600").replace("{h}", "600").replace("{c}", "bb").replace("{f}", "jpg"));
                out.push(Track { title: title.into(), artist: artist.into(), album, dur: dur / 1000.0, cover, ..Default::default() });
                return;
            }
            for x in o.values() {
                collect_apple(x, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                collect_apple(x, out);
            }
        }
        _ => {}
    }
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .chars()
        // invisible direction marks Apple puts around names
        .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{feff}'))
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services() {
        assert_eq!(service("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=x"), Some(Service::Spotify));
        assert_eq!(service("https://music.apple.com/us/playlist/x/pl.1"), Some(Service::Apple));
        assert_eq!(service("https://soundcloud.com/a/sets/b"), Some(Service::SoundCloud));
        assert_eq!(service("https://www.youtube.com/playlist?list=PL1"), Some(Service::YouTube));
        assert_eq!(service("hello"), None);
        assert_eq!(service("https://www.deezer.com/fr/playlist/1071669561"), Some(Service::Deezer));
        assert_eq!(deezer_ref("https://www.deezer.com/fr/playlist/1071669561?utm=x"), Some(("playlist".into(), "1071669561".into())));
        assert_eq!(deezer_ref("https://www.deezer.com/album/302127"), Some(("album".into(), "302127".into())));
        let copied = "https://open.spotify.com/track/3YeJXuRSNS5FYwOsDu44kD\r\nhttps://open.spotify.com/track/1bDbXMyjaUIooNwFE9wn0N?si=x\r\nspotify:track:3YeJXuRSNS5FYwOsDu44kD";
        assert_eq!(spotify_track_ids(copied), vec!["3YeJXuRSNS5FYwOsDu44kD".to_string(), "1bDbXMyjaUIooNwFE9wn0N".to_string()]);
        assert!(spotify_track_ids("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M").is_empty());
    }

    #[test]
    #[ignore] // network
    fn live_spotify_and_apple() {
        let f = read("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M").unwrap();
        assert!(f.tracks.len() >= 20 && f.tracks.iter().all(|t| !t.title.is_empty() && t.dur > 30.0), "{}", f.tracks.len());
        let d = read("https://www.deezer.com/playlist/1071669561").unwrap();
        assert!(d.tracks.len() >= 20 && d.tracks.iter().all(|t| !t.title.is_empty() && !t.artist.is_empty() && t.dur > 30.0), "{}", d.tracks.len());
        let t = spotify_track("3YeJXuRSNS5FYwOsDu44kD").expect("song page");
        assert!(t.title.contains("Dark Knight Dummo") && t.artist.contains("Trippie Redd") && t.dur > 200.0, "{t:?}");
        let f = read("https://music.apple.com/us/playlist/todays-hits/pl.f4d106fed2bd41149aaacabb233eb5eb").unwrap();
        assert!(f.tracks.len() >= 20, "{}", f.tracks.len());
        assert!(f.tracks.iter().all(|t| !t.artist.is_empty()));
    }
}
