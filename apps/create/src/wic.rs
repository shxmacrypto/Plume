//! Files of pictures. Opening goes through Windows' own codecs (WIC): PNG, JPEG, WebP, BMP, GIF (first frame),
//! TIFF, HEIC and AVIF when their codec is installed, turned as the camera says. Export writes PNG and JPEG with WIC
//! and WebP with the pure-Rust image-webp encoder (lossless; a quality below 100 rounds the colours a little first
//! so the file gets smaller). An export never replaces a file: "name (2).png" and so on.

use crate::engine::img::Img;
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::GENERIC_READ,
        Graphics::Imaging::*,
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, StructuredStorage::PROPBAG2, StructuredStorage::PROPVARIANT},
            Variant::VARIANT,
        },
    },
    core::{GUID, HSTRING, PCWSTR, PWSTR},
};

pub const OPEN_EXT: &[&str] = &["png", "jpg", "jpeg", "jpe", "jfif", "webp", "bmp", "dib", "gif", "tif", "tiff", "heic", "heif", "avif", "ico", "jxr"];

/// The long side a picture is brought down to when it is bigger (memory and speed stay reasonable).
pub const MAX_SIDE: u32 = 8192;

pub fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

pub fn is_picture(p: &Path) -> bool {
    OPEN_EXT.contains(&ext(p).as_str())
}

fn wic() -> Result<IWICImagingFactory, String> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message()) }
}

fn orientation(frame: &IWICBitmapFrameDecode) -> u32 {
    unsafe {
        let Ok(r) = frame.GetMetadataQueryReader() else { return 1 };
        for name in ["System.Photo.Orientation", "/app1/ifd/{ushort=274}", "/ifd/{ushort=274}"] {
            let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let mut v = PROPVARIANT::default();
            if r.GetMetadataByName(PCWSTR(wide.as_ptr()), &mut v).is_ok()
                && let Ok(o) = u16::try_from(&v)
            {
                return o as u32;
            }
        }
        1
    }
}

fn transform_of(o: u32) -> WICBitmapTransformOptions {
    match o {
        2 => WICBitmapTransformFlipHorizontal,
        3 => WICBitmapTransformRotate180,
        4 => WICBitmapTransformFlipVertical,
        5 => WICBitmapTransformOptions(WICBitmapTransformRotate90.0 | WICBitmapTransformFlipHorizontal.0),
        6 => WICBitmapTransformRotate90,
        7 => WICBitmapTransformOptions(WICBitmapTransformRotate270.0 | WICBitmapTransformFlipHorizontal.0),
        8 => WICBitmapTransformRotate270,
        _ => WICBitmapTransformRotate0,
    }
}

/// Opens a picture: its first frame, the right way up, at most [`MAX_SIDE`] pixels on its long side.
pub fn open(path: &Path) -> Result<Img, String> {
    let wic = wic()?;
    unsafe {
        let dec = wic
            .CreateDecoderFromFilename(&HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .map_err(|e| if e.code().0 as u32 == 0x8898_2F50 { "no codec".to_string() } else { e.message() })?;
        let frame = dec.GetFrame(0).map_err(|e| e.message())?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).map_err(|e| e.message())?;
        if w == 0 || h == 0 {
            return Err("empty".into());
        }
        let o = orientation(&frame);
        let mut src: IWICBitmapSource = windows::core::Interface::cast(&frame).map_err(|e: windows::core::Error| e.message())?;
        if w.max(h) > MAX_SIDE {
            let k = MAX_SIDE as f64 / w.max(h) as f64;
            let s = wic.CreateBitmapScaler().map_err(|e| e.message())?;
            s.Initialize(&src, ((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1), WICBitmapInterpolationModeFant)
                .map_err(|e| e.message())?;
            src = s.into();
        }
        if o > 1 {
            let r = wic.CreateBitmapFlipRotator().map_err(|e| e.message())?;
            r.Initialize(&src, transform_of(o)).map_err(|e| e.message())?;
            src = r.into();
        }
        let conv = wic.CreateFormatConverter().map_err(|e| e.message())?;
        conv.Initialize(&src, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).map_err(|e| e.message())?;
        let (mut cw, mut ch) = (0, 0);
        conv.GetSize(&mut cw, &mut ch).map_err(|e| e.message())?;
        let mut data = vec![0u8; cw as usize * ch as usize * 4];
        conv.CopyPixels(std::ptr::null(), cw * 4, &mut data).map_err(|e| e.message())?;
        Ok(Img::from_bgra_premul(cw, ch, &data))
    }
}

/// Decodes a picture held in memory (the app's own logo).
pub fn decode_bytes(bytes: &[u8], max: u32) -> Option<Img> {
    unsafe {
        let wic = wic().ok()?;
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        let mut src: IWICBitmapSource = windows::core::Interface::cast(&frame).ok()?;
        if w.max(h) > max {
            let k = max as f64 / w.max(h) as f64;
            let s = wic.CreateBitmapScaler().ok()?;
            s.Initialize(&src, ((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1), WICBitmapInterpolationModeFant).ok()?;
            src = s.into();
        }
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&src, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        let (mut cw, mut ch) = (0, 0);
        conv.GetSize(&mut cw, &mut ch).ok()?;
        let mut data = vec![0u8; cw as usize * ch as usize * 4];
        conv.CopyPixels(std::ptr::null(), cw * 4, &mut data).ok()?;
        Some(Img::from_bgra_premul(cw, ch, &data))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum Format {
    #[default]
    Png,
    Jpeg,
    Webp,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Webp => "WebP",
        }
    }
}

/// A path that does not exist yet: the one given, else "name (2).ext", "name (3).ext"…
pub fn unique(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    (2..).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| !p.exists()).unwrap()
}

/// Writes the picture (never over an existing file). `quality` 1 to 100 (JPEG, WebP). Returns the path written.
pub fn export(img: &Img, path: &Path, format: Format, quality: u32) -> Result<PathBuf, String> {
    let path = unique(path);
    let tmp = path.with_extension("plume-tmp");
    let r = match format {
        Format::Webp => write_webp(img, &tmp, quality),
        _ => write_wic(img, &tmp, format, quality),
    };
    if let Err(e) = r {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    // the name is taken now; a file that appeared meanwhile is still not replaced
    let path = unique(&path);
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })?;
    Ok(path)
}

fn write_webp(img: &Img, path: &Path, quality: u32) -> Result<(), String> {
    // fewer distinct colours compress better: drop low bits when the quality allows it
    let bits = match quality {
        95.. => 0,
        80..=94 => 1,
        60..=79 => 2,
        _ => 3,
    };
    let data: Vec<u8> = if bits == 0 {
        img.data.clone()
    } else {
        let half = 1u16 << (bits - 1);
        img.data.iter().enumerate().map(|(i, v)| if i % 4 == 3 { *v } else { (((*v as u16 + half) >> bits) << bits).min(255) as u8 }).collect()
    };
    let opaque = img.data.as_chunks::<4>().0.iter().all(|p| p[3] == 255);
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let w = std::io::BufWriter::new(f);
    let enc = image_webp::WebPEncoder::new(w);
    if opaque {
        let rgb: Vec<u8> = data.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
        enc.encode(&rgb, img.w, img.h, image_webp::ColorType::Rgb8).map_err(|e| e.to_string())
    } else {
        enc.encode(&data, img.w, img.h, image_webp::ColorType::Rgba8).map_err(|e| e.to_string())
    }
}

fn write_wic(img: &Img, path: &Path, format: Format, quality: u32) -> Result<(), String> {
    let wic = wic()?;
    let jpeg = format == Format::Jpeg;
    // JPEG has no transparency: on white
    let mut bgra = img.to_bgra_premul();
    if jpeg {
        for p in bgra.as_chunks_mut::<4>().0.iter_mut() {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = (*c as u32 + (255 - a)).min(255) as u8;
            }
            p[3] = 255;
        }
    }
    unsafe {
        let bmp = wic.CreateBitmapFromMemory(img.w, img.h, &GUID_WICPixelFormat32bppPBGRA, img.w * 4, &bgra).map_err(|e| e.message())?;
        let stream = wic.CreateStream().map_err(|e| e.message())?;
        stream.InitializeFromFilename(&HSTRING::from(path), windows::Win32::Foundation::GENERIC_WRITE.0).map_err(|e| e.message())?;
        let container: GUID = if jpeg { GUID_ContainerFormatJpeg } else { GUID_ContainerFormatPng };
        let enc = wic.CreateEncoder(&container, std::ptr::null()).map_err(|e| e.message())?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache).map_err(|e| e.message())?;
        let mut frame = None;
        let mut bag = None;
        enc.CreateNewFrame(&mut frame, &mut bag).map_err(|e| e.message())?;
        let frame = frame.ok_or("frame")?;
        if jpeg && let Some(bag) = &bag {
            let mut name: Vec<u16> = "ImageQuality".encode_utf16().chain(Some(0)).collect();
            let opt = PROPBAG2 { pstrName: PWSTR(name.as_mut_ptr()), ..Default::default() };
            let v = VARIANT::from(quality.clamp(1, 100) as f32 / 100.0);
            let _ = bag.Write(1, &opt, &v);
        }
        frame.Initialize(bag.as_ref()).map_err(|e| e.message())?;
        frame.SetSize(img.w, img.h).map_err(|e| e.message())?;
        let mut fmt = if jpeg { GUID_WICPixelFormat24bppBGR } else { GUID_WICPixelFormat32bppBGRA };
        frame.SetPixelFormat(&mut fmt).map_err(|e| e.message())?;
        let conv = wic.CreateFormatConverter().map_err(|e| e.message())?;
        conv.Initialize(&bmp, &fmt, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).map_err(|e| e.message())?;
        frame.WriteSource(&conv, std::ptr::null()).map_err(|e| e.message())?;
        frame.Commit().map_err(|e| e.message())?;
        enc.Commit().map_err(|e| e.message())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn com() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
    }

    #[test]
    fn export_formats_and_never_overwrite() {
        com();
        let dir = std::env::temp_dir().join(format!("plume-create-wic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut img = Img::filled(64, 32, [30, 120, 220, 255]);
        for y in 0..32 {
            for x in 0..32 {
                img.set(x, y, [240, 40, 40, 255]);
            }
        }
        for f in [Format::Png, Format::Jpeg, Format::Webp] {
            let p = dir.join(format!("out.{}", f.ext()));
            let written = export(&img, &p, f, 90).unwrap();
            assert_eq!(written, p);
            let back = open(&written).unwrap();
            assert_eq!((back.w, back.h), (64, 32));
            let (l, r) = (back.get(10, 16), back.get(50, 16));
            assert!(l[0] > 200 && l[2] < 80, "{f:?} {l:?}");
            assert!(r[2] > 180 && r[0] < 80, "{f:?} {r:?}");
            // a second export does not replace the first
            let again = export(&img, &p, f, 90).unwrap();
            assert_eq!(again, dir.join(format!("out (2).{}", f.ext())));
        }
        // transparency survives PNG; JPEG goes on white
        let clear = Img::new(8, 8);
        let p = export(&clear, &dir.join("clear.png"), Format::Png, 100).unwrap();
        assert_eq!(open(&p).unwrap().get(3, 3)[3], 0);
        let p = export(&clear, &dir.join("clear.jpg"), Format::Jpeg, 100).unwrap();
        assert!(open(&p).unwrap().get(3, 3)[0] > 250);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
