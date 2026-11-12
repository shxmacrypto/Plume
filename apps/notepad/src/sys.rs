//! Windows around the app: the icon near the clock and its menu, files dropped on a window, the input method
//! window at the caret, the shortcut for a new pad, administrator rights for replacing Windows Notepad, and
//! windows staying on top.

use plume_ui::Rect;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, WPARAM},
        Graphics::Gdi::{FW_NORMAL, LOGFONTW},
        System::{LibraryLoader::GetModuleHandleW, Threading::GetExitCodeProcess, Threading::INFINITE, Threading::WaitForSingleObject},
        UI::{
            Input::{
                Ime::{CFS_POINT, COMPOSITIONFORM, ImmGetContext, ImmReleaseContext, ImmSetCompositionFontW, ImmSetCompositionWindow},
                KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey},
            },
            Shell::{
                DragAcceptFiles, DragFinish, DragQueryFileW, HDROP, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
                SEE_MASK_NOCLOSEPROCESS, SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify, SHELLEXECUTEINFOW, Shell_NotifyIconW, ShellExecuteExW,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, PCWSTR, w},
};

/// The message the icon near the clock sends to the main window.
pub const WM_TRAY: u32 = WM_APP + 60;
pub const WM_HOTKEY: u32 = 0x0312;
pub const WM_DROPFILES: u32 = 0x0233;
pub const WM_IME_STARTCOMPOSITION: u32 = 0x010D;
pub const WM_IME_COMPOSITION: u32 = 0x010F;
pub const HOTKEY_ID: i32 = 0x504E;

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
        let _ = Shell_NotifyIconW(NIM_DELETE, &data(hwnd, ""));
    }
}

/// An entry of a menu: a command id (0: separator), its label, checked, greyed.
pub struct Item {
    pub id: u32,
    pub label: String,
    pub checked: bool,
    pub disabled: bool,
}

pub fn item(id: u32, label: impl Into<String>) -> Item {
    Item { id, label: label.into(), checked: false, disabled: false }
}

pub fn sep() -> Item {
    Item { id: 0, label: String::new(), checked: false, disabled: false }
}

/// Shows a menu at a point of the screen (pixels); returns the chosen command (0: none).
pub fn popup(hwnd: HWND, items: &[Item], at: Option<(i32, i32)>, tray: bool) -> u32 {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        for it in items {
            if it.id == 0 {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            } else {
                let mut f = MF_STRING;
                if it.checked {
                    f |= MF_CHECKED;
                }
                if it.disabled {
                    f |= MF_GRAYED;
                }
                let _ = AppendMenuW(menu, f, it.id as usize, &HSTRING::from(it.label.as_str()));
            }
        }
        let (x, y) = at.unwrap_or_else(|| {
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            (p.x, p.y)
        });
        // the menu closes when clicking elsewhere only if the window is in front
        if tray {
            let _ = SetForegroundWindow(hwnd);
        }
        let flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | if tray { TPM_BOTTOMALIGN } else { TPM_TOPALIGN };
        let cmd = TrackPopupMenu(menu, flags, x, y, Some(0), hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        cmd.0 as u32
    }
}

pub fn show(hwnd: HWND, on: bool, activate: bool) {
    unsafe {
        if !on {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, if activate { SW_RESTORE } else { SW_SHOWNOACTIVATE });
        } else {
            let _ = ShowWindow(hwnd, if activate { SW_SHOW } else { SW_SHOWNOACTIVATE });
        }
        if activate {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

pub fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

/// Files can be dropped on the window (they arrive as WM_DROPFILES).
pub fn accept_files(hwnd: HWND) {
    unsafe { DragAcceptFiles(hwnd, true) };
}

/// The files of a WM_DROPFILES.
pub fn dropped(wp: WPARAM) -> Vec<String> {
    let drop = HDROP(wp.0 as _);
    let mut out = Vec::new();
    unsafe {
        let n = DragQueryFileW(drop, u32::MAX, None);
        for i in 0..n {
            let len = DragQueryFileW(drop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            let got = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
            out.push(String::from_utf16_lossy(&buf[..got]));
        }
        DragFinish(drop);
    }
    out
}

/// Puts the input method's window (Chinese, Japanese, Korean…) at the caret, in the text's size.
pub fn ime_at(hwnd: HWND, caret: Rect, scale: f32, family: &str, size: f32) {
    unsafe {
        let ctx = ImmGetContext(hwnd);
        if ctx.is_invalid() {
            return;
        }
        let form = COMPOSITIONFORM {
            dwStyle: CFS_POINT,
            ptCurrentPos: POINT { x: (caret.x * scale).round() as i32, y: (caret.y * scale).round() as i32 },
            ..Default::default()
        };
        let _ = ImmSetCompositionWindow(ctx, &form);
        let mut lf = LOGFONTW { lfHeight: -(size * scale).round() as i32, lfWeight: FW_NORMAL.0 as i32, ..Default::default() };
        for (i, u) in family.encode_utf16().take(31).enumerate() {
            lf.lfFaceName[i] = u;
        }
        let _ = ImmSetCompositionFontW(ctx, &lf);
        let _ = ImmReleaseContext(hwnd, ctx);
        // the system caret follows ours, for the emoji panel and screen readers
        let _ = DestroyCaret();
        let _ = CreateCaret(hwnd, None, 1, (caret.h * scale).round() as i32);
        let _ = SetCaretPos((caret.x * scale).round() as i32, (caret.y * scale).round() as i32);
    }
}

/// Ctrl+Alt+N anywhere: a new pad. False if another app holds it.
pub fn register_hotkey(hwnd: HWND) -> bool {
    unsafe { RegisterHotKey(Some(hwnd), HOTKEY_ID, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0), b'N' as u32).is_ok() }
}

/// Over every other window, or not.
pub fn set_on_top(hwnd: HWND, on: bool) {
    unsafe {
        let _ = SetWindowPos(hwnd, Some(if on { HWND_TOPMOST } else { HWND_NOTOPMOST }), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// A pad: not in the taskbar or Alt+Tab, square-ish Windows 11 corners.
pub fn make_pad_window(hwnd: HWND) {
    use windows::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute};
    unsafe {
        let visible = IsWindowVisible(hwnd).as_bool();
        if visible {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_TOOLWINDOW.0 as isize);
        // no maximize: a pad is not a document window
        let st = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, st & !((WS_MAXIMIZEBOX | WS_MINIMIZEBOX).0 as isize));
        let small: i32 = 3; // DWMWCP_ROUNDSMALL
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &small as *const i32 as _, 4);
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }
}

pub fn window_rect(hwnd: HWND) -> (i32, i32, i32, i32) {
    let mut r = windows::Win32::Foundation::RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut r);
    }
    (r.left, r.top, r.right - r.left, r.bottom - r.top)
}

pub fn place(hwnd: HWND, r: (i32, i32, i32, i32)) {
    unsafe {
        let _ = SetWindowPos(hwnd, None, r.0, r.1, r.2, r.3, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/// The work area of the screen the cursor is on (pixels).
pub fn work_area() -> (i32, i32, i32, i32) {
    let mut r = windows::Win32::Foundation::RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut r as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    (r.left, r.top, r.right - r.left, r.bottom - r.top)
}

/// Runs this exe again as administrator (Windows asks first) with `args`, and waits for it.
/// Some(true): it worked; Some(false): it ran but failed; None: the user said no.
#[cfg_attr(feature = "test-hook", allow(dead_code))]
pub fn run_elevated(args: &str) -> Option<bool> {
    let Ok(exe) = std::env::current_exe() else { return None };
    let file = HSTRING::from(exe.as_os_str());
    let params = HSTRING::from(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        if ShellExecuteExW(&mut info).is_err() || info.hProcess.is_invalid() {
            return None;
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = windows::Win32::Foundation::CloseHandle(info.hProcess);
        Some(code == 0)
    }
}

/// Explorer reads the file associations again.
pub fn assoc_changed() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}

/// The command line as Windows gave it (the arguments after the exe name, untouched).
pub fn raw_args() -> String {
    let full = unsafe { windows::Win32::System::Environment::GetCommandLineW().to_string().unwrap_or_default() };
    let full = full.trim_start();
    let rest = if let Some(r) = full.strip_prefix('"') {
        r.split_once('"').map_or("", |(_, b)| b)
    } else {
        full.split_once(char::is_whitespace).map_or("", |(_, b)| b)
    };
    rest.to_string()
}
