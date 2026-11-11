//! Result icons: the ones Windows shows (apps, files, folders, the Settings app), asked to the shell on a worker
//! thread and handed to the interface as pixels, kept for the rest of the session. The interface never waits:
//! a result shows a plain icon of its kind until its own arrives.

use std::{
    collections::VecDeque,
    sync::{Mutex, mpsc},
};
use windows::{
    Win32::{
        Foundation::SIZE,
        Graphics::Gdi::{BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HGDIOBJ, ReleaseDC},
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY},
    },
    core::HSTRING,
};

/// The Settings app, whose icon the settings pages show.
pub const SETTINGS_APP: &str = "shell:AppsFolder\\windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel";

/// An icon's pixels: premultiplied BGRA, top row first.
#[derive(Debug)]
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

pub type Deliver = Box<dyn Fn(String, u32, Option<Pixels>) + Send>;

pub struct Worker {
    tx: Mutex<mpsc::Sender<(String, u32, bool)>>,
}

impl Worker {
    /// Asks for the icon of `name` (a path or a shell parsing name) at `px` pixels. `urgent`: the box shows it
    /// now, it goes before the icons fetched ahead of time.
    pub fn ask(&self, name: &str, px: u32, urgent: bool) {
        let _ = self.tx.lock().unwrap().send((name.to_string(), px, urgent));
    }
}

pub fn start(deliver: Deliver) -> Worker {
    let (tx, rx) = mpsc::channel::<(String, u32, bool)>();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let mut urgent = VecDeque::new();
        let mut later = VecDeque::new();
        loop {
            if urgent.is_empty() && later.is_empty() {
                match rx.recv() {
                    Ok(m) => sort(m, &mut urgent, &mut later),
                    Err(_) => return,
                }
            }
            for m in rx.try_iter() {
                sort(m, &mut urgent, &mut later);
            }
            let Some((name, px)) = urgent.pop_front().or_else(|| later.pop_front()) else { continue };
            let pixels = load(&name, px);
            deliver(name, px, pixels);
        }
    });
    Worker { tx: Mutex::new(tx) }
}

fn sort(m: (String, u32, bool), urgent: &mut VecDeque<(String, u32)>, later: &mut VecDeque<(String, u32)>) {
    if m.2 { urgent.push_back((m.0, m.1)) } else { later.push_back((m.0, m.1)) }
}

fn load(name: &str, px: u32) -> Option<Pixels> {
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(name), None).ok()?;
        let bmp = factory.GetImage(SIZE { cx: px as i32, cy: px as i32 }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        let mut info = BITMAP::default();
        let got = GetObjectW(HGDIOBJ(bmp.0), size_of::<BITMAP>() as i32, Some(&mut info as *mut BITMAP as _));
        let (w, h) = (info.bmWidth, info.bmHeight.abs());
        let mut out = None;
        if got != 0 && w > 0 && h > 0 && w <= 512 && h <= 512 {
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    // negative: top row first
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut data = vec![0u8; (w * h * 4) as usize];
            let dc = GetDC(None);
            let lines = GetDIBits(dc, bmp, 0, h as u32, Some(data.as_mut_ptr() as _), &mut bi, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            if lines == h {
                fix_alpha(&mut data);
                out = Some(Pixels { w: w as u32, h: h as u32, data });
            }
        }
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        out
    }
}

/// An icon without any transparency information is fully opaque.
fn fix_alpha(data: &mut [u8]) {
    if data.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        for p in data.as_chunks_mut::<4>().0 {
            p[3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_without_alpha() {
        let mut d = vec![10, 20, 30, 0, 40, 50, 60, 0];
        fix_alpha(&mut d);
        assert_eq!(d, [10, 20, 30, 255, 40, 50, 60, 255]);
        let mut d = vec![10, 20, 30, 128, 0, 0, 0, 0];
        fix_alpha(&mut d);
        assert_eq!(d, [10, 20, 30, 128, 0, 0, 0, 0]);
    }
}
