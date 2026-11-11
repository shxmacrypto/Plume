//! Windows around the window: the icon near the clock (Plume Calendar stays there to ring reminders when its window
//! is closed), Windows notifications from that icon, and the start with Windows. Adapted from Plume Viewer's.

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT, WPARAM},
        System::{
            LibraryLoader::GetModuleHandleW,
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegGetValueW, RegSetValueExW,
            },
        },
        UI::{
            Shell::{NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_USER, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW},
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, PCWSTR},
};

/// The message the icon sends to the window.
pub const WM_TRAY: u32 = WM_APP + 61;
pub const MENU_OPEN: u32 = 1;
pub const MENU_QUIT: u32 = 2;

fn copy(dst: &mut [u16], s: &str) {
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
    copy(&mut d.szTip, tip);
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

/// A Windows notification from the icon near the clock (the icon is added if needed).
pub fn notify(hwnd: HWND, title: &str, text: &str) {
    unsafe {
        let mut d = data(hwnd, "Plume Calendar");
        if !Shell_NotifyIconW(NIM_MODIFY, &d).as_bool() {
            let _ = Shell_NotifyIconW(NIM_ADD, &d);
        }
        d.uFlags = NIF_INFO;
        d.dwInfoFlags = NIIF_USER;
        copy(&mut d.szInfoTitle, title);
        copy(&mut d.szInfo, text);
        let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
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

pub fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() }
}

/// A small window always on top, in the bottom right corner of the screen, not in the taskbar.
pub fn place_corner(hwnd: HWND, w: i32, h: i32) {
    unsafe {
        let mut work = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize));
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), work.right - w - 16, work.bottom - h - 16, w, h, SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW);
    }
}

/// The Run key (tests use another one, PLUME_CALENDAR_RUN_KEY).
fn run_key() -> String {
    #[cfg(feature = "test-hook")]
    if let Ok(k) = std::env::var("PLUME_CALENDAR_RUN_KEY") {
        return k;
    }
    r"Software\Microsoft\Windows\CurrentVersion\Run".to_string()
}
const RUN_VALUE: &str = "Plume Calendar";

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
