//! Pictures: album covers (downloaded once into the cache, or taken from the file's tags), their main colors (for
//! the backgrounds and the accent), animated covers (Apple Music's motion artwork, turned by FFmpeg into a few
//! seconds of small JPEG frames) and animated GIF backgrounds. Decoding is Windows' own (WIC).

use crate::{
    library::{Track, data_dir},
    net, yt,
};
use plume_ui::Color;
use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use windows::{
    Win32::{
        Graphics::Imaging::*,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    },
    core::{GUID, Interface},
};

pub fn covers_dir() -> PathBuf {
    data_dir().join("covers")
}

/// The cover source of a song: its URL, or "file:<path>" for a picture inside the file.
pub fn source(t: &Track) -> Option<String> {
    if let Some(p) = t.path.as_ref().filter(|p| p.is_file()) {
        // a downloaded song carries its cover; a scanned one maybe
        return Some(format!("file:{}", p.display()));
    }
    t.cover.clone()
}

pub fn key(src: &str) -> u64 {
    yt::fnv(src)
}

/// The cached picture of a source, if it is there.
pub fn cached(src: &str) -> Option<PathBuf> {
    let p = covers_dir().join(format!("{:016x}.img", key(src)));
    p.is_file().then_some(p)
}

fn none_marker(src: &str) -> PathBuf {
    covers_dir().join(format!("{:016x}.none", key(src)))
}

/// Fetches a cover into the cache (blocking). False: there is none.
pub fn fetch(src: &str) -> bool {
    if cached(src).is_some() {
        return true;
    }
    if none_marker(src).exists() {
        return false;
    }
    let _ = std::fs::create_dir_all(covers_dir());
    let data = if let Some(path) = src.strip_prefix("file:") {
        embedded(Path::new(path)).or_else(|| folder_image(Path::new(path)))
    } else if src.starts_with("http") {
        net::get(src).ok().filter(|d| d.len() > 100)
    } else {
        None
    };
    match data {
        Some(d) => {
            let p = covers_dir().join(format!("{:016x}.img", key(src)));
            let tmp = p.with_extension("tmp");
            std::fs::write(&tmp, d).is_ok() && std::fs::rename(&tmp, &p).is_ok()
        }
        None => {
            let _ = std::fs::write(none_marker(src), b"");
            false
        }
    }
}

/// The front cover inside a music file.
pub fn embedded(path: &Path) -> Option<Vec<u8>> {
    use lofty::{file::TaggedFileExt, picture::PictureType};
    let f = lofty::read_from_path(path).ok()?;
    let tag = f.primary_tag().or_else(|| f.first_tag())?;
    let pics = tag.pictures();
    pics.iter().find(|p| p.pic_type() == PictureType::CoverFront).or(pics.first()).map(|p| p.data().to_vec())
}

/// cover.jpg / folder.jpg next to the file.
fn folder_image(path: &Path) -> Option<Vec<u8>> {
    let dir = path.parent()?;
    ["cover.jpg", "folder.jpg", "cover.png", "front.jpg", "album.jpg", "Cover.jpg", "Folder.jpg"].iter().find_map(|n| std::fs::read(dir.join(n)).ok())
}

pub fn wic() -> Option<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() }
}

/// A picture file, scaled to fit `px` on its longer side (premultiplied BGRA, ready for Direct2D).
pub fn decode_file(wic: &IWICImagingFactory, path: &Path, px: u32) -> Option<IWICBitmapSource> {
    unsafe {
        let dec = wic
            .CreateDecoderFromFilename(&windows::core::HSTRING::from(path), None, windows::Win32::Foundation::GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .ok()?;
        scaled(wic, &dec.GetFrame(0).ok()?.cast().ok()?, px)
    }
}

/// A picture in memory (`bytes` must live until Direct2D has copied it: the next draw).
pub fn decode_bytes(wic: &IWICImagingFactory, bytes: &[u8], px: u32) -> Option<IWICBitmapSource> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        scaled(wic, &dec.GetFrame(0).ok()?.cast().ok()?, px)
    }
}

fn scaled(wic: &IWICImagingFactory, src: &IWICBitmapSource, px: u32) -> Option<IWICBitmapSource> {
    unsafe {
        let (mut w, mut h) = (0u32, 0u32);
        src.GetSize(&mut w, &mut h).ok()?;
        // covers are drawn square: a wide picture (a video's) keeps its middle
        let cropped: IWICBitmapSource;
        let src = if w != h && w > 0 && h > 0 {
            let side = w.min(h);
            let clip = wic.CreateBitmapClipper().ok()?;
            let r = WICRect { X: ((w - side) / 2) as i32, Y: ((h - side) / 2) as i32, Width: side as i32, Height: side as i32 };
            clip.Initialize(src, &r).ok()?;
            cropped = clip.cast().ok()?;
            w = side;
            h = side;
            &cropped
        } else {
            src
        };
        let k = (px as f32 / w.max(h).max(1) as f32).min(1.0);
        let (nw, nh) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
        let input: IWICBitmapSource = if k < 1.0 {
            let s = wic.CreateBitmapScaler().ok()?;
            s.Initialize(src, nw, nh, WICBitmapInterpolationModeFant).ok()?;
            s.cast().ok()?
        } else {
            src.clone()
        };
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&input, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        // decoded now, into memory: the file can go, and drawing does not decode again
        let bmp = wic.CreateBitmapFromSource(&conv, WICBitmapCacheOnLoad).ok()?;
        bmp.cast().ok()
    }
}

/// Pixels of a decoded picture (BGRA).
fn pixels(src: &IWICBitmapSource) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let (mut w, mut h) = (0u32, 0u32);
        src.GetSize(&mut w, &mut h).ok()?;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        src.CopyPixels(std::ptr::null(), w * 4, &mut buf).ok()?;
        Some((w, h, buf))
    }
}

/// The main colors of a cover, the most colorful first (for gradients, the accent and the animated background).
pub fn palette(wic: &IWICImagingFactory, path: &Path) -> Vec<Color> {
    let Some(src) = decode_file(wic, path, 32) else { return Vec::new() };
    let Some((_, _, px)) = pixels(&src) else { return Vec::new() };
    // 4×4×4 color boxes, weighted by how many pixels and how colorful
    let mut bins = vec![(0u32, [0u32; 3]); 64];
    for p in px.as_chunks::<4>().0 {
        let (b, g, r) = (p[0] as u32, p[1] as u32, p[2] as u32);
        let i = ((r >> 6) << 4 | (g >> 6) << 2 | (b >> 6)) as usize;
        bins[i].0 += 1;
        bins[i].1[0] += r;
        bins[i].1[1] += g;
        bins[i].1[2] += b;
    }
    let mut cols: Vec<(f32, Color)> = bins
        .into_iter()
        .filter(|b| b.0 > 0)
        .map(|(n, s)| {
            let (r, g, b) = (s[0] as f32 / n as f32 / 255.0, s[1] as f32 / n as f32 / 255.0, s[2] as f32 / n as f32 / 255.0);
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
            let weight = n as f32 * (0.25 + sat) * (0.3 + max.min(0.85));
            (weight, Color { r, g, b, a: 1.0 })
        })
        .collect();
    cols.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<Color> = Vec::new();
    for (_, c) in cols {
        if out.iter().all(|o| (o.r - c.r).abs() + (o.g - c.g).abs() + (o.b - c.b).abs() > 0.25) {
            out.push(c);
        }
        if out.len() == 4 {
            break;
        }
    }
    out
}

/// A color bright enough for buttons and text on a dark background.
pub fn vivid(c: Color) -> Color {
    let max = c.r.max(c.g).max(c.b).max(0.01);
    let k = (0.85 / max).min(5.0);
    let mut v = Color { r: (c.r * k).min(1.0), g: (c.g * k).min(1.0), b: (c.b * k).min(1.0), a: 1.0 };
    // too grey: Plume Music's rose
    let min = v.r.min(v.g).min(v.b);
    if v.r.max(v.g).max(v.b) - min < 0.18 {
        v = Color::hex(0xfb3e6e);
    }
    v
}

// ───────────────────────── animated covers ─────────────────────────

/// Frames of an animated cover (JPEG each) and their rate.
pub struct Motion {
    pub frames: Vec<Vec<u8>>,
    pub fps: f32,
}

fn motion_file(album_key: &str) -> PathBuf {
    data_dir().join("motion").join(format!("{:016x}.mjpg", yt::fnv(album_key)))
}

/// Splits FFmpeg's MJPEG stream into its JPEG frames.
fn split_jpegs(d: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 < d.len() {
        if d[i] == 0xff && d[i + 1] == 0xd8 {
            let start = i;
            let mut j = i + 2;
            while j + 1 < d.len() && !(d[j] == 0xff && d[j + 1] == 0xd9) {
                j += 1;
            }
            out.push(d[start..(j + 2).min(d.len())].to_vec());
            i = j + 2;
        } else {
            i += 1;
        }
    }
    out
}

/// The animated cover of an album, if Apple Music has one (blocking: a search, a page, FFmpeg; a few seconds).
pub fn motion(t: &Track) -> Option<Motion> {
    let artist = t.artist.split(',').next().unwrap_or("").trim().to_string();
    let album = if t.album.is_empty() { t.title.clone() } else { t.album.clone() };
    let akey = format!("{}\u{1}{}", artist.to_lowercase(), album.to_lowercase());
    let file = motion_file(&akey);
    let none = file.with_extension("none");
    if let Ok(d) = std::fs::read(&file) {
        let frames = split_jpegs(&d);
        return (frames.len() > 4).then_some(Motion { frames, fps: 15.0 });
    }
    if none.exists() || !yt::have_ffmpeg() || artist.is_empty() {
        return None;
    }
    let _ = std::fs::create_dir_all(file.parent()?);
    let found = (|| -> Option<Vec<u8>> {
        let q = net::enc(&format!("{artist} {album}"));
        let v = net::get_json(&format!("https://itunes.apple.com/search?term={q}&entity=album&limit=5")).ok()?;
        let page = v["results"].as_array()?.iter().find(|r| {
            crate::yt::score(
                &Track { title: album.clone(), artist: artist.clone(), ..Default::default() },
                &Track {
                    title: r["collectionName"].as_str().unwrap_or("").into(),
                    artist: r["artistName"].as_str().unwrap_or("").into(),
                    ..Default::default()
                },
            ) > 0.6
        })?["collectionViewUrl"]
            .as_str()?
            .split('?')
            .next()?
            .to_string();
        let html = net::get_text(&page).ok()?;
        let cover_tag = html.find("editorial-video--cover")?;
        let rest = &html[cover_tag..];
        let src = rest.split("src=\"").nth(1)?.split('"').next()?.to_string();
        // the smallest variant at least 400 px wide (the master lists up to 4K)
        let master = net::get_text(&src).ok()?;
        let base = &src[..src.rfind('/')? + 1];
        let mut best: Option<(u32, String)> = None;
        let mut lines = master.lines();
        while let Some(l) = lines.next() {
            if let Some(info) = l.strip_prefix("#EXT-X-STREAM-INF:") {
                let w = info.split("RESOLUTION=").nth(1).and_then(|r| r.split('x').next()).and_then(|w| w.parse::<u32>().ok()).unwrap_or(0);
                let uri = lines.next()?.trim().to_string();
                let full = if uri.starts_with("http") { uri } else { format!("{base}{uri}") };
                if w >= 400 && best.as_ref().is_none_or(|b| w < b.0) {
                    best = Some((w, full));
                }
            }
        }
        let variant = best.map(|b| b.1).unwrap_or(src);
        let out = Command::new(yt::ffmpeg())
            .creation_flags(0x0800_0000)
            .args(["-v", "error", "-y", "-t", "8", "-i", &variant, "-vf", "fps=15,scale=480:-2", "-q:v", "4", "-f", "image2pipe", "-c:v", "mjpeg", "-"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        (out.stdout.len() > 10_000).then_some(out.stdout)
    })();
    match found {
        Some(d) => {
            let _ = std::fs::write(&file, &d);
            let frames = split_jpegs(&d);
            (frames.len() > 4).then_some(Motion { frames, fps: 15.0 })
        }
        None => {
            let _ = std::fs::write(&none, b"");
            None
        }
    }
}

// ───────────────────────── animated GIF ─────────────────────────

/// An animated GIF, decoded frame after frame onto one canvas (GIF frames only redraw part of the picture).
pub struct Gif {
    dec: IWICBitmapDecoder,
    pub count: u32,
    pub index: u32,
    pub w: u32,
    pub h: u32,
    canvas: Vec<u8>,
    /// Delay of the current frame, in ms.
    pub delay: u32,
    prev_rect: Option<(u32, u32, u32, u32, u32)>,
}

fn prop_u32(r: &IWICMetadataQueryReader, name: &str) -> Option<u32> {
    use windows::Win32::System::{
        Com::StructuredStorage::PROPVARIANT,
        Variant::{VT_I4, VT_UI1, VT_UI2, VT_UI4},
    };
    unsafe {
        let mut v = PROPVARIANT::default();
        r.GetMetadataByName(&windows::core::HSTRING::from(name), &mut v).ok()?;
        let a = &v.Anonymous.Anonymous;
        match a.vt {
            VT_UI1 => Some(a.Anonymous.bVal as u32),
            VT_UI2 => Some(a.Anonymous.uiVal as u32),
            VT_UI4 => Some(a.Anonymous.ulVal),
            VT_I4 => Some(a.Anonymous.lVal.max(0) as u32),
            _ => None,
        }
    }
}

impl Gif {
    pub fn open(wic: &IWICImagingFactory, path: &Path) -> Option<Gif> {
        unsafe {
            let dec = wic
                .CreateDecoderFromFilename(&windows::core::HSTRING::from(path), None, windows::Win32::Foundation::GENERIC_READ, WICDecodeMetadataCacheOnDemand)
                .ok()?;
            let count = dec.GetFrameCount().ok()?;
            let (mut w, mut h) = (0, 0);
            if let Ok(m) = dec.GetMetadataQueryReader() {
                w = prop_u32(&m, "/logscrdesc/Width").unwrap_or(0);
                h = prop_u32(&m, "/logscrdesc/Height").unwrap_or(0);
            }
            if w == 0 || h == 0 {
                let f = dec.GetFrame(0).ok()?;
                f.GetSize(&mut w, &mut h).ok()?;
            }
            if w == 0 || h == 0 || w > 4096 || h > 4096 {
                return None;
            }
            let mut g = Gif { dec, count, index: u32::MAX, w, h, canvas: vec![0; (w * h * 4) as usize], delay: 100, prev_rect: None };
            g.advance(wic)?;
            Some(g)
        }
    }

    /// Draws the next frame onto the canvas; returns it ready for Direct2D.
    pub fn advance(&mut self, wic: &IWICImagingFactory) -> Option<IWICBitmapSource> {
        unsafe {
            let next = if self.index == u32::MAX || self.index + 1 >= self.count { 0 } else { self.index + 1 };
            if next == 0 {
                self.canvas.fill(0);
                self.prev_rect = None;
            }
            // "restore to background" of the frame before
            if let Some((x, y, w, h, 2)) = self.prev_rect {
                for row in y..(y + h).min(self.h) {
                    let a = ((row * self.w + x) * 4) as usize;
                    let b = ((row * self.w + (x + w).min(self.w)) * 4) as usize;
                    self.canvas[a..b].fill(0);
                }
            }
            let frame = self.dec.GetFrame(next).ok()?;
            let (mut fx, mut fy, mut disposal, mut delay) = (0, 0, 0, 10);
            if let Ok(m) = frame.GetMetadataQueryReader() {
                fx = prop_u32(&m, "/imgdesc/Left").unwrap_or(0);
                fy = prop_u32(&m, "/imgdesc/Top").unwrap_or(0);
                disposal = prop_u32(&m, "/grctlext/Disposal").unwrap_or(0);
                delay = prop_u32(&m, "/grctlext/Delay").unwrap_or(10);
            }
            let conv = wic.CreateFormatConverter().ok()?;
            conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
            let (mut fw, mut fh) = (0, 0);
            conv.GetSize(&mut fw, &mut fh).ok()?;
            let mut buf = vec![0u8; (fw * fh * 4) as usize];
            conv.CopyPixels(std::ptr::null(), fw * 4, &mut buf).ok()?;
            for row in 0..fh {
                let cy = fy + row;
                if cy >= self.h {
                    break;
                }
                for col in 0..fw {
                    let cx = fx + col;
                    if cx >= self.w {
                        break;
                    }
                    let s = ((row * fw + col) * 4) as usize;
                    if buf[s + 3] == 0 {
                        continue; // transparent: what was there stays
                    }
                    let d = ((cy * self.w + cx) * 4) as usize;
                    self.canvas[d..d + 4].copy_from_slice(&buf[s..s + 4]);
                }
            }
            self.prev_rect = Some((fx, fy, fw, fh, disposal));
            self.index = next;
            self.delay = if delay < 2 { 100 } else { delay * 10 };
            let bmp = wic.CreateBitmapFromMemory(self.w, self.h, &GUID_WICPixelFormat32bppPBGRA, self.w * 4, &self.canvas).ok()?;
            bmp.cast().ok()
        }
    }
}

#[allow(dead_code)]
const _G: GUID = GUID::zeroed();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_split() {
        let d = [0u8, 0xff, 0xd8, 1, 2, 0xff, 0xd9, 9, 0xff, 0xd8, 3, 0xff, 0xd9];
        let f = split_jpegs(&d);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0], vec![0xff, 0xd8, 1, 2, 0xff, 0xd9]);
    }

    #[test]
    fn vivid_colors() {
        let v = vivid(Color { r: 0.2, g: 0.05, b: 0.05, a: 1.0 });
        assert!(v.r > 0.8 && v.g < 0.3);
        let grey = vivid(Color { r: 0.4, g: 0.4, b: 0.41, a: 1.0 });
        assert!(grey.r > 0.9); // rose
    }
}
