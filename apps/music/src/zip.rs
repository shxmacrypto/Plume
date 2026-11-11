//! A minimal ZIP reader (stored / deflate), safe against "../" paths: enough to take ffmpeg.exe out of its archive.
//! The same as Plume Web's.

use std::{
    fs,
    path::{Component, Path},
};

fn u16le(d: &[u8], i: usize) -> usize {
    u16::from_le_bytes([d[i], d[i + 1]]) as usize
}
fn u32le(d: &[u8], i: usize) -> usize {
    u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as usize
}

/// Extracts only the named files, flat, into `dir`.
pub fn extract_only(z: &[u8], dir: &Path, names: &[&str]) -> Result<(), String> {
    let eocd = (0..z.len().saturating_sub(21)).rev().find(|&i| z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06])).ok_or("zip: EOCD")?;
    let (count, mut p) = (u16le(z, eocd + 10), u32le(z, eocd + 16));
    for _ in 0..count {
        if p + 46 > z.len() || !z[p..].starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err("zip: central dir".into());
        }
        let (method, csize) = (u16le(z, p + 10), u32le(z, p + 20));
        let (nlen, elen, clen, local) = (u16le(z, p + 28), u16le(z, p + 30), u16le(z, p + 32), u32le(z, p + 42));
        let name = String::from_utf8_lossy(&z[p + 46..p + 46 + nlen]).replace('\\', "/");
        p += 46 + nlen + elen + clen;
        let rel = Path::new(&name);
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) || name.ends_with('/') {
            continue;
        }
        let Some(f) = rel.file_name().and_then(|f| f.to_str()).filter(|f| names.contains(f)) else { continue };
        let data_at = local + 30 + u16le(z, local + 26) + u16le(z, local + 28);
        let raw = z.get(data_at..data_at + csize).ok_or("zip: data")?;
        let bytes = match method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(raw).map_err(|_| "zip: inflate")?,
            m => return Err(format!("zip: method {m}")),
        };
        fs::write(dir.join(f), bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}
