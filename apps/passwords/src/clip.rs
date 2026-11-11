//! Copying secrets: the clipboard gets the text plus the formats that tell Windows to keep it out of the clipboard
//! history and of the cloud clipboard, and to leave it alone in clipboard monitors. Then, after a while, the
//! clipboard is emptied, unless something else was copied since.

use windows::{
    Win32::{
        Foundation::{HANDLE, HWND},
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        },
    },
    core::HSTRING,
};

const CF_UNICODETEXT: u32 = 13;

fn format(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(&HSTRING::from(name)) }
}

/// Opens the clipboard, trying a few times (another program may hold it for a moment).
fn open(hwnd: HWND) -> bool {
    for _ in 0..10 {
        if unsafe { OpenClipboard(Some(hwnd)) }.is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    false
}

fn put(fmt: u32, bytes: &[u8]) -> bool {
    unsafe {
        let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else { return false };
        let p = GlobalLock(mem) as *mut u8;
        if p.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        let _ = GlobalUnlock(mem);
        // the clipboard owns the memory from now on
        SetClipboardData(fmt, Some(HANDLE(mem.0))).is_ok()
    }
}

/// Copies a secret. Returns the clipboard's sequence number after the copy (to know later whether it is still
/// ours), or None if Windows refused.
pub fn copy_secret(hwnd: HWND, text: &str) -> Option<u32> {
    let mut wide: Vec<u8> = text.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect();
    if !open(hwnd) {
        return None;
    }
    let ok = unsafe {
        let _ = EmptyClipboard();
        let ok = put(CF_UNICODETEXT, &wide);
        let zero = 0u32.to_le_bytes();
        put(format("ExcludeClipboardContentFromMonitorProcessing"), &zero);
        put(format("CanIncludeInClipboardHistory"), &zero);
        put(format("CanUploadToCloudClipboard"), &zero);
        let _ = CloseClipboard();
        ok
    };
    zeroize::Zeroize::zeroize(&mut wide);
    ok.then(|| unsafe { GetClipboardSequenceNumber() })
}

/// Empties the clipboard if it still holds what we copied (sequence number `seq`). True if it was emptied.
pub fn clear_if_ours(hwnd: HWND, seq: u32) -> bool {
    if unsafe { GetClipboardSequenceNumber() } != seq || !open(hwnd) {
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
        let _ = CloseClipboard();
    }
    true
}

#[cfg(test)]
/// What the clipboard holds: each format whose data is a block of memory, with its bytes. For the tests, which
/// must give the person's clipboard back exactly as it was.
pub struct Snapshot(pub Vec<(u32, Vec<u8>)>);

#[cfg(test)]
/// Formats whose data is a GDI object, not memory: Windows makes them again from the memory ones.
const NOT_MEMORY: [u32; 8] = [2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E];

#[cfg(test)]
pub fn snapshot(hwnd: HWND) -> Option<Snapshot> {
    if !open(hwnd) {
        return None;
    }
    let mut out = Vec::new();
    unsafe {
        let mut f = 0;
        loop {
            f = windows::Win32::System::DataExchange::EnumClipboardFormats(f);
            if f == 0 {
                break;
            }
            if NOT_MEMORY.contains(&f) {
                continue;
            }
            let Ok(h) = windows::Win32::System::DataExchange::GetClipboardData(f) else { continue };
            let mem = windows::Win32::Foundation::HGLOBAL(h.0);
            let n = windows::Win32::System::Memory::GlobalSize(mem);
            let p = GlobalLock(mem) as *const u8;
            if p.is_null() {
                continue;
            }
            out.push((f, std::slice::from_raw_parts(p, n).to_vec()));
            let _ = GlobalUnlock(mem);
        }
        let _ = CloseClipboard();
    }
    Some(Snapshot(out))
}

#[cfg(test)]
pub fn restore(hwnd: HWND, s: &Snapshot) -> bool {
    if !open(hwnd) {
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
        for (f, bytes) in &s.0 {
            let Ok(mem) = GlobalAlloc(windows::Win32::System::Memory::GHND, bytes.len().max(1)) else { continue };
            let p = GlobalLock(mem) as *mut u8;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                let _ = GlobalUnlock(mem);
                let _ = SetClipboardData(*f, Some(HANDLE(mem.0)));
            }
        }
        let _ = CloseClipboard();
    }
    true
}

/// The clipboard's text, if any (tests).
#[cfg(test)]
fn text(hwnd: HWND) -> Option<String> {
    let s = snapshot(hwnd)?;
    let (_, b) = s.0.iter().find(|(f, _)| *f == CF_UNICODETEXT)?;
    let w: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|u| *u != 0).collect();
    Some(String::from_utf16_lossy(&w))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real clipboard, so it runs only when asked: PLUME_PASSWORDS_CLIPBOARD_TEST=1. The clipboard is
    /// saved first and given back exactly afterwards.
    #[test]
    fn copy_marks_and_clears() {
        if std::env::var_os("PLUME_PASSWORDS_CLIPBOARD_TEST").is_none() {
            return;
        }
        use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE};
        // the clipboard needs an owner window: a message-only one, never shown
        let hwnd = unsafe {
            CreateWindowExW(WINDOW_EX_STYLE(0), windows::core::w!("STATIC"), None, WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, None, None).unwrap()
        };
        let before = snapshot(hwnd).expect("clipboard");
        let secret = "test-only-value";
        let seq = copy_secret(hwnd, secret).expect("copy");
        let after = snapshot(hwnd).unwrap();
        let has = |name: &str| after.0.iter().any(|(f, b)| *f == format(name) && b.len() >= 4 && b[..4] == [0, 0, 0, 0]);
        let marks = [has("ExcludeClipboardContentFromMonitorProcessing"), has("CanIncludeInClipboardHistory"), has("CanUploadToCloudClipboard")];
        let same_text = text(hwnd).as_deref() == Some(secret);
        let cleared = clear_if_ours(hwnd, seq);
        let empty = text(hwnd).is_none();
        // a newer copy is not ours: left alone
        let seq2 = copy_secret(hwnd, secret).unwrap();
        let left = !clear_if_ours(hwnd, seq2.wrapping_sub(1));
        assert!(restore(hwnd, &before));
        let back = snapshot(hwnd).unwrap();
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        let unicode = |s: &Snapshot| s.0.iter().find(|(f, _)| *f == CF_UNICODETEXT).map(|(_, b)| b.clone());
        assert_eq!(unicode(&back), unicode(&before), "the clipboard's text came back");
        assert!(before.0.iter().all(|(f, _)| back.0.iter().any(|(g, _)| g == f)), "every format came back");
        assert!(marks.iter().all(|m| *m), "history and cloud marks");
        assert!(same_text && cleared && empty && left);
    }
}
