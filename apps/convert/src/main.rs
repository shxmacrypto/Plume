// Plume Convert — drop a file, get it in the format you need: videos and sounds through FFmpeg (downloaded the first
// time it is needed), pictures through Windows' own imaging. Everything happens on this PC.
//
//   plume-convert.exe [files…]     the window, with the files in the queue
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ffmpeg;
mod i18n;
mod image;
mod naming;
mod opts;
mod queue;
mod settings;
mod shell;
#[cfg(feature = "test-hook")]
mod test_hook;
mod tools;
mod ui;

use plume_ui::{Gfx, window};
use std::{path::PathBuf, rc::Rc};
use windows::{
    Win32::{
        Foundation::LRESULT,
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            Ole::OleInitialize,
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
            Threading::CreateMutexW,
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP},
            WindowsAndMessaging::WM_DROPFILES,
        },
    },
    core::w,
};

pub const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-convert-256.png");
const MARK: &str = "PlumeConvertWindow";

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

/// The paths of files dropped on the window.
fn dropped(drop: HDROP) -> Vec<PathBuf> {
    unsafe {
        let n = DragQueryFileW(drop, u32::MAX, None);
        let list = (0..n)
            .map(|i| {
                let mut name = vec![0u16; DragQueryFileW(drop, i, None) as usize + 1];
                let len = DragQueryFileW(drop, i, Some(&mut name)) as usize;
                PathBuf::from(String::from_utf16_lossy(&name[..len]))
            })
            .collect();
        DragFinish(drop);
        list
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let line = raw.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
    let testing = std::env::var_os("PLUME_CONVERT_TEST_PORT").is_some();
    if !testing {
        // Explorer starts one copy per selected file: the first opens the window, the others hand it their file
        let first = unsafe { CreateMutexW(None, true, w!("PlumeConvertStarting")) }.ok();
        let already = unsafe { windows::Win32::Foundation::GetLastError() } == windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
        let tries = if already { 30 } else { 1 };
        for _ in 0..tries {
            if let Some(w) = window::find(MARK) {
                w.send_text(&line);
                return;
            }
            if already {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        // the mutex stays held until this copy has its window
        let _ = first;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let dark = system_dark();
    let o = window::Options {
        title: "Plume Convert".into(),
        size: (1120.0, 760.0),
        min: (900.0, 620.0),
        dark,
        background: if dark { plume_ui::Color::hex(0x141416) } else { plume_ui::Color::hex(0xf6f6f8) },
        activate: std::env::var_os("PLUME_CONVERT_NOACTIVATE").is_none(),
        placement: None,
    };
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, dark))).expect("window");
    win.mark(MARK);
    // files dropped on the window join the queue
    unsafe { DragAcceptFiles(win.hwnd, true) };
    let sender = win.sender();
    win.set_hook(Some(Rc::new(move |msg, wp, _lp| {
        if msg != WM_DROPFILES {
            return None;
        }
        let files = dropped(HDROP(wp.0 as _));
        sender.send(move |h, cx| {
            if let Some(app) = ui::with_app(h) {
                app.add_paths(files, cx);
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
