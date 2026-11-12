// Plume Cut — a video editor: import clips, pictures and music, arrange them on a timeline (cut, trim, move,
// split, speed, transitions, texts, colour), watch the result and export it, all on this PC (FFmpeg does the
// decoding and the encoding).
//
//   plume-cut.exe                         the start screen (new project, recent projects)
//   plume-cut.exe project.plumecut        opens a project
//   plume-cut.exe --add <files…>          a new project holding these clips (Plume Media's "Open in Plume Cut")
//   plume-cut.exe <files…>                the same as --add
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod export;
mod ffmpeg;
mod i18n;
mod model;
mod preview;
mod project;
mod render;
mod settings;
#[cfg(feature = "test-hook")]
mod test_hook;
mod tools;
mod ui;

use plume_ui::{Gfx, Pt, window};
use std::{path::PathBuf, rc::Rc};
use windows::{
    Win32::{
        Foundation::{LRESULT, POINT},
        Graphics::Imaging::{
            GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapInterpolationModeHighQualityCubic,
            WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
        },
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            Ole::OleInitialize,
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, DragQueryPoint, HDROP},
            WindowsAndMessaging::WM_DROPFILES,
        },
    },
    core::w,
};

pub const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-cut-256.png");

/// The logo for the title bar and the start screen (64 px).
pub fn logo(wic: &IWICImagingFactory) -> Option<IWICBitmapSource> {
    unsafe {
        let stream = wic.CreateStream().ok()?;
        stream.InitializeFromMemory(LOGO).ok()?;
        let dec = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let s = wic.CreateBitmapScaler().ok()?;
        s.Initialize(&frame, 64, 64, WICBitmapInterpolationModeHighQualityCubic).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&s, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        Some(conv.into())
    }
}

pub fn system_dark() -> bool {
    if let Ok(t) = std::env::var("PLUME_SUITE_THEME") {
        return t != "light";
    }
    let (mut data, mut size) = (1u32, 4u32);
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    read.is_ok() && data == 0
}

/// The paths of files dropped on the window, and where they fell (pixels in the window).
fn dropped(drop: HDROP) -> (Vec<PathBuf>, POINT) {
    unsafe {
        let mut at = POINT::default();
        let _ = DragQueryPoint(drop, &mut at);
        let n = DragQueryFileW(drop, u32::MAX, None);
        let list = (0..n)
            .map(|i| {
                let mut name = vec![0u16; DragQueryFileW(drop, i, None) as usize + 1];
                let len = DragQueryFileW(drop, i, Some(&mut name)) as usize;
                PathBuf::from(String::from_utf16_lossy(&name[..len]))
            })
            .collect();
        DragFinish(drop);
        (list, at)
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // --register-defaults / --unregister-defaults: Windows default app (run by Plume Suite and the installer)
    if let Some(code) = plume_defaults::switch(&plume_defaults::CUT, &raw) {
        std::process::exit(code);
    }
    let line = raw.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let dark = system_dark();
    let o = window::Options {
        title: "Plume Cut".into(),
        size: (1440.0, 900.0),
        min: (1180.0, 720.0),
        dark,
        background: if dark { plume_ui::Color::hex(0x1c1c1e) } else { plume_ui::Color::hex(0xf2f2f7) },
        activate: std::env::var_os("PLUME_CUT_NOACTIVATE").is_none(),
        placement: None,
    };
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, dark))).expect("window");
    // files dropped from Explorer or Plume Media: on the timeline where they fall, elsewhere into the bin
    unsafe { DragAcceptFiles(win.hwnd, true) };
    let sender = win.sender();
    win.set_hook(Some(Rc::new(move |msg, wp, _lp| {
        if msg != WM_DROPFILES {
            return None;
        }
        let (files, at) = dropped(HDROP(wp.0 as _));
        sender.send(move |h, cx| {
            if let Some(app) = ui::with_app(h) {
                let s = cx.scale;
                app.dropped(files, Some(Pt { x: at.x as f32 / s, y: at.y as f32 / s }), cx);
            }
        });
        Some(LRESULT(0))
    })));
    if !raw.is_empty() {
        win.with(|h, cx| {
            if let Some(app) = ui::with_app(h) {
                app.open_args(&line, cx);
            }
        });
    }
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}
