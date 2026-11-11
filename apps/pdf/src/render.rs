//! Pages drawn by Windows' own PDF engine (Windows.Data.Pdf), on a thread of their own: the file is given as
//! bytes (it stays free on disk), each page is drawn at the size asked for, decoded by WIC and handed back as
//! pixels ready for Direct2D. The newest requests go first (what was just scrolled into view).

use std::sync::{Arc, Condvar, Mutex};
use windows::{
    Data::Pdf::{PdfDocument, PdfPageRenderOptions},
    Storage::Streams::{IRandomAccessStream, InMemoryRandomAccessStream},
    Win32::{
        Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut,
            WICDecodeMetadataCacheOnDemand,
        },
        System::{
            Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, IStream},
            WinRT::{BSOS_DEFAULT, CreateRandomAccessStreamOverStream, CreateStreamOverRandomAccessStream},
        },
        UI::Shell::SHCreateMemStream,
    },
    core::{GUID, HSTRING},
};

/// Decoded page: premultiplied BGRA, top row first.
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoadFail {
    /// The file is protected and the password is missing or wrong.
    Password,
    Other(String),
}

/// What the render thread reports.
pub enum Reply {
    /// `sizes`: each page's displayed size in points.
    Loaded {
        ver: u64,
        result: Result<Vec<(f32, f32)>, LoadFail>,
    },
    Page {
        job: Job,
        px: Pixels,
    },
    Failed {
        job: Job,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Job {
    pub ver: u64,
    pub page: usize,
    /// Size in pixels.
    pub w: u32,
    pub h: u32,
    pub dark: bool,
}

#[derive(Default)]
struct Queue {
    load: Option<(u64, Arc<Vec<u8>>, Option<String>)>,
    jobs: Vec<Job>,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

pub struct Renderer {
    shared: Shared,
}

impl Renderer {
    pub fn start(reply: impl Fn(Reply) + Send + 'static) -> Renderer {
        let shared: Shared = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let s = shared.clone();
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let Ok(wic) = (unsafe { CoCreateInstance::<_, IWICImagingFactory>(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }) else { return };
            let mut doc: Option<(u64, PdfDocument)> = None;
            let mut bmp = true;
            loop {
                let (load, job) = {
                    let (m, cv) = &*s;
                    let mut q = m.lock().unwrap();
                    while q.load.is_none() && q.jobs.is_empty() {
                        q = cv.wait(q).unwrap();
                    }
                    match q.load.take() {
                        Some(l) => (Some(l), None),
                        None => (None, q.jobs.pop()),
                    }
                };
                if let Some((ver, bytes, pw)) = load {
                    let r = open(&bytes, pw.as_deref());
                    let result = r.as_ref().map(sizes).map_err(Clone::clone);
                    doc = r.ok().map(|d| (ver, d));
                    reply(Reply::Loaded { ver, result });
                    continue;
                }
                let Some(job) = job else { continue };
                let px = match &doc {
                    Some((g, d)) if *g == job.ver => draw(d, &wic, job.page, job.w, job.h, job.dark, &mut bmp),
                    _ => None,
                };
                match px {
                    Some(px) => reply(Reply::Page { job, px }),
                    None => reply(Reply::Failed { job }),
                }
            }
        });
        Renderer { shared }
    }

    /// A new document (or a new version of it): earlier requests are dropped.
    pub fn load(&self, ver: u64, bytes: Arc<Vec<u8>>, password: Option<String>) {
        let (m, cv) = &*self.shared;
        let mut q = m.lock().unwrap();
        q.jobs.clear();
        q.load = Some((ver, bytes, password));
        cv.notify_one();
    }

    /// Asks for a page; asked again, it moves to the front.
    pub fn request(&self, job: Job) {
        let (m, cv) = &*self.shared;
        let mut q = m.lock().unwrap();
        q.jobs.retain(|j| *j != job);
        q.jobs.push(job);
        cv.notify_one();
    }

    /// Forgets the requests that are no longer wanted (pages scrolled away).
    pub fn keep_only(&self, keep: impl Fn(&Job) -> bool) {
        let (m, _) = &*self.shared;
        m.lock().unwrap().jobs.retain(keep);
    }

    pub fn pending(&self) -> usize {
        let (m, _) = &*self.shared;
        let q = m.lock().unwrap();
        q.jobs.len() + usize::from(q.load.is_some())
    }
}

/// HRESULT of a wrong or missing password (ERROR_WRONG_PASSWORD).
const WRONG_PASSWORD: i32 = 0x8007_052B_u32 as i32;

/// Opens a PDF from memory with Windows' engine.
pub fn open(bytes: &[u8], password: Option<&str>) -> Result<PdfDocument, LoadFail> {
    let fail = |e: windows::core::Error| if e.code().0 == WRONG_PASSWORD { LoadFail::Password } else { LoadFail::Other(e.message().to_string()) };
    unsafe {
        let mem: IStream = SHCreateMemStream(Some(bytes)).ok_or_else(|| LoadFail::Other("memory".into()))?;
        let ras: IRandomAccessStream = CreateRandomAccessStreamOverStream(&mem, BSOS_DEFAULT).map_err(fail)?;
        let op = match password {
            Some(pw) => PdfDocument::LoadFromStreamWithPasswordAsync(&ras, &HSTRING::from(pw)),
            None => PdfDocument::LoadFromStreamAsync(&ras),
        }
        .map_err(fail)?;
        op.get().map_err(fail)
    }
}

/// Each page's displayed size in points (Windows gives DIPs: 96 per inch).
pub fn sizes(doc: &PdfDocument) -> Vec<(f32, f32)> {
    let n = doc.PageCount().unwrap_or(0);
    (0..n).map(|i| doc.GetPage(i).and_then(|p| p.Size()).map(|s| (s.Width * 0.75, s.Height * 0.75)).unwrap_or((612.0, 792.0))).collect()
}

/// Windows' BMP encoder: no compression to undo (PNG is the default).
const BMP_ENCODER: GUID = GUID::from_u128(0x69be8bb4_d66d_47c8_865a_ed1589433782);

/// Draws one page at `w` × `h` pixels. `dark`: colors turned around (dark pages).
pub fn draw(doc: &PdfDocument, wic: &IWICImagingFactory, index: usize, w: u32, h: u32, dark: bool, bmp: &mut bool) -> Option<Pixels> {
    let page = doc.GetPage(index as u32).ok()?;
    let render = |use_bmp: bool| -> windows::core::Result<InMemoryRandomAccessStream> {
        let out = InMemoryRandomAccessStream::new()?;
        let opts = PdfPageRenderOptions::new()?;
        opts.SetDestinationWidth(w.max(1))?;
        opts.SetDestinationHeight(h.max(1))?;
        if use_bmp {
            opts.SetBitmapEncoderId(BMP_ENCODER)?;
        }
        page.RenderWithOptionsToStreamAsync(&out, &opts)?.get()?;
        Ok(out)
    };
    let out = match render(*bmp) {
        Ok(o) => o,
        Err(_) if *bmp => {
            *bmp = false;
            render(false).ok()?
        }
        Err(_) => return None,
    };
    let mut px = unsafe {
        let stream: IStream = CreateStreamOverRandomAccessStream(&out).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
        let (mut pw, mut ph) = (0u32, 0u32);
        conv.GetSize(&mut pw, &mut ph).ok()?;
        let mut data = vec![0u8; (pw * ph * 4) as usize];
        conv.CopyPixels(std::ptr::null(), pw * 4, &mut data).ok()?;
        Pixels { w: pw, h: ph, data }
    };
    if dark {
        invert(&mut px.data);
    }
    Some(px)
}

/// Dark pages: white paper becomes a soft dark grey, black ink a soft light grey.
pub fn invert(data: &mut [u8]) {
    for p in data.as_chunks_mut::<4>().0 {
        let a = p[3] as u32;
        for c in &mut p[..3] {
            // premultiplied: work on the full-opacity value, then back
            let v = (*c as u32 * 255).checked_div(a).map_or(0, |v| v.min(255));
            let inv = 30 + (255 - v) * 196 / 255;
            *c = (inv * a / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::invert;

    #[test]
    fn dark_pages_keep_contrast_without_pure_black_or_white() {
        let mut px = vec![255, 255, 255, 255, 0, 0, 0, 255, 0, 0, 0, 0];
        invert(&mut px);
        assert_eq!(&px[..4], &[30, 30, 30, 255]);
        assert_eq!(&px[4..8], &[226, 226, 226, 255]);
        assert_eq!(&px[8..], &[0, 0, 0, 0]);
    }
}
