// Plume Passwords — a password vault: logins, 2FA codes, secure notes and cards, in one encrypted file on this PC.
// Argon2id and AES-256-GCM, Windows Hello, auto-type, a generator, a security report, CSV import. Nothing online.
//
//   plume-passwords.exe                 the window
//   plume-passwords.exe --background    no window, only the icon near the clock (start with Windows)
//   plume-passwords.exe --unregister    no more start with Windows, and the running Plume Passwords quits (uninstall)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autotype;
mod clip;
mod csv;
mod generator;
mod hello;
mod i18n;
mod qr;
mod settings;
mod sys;
#[cfg(feature = "test-hook")]
mod test_hook;
mod totp;
mod ui;
mod vault;

use plume_ui::{Color, Gfx, Theme, window};
use settings::Settings;
use std::{cell::Cell, rc::Rc};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            Ole::OleInitialize,
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            WindowsAndMessaging::{PostMessageW, WM_CLOSE, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP},
        },
    },
    core::w,
};

/// Marks the window: a second launch finds it and brings it forward.
const MARK: &str = "PlumePasswordsWindow";

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

/// The theme: this app's setting, else the suite's (PLUME_SUITE_THEME, Plume Web's), else Windows'. The accent is
/// Plume Passwords' slate, the colour of its icon (a deeper slate on light backgrounds, to stay readable).
pub fn theme(set: &Settings) -> Theme {
    let suite = std::env::var("PLUME_SUITE_THEME").ok().or_else(|| {
        std::env::var_os("LOCALAPPDATA")
            .and_then(|b| std::fs::read_to_string(std::path::Path::new(&b).join("Plume").join("plume.json")).ok())
            .and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim_start_matches('\u{feff}')).ok())
            .and_then(|v| v["cfg"]["theme"].as_str().map(String::from))
    });
    let dark = match (set.theme.as_str(), suite.as_deref()) {
        ("dark", _) => true,
        ("light", _) => false,
        (_, Some("dark")) => true,
        (_, Some("light")) => false,
        _ => system_dark(),
    };
    Theme::new(dark, Some(if dark { Color::hex(0x94a3b8) } else { Color::hex(0x64748b) }))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let background = args.iter().any(|a| a == "--background");
    if args.iter().any(|a| a == "--unregister") {
        sys::set_autostart(false);
        if let Some(w) = window::find(MARK) {
            w.send_text("quit");
        }
        std::process::exit(0);
    }
    // already running: it comes forward (unless this launch only wanted the background)
    if std::env::var_os("PLUME_PASSWORDS_TEST_PORT").is_none()
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
    let set = Settings::load();
    i18n::set(&set.lang);
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let t = theme(&set);
    let o = window::Options {
        title: "Plume Passwords".into(),
        size: (1180.0, 760.0),
        min: (900.0, 600.0),
        dark: t.dark,
        background: t.bg,
        activate: !background && std::env::var_os("PLUME_PASSWORDS_NOACTIVATE").is_none(),
        placement: None,
    };
    let keep = Rc::new(Cell::new(false));
    let k = keep.clone();
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, t, set, k))).expect("window");
    win.mark(MARK);
    let hwnd = win.hwnd;
    if background {
        sys::show(hwnd, false);
    }
    win.with(|h, _| {
        if let Some(app) = ui::with_app(h) {
            app.apply_hotkey();
        }
    });
    sys::watch_session(hwnd, true);
    sys::tray_add(hwnd, "Plume Passwords");

    // the window's close button hides it while it should keep running; "Quit" in the icon's menu really quits
    let quitting = Rc::new(Cell::new(false));
    let q = quitting.clone();
    let hwnd_v = hwnd.0 as isize;
    let sender = win.sender();
    win.set_hook(Some(Rc::new(move |msg, wp, lp| {
        let hwnd = HWND(hwnd_v as _);
        match msg {
            WM_CLOSE if keep.get() && !q.get() => {
                sys::show(hwnd, false);
                Some(LRESULT(0))
            }
            sys::WM_HOTKEY_MSG => {
                // what is in front now, before anything moves
                let (fg, title) = autotype::foreground();
                let fg = fg.0 as isize;
                sender.send(move |h, cx| {
                    if let Some(app) = ui::with_app(h) {
                        app.on_hotkey(fg, title, cx);
                    }
                });
                Some(LRESULT(0))
            }
            sys::WM_SESSION_CHANGE if wp.0 == sys::WTS_SESSION_LOCK => {
                sender.send(|h, cx| {
                    if let Some(app) = ui::with_app(h)
                        && app.set.lock_on_windows_lock
                    {
                        app.lock("windows", cx);
                    }
                });
                None
            }
            sys::WM_TRAY => {
                match lp.0 as u32 {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => sys::show(hwnd, true),
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        let unlocked = std::cell::Cell::new(false);
                        // the menu needs to know whether there is something to lock
                        let w = window::Window { hwnd };
                        w.with(|h, _| {
                            if let Some(app) = ui::with_app(h) {
                                unlocked.set(app.vault.unlocked());
                            }
                        });
                        let lock = unlocked.get().then(|| i18n::tr("Lock"));
                        match sys::tray_menu(hwnd, i18n::tr("Open Plume Passwords"), lock, i18n::tr("Quit")) {
                            sys::MENU_OPEN => sys::show(hwnd, true),
                            sys::MENU_LOCK => {
                                w.with(|h, cx| {
                                    if let Some(app) = ui::with_app(h) {
                                        app.lock("tray", cx);
                                    }
                                });
                            }
                            sys::MENU_QUIT => {
                                q.set(true);
                                unsafe {
                                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
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
