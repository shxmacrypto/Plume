// Plume Backup — automatic backups of your folders to a USB drive, another folder or a network share: dated versions
// or a mirror, on a schedule or when the drive is plugged in, and a restore view to put any file back.
//
//   plume-backup.exe                 the window
//   plume-backup.exe --background    no window, only the icon near the clock (start with Windows)
//   plume-backup.exe --run           backs up every plan now (the running Plume Backup does it if there is one)
//   plume-backup.exe --unregister    no more start with Windows, and the running Plume Backup quits (uninstall)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod i18n;
mod settings;
mod sys;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;

use plume_ui::{Gfx, window};
use std::{cell::Cell, rc::Rc};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
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
const MARK: &str = "PlumeBackupWindow";

pub fn system_dark() -> bool {
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
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let has = |a: &str| args.iter().any(|x| x == a);
    let background = has("--background");
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
        std::process::exit(0);
    }
    // already running: it comes forward, or backs up (unless this launch only wanted the background)
    let test = std::env::var_os("PLUME_BACKUP_TEST_PORT").is_some();
    if !test && let Some(w) = window::find(MARK) {
        if has("--run") {
            w.send_text("run");
        } else if !background {
            w.send_text("open");
        }
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let set = settings::Settings::load();
    i18n::set(&set.lang);
    let theme = ui::theme(&set);
    let o = window::Options {
        title: "Plume Backup".into(),
        size: (1080.0, 760.0),
        min: (860.0, 600.0),
        dark: theme.dark,
        background: theme.bg,
        activate: !background && std::env::var_os("PLUME_BACKUP_NOACTIVATE").is_none(),
        placement: None,
    };
    let keep = Rc::new(Cell::new(true));
    let k = keep.clone();
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, k))).expect("window");
    win.mark(MARK);
    let hwnd = win.hwnd;
    if background {
        sys::show(hwnd, false);
    }
    let tip = win.with(|h, _| ui::with_app(h).map(|a| a.tray_text())).flatten().unwrap_or_default();
    sys::tray_add(hwnd, &tip);
    win.with(|h, cx| {
        if let Some(app) = ui::with_app(h) {
            app.start();
            if has("--run") {
                let ids: Vec<u64> = app.store.plans.iter().filter(|p| !p.paused).map(|p| p.id).collect();
                for id in ids {
                    app.enqueue(id, cx);
                }
            }
        }
    });

    // the window's close button hides it (the schedules keep running); "Quit" in the icon's menu really quits
    let quitting = Rc::new(Cell::new(false));
    let q = quitting.clone();
    let hwnd_v = hwnd.0 as isize;
    win.set_hook(Some(Rc::new(move |msg, _wp, lp| {
        let hwnd = HWND(hwnd_v as _);
        let quit = |q: &Cell<bool>| {
            q.set(true);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        };
        match msg {
            WM_CLOSE if keep.get() && !q.get() => {
                sys::show(hwnd, false);
                Some(LRESULT(0))
            }
            sys::WM_TRAY => {
                match lp.0 as u32 {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => sys::show(hwnd, true),
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        let (open, run, bye) = (i18n::tr("Open Plume Backup"), i18n::tr("Back up all now"), i18n::tr("Quit"));
                        match sys::tray_menu(hwnd, open, run, bye) {
                            sys::MENU_OPEN => sys::show(hwnd, true),
                            sys::MENU_RUN => {
                                window::Window { hwnd }.sender().send(|h, cx| {
                                    if let Some(app) = ui::with_app(h) {
                                        let ids: Vec<u64> = app.store.plans.iter().filter(|p| !p.paused).map(|p| p.id).collect();
                                        for id in ids {
                                            app.enqueue(id, cx);
                                        }
                                    }
                                });
                            }
                            sys::MENU_QUIT => quit(&q),
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
