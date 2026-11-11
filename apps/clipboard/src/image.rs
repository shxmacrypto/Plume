//! Pictures, with Windows Imaging Component: what the clipboard gives (a PNG, or a DIB: a BMP without its file
//! header) becomes a PNG and a small PNG thumbnail; a saved PNG becomes a DIB again to be pasted, or a bitmap to be
//! drawn.

use std::cell::RefCell;
use windows::{
    Win32::{
        Graphics::Imaging::*,
        System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, IStream, STREAM_SEEK_END, STREAM_SEEK_SET},
        UI::Shell::SHCreateMemStream,
    },
    core::Interface,
};

/// Pixels, 4 bytes each (blue, green, red, alpha), rows from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct Bgra {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

/// Larger pictures are not kept (about 16 000 × 16 000).
const MAX_PIXELS: u64 = 256 * 1024 * 1024;
/// The thumbnail's longest side.
pub const THUMB: u32 = 320;

thread_local! {
    static WIC: RefCell<Option<IWICImagingFactory>> = const { RefCell::new(None) };
}

/// For a worker thread: COM, before anything here.
pub fn init_thread() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn wic() -> Option<IWICImagingFactory> {
    WIC.with(|w| {
        let mut w = w.borrow_mut();
        if w.is_none() {
            *w = unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }.ok();
        }
        w.clone()
    })
}

fn source_from(f: &IWICImagingFactory, bytes: &[u8]) -> Option<IWICBitmapFrameDecode> {
    unsafe {
        let stream = f.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let s: IStream = stream.cast().ok()?;
        let dec = f.CreateDecoderFromStream(&s, std::ptr::null(), WICDecodeMetadataCacheOnLoad).ok()?;
        dec.GetFrame(0).ok()
    }
}

fn pixels(f: &IWICImagingFactory, src: &IWICBitmapSource) -> Option<Bgra> {
    unsafe {
        let conv = f.CreateFormatConverter().ok()?;
        conv.Initialize(src, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        conv.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 || w as u64 * h as u64 > MAX_PIXELS {
            return None;
        }
        let mut px = vec![0u8; w as usize * h as usize * 4];
        conv.CopyPixels(std::ptr::null(), w * 4, &mut px).ok()?;
        Some(Bgra { w, h, px })
    }
}

/// Any picture file in memory (PNG, BMP, JPEG…) as pixels.
pub fn decode(bytes: &[u8]) -> Option<Bgra> {
    let f = wic()?;
    let frame = source_from(&f, bytes)?;
    let mut b = pixels(&f, &frame.cast().ok()?)?;
    fix_alpha(&mut b);
    Some(b)
}

/// Some apps put a DIB whose alpha is zero everywhere: shown as it was meant, opaque.
fn fix_alpha(b: &mut Bgra) {
    if b.px.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        for p in b.px.as_chunks_mut::<4>().0 {
            p[3] = 255;
        }
    }
}

/// A clipboard DIB (BITMAPINFO followed by the pixels) as a BMP file, which WIC reads in all its forms.
pub fn dib_to_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    let u32_at = |o: usize| dib.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = u32_at(0)? as usize;
    if !(12..=124).contains(&header) || dib.len() < header {
        return None;
    }
    let (bits, compression, used) = if header == 12 {
        (u16::from_le_bytes([dib[10], dib[11]]) as u32, 0, 0)
    } else {
        (u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]) as u32, u32_at(16)?, u32_at(32)?)
    };
    let masks = match (header, compression) {
        (40, 3) => 12, // BI_BITFIELDS after a plain header
        (40, 6) => 16, // BI_ALPHABITFIELDS
        _ => 0,
    };
    let entry = if header == 12 { 3 } else { 4 };
    let colors = if used > 0 {
        used as usize
    } else if bits <= 8 {
        1usize << bits
    } else {
        0
    };
    let offset = 14 + header + masks + colors * entry;
    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

pub fn decode_dib(dib: &[u8]) -> Option<Bgra> {
    decode(&dib_to_bmp(dib)?)
}

fn bitmap(f: &IWICImagingFactory, b: &Bgra) -> Option<IWICBitmap> {
    unsafe { f.CreateBitmapFromMemory(b.w, b.h, &GUID_WICPixelFormat32bppBGRA, b.w * 4, &b.px).ok() }
}

fn encode_source(f: &IWICImagingFactory, src: &IWICBitmapSource, w: u32, h: u32) -> Option<Vec<u8>> {
    unsafe {
        let stream: IStream = SHCreateMemStream(None)?;
        let enc = f.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()).ok()?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
        let mut frame: Option<IWICBitmapFrameEncode> = None;
        enc.CreateNewFrame(&mut frame, std::ptr::null_mut()).ok()?;
        let frame = frame?;
        frame.Initialize(None).ok()?;
        frame.SetSize(w, h).ok()?;
        let mut fmt = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut fmt).ok()?;
        frame.WriteSource(src, std::ptr::null()).ok()?;
        frame.Commit().ok()?;
        enc.Commit().ok()?;
        let mut size = 0u64;
        stream.Seek(0, STREAM_SEEK_END, Some(&mut size)).ok()?;
        stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
        let mut out = vec![0u8; size as usize];
        let mut read = 0u32;
        stream.Read(out.as_mut_ptr() as _, size as u32, Some(&mut read)).ok().ok()?;
        out.truncate(read as usize);
        Some(out)
    }
}

pub fn encode_png(b: &Bgra) -> Option<Vec<u8>> {
    let f = wic()?;
    let bmp = bitmap(&f, b)?;
    encode_source(&f, &bmp.cast().ok()?, b.w, b.h)
}

/// The size of a thumbnail whose longest side is at most `max`.
pub fn fit(w: u32, h: u32, max: u32) -> (u32, u32) {
    if w <= max && h <= max {
        return (w.max(1), h.max(1));
    }
    if w >= h { (max, ((h as u64 * max as u64) / w as u64).max(1) as u32) } else { (((w as u64 * max as u64) / h as u64).max(1) as u32, max) }
}

pub fn thumb_png(b: &Bgra) -> Option<Vec<u8>> {
    let f = wic()?;
    let (tw, th) = fit(b.w, b.h, THUMB);
    let bmp = bitmap(&f, b)?;
    unsafe {
        let scaler = f.CreateBitmapScaler().ok()?;
        scaler.Initialize(&bmp, tw, th, WICBitmapInterpolationModeFant).ok()?;
        encode_source(&f, &scaler.cast().ok()?, tw, th)
    }
}

/// Pixels as a clipboard DIB: a BITMAPINFOHEADER, 32 bits, rows from the bottom.
pub fn to_dib(b: &Bgra) -> Vec<u8> {
    let row = b.w as usize * 4;
    let mut out = Vec::with_capacity(40 + b.px.len());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(b.w as i32).to_le_bytes());
    out.extend_from_slice(&(b.h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(b.px.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // resolution, colours
    for y in (0..b.h as usize).rev() {
        out.extend_from_slice(&b.px[y * row..(y + 1) * row]);
    }
    out
}

/// A saved PNG as a bitmap Direct2D can draw (premultiplied alpha, decoded once).
pub fn drawable(png: &[u8]) -> Option<IWICBitmapSource> {
    let f = wic()?;
    let frame = source_from(&f, png)?;
    unsafe {
        let conv = f.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let bmp = f.CreateBitmapFromSource(&conv, WICBitmapCacheOnLoad).ok()?;
        bmp.cast().ok()
    }
}

/// A picture made up for tests and the test socket: a soft diagonal gradient with a white disc.
#[cfg_attr(not(any(test, feature = "test-hook")), allow(dead_code))]
pub fn sample(w: u32, h: u32, hue: u8) -> Bgra {
    let mut px = Vec::with_capacity(w as usize * h as usize * 4);
    let (cx, cy, r) = (w as f32 * 0.62, h as f32 * 0.42, w.min(h) as f32 * 0.22);
    for y in 0..h {
        for x in 0..w {
            let t = (x + y) as f32 / (w + h) as f32;
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            if d < r {
                px.extend_from_slice(&[245, 245, 250, 255]);
            } else {
                let b = (120.0 + 120.0 * t) as u8;
                let g = (40.0 + hue as f32 * 0.5 * (1.0 - t)) as u8;
                let rr = (200.0 - 80.0 * t) as u8;
                px.extend_from_slice(&[b, g, rr, 255]);
            }
        }
    }
    Bgra { w, h, px }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip_and_thumb() {
        init_thread();
        let b = sample(640, 360, 80);
        let png = encode_png(&b).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let back = decode(&png).unwrap();
        assert_eq!((back.w, back.h), (640, 360));
        assert_eq!(back.px, b.px);
        let t = decode(&thumb_png(&b).unwrap()).unwrap();
        assert_eq!((t.w, t.h), (320, 180));
        assert!(drawable(&png).is_some());
    }

    #[test]
    fn dib_round_trip() {
        init_thread();
        let b = sample(33, 17, 10);
        let dib = to_dib(&b);
        assert_eq!(dib.len(), 40 + 33 * 17 * 4);
        let back = decode_dib(&dib).unwrap();
        assert_eq!((back.w, back.h), (33, 17));
        assert_eq!(back.px, b.px);
        // a DIB whose alpha is zero everywhere comes back opaque
        let mut clear = b.clone();
        clear.px.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 0);
        let mut v5 = to_dib(&clear);
        v5[16] = 3; // BI_BITFIELDS with masks after the header
        let masks: [u8; 12] = [0, 0, 0xff, 0, 0, 0xff, 0, 0, 0xff, 0, 0, 0];
        let mut with_masks = v5[..40].to_vec();
        with_masks.extend_from_slice(&masks);
        with_masks.extend_from_slice(&v5[40..]);
        let back = decode_dib(&with_masks).unwrap();
        assert!(back.px.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        assert!(decode_dib(b"junk").is_none());
    }

    #[test]
    fn thumb_sizes() {
        assert_eq!(fit(100, 50, 320), (100, 50));
        assert_eq!(fit(3200, 1600, 320), (320, 160));
        assert_eq!(fit(10, 4000, 320), (1, 320));
    }
}
