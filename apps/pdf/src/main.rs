// Plume PDF — a fast PDF reader and editor: pages drawn by Windows' own PDF engine, search, outline, dark pages,
// printing; highlights, notes, drawings, text, a signature and form filling written as real PDF annotations;
// merge, split, reorder, rotate, delete and extract pages. Nothing goes online.
//
//   plume-pdf.exe [files…]     the window (and opens the files given)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod edit;
mod i18n;
mod print;
mod render;
#[cfg(any(test, feature = "test-hook"))]
mod samples;
mod settings;
#[cfg(feature = "test-hook")]
mod test_hook;
mod text;
mod ui;

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

pub const LOGO: &[u8] = include_bytes!("../../../assets/brand/plume-pdf-256.png");
const MARK: &str = "PlumePdfWindow";

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

/// Splits a command line into its arguments (quotes keep spaces together).
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in s.chars() {
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

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // --register-defaults / --unregister-defaults: Windows default app (run by Plume Suite and the installer)
    if let Some(code) = plume_defaults::switch(&plume_defaults::PDF, &raw) {
        std::process::exit(code);
    }
    let line = raw.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
    // already open: it takes the files and comes forward
    if std::env::var_os("PLUME_PDF_TEST_PORT").is_none()
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
        title: "Plume PDF".into(),
        size: (1180.0, 800.0),
        min: (760.0, 520.0),
        dark,
        background: if dark { plume_ui::Color::hex(0x1b1b1e) } else { plume_ui::Color::hex(0xf6f6f8) },
        activate: std::env::var_os("PLUME_PDF_NOACTIVATE").is_none(),
        placement: None,
    };
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::App::new(win, g, dark))).expect("window");
    win.mark(MARK);
    let files: Vec<PathBuf> = raw.into_iter().map(PathBuf::from).filter(|p| p.is_file()).collect();
    if !files.is_empty() {
        win.with(|h, cx| {
            if let Some(app) = ui::with_app(h) {
                app.open_paths(files, cx);
            }
        });
    }
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}

#[cfg(test)]
mod tests {
    #[test]
    fn arguments_keep_quoted_spaces() {
        assert_eq!(super::split_args(r#""C:\My files\a.pdf" b.pdf"#), vec![r"C:\My files\a.pdf", "b.pdf"]);
    }
}
