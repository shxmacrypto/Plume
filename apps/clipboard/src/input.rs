//! The paste: the item is on the clipboard, the window that was in front has the keyboard again, and Plume presses
//! Ctrl+V for the user (SendInput). In tests (PLUME_CLIPBOARD_FAKE_INPUT, test builds only) nothing is pressed: the
//! keys are written to that file instead.

use windows::Win32::{
    Foundation::HWND,
    UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LMENU,
        VK_LSHIFT, VK_LWIN, VK_MENU, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    },
};

/// The file the keys go to instead of Windows (tests).
pub fn fake() -> Option<std::path::PathBuf> {
    #[cfg(feature = "test-hook")]
    if let Some(p) = std::env::var_os("PLUME_CLIPBOARD_FAKE_INPUT") {
        return Some(p.into());
    }
    None
}

pub fn log_fake(line: &str) -> bool {
    let Some(path) = fake() else { return false };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
    true
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
    }
}

/// Presses Ctrl+V in the window in front. Keys still held from the shortcut (Shift, Alt, Win) are released first,
/// or the app would see Ctrl+Shift+V.
pub fn paste(target: HWND, what: &str) {
    if log_fake(&format!("paste ctrl+v -> hwnd {} ({what})", target.0 as isize)) {
        return;
    }
    let mut inputs = Vec::new();
    for vk in [VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_MENU, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN] {
        if unsafe { GetAsyncKeyState(vk.0 as i32) } < 0 {
            inputs.push(key(vk, true));
        }
    }
    inputs.push(key(VK_CONTROL, false));
    inputs.push(key(VIRTUAL_KEY(0x56), false));
    inputs.push(key(VIRTUAL_KEY(0x56), true));
    inputs.push(key(VK_CONTROL, true));
    unsafe {
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}
