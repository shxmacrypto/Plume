//! The shortcut that opens the panel ("Ctrl+Shift+V" by default), registered with Windows (RegisterHotKey: no
//! keyboard hook, Plume never sees the other keys).

use windows::Win32::{
    Foundation::HWND,
    UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey},
};

pub const ID: i32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub vk: u16,
}

/// A key's name, as shown and saved.
pub fn key_name(vk: u16) -> Option<String> {
    Some(match vk {
        0x30..=0x39 | 0x41..=0x5A => (vk as u8 as char).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x20 => "Space".into(),
        0x2D => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0xC0 => "`".into(),
        0xBA => ";".into(),
        0xBC => ",".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xDC => "\\".into(),
        _ => return None,
    })
}

fn key_of(name: &str) -> Option<u16> {
    let up = name.to_uppercase();
    if up.len() == 1 {
        let c = up.as_bytes()[0];
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u16);
        }
    }
    if let Some(n) = up.strip_prefix('F').and_then(|n| n.parse::<u16>().ok())
        && (1..=24).contains(&n)
    {
        return Some(0x6F + n);
    }
    (0u16..=0xFF).find(|vk| key_name(*vk).is_some_and(|k| k.eq_ignore_ascii_case(name)))
}

impl Hotkey {
    /// "Ctrl+Shift+V" (at least one of Ctrl, Alt, Win).
    pub fn parse(s: &str) -> Option<Hotkey> {
        let mut h = Hotkey::default();
        let parts: Vec<&str> = s.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
        let (key, mods) = parts.split_last()?;
        for m in mods {
            match m.to_lowercase().as_str() {
                "ctrl" | "control" => h.ctrl = true,
                "shift" => h.shift = true,
                "alt" => h.alt = true,
                "win" | "windows" => h.win = true,
                _ => return None,
            }
        }
        h.vk = key_of(key)?;
        h.valid().then_some(h)
    }

    pub fn valid(&self) -> bool {
        (self.ctrl || self.alt || self.win) && key_name(self.vk).is_some()
    }

    pub fn label(&self) -> String {
        let mut s = String::new();
        for (on, n) in [(self.ctrl, "Ctrl+"), (self.alt, "Alt+"), (self.shift, "Shift+"), (self.win, "Win+")] {
            if on {
                s.push_str(n);
            }
        }
        s.push_str(&key_name(self.vk).unwrap_or_default());
        s
    }

    pub fn register(&self, hwnd: HWND) -> bool {
        let mut m = MOD_NOREPEAT;
        for (on, f) in [(self.ctrl, MOD_CONTROL), (self.shift, MOD_SHIFT), (self.alt, MOD_ALT), (self.win, MOD_WIN)] {
            if on {
                m |= f;
            }
        }
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), ID);
            RegisterHotKey(Some(hwnd), ID, HOT_KEY_MODIFIERS(m.0), self.vk as u32).is_ok()
        }
    }
}

pub fn unregister(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), ID);
    }
}

/// Tests with no shortcut (PLUME_CLIPBOARD_HOTKEY=none, test builds): the configured one is still shown as working,
/// so pictures of the window show the real default rather than an error.
pub fn off_for_test() -> bool {
    cfg!(feature = "test-hook") && std::env::var("PLUME_CLIPBOARD_HOTKEY").is_ok_and(|v| v.eq_ignore_ascii_case("none"))
}

/// The shortcut to register: the setting, or PLUME_CLIPBOARD_HOTKEY ("none": no shortcut; tests never take the
/// user's Ctrl+Shift+V).
pub fn wanted(setting: &str) -> Option<Hotkey> {
    match std::env::var("PLUME_CLIPBOARD_HOTKEY") {
        Ok(v) if v.eq_ignore_ascii_case("none") => None,
        Ok(v) => Hotkey::parse(&v),
        Err(_) => Hotkey::parse(setting),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_label() {
        let h = Hotkey::parse("Ctrl+Shift+V").unwrap();
        assert!(h.ctrl && h.shift && !h.alt && h.vk == 0x56);
        assert_eq!(h.label(), "Ctrl+Shift+V");
        assert_eq!(Hotkey::parse("shift + ctrl + v"), Some(h));
        assert_eq!(Hotkey::parse("Win+Alt+F9").unwrap().label(), "Alt+Win+F9");
        assert_eq!(Hotkey::parse("Ctrl+Space").unwrap().vk, 0x20);
        assert_eq!(Hotkey::parse("Ctrl+;").unwrap().vk, 0xBA);
        assert!(Hotkey::parse("Shift+V").is_none(), "a shortcut needs Ctrl, Alt or Win");
        assert!(Hotkey::parse("V").is_none());
        assert!(Hotkey::parse("Ctrl+Nope").is_none());
        assert!(Hotkey::parse("Hyper+V").is_none());
        assert!(Hotkey::parse("").is_none());
    }
}
