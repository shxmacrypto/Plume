//! FFmpeg and ffprobe live in %LOCALAPPDATA%\Plume\tools, shared with Plume Web and Plume Music. They are
//! downloaded the first time a conversion needs them (the only thing Plume Convert fetches from the internet), and
//! what is already there is reused, never replaced.
//! The ZIP reader and the download are small copies of Plume Music's (`zip.rs`, `net.rs`), with a progress report
//! and room for FFmpeg's 100 MB archive.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use windows::{
    Win32::Networking::WinInet::*,
    core::{HSTRING, PCWSTR},
};

const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) PlumeConvert";
/// Largest download accepted.
const MAX: usize = 400 * 1024 * 1024;

pub fn tools_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_TOOLS_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(r"Plume\tools")
}
pub fn ffmpeg() -> PathBuf {
    tools_dir().join("ffmpeg.exe")
}
pub fn ffprobe() -> PathBuf {
    tools_dir().join("ffprobe.exe")
}
pub fn ready() -> bool {
    ffmpeg().is_file() && ffprobe().is_file()
}

/// Downloads FFmpeg's archive and takes out what is missing (ffmpeg.exe, ffprobe.exe). Blocking: on a thread.
/// `progress`: 0..1 while downloading.
pub fn install(progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    if ready() {
        return Ok(());
    }
    let dir = tools_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let data = download(FFMPEG_URL, progress)?;
    let missing: Vec<&str> = ["ffmpeg.exe", "ffprobe.exe"].into_iter().filter(|n| !dir.join(n).is_file()).collect();
    extract_only(&data, &dir, &missing)?;
    if !ready() {
        return Err("ffmpeg.exe not found in the archive".into());
    }
    Ok(())
}

// ───────────────────────── download ─────────────────────────

struct H(*mut core::ffi::c_void);
impl Drop for H {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = InternetCloseHandle(self.0);
            }
        }
    }
}

/// GET through WinINet (Windows' proxy settings, no cookies, no cache), following redirects.
fn download(url: &str, progress: &mut dyn FnMut(f32)) -> Result<Vec<u8>, String> {
    unsafe {
        let net = H(InternetOpenW(&HSTRING::from(UA), INTERNET_OPEN_TYPE_PRECONFIG.0, PCWSTR::null(), PCWSTR::null(), 0));
        if net.0.is_null() {
            return Err("network unavailable".into());
        }
        for opt in [INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_SEND_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT] {
            let ms = 30_000u32;
            let _ = InternetSetOptionW(Some(net.0 as *const _), opt, Some(&ms as *const u32 as *const _), 4);
        }
        let flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_UI | INTERNET_FLAG_NO_COOKIES;
        let req = H(InternetOpenUrlW(net.0, &HSTRING::from(url), None, flags, None));
        if req.0.is_null() {
            return Err("cannot reach the download server".into());
        }
        let (mut status, mut len) = (0u32, 4u32);
        let _ = HttpQueryInfoW(req.0, HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER, Some(&mut status as *mut u32 as _), &mut len, None);
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        let (mut total, mut len) = (0u32, 4u32);
        let _ = HttpQueryInfoW(req.0, HTTP_QUERY_CONTENT_LENGTH | HTTP_QUERY_FLAG_NUMBER, Some(&mut total as *mut u32 as _), &mut len, None);
        let (mut out, mut buf) = (Vec::with_capacity(total as usize), vec![0u8; 256 * 1024]);
        loop {
            let mut n = 0u32;
            InternetReadFile(req.0, buf.as_mut_ptr() as _, buf.len() as u32, &mut n).map_err(|e| e.message().to_string())?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
            if out.len() > MAX {
                return Err("download too large".into());
            }
            if total > 0 {
                progress((out.len() as f32 / total as f32).min(1.0));
            }
        }
        if total > 0 && out.len() < total as usize {
            return Err("download cut short".into());
        }
        Ok(out)
    }
}

// ───────────────────────── ZIP ─────────────────────────

fn u16le(d: &[u8], i: usize) -> usize {
    d.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).unwrap_or(0)
}
fn u32le(d: &[u8], i: usize) -> usize {
    d.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).unwrap_or(0)
}

/// Extracts only the named files, flat, into `dir` (stored or deflate; "../" paths are skipped). A file already
/// there is left alone; a new one is written beside and renamed when complete.
pub fn extract_only(z: &[u8], dir: &Path, names: &[&str]) -> Result<(), String> {
    let eocd = (0..z.len().saturating_sub(21)).rev().find(|&i| z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06])).ok_or("zip: EOCD")?;
    let (count, mut p) = (u16le(z, eocd + 10), u32le(z, eocd + 16));
    for _ in 0..count {
        if p + 46 > z.len() || !z[p..].starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err("zip: central dir".into());
        }
        let (method, csize) = (u16le(z, p + 10), u32le(z, p + 20));
        let (nlen, elen, clen, local) = (u16le(z, p + 28), u16le(z, p + 30), u16le(z, p + 32), u32le(z, p + 42));
        let name = String::from_utf8_lossy(z.get(p + 46..p + 46 + nlen).ok_or("zip: name")?).replace('\\', "/");
        p += 46 + nlen + elen + clen;
        let rel = Path::new(&name);
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) || name.ends_with('/') {
            continue;
        }
        let Some(f) = rel.file_name().and_then(|f| f.to_str()).filter(|f| names.contains(f)) else { continue };
        let target = dir.join(f);
        if target.exists() {
            continue;
        }
        let data_at = local + 30 + u16le(z, local + 26) + u16le(z, local + 28);
        let raw = z.get(data_at..data_at + csize).ok_or("zip: data")?;
        let bytes = match method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(raw).map_err(|_| "zip: inflate")?,
            m => return Err(format!("zip: method {m}")),
        };
        let part = dir.join(format!("{f}.part"));
        fs::write(&part, bytes).map_err(|e| e.to_string())?;
        fs::rename(&part, &target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ZIP with one stored file, written by hand.
    fn zip_with(name: &str, data: &[u8]) -> Vec<u8> {
        let mut z = Vec::new();
        let n = name.as_bytes();
        // local header
        z.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(n.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(n);
        z.extend_from_slice(data);
        let cd = z.len();
        z.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(n.len() as u16).to_le_bytes());
        z.extend_from_slice(&[0; 12]);
        z.extend_from_slice(&0u32.to_le_bytes());
        z.extend_from_slice(n);
        let cd_len = z.len() - cd;
        z.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend_from_slice(&(cd_len as u32).to_le_bytes());
        z.extend_from_slice(&(cd as u32).to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        z
    }

    #[test]
    fn extracts_and_keeps_existing() {
        let d = std::env::temp_dir().join(format!("plume-convert-zip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let z = zip_with("ffmpeg-7/bin/ffmpeg.exe", b"MZ new");
        extract_only(&z, &d, &["ffmpeg.exe"]).unwrap();
        assert_eq!(fs::read(d.join("ffmpeg.exe")).unwrap(), b"MZ new");
        // never replaced
        fs::write(d.join("ffmpeg.exe"), b"MZ mine").unwrap();
        extract_only(&z, &d, &["ffmpeg.exe"]).unwrap();
        assert_eq!(fs::read(d.join("ffmpeg.exe")).unwrap(), b"MZ mine");
        // a "../" path is skipped
        let evil = zip_with("../evil.exe", b"x");
        extract_only(&evil, &d, &["evil.exe"]).unwrap();
        assert!(!d.parent().unwrap().join("evil.exe").exists() && !d.join("evil.exe").exists());
        assert!(extract_only(b"nope", &d, &["a"]).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
