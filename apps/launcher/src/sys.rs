//! Windows around the launcher: the floating box's window (no frame, no taskbar button, hidden until called),
//! the shortcut that calls it, the icon near the clock, the start with Windows, and opening what was picked.
//! In tests, nothing is opened: what would have been opened is written down instead (`actions`).

use std::{
    cell::Cell,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{DWMWA_CLOAK, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute},
            Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegGetValueW, RegSetValueExW,
            },
        },
        UI::{
            HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
            Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey},
            Shell::{
                DefSubclassProc, ILCreateFromPathW, ILFree, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
                SHOpenFolderAndSelectItems, SetWindowSubclass, Shell_NotifyIconW, ShellExecuteW,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, PCWSTR, w},
};

// ───────────────────────── test mode ─────────────────────────

static TEST: AtomicBool = AtomicBool::new(false);
static ACTIONS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// From now on nothing is opened, copied or registered for real: it is written down (tests).
#[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
pub fn set_test_mode() {
    TEST.store(true, Ordering::Relaxed);
}

pub fn test_mode() -> bool {
    TEST.load(Ordering::Relaxed)
}

/// What would have been opened, in test mode.
#[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
pub fn actions() -> Vec<String> {
    ACTIONS.lock().unwrap().clone()
}

fn note(s: String) {
    ACTIONS.lock().unwrap().push(s);
}

/// Writes down why the box went away (tests only).
pub fn test_note(s: &str) {
    if test_mode() {
        note(s.to_string());
    }
}

// ───────────────────────── opening ─────────────────────────

/// Opens a file, a shortcut, an app of the Apps folder, a settings page or a web address.
pub fn open(target: &str) -> bool {
    if test_mode() {
        note(format!("open {target}"));
        return true;
    }
    let wide = HSTRING::from(target);
    unsafe { ShellExecuteW(None, w!("open"), &wide, None, None, SW_SHOWNORMAL).0 as usize > 32 }
}

/// Shows a file or folder selected in its folder.
pub fn reveal(path: &Path) -> bool {
    if test_mode() {
        note(format!("reveal {}", path.display()));
        return true;
    }
    // Plume FEx when it opens folders, otherwise the Windows shell
    if plume_ui::folder::fex().is_some() {
        plume_ui::folder::show(path);
        return true;
    }
    unsafe {
        let pidl = ILCreateFromPathW(&HSTRING::from(path.as_os_str()));
        if pidl.is_null() {
            return false;
        }
        let ok = SHOpenFolderAndSelectItems(pidl, None, 0).is_ok();
        ILFree(Some(pidl));
        ok
    }
}

/// Puts a text in the clipboard (through the window); in tests, only written down.
pub fn copy(win: &plume_ui::Window, text: &str) -> bool {
    if test_mode() {
        note(format!("copy {text}"));
        return true;
    }
    win.copy_text(text)
}

// ───────────────────────── the shortcut ─────────────────────────

pub const HOTKEY_ID: i32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub vk: u32,
}

fn key_name(vk: u32) -> Option<String> {
    Some(match vk {
        0x20 => "Space".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk)?.to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        0xDE => "'".into(),
        _ => return None,
    })
}

impl Hotkey {
    /// "Alt+Space", "Ctrl+Shift+K", "Win+Alt+F1".
    pub fn parse(s: &str) -> Option<Hotkey> {
        let mut h = Hotkey::default();
        let parts: Vec<String> = s.split('+').map(|p| p.trim().to_lowercase()).collect();
        let (key, mods) = parts.split_last()?;
        for m in mods {
            match m.as_str() {
                "ctrl" | "control" => h.ctrl = true,
                "alt" => h.alt = true,
                "shift" | "maj" => h.shift = true,
                "win" | "windows" => h.win = true,
                _ => return None,
            }
        }
        h.vk = (0..=0xFF).find(|&vk| key_name(vk).is_some_and(|n| n.to_lowercase() == *key))?;
        h.valid().then_some(h)
    }

    /// A shortcut needs a modifier (a lone letter would be taken from every app).
    pub fn valid(&self) -> bool {
        (self.ctrl || self.alt || self.win) && key_name(self.vk).is_some()
    }

    pub fn label(&self) -> String {
        let mut s = String::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.win, "Win"), (self.alt, "Alt"), (self.shift, "Shift")] {
            if on {
                s.push_str(name);
                s.push('+');
            }
        }
        s.push_str(&key_name(self.vk).unwrap_or_default());
        s
    }

    fn mods(&self) -> HOT_KEY_MODIFIERS {
        let mut m = MOD_NOREPEAT;
        for (on, f) in [(self.ctrl, MOD_CONTROL), (self.alt, MOD_ALT), (self.shift, MOD_SHIFT), (self.win, MOD_WIN)] {
            if on {
                m |= f;
            }
        }
        m
    }
}

/// Takes the shortcut for the window (false: another app already has it).
pub fn register_hotkey(hwnd: HWND, h: &Hotkey) -> bool {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
        RegisterHotKey(Some(hwnd), HOTKEY_ID, h.mods(), h.vk).is_ok()
    }
}

pub fn unregister_hotkey(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
    }
}

// ───────────────────────── the floating box's window ─────────────────────────

thread_local! {
    /// The box may be on screen (anything else that would show it, like the window kit's first show, is refused).
    static MAY_SHOW: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "system" fn popup_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    unsafe {
        match msg {
            WM_WINDOWPOSCHANGING if !MAY_SHOW.with(|m| m.get()) => {
                let pos = &mut *(lp.0 as *mut WINDOWPOS);
                pos.flags &= !SWP_SHOWWINDOW;
                DefSubclassProc(hwnd, msg, wp, lp)
            }
            // no edge to resize or drag: all of it is the box
            WM_NCHITTEST => LRESULT(HTCLIENT as isize),
            _ => DefSubclassProc(hwnd, msg, wp, lp),
        }
    }
}

/// Turns a window of the kit into the floating box: no frame, no taskbar button, above the others, rounded
/// corners and the shadow of Windows 11; and it stays hidden until `place` shows it.
pub fn make_popup(hwnd: HWND) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_POPUP | WS_CLIPCHILDREN).0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (WS_EX_TOOLWINDOW | WS_EX_TOPMOST).0 as isize);
        let round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const _ as _, 4);
        let _ = SetWindowSubclass(hwnd, Some(popup_proc), 1, 0);
    }
}

/// Hides or uncovers the window for the compositor only (it can be drawn while nobody sees it).
pub fn cloak(hwnd: HWND, on: bool) {
    let v: i32 = on as i32;
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CLOAK, &v as *const i32 as _, 4);
    }
}

/// The work area (pixels) and scale of the screen under the mouse.
pub fn screen_under_mouse() -> (RECT, f32) {
    unsafe {
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let mon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        (mi.rcWork, dx as f32 / 96.0)
    }
}

/// Places the box (size in DIP) centered on the screen under the mouse, a fifth of the way down, and shows it.
pub fn place(hwnd: HWND, w: f32, h: f32, activate: bool) {
    MAY_SHOW.with(|m| m.set(true));
    unsafe {
        let (work, scale) = screen_under_mouse();
        let put = |scale: f32, flags: SET_WINDOW_POS_FLAGS| {
            let (pw, ph) = ((w * scale).round() as i32, (h * scale).round() as i32);
            let x = work.left + (work.right - work.left - pw) / 2;
            let y = work.top + (work.bottom - work.top) / 5;
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, pw, ph, flags);
        };
        put(scale, SWP_NOACTIVATE);
        // another screen with another scale: the window has taken it, its size follows
        let now = GetDpiForWindow(hwnd) as f32 / 96.0;
        let show = if activate { SWP_SHOWWINDOW } else { SWP_SHOWWINDOW | SWP_NOACTIVATE };
        put(now, show);
        if activate {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

/// New height of the box (DIP), its top staying where it is.
pub fn set_height(hwnd: HWND, w: f32, h: f32) {
    unsafe {
        let s = GetDpiForWindow(hwnd) as f32 / 96.0;
        let _ = SetWindowPos(hwnd, None, 0, 0, (w * s).round() as i32, (h * s).round() as i32, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

pub fn hide(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    MAY_SHOW.with(|m| m.set(false));
}

/// Draws the hidden box once, out of sight, so the first call shows it as fast as the next ones.
pub fn warm_up(hwnd: HWND) {
    cloak(hwnd, true);
    MAY_SHOW.with(|m| m.set(true));
    unsafe {
        let _ = SetWindowPos(hwnd, None, -32000, -32000, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
    }
    hide(hwnd);
    cloak(hwnd, false);
}

pub fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

// ───────────────────────── the icon near the clock ─────────────────────────

/// The message the icon sends to the window.
pub const WM_TRAY: u32 = WM_APP + 60;
pub const MENU_OPEN: u32 = 1;
pub const MENU_SETTINGS: u32 = 2;
pub const MENU_QUIT: u32 = 3;

fn tray_data(hwnd: HWND, tip: &str) -> NOTIFYICONDATAW {
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
        let d = tray_data(hwnd, tip);
        if !Shell_NotifyIconW(NIM_ADD, &d).as_bool() {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
        }
    }
}

pub fn tray_remove(hwnd: HWND) {
    unsafe {
        let d = tray_data(hwnd, "");
        let _ = Shell_NotifyIconW(NIM_DELETE, &d);
    }
}

/// The icon's menu; returns the chosen command (0: none).
pub fn tray_menu(hwnd: HWND, open: &str, settings: &str, quit: &str) -> u32 {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        let _ = AppendMenuW(menu, MF_STRING, MENU_OPEN as usize, &HSTRING::from(open));
        let _ = AppendMenuW(menu, MF_STRING, MENU_SETTINGS as usize, &HSTRING::from(settings));
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

// ───────────────────────── start with Windows ─────────────────────────

/// The Run key. Tests never touch the real one: PLUME_LAUNCHER_RUN_KEY, or a test key of Plume's.
fn run_key() -> String {
    if let Ok(k) = std::env::var("PLUME_LAUNCHER_RUN_KEY") {
        return k;
    }
    if test_mode() {
        return r"Software\Plume\LauncherTestRun".into();
    }
    r"Software\Microsoft\Windows\CurrentVersion\Run".to_string()
}
const RUN_VALUE: &str = "Plume Launcher";

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts() {
        let h = Hotkey::parse("Alt+Space").unwrap();
        assert!(h.alt && !h.ctrl && h.vk == 0x20);
        assert_eq!(h.label(), "Alt+Space");
        assert_eq!(Hotkey::parse("ctrl + shift + k").unwrap().label(), "Ctrl+Shift+K");
        assert_eq!(Hotkey::parse("Win+Alt+F12").unwrap().label(), "Win+Alt+F12");
        // a modifier is needed, and the key must exist
        assert_eq!(Hotkey::parse("Space"), None);
        assert_eq!(Hotkey::parse("Shift+A"), None);
        assert_eq!(Hotkey::parse("Alt+Nope"), None);
        assert_eq!(Hotkey::parse("Hyper+A"), None);
    }
}
