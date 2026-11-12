// Plume FEx — explorateur de fichiers natif pour Windows 10 et 11 : interface dessinée à la main
// (Win32 + Direct2D + DirectWrite), aux couleurs de Plume, recherche instantanée façon Everything.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// la prise d’essai décrit tout l’état de la fenêtre dans un seul objet JSON
#![cfg_attr(feature = "test-hook", recursion_limit = "256")]

mod app;
mod shell;
#[cfg(feature = "test-hook")]
mod test_hook;
// le kit d'interface commun aux applications Plume
use plume_ui as ui;

use app::{
    App,
    model::{Location, parse_address},
    settings::Settings,
};
use std::rc::Rc;
use ui::Theme;
use windows::Win32::{
    System::Ole::OleInitialize,
    UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
};

/// Les dossiers (ou fichiers, à montrer sélectionnés dans leur dossier) demandés sur la ligne de commande, en
/// chemins absolus.
fn start_folders() -> Vec<std::path::PathBuf> {
    let cwd = std::env::current_dir().ok();
    std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .filter_map(|a| parse_address(&a, cwd.as_deref()))
        .filter(|p| app::model::open_target(p).is_some())
        .collect()
}

/// Nom de cette installation de Plume FEx sur cet ordinateur : un par dossier de données, pour que les essais
/// (qui ont le leur) ne se mêlent jamais à la fenêtre de l'utilisateur.
fn instance_name(data_dir: &std::path::Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data_dir.to_string_lossy().to_lowercase().hash(&mut h);
    format!("PlumeFEx-{:016x}", h.finish())
}

/// Si Plume FEx tourne déjà, lui confie ce qu'il fallait ouvrir et renvoie vrai : ce lancement n'a plus rien à faire.
/// Le verrou renvoyé sinon est à garder jusqu'à la fin du programme.
fn hand_off(name: &str, folders: &[std::path::PathBuf]) -> Result<windows::Win32::Foundation::HANDLE, ()> {
    use windows::Win32::{
        Foundation::{ERROR_ALREADY_EXISTS, GetLastError},
        System::Threading::CreateMutexW,
    };
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let lock = unsafe { CreateMutexW(None, false, windows::core::PCWSTR(wide.as_ptr())) };
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if already {
        let text = folders.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n");
        // l'autre lancement est peut-être encore en train de créer sa fenêtre : on lui laisse trois secondes
        for _ in 0..30 {
            if let Some(win) = ui::window::find(name) {
                if win.send_text(&text) {
                    return Err(());
                }
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    Ok(lock.unwrap_or_default())
}

/// `plume-fex.exe --make-default` : Plume FEx ouvre désormais les dossiers et Win+E (comme l'option de son menu), en
/// gardant ce qui les ouvrait avant. PlumeSetup.exe le lance quand on installe Plume FEx depuis Plume Suite ; l'option
/// du menu et le retrait de Plume FEx le défont.
fn make_default() -> i32 {
    let Some(classes) = shell::integration::Classes::user() else { return 0 };
    let Ok(exe) = std::env::current_exe() else { return 1 };
    if classes.is_default(&exe) {
        return 0;
    }
    let mut set = Settings::load();
    let s = app::i18n::strings(set.language());
    match classes.make_default(&exe, s.open_in_fex) {
        Some(backup) => {
            set.explorer_backup.get_or_insert(backup);
            set.save();
            0
        }
        None => 1,
    }
}

/// `plume-fex.exe --unregister` : retire l'entrée « Ouvrir dans Plume FEx » et rend les dossiers et Win+E à ce qui
/// les ouvrait avant. PlumeSetup.exe le lance avant d'effacer le programme. Les réglages et l'index restent.
fn unregister() -> i32 {
    let Some(classes) = shell::integration::Classes::user() else { return 0 };
    let Ok(exe) = std::env::current_exe() else { return 1 };
    let mut set = Settings::load();
    if classes.is_default(&exe) || set.explorer_backup.is_some() {
        classes.restore(&set.explorer_backup.take().unwrap_or_default());
        set.save();
    }
    classes.set_entry(&exe, "", false);
    0
}

fn main() {
    // lancé par ssh pour poser une question (le mot de passe d'un serveur) : on la porte à la fenêtre, rien d'autre
    if let Ok(pipe) = std::env::var(shell::sftp::ASKPASS_ENV) {
        std::process::exit(shell::sftp::askpass(&pipe));
    }
    // retrait de Plume FEx (Plume Suite) : rendre à Windows les dossiers, Win+E et le clic droit, puis s'arrêter
    if std::env::args().skip(1).any(|a| a.eq_ignore_ascii_case("--unregister")) {
        std::process::exit(unregister());
    }
    if std::env::args().skip(1).any(|a| a.eq_ignore_ascii_case("--make-default")) {
        std::process::exit(make_default());
    }
    // le veilleur de Win+E (lancé avec Windows quand Plume FEx est l'explorateur par défaut) : pas de fenêtre
    if std::env::args().skip(1).any(|a| a.eq_ignore_ascii_case("--hotkey")) {
        std::process::exit(shell::hotkey::run());
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // OLE (et donc COM) : presse-papiers, glisser-déposer, menus du shell
        let _ = OleInitialize(None);
    }
    let set = Settings::load();
    let folders = start_folders();
    let name = instance_name(&set.dir);
    let Ok(_lock) = hand_off(&name, &folders) else { return };
    let first = folders.first().and_then(|p| app::model::open_target(p));
    let start = first.as_ref().map_or(Location::ThisPc, |(dir, _)| Location::Path(dir.clone()));
    let select = first.and_then(|(_, name)| name);
    let gfx = Rc::new(ui::Gfx::new().expect("Direct2D"));
    let theme = Theme::new(set.dark(), Some(set.accent_color()));
    shell::menu::set_dark_menus(theme.dark);
    let o = ui::window::Options {
        title: "Plume FEx".into(),
        size: (1100.0, 720.0),
        min: (480.0, 320.0),
        dark: theme.dark,
        background: theme.bg,
        activate: std::env::var_os("PLUME_FEX_NOACTIVATE").is_none(),
        placement: set.win,
    };
    let g = gfx.clone();
    let win = ui::window::create(&gfx, o, move |win| Box::new(App::new(win, g, set, start))).expect("fenêtre");
    win.mark(&name);
    // les autres dossiers de la ligne de commande : un onglet chacun
    win.with(|h, _| {
        if let Some(app) = h.as_any().downcast_mut::<App>() {
            // un fichier demandé : son dossier s'ouvre avec le fichier sélectionné
            if let Some(name) = select.clone() {
                app.select_when_loaded(name);
            }
            folders.iter().skip(1).for_each(|p| {
                app.open_path_tab(p, false);
            });
        }
    });
    // la fenêtre reçoit les fichiers qu'on y dépose : l'application dit quel dossier est sous le pointeur
    shell::dnd::register(
        win,
        |win, p| win.with(|h, _| h.as_any().downcast_mut::<App>().and_then(|app| app.drop_at(p))).flatten(),
        // un onglet qui montre un serveur : les fichiers déposés y sont envoyés
        |win, files| win.with(|h, _| h.as_any().downcast_mut::<App>().is_some_and(|app| app.remote_drop(files))).unwrap_or(false),
        |win| {
            win.with(|h, _| {
                if let Some(app) = h.as_any().downcast_mut::<App>() {
                    app.check_now();
                }
            });
        },
    );
    #[cfg(feature = "test-hook")]
    test_hook::start(win);
    let index = win.with(|h, _| h.as_any().downcast_mut::<App>().map(|app| app.ix.clone())).flatten();
    ui::window::run();
    // la fenêtre est fermée : le service de l'index enregistre son travail avant que le programme ne se termine
    if let Some(index) = index {
        index.shutdown();
    }
}
