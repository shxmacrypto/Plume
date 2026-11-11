//! Small pieces with no window and no network: the secret code, safe file names, sizes, links, URL decoding.

use std::path::{Path, PathBuf};

const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Random letters and digits from Windows' generator (`n` × ~5.95 bits).
pub fn random_text(n: usize) -> String {
    let mut out = String::with_capacity(n);
    let mut buf = [0u8; 64];
    while out.len() < n {
        if getrandom::fill(&mut buf).is_err() {
            // never seen; still no weak fallback: the time alone would be guessable
            panic!("no random numbers");
        }
        // 248 = 4 × 62: keeping only bytes below it leaves no bias
        for b in buf.iter().filter(|b| **b < 248) {
            if out.len() == n {
                break;
            }
            out.push(ALPHABET[(*b % 62) as usize] as char);
        }
    }
    out
}

/// The code in the QR code: 22 characters, about 131 bits.
pub fn new_token() -> String {
    random_text(22)
}

/// Same bytes, in a time that depends only on the length (a wrong guess tells nothing about the right one).
pub fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

/// A name a phone sends, made safe for Windows: no folders, no forbidden characters, no reserved names.
pub fn safe_name(raw: &str) -> String {
    // the last part only: a name like "../../x" or "C:\x" must not leave the folder
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let mut s: String = base.chars().map(|c| if c < ' ' || "<>:\"|?*".contains(c) { '_' } else { c }).collect();
    // Windows drops trailing dots and spaces itself, which could change the name we checked
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    let s = s.trim_start().to_string();
    let mut s = if s.is_empty() { "file".to_string() } else { s };
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4 && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        s.insert(0, '_');
    }
    // long names: keep the extension
    if s.chars().count() > 150 {
        let ext = Path::new(&s).extension().map(|e| e.to_string_lossy().to_string()).filter(|e| e.len() <= 12);
        let keep: String = s.chars().take(140).collect();
        s = match ext {
            Some(e) => format!("{keep}.{e}"),
            None => keep,
        };
    }
    s
}

/// `name` in `dir`, or "name (2).ext", "name (3).ext"… if taken.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string());
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..10_000).map(|i| dir.join(format!("{stem} ({i}){ext}"))).find(|c| !c.exists()).unwrap_or(first)
}

/// 1536 → "1.5 KB".
pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else if v < 10.0 {
        format!("{v:.1} {}", UNITS[u])
    } else {
        format!("{v:.0} {}", UNITS[u])
    }
}

/// The text is one web address (a link to open, not a text to copy).
pub fn is_link(text: &str) -> bool {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")) && t.len() > 8 && !t.chars().any(char::is_whitespace)
}

/// "%C3%A9t%C3%A9+x" → "été x" (query strings). Bad escapes are kept as they are.
pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// For a `filename*=UTF-8''…` header: every byte outside a few safe ones escaped.
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The type a browser needs to show a picture or play a video; everything else is a plain download.
pub fn mime(name: &str) -> &'static str {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "bmp" => "image/bmp",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// A picture the phone can show as a preview.
pub fn is_image(name: &str) -> bool {
    matches!(mime(name), "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/bmp")
}

/// Seconds since 1970.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let a = new_token();
        assert_eq!(a.len(), 22);
        assert!(a.bytes().all(|b| ALPHABET.contains(&b)));
        assert_ne!(a, new_token());
    }

    #[test]
    fn secrets() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "abcd"));
        assert!(!same_secret("", ""));
    }

    #[test]
    fn names() {
        assert_eq!(safe_name("photo.jpg"), "photo.jpg");
        assert_eq!(safe_name("../../Windows/evil.exe"), "evil.exe");
        assert_eq!(safe_name(r"C:\Users\x\a.txt"), "a.txt");
        assert_eq!(safe_name("a<b>c:d.txt"), "a_b_c_d.txt");
        assert_eq!(safe_name("CON.txt"), "_CON.txt");
        assert_eq!(safe_name("com1"), "_com1");
        assert_eq!(safe_name("console.txt"), "console.txt");
        assert_eq!(safe_name("name. . "), "name");
        assert_eq!(safe_name(""), "file");
        assert_eq!(safe_name(".."), "file");
        assert_eq!(safe_name("a\u{0}b"), "a_b");
        let long = format!("{}.jpeg", "x".repeat(300));
        let s = safe_name(&long);
        assert!(s.ends_with(".jpeg") && s.chars().count() <= 150);
        assert_eq!(safe_name("été 🌞.png"), "été 🌞.png");
    }

    #[test]
    fn unique() {
        let dir = std::env::temp_dir().join(format!("plume-drop-test-{}", random_text(8)));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, "a.txt"), dir.join("a.txt"));
        std::fs::write(dir.join("a.txt"), b"1").unwrap();
        assert_eq!(unique_path(&dir, "a.txt"), dir.join("a (2).txt"));
        std::fs::write(dir.join("a (2).txt"), b"1").unwrap();
        assert_eq!(unique_path(&dir, "a.txt"), dir.join("a (3).txt"));
        std::fs::write(dir.join("noext"), b"1").unwrap();
        assert_eq!(unique_path(&dir, "noext"), dir.join("noext (2)"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(25 * 1024 * 1024), "25 MB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn links() {
        assert!(is_link("https://example.com/a?b=c"));
        assert!(is_link("  HTTP://x.org  "));
        assert!(!is_link("see https://example.com"));
        assert!(!is_link("ftp://x"));
        assert!(!is_link("https://"));
    }

    #[test]
    fn decoding() {
        assert_eq!(url_decode("a%20b+c"), "a b c");
        assert_eq!(url_decode("%C3%A9t%C3%A9.jpg"), "été.jpg");
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("%zz"), "%zz");
        assert_eq!(url_encode("été 1.jpg"), "%C3%A9t%C3%A9%201.jpg");
    }

    #[test]
    fn types() {
        assert_eq!(mime("A.JPG"), "image/jpeg");
        assert!(is_image("x.png"));
        assert!(!is_image("x.heic"));
        assert_eq!(mime("x.exe"), "application/octet-stream");
    }
}
