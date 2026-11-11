//! The screen, as tiles of what changed. Windows' Desktop Duplication gives the image of the main screen as soon as
//! something changes (no work while nothing moves); GDI takes over where it is not available (remote session,
//! old driver). Each changed 256 px tile is compressed to JPEG by Windows (WIC) and sent; the viewer puts it in place.
//!
//! Messages (little-endian), on the "screen" channel:
//!   tile       [1][u32 frame][u16 x][u16 y][u16 w][u16 h][jpeg]
//!   frame end  [2][u32 frame][u16 width][u16 height][i16 cursor x][i16 cursor y][u8 cursor visible]
//!   cursor     [3][i16 x][i16 y][u8 visible]
//!   goodbye    [4]  (the PC ends the session; sent by the session, see rtc.rs)

use std::{
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError},
    },
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HMODULE, POINT, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::{Common::*, *},
            Gdi::*,
            Imaging::*,
        },
        System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, IStream, STREAM_SEEK_CUR, STREAM_SEEK_SET},
        UI::{
            Shell::SHCreateMemStream,
            WindowsAndMessaging::{CURSOR_SHOWING, CURSORINFO, GetCursorInfo, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN},
        },
    },
    core::{Interface, w},
};

pub const TILE: usize = 256;

/// Counters, for the tests and for tuning: frames sent, tiles, bytes, time spent grabbing and compressing (µs).
pub mod stats {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    pub static FRAMES: AtomicU64 = AtomicU64::new(0);
    pub static TILES: AtomicU64 = AtomicU64::new(0);
    pub static BYTES: AtomicU64 = AtomicU64::new(0);
    pub static GRAB_US: AtomicU64 = AtomicU64::new(0);
    pub static ENCODE_US: AtomicU64 = AtomicU64::new(0);
    pub fn add(c: &AtomicU64, v: u64) {
        c.fetch_add(v, Relaxed);
    }
    #[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
    pub fn get() -> [u64; 5] {
        [FRAMES.load(Relaxed), TILES.load(Relaxed), BYTES.load(Relaxed), GRAB_US.load(Relaxed), ENCODE_US.load(Relaxed)]
    }
}
/// A message stays under the 64 KB that every WebRTC stack accepts.
const MAX_MSG: usize = 62 * 1024;

/// Where the image comes from on the Windows desktop, for the mouse: viewer pixel × scale + origin.
#[derive(Clone, Copy, Debug, Default)]
pub struct Geom {
    pub left: i32,
    pub top: i32,
    pub scale: i32,
    /// Size of the image sent.
    pub w: i32,
    pub h: i32,
}

/// A frame for the viewer: its messages, and whether it waits for an acknowledgement (frame end) or not (cursor).
pub struct Frame {
    pub msgs: Vec<Vec<u8>>,
    pub counted: bool,
}

/// What the session asks from the capture: one more frame (true: the whole screen).
pub type Want = bool;

struct Dxgi {
    ctx: ID3D11DeviceContext,
    dev: ID3D11Device,
    dup: IDXGIOutputDuplication,
    staging: Option<ID3D11Texture2D>,
    rect: RECT,
}

impl Dxgi {
    /// The duplication of the main screen (the one at 0,0), on the graphics card that drives it.
    fn new() -> Option<Dxgi> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
            let mut a = 0;
            while let Ok(adapter) = factory.EnumAdapters1(a) {
                a += 1;
                let mut o = 0;
                while let Ok(output) = adapter.EnumOutputs(o) {
                    o += 1;
                    let desc = output.GetDesc().ok()?;
                    let r = desc.DesktopCoordinates;
                    if !(r.left <= 0 && r.top <= 0 && r.right > 0 && r.bottom > 0) {
                        continue;
                    }
                    let mut dev = None;
                    let mut ctx = None;
                    D3D11CreateDevice(
                        &adapter,
                        D3D_DRIVER_TYPE_UNKNOWN,
                        HMODULE::default(),
                        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                        None,
                        D3D11_SDK_VERSION,
                        Some(&mut dev),
                        None,
                        Some(&mut ctx),
                    )
                    .ok()?;
                    let (dev, ctx) = (dev?, ctx?);
                    let out1: IDXGIOutput1 = output.cast().ok()?;
                    let dup = out1.DuplicateOutput(&dev).ok()?;
                    return Some(Dxgi { ctx, dev, dup, staging: None, rect: r });
                }
            }
            None
        }
    }

    /// Ok(true): a new image in `buf`; Ok(false): nothing changed during `wait`; Err: start again (screen change,
    /// secure desktop, resolution change).
    fn grab(&mut self, buf: &mut Vec<u8>, wait: u32) -> Result<bool, ()> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut res = None;
            match self.dup.AcquireNextFrame(wait, &mut info, &mut res) {
                Ok(()) => {}
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
                Err(_) => return Err(()),
            }
            let result = (|| -> Result<bool, ()> {
                if info.LastPresentTime == 0 && !buf.is_empty() {
                    return Ok(false); // the pointer moved, not the image
                }
                let tex: ID3D11Texture2D = res.as_ref().ok_or(())?.cast().map_err(|_| ())?;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                tex.GetDesc(&mut desc);
                let (w, h) = (desc.Width as usize, desc.Height as usize);
                if self.staging.is_none() {
                    let sd = D3D11_TEXTURE2D_DESC {
                        Width: desc.Width,
                        Height: desc.Height,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: desc.Format,
                        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                        Usage: D3D11_USAGE_STAGING,
                        BindFlags: 0,
                        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                        MiscFlags: 0,
                    };
                    let mut st = None;
                    self.dev.CreateTexture2D(&sd, None, Some(&mut st)).map_err(|_| ())?;
                    self.staging = st;
                }
                let st = self.staging.as_ref().ok_or(())?;
                self.ctx.CopyResource(st, &tex);
                let mut map = D3D11_MAPPED_SUBRESOURCE::default();
                self.ctx.Map(st, 0, D3D11_MAP_READ, 0, Some(&mut map)).map_err(|_| ())?;
                buf.resize(w * h * 4, 0);
                for y in 0..h {
                    let src = std::slice::from_raw_parts((map.pData as *const u8).add(y * map.RowPitch as usize), w * 4);
                    buf[y * w * 4..(y + 1) * w * 4].copy_from_slice(src);
                }
                self.ctx.Unmap(st, 0);
                Ok(true)
            })();
            let _ = self.dup.ReleaseFrame();
            result
        }
    }

    fn size(&self) -> (usize, usize) {
        ((self.rect.right - self.rect.left) as usize, (self.rect.bottom - self.rect.top) as usize)
    }
}

/// The main screen through GDI (slower, but works everywhere).
fn gdi_grab(buf: &mut Vec<u8>) -> Option<(usize, usize)> {
    unsafe {
        let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        if w <= 0 || h <= 0 {
            return None;
        }
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let ok = match CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bmp) => {
                let old = SelectObject(mem, bmp.into());
                let ok = BitBlt(mem, 0, 0, w, h, Some(screen), 0, 0, SRCCOPY | CAPTUREBLT).is_ok();
                if ok {
                    let n = (w * h * 4) as usize;
                    buf.resize(n, 0);
                    buf.copy_from_slice(std::slice::from_raw_parts(bits as *const u8, n));
                }
                SelectObject(mem, old);
                let _ = DeleteObject(bmp.into());
                ok
            }
            Err(_) => false,
        };
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        ok.then_some((w as usize, h as usize))
    }
}

/// The pointer, in pixels of the image sent.
fn cursor(g: &Geom) -> (i16, i16, bool) {
    unsafe {
        let mut ci = CURSORINFO { cbSize: size_of::<CURSORINFO>() as u32, ..Default::default() };
        if GetCursorInfo(&mut ci).is_err() {
            return (-1, -1, false);
        }
        let POINT { x, y } = ci.ptScreenPos;
        let s = g.scale.max(1);
        let (cx, cy) = ((x - g.left) / s, (y - g.top) / s);
        let inside = cx >= 0 && cy >= 0 && cx < g.w && cy < g.h;
        (cx.clamp(-1, i16::MAX as i32) as i16, cy.clamp(-1, i16::MAX as i32) as i16, inside && (ci.flags.0 & CURSOR_SHOWING.0) != 0)
    }
}

/// JPEG through Windows Imaging Component.
struct Jpeg {
    wic: IWICImagingFactory,
}

impl Jpeg {
    fn new() -> Option<Jpeg> {
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok().map(|wic| Jpeg { wic }) }
    }

    /// `bgr`: w × h pixels, 3 bytes each, rows of `stride` bytes.
    fn encode(&self, bgr: &[u8], w: u32, h: u32, stride: u32, quality: f32) -> Option<Vec<u8>> {
        unsafe {
            let stream: IStream = SHCreateMemStream(None)?;
            let enc = self.wic.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null()).ok()?;
            enc.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
            let mut frame = None;
            let mut props = None;
            enc.CreateNewFrame(&mut frame, &mut props).ok()?;
            let frame = frame?;
            if let Some(props) = &props {
                use windows::Win32::System::{
                    Com::StructuredStorage::PROPBAG2,
                    Variant::{VARIANT, VT_R4},
                };
                let name = w!("ImageQuality");
                let bag = PROPBAG2 { pstrName: windows::core::PWSTR(name.as_ptr() as *mut u16), ..Default::default() };
                let mut value = VARIANT::default();
                (*value.Anonymous.Anonymous).vt = VT_R4;
                (*value.Anonymous.Anonymous).Anonymous.fltVal = quality;
                let _ = props.Write(1, &bag, &value);
            }
            frame.Initialize(props.as_ref()).ok()?;
            frame.SetSize(w, h).ok()?;
            let mut fmt = GUID_WICPixelFormat24bppBGR;
            frame.SetPixelFormat(&mut fmt).ok()?;
            frame.WritePixels(h, stride, bgr).ok()?;
            frame.Commit().ok()?;
            enc.Commit().ok()?;
            let mut len = 0u64;
            stream.Seek(0, STREAM_SEEK_CUR, Some(&mut len)).ok()?;
            stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
            let mut out = vec![0u8; len as usize];
            let mut read = 0u32;
            stream.Read(out.as_mut_ptr() as *mut _, len as u32, Some(&mut read)).ok().ok()?;
            out.truncate(read as usize);
            Some(out)
        }
    }
}

/// Half size, averaging 2×2 pixels (very large screens: 4K and up).
fn halve(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (w2, h2) = (w / 2, h / 2);
    let mut out = vec![0u8; w2 * h2 * 4];
    for y in 0..h2 {
        let r0 = &src[(2 * y) * w * 4..];
        let r1 = &src[(2 * y + 1) * w * 4..];
        for x in 0..w2 {
            for c in 0..3 {
                let s = r0[8 * x + c] as u32 + r0[8 * x + 4 + c] as u32 + r1[8 * x + c] as u32 + r1[8 * x + 4 + c] as u32;
                out[(y * w2 + x) * 4 + c] = (s / 4) as u8;
            }
        }
    }
    out
}

/// The tiles of `cur` that differ from `prev` (all of them if `full`).
pub fn changed_tiles(cur: &[u8], prev: &[u8], w: usize, h: usize, full: bool) -> Vec<(usize, usize, usize, usize)> {
    let mut out = Vec::new();
    for ty in (0..h).step_by(TILE) {
        for tx in (0..w).step_by(TILE) {
            let (tw, th) = (TILE.min(w - tx), TILE.min(h - ty));
            let differs = full
                || prev.len() != cur.len()
                || (ty..ty + th).any(|y| {
                    let a = (y * w + tx) * 4;
                    cur[a..a + tw * 4] != prev[a..a + tw * 4]
                });
            if differs {
                out.push((tx, ty, tw, th));
            }
        }
    }
    out
}

fn tile_msg(jpeg: &Jpeg, img: &[u8], w: usize, frame: u32, (tx, ty, tw, th): (usize, usize, usize, usize), bgr: &mut Vec<u8>) -> Option<Vec<u8>> {
    bgr.clear();
    let stride = (tw * 3 + 3) & !3;
    for y in ty..ty + th {
        let row = &img[(y * w + tx) * 4..(y * w + tx + tw) * 4];
        for px in row.as_chunks::<4>().0 {
            bgr.extend_from_slice(&px[..3]);
        }
        bgr.resize(bgr.len() + stride - tw * 3, 0);
    }
    let mut data = None;
    for q in [0.78, 0.55, 0.3] {
        let j = jpeg.encode(bgr, tw as u32, th as u32, stride as u32, q)?;
        if j.len() + 13 <= MAX_MSG {
            data = Some(j);
            break;
        }
    }
    let j = data?;
    let mut m = Vec::with_capacity(13 + j.len());
    m.push(1);
    m.extend_from_slice(&frame.to_le_bytes());
    for v in [tx, ty, tw, th] {
        m.extend_from_slice(&(v as u16).to_le_bytes());
    }
    m.extend_from_slice(&j);
    Some(m)
}

fn cursor_msg(c: (i16, i16, bool)) -> Vec<u8> {
    let mut m = vec![3];
    m.extend_from_slice(&c.0.to_le_bytes());
    m.extend_from_slice(&c.1.to_le_bytes());
    m.push(c.2 as u8);
    m
}

fn frame_end(frame: u32, w: usize, h: usize, c: (i16, i16, bool)) -> Vec<u8> {
    let mut m = vec![2];
    m.extend_from_slice(&frame.to_le_bytes());
    m.extend_from_slice(&(w as u16).to_le_bytes());
    m.extend_from_slice(&(h as u16).to_le_bytes());
    m.extend_from_slice(&c.0.to_le_bytes());
    m.extend_from_slice(&c.1.to_le_bytes());
    m.push(c.2 as u8);
    m
}

/// The capture loop of a session: runs until the session drops `want`.
pub fn run(want: Receiver<Want>, out: SyncSender<Frame>, geom: Arc<Mutex<Geom>>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let Some(jpeg) = Jpeg::new() else { return };
    let mut dxgi = Dxgi::new();
    let mut raw = Vec::new();
    let mut prev: Vec<u8> = Vec::new();
    let mut bgr = Vec::with_capacity(TILE * TILE * 3 + TILE * 4);
    let mut credit = 0u32;
    let mut full = true;
    let mut frame = 0u32;
    let mut last_cursor = (i16::MIN, i16::MIN, false);
    let mut last_gdi = Instant::now() - Duration::from_secs(1);
    let mut retry_dxgi = Instant::now();
    loop {
        // credits from the session (each one: the viewer can take one more frame)
        loop {
            match want.try_recv() {
                Ok(f) => {
                    credit += 1;
                    full |= f;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let g = *geom.lock().unwrap();
        if credit == 0 {
            // nothing may be sent yet: only the pointer, which is light
            let c = cursor(&g);
            if c != last_cursor && g.w > 0 {
                last_cursor = c;
                if out.send(Frame { msgs: vec![cursor_msg(c)], counted: false }).is_err() {
                    return;
                }
            }
            match want.recv_timeout(Duration::from_millis(15)) {
                Ok(f) => {
                    credit += 1;
                    full |= f;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            continue;
        }
        // a new image
        if dxgi.is_none() && retry_dxgi.elapsed() > Duration::from_secs(3) {
            dxgi = Dxgi::new();
            retry_dxgi = Instant::now();
            full = true;
        }
        let grab_start = Instant::now();
        let (w, h, fresh) = match dxgi.as_mut() {
            Some(d) => match d.grab(&mut raw, 40) {
                Ok(fresh) => {
                    let (w, h) = d.size();
                    (w, h, fresh)
                }
                Err(()) => {
                    dxgi = None;
                    retry_dxgi = Instant::now();
                    full = true;
                    continue;
                }
            },
            None => {
                // GDI: no change notice, so at most 15 images a second
                let since = last_gdi.elapsed();
                if since < Duration::from_millis(66) {
                    std::thread::sleep(Duration::from_millis(66) - since);
                }
                last_gdi = Instant::now();
                match gdi_grab(&mut raw) {
                    Some((w, h)) => (w, h, true),
                    None => {
                        std::thread::sleep(Duration::from_millis(200));
                        continue;
                    }
                }
            }
        };
        let scale = if w > 2560 { 2 } else { 1 };
        let (iw, ih) = (w / scale, h / scale);
        if !fresh && full && prev.len() == iw * ih * 4 {
            // nothing new, but the whole screen is wanted (the viewer asked again): the last image, whole
            let g = Geom { left: g.left, top: g.top, scale: scale as i32, w: iw as i32, h: ih as i32 };
            let c = cursor(&g);
            frame = frame.wrapping_add(1);
            let mut msgs: Vec<Vec<u8>> =
                changed_tiles(&prev, &prev, iw, ih, true).into_iter().filter_map(|t| tile_msg(&jpeg, &prev, iw, frame, t, &mut bgr)).collect();
            msgs.push(frame_end(frame, iw, ih, c));
            credit -= 1;
            full = false;
            if out.send(Frame { msgs, counted: true }).is_err() {
                return;
            }
            continue;
        }
        if !fresh || raw.len() != w * h * 4 || w == 0 {
            let c = cursor(&g);
            if c != last_cursor && g.w > 0 {
                last_cursor = c;
                if out.send(Frame { msgs: vec![cursor_msg(c)], counted: false }).is_err() {
                    return;
                }
            }
            continue;
        }
        stats::add(&stats::GRAB_US, grab_start.elapsed().as_micros() as u64);
        // very large screens are sent at half size
        let img = if scale == 2 { halve(&raw, w, h) } else { std::mem::take(&mut raw) };
        let origin = dxgi.as_ref().map(|d| (d.rect.left, d.rect.top)).unwrap_or((0, 0));
        let g = Geom { left: origin.0, top: origin.1, scale: scale as i32, w: iw as i32, h: ih as i32 };
        *geom.lock().unwrap() = g;
        let tiles = changed_tiles(&img, &prev, iw, ih, full);
        let c = cursor(&g);
        if tiles.is_empty() {
            if scale == 1 {
                raw = img;
            }
            if c != last_cursor {
                last_cursor = c;
                if out.send(Frame { msgs: vec![cursor_msg(c)], counted: false }).is_err() {
                    return;
                }
            }
            continue;
        }
        frame = frame.wrapping_add(1);
        let enc_start = Instant::now();
        stats::add(&stats::TILES, tiles.len() as u64);
        let mut msgs: Vec<Vec<u8>> = tiles.into_iter().filter_map(|t| tile_msg(&jpeg, &img, iw, frame, t, &mut bgr)).collect();
        stats::add(&stats::ENCODE_US, enc_start.elapsed().as_micros() as u64);
        stats::add(&stats::FRAMES, 1);
        stats::add(&stats::BYTES, msgs.iter().map(|m| m.len() as u64).sum());
        last_cursor = c;
        msgs.push(frame_end(frame, iw, ih, c));
        credit -= 1;
        full = false;
        // the image sent becomes the reference; the buffer of the one before is reused
        let old = std::mem::replace(&mut prev, img);
        if scale == 1 {
            raw = old;
        }
        if out.send(Frame { msgs, counted: true }).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_that_changed() {
        let (w, h) = (600, 300);
        let a = vec![0u8; w * h * 4];
        let mut b = a.clone();
        assert_eq!(changed_tiles(&b, &a, w, h, false).len(), 0);
        assert_eq!(changed_tiles(&b, &a, w, h, true).len(), 6);
        b[(280 * w + 590) * 4] = 9; // bottom right tile
        assert_eq!(changed_tiles(&b, &a, w, h, false), vec![(512, 256, 88, 44)]);
    }

    #[test]
    fn jpeg_tile() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let jpeg = Jpeg::new().unwrap();
        let (w, h) = (300, 200);
        let img: Vec<u8> = (0..w * h * 4).map(|i| (i % 251) as u8).collect();
        let m = tile_msg(&jpeg, &img, w, 7, (256, 0, 44, 200), &mut Vec::new()).unwrap();
        assert_eq!(m[0], 1);
        assert_eq!(u32::from_le_bytes(m[1..5].try_into().unwrap()), 7);
        assert_eq!(&m[13..15], &[0xff, 0xd8]); // JPEG start
        assert!(m.len() <= MAX_MSG);
    }

    #[test]
    fn half_size() {
        let src = vec![100u8; 8 * 4 * 4];
        let out = halve(&src, 8, 4);
        assert_eq!(out.len(), 4 * 2 * 4);
        assert_eq!(out[0], 100);
    }
}
