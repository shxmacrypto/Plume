// Mises à jour intégrées : Plume lit sa dernière version publiée sur GitHub, télécharge PlumeSetup.exe,
// vérifie son empreinte SHA-256 (publiée par GitHub) puis le lance avec --update. L'installeur remplace
// les fichiers (même pendant que Plume tourne) ; un redémarrage de Plume termine la mise à jour.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use windows::{
    Win32::Networking::WinInet::{
        HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE, HttpQueryInfoW, INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_NO_COOKIES,
        INTERNET_FLAG_NO_UI, INTERNET_FLAG_RELOAD, INTERNET_OPEN_TYPE_PRECONFIG, InternetCloseHandle, InternetOpenUrlW, InternetOpenW,
        InternetReadFile,
    },
    core::{HSTRING, PCWSTR},
};

pub const ASSET: &str = "PlumeSetup.exe";
const MAX: usize = 64 << 20;

pub struct Asset {
    pub url: String,
    pub size: u64,
    sha_url: String,
    digest: String,
}

pub struct Release {
    pub tag: String,
    pub page: String,
    pub asset: Option<Asset>,
}

fn local(u: &str) -> bool {
    u.starts_with("http://127.0.0.1:") || u.starts_with("http://localhost:")
}

/// Adresse de la dernière version. PLUME_UPDATE_FEED permet de tester avec un serveur local (uniquement).
fn feed() -> String {
    match std::env::var("PLUME_UPDATE_FEED") {
        Ok(u) if local(&u) => u,
        _ => format!("https://api.github.com/repos/{}/releases/latest", crate::REPO),
    }
}

/// Les fichiers ne peuvent venir que des versions publiées de ce dépôt (ou du serveur de test local).
fn trusted(url: &str, feed: &str) -> bool {
    if local(feed) {
        let origin: String = feed.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
        return url.starts_with(&format!("{origin}/"));
    }
    url.to_lowercase().starts_with(&format!("https://github.com/{}/releases/download/", crate::REPO.to_lowercase()))
}

fn ver(v: &str) -> (u32, u32, u32) {
    let mut n = v.trim_start_matches(['v', 'V']).split(['.', '-', '+']).map(|x| x.parse().unwrap_or(0));
    (n.next().unwrap_or(0), n.next().unwrap_or(0), n.next().unwrap_or(0))
}

pub fn newer(tag: &str, current: &str) -> bool {
    ver(tag) > ver(current)
}

pub fn latest() -> Result<Release, String> {
    let feed = feed();
    let j: Value = serde_json::from_slice(&get(&feed, &mut |_| {})?).map_err(|e| e.to_string())?;
    let tag = j["tag_name"].as_str().unwrap_or("").to_string();
    if tag.is_empty() {
        return Err("no release".into());
    }
    let assets = j["assets"].as_array().cloned().unwrap_or_default();
    let named = |n: &str| assets.iter().find(|a| a["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(n))).cloned();
    let asset = named(ASSET).and_then(|a| {
        let url = a["browser_download_url"].as_str()?.to_string();
        let sha_url = named(&format!("{ASSET}.sha256")).and_then(|s| s["browser_download_url"].as_str().map(String::from)).unwrap_or_default();
        trusted(&url, &feed).then(|| Asset {
            url,
            size: a["size"].as_u64().unwrap_or(0),
            digest: a["digest"].as_str().unwrap_or("").to_string(),
            sha_url: if trusted(&sha_url, &feed) { sha_url } else { String::new() },
        })
    });
    Ok(Release { tag, page: j["html_url"].as_str().unwrap_or("").to_string(), asset })
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Empreinte attendue : celle que GitHub calcule pour chaque fichier publié, sinon PlumeSetup.exe.sha256.
fn expected_sha(a: &Asset) -> Result<String, String> {
    if let Some(d) = a.digest.strip_prefix("sha256:").filter(|d| is_hex64(d)) {
        return Ok(d.to_lowercase());
    }
    if !a.sha_url.is_empty() {
        let t = String::from_utf8_lossy(&get(&a.sha_url, &mut |_| {})?).to_string();
        if let Some(h) = t.split_whitespace().next().filter(|h| is_hex64(h)) {
            return Ok(h.to_lowercase());
        }
    }
    Err("no SHA-256 published for the installer".into())
}

/// Télécharge l'installeur dans `dir` et vérifie son empreinte. `progress` reçoit 0..=100.
pub fn download(a: &Asset, tag: &str, dir: &Path, progress: &mut dyn FnMut(u32)) -> Result<PathBuf, String> {
    let want = expected_sha(a)?;
    let mut last = u32::MAX;
    let size = a.size.max(1);
    let body = get(&a.url, &mut |n| {
        let p = ((n as u64 * 100) / size).min(100) as u32;
        if p != last {
            last = p;
            progress(p);
        }
    })?;
    let got: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
    if got != want {
        return Err("the download is damaged (SHA-256 mismatch)".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("PlumeSetup-{}.exe", tag.trim_start_matches('v')));
    std::fs::write(&file, &body).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Lance l'installeur en mode mise à jour et attend la fin (quelques dixièmes de seconde).
pub fn install(setup: &Path) -> Result<(), String> {
    let st = std::process::Command::new(setup).arg("--update").status().map_err(|e| e.to_string())?;
    if st.success() { Ok(()) } else { Err(format!("the installer failed (code {})", st.code().unwrap_or(-1))) }
}

/// GET via WinINet (réglages proxy de Windows, suit les redirections de GitHub, pas de cache).
fn get(url: &str, progress: &mut dyn FnMut(usize)) -> Result<Vec<u8>, String> {
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
    unsafe {
        let net = H(InternetOpenW(&HSTRING::from(format!("Plume/{}", crate::VERSION)), INTERNET_OPEN_TYPE_PRECONFIG.0, PCWSTR::null(), PCWSTR::null(), 0));
        if net.0.is_null() {
            return Err("network unavailable".into());
        }
        let headers: Vec<u16> = "Accept: application/vnd.github+json\r\n".encode_utf16().collect();
        let flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_UI | INTERNET_FLAG_NO_COOKIES;
        let req = H(InternetOpenUrlW(net.0, &HSTRING::from(url), Some(&headers), flags, None));
        if req.0.is_null() {
            return Err("could not reach GitHub".into());
        }
        let (mut code, mut len) = (0u32, 4u32);
        let _ = HttpQueryInfoW(req.0, HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER, Some(&mut code as *mut u32 as _), &mut len, None);
        if code != 200 {
            return Err(format!("HTTP {code}"));
        }
        let (mut out, mut buf) = (Vec::new(), vec![0u8; 64 * 1024]);
        loop {
            let mut n = 0u32;
            InternetReadFile(req.0, buf.as_mut_ptr() as _, buf.len() as u32, &mut n).map_err(|e| e.message())?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
            if out.len() > MAX {
                return Err("download too large".into());
            }
            progress(out.len());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::{Read, Write}, net::TcpListener};

    #[test]
    fn versions() {
        assert!(newer("v0.6.0", "0.5.0"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(newer("0.5.10", "0.5.9"));
        assert!(!newer("v0.5.0", "0.5.0"));
        assert!(!newer("v0.4.9", "0.5.0"));
    }

    #[test]
    fn only_this_repo() {
        let gh = format!("https://api.github.com/repos/{}/releases/latest", crate::REPO);
        assert!(trusted(&format!("https://github.com/{}/releases/download/v1.0.0/PlumeSetup.exe", crate::REPO), &gh));
        assert!(trusted(&format!("https://github.com/{}/releases/download/v1.0.0/PlumeSetup.exe", crate::REPO.to_uppercase()), &gh));
        assert!(!trusted("https://github.com/someone/else/releases/download/v1.0.0/PlumeSetup.exe", &gh));
        assert!(!trusted("https://evil.example/PlumeSetup.exe", &gh));
        assert!(trusted("http://127.0.0.1:4810/PlumeSetup.exe", "http://127.0.0.1:4810/latest.json"));
        assert!(!trusted("http://127.0.0.1:9999/PlumeSetup.exe", "http://127.0.0.1:4810/latest.json"));
    }

    /// Flux local : release JSON + « installeur » ; vérifie le choix du fichier, la progression et l'empreinte.
    #[test]
    fn feed_download_and_checksum() {
        let srv = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", srv.local_addr().unwrap().port());
        let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let sha: String = Sha256::digest(&payload).iter().map(|b| format!("{b:02x}")).collect();
        let json = format!(
            r#"{{"tag_name":"v9.9.9","html_url":"{base}/rel","assets":[{{"name":"Plume.exe","browser_download_url":"{base}/old.exe","size":1}},{{"name":"PlumeSetup.exe","browser_download_url":"{base}/PlumeSetup.exe","size":{},"digest":"sha256:{sha}"}}]}}"#,
            payload.len()
        );
        let body = payload.clone();
        std::thread::spawn(move || {
            for s in srv.incoming().take(3).flatten() {
                let mut s = s;
                let mut req = [0u8; 2048];
                let n = s.read(&mut req).unwrap_or(0);
                let path = String::from_utf8_lossy(&req[..n]).split_whitespace().nth(1).unwrap_or("").to_string();
                let data: &[u8] = if path == "/latest.json" { json.as_bytes() } else { &body };
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len());
                let _ = s.write_all(data);
            }
        });
        unsafe { std::env::set_var("PLUME_UPDATE_FEED", format!("{base}/latest.json")) };
        let r = latest().unwrap();
        assert_eq!(r.tag, "v9.9.9");
        let a = r.asset.expect("installer asset");
        assert!(a.url.ends_with("/PlumeSetup.exe"));
        let dir = std::env::temp_dir().join(format!("plume-upd-test-{}", std::process::id()));
        let mut seen = vec![];
        let file = download(&a, &r.tag, &dir, &mut |p| seen.push(p)).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), payload);
        assert_eq!(seen.last(), Some(&100));
        let bad = Asset { digest: format!("sha256:{}", "0".repeat(64)), ..a };
        assert!(download(&bad, &r.tag, &dir, &mut |_| {}).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
