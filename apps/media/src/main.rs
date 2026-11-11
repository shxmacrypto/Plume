// Plume Media — an instant photo viewer and organizer, and a video player, in one light native app.
//
//   plume-media.exe [file]                  the window (and opens the photo or video given)
//   plume-media.exe --register-open-with    offers Plume Media in Windows' "Open with" menu (current user)
//   plume-media.exe --unregister-open-with  removes it
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod exif_info;
mod i18n;
mod image;
mod library;
mod openwith;
mod settings;
mod srt;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;
mod video;

use plume_ui::{Gfx, window};
use std::{path::PathBuf, rc::Rc};
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

const MARK: &str = "PlumeMediaWindow";

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
    let raw: Vec<String> = std::env::args().skip(1).collect();
    match raw.first().map(String::as_str) {
        Some("--register-open-with") => {
            let ok = openwith::register(&std::env::current_exe().unwrap_or_default());
            std::process::exit(if ok { 0 } else { 1 });
        }
        Some("--unregister-open-with") => {
            openwith::unregister();
            return;
        }
        _ => {}
    }
    let file = raw.iter().map(PathBuf::from).find(|p| p.is_file());
    let testing = std::env::var_os("PLUME_MEDIA_TEST_PORT").is_some();
    // already open: it takes the file and comes forward
    if !testing && let Some(w) = window::find(MARK) {
        let line = raw.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
        w.send_text(&line);
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    // a photo starts decoding while the window is being made: it shows at once
    let early = file.clone().filter(|p| image::is_photo(p)).map(|p| {
        let max = early_size();
        std::thread::spawn(move || {
            ui::com_mta();
            image::decode(&p, max)
        })
    });
    let set = settings::Settings::load();
    i18n::set(&set.lang);
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let dark = ui::theme_dark(&set);
    let o = window::Options {
        title: "Plume Media".into(),
        size: (1240.0, 800.0),
        min: (720.0, 480.0),
        dark,
        background: if file.is_some() {
            plume_ui::Color::hex(0x08090a)
        } else if dark {
            plume_ui::Color::hex(0x121315)
        } else {
            plume_ui::Color::hex(0xf6f7f6)
        },
        activate: std::env::var_os("PLUME_MEDIA_NOACTIVATE").is_none(),
        placement: None,
    };
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, set))).expect("window");
    win.mark(MARK);
    win.with(|h, cx| {
        if let Some(app) = ui::with_app(h) {
            app.scale = app.win.scale();
            match file {
                Some(f) => app.open_file(f, early, cx),
                None => app.rescan(),
            }
            cx.timer(ui::TICK, Some(200));
        }
    });
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}

/// Long side of the primary screen, in pixels (the window is not there yet).
fn early_size() -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    (w.max(h).max(0) as u32).clamp(1280, 7680)
}
