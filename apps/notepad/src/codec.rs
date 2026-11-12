//! Bytes of a file to text and back: the encoding (UTF-8 with or without BOM, UTF-16, the ANSI code page of
//! Windows) and the line endings (Windows CRLF, Unix LF, old Mac CR).

use serde::{Deserialize, Serialize};
use windows::{
    Win32::Globalization::{GetACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar, WC_NO_BEST_FIT_CHARS, WideCharToMultiByte},
    core::{BOOL, PCSTR},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    /// The ANSI code page of this PC (Windows-1252 in western Europe).
    Ansi,
}

pub const ENCODINGS: [Encoding; 5] = [Encoding::Utf8, Encoding::Utf8Bom, Encoding::Utf16Le, Encoding::Utf16Be, Encoding::Ansi];

impl Encoding {
    pub fn label(self) -> String {
        match self {
            Encoding::Utf8 => "UTF-8".into(),
            Encoding::Utf8Bom => "UTF-8 BOM".into(),
            Encoding::Utf16Le => "UTF-16 LE".into(),
            Encoding::Utf16Be => "UTF-16 BE".into(),
            Encoding::Ansi => format!("ANSI ({})", ansi_name()),
        }
    }
    /// Short name for the test socket and the session file.
    pub fn key(self) -> &'static str {
        match self {
            Encoding::Utf8 => "utf8",
            Encoding::Utf8Bom => "utf8bom",
            Encoding::Utf16Le => "utf16le",
            Encoding::Utf16Be => "utf16be",
            Encoding::Ansi => "ansi",
        }
    }
    #[cfg_attr(not(any(test, feature = "test-hook")), allow(dead_code))]
    pub fn from_key(k: &str) -> Option<Encoding> {
        ENCODINGS.into_iter().find(|e| e.key() == k)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Eol {
    #[default]
    Crlf,
    Lf,
    Cr,
}

pub const EOLS: [Eol; 3] = [Eol::Crlf, Eol::Lf, Eol::Cr];

impl Eol {
    pub fn label(self) -> &'static str {
        match self {
            Eol::Crlf => "Windows (CRLF)",
            Eol::Lf => "Unix (LF)",
            Eol::Cr => "Macintosh (CR)",
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Eol::Crlf => "crlf",
            Eol::Lf => "lf",
            Eol::Cr => "cr",
        }
    }
    #[cfg_attr(not(any(test, feature = "test-hook")), allow(dead_code))]
    pub fn from_key(k: &str) -> Option<Eol> {
        EOLS.into_iter().find(|e| e.key() == k)
    }
}

pub fn ansi_page() -> u32 {
    unsafe { GetACP() }
}

fn ansi_name() -> String {
    match ansi_page() {
        p @ 1250..=1258 => format!("Windows-{p}"),
        932 => "Shift-JIS".into(),
        936 => "GBK".into(),
        949 => "EUC-KR".into(),
        950 => "Big5".into(),
        p => format!("CP{p}"),
    }
}

/// What was read from a file.
#[derive(Debug, PartialEq)]
pub struct Decoded {
    /// Lines separated by '\n' only.
    pub text: String,
    pub enc: Encoding,
    pub eol: Eol,
    /// Some bytes did not fit the encoding found and were replaced.
    pub lossy: bool,
}

/// '\r\n' and lone '\r' become '\n'. Also says which ending was used most (CRLF when there is none).
pub fn normalize(s: &str) -> (String, Eol) {
    if !s.contains('\r') {
        let lf = s.bytes().filter(|&b| b == b'\n').count();
        return (s.to_string(), if lf > 0 { Eol::Lf } else { Eol::Crlf });
    }
    let (mut crlf, mut lf, mut cr) = (0usize, 0usize, 0usize);
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut last = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\r' => {
                out.push_str(&s[last..i]);
                out.push('\n');
                if b.get(i + 1) == Some(&b'\n') {
                    crlf += 1;
                    i += 1;
                } else {
                    cr += 1;
                }
                last = i + 1;
            }
            b'\n' => lf += 1,
            _ => {}
        }
        i += 1;
    }
    out.push_str(&s[last..]);
    let eol = if crlf >= lf && crlf >= cr {
        Eol::Crlf
    } else if lf >= cr {
        Eol::Lf
    } else {
        Eol::Cr
    };
    (out, eol)
}

fn utf16(bytes: &[u8], be: bool) -> (String, bool) {
    let units: Vec<u16> =
        bytes.as_chunks::<2>().0.iter().map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect();
    let mut lossy = bytes.len() % 2 == 1;
    let s: String = char::decode_utf16(units)
        .map(|r| {
            r.unwrap_or_else(|_| {
                lossy = true;
                '\u{FFFD}'
            })
        })
        .collect();
    (s, lossy)
}

/// Bytes of a Windows code page as text.
pub fn from_page(bytes: &[u8], page: u32) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    unsafe {
        let n = MultiByteToWideChar(page, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
        if n <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut w = vec![0u16; n as usize];
        let n = MultiByteToWideChar(page, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, Some(&mut w));
        String::from_utf16_lossy(&w[..n.max(0) as usize])
    }
}

/// Text in a Windows code page; the flag says that some characters had no place in it (written as "?").
pub fn to_page(s: &str, page: u32) -> (Vec<u8>, bool) {
    if s.is_empty() {
        return (Vec::new(), false);
    }
    let w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let mut used = BOOL(0);
        // UTF-7 and UTF-8 refuse the flags and the "default char" question
        let utf = page == 65000 || page == 65001;
        let flags = if utf { 0 } else { WC_NO_BEST_FIT_CHARS };
        let used_ptr = if utf { None } else { Some(&mut used as *mut BOOL) };
        let n = WideCharToMultiByte(page, flags, &w, None, PCSTR::null(), used_ptr);
        if n <= 0 {
            return (s.as_bytes().to_vec(), true);
        }
        let mut out = vec![0u8; n as usize];
        let used_ptr = if utf { None } else { Some(&mut used as *mut BOOL) };
        let n = WideCharToMultiByte(page, flags, &w, Some(&mut out), PCSTR::null(), used_ptr);
        out.truncate(n.max(0) as usize);
        (out, used.as_bool())
    }
}

/// UTF-16 without a BOM: one byte of each pair is mostly zero (plain Latin text).
fn looks_utf16(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 4 || bytes.len() % 2 == 1 {
        return None;
    }
    let sample = &bytes[..bytes.len().min(8192)];
    let pairs = sample.len() / 2;
    let (mut even, mut odd) = (0usize, 0usize);
    for c in sample.as_chunks::<2>().0 {
        even += (c[0] == 0) as usize;
        odd += (c[1] == 0) as usize;
    }
    if odd * 10 >= pairs * 4 && even * 10 < pairs {
        Some(false)
    } else if even * 10 >= pairs * 4 && odd * 10 < pairs {
        Some(true)
    } else {
        None
    }
}

/// Reads bytes as the encoding found: a BOM first, then valid UTF-8, UTF-16 without a BOM, else ANSI.
pub fn decode(bytes: &[u8]) -> Decoded {
    let enc = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Encoding::Utf8Bom
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Encoding::Utf16Le
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Encoding::Utf16Be
    } else if let Some(be) = looks_utf16(bytes) {
        // zero bytes are valid UTF-8 but never in a real text file: UTF-16 without a BOM
        if be { Encoding::Utf16Be } else { Encoding::Utf16Le }
    } else if std::str::from_utf8(bytes).is_ok() {
        Encoding::Utf8
    } else {
        Encoding::Ansi
    };
    decode_as(bytes, enc)
}

/// Reads bytes as a given encoding ("Reopen with"); a BOM of that encoding is skipped.
pub fn decode_as(bytes: &[u8], enc: Encoding) -> Decoded {
    let (raw, lossy) = match enc {
        Encoding::Utf8 | Encoding::Utf8Bom => {
            let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
            match std::str::from_utf8(b) {
                Ok(s) => (s.to_string(), false),
                Err(_) => (String::from_utf8_lossy(b).into_owned(), true),
            }
        }
        Encoding::Utf16Le => utf16(bytes.strip_prefix(&[0xFF, 0xFE][..]).unwrap_or(bytes), false),
        Encoding::Utf16Be => utf16(bytes.strip_prefix(&[0xFE, 0xFF][..]).unwrap_or(bytes), true),
        Encoding::Ansi => (from_page(bytes, ansi_page()), false),
    };
    let (text, eol) = normalize(&raw);
    Decoded { text, enc, eol, lossy }
}

/// The bytes of a file: `text` has '\n' between lines. The flag says that some characters could not be written
/// (ANSI only).
pub fn encode(text: &str, enc: Encoding, eol: Eol) -> (Vec<u8>, bool) {
    let owned;
    let s = if eol == Eol::Lf {
        text
    } else {
        owned = text.replace('\n', eol.as_str());
        &owned
    };
    match enc {
        Encoding::Utf8 => (s.as_bytes().to_vec(), false),
        Encoding::Utf8Bom => {
            let mut v = Vec::with_capacity(s.len() + 3);
            v.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            v.extend_from_slice(s.as_bytes());
            (v, false)
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let be = enc == Encoding::Utf16Be;
            let mut v = Vec::with_capacity(s.len() * 2 + 2);
            v.extend_from_slice(if be { &[0xFE, 0xFF] } else { &[0xFF, 0xFE] });
            for u in s.encode_utf16() {
                v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
            }
            (v, false)
        }
        Encoding::Ansi => to_page(s, ansi_page()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings() {
        assert_eq!(normalize("a\r\nb\r\nc\nd"), ("a\nb\nc\nd".to_string(), Eol::Crlf));
        assert_eq!(normalize("a\nb\nc"), ("a\nb\nc".to_string(), Eol::Lf));
        assert_eq!(normalize("a\rb\rc\r\n"), ("a\nb\nc\n".to_string(), Eol::Cr));
        assert_eq!(normalize("one line").1, Eol::Crlf);
        let (bytes, _) = encode("a\nb", Encoding::Utf8, Eol::Crlf);
        assert_eq!(bytes, b"a\r\nb");
        let (bytes, _) = encode("a\nb", Encoding::Utf8, Eol::Cr);
        assert_eq!(bytes, b"a\rb");
    }

    #[test]
    fn utf8_with_and_without_bom() {
        let d = decode("héllo\r\nwörld".as_bytes());
        assert_eq!((d.text.as_str(), d.enc, d.eol, d.lossy), ("héllo\nwörld", Encoding::Utf8, Eol::Crlf, false));
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice("é".as_bytes());
        let d = decode(&bom);
        assert_eq!((d.text.as_str(), d.enc), ("é", Encoding::Utf8Bom));
        let (out, lossy) = encode("é", Encoding::Utf8Bom, Eol::Crlf);
        assert_eq!((out, lossy), (bom, false));
    }

    #[test]
    fn utf16_both_ways() {
        for enc in [Encoding::Utf16Le, Encoding::Utf16Be] {
            let (bytes, _) = encode("Ça va 😀\nok", enc, Eol::Crlf);
            assert_eq!(bytes.len(), 2 + 2 * "Ça va 😀\r\nok".encode_utf16().count());
            let d = decode(&bytes);
            assert_eq!((d.text.as_str(), d.enc, d.eol), ("Ça va 😀\nok", enc, Eol::Crlf));
        }
        // no BOM: found by its zero bytes
        let le: Vec<u8> = "plain text here".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(decode(&le).enc, Encoding::Utf16Le);
        let be: Vec<u8> = "plain text here".encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        assert_eq!(decode(&be).enc, Encoding::Utf16Be);
    }

    #[test]
    fn ansi_detection_and_code_pages() {
        // "café crème" in Windows-1252: not valid UTF-8
        let bytes = [b'c', b'a', b'f', 0xE9, b' ', b'c', b'r', 0xE8, b'm', b'e'];
        assert_eq!(decode(&bytes).enc, Encoding::Ansi);
        assert_eq!(from_page(&bytes, 1252), "café crème");
        assert_eq!(to_page("café crème", 1252), (bytes.to_vec(), false));
        // a character Windows-1252 does not have
        let (out, lossy) = to_page("a€b✓", 1252);
        assert!(lossy);
        assert_eq!(out[..3], [b'a', 0x80, b'b']);
        if ansi_page() == 1252 {
            let d = decode(&bytes);
            assert_eq!(d.text, "café crème");
        }
    }

    #[test]
    fn reopen_as_other_encoding() {
        let d = decode_as("é".as_bytes(), Encoding::Utf16Le);
        assert_eq!(d.text.chars().count(), 1);
        let d = decode_as(&[0xC3, 0x28], Encoding::Utf8);
        assert!(d.lossy);
        assert_eq!(Encoding::from_key("utf16be"), Some(Encoding::Utf16Be));
        assert_eq!(Eol::from_key("lf"), Some(Eol::Lf));
    }
}
