// Plume Viewer — see and control this PC from a phone or any browser: a QR code, or an ID and a code.
// The relay (apps/viewer/relay, a Cloudflare Worker) only puts devices in touch; the screen, the mouse and the keys
// go directly from device to device, encrypted (WebRTC).
//
//   plume-viewer.exe                 the window
//   plume-viewer.exe --background    no window, only the icon near the clock (start with Windows)
//   plume-viewer.exe --unregister    no more start with Windows, and the running Plume Viewer quits (uninstall)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod crypto;
mod gate;
mod i18n;
mod input;
mod relay;
mod rtc;
mod store;
mod sys;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;

use plume_ui::{Color, Gfx, Theme, window};
use std::{cell::Cell, rc::Rc};
use windows::{
    Win32::{
        Foundation::{HWND, LRESULT},
        System::{
            Ole::OleInitialize,
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            WindowsAndMessaging::{WM_CLOSE, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP},
        },
    },
    core::w,
};

/// Marks the window: a second launch finds it and brings it forward.
const MARK: &str = "PlumeViewerWindow";

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

/// The theme chosen in Plume Web, otherwise Windows'; the accent is Plume Viewer's teal.
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
    Theme::new(dark, Color::parse(if dark { "#14b8a6" } else { "#0d9488" }))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let background = args.iter().any(|a| a == "--background");
    if args.iter().any(|a| a == "--unregister") {
        sys::set_autostart(false);
        if let Some(w) = window::find(MARK) {
            w.send_text("quit");
            // it closes its sessions and its link: wait for it, so its file can be removed
            for _ in 0..50 {
                if window::find(MARK).is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        std::process::exit(0);
    }
    // already running: it comes forward (unless this launch only wanted the background)
    if std::env::var_os("PLUME_VIEWER_TEST_PORT").is_none()
        && let Some(w) = window::find(MARK)
    {
        if !background {
            w.send_text("open");
        }
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let theme = theme();
    let s = i18n::strings(i18n::french());
    let o = window::Options {
        title: "Plume Viewer".into(),
        size: (900.0, 760.0),
        min: (700.0, 640.0),
        dark: theme.dark,
        background: theme.bg,
        activate: !background && std::env::var_os("PLUME_VIEWER_NOACTIVATE").is_none(),
        placement: None,
    };
    let keep = Rc::new(Cell::new(false));
    let k = keep.clone();
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, theme, s, k))).expect("window");
    win.mark(MARK);
    let hwnd = win.hwnd;
    if background {
        sys::show(hwnd, false);
    }
    let tip = win.with(|h, _| h.as_any().downcast_mut::<ui::App>().map(|a| a.tray_tip())).flatten().unwrap_or_default();
    sys::tray_add(hwnd, &tip);

    // the window's close button hides it while the PC must stay reachable; "Quit" in the icon's menu really quits
    let quitting = Rc::new(Cell::new(false));
    let q = quitting.clone();
    let hwnd_v = hwnd.0 as isize;
    win.set_hook(Some(Rc::new(move |msg, _wp, lp| {
        let hwnd = HWND(hwnd_v as _);
        match msg {
            WM_CLOSE if keep.get() && !q.get() => {
                sys::show(hwnd, false);
                Some(LRESULT(0))
            }
            sys::WM_TRAY => {
                match lp.0 as u32 {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => sys::show(hwnd, true),
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        let (open, quit) = (s.tray_open, s.tray_quit);
                        match sys::tray_menu(hwnd, open, quit) {
                            sys::MENU_OPEN => sys::show(hwnd, true),
                            sys::MENU_QUIT => {
                                q.set(true);
                                unsafe {
                                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                        Some(hwnd),
                                        WM_CLOSE,
                                        windows::Win32::Foundation::WPARAM(0),
                                        windows::Win32::Foundation::LPARAM(0),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    })));
    #[cfg(feature = "test-hook")]
    test_hook::start(win, quitting);
    #[cfg(not(feature = "test-hook"))]
    let _ = quitting;
    window::run();
}
