//! Reading and writing the Windows clipboard.
//!
//! What is copied is read when Windows says the clipboard changed (AddClipboardFormatListener, no polling): the
//! copied files, else the text (with its HTML and RTF formatting), else the picture. Content an app marks as private
//! is never read: password managers add `ExcludeClipboardContentFromMonitorProcessing`, or set
//! `CanIncludeInClipboardHistory` / `CanUploadToCloudClipboard` to 0.

use crate::store::Kind;
use std::sync::OnceLock;
use windows::{
    Win32::{
        Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND},
        System::{
            DataExchange::*,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
            Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
        },
        UI::WindowsAndMessaging::GetWindowThreadProcessId,
    },
    core::{HSTRING, PWSTR},
};

pub const CF_DIB: u32 = 8;
pub const CF_UNICODETEXT: u32 = 13;
pub const CF_HDROP: u32 = 15;
pub const CF_DIBV5: u32 = 17;

/// The formats Windows numbers when they are first asked for.
pub struct Formats {
    pub exclude: u32,
    pub include: u32,
    pub upload: u32,
    pub viewer_ignore: u32,
    pub html: u32,
    pub rtf: u32,
    pub png: u32,
    pub drop_effect: u32,
}

pub fn formats() -> &'static Formats {
    static F: OnceLock<Formats> = OnceLock::new();
    F.get_or_init(|| {
        let r = |n: &str| unsafe { RegisterClipboardFormatW(&HSTRING::from(n)) };
        Formats {
            exclude: r("ExcludeClipboardContentFromMonitorProcessing"),
            include: r("CanIncludeInClipboardHistory"),
            upload: r("CanUploadToCloudClipboard"),
            viewer_ignore: r("Clipboard Viewer Ignore"),
            html: r("HTML Format"),
            rtf: r("Rich Text Format"),
            png: r("PNG"),
            drop_effect: r("Preferred DropEffect"),
        }
    })
}

/// Private content: the formats apps add so that clipboard histories leave it alone. `include` and `upload` are
/// the DWORD values of `CanIncludeInClipboardHistory` and `CanUploadToCloudClipboard` when present.
pub fn is_private(exclude: bool, include: Option<u32>, upload: Option<u32>, viewer_ignore: bool) -> bool {
    exclude || viewer_ignore || include == Some(0) || upload == Some(0)
}

/// A link: one word that starts like an address.
pub fn is_link(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.len() > 4096 || t.chars().any(char::is_whitespace) {
        return false;
    }
    let l = t.to_lowercase();
    for p in ["http://", "https://", "ftp://", "mailto:", "file://"] {
        if let Some(rest) = l.strip_prefix(p) {
            return !rest.is_empty();
        }
    }
    l.strip_prefix("www.").is_some_and(|rest| rest.contains('.') && !rest.starts_with('.') && !rest.ends_with('.'))
}

pub fn kind_of_text(s: &str) -> Kind {
    if is_link(s) { Kind::Link } else { Kind::Text }
}

/// Texts larger than this are not kept (a whole log file copied by mistake).
pub const MAX_TEXT: usize = 4 << 20;
/// The formatting is kept up to this size.
const MAX_FORMAT: usize = 1 << 20;

#[derive(Debug, PartialEq)]
pub enum Captured {
    Text {
        text: String,
        html: Option<String>,
        rtf: Option<String>,
    },
    /// The bytes as the clipboard has them: a PNG, or a DIB.
    Image {
        data: Vec<u8>,
        png: bool,
    },
    Files(Vec<String>),
}

#[derive(Debug, PartialEq)]
pub enum Read {
    /// Another program holds the clipboard: try again a little later.
    Busy,
    /// Nothing to keep, and why (for the log).
    Skip(&'static str),
    Got(Captured),
}

/// The list of a CF_HDROP (DROPFILES followed by the paths).
pub fn parse_dropfiles(b: &[u8]) -> Vec<String> {
    if b.len() < 20 {
        return vec![];
    }
    let start = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let wide = u32::from_le_bytes([b[16], b[17], b[18], b[19]]) != 0;
    let Some(rest) = b.get(start..) else { return vec![] };
    let mut out = Vec::new();
    if wide {
        let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        for part in units.split(|u| *u == 0) {
            if part.is_empty() {
                break;
            }
            out.push(String::from_utf16_lossy(part));
        }
    } else {
        for part in rest.split(|u| *u == 0) {
            if part.is_empty() {
                break;
            }
            out.push(String::from_utf8_lossy(part).into_owned());
        }
    }
    out
}

pub fn make_dropfiles(files: &[String]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&20u32.to_le_bytes());
    b.extend_from_slice(&[0u8; 12]); // pt, fNC
    b.extend_from_slice(&1u32.to_le_bytes()); // fWide
    for f in files {
        for u in f.encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        b.extend_from_slice(&[0, 0]);
    }
    b.extend_from_slice(&[0, 0]);
    b
}

/// A CF_UNICODETEXT block up to its end.
pub fn utf16_text(b: &[u8]) -> String {
    let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|u| *u != 0).collect();
    String::from_utf16_lossy(&units)
}

/// A text format (HTML, RTF) up to its end, if it is valid UTF-8.
fn utf8_text(b: &[u8]) -> Option<String> {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    std::str::from_utf8(&b[..end]).ok().map(str::to_string)
}

/// The bytes of a format (the clipboard is open).
unsafe fn bytes(fmt: u32, limit: usize) -> Option<Vec<u8>> {
    unsafe {
        let h = GetClipboardData(fmt).ok()?;
        let g = HGLOBAL(h.0);
        let size = GlobalSize(g);
        if size == 0 || size > limit {
            return None;
        }
        let p = GlobalLock(g) as *const u8;
        if p.is_null() {
            return None;
        }
        let v = std::slice::from_raw_parts(p, size).to_vec();
        let _ = GlobalUnlock(g);
        Some(v)
    }
}

fn has(fmt: u32) -> bool {
    unsafe { IsClipboardFormatAvailable(fmt).is_ok() }
}

unsafe fn dword(fmt: u32) -> Option<u32> {
    if !has(fmt) {
        return None;
    }
    // present but unreadable: counted as "no"
    let b = unsafe { bytes(fmt, 64) }.unwrap_or_default();
    Some(if b.len() >= 4 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { 0 })
}

/// The exe name of the program behind a window ("notepad.exe"), lowercase.
pub fn exe_of(hwnd: HWND) -> String {
    if hwnd.0.is_null() {
        return String::new();
    }
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return String::new();
        }
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase()
    }
}

/// The program that put the current content on the clipboard.
pub fn owner_app() -> String {
    unsafe { GetClipboardOwner() }.map(exe_of).unwrap_or_default()
}

pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// Reads what was copied. `images`, `files`: whether they are kept; `max_image`: largest picture, in bytes.
pub fn read(hwnd: HWND, images: bool, files: bool, max_image: usize) -> Read {
    read_any(hwnd, images, files, max_image, true)
}

/// `private`: whether private content is refused (always, except to check a test's own write).
pub fn read_any(hwnd: HWND, images: bool, files: bool, max_image: usize, private: bool) -> Read {
    let f = formats();
    unsafe {
        if OpenClipboard(Some(hwnd)).is_err() {
            return Read::Busy;
        }
        let r = read_open(f, images, files, max_image, private);
        let _ = CloseClipboard();
        r
    }
}

unsafe fn read_open(f: &Formats, images: bool, files: bool, max_image: usize, private: bool) -> Read {
    unsafe {
        if private && is_private(has(f.exclude), dword(f.include), dword(f.upload), has(f.viewer_ignore)) {
            return Read::Skip("private");
        }
        if has(CF_HDROP) {
            if !files {
                return Read::Skip("files are not kept");
            }
            let list = bytes(CF_HDROP, 16 << 20).map(|b| parse_dropfiles(&b)).unwrap_or_default();
            return if list.is_empty() { Read::Skip("no files") } else { Read::Got(Captured::Files(list)) };
        }
        if has(CF_UNICODETEXT) {
            let Some(b) = bytes(CF_UNICODETEXT, MAX_TEXT * 2 + 2) else { return Read::Skip("text too large") };
            let text = utf16_text(&b);
            if text.trim().is_empty() {
                return Read::Skip("empty text");
            }
            let html = if has(f.html) { bytes(f.html, MAX_FORMAT).and_then(|b| utf8_text(&b)) } else { None };
            let rtf = if has(f.rtf) { bytes(f.rtf, MAX_FORMAT).and_then(|b| utf8_text(&b)) } else { None };
            return Read::Got(Captured::Text { text, html, rtf });
        }
        for (fmt, png) in [(f.png, true), (CF_DIBV5, false), (CF_DIB, false)] {
            if has(fmt) {
                if !images {
                    return Read::Skip("pictures are not kept");
                }
                // a DIB is about four times its PNG: a generous bound before encoding
                return match bytes(fmt, if png { max_image } else { max_image.saturating_mul(6) }) {
                    Some(data) => Read::Got(Captured::Image { data, png }),
                    None => Read::Skip("picture too large"),
                };
            }
        }
        Read::Skip("nothing kept")
    }
}

/// What to put on the clipboard.
pub enum Put<'a> {
    Text { text: &'a str, html: Option<&'a str>, rtf: Option<&'a str> },
    Image { png: &'a [u8], dib: &'a [u8] },
    Files(&'a [String]),
}

unsafe fn set(fmt: u32, data: &[u8]) -> bool {
    unsafe {
        let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)) else { return false };
        let p = GlobalLock(mem) as *mut u8;
        if p.is_null() {
            let _ = GlobalFree(Some(mem));
            return false;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
        let _ = GlobalUnlock(mem);
        // the clipboard owns the memory from now on, unless it refused it
        if SetClipboardData(fmt, Some(HANDLE(mem.0))).is_err() {
            let _ = GlobalFree(Some(mem));
            return false;
        }
        true
    }
}

fn wide_z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect()
}

fn bytes_z(s: &str) -> Vec<u8> {
    s.bytes().chain(Some(0)).collect()
}

/// Puts an item on the clipboard. Returns the clipboard's new sequence number (to recognize this change).
pub fn write(hwnd: HWND, put: &Put) -> Option<u32> {
    write_ex(hwnd, put, false)
}

/// `private`: marked so that clipboard histories (Windows' own, Plume's) leave it alone (tests).
pub fn write_ex(hwnd: HWND, put: &Put, private: bool) -> Option<u32> {
    let f = formats();
    unsafe {
        let mut tries = 0;
        while OpenClipboard(Some(hwnd)).is_err() {
            tries += 1;
            if tries > 10 {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        let _ = EmptyClipboard();
        let ok = match put {
            Put::Text { text, html, rtf } => {
                let ok = set(CF_UNICODETEXT, &wide_z(text));
                if let Some(h) = html {
                    set(f.html, &bytes_z(h));
                }
                if let Some(r) = rtf {
                    set(f.rtf, &bytes_z(r));
                }
                ok
            }
            Put::Image { png, dib } => {
                let a = set(f.png, png);
                let b = set(CF_DIB, dib);
                a || b
            }
            Put::Files(list) => {
                let ok = set(CF_HDROP, &make_dropfiles(list));
                set(f.drop_effect, &1u32.to_le_bytes());
                ok
            }
        };
        if private {
            set(f.include, &0u32.to_le_bytes());
            set(f.upload, &0u32.to_le_bytes());
        }
        let _ = CloseClipboard();
        ok.then(sequence)
    }
}

/// Everything on the clipboard, to put it back as it was (tests: the user's clipboard is theirs).
#[cfg(feature = "test-hook")]
pub mod saved {
    use super::*;
    use windows::Win32::Graphics::Gdi::{GetEnhMetaFileBits, HENHMETAFILE, SetEnhMetaFileBits};

    const CF_BITMAP: u32 = 2;
    const CF_METAFILEPICT: u32 = 3;
    const CF_PALETTE: u32 = 9;
    const CF_ENHMETAFILE: u32 = 14;

    #[derive(Debug, PartialEq)]
    pub struct Saved(pub Vec<(u32, Vec<u8>)>);

    /// Formats whose data is not memory (GDI objects, owner drawing): Windows rebuilds the usual ones.
    fn is_gdi(fmt: u32) -> bool {
        matches!(fmt, CF_BITMAP | CF_METAFILEPICT | CF_PALETTE | 0x80 | 0x82 | 0x83 | 0x8E) || (0x300..=0x3FF).contains(&fmt)
    }

    pub fn save(hwnd: HWND) -> Option<Saved> {
        unsafe {
            OpenClipboard(Some(hwnd)).ok()?;
            let mut out = Vec::new();
            let mut fmt = 0u32;
            loop {
                fmt = EnumClipboardFormats(fmt);
                if fmt == 0 {
                    break;
                }
                if fmt == CF_ENHMETAFILE {
                    if let Ok(h) = GetClipboardData(fmt) {
                        let emf = HENHMETAFILE(h.0);
                        let n = GetEnhMetaFileBits(emf, None);
                        let mut buf = vec![0u8; n as usize];
                        GetEnhMetaFileBits(emf, Some(&mut buf));
                        out.push((fmt, buf));
                    }
                    continue;
                }
                if is_gdi(fmt) {
                    continue;
                }
                if let Some(b) = bytes(fmt, 512 << 20) {
                    out.push((fmt, b));
                }
            }
            let _ = CloseClipboard();
            Some(Saved(out))
        }
    }

    pub fn restore(hwnd: HWND, s: &Saved) -> bool {
        unsafe {
            if OpenClipboard(Some(hwnd)).is_err() {
                return false;
            }
            let _ = EmptyClipboard();
            let mut ok = true;
            for (fmt, data) in &s.0 {
                if *fmt == CF_ENHMETAFILE {
                    let h = SetEnhMetaFileBits(data);
                    ok &= SetClipboardData(*fmt, Some(HANDLE(h.0))).is_ok();
                } else {
                    ok &= set(*fmt, data);
                }
            }
            let _ = CloseClipboard();
            ok
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        for s in ["https://example.com", " http://a.b/c?d=e ", "www.plume.app", "mailto:me@example.com", "ftp://host/file", "file:///C:/x.txt"] {
            assert!(is_link(s), "{s}");
            assert_eq!(kind_of_text(s), Kind::Link);
        }
        for s in ["", "hello", "https://", "see https://example.com", "www.", "www.nodot", "www.trailing.", "example.com"] {
            assert!(!is_link(s), "{s}");
        }
        assert_eq!(kind_of_text("two words"), Kind::Text);
    }

    #[test]
    fn private_formats() {
        assert!(!is_private(false, None, None, false));
        assert!(is_private(true, None, None, false));
        assert!(is_private(false, Some(0), None, false));
        assert!(!is_private(false, Some(1), Some(1), false));
        assert!(is_private(false, Some(1), Some(0), false));
        assert!(is_private(false, None, None, true));
    }

    #[test]
    fn dropfiles_round_trip() {
        let files = vec![r"C:\Users\me\a.txt".to_string(), r"D:\photos\été 2026.jpg".to_string()];
        let b = make_dropfiles(&files);
        assert_eq!(parse_dropfiles(&b), files);
        // ANSI list, as old programs give it
        let mut a = Vec::new();
        a.extend_from_slice(&20u32.to_le_bytes());
        a.extend_from_slice(&[0u8; 16]);
        a.extend_from_slice(b"C:\\x.txt\0C:\\y.txt\0\0");
        assert_eq!(parse_dropfiles(&a), vec![r"C:\x.txt", r"C:\y.txt"]);
        assert!(parse_dropfiles(b"short").is_empty());
    }

    #[test]
    fn texts() {
        let b = wide_z("héllo 👋");
        assert_eq!(utf16_text(&b), "héllo 👋");
        assert_eq!(utf8_text(b"Version:0.9\0junk").as_deref(), Some("Version:0.9"));
        assert!(utf8_text(&[0xff, 0xfe]).is_none());
    }

    #[test]
    fn registered_formats() {
        let f = formats();
        assert!(f.exclude >= 0xC000 && f.png >= 0xC000 && f.html != f.rtf);
    }
}
