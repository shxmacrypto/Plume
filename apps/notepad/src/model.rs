//! What the main window and the pads share: the settings, the theme, the pads and their windows, where data goes,
//! and which registry roots are used (test roots in a test build).

use crate::{
    layout::Measure,
    registry::Root,
    store::{Pads, Settings},
};
use plume_ui::{Color, Gfx, Theme, Window};
use std::{collections::HashMap, path::PathBuf, rc::Rc};

/// Plume Notepad's accent: the sand of its icon (darker in light mode, for white text on it).
pub const ACCENT_DARK: u32 = 0xc8894e;
pub const ACCENT_LIGHT: u32 = 0xa0652e;

pub fn system_dark() -> bool {
    use windows::{
        Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        core::w,
    };
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

pub fn theme_for(set: &Settings) -> Theme {
    let dark = match set.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => system_dark(),
    };
    Theme::new(dark, Some(Color::hex(if dark { ACCENT_DARK } else { ACCENT_LIGHT })))
}

/// Text on the accent: near-black on the light sand of dark mode, white on the deeper one of light mode.
pub fn on_accent(t: &Theme) -> Color {
    if t.dark { Color::hex(0x1c1410) } else { Color::hex(0xffffff) }
}

/// The registry root for the machine-wide keys (Windows Notepad's replacement): a test root in a test build.
pub fn machine_root() -> Root {
    #[cfg(feature = "test-hook")]
    {
        Root::test(&std::env::var("PLUME_NOTEPAD_TEST_HKLM").unwrap_or_else(|_| r"Software\PlumeNotepadTest\HKLM".into()))
    }
    #[cfg(not(feature = "test-hook"))]
    {
        Root::machine()
    }
}

/// The registry root for the user's keys (Run, file types): a test root in a test build.
pub fn user_root() -> Root {
    #[cfg(feature = "test-hook")]
    {
        Root::test(&std::env::var("PLUME_NOTEPAD_TEST_HKCU").unwrap_or_else(|_| r"Software\PlumeNotepadTest\HKCU".into()))
    }
    #[cfg(not(feature = "test-hook"))]
    {
        Root::user()
    }
}

pub fn exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
}

/// The fixed-width font: Cascadia Mono (Windows 11), else Consolas.
pub fn mono_font(gfx: &Gfx) -> &'static str {
    use windows::core::{BOOL, w};
    unsafe {
        let mut fonts = None;
        if gfx.dw.GetSystemFontCollection(&mut fonts, false).is_ok()
            && let Some(f) = fonts
        {
            let (mut i, mut ok) = (0u32, BOOL(0));
            if f.FindFamilyName(w!("Cascadia Mono"), &mut i, &mut ok).is_ok() && ok.as_bool() {
                return "Cascadia Mono";
            }
        }
    }
    "Consolas"
}

pub struct Shared {
    pub dir: PathBuf,
    pub gfx: Rc<Gfx>,
    pub set: Settings,
    pub theme: Theme,
    pub pads: Pads,
    pub pads_dirty: bool,
    pub pad_wins: HashMap<u64, Window>,
    pub main: Option<Window>,
    /// The font of the pads (they all share it).
    pub pad_measure: Rc<Measure>,
    pub quitting: bool,
    /// The main window's write timer is running.
    pub save_armed: bool,
    /// Test windows never take the focus.
    pub noactivate: bool,
    pub log: Vec<String>,
}

impl Shared {
    pub fn note(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 60 {
            self.log.remove(0);
        }
    }

    /// Tabs and pads are written soon: at most a moment after the first change not written yet (typing on does
    /// not push it back, so a crash loses little).
    pub fn arm_save(&mut self) {
        if !self.save_armed
            && let Some(m) = self.main
        {
            self.save_armed = true;
            m.timer(crate::ui::SAVE, Some(1500));
        }
    }

    /// Kept running when the window is closed: pads to look after, or they open at sign-in.
    pub fn keep_alive(&self) -> bool {
        self.pads.list.iter().any(|p| p.open) || crate::registry::run_on(&user_root())
    }
}
