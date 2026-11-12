// Plume Launcher — a shortcut (Alt+Space) brings up a search box over everything: apps, files and folders,
// Windows settings, a calculator, and the web. Recent picks come first.
//
//   plume-launcher.exe                 starts (if needed) and opens the box
//   plume-launcher.exe --background    starts with no box, only the icon near the clock (start with Windows)
//   plume-launcher.exe --settings      opens the settings window
//   plume-launcher.exe --unregister    no more start with Windows, and the running Plume Launcher quits (uninstall)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod calc;
mod files;
mod i18n;
mod icons;
mod pages;
mod rank;
mod settings;
mod settings_ui;
mod sys;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;

use plume_ui::{Gfx, window};
use std::{cell::Cell, cell::RefCell, rc::Rc};
use windows::Win32::{
    Foundation::{HWND, LRESULT},
    System::Ole::OleInitialize,
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
        WindowsAndMessaging::{WM_CLOSE, WM_CONTEXTMENU, WM_HOTKEY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP},
    },
};

/// Marks the box's window: a second launch finds it and hands it its command line.
const MARK: &str = "PlumeLauncherWindow";

fn test_port() -> bool {
    std::env::var_os("PLUME_LAUNCHER_TEST_PORT").is_some()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let has = |a: &str| args.iter().any(|x| x == a);
    let background = has("--background");
    let command = if has("--settings") {
        "settings"
    } else if background {
        "background"
    } else {
        "show"
    };
    if has("--unregister") {
        sys::set_autostart(false);
        if let Some(w) = window::find(MARK) {
            w.send_text("quit");
            for _ in 0..50 {
                if window::find(MARK).is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        return;
    }
    // already running: it does what was asked (the box, the settings)
    if !test_port()
        && let Some(w) = window::find(MARK)
    {
        w.send_text(command);
        return;
    }
    #[cfg(feature = "test-hook")]
    if test_port() {
        sys::set_test_mode();
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
    }
    let (set, existed) = settings::Settings::load();
    i18n::set(&set.lang);
    // the first start turns on the start with Windows: a launcher that is not running does nothing
    if !existed {
        set.save();
        if !sys::test_mode() {
            sys::set_autostart(true);
        }
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let theme = ui::theme_for(&set);
    let data = settings::data_dir();

    // the file index answers on its own thread; its answers reach the box through its window
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None::<plume_ui::Sender>));
    let s2 = slot.clone();
    let files = files::start(
        data,
        Box::new(move |generation, hits| {
            if let Some(sender) = *s2.lock().unwrap() {
                sender.send(move |h, cx| {
                    if let Some(app) = ui::with_app(h) {
                        app.on_files(generation, hits, cx);
                    }
                });
            }
        }),
    );
    let shared = Rc::new(ui::Shared {
        set: RefCell::new(set),
        history: RefCell::new(ui::load_history()),
        files,
        hotkey_ok: Cell::new(true),
        popup: Cell::new(None),
        settings_win: Cell::new(None),
    });
    let o = window::Options {
        title: "Plume Launcher".into(),
        size: (ui::W, 94.0),
        min: (200.0, 60.0),
        dark: theme.dark,
        background: ui::surface(&theme),
        activate: false,
        placement: None,
    };
    let (g, sh) = (gfx.clone(), shared.clone());
    let win = window::create(&gfx, o, move |win| {
        sys::make_popup(win.hwnd);
        Box::new(ui::App::new(win, g, sh))
    })
    .expect("window");
    *slot.lock().unwrap() = Some(win.sender());
    shared.popup.set(Some(win));
    win.mark(MARK);
    let hwnd = win.hwnd;
    sys::hide(hwnd);
    sys::warm_up(hwnd);
    sys::tray_add(hwnd, "Plume Launcher");

    // the shortcut (tests: only PLUME_LAUNCHER_HOTKEY, never the real one)
    let wanted = if sys::test_mode() { std::env::var("PLUME_LAUNCHER_HOTKEY").ok() } else { Some(shared.set.borrow().hotkey.clone()) };
    if let Some(hk) = wanted.as_deref().and_then(sys::Hotkey::parse) {
        shared.hotkey_ok.set(sys::register_hotkey(hwnd, &hk));
    }

    let hwnd_v = hwnd.0 as isize;
    let quitting = Rc::new(Cell::new(false));
    let q = quitting.clone();
    win.set_hook(Some(Rc::new(move |msg, wp, lp| {
        let hwnd = HWND(hwnd_v as _);
        let w = plume_ui::Window { hwnd };
        match msg {
            WM_HOTKEY if wp.0 as i32 == sys::HOTKEY_ID => {
                w.with(|h, cx| {
                    if let Some(app) = ui::with_app(h) {
                        app.toggle(cx);
                    }
                });
                Some(LRESULT(0))
            }
            // closing the box (Alt+F4) only hides it; "Quit" in the icon's menu really quits
            WM_CLOSE if !q.get() => {
                let quitting = w
                    .with(|h, cx| {
                        ui::with_app(h).map(|app| {
                            app.quitting || {
                                sys::test_note("hide: WM_CLOSE");
                                app.hide(cx);
                                false
                            }
                        })
                    })
                    .flatten()
                    .unwrap_or(false);
                if quitting {
                    q.set(true);
                    None
                } else {
                    Some(LRESULT(0))
                }
            }
            sys::WM_TRAY => {
                match lp.0 as u32 {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                        w.with(|h, cx| {
                            if let Some(app) = ui::with_app(h)
                                && !app.visible
                            {
                                app.show(true, cx);
                            }
                        });
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        let cmd = sys::tray_menu(hwnd, i18n::tr("Open"), i18n::tr("Settings"), i18n::tr("Quit"));
                        w.with(|h, cx| {
                            if let Some(app) = ui::with_app(h) {
                                match cmd {
                                    sys::MENU_OPEN => app.show(true, cx),
                                    sys::MENU_SETTINGS => app.open_settings(cx),
                                    sys::MENU_QUIT => app.quit(cx),
                                    _ => {}
                                }
                            }
                        });
                    }
                    _ => {}
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    })));

    match command {
        "settings" => settings_ui::open(&gfx, &shared, true),
        "show" if !test_port() => {
            win.with(|h, cx| {
                if let Some(app) = ui::with_app(h) {
                    app.show(true, cx);
                }
            });
        }
        _ => {}
    }
    #[cfg(feature = "test-hook")]
    test_hook::start(win, gfx.clone(), shared.clone());
    window::run();
    // settings, history and file index are saved as they change: leave now, without releasing the COM objects of
    // the graphics and the shell one by one (that can stall the exit for seconds)
    std::process::exit(0);
}
