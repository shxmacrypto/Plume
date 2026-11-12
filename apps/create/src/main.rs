// Plume Create — photo editing and graphic design in one native app.
//
//   plume-create.exe [file]   the window; a photo opens to edit, a .plumecreate project opens as it was saved
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod i18n;
mod settings;
#[cfg(feature = "test-hook")]
mod test_hook;
mod text;
mod ui;
mod wic;

use plume_ui::{Gfx, window};
use std::{path::PathBuf, rc::Rc};
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

const MARK: &str = "PlumeCreateWindow";
pub const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-create-256.png");

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
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // --register-defaults / --unregister-defaults: Windows default app (run by Plume Suite and the installer)
    if let Some(code) = plume_defaults::switch(&plume_defaults::CREATE, &raw) {
        std::process::exit(code);
    }
    let file = raw.iter().map(PathBuf::from).find(|p| p.is_file());
    let testing = std::env::var_os("PLUME_CREATE_TEST_PORT").is_some();
    // already open: it takes the file and comes forward
    if !testing && let Some(w) = window::find(MARK) {
        if let Some(f) = &file {
            w.send_text(&format!("\"{}\"", f.display()));
        } else {
            w.send_text("");
        }
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let set = settings::Settings::load();
    i18n::set(&set.lang);
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let dark = ui::theme_dark(&set);
    let o = window::Options {
        title: "Plume Create".into(),
        size: (1360.0, 860.0),
        min: (980.0, 620.0),
        dark,
        background: if dark { plume_ui::Color::hex(0x1c1c1e) } else { plume_ui::Color::hex(0xf2f2f7) },
        activate: std::env::var_os("PLUME_CREATE_NOACTIVATE").is_none(),
        placement: None,
    };
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, set))).expect("window");
    win.mark(MARK);
    if let Some(f) = file {
        win.with(|h, cx| {
            if let Some(app) = ui::with_app(h) {
                app.open_path(&f, cx);
            }
        });
    }
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}
