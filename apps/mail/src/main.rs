// Plume Mail — le courrier de la suite Plume, pour Windows 10 et 11 : une application native, à l'interface
// dessinée à la main (Win32 + Direct2D + DirectWrite) aux couleurs de Plume. Les messages ne passent jamais
// par un navigateur embarqué : un message en HTML est rendu en texte, sans rien télécharger ni exécuter.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// la prise d’essai décrit tout l’état de la fenêtre dans un seul objet JSON
#![cfg_attr(feature = "test-hook", recursion_limit = "256")]

mod app;
#[cfg(feature = "test-hook")]
mod test_hook;

use app::{App, settings::Settings};
use mail_core::store::Store;
use plume_ui::{Gfx, Theme, window};
use std::rc::Rc;
use windows::Win32::{
    System::Ole::OleInitialize,
    UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
};

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // OLE (et donc COM) : presse-papiers, boîtes de dialogue de Windows
        let _ = OleInitialize(None);
    }
    let store = Store::open();
    // un dossier d'essai ne reprend rien des réglages de l'utilisateur
    let set = Settings::load(store.dir.clone(), std::env::var_os("PLUME_MAIL_PROFILE").is_some());
    let gfx = Rc::new(Gfx::new().expect("Direct2D"));
    let theme = Theme::new(set.dark(), set.accent);
    let o = window::Options {
        title: "Plume Mail".into(),
        size: (1180.0, 760.0),
        min: (780.0, 480.0),
        dark: theme.dark,
        background: theme.bg,
        activate: std::env::var_os("PLUME_MAIL_NOACTIVATE").is_none(),
        placement: set.win,
    };
    let win = window::create(&gfx, o, move |win| Box::new(App::new(win, set, store))).expect("fenêtre");
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    let _ = win;
    window::run();
}
