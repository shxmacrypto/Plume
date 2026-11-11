// Navigateur par défaut : Plume s'enregistre auprès de Windows (HKCU, sans droits administrateur, comme un
// navigateur installé pour un seul utilisateur), lit le choix actuel, ouvre la page « Applications par défaut »,
// et un deuxième lancement (lien cliqué dans une autre application) passe son lien à la fenêtre déjà ouverte.
use std::{path::Path, sync::OnceLock};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_ABANDONED, WAIT_OBJECT_0, WPARAM},
        System::{
            Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegDeleteKeyW, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW},
            Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
        },
        UI::{
            Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify, ShellExecuteW},
            WindowsAndMessaging::{
                AllowSetForegroundWindow, EnumWindows, GetPropW, GetWindowThreadProcessId, IsWindowVisible, RemovePropW, SMTO_ABORTIFHUNG, SW_SHOWNORMAL,
                SendMessageTimeoutW, SetPropW, WM_COPYDATA,
            },
        },
    },
    core::{BOOL, HSTRING, PCWSTR, w},
};

/// Type de document de Plume (liens http/https, pages .html) dans le registre.
pub const PROG_ID: &str = "PlumeHTML";
const CLASSES: &str = r"Software\Classes\PlumeHTML";
const CLIENT: &str = r"Software\Clients\StartMenuInternet\Plume";
const REGISTERED: &str = r"Software\RegisteredApplications";
const DESC: &str = "Light browser for Windows";
/// Fichiers que Plume propose d'ouvrir (Windows ne change rien tant que l'utilisateur ne le choisit pas).
const FILES: [&str; 5] = [".htm", ".html", ".shtml", ".xht", ".xhtml"];
/// Marque du message WM_COPYDATA qui passe un lien à la fenêtre ouverte (« PLUM »).
const REMOTE_TAG: usize = 0x504C_554D;
/// Propriété posée sur chaque fenêtre Plume : empreinte du dossier de données (un profil = une instance).
const PROP: PCWSTR = w!("PlumeProfile");

/// Même disposition que COPYDATASTRUCT (évite une fonctionnalité de plus du crate windows).
#[repr(C)]
struct CopyData {
    tag: usize,
    len: u32,
    ptr: *const u8,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCommandLineW() -> *const u16;
}

fn set(key: &str, name: Option<&str>, value: &str) -> bool {
    let data: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let name = name.map(HSTRING::from);
    let name = name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), name, REG_SZ.0, Some(data.as_ptr() as _), (data.len() * 2) as u32).is_ok() }
}

fn get(key: &str, name: &str) -> Option<String> {
    let mut buf = [0u16; 260];
    let mut len = (buf.len() * 2) as u32;
    let ok = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as _), Some(&mut len)).is_ok()
    };
    ok.then(|| String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())]))
}

fn notify() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) }
}

/// Plume apparaît dans Paramètres › Applications par défaut et dans « Ouvrir avec ».
/// Windows passe le lien après --single-argument : tout le reste de la ligne de commande est l'adresse,
/// jamais une option (un lien piégé ne peut pas y glisser --uninstall).
pub fn register(exe: &Path) -> bool {
    let e = exe.display().to_string();
    let icon = format!("{e},0");
    let cap = format!(r"{CLIENT}\Capabilities");
    let app = format!(r"{CLASSES}\Application");
    let mut ok = set(CLASSES, None, "Plume HTML Document")
        & set(&format!(r"{CLASSES}\DefaultIcon"), None, &icon)
        & set(&format!(r"{CLASSES}\shell\open\command"), None, &format!("\"{e}\" --single-argument %1"))
        & set(&app, Some("ApplicationName"), "Plume")
        & set(&app, Some("ApplicationIcon"), &icon)
        & set(&app, Some("ApplicationDescription"), DESC)
        & set(CLIENT, None, "Plume")
        & set(&format!(r"{CLIENT}\DefaultIcon"), None, &icon)
        & set(&format!(r"{CLIENT}\shell\open\command"), None, &format!("\"{e}\""))
        & set(&cap, Some("ApplicationName"), "Plume")
        & set(&cap, Some("ApplicationDescription"), DESC)
        & set(&cap, Some("ApplicationIcon"), &icon)
        & set(&format!(r"{cap}\StartMenu"), Some("StartMenuInternet"), "Plume");
    for f in FILES {
        ok &= set(&format!(r"{cap}\FileAssociations"), Some(f), PROG_ID);
        // « Ouvrir avec » de l'Explorateur
        ok &= set(&format!(r"Software\Classes\{f}\OpenWithProgids"), Some(PROG_ID), "");
    }
    for u in ["http", "https"] {
        ok &= set(&format!(r"{cap}\URLAssociations"), Some(u), PROG_ID);
    }
    ok &= set(REGISTERED, Some("Plume"), &cap);
    notify();
    ok
}

/// Désinstallation : Plume disparaît des applications par défaut.
pub fn unregister() {
    unsafe {
        for k in [CLASSES, CLIENT] {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(k));
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(k));
        }
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(REGISTERED), &HSTRING::from("Plume"));
        // seulement notre entrée : la clé OpenWithProgids sert aussi aux autres applications
        for f in FILES {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(format!(r"Software\Classes\{f}\OpenWithProgids")), &HSTRING::from(PROG_ID));
        }
    }
    notify();
}

/// Plume ouvre-t-il les liens https ? Windows 11 récent range le choix dans UserChoiceLatest\ProgId
/// (l'ancienne clé UserChoice peut alors être périmée).
pub fn is_default() -> bool {
    let base = r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https";
    [r"UserChoiceLatest\ProgId", "UserChoiceLatest", "UserChoice"]
        .iter()
        .find_map(|k| get(&format!(r"{base}\{k}"), "ProgId"))
        .is_some_and(|p| p.eq_ignore_ascii_case(PROG_ID))
}

/// Paramètres › Applications par défaut › Plume (bouton « Définir par défaut » sous Windows 11).
pub fn open_settings() {
    unsafe {
        ShellExecuteW(None, w!("open"), w!("ms-settings:defaultapps?registeredAppUser=Plume"), None, None, SW_SHOWNORMAL);
    }
}

/// Lancement par Windows (lien ou fichier ouvert depuis une autre application) : l'adresse après
/// --single-argument, prise telle quelle dans la ligne de commande. None : lancement ordinaire.
pub fn single_argument() -> Option<String> {
    let raw = unsafe {
        let p = GetCommandLineW();
        if p.is_null() {
            return None;
        }
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    };
    let (_, rest) = raw.split_once(" --single-argument ")?;
    let u = rest.trim();
    Some(u.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(u).to_string())
}

fn profile_key() -> usize {
    static K: OnceLock<usize> = OnceLock::new();
    *K.get_or_init(|| {
        let d = crate::setup::data_dir().to_string_lossy().to_lowercase();
        let h = d.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
        (h | 1) as usize
    })
}

/// Démarrage réservé sur ce profil, rendu quand la fenêtre est prête (ou à la fin du processus).
pub struct StartLock(HANDLE);

impl Drop for StartLock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

/// Un seul Plume démarre à la fois sur un profil : plusieurs liens ou fichiers ouverts d'un coup, Plume fermé,
/// lancent plusieurs plume.exe. Le premier démarre ; les suivants attendent sa fenêtre, puis lui passent leur lien.
/// Abandonné (le précédent a planté pendant son démarrage) : on le prend quand même. None après 15 s.
pub fn start_lock() -> Option<StartLock> {
    unsafe {
        let h = CreateMutexW(None, false, &HSTRING::from(format!(r"Local\PlumeStart-{:x}", profile_key()))).ok()?;
        let r = WaitForSingleObject(h, 15_000);
        if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
            Some(StartLock(h))
        } else {
            let _ = CloseHandle(h);
            None
        }
    }
}

/// Chaque fenêtre Plume porte l'empreinte de son profil (retirée quand elle est détruite).
pub fn mark_window(h: HWND) {
    unsafe {
        let _ = SetPropW(h, PROP, Some(HANDLE(profile_key() as _)));
    }
}

pub fn unmark_window(h: HWND) {
    unsafe {
        let _ = RemovePropW(h, PROP);
    }
}

unsafe extern "system" fn find(h: HWND, lp: LPARAM) -> BOOL {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if GetPropW(h, PROP).0 as usize == profile_key() && pid != std::process::id() && IsWindowVisible(h).as_bool() {
            *(lp.0 as *mut HWND) = h;
            return BOOL(0); // la plus haute dans l'ordre d'affichage : la dernière utilisée
        }
        BOOL(1)
    }
}

/// Plume déjà ouvert sur ce profil : il reçoit le lien (vide : il passe juste au premier plan)
/// et ce lancement-ci s'arrête. false : aucun Plume ne répond, on démarre normalement.
pub fn hand_off(url: &str) -> bool {
    let mut h = HWND::default();
    unsafe {
        let _ = EnumWindows(Some(find), LPARAM(&mut h as *mut HWND as isize));
        if h.is_invalid() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        let _ = AllowSetForegroundWindow(pid); // ce lancement vient d'un clic : il peut céder le premier plan
        let cd = CopyData { tag: REMOTE_TAG, len: url.len() as u32, ptr: url.as_ptr() };
        let mut res = 0usize;
        let r = SendMessageTimeoutW(h, WM_COPYDATA, WPARAM(0), LPARAM(&cd as *const CopyData as isize), SMTO_ABORTIFHUNG, 5000, Some(&mut res));
        r.0 != 0 && res == 1
    }
}

/// Lien reçu par WM_COPYDATA (procédure de la fenêtre), None si le message ne vient pas de Plume.
///
/// # Safety
/// `lp` doit être le LPARAM d'un message WM_COPYDATA en cours de traitement.
pub unsafe fn received(lp: LPARAM) -> Option<String> {
    let cd = unsafe { (lp.0 as *const CopyData).as_ref()? };
    if cd.tag != REMOTE_TAG || cd.len > 64 * 1024 {
        return None;
    }
    if cd.len == 0 {
        return Some(String::new()); // Plume relancé sans lien : la fenêtre passe au premier plan
    }
    if cd.ptr.is_null() {
        return None;
    }
    let b = unsafe { std::slice::from_raw_parts(cd.ptr, cd.len as usize) };
    Some(String::from_utf8_lossy(b).into_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn profile_key_is_stable_and_nonzero() {
        let k = super::profile_key();
        assert_ne!(k, 0);
        assert_eq!(k, super::profile_key());
    }

    #[test]
    fn reads_the_current_default_without_writing() {
        // lecture seule : Plume (build de test) n'est pas le navigateur par défaut de la machine de développement
        let _ = super::is_default();
    }
}
