// Plume Boost — Windows' performance settings in one place: each one explained (what it does, what it gains, what it
// costs, where the evidence comes from), applied only when ticked, and all of them reversible.
//
//   plume-boost.exe              the window (a second launch brings the open one forward)
//   plume-boost.exe --elevated   the same, started again as administrator at the user's request
//   plume-boost.exe --undo-all   puts back everything Plume Boost changed, without a window (for the uninstaller);
//                                asks Windows for administrator rights first when a change needs them
//   plume-boost.exe --apply-gpu-profile   applies the graphics profile chosen for sign-in (the Run value), unless the
//                                crash guard blames the last change for a crash or a driver reset
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod backup_link;
mod engine;
mod gpu;
mod i18n;
mod live;
mod regfile;
mod settings;
mod snapshot;
#[cfg(feature = "test-hook")]
mod test_hook;
mod tweaks;
mod ui;

use plume_ui::{Gfx, window};
use std::rc::Rc;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            Ole::OleInitialize,
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
            Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{HSTRING, PCWSTR, w},
};

pub const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-boost-256.png");
const MARK: &str = "PlumeBoostWindow";

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

/// Starts Plume Boost again as administrator: Windows shows its UAC prompt and the user decides. Never in a test
/// build. `wait`: waits for it and returns its exit code. None when Windows didn't start it (prompt refused).
pub fn run_elevated(owner: Option<HWND>, args: &str, wait: bool) -> Option<u32> {
    if cfg!(feature = "test-hook") {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let (file, params) = (HSTRING::from(exe.as_os_str()), HSTRING::from(args));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: if wait { SEE_MASK_NOCLOSEPROCESS } else { 0 },
        hwnd: owner.unwrap_or_default(),
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut info).ok()?;
        if !wait {
            return Some(0);
        }
        if info.hProcess.is_invalid() {
            return None;
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        Some(code)
    }
}

/// `--undo-all`: 0 when everything was put back (or nothing had been changed), 1 otherwise.
fn undo_all_headless(be: backend::Shared) -> i32 {
    let mut e = engine::Engine::new(be.clone(), Some(settings::data_dir()));
    gpu::Guard::clear();
    if e.log.entries.is_empty() {
        return 0;
    }
    if e.undo_needs_admin() && !be.is_admin() && !be.is_test() {
        return match run_elevated(None, "--undo-all", true) {
            Some(code) => code as i32,
            None => 1,
        };
    }
    // a snapshot first, like Undo everything in the window (kept on this PC: no window to answer about Plume Backup)
    let _ = snapshot::take(&be, snapshot::Why::UndoAll, "--undo-all");
    if e.undo_all().is_empty() { 0 } else { 1 }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (be, fake_dir) = backend::choose();
    settings::init_dir(fake_dir);
    if args.iter().any(|a| a == "--undo-all") {
        i18n::set(&settings::Settings::load().lang);
        std::process::exit(undo_all_headless(be));
    }
    if args.iter().any(|a| a == "--apply-gpu-profile") {
        i18n::set(&settings::Settings::load().lang);
        std::process::exit(ui::flows::apply_profile_headless(be));
    }
    let elevated_start = args.iter().any(|a| a == "--elevated");
    // already open: it comes forward (not for the copy started as administrator: the other one is closing)
    if std::env::var_os("PLUME_BOOST_TEST_PORT").is_none()
        && !elevated_start
        && let Some(w) = window::find(MARK)
    {
        w.send_text("");
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let dark = system_dark();
    let o = window::Options {
        title: "Plume Boost".into(),
        size: (1180.0, 780.0),
        min: (980.0, 620.0),
        dark,
        background: if dark { plume_ui::Color::hex(0x1c1c1e) } else { plume_ui::Color::hex(0xf2f2f7) },
        activate: std::env::var_os("PLUME_BOOST_NOACTIVATE").is_none(),
        placement: None,
    };
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, dark, be))).expect("window");
    win.mark(MARK);
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}
