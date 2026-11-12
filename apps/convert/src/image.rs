//! Pictures through WIC (Windows Imaging Component): it reads whatever Windows can read (JPEG, PNG, GIF, BMP,
//! TIFF, ICO, WebP, HEIC when its codec is installed…), turns a photo the way the camera says, resizes, and writes
//! PNG, JPEG (with a quality), BMP, TIFF and ICO. WebP is written by FFmpeg from a PNG made here.

use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::GENERIC_READ,
        Graphics::Imaging::*,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, CoCreateInstance,
                StructuredStorage::{IPropertyBag2, PROPBAG2, PROPVARIANT, PropVariantClear},
            },
            Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_R4, VT_UI2},
        },
    },
    core::{GUID, HSTRING, PWSTR, w},
};

/// Straight (not premultiplied) BGRA, 4 bytes a pixel, rows packed.
#[derive(Clone, Debug, PartialEq)]
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fmt {
    Png,
    Jpeg,
    Bmp,
    Tiff,
    Ico,
    Webp,
}

impl Fmt {
    pub fn from_name(s: &str) -> Fmt {
        match s {
            "jpeg" | "jpg" => Fmt::Jpeg,
            "png" => Fmt::Png,
            "bmp" => Fmt::Bmp,
            "tiff" | "tif" => Fmt::Tiff,
            "ico" => Fmt::Ico,
            _ => Fmt::Webp,
        }
    }
    fn container(self) -> GUID {
        match self {
            Fmt::Jpeg => GUID_ContainerFormatJpeg,
            Fmt::Bmp => GUID_ContainerFormatBmp,
            Fmt::Tiff => GUID_ContainerFormatTiff,
            _ => GUID_ContainerFormatPng,
        }
    }
}

fn err(e: windows::core::Error) -> String {
    e.message().to_string()
}

pub fn factory() -> Result<IWICImagingFactory, String> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(err) }
}

/// EXIF orientation (1..8) → the turn that shows the photo upright.
pub fn orientation_transform(o: u16) -> WICBitmapTransformOptions {
    WICBitmapTransformOptions(match o {
        2 => WICBitmapTransformFlipHorizontal.0,
        3 => WICBitmapTransformRotate180.0,
        4 => WICBitmapTransformFlipVertical.0,
        5 => WICBitmapTransformRotate90.0 | WICBitmapTransformFlipHorizontal.0,
        6 => WICBitmapTransformRotate90.0,
        7 => WICBitmapTransformRotate270.0 | WICBitmapTransformFlipHorizontal.0,
        8 => WICBitmapTransformRotate270.0,
        _ => 0,
    })
}

fn orientation(frame: &IWICBitmapFrameDecode) -> u16 {
    unsafe {
        let Ok(reader) = frame.GetMetadataQueryReader() else { return 1 };
        let mut v = PROPVARIANT::default();
        if reader.GetMetadataByName(w!("/app1/ifd/{ushort=274}"), &mut v).is_err() {
            return 1;
        }
        let inner = &v.Anonymous.Anonymous;
        let o = if inner.vt == VT_UI2 { inner.Anonymous.uiVal } else { 1 };
        let _ = PropVariantClear(&mut v);
        o
    }
}

fn copy(src: &IWICBitmapSource, w: u32, h: u32) -> Result<Pixels, String> {
    let mut data = vec![0u8; (w * h * 4) as usize];
    unsafe { src.CopyPixels(std::ptr::null(), w * 4, &mut data).map_err(err)? };
    Ok(Pixels { w, h, data })
}

fn to_bgra(wic: &IWICImagingFactory, src: &IWICBitmapSource) -> Result<IWICBitmapSource, String> {
    unsafe {
        let conv = wic.CreateFormatConverter().map_err(err)?;
        conv.Initialize(src, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).map_err(err)?;
        Ok(conv.into())
    }
}

/// A picture file, upright, as BGRA.
pub fn load(wic: &IWICImagingFactory, path: &Path) -> Result<Pixels, String> {
    unsafe {
        let dec = wic
            .CreateDecoderFromFilename(&HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .map_err(|_| "Windows cannot read this picture (its codec may be missing)".to_string())?;
        let frame = dec.GetFrame(0).map_err(err)?;
        let turn = orientation_transform(orientation(&frame));
        let src: IWICBitmapSource = if turn.0 != 0 {
            let rot = wic.CreateBitmapFlipRotator().map_err(err)?;
            rot.Initialize(&frame, turn).map_err(err)?;
            rot.into()
        } else {
            frame.into()
        };
        let bgra = to_bgra(wic, &src)?;
        let (mut w, mut h) = (0u32, 0u32);
        bgra.GetSize(&mut w, &mut h).map_err(err)?;
        if w == 0 || h == 0 {
            return Err("empty picture".into());
        }
        copy(&bgra, w, h)
    }
}

fn bitmap(wic: &IWICImagingFactory, p: &Pixels) -> Result<IWICBitmap, String> {
    unsafe { wic.CreateBitmapFromMemory(p.w, p.h, &GUID_WICPixelFormat32bppBGRA, p.w * 4, &p.data).map_err(err) }
}

/// Resized to w × h (high quality).
pub fn resize(wic: &IWICImagingFactory, p: &Pixels, w: u32, h: u32) -> Result<Pixels, String> {
    if (w, h) == (p.w, p.h) {
        return Ok(p.clone());
    }
    unsafe {
        let b = bitmap(wic, p)?;
        let s = wic.CreateBitmapScaler().map_err(err)?;
        s.Initialize(&b, w, h, WICBitmapInterpolationModeHighQualityCubic).map_err(err)?;
        copy(&s.into(), w, h)
    }
}

/// On a white background, 3 bytes a pixel (rows padded to 4 bytes): for JPEG and BMP, which have no transparency.
pub fn flatten_white(p: &Pixels) -> (Vec<u8>, u32) {
    let stride = (p.w * 3).div_ceil(4) * 4;
    let mut out = vec![0u8; (stride * p.h) as usize];
    for y in 0..p.h as usize {
        for x in 0..p.w as usize {
            let i = (y * p.w as usize + x) * 4;
            let a = p.data[i + 3] as u32;
            let o = y * stride as usize + x * 3;
            for c in 0..3 {
                out[o + c] = ((p.data[i + c] as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
            }
        }
    }
    (out, stride)
}

/// Centered in a transparent square of side `s` (the picture must already fit).
pub fn pad_square(p: &Pixels, s: u32) -> Pixels {
    pad_to(p, s, s, false)
}

/// The picture centred on a `w`×`h` canvas: transparent around it, or (`black`) laid on opaque black.
pub fn pad_to(p: &Pixels, w: u32, h: u32, black: bool) -> Pixels {
    let mut data = vec![0u8; (w * h * 4) as usize];
    if black {
        for px in data.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
    }
    let (ox, oy) = ((w.saturating_sub(p.w)) / 2, (h.saturating_sub(p.h)) / 2);
    let n = p.w.min(w) as usize;
    for y in 0..p.h.min(h) {
        let src = (y * p.w * 4) as usize;
        let dst = (((y + oy) * w + ox) * 4) as usize;
        let (from, to) = (&p.data[src..src + n * 4], &mut data[dst..dst + n * 4]);
        if black {
            for (s, d) in from.as_chunks::<4>().0.iter().zip(to.as_chunks_mut::<4>().0) {
                let a = s[3] as u32;
                for k in 0..3 {
                    d[k] = (s[k] as u32 * a / 255) as u8;
                }
            }
        } else {
            to.copy_from_slice(from);
        }
    }
    Pixels { w, h, data }
}

/// Photos → `frame_0001.png`… in `dir`, all one size, for a GIF. Each is turned upright and fit inside `long`
/// pixels, then centred on the size of the largest one. Several photos are laid on black, so nothing of a photo
/// shows behind the next; a single one keeps its transparency. `step(i)` after each photo; false stops.
pub fn gif_frames(srcs: &[std::path::PathBuf], long: u32, dir: &Path, step: &mut dyn FnMut(usize) -> bool) -> Result<(u32, u32), String> {
    let wic = factory()?;
    let mut small = Vec::new();
    for (i, src) in srcs.iter().enumerate() {
        let p = load(&wic, src).map_err(|e| format!("{}: {e}", src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))?;
        let (w, h) = crate::ffmpeg::fit_long(p.w, p.h, long);
        small.push(resize(&wic, &p, w, h)?);
        if !step(i) {
            return Err(crate::queue::CANCELLED.into());
        }
    }
    let w = small.iter().map(|p| p.w).max().unwrap_or(1);
    let h = small.iter().map(|p| p.h).max().unwrap_or(1);
    let black = small.len() > 1;
    for (i, p) in small.iter().enumerate() {
        encode(&wic, &pad_to(p, w, h, black), Fmt::Png, 100, &dir.join(format!("frame_{:04}.png", i + 1)))?;
    }
    Ok((w, h))
}

/// Writes `p` as PNG, JPEG, BMP or TIFF.
pub fn encode(wic: &IWICImagingFactory, p: &Pixels, fmt: Fmt, quality: u32, path: &Path) -> Result<(), String> {
    unsafe {
        let source: IWICBitmap = if matches!(fmt, Fmt::Jpeg | Fmt::Bmp) {
            let (rgb, stride) = flatten_white(p);
            wic.CreateBitmapFromMemory(p.w, p.h, &GUID_WICPixelFormat24bppBGR, stride, &rgb).map_err(err)?
        } else {
            bitmap(wic, p)?
        };
        let stream = wic.CreateStream().map_err(err)?;
        stream.InitializeFromFilename(&HSTRING::from(path), windows::Win32::Foundation::GENERIC_WRITE.0).map_err(err)?;
        let enc = wic.CreateEncoder(&fmt.container(), std::ptr::null()).map_err(err)?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache).map_err(err)?;
        let mut frame: Option<IWICBitmapFrameEncode> = None;
        let mut bag: Option<IPropertyBag2> = None;
        enc.CreateNewFrame(&mut frame, &mut bag).map_err(err)?;
        let frame = frame.ok_or("no frame")?;
        if fmt == Fmt::Jpeg
            && let Some(b) = &bag
        {
            let mut name: Vec<u16> = "ImageQuality".encode_utf16().chain(Some(0)).collect();
            let prop = PROPBAG2 { pstrName: PWSTR(name.as_mut_ptr()), ..Default::default() };
            let v = VARIANT {
                Anonymous: VARIANT_0 {
                    Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                        vt: VT_R4,
                        wReserved1: 0,
                        wReserved2: 0,
                        wReserved3: 0,
                        Anonymous: VARIANT_0_0_0 { fltVal: quality.clamp(1, 100) as f32 / 100.0 },
                    }),
                },
            };
            let _ = b.Write(1, &prop, &v);
        }
        frame.Initialize(bag.as_ref()).map_err(err)?;
        frame.SetSize(p.w, p.h).map_err(err)?;
        let mut want = if matches!(fmt, Fmt::Jpeg | Fmt::Bmp) { GUID_WICPixelFormat24bppBGR } else { GUID_WICPixelFormat32bppBGRA };
        frame.SetPixelFormat(&mut want).map_err(err)?;
        frame.WriteSource(&source, std::ptr::null()).map_err(err)?;
        frame.Commit().map_err(err)?;
        enc.Commit().map_err(err)?;
        Ok(())
    }
}

/// An .ico made of PNG pictures (Windows Vista and later read them): (side, PNG bytes) each.
pub fn ico_bytes(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * entries.len() as u32;
    for (side, png) in entries {
        let b = if *side >= 256 { 0 } else { *side as u8 };
        out.extend_from_slice(&[b, b, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in entries {
        out.extend_from_slice(png);
    }
    out
}

fn temp_png(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!("plume-convert-{}-{tag}-{}.png", std::process::id(), N.fetch_add(1, Ordering::Relaxed)))
}

/// The icon sizes for a picture whose longer side is `long`.
pub fn ico_sizes(long: u32) -> Vec<u32> {
    let all = [16, 24, 32, 48, 64, 128, 256];
    let v: Vec<u32> = all.into_iter().filter(|s| *s <= long.max(16)).collect();
    if v.is_empty() { vec![16] } else { v }
}

/// The whole job: `src` to `dst` (already reserved) in `fmt`, the longer side at most `max` (0: as it is).
/// `webp`: makes the WebP from a PNG (FFmpeg).
pub fn convert(src: &Path, dst: &Path, fmt: Fmt, quality: u32, max: u32, webp: &dyn Fn(&Path, &Path, u32) -> Result<(), String>) -> Result<(), String> {
    let wic = factory()?;
    let mut p = load(&wic, src)?;
    let (w, h) = crate::ffmpeg::fit_long(p.w, p.h, if fmt == Fmt::Ico { if max == 0 { 256 } else { max.min(256) } } else { max });
    if (w, h) != (p.w, p.h) {
        p = resize(&wic, &p, w, h)?;
    }
    match fmt {
        Fmt::Webp => {
            let tmp = temp_png("webp");
            let r = encode(&wic, &p, Fmt::Png, 100, &tmp).and_then(|_| webp(&tmp, dst, quality));
            let _ = std::fs::remove_file(&tmp);
            r
        }
        Fmt::Ico => {
            let mut entries = Vec::new();
            for s in ico_sizes(p.w.max(p.h)) {
                let (fw, fh) = crate::ffmpeg::fit_long(p.w, p.h, s);
                let small = pad_square(&resize(&wic, &p, fw, fh)?, s);
                let tmp = temp_png("ico");
                let r = encode(&wic, &small, Fmt::Png, 100, &tmp).and_then(|_| std::fs::read(&tmp).map_err(|e| e.to_string()));
                let _ = std::fs::remove_file(&tmp);
                entries.push((s, r?));
            }
            std::fs::write(dst, ico_bytes(&entries)).map_err(|e| e.to_string())
        }
        _ => encode(&wic, &p, fmt, quality, dst),
    }
}

/// A PNG in memory for Direct2D (premultiplied), its longer side at most `px`: the logo.
pub fn decode_bytes(wic: &IWICImagingFactory, bytes: &[u8], px: u32) -> Option<IWICBitmapSource> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        let (nw, nh) = crate::ffmpeg::fit_long(w, h, px);
        let s = wic.CreateBitmapScaler().ok()?;
        s.Initialize(&frame, nw, nh, WICBitmapInterpolationModeHighQualityCubic).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&s, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        Some(conv.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn com() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
    }

    fn sample(w: u32, h: u32) -> Pixels {
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                // left half opaque red, right half transparent
                let a = if x < w / 2 { 255 } else { 0 };
                data.extend_from_slice(&[0, (y % 256) as u8, 255, a]);
            }
        }
        Pixels { w, h, data }
    }

    #[test]
    fn white_background() {
        let p = Pixels { w: 2, h: 1, data: vec![0, 0, 255, 255, 0, 0, 0, 0] };
        let (rgb, stride) = flatten_white(&p);
        assert_eq!(stride, 8);
        assert_eq!(&rgb[..6], &[0, 0, 255, 255, 255, 255]);
    }

    #[test]
    fn square_padding() {
        let p = Pixels { w: 2, h: 1, data: vec![1, 2, 3, 4, 5, 6, 7, 8] };
        let s = pad_square(&p, 4);
        assert_eq!((s.w, s.h, s.data.len()), (4, 4, 64));
        // row 1 (offset y = 1), x from 1
        assert_eq!(&s.data[(4 + 1) * 4..(4 + 3) * 4], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(s.data[..16].iter().all(|b| *b == 0));
    }

    #[test]
    fn padding_on_black() {
        // half-transparent blue pixel on a 3×1 black canvas
        let p = Pixels { w: 1, h: 1, data: vec![200, 0, 0, 128] };
        let s = pad_to(&p, 3, 1, true);
        assert_eq!(s.data, vec![0, 0, 0, 255, 100, 0, 0, 255, 0, 0, 0, 255]);
    }

    #[test]
    fn gif_frames_share_one_size() {
        com();
        let wic = factory().unwrap();
        let d = std::env::temp_dir().join(format!("plume-convert-gifframes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("f")).unwrap();
        let srcs: Vec<std::path::PathBuf> = [(400, 300), (200, 400), (100, 100)]
            .iter()
            .enumerate()
            .map(|(i, (w, h))| {
                let p = d.join(format!("p{i}.png"));
                encode(&wic, &sample(*w, *h), Fmt::Png, 100, &p).unwrap();
                p
            })
            .collect();
        let mut seen = 0;
        let size = gif_frames(&srcs, 200, &d.join("f"), &mut |i| {
            seen = i + 1;
            true
        })
        .unwrap();
        // 400×300 → 200×150, 200×400 → 100×200, 100×100 stays: the canvas is 200×200
        assert_eq!(size, (200, 200));
        assert_eq!(seen, 3);
        for i in 1..=3 {
            let f = load(&wic, &d.join("f").join(format!("frame_{i:04}.png"))).unwrap();
            assert_eq!((f.w, f.h), (200, 200));
            assert_eq!(f.data[3], 255, "laid on black");
        }
        // a stop asked for after the first photo
        assert!(gif_frames(&srcs, 200, &d.join("f"), &mut |_| false).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn icon_container() {
        let ico = ico_bytes(&[(16, vec![1, 2, 3]), (256, vec![4, 5])]);
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 2, 0]);
        assert_eq!(ico[6], 16);
        assert_eq!(ico[22], 0); // 256 is written as 0
        assert_eq!(u32::from_le_bytes(ico[18..22].try_into().unwrap()), 38);
        assert_eq!(u32::from_le_bytes(ico[34..38].try_into().unwrap()), 41);
        assert_eq!(&ico[38..], &[1, 2, 3, 4, 5]);
        assert_eq!(ico_sizes(40), vec![16, 24, 32]);
        assert_eq!(ico_sizes(4000).len(), 7);
        assert_eq!(ico_sizes(8), vec![16]);
        assert_eq!(orientation_transform(6), WICBitmapTransformRotate90);
        assert_eq!(orientation_transform(1).0, 0);
    }

    #[test]
    fn wic_round_trip() {
        com();
        let wic = factory().unwrap();
        let d = std::env::temp_dir().join(format!("plume-convert-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let src = d.join("src.png");
        encode(&wic, &sample(64, 40), Fmt::Png, 100, &src).unwrap();
        let back = load(&wic, &src).unwrap();
        assert_eq!((back.w, back.h), (64, 40));
        assert_eq!(&back.data[..4], &[0, 0, 255, 255]);
        let no_webp = |_: &Path, _: &Path, _: u32| -> Result<(), String> { Err("no".into()) };
        for (fmt, name) in [(Fmt::Jpeg, "a.jpg"), (Fmt::Bmp, "a.bmp"), (Fmt::Tiff, "a.tif"), (Fmt::Png, "a.png")] {
            let out = d.join(name);
            convert(&src, &out, fmt, 80, 32, &no_webp).unwrap();
            let p = load(&wic, &out).unwrap();
            assert_eq!((p.w, p.h), (32, 20), "{name}");
        }
        // JPEG: the transparent half became white
        let j = load(&wic, &d.join("a.jpg")).unwrap();
        let px = &j.data[(10 * 32 + 30) * 4..(10 * 32 + 31) * 4];
        assert!(px[0] > 230 && px[1] > 230 && px[2] > 230, "{px:?}");
        let ico = d.join("a.ico");
        convert(&src, &ico, Fmt::Ico, 0, 0, &no_webp).unwrap();
        let bytes = std::fs::read(&ico).unwrap();
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 5); // 16, 24, 32, 48, 64
        let i = load(&wic, &ico).unwrap();
        assert_eq!(i.w, i.h);
        assert!(convert(&d.join("missing.png"), &d.join("x.png"), Fmt::Png, 80, 0, &no_webp).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
