// Plume Music — your music, YouTube Music's catalog and the playlists you import from Spotify, Apple Music and
// SoundCloud, with synced lyrics, animated covers and backgrounds, in one light native player.
//
//   plume-music.exe [files…]     the window (and plays the files given)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod audio;
mod i18n;
mod import;
mod library;
mod lyrics;
mod net;
mod settings;
mod smtc;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;
mod wasapi;
mod yt;
mod zip;

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

pub const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-music-256.png");
const MARK: &str = "PlumeMusicWindow";

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
    let line = raw.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
    // already open: it takes the files and comes forward
    if std::env::var_os("PLUME_MUSIC_TEST_PORT").is_none()
        && let Some(w) = window::find(MARK)
    {
        w.send_text(&line);
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
        title: "Plume Music".into(),
        size: (1240.0, 760.0),
        min: (860.0, 600.0),
        dark,
        background: if dark { plume_ui::Color::hex(0x121214) } else { plume_ui::Color::hex(0xf6f6f8) },
        activate: std::env::var_os("PLUME_MUSIC_NOACTIVATE").is_none(),
        placement: None,
    };
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, dark))).expect("window");
    win.mark(MARK);
    if !raw.is_empty() {
        win.with(|h, cx| {
            if let Some(app) = ui::with_app(h) {
                app.open_args(&line, cx);
            }
        });
    }
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}
