//! Windows around the window: the icon near the clock (it tells that this PC can be reached, and keeps Plume Viewer
//! running when its window is closed), and the start with Windows.

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, WPARAM},
        System::{
            LibraryLoader::GetModuleHandleW,
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegDeleteKeyValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
            },
        },
        UI::{
            Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW},
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, PCWSTR, w},
};

/// The message the icon sends to the window.
pub const WM_TRAY: u32 = WM_APP + 60;
pub const MENU_OPEN: u32 = 1;
pub const MENU_QUIT: u32 = 2;

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
    for (i, u) in tip.encode_utf16().take(d.szTip.len() - 1).enumerate() {
        d.szTip[i] = u;
    }
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

pub fn tray_remove(hwnd: HWND) {
    unsafe {
        let d = data(hwnd, "");
        let _ = Shell_NotifyIconW(NIM_DELETE, &d);
    }
}

/// The icon's menu; returns the chosen command (0: none).
pub fn tray_menu(hwnd: HWND, open: &str, quit: &str) -> u32 {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        let _ = AppendMenuW(menu, MF_STRING, MENU_OPEN as usize, &HSTRING::from(open));
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

/// The Run key (tests use another one).
fn run_key() -> String {
    #[cfg(feature = "test-hook")]
    if let Ok(k) = std::env::var("PLUME_VIEWER_RUN_KEY") {
        return k;
    }
    r"Software\Microsoft\Windows\CurrentVersion\Run".to_string()
}
const RUN_VALUE: &str = "Plume Viewer";

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
        if RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(run_key()), Some(0), KEY_SET_VALUE, &mut key).is_err() {
            // the test key may not exist yet
            let mut disp = Default::default();
            if windows::Win32::System::Registry::RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(run_key()),
                None,
                None,
                Default::default(),
                KEY_SET_VALUE,
                None,
                &mut key,
                Some(&mut disp),
            )
            .is_err()
            {
                return false;
            }
        }
        let wide: Vec<u16> = cmd.encode_utf16().chain(Some(0)).collect();
        let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
        let ok = RegSetValueExW(key, &HSTRING::from(RUN_VALUE), None, REG_SZ, Some(bytes)).is_ok();
        let _ = RegCloseKey(key);
        ok
    }
}

#[allow(dead_code)]
const _UNUSED: PCWSTR = w!("");
