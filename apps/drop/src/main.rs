// Plume Drop — send files, photos, text and links between this PC and a phone on the same Wi-Fi. The phone scans a
// QR code and gets a page served by this PC: no account, nothing to install, nothing goes through the internet.
//
//   plume-drop.exe [files…]     the window (and offers the files given to the phones)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod http;
mod hub;
mod i18n;
mod net;
mod settings;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;
mod util;

use plume_ui::{Color, Gfx, Theme, window};
use std::rc::Rc;
use windows::{
    Win32::{
        Foundation::LRESULT,
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            Ole::OleInitialize,
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Shell::{DragAcceptFiles, DragFinish, HDROP},
            WindowsAndMessaging::WM_DROPFILES,
        },
    },
    core::w,
};

/// Marks the window: a second launch finds it, hands it its files and brings it forward.
const MARK: &str = "PlumeDropWindow";

fn system_dark() -> bool {
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

/// The theme chosen in Plume, otherwise Windows'; the accent is Plume Drop's green.
fn theme() -> Theme {
    let cfg = std::env::var_os("LOCALAPPDATA")
        .and_then(|b| std::fs::read_to_string(std::path::Path::new(&b).join("Plume").join("plume.json")).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim_start_matches('\u{feff}')).ok())
        .map(|v| v["cfg"].clone())
        .unwrap_or_default();
    let dark = match (std::env::var("PLUME_SUITE_THEME").ok().as_deref(), cfg["theme"].as_str()) {
        (Some("dark"), _) | (None, Some("dark")) => true,
        (Some("light"), _) | (None, Some("light")) => false,
        _ => system_dark(),
    };
    Theme::new(dark, Color::parse(if dark { "#22c55e" } else { "#16a34a" }))
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let line = raw.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
    // already running: it takes the files and comes forward
    if std::env::var_os("PLUME_DROP_TEST_PORT").is_none()
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
    let theme = theme();
    let o = window::Options {
        title: "Plume Drop".into(),
        size: (1060.0, 720.0),
        min: (480.0, 520.0),
        dark: theme.dark,
        background: theme.bg,
        activate: std::env::var_os("PLUME_DROP_NOACTIVATE").is_none(),
        placement: None,
    };
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, theme))).expect("window");
    win.mark(MARK);
    // files dropped on the window are offered to the phones
    unsafe { DragAcceptFiles(win.hwnd, true) };
    win.set_hook(Some(Rc::new(move |msg, wp, _lp| {
        if msg != WM_DROPFILES {
            return None;
        }
        let drop = HDROP(wp.0 as _);
        let paths = ui::dropped_files(drop);
        unsafe { DragFinish(drop) };
        win.with(|h, cx| {
            if let Some(app) = ui::with_app(h) {
                app.share(paths, cx);
            }
        });
        Some(LRESULT(0))
    })));
    if !raw.is_empty() {
        let paths = raw.iter().map(std::path::PathBuf::from).collect();
        win.with(|h, cx| {
            if let Some(app) = ui::with_app(h) {
                app.share(paths, cx);
            }
        });
    }
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}
