// PlumeSetup.exe : l'installeur de la suite Plume. Il porte les programmes (Plume Suite, Plume Web et les autres
// applications de la suite, voir `ONES`) et les installe pour l'utilisateur courant, sans droits administrateur, dans
// %LOCALAPPDATA%\Programs\Plume. Une copie de lui-même y reste : Plume Suite s'en sert pour installer une application
// sans rien télécharger. Les mises à jour (de Plume Suite ou de Plume Web) le téléchargent puis le lancent avec --update.
//
//   PlumeSetup.exe                     installe Plume Suite (et met à jour les applications déjà là), puis l'ouvre
//   PlumeSetup.exe --quiet | /s        la même chose, sans rien ouvrir ; --no-desktop : pas de raccourci sur le bureau
//   PlumeSetup.exe --update            met à jour Plume Suite et toutes les applications installées, sans fenêtre
//   PlumeSetup.exe --install <app>     installe une application (utilisé par Plume Suite) : web, fex, music…
//   PlumeSetup.exe --uninstall <app>   la retire ; ses données restent
//   PlumeSetup.exe --uninstall-all [--quiet]   retire toute la suite (Paramètres › Applications) ; les données restent
#![windows_subsystem = "windows"]

use std::{
    env, fs, io,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

static SUITE: &[u8] = include_bytes!(env!("PLUME_SUITE_EXE"));
static WEB: &[u8] = include_bytes!(env!("PLUME_CORE_EXE"));
static DLL: &[u8] = include_bytes!(env!("PLUME_LOADER_DLL"));
#[cfg(plume_mail)]
const MAIL: &[u8] = include_bytes!(env!("PLUME_MAIL_EXE"));
/// Plume Mail ne fait pas partie de cette version : il ne s'installe pas (un ancien reste se retire quand même).
#[cfg(not(plume_mail))]
const MAIL: &[u8] = &[];

/// Une application de la suite qui tient en un seul programme.
struct One {
    /// Le nom de la ligne de commande (`--install music`).
    key: &'static str,
    name: &'static str,
    exe: &'static str,
    bytes: &'static [u8],
    /// Lancé avant de retirer le programme : il défait ce qu'il a fait dans Windows (démarrage avec Windows, « Ouvrir
    /// avec », dossiers…) et quitte s'il tournait près de l'horloge.
    unregister: &'static str,
    /// Les applications dont elle a besoin : installées avec elle (`--install boost` installe aussi Plume Backup).
    needs: &'static [&'static str],
}

/// Dans l'ordre de Plume Suite (et des mises à jour). Plume Mail en dernier : il peut manquer à cette version.
static ONES: [One; 19] = [
    One { key: "fex", name: "Plume FEx", exe: "plume-fex.exe", bytes: include_bytes!(env!("PLUME_FEX_EXE")), unregister: "--unregister", needs: &[] },
    One {
        key: "viewer",
        name: "Plume Viewer",
        exe: "plume-viewer.exe",
        bytes: include_bytes!(env!("PLUME_VIEWER_EXE")),
        unregister: "--unregister",
        needs: &[],
    },
    One { key: "music", name: "Plume Music", exe: "plume-music.exe", bytes: include_bytes!(env!("PLUME_MUSIC_EXE")), unregister: "", needs: &[] },
    One { key: "drop", name: "Plume Drop", exe: "plume-drop.exe", bytes: include_bytes!(env!("PLUME_DROP_EXE")), unregister: "", needs: &[] },
    One {
        key: "launcher",
        name: "Plume Launcher",
        exe: "plume-launcher.exe",
        bytes: include_bytes!(env!("PLUME_LAUNCHER_EXE")),
        unregister: "--unregister",
        needs: &[],
    },
    One {
        key: "passwords",
        name: "Plume Passwords",
        exe: "plume-passwords.exe",
        bytes: include_bytes!(env!("PLUME_PASSWORDS_EXE")),
        unregister: "--unregister",
        needs: &[],
    },
    One {
        key: "media",
        name: "Plume Media",
        exe: "plume-media.exe",
        bytes: include_bytes!(env!("PLUME_MEDIA_EXE")),
        unregister: "--unregister-open-with",
        needs: &[],
    },
    One { key: "task", name: "Plume Task", exe: "plume-task.exe", bytes: include_bytes!(env!("PLUME_TASK_EXE")), unregister: "", needs: &[] },
    One { key: "clean", name: "Plume Clean", exe: "plume-clean.exe", bytes: include_bytes!(env!("PLUME_CLEAN_EXE")), unregister: "", needs: &[] },
    One {
        key: "backup",
        name: "Plume Backup",
        exe: "plume-backup.exe",
        bytes: include_bytes!(env!("PLUME_BACKUP_EXE")),
        unregister: "--unregister",
        needs: &[],
    },
    One {
        key: "clipboard",
        name: "Plume Clipboard",
        exe: "plume-clipboard.exe",
        bytes: include_bytes!(env!("PLUME_CLIPBOARD_EXE")),
        unregister: "--unregister",
        needs: &[],
    },
    One { key: "pdf", name: "Plume PDF", exe: "plume-pdf.exe", bytes: include_bytes!(env!("PLUME_PDF_EXE")), unregister: "", needs: &[] },
    One {
        key: "calendar",
        name: "Plume Calendar",
        exe: "plume-calendar.exe",
        bytes: include_bytes!(env!("PLUME_CALENDAR_EXE")),
        unregister: "--unregister",
        needs: &[],
    },
    One { key: "convert", name: "Plume Convert", exe: "plume-convert.exe", bytes: include_bytes!(env!("PLUME_CONVERT_EXE")), unregister: "", needs: &[] },
    // Plume Notepad rend notepad.exe à Windows (et retire son démarrage avec Windows et ses types de fichiers)
    One {
        key: "notepad",
        name: "Plume Notepad",
        exe: "plume-notepad.exe",
        bytes: include_bytes!(env!("PLUME_NOTEPAD_EXE")),
        unregister: "--unregister",
        needs: &[],
    },
    One { key: "cut", name: "Plume Cut", exe: "plume-cut.exe", bytes: include_bytes!(env!("PLUME_CUT_EXE")), unregister: "", needs: &[] },
    One { key: "create", name: "Plume Create", exe: "plume-create.exe", bytes: include_bytes!(env!("PLUME_CREATE_EXE")), unregister: "", needs: &[] },
    // Plume Boost remet chaque réglage de Windows qu'il avait changé
    One {
        key: "boost",
        name: "Plume Boost",
        exe: "plume-boost.exe",
        bytes: include_bytes!(env!("PLUME_BOOST_EXE")),
        unregister: "--undo-all",
        needs: &["backup"],
    },
    One { key: "mail", name: "Plume Mail", exe: "plume-mail.exe", bytes: MAIL, unregister: "", needs: &[] },
];
/// Celles que Plume Suite peut faire ouvrir les fichiers à la place des applications de Windows (« Set as default »,
/// crates/plume-defaults) : nom dans RegisteredApplications, clé de leurs Capabilities, leurs ProgID. Avant de les
/// retirer, `--unregister-defaults` défait tout ; [`defaults_cleanup`] vérifie ensuite.
const DEFAULTS: [(&str, &str, &str, &[&str]); 6] = [
    ("media", "Plume Media", r"Software\Plume\Media", &["PlumeMedia.Photo", "PlumeMedia.Video"]),
    ("music", "Plume Music", r"Software\Plume\Music", &["PlumeMusic.Audio"]),
    ("pdf", "Plume PDF", r"Software\Plume\PDF", &["PlumePDF.Document"]),
    ("notepad", "Plume Notepad", r"Software\Plume\Notepad", &["PlumeNotepad.txt"]),
    ("cut", "Plume Cut", r"Software\Plume\Cut", &["PlumeCut.Project"]),
    ("create", "Plume Create", r"Software\Plume\Create", &["PlumeCreate.Project"]),
];
const VERSION: &str = env!("CARGO_PKG_VERSION");
const NO_WINDOW: u32 = 0x0800_0000;

#[link(name = "user32", kind = "raw-dylib")]
unsafe extern "system" {
    fn MessageBoxW(hwnd: *mut u8, text: *const u16, caption: *const u16, kind: u32) -> i32;
}
#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}
#[link(name = "advapi32", kind = "raw-dylib")]
unsafe extern "system" {
    fn RegDeleteTreeW(key: isize, subkey: *const u16) -> i32;
    fn RegDeleteKeyW(key: isize, subkey: *const u16) -> i32;
    fn RegDeleteKeyValueW(key: isize, subkey: *const u16, name: *const u16) -> i32;
    fn RegGetValueW(key: isize, subkey: *const u16, name: *const u16, flags: u32, kind: *mut u32, data: *mut u16, bytes: *mut u32) -> i32;
}
#[link(name = "user32", kind = "raw-dylib")]
unsafe extern "system" {
    fn FindWindowExW(parent: isize, after: isize, class: *const u16, title: *const u16) -> isize;
    fn PostMessageW(hwnd: isize, msg: u32, wp: usize, lp: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
}
/// HKEY_CURRENT_USER
const HKCU: isize = 0x8000_0001_u32 as i32 as isize;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Boîte de message ; rend vrai pour « Oui ».
fn alert(text: &str, kind: u32) -> bool {
    unsafe { MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("Plume Suite").as_ptr(), kind) == 6 }
}

fn local_appdata() -> PathBuf {
    env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir)
}

/// Langue choisie dans Plume, sinon celle de Windows.
fn french() -> bool {
    let cfg = fs::read_to_string(local_appdata().join(r"Plume\plume.json")).unwrap_or_default();
    if cfg.contains(r#""lang":"fr""#) || cfg.contains(r#""lang":"en""#) {
        return cfg.contains(r#""lang":"fr""#);
    }
    unsafe { GetUserDefaultUILanguage() & 0x3ff == 0x0c }
}

/// Essais : PLUME_SETUP_SANDBOX évite tout ce qui touche Windows au-delà des fichiers (registre, raccourcis).
fn sandbox() -> bool {
    env::var_os("PLUME_SETUP_SANDBOX").is_some()
}

/// Plume Web (le navigateur, qui a sa propre installation) ou l'une des applications d'un seul programme.
#[derive(Clone, Copy, PartialEq)]
enum App {
    Web,
    One(usize),
}

impl App {
    fn parse(s: &str) -> Option<App> {
        if s == "web" {
            return Some(App::Web);
        }
        let i = ONES.iter().position(|o| o.key == s)?;
        // une application absente de cette version (Plume Mail) ne s'installe pas, mais un ancien reste se retire
        (!ONES[i].bytes.is_empty() || std::env::args().any(|a| a.eq_ignore_ascii_case("--uninstall"))).then_some(App::One(i))
    }
    fn key(self) -> &'static str {
        match self {
            App::Web => "web",
            App::One(i) => ONES[i].key,
        }
    }
    fn name(self) -> &'static str {
        match self {
            App::Web => "Plume Web",
            App::One(i) => ONES[i].name,
        }
    }
    fn exe(self, dir: &Path) -> PathBuf {
        dir.join(match self {
            App::Web => "plume.exe",
            App::One(i) => ONES[i].exe,
        })
    }
    /// Les applications à installer avec elle.
    fn needs(self) -> &'static [&'static str] {
        match self {
            App::Web => &[],
            App::One(i) => ONES[i].needs,
        }
    }
    fn installed(self, dir: &Path) -> bool {
        self.exe(dir).is_file()
    }
    /// Écrit les fichiers de l'application. Vrai si un ancien fichier tournait encore (redémarrage à faire).
    fn write(self, dir: &Path) -> io::Result<bool> {
        Ok(match self {
            App::Web => put(&dir.join("WebView2Loader.dll"), DLL)? | put(&self.exe(dir), WEB)?,
            App::One(i) if ONES[i].bytes.is_empty() => false,
            App::One(i) => put(&self.exe(dir), ONES[i].bytes)?,
        })
    }
}

/// Toutes les applications : Plume Web, puis celles de `ONES`.
fn apps() -> impl Iterator<Item = App> {
    std::iter::once(App::Web).chain((0..ONES.len()).map(App::One))
}

/// Écrit un fichier même s'il est en cours d'utilisation : un exe ou une DLL chargés ne peuvent pas être
/// remplacés, mais peuvent être renommés. L'ancien est mis de côté (*.old), effacé au démarrage suivant.
/// Renvoie vrai si l'ancien fichier était encore utilisé (programme ouvert).
fn put(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    if fs::metadata(path).is_ok_and(|m| m.len() == bytes.len() as u64) && fs::read(path).is_ok_and(|b| b == bytes) {
        return Ok(false);
    }
    let new = path.with_extension("new");
    fs::write(&new, bytes)?;
    let mut in_use = false;
    if path.exists() {
        let moved = (0..10).any(|n| {
            let old = path.with_extension(if n == 0 { "old".into() } else { format!("old{n}") });
            let _ = fs::remove_file(&old);
            if fs::rename(path, &old).is_ok() {
                in_use = fs::remove_file(&old).is_err(); // impossible à effacer : il tourne encore
                true
            } else {
                false
            }
        });
        if !moved {
            let _ = fs::remove_file(&new);
            return Err(io::Error::other("file in use"));
        }
    }
    fs::rename(&new, path).map(|_| in_use)
}

/// Lance un programme du dossier et attend sa fin. Rend son code de sortie (-1 s'il n'a pas pu démarrer).
fn run(exe: &Path, args: &[&str]) -> i32 {
    Command::new(exe).args(args).current_dir(exe.parent().unwrap_or(Path::new("."))).status().map_or(-1, |s| s.code().unwrap_or(-1))
}

/// Plume Suite elle-même, et la copie de cet installeur (pour installer une application plus tard sans télécharger).
fn write_suite(dir: &Path) -> io::Result<bool> {
    fs::create_dir_all(dir)?;
    let running = put(&dir.join("plume-suite.exe"), SUITE)?;
    let me = env::current_exe()?;
    let copy = dir.join("PlumeSetup.exe");
    let same = |a: &Path, b: &Path| a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy());
    if !same(&me, &copy) {
        put(&copy, &fs::read(&me)?)?;
    }
    Ok(running)
}

/// Raccourcis et entrées dans Windows : Plume Web s'occupe des siens (et de ce qui fait de lui un navigateur),
/// Plume Suite des autres et de l'entrée de Paramètres › Applications. Ce dernier passe en second : l'entrée est la sienne.
fn register(dir: &Path, desktop: bool) -> bool {
    if sandbox() {
        return true;
    }
    let mut ok = true;
    if App::Web.installed(dir) {
        ok &= run(&App::Web.exe(dir), &["--register"]) == 0;
    }
    let suite = dir.join("plume-suite.exe");
    ok &= run(&suite, if desktop { &["--register", "--desktop"] } else { &["--register"] }) == 0;
    ok
}

/// Valeur texte d'une clé de HKCU (`name` vide : la valeur par défaut).
fn reg_text(key: &str, name: &str) -> Option<String> {
    let mut buf = vec![0u16; 1024];
    let mut bytes = (buf.len() * 2) as u32;
    // 2 : RRF_RT_REG_SZ
    let r = unsafe { RegGetValueW(HKCU, wide(key).as_ptr(), wide(name).as_ptr(), 2, std::ptr::null_mut(), buf.as_mut_ptr(), &mut bytes) };
    (r == 0).then(|| String::from_utf16_lossy(&buf[..(bytes as usize / 2).min(buf.len())]).trim_end_matches('\0').to_string())
}

/// Plume FEx retiré : tout ce qui faisait de lui l'explorateur de Windows s'en va, même si son propre `--unregister`
/// n'a pas pu tourner (programme déjà effacé, réglages perdus). Ses entrées seulement : un autre choix de
/// l'utilisateur reste en place. Les dossiers, les lecteurs et Win+E reviennent à l'Explorateur de Windows.
fn fex_cleanup() {
    const CLASSES: &str = r"Software\Classes";
    unsafe {
        // le veilleur de Win+E s'arrête (il tient le programme ouvert)
        let helper = FindWindowExW(-3, 0, wide("PlumeFExWinE").as_ptr(), std::ptr::null()); // -3 : HWND_MESSAGE
        if helper != 0 {
            PostMessageW(helper, 0x0010, 0, 0); // WM_CLOSE
            for _ in 0..30 {
                if IsWindow(helper) == 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        RegDeleteKeyValueW(HKCU, wide(r"Software\Microsoft\Windows\CurrentVersion\Run").as_ptr(), wide("Plume FEx Win+E").as_ptr());
        for place in ["Directory", "Drive"] {
            let shell = format!(r"{CLASSES}\{place}\shell");
            if reg_text(&shell, "").is_some_and(|v| v.eq_ignore_ascii_case("PlumeFEx")) {
                RegDeleteKeyValueW(HKCU, wide(&shell).as_ptr(), wide("").as_ptr());
            }
        }
        for place in ["Directory", r"Directory\Background", "Drive"] {
            RegDeleteTreeW(HKCU, wide(&format!(r"{CLASSES}\{place}\shell\PlumeFEx")).as_ptr());
            RegDeleteKeyW(HKCU, wide(&format!(r"{CLASSES}\{place}\shell\PlumeFEx")).as_ptr());
        }
        let clsid = format!(r"{CLASSES}\CLSID\{{52205fd8-5dfb-447d-801a-d0b52f2e83e1}}");
        let win_e = format!(r"{clsid}\shell\opennewwindow");
        if reg_text(&format!(r"{win_e}\command"), "").is_some_and(|c| c.to_lowercase().contains("plume-fex.exe")) {
            RegDeleteTreeW(HKCU, wide(&win_e).as_ptr());
            RegDeleteKeyW(HKCU, wide(&win_e).as_ptr());
            // les clés parentes, si elles sont vides (refusé sinon : rien d'autre n'est touché)
            RegDeleteKeyW(HKCU, wide(&format!(r"{clsid}\shell")).as_ptr());
            RegDeleteKeyW(HKCU, wide(&clsid).as_ptr());
        }
    }
}

/// Une application qui ouvrait des fichiers à la place de celles de Windows est retirée : ses types de fichiers, ses
/// Capabilities et son entrée dans RegisteredApplications s'en vont même si son `--unregister-defaults` n'a pas pu
/// tourner. Sans son ProgID, Windows revient de lui-même à ses propres applications pour ces fichiers.
fn defaults_cleanup(name: &str, key: &str, progids: &[&str]) {
    unsafe {
        RegDeleteKeyValueW(HKCU, wide(r"Software\RegisteredApplications").as_ptr(), wide(name).as_ptr());
        let caps = format!(r"{key}\Capabilities");
        RegDeleteTreeW(HKCU, wide(&caps).as_ptr());
        RegDeleteKeyW(HKCU, wide(&caps).as_ptr());
        // (les clés parentes restent : `--unregister-defaults` les retire quand elles sont vides)
        for id in progids {
            let k = format!(r"Software\Classes\{id}");
            RegDeleteTreeW(HKCU, wide(&k).as_ptr());
            RegDeleteKeyW(HKCU, wide(&k).as_ptr());
        }
    }
}

/// Retire une application. Rend un code de sortie (0 : retirée).
fn uninstall(app: App, dir: &Path, quiet: bool, fr: bool) -> i32 {
    if !app.installed(dir) {
        return 0;
    }
    match app {
        // le navigateur a sa propre désinstallation (navigateur par défaut, raccourci, données à garder ou non)
        App::Web => {
            if sandbox() {
                let _ = fs::remove_file(App::Web.exe(dir));
                let _ = fs::remove_file(dir.join("WebView2Loader.dll"));
                return 0;
            }
            let code = run(&App::Web.exe(dir), if quiet { &["--uninstall", "--quiet"] } else { &["--uninstall"] });
            if code != 0 {
                return code;
            }
        }
        App::One(i) => {
            // Plume FEx rend les dossiers à l'Explorateur ; celles qui tournent près de l'horloge quittent et ne
            // démarrent plus avec Windows ; Plume Media n'est plus proposé dans « Ouvrir avec »
            // Celles qui ouvraient des fichiers à la place de Windows les lui rendent d'abord.
            let defaults = DEFAULTS.iter().find(|d| d.0 == app.key());
            if defaults.is_some() && !sandbox() {
                run(&app.exe(dir), &["--unregister-defaults"]);
            }
            if !ONES[i].unregister.is_empty() && !sandbox() {
                run(&app.exe(dir), &[ONES[i].unregister]);
            }
            if app.key() == "fex" && !sandbox() {
                fex_cleanup();
            }
            // « Convertir avec Plume » dans le menu du clic droit de l'Explorateur
            if app.key() == "convert" && !sandbox() {
                unsafe { RegDeleteTreeW(HKCU, wide(r"Software\Classes\*\shell\PlumeConvert").as_ptr()) };
            }
            // le programme qui vient de quitter peut tenir son fichier quelques instants
            let removed = (0..20).any(|_| {
                if fs::remove_file(app.exe(dir)).is_ok() {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
                false
            });
            if !removed {
                if !quiet {
                    let msg = if fr { format!("Fermez {}, puis réessayez.", app.name()) } else { format!("Close {}, then try again.", app.name()) };
                    alert(&msg, 0x30);
                }
                return 4;
            }
            // Plume FEx parti : les dossiers et Win+E reviennent à coup sûr à l'Explorateur de Windows
            if app.key() == "fex" && !sandbox() {
                fex_cleanup();
            }
            // de même, les photos, la musique, les PDF reviennent aux applications de Windows
            if let Some((_, name, key, progids)) = defaults
                && !sandbox()
            {
                defaults_cleanup(name, key, progids);
            }
        }
    }
    if dir.join("plume-suite.exe").is_file() && !sandbox() {
        run(&dir.join("plume-suite.exe"), &["--register"]);
    }
    0
}

/// Retire toute la suite (Paramètres › Applications › Plume Suite › Désinstaller). Les données restent.
fn uninstall_all(dir: &Path, quiet: bool, fr: bool) -> i32 {
    if !quiet {
        let ask = if fr {
            "Désinstaller Plume Suite et ses applications ?\n\nVos données (favoris, mots de passe, courrier, réglages) restent sur ce PC."
        } else {
            "Uninstall Plume Suite and its apps?\n\nYour data (favorites, passwords, mail, settings) stays on this PC."
        };
        if !alert(ask, 0x24 | 0x100) {
            return 1;
        }
    }
    for app in apps() {
        let code = uninstall(app, dir, true, fr);
        if code != 0 {
            if !quiet {
                let msg = if fr {
                    format!("Fermez {}, puis relancez la désinstallation.", app.name())
                } else {
                    format!("Close {}, then uninstall again.", app.name())
                };
                alert(&msg, 0x30);
            }
            return code;
        }
    }
    if !sandbox() {
        run(&dir.join("plume-suite.exe"), &["--unregister"]);
    }
    // cet installeur tourne depuis le dossier : cmd l'efface une fois fermé
    let _ = Command::new("cmd")
        .raw_arg(format!("/c ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"", dir.display()))
        .current_dir(env::temp_dir())
        .creation_flags(NO_WINDOW)
        .spawn();
    if !quiet {
        alert(if fr { "Plume Suite a été désinstallée." } else { "Plume Suite has been uninstalled." }, 0x40);
    }
    0
}

fn main() {
    let args: Vec<String> = env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let after = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).and_then(|a| App::parse(a));
    let fr = french();
    let dir = local_appdata().join(r"Programs\Plume");
    let quiet = has("--quiet") || has("/s");

    if has("--uninstall-all") {
        std::process::exit(uninstall_all(&dir, quiet, fr));
    }
    if has("--uninstall") {
        let Some(app) = after("--uninstall") else { std::process::exit(2) };
        std::process::exit(uninstall(app, &dir, quiet, fr));
    }

    let update = has("--update");
    let install = after("--install");
    // une application inconnue, ou qui n'est pas dans cette version
    if has("--install") && install.is_none() {
        std::process::exit(2);
    }
    let first = !dir.join("plume-suite.exe").is_file();
    // Plume Suite, puis les applications déjà installées (une mise à jour les prend toutes), puis celle demandée
    let mut running = false;
    let written = write_suite(&dir).and_then(|r| {
        running |= r;
        for app in apps() {
            // celle demandée, et celles dont elle a besoin (Plume Boost garde ses réglages avec Plume Backup)
            let needed = install.is_some_and(|i| i.needs().contains(&app.key()));
            if app.installed(&dir) || install == Some(app) || needed {
                running |= app.write(&dir)?;
            }
        }
        Ok(())
    });
    if let Err(e) = written {
        if !(quiet || update || install.is_some()) {
            let msg = if fr { format!("Impossible d'installer Plume Suite :\n{e}") } else { format!("Plume Suite could not be installed:\n{e}") };
            alert(&msg, 0x10);
        }
        std::process::exit(2);
    }
    let desktop = first && !update && install.is_none() && !has("--no-desktop");
    let registered = register(&dir, desktop);
    // Plume FEx qu'on vient d'installer ouvre les dossiers et Win+E (son menu, ou son retrait, le défait)
    if let Some(app) = install.filter(|a| a.key() == "fex" && !sandbox()) {
        run(&app.exe(&dir), &["--make-default"]);
    }
    if update || install.is_some() {
        std::process::exit(if registered { 0 } else { 3 });
    }
    if quiet {
        return;
    }
    if running {
        let msg = if fr {
            format!("Plume Suite {VERSION} est installée.\n\nLes applications ouvertes prendront la nouvelle version à leur prochain lancement.")
        } else {
            format!("Plume Suite {VERSION} is installed.\n\nOpen apps will use the new version the next time you start them.")
        };
        alert(&msg, 0x40);
    }
    let _ = Command::new(dir.join("plume-suite.exe")).current_dir(&dir).spawn();
}
