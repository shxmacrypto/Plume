//! Windows around the window: the icon near the clock and its messages, the start with Windows, local time, and
//! drives (a USB drive is recognized by its volume serial number, whatever letter Windows gives it).

use std::path::{Component, Path, PathBuf, Prefix};
use windows::{
    Win32::{
        Foundation::{FILETIME, HWND, LPARAM, POINT, SYSTEMTIME, WPARAM},
        Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW},
        System::{
            Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX, SetThreadErrorMode},
            LibraryLoader::GetModuleHandleW,
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegGetValueW, RegSetValueExW,
            },
            Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
        },
        UI::{
            Shell::{
                NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO, NIIF_NOSOUND, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, PCWSTR},
};

/// The message the icon sends to the window.
pub const WM_TRAY: u32 = WM_APP + 61;
pub const MENU_OPEN: u32 = 1;
pub const MENU_RUN: u32 = 2;
pub const MENU_QUIT: u32 = 3;

/// %LOCALAPPDATA%\Plume\Backup (PLUME_BACKUP_DIR for tests).
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_BACKUP_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("Backup")
}

/// A test run (test socket): no balloon on the real desktop.
pub fn testing() -> bool {
    cfg!(feature = "test-hook") && std::env::var_os("PLUME_BACKUP_TEST_PORT").is_some()
}

// ───────────────────────── the icon near the clock ─────────────────────────

fn copy_into(dst: &mut [u16], s: &str) {
    for (i, u) in s.encode_utf16().take(dst.len() - 1).enumerate() {
        dst[i] = u;
    }
}

fn data(hwnd: HWND, tip: &str) -> NOTIFYICONDATAW {
    let mut d = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        ..Default::default()
    };
    unsafe {
        if let Ok(inst) = GetModuleHandleW(None) {
            d.hIcon = LoadIconW(Some(inst.into()), PCWSTR(1 as _)).unwrap_or_default();
        }
    }
    copy_into(&mut d.szTip, tip);
    d
}

pub fn tray_add(hwnd: HWND, tip: &str) {
    unsafe {
        let d = data(hwnd, tip);
        if !Shell_NotifyIconW(NIM_ADD, &d).as_bool() {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
        }
    }
}

pub fn tray_tip(hwnd: HWND, tip: &str) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data(hwnd, tip));
    }
}

pub fn tray_remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data(hwnd, ""));
    }
}

/// A notification from the icon (a toast on Windows 10 and 11), silent.
pub fn balloon(hwnd: HWND, title: &str, text: &str, error: bool) {
    let mut d = data(hwnd, "");
    d.uFlags = NIF_INFO;
    copy_into(&mut d.szInfoTitle, title);
    copy_into(&mut d.szInfo, text);
    d.dwInfoFlags = (if error { NIIF_ERROR } else { NIIF_INFO }) | NIIF_NOSOUND;
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
    }
}

/// The icon's menu; returns the chosen command (0: none).
pub fn tray_menu(hwnd: HWND, open: &str, run: &str, quit: &str) -> u32 {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        let _ = AppendMenuW(menu, MF_STRING, MENU_OPEN as usize, &HSTRING::from(open));
        let _ = AppendMenuW(menu, MF_STRING, MENU_RUN as usize, &HSTRING::from(run));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT as usize, &HSTRING::from(quit));
        let _ = SetMenuDefaultItem(menu, MENU_OPEN, 0);
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        // the menu closes when clicking elsewhere only if the window is in front
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, p.x, p.y, Some(0), hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        cmd.0 as u32
    }
}

pub fn show(hwnd: HWND, on: bool) {
    unsafe {
        let _ = ShowWindow(hwnd, if on { SW_SHOW } else { SW_HIDE });
        if on {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
pub fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

// ───────────────────────── start with Windows ─────────────────────────

/// The Run key (tests use another one).
fn run_key() -> String {
    #[cfg(feature = "test-hook")]
    if let Ok(k) = std::env::var("PLUME_BACKUP_RUN_KEY") {
        return k;
    }
    r"Software\Microsoft\Windows\CurrentVersion\Run".to_string()
}
const RUN_VALUE: &str = "Plume Backup";

pub fn autostart() -> bool {
    let mut size = 0u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(run_key()), &HSTRING::from(RUN_VALUE), RRF_RT_REG_SZ, None, None, Some(&mut size)).is_ok() }
}

pub fn set_autostart(on: bool) -> bool {
    unsafe {
        if !on {
            let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(run_key()), &HSTRING::from(RUN_VALUE));
            return r.is_ok() || !autostart();
        }
        let Ok(exe) = std::env::current_exe() else { return false };
        let cmd = format!("\"{}\" --background", exe.display());
        let mut key = HKEY::default();
        // opens the key, or creates it (a test key may not exist yet)
        if RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(run_key()), None, None, Default::default(), KEY_SET_VALUE, None, &mut key, None).is_err() {
            return false;
        }
        let wide: Vec<u16> = cmd.encode_utf16().chain(Some(0)).collect();
        let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
        let ok = RegSetValueExW(key, &HSTRING::from(RUN_VALUE), None, REG_SZ, Some(bytes)).is_ok();
        let _ = RegCloseKey(key);
        ok
    }
}

// ───────────────────────── local time ─────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Local {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    /// 0 Sunday … 6 Saturday.
    pub weekday: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

pub fn local(unix: u64) -> Local {
    let ft100 = (unix + 11_644_473_600) * 10_000_000;
    let ft = FILETIME { dwLowDateTime: ft100 as u32, dwHighDateTime: (ft100 >> 32) as u32 };
    let (mut utc, mut loc) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        if FileTimeToSystemTime(&ft, &mut utc).is_err() || SystemTimeToTzSpecificLocalTime(None, &utc, &mut loc).is_err() {
            loc = utc;
        }
    }
    Local { year: loc.wYear, month: loc.wMonth, day: loc.wDay, weekday: loc.wDayOfWeek, hour: loc.wHour, minute: loc.wMinute, second: loc.wSecond }
}

/// Unix time of the local midnight that starts the day of `unix`.
pub fn day_start(unix: u64) -> u64 {
    let l = local(unix);
    unix.saturating_sub(l.hour as u64 * 3600 + l.minute as u64 * 60 + l.second as u64)
}

/// The name of a run: "2026-11-11 14-30-05" (sorts by date, valid as a folder name).
pub fn stamp(unix: u64) -> String {
    let l = local(unix);
    format!("{:04}-{:02}-{:02} {:02}-{:02}-{:02}", l.year, l.month, l.day, l.hour, l.minute, l.second)
}

// ───────────────────────── drives ─────────────────────────

/// What kind of place a destination is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Removable,
    Fixed,
    Network,
    Unknown,
}

/// "E:" for E:\Backups.
fn drive_letter(p: &Path) -> Option<char> {
    match p.components().next() {
        Some(Component::Prefix(x)) => match x.kind() {
            Prefix::Disk(l) | Prefix::VerbatimDisk(l) => Some((l as char).to_ascii_uppercase()),
            _ => None,
        },
        _ => None,
    }
}

fn root_of(letter: char) -> HSTRING {
    HSTRING::from(format!("{letter}:\\"))
}

pub fn place(p: &Path) -> Place {
    if p.to_string_lossy().starts_with(r"\\") {
        return Place::Network;
    }
    let Some(l) = drive_letter(p) else { return Place::Unknown };
    match unsafe { GetDriveTypeW(&root_of(l)) } {
        2 => Place::Removable,
        3 => Place::Fixed,
        4 => Place::Network,
        _ => Place::Unknown,
    }
}

/// Serial number and label of the volume of drive `letter`.
#[allow(dead_code)]
fn volume(letter: char) -> Option<(u32, String)> {
    let mut serial = 0u32;
    let mut label = [0u16; 64];
    unsafe {
        // no "insert a disk" box for an empty card reader
        let mut old = Default::default();
        let _ = SetThreadErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX, Some(&mut old));
        let r = GetVolumeInformationW(&root_of(letter), Some(&mut label), Some(&mut serial), None, None, None);
        let _ = SetThreadErrorMode(old, None);
        r.ok()?;
    }
    let len = label.iter().position(|&c| c == 0).unwrap_or(label.len());
    Some((serial, String::from_utf16_lossy(&label[..len])))
}

/// The serial number of the volume holding `p` (None for a network share).
pub fn serial_of(p: &Path) -> Option<u32> {
    volume(drive_letter(p)?).map(|v| v.0)
}

/// Where the destination is now: the same path, or the same folder on the drive with this serial number under
/// another letter. None: not reachable (unplugged, share offline). Can take a while for a share: not on the UI thread.
pub fn resolve(dest: &Path, serial: Option<u32>) -> Option<PathBuf> {
    if dest.as_os_str().is_empty() {
        return None;
    }
    let here = drive_letter(dest);
    if let (Some(s), Some(l)) = (serial, here) {
        if volume(l).is_some_and(|v| v.0 == s) {
            return dest.is_dir().then(|| dest.to_path_buf());
        }
        let mask = unsafe { GetLogicalDrives() };
        for i in 2..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            if letter != l && volume(letter).is_some_and(|v| v.0 == s) {
                let rest: PathBuf = dest.components().skip(1).collect();
                let moved = PathBuf::from(format!("{letter}:\\")).join(rest);
                return moved.is_dir().then_some(moved);
            }
        }
        return None;
    }
    dest.is_dir().then(|| dest.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_and_days() {
        let now = crate::engine::now();
        let s = stamp(now);
        assert_eq!(s.len(), 19);
        assert_eq!(&s[4..5], "-");
        let start = day_start(now);
        assert!(start <= now && now - start < 86_400 + 3600);
        assert_eq!(local(start).hour, 0);
    }

    #[test]
    fn letters_and_places() {
        assert_eq!(drive_letter(Path::new(r"e:\Backups")), Some('E'));
        assert_eq!(drive_letter(Path::new(r"\\pc\share")), None);
        assert_eq!(place(Path::new(r"\\pc\share\x")), Place::Network);
        // the system drive is there and has a serial number
        let sys = std::env::var("SystemDrive").unwrap_or("C:".into());
        let p = PathBuf::from(format!("{sys}\\"));
        assert!(serial_of(&p).is_some());
        let tmp = std::env::temp_dir();
        assert_eq!(resolve(&tmp, serial_of(&tmp)), Some(tmp.clone()));
        assert_eq!(resolve(&tmp.join("no-such-folder-plume"), None), None);
    }
}
