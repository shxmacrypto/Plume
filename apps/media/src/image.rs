//! Pictures through Windows' own codecs (WIC): JPEG, PNG, GIF (animated too), BMP, TIFF, WebP, HEIC and camera RAW
//! when their codec is installed. Decoding happens off the interface thread and gives plain pixels ([`Px`]); the
//! window turns them into Direct2D bitmaps. Also: thumbnails cached on disk, and rotation written into the file
//! (JPEG: only the EXIF orientation changes, the picture itself is not re-encoded).

use crate::{exif_info, settings::data_dir};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::GENERIC_READ,
        Graphics::Imaging::*,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, StructuredStorage::PROPVARIANT},
    },
    core::{GUID, HSTRING, PCWSTR},
};

/// Pixels: BGRA premultiplied, `w * 4` bytes per row.
#[derive(Clone, Default)]
pub struct Px {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Px {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Px({}x{})", self.w, self.h)
    }
}

impl Px {
    /// The same picture, at most `max` pixels on its long side (Windows' high quality scaler).
    pub fn scaled_to(self, max: u32) -> Px {
        if self.w.max(self.h) <= max || self.w == 0 || self.h == 0 {
            return self;
        }
        let k = max as f64 / self.w.max(self.h) as f64;
        let (nw, nh) = (((self.w as f64 * k).round() as u32).max(1), ((self.h as f64 * k).round() as u32).max(1));
        let scaled = (|| unsafe {
            let wic = wic()?;
            let bmp = wic.CreateBitmapFromMemory(self.w, self.h, &GUID_WICPixelFormat32bppPBGRA, self.w * 4, &self.data).ok()?;
            let s = wic.CreateBitmapScaler().ok()?;
            s.Initialize(&bmp, nw, nh, WICBitmapInterpolationModeFant).ok()?;
            pixels(&s.into())
        })();
        scaled.unwrap_or(self)
    }
    pub fn bytes(&self) -> usize {
        self.data.len()
    }
}

pub fn wic() -> Option<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() }
}

/// The pixels of a WIC source, converted to BGRA premultiplied.
fn pixels(src: &IWICBitmapSource) -> Option<Px> {
    unsafe {
        let wic = wic()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(src, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        let (mut w, mut h) = (0, 0);
        conv.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let mut data = vec![0u8; w as usize * h as usize * 4];
        conv.CopyPixels(std::ptr::null(), w * 4, &mut data).ok()?;
        Some(Px { w, h, data })
    }
}

pub const PHOTO_EXT: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "dib", "tif", "tiff", "webp", "heic", "heif", "avif", "ico", "jxr", "wdp", "cr2", "cr3", "nef", "nrw",
    "arw", "dng", "orf", "rw2", "raf", "pef", "srw",
];
pub const VIDEO_EXT: &[&str] = &["mp4", "m4v", "mov", "mkv", "avi", "wmv", "webm", "mpg", "mpeg", "ts", "m2ts", "mts", "3gp", "3g2", "asf"];

fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}
pub fn is_photo(path: &Path) -> bool {
    PHOTO_EXT.contains(&ext(path).as_str())
}
pub fn is_video(path: &Path) -> bool {
    VIDEO_EXT.contains(&ext(path).as_str())
}
pub fn is_media(path: &Path) -> bool {
    is_photo(path) || is_video(path)
}
pub fn is_jpeg(path: &Path) -> bool {
    matches!(ext(path).as_str(), "jpg" | "jpeg" | "jpe" | "jfif")
}

/// A decoded picture: one frame, or the frames of an animated GIF and how long each one shows.
#[derive(Clone, Debug, Default)]
pub struct Decoded {
    pub frames: Vec<Px>,
    pub delays: Vec<Duration>,
    /// Size of the whole picture as shown (after its orientation), even when `frames` are smaller.
    pub full_w: u32,
    pub full_h: u32,
    /// The decoded frames are the picture at full size.
    pub full: bool,
}

impl Decoded {
    pub fn w(&self) -> u32 {
        self.frames.first().map_or(0, |f| f.w)
    }
    pub fn h(&self) -> u32 {
        self.frames.first().map_or(0, |f| f.h)
    }
    pub fn bytes(&self) -> usize {
        self.frames.iter().map(|f| f.bytes()).sum()
    }
}

/// EXIF orientation (1…8) → WIC's turn and flip (WIC turns first, then flips).
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

/// Orientations that swap width and height.
pub fn swaps(o: u32) -> bool {
    matches!(o, 5..=8)
}

fn decoder(wic: &IWICImagingFactory, path: &Path) -> Result<IWICBitmapDecoder, String> {
    unsafe {
        wic.CreateDecoderFromFilename(&HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).map_err(|e| {
            // WINCODEC_ERR_COMPONENTNOTFOUND: no codec for this format on this PC
            if e.code().0 as u32 == 0x8898_2F50 { "no codec".to_string() } else { e.message() }
        })
    }
}

/// Decodes a picture with at most `max` pixels on its long side (`u32::MAX`: full size), turned as its EXIF says.
pub fn decode(path: &Path, max: u32) -> Result<Decoded, String> {
    let wic = wic().ok_or("WIC")?;
    let dec = decoder(&wic, path)?;
    unsafe {
        let count = dec.GetFrameCount().map_err(|e| e.message())?;
        let gif = dec.GetContainerFormat().ok() == Some(GUID_ContainerFormatGif);
        if gif && count > 1 {
            return gif_frames(&wic, &dec, count, max);
        }
        let frame = dec.GetFrame(0).map_err(|e| e.message())?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).map_err(|e| e.message())?;
        if w == 0 || h == 0 {
            return Err("empty".into());
        }
        let o = exif_info::orientation(path);
        let mut src: IWICBitmapSource = windows::core::Interface::cast(&frame).map_err(|e: windows::core::Error| e.message())?;
        let big = w.max(h);
        let full = big <= max;
        if !full {
            let k = max as f64 / big as f64;
            let (nw, nh) = (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
            let s = wic.CreateBitmapScaler().map_err(|e| e.message())?;
            s.Initialize(&src, nw, nh, WICBitmapInterpolationModeFant).map_err(|e| e.message())?;
            src = s.into();
        }
        if o > 1 {
            let r = wic.CreateBitmapFlipRotator().map_err(|e| e.message())?;
            r.Initialize(&src, transform_of(o)).map_err(|e| e.message())?;
            src = r.into();
        }
        let px = pixels(&src).ok_or("pixels")?;
        let (full_w, full_h) = if swaps(o) { (h, w) } else { (w, h) };
        Ok(Decoded { frames: vec![px], delays: vec![], full_w, full_h, full })
    }
}

fn meta_u32(r: &IWICMetadataQueryReader, name: &str) -> Option<u32> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut v = PROPVARIANT::default();
    unsafe {
        r.GetMetadataByName(PCWSTR(wide.as_ptr()), &mut v).ok()?;
    }
    u32::try_from(&v).ok()
}

/// The frames of an animated GIF, composed as they show (each frame only paints part of the picture).
fn gif_frames(wic: &IWICImagingFactory, dec: &IWICBitmapDecoder, count: u32, max: u32) -> Result<Decoded, String> {
    unsafe {
        let meta = dec.GetMetadataQueryReader().ok();
        let first = dec.GetFrame(0).map_err(|e| e.message())?;
        let (mut fw, mut fh) = (0, 0);
        let _ = first.GetSize(&mut fw, &mut fh);
        let w = meta.as_ref().and_then(|m| meta_u32(m, "/logscrdesc/Width")).filter(|v| *v > 0).unwrap_or(fw).max(1);
        let h = meta.as_ref().and_then(|m| meta_u32(m, "/logscrdesc/Height")).filter(|v| *v > 0).unwrap_or(fh).max(1);
        let (cw, ch) = (w as usize, h as usize);
        let mut canvas = vec![0u8; cw * ch * 4];
        let mut frames = vec![];
        let mut delays = vec![];
        // at most ~300 MB of frames: a very long GIF shows its beginning
        let budget = 300usize << 20;
        let mut used = 0usize;
        for i in 0..count {
            let Ok(frame) = dec.GetFrame(i) else { break };
            let fm = frame.GetMetadataQueryReader().ok();
            let get = |n: &str| fm.as_ref().and_then(|m| meta_u32(m, n));
            let (left, top) = (get("/imgdesc/Left").unwrap_or(0) as usize, get("/imgdesc/Top").unwrap_or(0) as usize);
            let delay = get("/grctlext/Delay").unwrap_or(10);
            let disposal = get("/grctlext/Disposal").unwrap_or(0);
            let saved = (disposal == 3).then(|| canvas.clone());
            let Some(px) = windows::core::Interface::cast::<IWICBitmapSource>(&frame).ok().and_then(|s| pixels(&s)) else { break };
            for y in 0..px.h as usize {
                let cy = top + y;
                if cy >= ch {
                    break;
                }
                for x in 0..px.w as usize {
                    let cxp = left + x;
                    if cxp >= cw {
                        break;
                    }
                    let s = (y * px.w as usize + x) * 4;
                    if px.data[s + 3] > 0 {
                        let d = (cy * cw + cxp) * 4;
                        canvas[d..d + 4].copy_from_slice(&px.data[s..s + 4]);
                    }
                }
            }
            let shot = Px { w, h, data: canvas.clone() };
            let shot = if w.max(h) > max { shot.scaled_to(max) } else { shot };
            used += shot.bytes();
            frames.push(shot);
            // browsers show 0 or 1 (hundredths) as 0.1 s
            delays.push(Duration::from_millis(if delay <= 1 { 100 } else { delay as u64 * 10 }));
            match disposal {
                2 => {
                    for y in top..(top + px.h as usize).min(ch) {
                        let a = (y * cw + left.min(cw)) * 4;
                        let b = (y * cw + (left + px.w as usize).min(cw)) * 4;
                        canvas[a..b].fill(0);
                    }
                }
                3 => {
                    if let Some(s) = saved {
                        canvas = s;
                    }
                }
                _ => {}
            }
            if used > budget {
                break;
            }
        }
        if frames.is_empty() {
            return Err("gif".into());
        }
        let full = w.max(h) <= max;
        let _ = wic;
        Ok(Decoded { frames, delays, full_w: w, full_h: h, full })
    }
}

/// A picture held in memory (the app's logo), at most `max` pixels on its long side.
pub fn decode_bytes(bytes: &[u8], max: u32) -> Option<Px> {
    unsafe {
        let wic = wic()?;
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame: IWICBitmapSource = windows::core::Interface::cast(&dec.GetFrame(0).ok()?).ok()?;
        Some(pixels(&frame)?.scaled_to(max.max(1)))
    }
}

/// Size of a picture as shown (after its orientation), read from the header only.
pub fn size_of(path: &Path) -> Option<(u32, u32)> {
    let wic = wic()?;
    let dec = decoder(&wic, path).ok()?;
    unsafe {
        let f = dec.GetFrame(0).ok()?;
        let (mut w, mut h) = (0, 0);
        f.GetSize(&mut w, &mut h).ok()?;
        Some(if swaps(exif_info::orientation(path)) { (h, w) } else { (w, h) })
    }
}

/// FNV-1a: stable keys for cache files and bitmaps.
pub fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn thumbs_dir() -> PathBuf {
    data_dir().join("thumbs")
}

/// Long side of a thumbnail, in pixels.
pub const THUMB: u32 = 360;

/// The cache key of a file's thumbnail: it changes when the file does.
pub fn thumb_key(path: &Path, mtime: i64, size: u64) -> u64 {
    fnv(&format!("{}|{mtime}|{size}", path.display().to_string().to_lowercase()))
}

/// A thumbnail from the cache, made (and cached) the first time. Videos also give their length.
pub fn thumbnail(path: &Path, key: u64, video: bool) -> Option<(Px, f64)> {
    let dir = thumbs_dir();
    let file = dir.join(format!("{key:016x}.jpg"));
    let dur_file = dir.join(format!("{key:016x}.len"));
    if file.is_file()
        && let Ok(d) = decode(&file, THUMB)
        && let Some(px) = d.frames.into_iter().next()
    {
        let dur = std::fs::read_to_string(&dur_file).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
        return Some((px, dur));
    }
    let (px, dur) = if video {
        crate::video::thumbnail(path, THUMB)?
    } else {
        let d = decode(path, THUMB).ok()?;
        (d.frames.into_iter().next()?, 0.0)
    };
    let _ = std::fs::create_dir_all(&dir);
    save(&px, &file, Format::Jpeg);
    if video {
        let _ = std::fs::write(&dur_file, format!("{dur:.2}"));
    }
    Some((px, dur))
}

/// Size of the thumbnail cache, in bytes.
pub fn cache_size() -> u64 {
    std::fs::read_dir(thumbs_dir()).map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0)
}

pub fn clear_cache() {
    if let Ok(rd) = std::fs::read_dir(thumbs_dir()) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Png,
    Bmp,
    Tiff,
}

impl Format {
    fn container(self) -> GUID {
        match self {
            Format::Jpeg => GUID_ContainerFormatJpeg,
            Format::Png => GUID_ContainerFormatPng,
            Format::Bmp => GUID_ContainerFormatBmp,
            Format::Tiff => GUID_ContainerFormatTiff,
        }
    }
}

/// Writes pixels into a file (through a temporary file, so a failure leaves nothing half written).
pub fn save(px: &Px, path: &Path, format: Format) -> bool {
    let tmp = path.with_extension("plume-tmp");
    let ok = (|| unsafe {
        let wic = wic()?;
        let bmp = wic.CreateBitmapFromMemory(px.w, px.h, &GUID_WICPixelFormat32bppPBGRA, px.w * 4, &px.data).ok()?;
        write_source(&wic, &bmp.into(), &tmp, format)
    })()
    .is_some();
    if ok && std::fs::rename(&tmp, path).is_ok() {
        return true;
    }
    let _ = std::fs::remove_file(&tmp);
    false
}

unsafe fn write_source(wic: &IWICImagingFactory, src: &IWICBitmapSource, path: &Path, format: Format) -> Option<()> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromFilename(&HSTRING::from(path), windows::Win32::Foundation::GENERIC_WRITE.0).ok()?;
        let enc = wic.CreateEncoder(&format.container(), std::ptr::null()).ok()?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
        let mut frame = None;
        enc.CreateNewFrame(&mut frame, std::ptr::null_mut()).ok()?;
        let frame = frame?;
        frame.Initialize(None).ok()?;
        let (mut w, mut h) = (0, 0);
        src.GetSize(&mut w, &mut h).ok()?;
        frame.SetSize(w, h).ok()?;
        // JPEG has no transparency: 24 bits; the others keep it
        let mut fmt = if format == Format::Jpeg { GUID_WICPixelFormat24bppBGR } else { GUID_WICPixelFormat32bppBGRA };
        frame.SetPixelFormat(&mut fmt).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(src, &fmt, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        frame.WriteSource(&conv, std::ptr::null()).ok()?;
        frame.Commit().ok()?;
        enc.Commit().ok()?;
        Some(())
    }
}

/// How a rotation was kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotated {
    /// In the file, without touching the picture (JPEG's EXIF orientation).
    Lossless,
    /// The file was rewritten (PNG, BMP, TIFF: lossless formats).
    Rewritten,
    /// Only in Plume Media (formats it cannot write).
    ViewOnly,
}

/// EXIF orientation ↔ (quarter turns clockwise, mirrored first).
pub fn orientation_parts(o: u32) -> (u32, bool) {
    match o {
        2 => (0, true),
        3 => (2, false),
        4 => (2, true),
        5 => (3, true),
        6 => (1, false),
        7 => (1, true),
        8 => (3, false),
        _ => (0, false),
    }
}
pub fn orientation_from(turns: u32, mirrored: bool) -> u32 {
    match (turns % 4, mirrored) {
        (0, false) => 1,
        (1, false) => 6,
        (2, false) => 3,
        (3, false) => 8,
        (0, true) => 2,
        (1, true) => 7,
        (2, true) => 4,
        _ => 5,
    }
}

/// Turns a picture by `turns` quarter turns clockwise, in the file when its format allows it without loss.
/// The file keeps its modification date (the timeline does not move it).
pub fn rotate_file(path: &Path, turns: u32) -> Result<Rotated, String> {
    let turns = turns % 4;
    if turns == 0 {
        return Ok(Rotated::Lossless);
    }
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let keep_date = |p: &Path| {
        if let (Some(t), Ok(f)) = (modified, std::fs::OpenOptions::new().write(true).open(p)) {
            let _ = f.set_modified(t);
        }
    };
    if is_jpeg(path) {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let (t0, m) = orientation_parts(jpeg_orientation(&data).unwrap_or(1));
        let o = orientation_from(t0 + turns, m);
        let out = jpeg_set_orientation(&data, o as u16).ok_or("EXIF")?;
        let tmp = path.with_extension("plume-tmp");
        std::fs::write(&tmp, out).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })?;
        keep_date(path);
        return Ok(Rotated::Lossless);
    }
    let format = match ext(path).as_str() {
        "png" => Format::Png,
        "bmp" | "dib" => Format::Bmp,
        "tif" | "tiff" => Format::Tiff,
        _ => return Ok(Rotated::ViewOnly),
    };
    let wic = wic().ok_or("WIC")?;
    let tmp = path.with_extension("plume-tmp");
    let done = (|| unsafe {
        let dec = decoder(&wic, path).ok()?;
        if dec.GetFrameCount().ok()? != 1 {
            return None;
        }
        let frame = dec.GetFrame(0).ok()?;
        let r = wic.CreateBitmapFlipRotator().ok()?;
        let t = [WICBitmapTransformRotate0, WICBitmapTransformRotate90, WICBitmapTransformRotate180, WICBitmapTransformRotate270][turns as usize];
        r.Initialize(&frame, t).ok()?;
        // read fully before the file is replaced
        let bmp = wic.CreateBitmapFromSource(&r, WICBitmapCacheOnLoad).ok()?;
        drop(dec);
        write_source(&wic, &bmp.into(), &tmp, format)
    })();
    if done.is_none() {
        let _ = std::fs::remove_file(&tmp);
        return Ok(Rotated::ViewOnly);
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })?;
    keep_date(path);
    Ok(Rotated::Rewritten)
}

/// Where the EXIF block (TIFF header onward) of a JPEG starts and ends, and where its APP1 segment starts.
fn jpeg_exif(data: &[u8]) -> Option<(usize, usize, usize)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        if marker == 0xDA || marker == 0xD9 {
            return None; // picture data: no EXIF before it
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if len < 2 || i + 2 + len > data.len() {
            return None;
        }
        let body = &data[i + 4..i + 2 + len];
        if marker == 0xE1 && body.starts_with(b"Exif\0\0") {
            return Some((i, i + 10, i + 2 + len));
        }
        i += 2 + len;
    }
    None
}

struct Tiff<'a> {
    d: &'a [u8],
    le: bool,
}
impl Tiff<'_> {
    fn u16(&self, at: usize) -> Option<u16> {
        let b = self.d.get(at..at + 2)?;
        Some(if self.le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    }
    fn u32(&self, at: usize) -> Option<u32> {
        let b = self.d.get(at..at + 4)?;
        Some(if self.le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) })
    }
}

fn tiff(d: &[u8]) -> Option<Tiff<'_>> {
    let le = match d.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    Some(Tiff { d, le })
}

/// The orientation written in a JPEG's EXIF.
pub fn jpeg_orientation(data: &[u8]) -> Option<u32> {
    let (_, a, b) = jpeg_exif(data)?;
    let t = tiff(&data[a..b])?;
    let ifd = t.u32(4)? as usize;
    let n = t.u16(ifd)? as usize;
    (0..n).find_map(|k| {
        let e = ifd + 2 + k * 12;
        (t.u16(e)? == 0x0112).then(|| t.u16(e + 8).map(|v| v as u32)).flatten()
    })
}

/// A copy of the JPEG with its EXIF orientation set to `o`: in place when the tag is there; otherwise the first
/// directory is copied, with the tag, to the end of the EXIF block (nothing else moves, so no offset breaks);
/// and a JPEG without EXIF gets a minimal one.
pub fn jpeg_set_orientation(data: &[u8], o: u16) -> Option<Vec<u8>> {
    let Some((app1, a, b)) = jpeg_exif(data) else {
        if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
            return None;
        }
        // after a JFIF APP0 if there is one
        let mut at = 2;
        if data[2] == 0xFF && data[3] == 0xE0 && data.len() > 6 {
            at = 4 + u16::from_be_bytes([data[4], data[5]]) as usize;
        }
        let mut seg = vec![0xFF, 0xE1, 0, 0];
        seg.extend_from_slice(b"Exif\0\0II*\0\x08\0\0\0\x01\0");
        seg.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0]);
        seg.extend_from_slice(&o.to_le_bytes());
        seg.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let len = (seg.len() - 2) as u16;
        seg[2..4].copy_from_slice(&len.to_be_bytes());
        let mut out = Vec::with_capacity(data.len() + seg.len());
        out.extend_from_slice(&data[..at]);
        out.extend_from_slice(&seg);
        out.extend_from_slice(&data[at..]);
        return Some(out);
    };
    let block = &data[a..b];
    let t = tiff(block)?;
    let le = t.le;
    let w16 = |v: u16| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    let w32 = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    let ifd = t.u32(4)? as usize;
    let n = t.u16(ifd)? as usize;
    let mut entries = Vec::with_capacity(n + 1);
    for k in 0..n {
        let e = ifd + 2 + k * 12;
        let tag = t.u16(e)?;
        if tag == 0x0112 {
            // already there: just the value changes
            let mut out = data.to_vec();
            let at = a + e;
            out[at + 2..at + 4].copy_from_slice(&w16(3));
            out[at + 4..at + 8].copy_from_slice(&w32(1));
            out[at + 8..at + 10].copy_from_slice(&w16(o));
            out[at + 10..at + 12].copy_from_slice(&[0, 0]);
            return Some(out);
        }
        entries.push((tag, block.get(e..e + 12)?.to_vec()));
    }
    let next = t.u32(ifd + 2 + n * 12)?;
    let mut ent = Vec::with_capacity(12);
    ent.extend_from_slice(&w16(0x0112));
    ent.extend_from_slice(&w16(3));
    ent.extend_from_slice(&w32(1));
    ent.extend_from_slice(&w16(o));
    ent.extend_from_slice(&[0, 0]);
    entries.push((0x0112, ent));
    entries.sort_by_key(|e| e.0);
    let mut new_block = block.to_vec();
    // directories start on an even offset
    if new_block.len() % 2 == 1 {
        new_block.push(0);
    }
    let at = new_block.len() as u32;
    new_block.extend_from_slice(&w16(entries.len() as u16));
    for (_, e) in &entries {
        new_block.extend_from_slice(e);
    }
    new_block.extend_from_slice(&w32(next));
    new_block[4..8].copy_from_slice(&w32(at));
    let seg_len = new_block.len() + 8;
    if seg_len > 0xFFFF {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() + 16 + entries.len() * 12);
    out.extend_from_slice(&data[..app1]);
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&(seg_len as u16).to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(&new_block);
    out.extend_from_slice(&data[b..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare_jpeg() -> Vec<u8> {
        // SOI, a JFIF APP0, then the end: enough for the EXIF code (it stops at SOS or EOI)
        let mut d = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16];
        d.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        d.extend_from_slice(&[0xFF, 0xDA, 0, 2, 0xFF, 0xD9]);
        d
    }

    #[test]
    fn orientation_round_trip() {
        for o in 1..=8 {
            let (t, m) = orientation_parts(o);
            assert_eq!(orientation_from(t, m), o);
        }
        // four quarter turns come back
        let (t, m) = orientation_parts(6);
        assert_eq!(orientation_from(t + 3, m), 1);
    }

    #[test]
    fn exif_inserted_then_patched() {
        let d = bare_jpeg();
        assert_eq!(jpeg_orientation(&d), None);
        let d2 = jpeg_set_orientation(&d, 6).unwrap();
        assert_eq!(jpeg_orientation(&d2), Some(6));
        // the JFIF segment stays first
        assert_eq!(&d2[2..4], &[0xFF, 0xE0]);
        let d3 = jpeg_set_orientation(&d2, 3).unwrap();
        assert_eq!(d3.len(), d2.len());
        assert_eq!(jpeg_orientation(&d3), Some(3));
    }

    #[test]
    fn exif_without_orientation_gets_the_tag() {
        // EXIF with one tag (Make → offset to "Plume\0"), big endian
        let mut tiffb = b"MM\0*\0\0\0\x08".to_vec();
        tiffb.extend_from_slice(&[0, 1, 0x01, 0x0F, 0, 2, 0, 0, 0, 6, 0, 0, 0, 26, 0, 0, 0, 0]);
        tiffb.extend_from_slice(b"Plume\0");
        let mut d = vec![0xFF, 0xD8, 0xFF, 0xE1];
        d.extend_from_slice(&((tiffb.len() + 8) as u16).to_be_bytes());
        d.extend_from_slice(b"Exif\0\0");
        d.extend_from_slice(&tiffb);
        d.extend_from_slice(&[0xFF, 0xDA, 0, 2, 0xFF, 0xD9]);
        assert_eq!(jpeg_orientation(&d), None);
        let out = jpeg_set_orientation(&d, 8).unwrap();
        assert_eq!(jpeg_orientation(&out), Some(8));
        // the Make string is still where its offset says
        let (_, a, b) = jpeg_exif(&out).unwrap();
        assert_eq!(&out[a..b][26..31], b"Plume");
        assert!(out.ends_with(&[0xFF, 0xDA, 0, 2, 0xFF, 0xD9]));
    }

    #[test]
    fn kinds() {
        assert!(is_photo(Path::new("a.JPG")));
        assert!(is_video(Path::new("b.mkv")));
        assert!(!is_media(Path::new("c.txt")));
    }
}
