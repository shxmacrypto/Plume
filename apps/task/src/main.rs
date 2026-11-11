// Plume Task — see what uses your processor, memory, disk, graphics and battery, in plain words, and stop it.
// Programs grouped by executable, live graphs, the programs that start with Windows. Nothing goes online.
//
//   plume-task.exe       the window (a second launch brings the open one forward)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod i18n;
mod model;
mod pdh;
mod sampler;
mod settings;
mod startup;
mod sys;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;

use plume_ui::{Gfx, window};
use std::rc::Rc;
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

pub const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-task-256.png");
const MARK: &str = "PlumeTaskWindow";

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
    // already open: it comes forward
    if std::env::var_os("PLUME_TASK_TEST_PORT").is_none()
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
        title: "Plume Task".into(),
        size: (1180.0, 760.0),
        min: (1000.0, 600.0),
        dark,
        background: if dark { plume_ui::Color::hex(0x131416) } else { plume_ui::Color::hex(0xf5f6f7) },
        activate: std::env::var_os("PLUME_TASK_NOACTIVATE").is_none(),
        placement: None,
    };
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, dark))).expect("window");
    win.mark(MARK);
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}
