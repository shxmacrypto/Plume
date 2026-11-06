//! Extensions Chrome : téléchargement depuis le Chrome Web Store (CRX), décompression
//! (CRX → ZIP, extracteur maison avec miniz_oxide, pur Rust) et lecture du manifeste.
use serde_json::{Value, json};
use std::{fs, path::{Component, Path, PathBuf}};
use windows::{
    Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, Urlmon::URLDownloadToFileW},
    core::HSTRING,
};

/// Extensions suggérées (compatibles WebView2).
pub const RECOMMENDED: &[(&str, &str, &str)] = &[
    ("ddkjiahejlhfcafbddmgiahcphecmpfh", "uBlock Origin Lite", "adblock"),
    ("eimadpbcbfnmbkopoojfekhnkhdbieeh", "Dark Reader", "dark"),
    ("nngceckbapebfimnlniiiahkandclblb", "Bitwarden", "vault"),
    ("mnjggcdmjocbbbhaepdhchncahnbgone", "SponsorBlock", "skip"),
];

/// Extrait l'identifiant (32 lettres a-p) d'un lien du Chrome Web Store ou d'un ID brut.
pub fn store_id(link: &str) -> Option<String> {
    link.split(|c: char| !c.is_ascii_alphanumeric())
        .find(|s| s.len() == 32 && s.bytes().all(|b| (b'a'..=b'p').contains(&b)))
        .map(String::from)
}

/// Télécharge et décompresse une extension dans `root/<id>`. Renvoie le dossier.
pub fn fetch(id: &str, root: &Path) -> Result<PathBuf, String> {
    let url = format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&prodversion=140.0.0.0\
         &acceptformat=crx2,crx3&x=id%3D{id}%26installsource%3Dondemand%26uc"
    );
    let tmp = std::env::temp_dir().join(format!("plume-{id}.crx"));
    unsafe { let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); }
    unsafe { URLDownloadToFileW(None, &HSTRING::from(url), &HSTRING::from(tmp.as_path()), 0, None) }
        .map_err(|e| format!("download: {e}"))?;
    let data = fs::read(&tmp).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&tmp);
    let dir = root.join(id);
    let _ = fs::remove_dir_all(&dir);
    unzip(crx_zip(&data)?, &dir)?;
    if !dir.join("manifest.json").exists() {
        return Err("manifest.json missing".into());
    }
    Ok(dir)
}

fn u32le(d: &[u8], o: usize) -> usize {
    d.get(o..o + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
}
fn u16le(d: &[u8], o: usize) -> usize {
    d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes(b.try_into().unwrap()) as usize)
}

/// Partie ZIP d'un fichier CRX2/CRX3.
fn crx_zip(d: &[u8]) -> Result<&[u8], String> {
    if d.starts_with(b"PK") {
        return Ok(d);
    }
    if !d.starts_with(b"Cr24") {
        return Err("not a CRX (extension unavailable?)".into());
    }
    let start = match u32le(d, 4) {
        3 => 12 + u32le(d, 8),
        2 => 16 + u32le(d, 8) + u32le(d, 12),
        v => return Err(format!("CRX v{v}")),
    };
    d.get(start..).ok_or_else(|| "CRX tronqué".into())
}

/// Extrait seulement les fichiers nommés (à plat, dans `dir`).
pub fn unzip_only(z: &[u8], dir: &Path, names: &[&str]) -> Result<(), String> {
    unzip_filter(z, dir, Some(names))
}

fn unzip(z: &[u8], dir: &Path) -> Result<(), String> {
    unzip_filter(z, dir, None)
}

/// Extracteur ZIP minimal (stocké / deflate), protégé contre les chemins « ../ ».
fn unzip_filter(z: &[u8], dir: &Path, only: Option<&[&str]>) -> Result<(), String> {
    let eocd = (0..z.len().saturating_sub(21)).rev().find(|&i| z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06])).ok_or("zip: EOCD")?;
    let (count, mut p) = (u16le(z, eocd + 10), u32le(z, eocd + 16));
    for _ in 0..count {
        if !z[p..].starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err("zip: central dir".into());
        }
        let (method, csize) = (u16le(z, p + 10), u32le(z, p + 20));
        let (nlen, elen, clen, local) = (u16le(z, p + 28), u16le(z, p + 30), u16le(z, p + 32), u32le(z, p + 42));
        let name = String::from_utf8_lossy(&z[p + 46..p + 46 + nlen]).replace('\\', "/");
        p += 46 + nlen + elen + clen;
        let rel = Path::new(&name);
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            continue; // chemin dangereux ignoré
        }
        let out = match only {
            Some(names) => match rel.file_name().and_then(|f| f.to_str()).filter(|f| names.contains(f)) {
                Some(f) => dir.join(f),
                None => continue,
            },
            None => dir.join(rel),
        };
        if name.ends_with('/') {
            let _ = fs::create_dir_all(&out);
            continue;
        }
        let data_at = local + 30 + u16le(z, local + 26) + u16le(z, local + 28);
        let raw = z.get(data_at..data_at + csize).ok_or("zip: data")?;
        let bytes = match method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(raw).map_err(|_| "zip: inflate")?,
            m => return Err(format!("zip: method {m}")),
        };
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&out, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Infos utiles du manifeste : page popup, page d'options, icône (data: URI).
pub fn manifest_info(dir: &Path) -> Value {
    let m: Value = fs::read_to_string(dir.join("manifest.json"))
        .ok()
        .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default();
    let popup = m["action"]["default_popup"].as_str().or(m["browser_action"]["default_popup"].as_str()).unwrap_or("");
    let options = m["options_ui"]["page"].as_str().or(m["options_page"].as_str()).unwrap_or("");
    // plus grande icône ≤ 64 px
    let icon = m["icons"]
        .as_object()
        .and_then(|o| {
            o.iter()
                .filter_map(|(k, v)| Some((k.parse::<u32>().ok()?, v.as_str()?)))
                .filter(|(k, _)| *k <= 64)
                .max_by_key(|(k, _)| *k)
                .map(|(_, v)| v.trim_start_matches('/').to_string())
        })
        .and_then(|f| fs::read(dir.join(f)).ok())
        .map(|b| format!("data:image/png;base64,{}", b64(&b)))
        .unwrap_or_default();
    let side = m["side_panel"]["default_path"].as_str().unwrap_or("");
    json!({"popup": popup.trim_start_matches('/'), "options": options.trim_start_matches('/'), "side": side.trim_start_matches('/'), "icon": icon})
}

pub fn b64(d: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn store_id_parse() {
        assert_eq!(super::store_id("https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh?hl=fr").as_deref(), Some("ddkjiahejlhfcafbddmgiahcphecmpfh"));
        assert_eq!(super::store_id("https://example.com/"), None);
    }

    #[test]
    #[ignore] // réseau
    fn fetch_ubol() {
        let root = std::env::temp_dir().join("plume-ext-test");
        let dir = super::fetch("ddkjiahejlhfcafbddmgiahcphecmpfh", &root).unwrap();
        let info = super::manifest_info(&dir);
        println!("EXT {} popup={} icon={}", dir.display(), info["popup"], info["icon"].as_str().unwrap().len());
        let _ = std::fs::remove_dir_all(root);
    }
}
