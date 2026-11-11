//! A 2FA QR code from a picture file (a screenshot of a site's "set up an authenticator" page). Windows decodes the
//! picture (PNG, JPEG, BMP, GIF…); the QR code itself is read in Rust (rqrr). Nothing leaves the PC.

use std::path::Path;
use windows::{
    Win32::{
        Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_WICPixelFormat8bppGray, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
            WICDecodeMetadataCacheOnDemand,
        },
        System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
    },
    core::HSTRING,
};

/// Gray pixels of a picture file (one byte each, row after row), at most 4096 pixels on a side.
pub fn gray(path: &Path) -> Option<(usize, usize, Vec<u8>)> {
    unsafe {
        // the calling thread may already have COM in another mode: that is fine
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let decoder = wic
            .CreateDecoderFromFilename(&HSTRING::from(path.as_os_str()), None, windows::Win32::Foundation::GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat8bppGray, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        conv.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return None;
        }
        let mut px = vec![0u8; (w * h) as usize];
        conv.CopyPixels(std::ptr::null(), w, &mut px).ok()?;
        Some((w as usize, h as usize, px))
    }
}

/// The texts of the QR codes in these gray pixels.
pub fn read_gray(w: usize, h: usize, px: &[u8]) -> Vec<String> {
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| px[y * w + x]);
    img.detect_grids().into_iter().filter_map(|g| g.decode().ok().map(|(_, text)| text)).collect()
}

/// The first otpauth:// address found in a picture file.
pub fn otpauth_from_file(path: &Path) -> Result<String, &'static str> {
    let (w, h, px) = gray(path).ok_or("unreadable picture")?;
    let texts = read_gray(w, h, &px);
    if texts.is_empty() {
        return Err("no QR code");
    }
    texts.into_iter().find(|t| t.to_ascii_lowercase().starts_with("otpauth://totp/")).ok_or("not a 2FA code")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A QR code drawn as gray pixels: 6 pixels per module, with a quiet zone.
    fn render(text: &str) -> (usize, usize, Vec<u8>) {
        let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
        let n = code.width();
        let (k, q) = (6, 4);
        let side = (n + 2 * q) * k;
        let mut px = vec![255u8; side * side];
        for y in 0..n {
            for x in 0..n {
                if code[(x, y)] == qrcode::Color::Dark {
                    for dy in 0..k {
                        for dx in 0..k {
                            px[((y + q) * k + dy) * side + (x + q) * k + dx] = 0;
                        }
                    }
                }
            }
        }
        (side, side, px)
    }

    /// The same pixels as a 24-bit BMP file (simple enough to write by hand).
    fn bmp(w: usize, h: usize, px: &[u8]) -> Vec<u8> {
        let row = (w * 3).div_ceil(4) * 4;
        let size = 54 + row * h;
        let mut b = Vec::with_capacity(size);
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&(size as u32).to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&54u32.to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(w as i32).to_le_bytes());
        b.extend_from_slice(&(h as i32).to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&[0; 24]);
        for y in (0..h).rev() {
            for x in 0..w {
                let v = px[y * w + x];
                b.extend_from_slice(&[v, v, v]);
            }
            b.extend(std::iter::repeat_n(0, row - w * 3));
        }
        b
    }

    #[test]
    fn reads_a_qr_code_from_a_picture_file() {
        let uri = "otpauth://totp/Test:tester?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Test";
        let (w, h, px) = render(uri);
        assert_eq!(read_gray(w, h, &px), vec![uri.to_string()]);
        let dir = std::env::temp_dir().join(format!("plume-passwords-qr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("code.bmp");
        std::fs::write(&f, bmp(w, h, &px)).unwrap();
        assert_eq!(otpauth_from_file(&f).unwrap(), uri);
        // a QR code that is not a 2FA one
        let (w, h, px) = render("https://example.com");
        std::fs::write(&f, bmp(w, h, &px)).unwrap();
        assert_eq!(otpauth_from_file(&f), Err("not a 2FA code"));
        std::fs::write(&f, b"not a picture").unwrap();
        assert_eq!(otpauth_from_file(&f), Err("unreadable picture"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
