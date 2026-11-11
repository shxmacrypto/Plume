//! Plume Media in Windows' "Open with" menu, for the current user only (HKCU\Software\Classes): two file types
//! (PlumeMedia.Photo, PlumeMedia.Video) offered for each photo and video extension. Windows keeps the user's
//! default app; Plume Media is only one more choice. PLUME_MEDIA_REG_ROOT moves everything under another key
//! (tests write there, never into the real classes).
//!
//!   plume-media.exe --register-open-with      (the installer can call these)
//!   plume-media.exe --unregister-open-with

use crate::image::{PHOTO_EXT, VIDEO_EXT};
use std::path::Path;
use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW,
            RegOpenKeyExW, RegSetValueExW,
        },
        UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify},
    },
    core::PCWSTR,
};

const PHOTO: &str = "PlumeMedia.Photo";
const VIDEO: &str = "PlumeMedia.Video";
const APP: &str = "plume-media.exe";

pub fn root() -> String {
    std::env::var("PLUME_MEDIA_REG_ROOT").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| r"Software\Classes".to_string())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Sets a string value (`name` None: the key's default value), creating the key.
fn set(key: &str, name: Option<&str>, value: &str) -> bool {
    unsafe {
        let mut h = HKEY::default();
        let k = wide(key);
        if RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut h, None) != ERROR_SUCCESS {
            return false;
        }
        let v = wide(value);
        let bytes = std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
        let n = name.map(wide);
        let ok = RegSetValueExW(h, n.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr())), None, REG_SZ, Some(bytes)) == ERROR_SUCCESS;
        let _ = RegCloseKey(h);
        ok
    }
}

fn exists(key: &str) -> bool {
    unsafe {
        let mut h = HKEY::default();
        let k = wide(key);
        let ok = RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), None, KEY_READ, &mut h) == ERROR_SUCCESS;
        if ok {
            let _ = RegCloseKey(h);
        }
        ok
    }
}

fn delete_tree(key: &str) {
    let k = wide(key);
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()));
        let _ = windows::Win32::System::Registry::RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()));
    }
}

fn delete_value(key: &str, name: &str) {
    let (k, n) = (wide(key), wide(name));
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(n.as_ptr()));
    }
}

fn notify() {
    // only the real classes concern Explorer
    if std::env::var_os("PLUME_MEDIA_REG_ROOT").is_none() {
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    }
}

pub fn is_registered() -> bool {
    exists(&format!(r"{}\{PHOTO}\shell\open\command", root()))
}

pub fn register(exe: &Path) -> bool {
    let r = root();
    let exe = exe.display().to_string();
    let cmd = format!("\"{exe}\" \"%1\"");
    let mut ok = true;
    for (id, name, exts) in [(PHOTO, "Photo (Plume Media)", PHOTO_EXT), (VIDEO, "Video (Plume Media)", VIDEO_EXT)] {
        ok &= set(&format!(r"{r}\{id}"), None, name);
        ok &= set(&format!(r"{r}\{id}"), Some("FriendlyTypeName"), name);
        ok &= set(&format!(r"{r}\{id}\DefaultIcon"), None, &format!("\"{exe}\",0"));
        ok &= set(&format!(r"{r}\{id}\shell\open"), Some("FriendlyAppName"), "Plume Media");
        ok &= set(&format!(r"{r}\{id}\shell\open\command"), None, &cmd);
        for e in exts {
            ok &= set(&format!(r"{r}\.{e}\OpenWithProgids"), Some(id), "");
            ok &= set(&format!(r"{r}\Applications\{APP}\SupportedTypes"), Some(&format!(".{e}")), "");
        }
    }
    ok &= set(&format!(r"{r}\Applications\{APP}"), Some("FriendlyAppName"), "Plume Media");
    ok &= set(&format!(r"{r}\Applications\{APP}\shell\open\command"), None, &cmd);
    notify();
    ok
}

pub fn unregister() {
    let r = root();
    for (id, exts) in [(PHOTO, PHOTO_EXT), (VIDEO, VIDEO_EXT)] {
        delete_tree(&format!(r"{r}\{id}"));
        for e in exts {
            delete_value(&format!(r"{r}\.{e}\OpenWithProgids"), id);
        }
    }
    delete_tree(&format!(r"{r}\Applications\{APP}"));
    notify();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes only under a test key of HKCU, then removes it.
    #[test]
    fn register_in_a_test_key() {
        let key = format!(r"Software\PlumeTest\MediaClasses{}", std::process::id());
        // SAFETY: only this test reads the variable while it is set (the other tests do not touch the registry)
        unsafe { std::env::set_var("PLUME_MEDIA_REG_ROOT", &key) };
        assert!(!is_registered());
        assert!(register(Path::new(r"C:\Test\plume-media.exe")));
        assert!(is_registered());
        assert!(exists(&format!(r"{key}\.jpg\OpenWithProgids")));
        unregister();
        assert!(!is_registered());
        delete_tree(&key);
        unsafe { std::env::remove_var("PLUME_MEDIA_REG_ROOT") };
    }
}
