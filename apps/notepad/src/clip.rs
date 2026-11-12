//! The clipboard: text in and out. In a test build driven by a script, a clipboard of its own is used, so the
//! user's clipboard is never touched.

use plume_ui::Window;

#[cfg(feature = "test-hook")]
thread_local! {
    static FAKE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

#[cfg(feature = "test-hook")]
fn fake() -> bool {
    std::env::var_os("PLUME_NOTEPAD_TEST_PORT").is_some()
}

/// Puts text on the clipboard (with Windows line endings).
pub fn set(win: Window, text: &str) -> bool {
    let text = text.replace('\n', "\r\n");
    #[cfg(feature = "test-hook")]
    if fake() {
        FAKE.with(|f| *f.borrow_mut() = text);
        return true;
    }
    win.copy_text(&text)
}

/// The text on the clipboard, if any.
pub fn get(win: Window) -> Option<String> {
    #[cfg(feature = "test-hook")]
    if fake() {
        let _ = win;
        return Some(FAKE.with(|f| f.borrow().clone()));
    }
    use windows::Win32::{
        Foundation::HGLOBAL,
        System::{
            DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard},
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        },
    };
    const CF_UNICODETEXT: u32 = 13;
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() || OpenClipboard(Some(win.hwnd)).is_err() {
            return None;
        }
        let mut out = None;
        if let Ok(h) = GetClipboardData(CF_UNICODETEXT) {
            let mem = HGLOBAL(h.0);
            let p = GlobalLock(mem) as *const u16;
            if !p.is_null() {
                let max = GlobalSize(mem) / 2;
                let s = std::slice::from_raw_parts(p, max);
                let n = s.iter().position(|&c| c == 0).unwrap_or(max);
                out = Some(String::from_utf16_lossy(&s[..n]));
                let _ = GlobalUnlock(mem);
            }
        }
        let _ = CloseClipboard();
        out
    }
}
