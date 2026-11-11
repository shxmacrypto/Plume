//! Vraies icônes des navigateurs : lues directement dans leur .exe installé (aucune image embarquée),
//! converties en petit fichier .ico (data: URI) pour l'interface.
use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};
use windows::Win32::{
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO, PrivateExtractIconsW},
};

/// Emplacements habituels de l'exécutable de chaque navigateur.
pub fn exe_for(browser: &str) -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from).unwrap_or_default();
    let (local, pf, pf86) = (env("LOCALAPPDATA"), env("ProgramFiles"), env("ProgramFiles(x86)"));
    let c: Vec<PathBuf> = match browser {
        "Google Chrome" => vec![
            pf.join(r"Google\Chrome\Application\chrome.exe"),
            pf86.join(r"Google\Chrome\Application\chrome.exe"),
            local.join(r"Google\Chrome\Application\chrome.exe"),
        ],
        "Microsoft Edge" => vec![pf86.join(r"Microsoft\Edge\Application\msedge.exe"), pf.join(r"Microsoft\Edge\Application\msedge.exe")],
        "Brave" => vec![pf.join(r"BraveSoftware\Brave-Browser\Application\brave.exe"), local.join(r"BraveSoftware\Brave-Browser\Application\brave.exe")],
        "Vivaldi" => vec![local.join(r"Vivaldi\Application\vivaldi.exe"), pf.join(r"Vivaldi\Application\vivaldi.exe")],
        "Chromium" => vec![local.join(r"Chromium\Application\chrome.exe")],
        "Opera" => vec![local.join(r"Programs\Opera\opera.exe"), local.join(r"Programs\Opera\launcher.exe"), pf.join(r"Opera\launcher.exe")],
        "Opera GX" => vec![local.join(r"Programs\Opera GX\opera.exe"), local.join(r"Programs\Opera GX\launcher.exe"), pf.join(r"Opera GX\launcher.exe")],
        "Firefox" => vec![pf.join(r"Mozilla Firefox\firefox.exe"), pf86.join(r"Mozilla Firefox\firefox.exe"), local.join(r"Mozilla Firefox\firefox.exe")],
        _ => vec![],
    };
    c.into_iter().find(|p| p.exists())
}

/// Icône principale d'un .exe en `size`×`size` px, sous forme de data URI (.ico 32 bits avec transparence).
pub fn data_uri(exe: &Path, size: i32) -> Option<String> {
    let w: Vec<u16> = exe.as_os_str().encode_wide().collect();
    if w.len() >= 260 {
        return None;
    }
    let mut name = [0u16; 260];
    name[..w.len()].copy_from_slice(&w);
    unsafe {
        let mut h = [HICON::default()];
        let n = PrivateExtractIconsW(&name, 0, size, size, Some(&mut h), None, 0);
        if n == 0 || n == u32::MAX || h[0].is_invalid() {
            return None;
        }
        let mut ii = ICONINFO::default();
        let ok = GetIconInfo(h[0], &mut ii).is_ok();
        let mut px = vec![0u8; (size * size * 4) as usize];
        if ok {
            let dc = CreateCompatibleDC(None);
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size,
                    biHeight: size, // de bas en haut, comme dans un .ico
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            GetDIBits(dc, ii.hbmColor, 0, size as u32, Some(px.as_mut_ptr() as _), &mut bi, DIB_RGB_COLORS);
            let _ = DeleteDC(dc);
            let _ = DeleteObject(ii.hbmColor.into());
            let _ = DeleteObject(ii.hbmMask.into());
        }
        let _ = DestroyIcon(h[0]);
        if !ok || px.iter().all(|&b| b == 0) {
            return None;
        }
        if px.chunks(4).all(|p| p[3] == 0) {
            px.chunks_mut(4).for_each(|p| p[3] = 255); // ancienne icône sans canal alpha
        }
        // Fichier .ico : en-tête + entrée + BITMAPINFOHEADER (hauteur ×2) + pixels + masque AND vide
        let mask = (((size + 31) / 32) * 4 * size) as usize;
        let img = 40 + px.len() + mask;
        let mut o = Vec::with_capacity(22 + img);
        o.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
        o.extend_from_slice(&[(size % 256) as u8, (size % 256) as u8, 0, 0, 1, 0, 32, 0]);
        o.extend_from_slice(&(img as u32).to_le_bytes());
        o.extend_from_slice(&22u32.to_le_bytes());
        o.extend_from_slice(&40u32.to_le_bytes());
        o.extend_from_slice(&size.to_le_bytes());
        o.extend_from_slice(&(size * 2).to_le_bytes());
        o.extend_from_slice(&[1, 0, 32, 0]);
        o.extend_from_slice(&[0u8; 24]);
        o.extend_from_slice(&px);
        o.resize(o.len() + mask, 0);
        Some(format!("data:image/x-icon;base64,{}", crate::ext::b64(&o)))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn edge_icon() {
        if let Some(exe) = super::exe_for("Microsoft Edge") {
            let uri = super::data_uri(&exe, 64).expect("icône Edge");
            assert!(uri.len() > 1000);
            println!("EDGE {} -> {} chars", exe.display(), uri.len());
        }
    }
}
