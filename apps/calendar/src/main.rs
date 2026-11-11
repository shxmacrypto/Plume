// Plume Calendar — a clean calendar that works offline: month, week, day and agenda views, repeating events,
// reminders near the clock, several local calendars, .ics import and export, and subscriptions to published
// calendars (the only thing that goes online).
//
//   plume-calendar.exe [file.ics | webcal://…]   the window (and imports the file or subscribes)
//   plume-calendar.exe --background             no window, only the icon near the clock (start with Windows)
//   plume-calendar.exe --unregister             no more start with Windows, and the running one quits (uninstall)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod date;
mod i18n;
mod ics;
mod model;
mod net;
mod popup;
mod quickadd;
mod recur;
mod reminders;
mod settings;
mod sys;
#[cfg(feature = "test-hook")]
mod test_hook;
mod tz;
mod ui;

use plume_ui::{Gfx, window};
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

/// Marks the window: a second launch finds it and hands it its arguments.
const MARK: &str = "PlumeCalendarWindow";

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

/// `"a b" c` → [a b, c].
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn test_port() -> bool {
    cfg!(feature = "test-hook") && std::env::var_os("PLUME_CALENDAR_TEST_PORT").is_some()
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let background = raw.iter().any(|a| a.eq_ignore_ascii_case("--background"));
    if raw.iter().any(|a| a.eq_ignore_ascii_case("--unregister")) {
        sys::set_autostart(false);
        if let Some(w) = window::find(MARK) {
            w.send_text("quit");
        }
        std::process::exit(0);
    }
    let line = raw.iter().filter(|a| !a.starts_with("--")).map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
    // already running: it takes the arguments and comes forward
    if !test_port()
        && let Some(w) = window::find(MARK)
    {
        w.send_text(if background {
            "--background"
        } else if line.is_empty() {
            "open"
        } else {
            &line
        });
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
    }
    let set = settings::Settings::load();
    i18n::set(&set.lang);
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let theme = ui::theme_for(&set);
    let quiet = test_port() || std::env::var_os("PLUME_CALENDAR_QUIET_REMINDERS").is_some();
    let o = window::Options {
        title: "Plume Calendar".into(),
        size: (1240.0, 800.0),
        min: (900.0, 640.0),
        dark: theme.dark,
        background: theme.bg,
        activate: !background && std::env::var_os("PLUME_CALENDAR_NOACTIVATE").is_none(),
        placement: None,
    };
    let keep = Rc::new(Cell::new(false));
    let k = keep.clone();
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, set, k, quiet))).expect("window");
    win.mark(MARK);
    let hwnd = win.hwnd;
    if background {
        sys::show(hwnd, false);
    }
    // the icon near the clock (not during tests unless asked)
    let tray = win
        .with(|h, _| {
            let app = ui::with_app(h)?;
            let on = app.set.background && (!quiet || std::env::var_os("PLUME_CALENDAR_TEST_TRAY").is_some());
            app.tray = on;
            on.then(|| app.tray_tip())
        })
        .flatten();
    if let Some(tip) = tray {
        sys::tray_add(hwnd, &tip);
    }
    win.with(|h, cx| {
        if let Some(app) = ui::with_app(h) {
            cx.timer(ui::TICK, Some(if quiet { 1000 } else { 30_000 }));
            app.check_reminders(cx);
            if !line.is_empty() {
                app.open_args(&line, cx);
            }
            // subscriptions: the first refresh comes with the first tick
        }
    });

    // closing the window hides it while Plume Calendar stays near the clock; "Quit" in the icon's menu quits
    let quitting = Rc::new(Cell::new(false));
    let q = quitting.clone();
    let hwnd_v = hwnd.0 as isize;
    let s_open = i18n::tr("Open");
    let s_quit = i18n::tr("Quit");
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
                    WM_RBUTTONUP | WM_CONTEXTMENU => match sys::tray_menu(hwnd, s_open, s_quit) {
                        sys::MENU_OPEN => sys::show(hwnd, true),
                        sys::MENU_QUIT => {
                            q.set(true);
                            unsafe {
                                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                            }
                        }
                        _ => {}
                    },
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

#[cfg(test)]
mod tests {
    #[test]
    fn args() {
        assert_eq!(super::split_args(r#""C:\a b\x.ics" webcal://h/c.ics"#), vec![r"C:\a b\x.ics", "webcal://h/c.ics"]);
    }
}
