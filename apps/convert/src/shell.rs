//! "Convert with Plume" in Explorer's right-click menu, for the current user only (HKCU). In a test build the key
//! goes under PLUME_CONVERT_REG_ROOT (for example `Software\PlumeConvertTest`) and nowhere else.

use std::path::Path;
use windows::{
    Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW,
        RegSetValueExW,
    },
    core::{HSTRING, PCWSTR},
};

fn root() -> Option<String> {
    let env = std::env::var("PLUME_CONVERT_REG_ROOT").ok().filter(|s| !s.trim().is_empty());
    if cfg!(feature = "test-hook") { env } else { Some(env.unwrap_or_else(|| r"Software\Classes".into())) }
}

fn verb_key() -> Option<String> {
    root().map(|r| format!(r"{r}\*\shell\PlumeConvert"))
}

/// The command Explorer runs: `"…\plume-convert.exe" "%1"`.
pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

fn set(key: HKEY, name: Option<&str>, value: &str) -> Result<(), String> {
    let data: Vec<u8> = value.encode_utf16().chain(Some(0)).flat_map(|u| u.to_le_bytes()).collect();
    let n = name.map(HSTRING::from);
    let name_ptr = n.as_ref().map(|h| PCWSTR(h.as_ptr())).unwrap_or(PCWSTR::null());
    unsafe { RegSetValueExW(key, name_ptr, None, REG_SZ, Some(&data)).ok().map_err(|e| e.message().to_string()) }
}

fn create(path: &str) -> Result<HKEY, String> {
    let mut key = HKEY::default();
    unsafe {
        RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None)
            .ok()
            .map_err(|e| e.message().to_string())?;
    }
    Ok(key)
}

pub fn install(exe: &Path, label: &str) -> Result<(), String> {
    let path = verb_key().ok_or("no test key set")?;
    let k = create(&path)?;
    let r = set(k, Some("MUIVerb"), label).and_then(|_| set(k, Some("Icon"), &format!("\"{}\",0", exe.display())));
    unsafe {
        let _ = RegCloseKey(k);
    }
    r?;
    let c = create(&format!(r"{path}\command"))?;
    let r = set(c, None, &command_line(exe));
    unsafe {
        let _ = RegCloseKey(c);
    }
    r
}

pub fn uninstall() -> Result<(), String> {
    let path = verb_key().ok_or("no test key set")?;
    if !installed() {
        return Ok(());
    }
    unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path)).ok().map_err(|e| e.message().to_string()) }
}

pub fn installed() -> bool {
    let Some(path) = verb_key() else { return false };
    let mut key = HKEY::default();
    unsafe {
        let ok = RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), None, KEY_READ, &mut key).is_ok();
        if ok {
            let _ = RegCloseKey(key);
        }
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command() {
        assert_eq!(command_line(Path::new(r"C:\P\plume-convert.exe")), r#""C:\P\plume-convert.exe" "%1""#);
    }
}
