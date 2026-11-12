// Plume Clipboard — a fast, private clipboard history: text, links, pictures and copied files, pins and snippets,
// and a small panel (Ctrl+Shift+V) that pastes them into any app. Everything stays on this PC.
//
//   plume-clipboard.exe                 the window
//   plume-clipboard.exe --background    no window, only the icon near the clock (start with Windows)
//   plume-clipboard.exe --panel         opens the panel of the running Plume Clipboard
//   plume-clipboard.exe --unregister    no more start with Windows, and the running Plume Clipboard quits (uninstall)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clip;
mod crypto;
mod detect;
mod hotkey;
mod i18n;
mod image;
mod input;
mod model;
mod settings;
mod store;
mod sys;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;

use model::Model;
use plume_ui::{Gfx, window};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use ui::panel::{PANEL_H, PANEL_W, Panel};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Dwm::{DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute},
        System::{DataExchange::AddClipboardFormatListener, Ole::OleInitialize},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

/// Marks the window: a second launch finds it.
const MARK: &str = "PlumeClipboardWindow";
const WM_CLIPBOARDUPDATE: u32 = 0x031D;
const WM_HOTKEY: u32 = 0x0312;

/// The panel: no frame, over every window, not in the taskbar, shown and hidden at once.
fn make_popup(hwnd: HWND) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_POPUP | WS_CLIPCHILDREN).0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (WS_EX_TOOLWINDOW | WS_EX_TOPMOST).0 as isize);
        let round: i32 = 2; // DWMWCP_ROUND
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const i32 as _, 4);
        let off: i32 = 1;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED, &off as *const i32 as _, 4);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_HIDEWINDOW);
    }
}

fn post_close(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

fn open_panel(panel: window::Window) {
    let (target, at) = ui::panel::anchor();
    panel.with(|h, cx| {
        if let Some(p) = h.as_any().downcast_mut::<Panel>() {
            p.open(target, Some(at), cx);
        }
    });
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let background = args.iter().any(|a| a == "--background");
    if args.iter().any(|a| a == "--unregister") {
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
    // already running: it comes forward, or opens its panel (unless this launch only wanted the background)
    if std::env::var_os("PLUME_CLIPBOARD_TEST_PORT").is_none()
        && let Some(w) = window::find(MARK)
    {
        if args.iter().any(|a| a == "--panel") {
            w.send_text("panel");
        } else if !background {
            w.send_text("open");
        }
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let model = Rc::new(RefCell::new(Model::load()));
    i18n::set(&model.borrow().set.lang);
    let theme = model.borrow().theme.clone();
    let noactivate = std::env::var_os("PLUME_CLIPBOARD_NOACTIVATE").is_some();
    let quitting = Rc::new(Cell::new(false));

    let o = window::Options {
        title: "Plume Clipboard".into(),
        size: (1000.0, 700.0),
        min: (780.0, 540.0),
        dark: theme.dark,
        background: theme.bg,
        activate: !background && !noactivate,
        placement: None,
    };
    let (m, q) = (model.clone(), quitting.clone());
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, m, q))).expect("window");
    win.mark(MARK);
    let hwnd = win.hwnd;
    if background {
        sys::show(hwnd, false);
    }

    let po = window::Options {
        title: "Plume Clipboard".into(),
        size: (PANEL_W, PANEL_H),
        min: (300.0, 200.0),
        dark: theme.dark,
        background: theme.bg,
        activate: false,
        placement: None,
    };
    let m = model.clone();
    let panel = window::create(&gfx, po, move |w| Box::new(Panel::new(w, m))).expect("panel");
    make_popup(panel.hwnd);
    {
        let mut m = model.borrow_mut();
        m.main = Some(win);
        m.panel = Some(panel);
        m.register_hotkey();
    }

    // the clipboard, without polling (tests can leave the user's copies alone: PLUME_CLIPBOARD_LISTEN=0)
    if std::env::var("PLUME_CLIPBOARD_LISTEN").as_deref() != Ok("0") {
        unsafe {
            let _ = AddClipboardFormatListener(hwnd);
        }
    }
    // every minute: sensitive items only live a short time
    win.timer(model::PRUNE, Some(60_000));
    let tip = win.with(|h, _| ui::with_app(h).map(|a| a.tray_tip())).flatten().unwrap_or_default();
    sys::tray_add(hwnd, &tip);

    // the panel's close (Alt+F4) only hides it
    let q = quitting.clone();
    panel.set_hook(Some(Rc::new(move |msg, _wp, _lp| match msg {
        WM_CLOSE if !q.get() => {
            panel.with(|h, cx| {
                if let Some(p) = h.as_any().downcast_mut::<Panel>() {
                    p.hide(cx);
                }
            });
            Some(LRESULT(0))
        }
        _ => None,
    })));

    // the main window: the clipboard, the shortcut, the icon near the clock; its close button hides it
    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    let q = quitting.clone();
    let m = model.clone();
    let sender = win.sender();
    win.set_hook(Some(Rc::new(move |msg, wp, lp| {
        match msg {
            WM_CLIPBOARDUPDATE => {
                sender.send(|h, cx| {
                    if let Some(a) = ui::with_app(h) {
                        a.on_clipboard(cx);
                    }
                });
                Some(LRESULT(0))
            }
            WM_HOTKEY if wp.0 == hotkey::ID as usize => {
                open_panel(panel);
                Some(LRESULT(0))
            }
            WM_CLOSE if !q.get() => {
                sys::show(hwnd, false);
                Some(LRESULT(0))
            }
            WM_ENDSESSION if wp.0 != 0 => {
                if let Ok(mut m) = m.try_borrow_mut() {
                    m.dirty = true;
                    m.save();
                }
                None
            }
            sys::WM_TRAY => {
                match lp.0 as u32 & 0xffff {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => sys::show(hwnd, true),
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        let paused = m.try_borrow().map(|m| m.set.paused).unwrap_or(false);
                        let labels = [
                            i18n::tr("Open Plume Clipboard"),
                            i18n::tr("Open the panel"),
                            i18n::tr("Pause"),
                            i18n::tr("Quit"),
                            i18n::tr("Clear sensitive items"),
                        ];
                        match sys::tray_menu(hwnd, labels, paused) {
                            sys::MENU_OPEN => sys::show(hwnd, true),
                            sys::MENU_PANEL => open_panel(panel),
                            sys::MENU_PAUSE => {
                                win.with(|h, cx| {
                                    if let Some(a) = ui::with_app(h) {
                                        a.change_setting(|s| s.paused = !s.paused, cx);
                                    }
                                });
                            }
                            sys::MENU_CLEAR_SECRETS => {
                                win.with(|h, cx| {
                                    if let Some(a) = ui::with_app(h) {
                                        a.act_public(ui::Hit::ClearSecrets, cx);
                                    }
                                });
                            }
                            sys::MENU_QUIT => {
                                q.set(true);
                                post_close(panel.hwnd);
                                post_close(hwnd);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                Some(LRESULT(0))
            }
            _ if msg == taskbar_created => {
                // Explorer started again: the icon comes back
                let tip = win.with(|h, _| ui::with_app(h).map(|a| a.tray_tip())).flatten().unwrap_or_default();
                sys::tray_add(hwnd, &tip);
                None
            }
            _ => None,
        }
    })));
    #[cfg(feature = "test-hook")]
    test_hook::start(win, panel);
    window::run();
}
