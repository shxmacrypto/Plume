//! What the viewer does, played on this PC: mouse, wheel, keys (by physical position, like a keyboard plugged in
//! here), and text typed on a phone (as characters, whatever the keyboard layout). Windows refuses this for windows
//! of programs run as administrator (UAC), unless Plume Viewer itself runs as administrator.

use crate::capture::Geom;
use serde_json::Value;
use std::collections::HashSet;
use windows::Win32::UI::{
    Input::KeyboardAndMouse::*,
    WindowsAndMessaging::{GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN},
};

/// KeyboardEvent.code → (virtual key, extended key).
pub fn vk_of(code: &str) -> Option<(u16, bool)> {
    if let Some(c) = code.strip_prefix("Key")
        && c.len() == 1
        && c.as_bytes()[0].is_ascii_uppercase()
    {
        return Some((c.as_bytes()[0] as u16, false));
    }
    if let Some(d) = code.strip_prefix("Digit")
        && d.len() == 1
        && d.as_bytes()[0].is_ascii_digit()
    {
        return Some((d.as_bytes()[0] as u16, false));
    }
    if let Some(n) = code.strip_prefix('F').and_then(|n| n.parse::<u16>().ok())
        && (1..=24).contains(&n)
    {
        return Some((VK_F1.0 + n - 1, false));
    }
    if let Some(d) = code.strip_prefix("Numpad")
        && d.len() == 1
        && d.as_bytes()[0].is_ascii_digit()
    {
        return Some((VK_NUMPAD0.0 + (d.as_bytes()[0] - b'0') as u16, false));
    }
    let (vk, ext) = match code {
        "Enter" => (VK_RETURN, false),
        "NumpadEnter" => (VK_RETURN, true),
        "Escape" => (VK_ESCAPE, false),
        "Backspace" => (VK_BACK, false),
        "Tab" => (VK_TAB, false),
        "Space" => (VK_SPACE, false),
        "ShiftLeft" => (VK_LSHIFT, false),
        "ShiftRight" => (VK_RSHIFT, false),
        "ControlLeft" => (VK_LCONTROL, false),
        "ControlRight" => (VK_RCONTROL, true),
        "AltLeft" => (VK_LMENU, false),
        "AltRight" => (VK_RMENU, true),
        "MetaLeft" | "OSLeft" => (VK_LWIN, true),
        "MetaRight" | "OSRight" => (VK_RWIN, true),
        "ContextMenu" => (VK_APPS, true),
        "CapsLock" => (VK_CAPITAL, false),
        "NumLock" => (VK_NUMLOCK, true),
        "ScrollLock" => (VK_SCROLL, false),
        "Pause" => (VK_PAUSE, false),
        "PrintScreen" => (VK_SNAPSHOT, true),
        "Insert" => (VK_INSERT, true),
        "Delete" => (VK_DELETE, true),
        "Home" => (VK_HOME, true),
        "End" => (VK_END, true),
        "PageUp" => (VK_PRIOR, true),
        "PageDown" => (VK_NEXT, true),
        "ArrowLeft" => (VK_LEFT, true),
        "ArrowRight" => (VK_RIGHT, true),
        "ArrowUp" => (VK_UP, true),
        "ArrowDown" => (VK_DOWN, true),
        "Minus" => (VK_OEM_MINUS, false),
        "Equal" => (VK_OEM_PLUS, false),
        "BracketLeft" => (VK_OEM_4, false),
        "BracketRight" => (VK_OEM_6, false),
        "Backslash" => (VK_OEM_5, false),
        "IntlBackslash" => (VK_OEM_102, false),
        "Semicolon" => (VK_OEM_1, false),
        "Quote" => (VK_OEM_7, false),
        "Backquote" => (VK_OEM_3, false),
        "Comma" => (VK_OEM_COMMA, false),
        "Period" => (VK_OEM_PERIOD, false),
        "Slash" => (VK_OEM_2, false),
        "NumpadAdd" => (VK_ADD, false),
        "NumpadSubtract" => (VK_SUBTRACT, false),
        "NumpadMultiply" => (VK_MULTIPLY, false),
        "NumpadDivide" => (VK_DIVIDE, true),
        "NumpadDecimal" => (VK_DECIMAL, false),
        "AudioVolumeMute" => (VK_VOLUME_MUTE, true),
        "AudioVolumeDown" => (VK_VOLUME_DOWN, true),
        "AudioVolumeUp" => (VK_VOLUME_UP, true),
        "MediaPlayPause" => (VK_MEDIA_PLAY_PAUSE, true),
        _ => return None,
    };
    Some((vk.0, ext))
}

/// Plays the viewer's input. Keeps the keys and buttons held, to release them at the end of the session.
pub struct Injector {
    keys: HashSet<(u16, bool)>,
    buttons: HashSet<u8>,
    /// Tests: what would be played is written here instead (the user's mouse and keyboard are left alone).
    #[cfg(feature = "test-hook")]
    fake: Option<std::path::PathBuf>,
}

fn mouse(dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

fn key(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

impl Default for Injector {
    fn default() -> Self {
        Self::new()
    }
}

impl Injector {
    pub fn new() -> Injector {
        Injector {
            keys: HashSet::new(),
            buttons: HashSet::new(),
            #[cfg(feature = "test-hook")]
            fake: std::env::var_os("PLUME_VIEWER_FAKE_INPUT").map(Into::into),
        }
    }

    fn play(&self, what: &str, inputs: &[INPUT]) {
        #[cfg(feature = "test-hook")]
        if let Some(path) = &self.fake {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "{what}");
            }
            return;
        }
        let _ = what;
        unsafe {
            SendInput(inputs, size_of::<INPUT>() as i32);
        }
    }

    /// A pixel of the image sent → absolute mouse position over all screens (0..65535).
    fn abs(g: &Geom, x: i64, y: i64) -> (i32, i32, i32, i32) {
        let s = g.scale.max(1) as i64;
        let px = g.left as i64 + x.clamp(0, (g.w as i64 - 1).max(0)) * s;
        let py = g.top as i64 + y.clamp(0, (g.h as i64 - 1).max(0)) * s;
        unsafe {
            let (vx, vy) = (GetSystemMetrics(SM_XVIRTUALSCREEN) as i64, GetSystemMetrics(SM_YVIRTUALSCREEN) as i64);
            let (vw, vh) = (GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2) as i64, GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2) as i64);
            (((px - vx) * 65535 / (vw - 1)) as i32, ((py - vy) * 65535 / (vh - 1)) as i32, px as i32, py as i32)
        }
    }

    fn move_to(&self, g: &Geom, m: &Value) -> Option<INPUT> {
        let (x, y) = (m["x"].as_i64()?, m["y"].as_i64()?);
        let (ax, ay, _, _) = Self::abs(g, x, y);
        Some(mouse(ax, ay, 0, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK))
    }

    fn keystroke(&mut self, code: &str, down: bool) {
        let Some((vk, ext)) = vk_of(code) else { return };
        if down {
            self.keys.insert((vk, ext));
        } else if !self.keys.remove(&(vk, ext)) {
            return; // never pressed here: nothing to release
        }
        let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
        let mut flags = if ext { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) };
        if !down {
            flags |= KEYEVENTF_KEYUP;
        }
        self.play(&format!("key {code} {}", if down { "down" } else { "up" }), &[key(vk, scan, flags)]);
    }

    /// One message of the "input" channel.
    pub fn handle(&mut self, g: &Geom, m: &Value) {
        match m["t"].as_str() {
            Some("mm") => {
                if let Some(i) = self.move_to(g, m) {
                    self.play(&format!("move {} {}", m["x"], m["y"]), &[i]);
                }
            }
            Some("mb") => {
                let b = m["b"].as_u64().unwrap_or(0) as u8;
                let down = m["d"].as_bool().unwrap_or(false);
                let flags = match (b, down) {
                    (0, true) => MOUSEEVENTF_LEFTDOWN,
                    (0, false) => MOUSEEVENTF_LEFTUP,
                    (1, true) => MOUSEEVENTF_MIDDLEDOWN,
                    (1, false) => MOUSEEVENTF_MIDDLEUP,
                    (2, true) => MOUSEEVENTF_RIGHTDOWN,
                    (2, false) => MOUSEEVENTF_RIGHTUP,
                    _ => return,
                };
                if down {
                    self.buttons.insert(b);
                } else if !self.buttons.remove(&b) {
                    return;
                }
                let mut list = Vec::new();
                if let Some(i) = self.move_to(g, m) {
                    list.push(i);
                }
                list.push(mouse(0, 0, 0, flags));
                self.play(&format!("button {b} {} {} {}", if down { "down" } else { "up" }, m["x"], m["y"]), &list);
            }
            Some("wh") => {
                // pixels from the browser → wheel units (120 a notch for ~100 px)
                let dy = (m["dy"].as_f64().unwrap_or(0.0) * 1.2).clamp(-2400.0, 2400.0) as i32;
                let dx = (m["dx"].as_f64().unwrap_or(0.0) * 1.2).clamp(-2400.0, 2400.0) as i32;
                let mut list = Vec::new();
                if dy != 0 {
                    list.push(mouse(0, 0, dy, MOUSEEVENTF_WHEEL));
                }
                if dx != 0 {
                    list.push(mouse(0, 0, -dx, MOUSEEVENTF_HWHEEL));
                }
                if !list.is_empty() {
                    self.play(&format!("wheel {dx} {dy}"), &list);
                }
            }
            Some("kd") => self.keystroke(m["c"].as_str().unwrap_or(""), true),
            Some("ku") => self.keystroke(m["c"].as_str().unwrap_or(""), false),
            Some("tx") => {
                let s: String = m["s"].as_str().unwrap_or("").chars().take(500).collect();
                let mut list = Vec::new();
                for ch in s.chars() {
                    if ch == '\n' || ch == '\r' {
                        list.push(key(VK_RETURN.0, 0, KEYBD_EVENT_FLAGS(0)));
                        list.push(key(VK_RETURN.0, 0, KEYEVENTF_KEYUP));
                        continue;
                    }
                    let mut buf = [0u16; 2];
                    for unit in ch.encode_utf16(&mut buf) {
                        list.push(key(0, *unit, KEYEVENTF_UNICODE));
                        list.push(key(0, *unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                    }
                }
                if !list.is_empty() {
                    self.play(&format!("text {s}"), &list);
                }
            }
            Some("combo") if m["k"].as_str() == Some("win") => {
                self.keystroke("MetaLeft", true);
                self.keystroke("MetaLeft", false);
            }
            _ => {}
        }
    }

    /// Releases what the viewer still holds (it left in the middle of a drag or a shortcut).
    pub fn release_all(&mut self) {
        let keys: Vec<_> = self.keys.iter().copied().collect();
        for (vk, ext) in keys {
            let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
            let flags = KEYEVENTF_KEYUP | if ext { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) };
            self.play(&format!("release key {vk}"), &[key(vk, scan, flags)]);
        }
        self.keys.clear();
        let buttons: Vec<_> = self.buttons.drain().collect();
        for b in buttons {
            let flags = match b {
                0 => MOUSEEVENTF_LEFTUP,
                1 => MOUSEEVENTF_MIDDLEUP,
                _ => MOUSEEVENTF_RIGHTUP,
            };
            self.play(&format!("release button {b}"), &[mouse(0, 0, 0, flags)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        assert_eq!(vk_of("KeyA"), Some((b'A' as u16, false)));
        assert_eq!(vk_of("Digit7"), Some((b'7' as u16, false)));
        assert_eq!(vk_of("F12"), Some((VK_F12.0, false)));
        assert_eq!(vk_of("ArrowLeft"), Some((VK_LEFT.0, true)));
        assert_eq!(vk_of("Numpad5"), Some((VK_NUMPAD5.0, false)));
        assert_eq!(vk_of("Keyb"), None);
        assert_eq!(vk_of("Fn"), None);
        assert_eq!(vk_of("F25"), None);
    }
}
