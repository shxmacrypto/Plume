// Plume Clean — see what fills your drives and free space safely: a fast scan with a map of the space, and
// cleanup suggestions explained in plain words, never ticked for you, sent to the Recycle Bin.
//
//   plume-clean.exe              the window
//   plume-clean.exe --cleanup    the window, on the Cleanup page
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clean;
mod drives;
mod dupes;
mod i18n;
mod scan;
mod settings;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;

use plume_ui::{Gfx, window};
use std::rc::Rc;
use windows::{
    Win32::{
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            Ole::OleInitialize,
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        },
        UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
    },
    core::w,
};

pub const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-clean-256.png");
const MARK: &str = "PlumeCleanWindow";

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let line = args.join(" ");
    // already open: it comes forward
    if std::env::var_os("PLUME_CLEAN_TEST_PORT").is_none()
        && let Some(w) = window::find(MARK)
    {
        w.send_text(&line);
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let theme = settings::Settings::load().theme;
    let dark = ui::dark_for(&theme);
    let o = window::Options {
        title: "Plume Clean".into(),
        size: (1180.0, 780.0),
        min: (880.0, 600.0),
        dark,
        background: if dark { plume_ui::Color::hex(0x121214) } else { plume_ui::Color::hex(0xf6f6f8) },
        activate: std::env::var_os("PLUME_CLEAN_NOACTIVATE").is_none(),
        placement: None,
    };
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g))).expect("window");
    win.mark(MARK);
    if line.contains("--cleanup") {
        win.with(|h, cx| {
            if let Some(app) = ui::with_app(h) {
                app.go(ui::View::Cleanup, cx);
            }
        });
    }
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}
