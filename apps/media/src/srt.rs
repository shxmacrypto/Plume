//! Subtitles from an .srt file next to the video ("Movie.srt", "Movie.en.srt", "Movie.fr.srt"…).

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// "00:01:02,500" → 62.5
fn time(s: &str) -> Option<f64> {
    let s = s.trim();
    let (hms, ms) = s.split_once([',', '.']).unwrap_or((s, "0"));
    let mut parts = hms.split(':').map(|p| p.trim().parse::<f64>());
    let (h, m, sec) = match (parts.next(), parts.next(), parts.next()) {
        (Some(Ok(h)), Some(Ok(m)), Some(Ok(s))) => (h, m, s),
        (Some(Ok(m)), Some(Ok(s)), None) => (0.0, m, s),
        _ => return None,
    };
    let digits: String = ms.chars().take_while(|c| c.is_ascii_digit()).collect();
    let frac = if digits.is_empty() { 0.0 } else { digits.parse::<f64>().ok()? / 10f64.powi(digits.len() as i32) };
    Some(h * 3600.0 + m * 60.0 + sec + frac)
}

/// Removes <i>, <b>, <font …> and {\an8}-style marks.
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0i32;
    let mut brace = false;
    for ch in s.chars() {
        match ch {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            '{' => brace = true,
            '}' if brace => brace = false,
            _ if depth == 0 && !brace => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}

pub fn parse(text: &str) -> Vec<Cue> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = vec![];
    for block in text.split("\n\n") {
        let mut lines = block.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).peekable();
        // the number line is optional
        if lines.peek().is_some_and(|l| !l.contains("-->")) {
            lines.next();
        }
        let Some(timing) = lines.next() else { continue };
        let Some((a, b)) = timing.split_once("-->") else { continue };
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (time(a), time(b)) else { continue };
        let body = lines.map(clean).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
        if !body.is_empty() && end > start {
            cues.push(Cue { start, end, text: body });
        }
    }
    cues.sort_by(|a, b| a.start.total_cmp(&b.start));
    cues
}

/// Reads an .srt file (UTF-8, or Windows-1252 for older files).
pub fn load(path: &Path) -> Vec<Cue> {
    let Ok(bytes) = std::fs::read(path) else { return vec![] };
    let text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
    };
    parse(&text)
}

/// The text shown at `t` (several cues can overlap).
pub fn at(cues: &[Cue], t: f64) -> Option<String> {
    let shown: Vec<&str> = cues.iter().take_while(|c| c.start <= t).filter(|c| c.end > t).map(|c| c.text.as_str()).collect();
    (!shown.is_empty()).then(|| shown.join("\n"))
}

/// Subtitle files next to a video: same name, any language suffix.
pub fn find(video: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (video.parent(), video.file_stem()) else { return vec![] };
    let stem = stem.to_string_lossy().to_lowercase();
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                    name.ends_with(".srt") && (name == format!("{stem}.srt") || name.starts_with(&format!("{stem}.")))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// "Movie.fr.srt" → "fr" ; "Movie.srt" → "".
pub fn label(video: &Path, srt: &Path) -> String {
    let stem = video.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let name = srt.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    name.get(stem.len()..).unwrap_or("").trim_start_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_srt() {
        let s = "\u{feff}1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Hello</i>\r\nthere\r\n\r\n2\r\n00:00:03.250 --> 00:00:04,000 X1:0\r\n{\\an8}Top\r\n";
        let c = parse(s);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].text, "Hello\nthere");
        assert_eq!(c[1].start, 3.25);
        assert_eq!(c[1].text, "Top");
        assert_eq!(at(&c, 1.5).as_deref(), Some("Hello\nthere"));
        assert_eq!(at(&c, 2.7), None);
    }

    #[test]
    fn labels() {
        assert_eq!(label(Path::new("C:/v/Movie.mp4"), Path::new("C:/v/Movie.fr.srt")), "fr");
        assert_eq!(label(Path::new("C:/v/Movie.mp4"), Path::new("C:/v/Movie.srt")), "");
    }
}
