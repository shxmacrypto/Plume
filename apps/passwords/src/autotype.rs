//! Auto-type: the user name, Tab, the password (and Enter if asked) typed into the window in front, as if typed on
//! the keyboard. Characters go as Unicode, so the keyboard layout does not matter. Windows refuses this for
//! programs run as administrator (unless Plume Passwords is too).
//!
//! In a test build with PLUME_PASSWORDS_FAKE_INPUT set, nothing is typed: what would be typed is written to that
//! file, as lengths only (never the secrets themselves).

use windows::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY,
            VK_CONTROL, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT, VK_TAB,
        },
        WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, SetForegroundWindow},
    },
};
use zeroize::Zeroizing;

pub enum Step {
    Text(Zeroizing<String>),
    Tab,
    Enter,
}

/// The window in front, and its title.
pub fn foreground() -> (HWND, String) {
    unsafe {
        let h = GetForegroundWindow();
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(h, &mut buf);
        (h, String::from_utf16_lossy(&buf[..n.max(0) as usize]))
    }
}

pub fn process_of(h: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

fn key(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

fn fake_log() -> Option<std::path::PathBuf> {
    #[cfg(feature = "test-hook")]
    if let Some(p) = std::env::var_os("PLUME_PASSWORDS_FAKE_INPUT") {
        return Some(p.into());
    }
    None
}

/// What the fake log says for these steps (lengths, never the text).
pub fn describe(steps: &[Step]) -> String {
    steps
        .iter()
        .map(|s| match s {
            Step::Text(t) => format!("text:{}", t.chars().count()),
            Step::Tab => "key:tab".into(),
            Step::Enter => "key:enter".into(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Types the steps into `target` (brought to the front first). Runs on its own thread: it waits for the keys of the
/// shortcut to be released, so Ctrl or Alt still held does not turn letters into shortcuts.
pub fn type_into(target: HWND, title: String, steps: Vec<Step>, done: impl FnOnce(bool) + Send + 'static) {
    let target_v = target.0 as isize;
    std::thread::spawn(move || {
        let target = HWND(target_v as _);
        if let Some(path) = fake_log() {
            use std::io::Write;
            let ok = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| writeln!(f, "into \"{}\": {}", title.replace('"', "'"), describe(&steps)))
                .is_ok();
            done(ok);
            return;
        }
        unsafe {
            let _ = SetForegroundWindow(target);
            // up to two seconds for Ctrl, Alt, Shift and Windows to be released
            for _ in 0..100 {
                let held = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN].iter().any(|vk| GetAsyncKeyState(vk.0 as i32) < 0);
                if !held {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            std::thread::sleep(std::time::Duration::from_millis(120));
            if GetForegroundWindow() != target {
                done(false);
                return;
            }
        }
        let mut sent_all = true;
        for s in &steps {
            let mut inputs: Vec<INPUT> = Vec::new();
            match s {
                Step::Text(t) => {
                    for u in t.encode_utf16() {
                        inputs.push(key(0, u, KEYEVENTF_UNICODE));
                        inputs.push(key(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                    }
                }
                Step::Tab => {
                    inputs.push(key(VK_TAB.0, 0, KEYBD_EVENT_FLAGS(0)));
                    inputs.push(key(VK_TAB.0, 0, KEYEVENTF_KEYUP));
                }
                Step::Enter => {
                    inputs.push(key(VK_RETURN.0, 0, KEYBD_EVENT_FLAGS(0)));
                    inputs.push(key(VK_RETURN.0, 0, KEYEVENTF_KEYUP));
                }
            }
            let n = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
            sent_all &= n as usize == inputs.len();
            // the inputs held the characters: overwrite them
            for i in &mut inputs {
                i.Anonymous.ki.wScan = 0;
            }
            // let the page react to Tab (focus moves to the next field)
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        drop(steps);
        done(sent_all);
    });
}

/// Words of a window title worth matching against entries ("Sign in – Example – Plume Web" → sign, example, plume…).
fn words(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() >= 3).map(String::from).collect()
}

/// How well an entry fits a window title (0: not at all). Its address host counts most, then its title.
pub fn score(title: &str, entry_title: &str, url: &str) -> u32 {
    let t = title.to_lowercase();
    let host = crate::csv::host_of(url);
    let mut s = 0;
    if !host.is_empty() {
        let name = host.split('.').rev().nth(1).unwrap_or(&host);
        if t.contains(&host) {
            // the longer the address, the more precise: shop.example.net beats example.net
            s += 10 + host.len().min(40) as u32 / 4;
        } else if name.len() >= 3 && words(&t).iter().any(|w| w == name) {
            s += 6;
        }
    }
    let et = entry_title.to_lowercase();
    if !et.is_empty() && et.len() >= 3 && t.contains(&et) {
        s += 5;
    } else if words(&et).iter().any(|w| words(&t).contains(w)) {
        s += 2;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_and_describing() {
        assert!(score("Sign in - Example - Plume Web", "Example", "https://accounts.example.com/login") >= 6);
        assert!(score("login.example.org — Mozilla Firefox", "", "https://login.example.org") >= 10);
        assert_eq!(score("Untitled - Notepad", "Example", "https://example.com"), 0);
        let t = "Shop checkout - shop.example.net";
        assert!(score(t, "Shop", "https://shop.example.net") > score(t, "example.net", "https://example.net/login"));
        let steps = vec![Step::Text(Zeroizing::new("user".into())), Step::Tab, Step::Text(Zeroizing::new("secret!".into())), Step::Enter];
        assert_eq!(describe(&steps), "text:4 key:tab text:7 key:enter");
    }
}
