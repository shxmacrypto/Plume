// Plume Suite — installe, ouvre, retire et met à jour les applications Plume (Plume Web, Plume FEx, Plume Mail).
// Fenêtre native dessinée à la main (Win32 + Direct2D + DirectWrite), comme les autres applications de la suite.
//
//   plume-suite.exe                    la fenêtre
//   plume-suite.exe --update-now       la fenêtre, qui lance aussitôt la mise à jour (bouton d'une autre application)
//   plume-suite.exe --register [--desktop]   raccourcis + entrée de Paramètres › Applications (lancé par PlumeSetup.exe)
//   plume-suite.exe --unregister       les retire (désinstallation de toute la suite)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod i18n;
#[cfg(feature = "test-hook")]
mod test_hook;
mod ui;

use plume_ui::{Color, Gfx, Theme, window};
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

/// Nom qui marque la fenêtre : un second lancement la retrouve et lui passe sa ligne de commande.
const MARK: &str = "PlumeSuiteWindow";

fn system_dark() -> bool {
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

/// Le thème et la couleur d'accent choisis dans Plume Web, sinon ceux de Windows : toute la suite a le même visage.
fn theme() -> Theme {
    let cfg = std::env::var_os("LOCALAPPDATA")
        .and_then(|b| std::fs::read_to_string(std::path::Path::new(&b).join("Plume").join("plume.json")).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim_start_matches('\u{feff}')).ok())
        .map(|v| v["cfg"].clone())
        .unwrap_or_default();
    let dark = match (std::env::var("PLUME_SUITE_THEME").ok().as_deref(), cfg["theme"].as_str()) {
        (Some("dark"), _) | (None, Some("dark")) => true,
        (Some("light"), _) | (None, Some("light")) => false,
        _ => system_dark(),
    };
    Theme::new(dark, cfg["accent"].as_str().and_then(Color::parse))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--register") || has("--unregister") {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        if has("--unregister") {
            apps::unregister();
            std::process::exit(0);
        }
        std::process::exit(if apps::register(has("--desktop")) { 0 } else { 1 });
    }
    // déjà ouverte : elle reçoit la demande et passe au premier plan
    if std::env::var_os("PLUME_SUITE_TEST_PORT").is_none()
        && let Some(w) = window::find(MARK)
    {
        w.send_text(&args.join(" "));
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
    }
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let theme = theme();
    let s = i18n::strings(i18n::french());
    let o = window::Options {
        title: "Plume Suite".into(),
        size: (900.0, 700.0),
        min: (640.0, 660.0),
        dark: theme.dark,
        background: theme.bg,
        activate: std::env::var_os("PLUME_SUITE_NOACTIVATE").is_none(),
        placement: None,
    };
    let (updated, update_now) = (has("--updated"), has("--update-now"));
    let g = gfx.clone();
    let win = window::create(&gfx, o, move |win| Box::new(ui::Hub::new(win, g, theme, s, updated, update_now))).expect("fenêtre");
    win.mark(MARK);
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    window::run();
}
