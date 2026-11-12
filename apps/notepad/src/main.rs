// Plume Notepad — a fast plain-text editor with tabs, that can take the place of Windows Notepad, and sticky pads
// that stay over every window.
//
//   plume-notepad.exe [file…]               the window (a running Plume Notepad opens the files in tabs)
//   plume-notepad.exe --pad                 a new sticky pad
//   plume-notepad.exe --background          only the pads and the icon near the clock (sign-in)
//   plume-notepad.exe --ifeo <notepad.exe> [file]
//                                           how Windows starts it in place of notepad.exe (Image File Execution
//                                           Options): the path of notepad.exe is skipped
//   plume-notepad.exe --set-default on|off  (as administrator) Windows Notepad's replacement, on or off
//   plume-notepad.exe --unregister          everything Plume Notepad put in the registry removed (the
//                                           replacement through Windows' administrator prompt if it is on), and
//                                           the running Plume Notepad quits: for the uninstaller
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clip;
mod codec;
mod editor;
mod i18n;
mod layout;
mod model;
mod pads;
mod registry;
mod search;
mod store;
mod sys;
#[cfg(feature = "test-hook")]
mod test_hook;
mod text;
mod ui;

use model::Shared;
use plume_ui::{Gfx, window};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use windows::{
    Win32::{
        Foundation::{LRESULT, POINT},
        Graphics::Gdi::ScreenToClient,
        System::Ole::OleInitialize,
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

/// Marks the main window: a second launch finds it.
const MARK: &str = "PlumeNotepadWindow";

/// Quits: everything is written, the pads and the window close.
pub fn quit(sh: &Rc<RefCell<Shared>>) {
    let main = {
        let mut s = sh.borrow_mut();
        s.quitting = true;
        s.main
    };
    if let Some(m) = main {
        sys::tray_remove(m.hwnd);
        m.close();
    }
}

/// Windows Notepad's replacement, from the settings switch: written at once when Plume Notepad already runs as
/// administrator, else by a copy of itself started through Windows' administrator prompt.
#[cfg(not(feature = "test-hook"))]
pub fn set_default_elevated(on: bool) -> Option<bool> {
    let root = registry::Root::machine();
    let plan = registry::ifeo_plan(on, &registry::read_ifeo(&root), &registry::debugger_cmd(&model::exe()));
    if registry::apply(&root, &plan).is_ok() {
        return Some(true);
    }
    sys::run_elevated(if on { "--set-default on" } else { "--set-default off" })
}

/// `--unregister`: the uninstaller's call.
fn unregister() {
    let exe = model::exe();
    let user = model::user_root();
    let _ = registry::apply(&user, &registry::run_plan(false, &exe));
    let _ = registry::apply(&user, &registry::assoc_plan(false, &exe));
    sys::assoc_changed();
    let machine = model::machine_root();
    let now = registry::read_ifeo(&machine);
    let plan = registry::ifeo_plan(false, &now, "");
    if !plan.is_empty() && registry::apply(&machine, &plan).is_err() {
        #[cfg(not(feature = "test-hook"))]
        let _ = sys::run_elevated("--set-default off");
    }
    if let Some(w) = window::find(MARK) {
        w.send_text("quit");
        for _ in 0..50 {
            if window::find(MARK).is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

/// What the command line asks for.
#[derive(Debug, Default, PartialEq)]
struct Args {
    files: Vec<String>,
    background: bool,
    pad: bool,
}

fn parse_args(raw: &str, args: &[String]) -> Args {
    let mut a = Args::default();
    let lower = raw.to_ascii_lowercase();
    if let Some(i) = lower.find("--ifeo") {
        a.files.extend(registry::ifeo_file(&raw[i + 6..]));
        return a;
    }
    for s in args {
        match s.to_ascii_lowercase().as_str() {
            "--background" => a.background = true,
            "--pad" | "--new-pad" => a.pad = true,
            s if s.starts_with("--") => {}
            _ => a.files.push(s.clone()),
        }
    }
    a
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // --register-defaults / --unregister-defaults: Windows default app (run by Plume Suite and the installer)
    if let Some(code) = plume_defaults::switch(&plume_defaults::NOTEPAD, &args) {
        std::process::exit(code);
    }
    match args.first().map(|a| a.to_ascii_lowercase()).as_deref() {
        Some("--set-default") => {
            let on = args.get(1).is_some_and(|a| a.eq_ignore_ascii_case("on"));
            let root = model::machine_root();
            let plan = registry::ifeo_plan(on, &registry::read_ifeo(&root), &registry::debugger_cmd(&model::exe()));
            std::process::exit(if registry::apply(&root, &plan).is_ok() { 0 } else { 1 });
        }
        Some("--unregister") => {
            unregister();
            std::process::exit(0);
        }
        _ => {}
    }
    let a = parse_args(&sys::raw_args(), &args);
    // the paths as this launch sees them (the running Plume Notepad has another current folder)
    let files: Vec<String> = a.files.iter().map(|f| std::path::absolute(f).map(|p| p.display().to_string()).unwrap_or_else(|_| f.clone())).collect();
    let testing = std::env::var_os("PLUME_NOTEPAD_TEST_PORT").is_some();
    if !testing && let Some(w) = window::find(MARK) {
        let msg = if a.pad {
            "pad".to_string()
        } else if a.background {
            String::new()
        } else {
            std::iter::once("open".to_string()).chain(files).collect::<Vec<_>>().join("\n")
        };
        if !msg.is_empty() {
            w.send_text(&msg);
        }
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let dir = store::dir();
    let set = store::Settings::load(&dir);
    i18n::set(&set.lang);
    let theme = model::theme_for(&set);
    let noactivate = std::env::var_os("PLUME_NOTEPAD_NOACTIVATE").is_some();
    let pad_measure = Rc::new(layout::Measure::new(&gfx.dw, gfx.ui_font, 15.0));
    let placement = set.placement.map(|(x, y, w, h, maximized)| window::Placement { x, y, w, h, maximized });
    let sh = Rc::new(RefCell::new(Shared {
        dir,
        gfx: gfx.clone(),
        set,
        theme: theme.clone(),
        pads: store::Pads::default(),
        pads_dirty: false,
        pad_wins: HashMap::new(),
        main: None,
        pad_measure,
        quitting: false,
        save_armed: false,
        noactivate,
        log: Vec::new(),
    }));
    {
        let mut s = sh.borrow_mut();
        let dir = s.dir.clone();
        s.pads = store::Pads::load(&dir);
    }
    let o = window::Options {
        title: "Plume Notepad".into(),
        size: (980.0, 680.0),
        min: (520.0, 360.0),
        dark: theme.dark,
        background: theme.bg,
        activate: !a.background && !noactivate,
        placement,
    };
    let s2 = sh.clone();
    let win = window::create(&gfx, o, move |w| Box::new(ui::App::new(w, s2))).expect("window");
    win.mark(MARK);
    sh.borrow_mut().main = Some(win);
    let hwnd = win.hwnd;
    if a.background {
        sys::show(hwnd, false, false);
    }
    sys::accept_files(hwnd);
    if std::env::var("PLUME_NOTEPAD_HOTKEY").as_deref() != Ok("0") && !sys::register_hotkey(hwnd) {
        sh.borrow_mut().note("Ctrl+Alt+N is taken by another app");
    }
    if !files.is_empty() {
        win.with(|h, cx| {
            if let Some(app) = ui::with_app(h) {
                for f in &files {
                    app.open_path(std::path::Path::new(f), cx);
                }
            }
        });
    }
    pads::reopen(&sh);
    if a.pad {
        pads::new_pad(&sh, !noactivate);
    }
    pads::refresh_tray(&sh);

    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    let s = sh.clone();
    win.set_hook(Some(Rc::new(move |msg, wp, lp| {
        match msg {
            WM_CLOSE => {
                let (quitting, keep) = {
                    let st = s.borrow();
                    (st.quitting, st.keep_alive())
                };
                if !quitting && keep {
                    // the pads go on: the window only hides, its tabs stay as they are
                    win.with(|h, _| {
                        if let Some(app) = ui::with_app(h) {
                            app.flush();
                        }
                    });
                    sys::show(hwnd, false, false);
                    pads::refresh_tray(&s);
                    return Some(LRESULT(0));
                }
                None
            }
            sys::WM_TRAY => {
                match lp.0 as u32 & 0xffff {
                    WM_LBUTTONUP => {
                        let noact = s.borrow().noactivate;
                        pads::show_all(&s, !noact);
                    }
                    WM_LBUTTONDBLCLK => sys::show(hwnd, true, true),
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        let items = pads::tray_items(&s.borrow());
                        let cmd = sys::popup(hwnd, &items, None, true);
                        pads::tray_command(&s, cmd);
                    }
                    _ => {}
                }
                Some(LRESULT(0))
            }
            sys::WM_HOTKEY if wp.0 as i32 == sys::HOTKEY_ID => {
                let noact = s.borrow().noactivate;
                pads::new_pad(&s, !noact);
                Some(LRESULT(0))
            }
            sys::WM_DROPFILES => {
                let files = sys::dropped(wp);
                win.with(|h, cx| {
                    if let Some(app) = ui::with_app(h) {
                        app.settings = false;
                        for f in &files {
                            app.open_path(std::path::Path::new(f), cx);
                        }
                    }
                });
                Some(LRESULT(0))
            }
            sys::WM_IME_STARTCOMPOSITION | sys::WM_IME_COMPOSITION => {
                let info =
                    win.with(|h, _| ui::with_app(h).and_then(|a| a.tab().ed.caret_rect.map(|r| (r, a.measure.family.clone(), a.measure.size)))).flatten();
                if let Some((r, fam, size)) = info {
                    sys::ime_at(hwnd, r, win.scale(), &fam, size);
                }
                None
            }
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT => {
                let mut p = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut p);
                    let _ = ScreenToClient(hwnd, &mut p);
                }
                let sc = win.scale();
                let pt = plume_ui::pt(p.x as f32 / sc, p.y as f32 / sc);
                let ibeam = win.with(|h, _| ui::with_app(h).is_some_and(|a| a.ibeam_at(pt))).unwrap_or(false);
                if ibeam {
                    unsafe {
                        if let Ok(c) = LoadCursorW(None, IDC_IBEAM) {
                            SetCursor(Some(c));
                        }
                    }
                    return Some(LRESULT(1));
                }
                None
            }
            WM_ENDSESSION if wp.0 != 0 => {
                win.with(|h, _| {
                    if let Some(app) = ui::with_app(h) {
                        app.flush();
                    }
                });
                None
            }
            _ if msg == taskbar_created => {
                pads::refresh_tray(&s);
                None
            }
            _ => None,
        }
    })));
    #[cfg(feature = "test-hook")]
    test_hook::start(win, sh.clone());
    window::run();
    sys::tray_remove(hwnd);
    // the graphics objects are not dropped one by one after the loop: quitting at once
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        let a = parse_args(r#""C:\a\plume-notepad.exe" x"#, &["x.txt".into(), "--background".into()]);
        assert_eq!(a, Args { files: vec!["x.txt".into()], background: true, pad: false });
        // started by Windows in place of notepad.exe
        let a = parse_args(r#" --ifeo C:\Windows\System32\notepad.exe C:\notes\my list.txt"#, &[]);
        assert_eq!(a.files, vec![r"C:\notes\my list.txt".to_string()]);
        let a = parse_args(r#" --IFEO "C:\Windows\notepad.exe""#, &[]);
        assert!(a.files.is_empty());
        assert!(parse_args("", &["--pad".into()]).pad);
    }
}
