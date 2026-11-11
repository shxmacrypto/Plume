//! What both windows share: the history, the settings, the theme, and what goes to the clipboard.

use crate::{
    clip::{self, Put},
    hotkey::{self, Hotkey},
    image, input,
    settings::Settings,
    store::{self, Item, Kind, Store},
};
use plume_ui::{Color, Theme, Window};
use std::path::PathBuf;
use windows::Win32::Foundation::HWND;

/// Timers of the main window.
pub const SAVE: usize = 1;
pub const PRUNE: usize = 2;
pub const RETRY: usize = 3;
pub const TOAST: usize = 4;

pub fn system_dark() -> bool {
    use windows::{
        Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        core::w,
    };
    if let Ok(t) = std::env::var("PLUME_SUITE_THEME") {
        return t != "light";
    }
    let (mut data, mut size) = (1u32, 4u32);
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    read.is_ok() && data == 0
}

/// Plume Clipboard's colours: Plume's theme with its fuchsia accent.
pub fn theme_for(set: &Settings) -> Theme {
    let dark = match set.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => system_dark(),
    };
    Theme::new(dark, Some(if dark { Color::hex(0xe879f9) } else { Color::hex(0xa21caf) }))
}

pub struct Model {
    pub dir: PathBuf,
    pub store: Store,
    pub set: Settings,
    pub theme: Theme,
    pub dirty: bool,
    /// Grows at each change of the history: the windows rebuild their lists.
    pub version: u64,
    /// The clipboard's sequence number after Plume's own last write (that change is not a new copy).
    pub own_seq: u32,
    pub autostart: bool,
    /// The shortcut in service, and whether Windows accepted it.
    pub hotkey: Option<Hotkey>,
    pub hotkey_ok: bool,
    pub main: Option<Window>,
    pub panel: Option<Window>,
    /// What happened, for the test socket.
    pub log: Vec<String>,
}

impl Model {
    pub fn load() -> Model {
        let dir = store::dir();
        let set = Settings::load(&dir);
        let mut store = Store::load_from(&dir);
        let removed = store.prune(set.max_items as usize, set.days, store::now());
        for it in &removed {
            store::delete_image(&dir, it.id);
        }
        let theme = theme_for(&set);
        Model {
            dirty: !removed.is_empty(),
            dir,
            store,
            set,
            theme,
            version: 1,
            own_seq: 0,
            autostart: crate::sys::autostart(),
            hotkey: None,
            hotkey_ok: false,
            main: None,
            panel: None,
            log: Vec::new(),
        }
    }

    pub fn note(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    pub fn main_hwnd(&self) -> HWND {
        self.main.map(|w| w.hwnd).unwrap_or_default()
    }

    /// The history changed: both windows are redrawn, and it is saved a moment later.
    pub fn changed(&mut self) {
        self.dirty = true;
        self.version += 1;
        if let Some(m) = self.main {
            m.timer(SAVE, Some(1200));
            m.repaint();
        }
        if let Some(p) = self.panel {
            p.repaint();
        }
    }

    pub fn save(&mut self) {
        if self.dirty && self.store.save_to(&self.dir, self.set.encrypt) {
            self.dirty = false;
        }
    }

    pub fn save_settings(&self) {
        self.set.save(&self.dir);
    }

    /// Registers the shortcut of the settings (or the test's); false if another app already has it.
    pub fn register_hotkey(&mut self) -> bool {
        let hwnd = self.main_hwnd();
        hotkey::unregister(hwnd);
        self.hotkey = hotkey::wanted(&self.set.hotkey);
        self.hotkey_ok = self.hotkey.is_some_and(|h| h.register(hwnd));
        let msg =
            format!("hotkey {} {}", self.hotkey.map(|h| h.label()).unwrap_or("none".into()), if self.hotkey_ok { "registered" } else { "not registered" });
        self.note(msg);
        self.hotkey_ok
    }

    /// Why a copy from `app` is not kept now, if it is not.
    pub fn refuse(&self, app: &str) -> Option<&'static str> {
        if self.set.paused {
            Some("paused")
        } else if self.set.ignores(app) {
            Some("ignored app")
        } else {
            None
        }
    }

    fn added(&mut self, id: u64, fresh: bool, what: &str) {
        let removed = self.store.prune(self.set.max_items as usize, self.set.days, store::now());
        for it in &removed {
            store::delete_image(&self.dir, it.id);
        }
        self.note(format!("{} {what} #{id}", if fresh { "added" } else { "moved up" }));
        self.changed();
    }

    pub fn add_text(&mut self, text: String, html: Option<String>, rtf: Option<String>, app: &str) -> u64 {
        let kind = clip::kind_of_text(&text);
        let it = Item { html, rtf, app: app.to_string(), ..Item::text(&text, kind) };
        let (id, fresh) = self.store.add(it, store::now());
        self.added(id, fresh, if kind == Kind::Link { "link" } else { "text" });
        id
    }

    pub fn add_files(&mut self, files: Vec<String>, app: &str) -> u64 {
        let (id, fresh) = self.store.add(Item { app: app.to_string(), ..Item::files(files) }, store::now());
        self.added(id, fresh, "files");
        id
    }

    /// A picture already turned into PNG (by a worker thread). None if it is larger than the setting allows.
    pub fn add_image(&mut self, w: u32, h: u32, hash: u64, png: &[u8], thumb: &[u8], app: &str) -> Option<u64> {
        if png.len() as u64 > self.set.max_image_mb as u64 * 1024 * 1024 {
            self.note(format!("skipped picture: {} bytes", png.len()));
            return None;
        }
        let it = Item { app: app.to_string(), ..Item::image(w, h, hash, png.len() as u64) };
        let (id, fresh) = self.store.add(it, store::now());
        if fresh {
            let enc = self.set.encrypt;
            store::write_image(&self.dir, id, false, png, enc);
            store::write_image(&self.dir, id, true, thumb, enc);
        }
        self.added(id, fresh, "image");
        Some(id)
    }

    pub fn remove(&mut self, id: u64) {
        if self.store.remove(id).is_some() {
            store::delete_image(&self.dir, id);
            self.note(format!("removed #{id}"));
            self.changed();
        }
    }

    pub fn clear(&mut self) {
        for it in self.store.clear() {
            store::delete_image(&self.dir, it.id);
        }
        self.note("cleared");
        self.changed();
    }

    pub fn set_pin(&mut self, id: u64, on: bool) {
        self.store.set_pin(id, on);
        self.changed();
    }

    fn write(&mut self, put: &Put, what: &str) -> bool {
        // tests: the user's clipboard stays theirs, the write is only written down
        if input::log_fake(&format!("clipboard <- {what}")) {
            self.note(format!("clipboard <- {what}"));
            return true;
        }
        match clip::write(self.main_hwnd(), put) {
            Some(seq) => {
                self.own_seq = seq;
                true
            }
            None => false,
        }
    }

    /// Puts an item back on the clipboard (`plain`: the text without its formatting) and moves it to the top.
    pub fn put(&mut self, id: u64, plain: bool) -> bool {
        let Some(it) = self.store.get(id).cloned() else { return false };
        let ok = self.put_item(&it, plain, false);
        if ok {
            self.store.touch(id, store::now());
            self.changed();
        }
        ok
    }

    /// The write itself; `real`: always on the clipboard even in a test, marked private (the round trip of the
    /// test socket: Windows' clipboard history does not keep it).
    pub fn put_item(&mut self, it: &Item, plain: bool, real: bool) -> bool {
        let what = format!("#{} {:?}{} {}", it.id, it.kind, if plain { " plain" } else { "" }, it.preview(40));
        let png;
        let dib;
        let files_text;
        let put = match it.kind {
            Kind::Text | Kind::Link => {
                Put::Text { text: &it.text, html: if plain { None } else { it.html.as_deref() }, rtf: if plain { None } else { it.rtf.as_deref() } }
            }
            Kind::Files if plain => {
                files_text = it.files.join("\r\n");
                Put::Text { text: &files_text, html: None, rtf: None }
            }
            Kind::Files => Put::Files(&it.files),
            Kind::Image => {
                let Some(p) = store::read_image(&self.dir, it.id, false) else { return false };
                let Some(b) = image::decode(&p) else { return false };
                png = p;
                dib = image::to_dib(&b);
                Put::Image { png: &png, dib: &dib }
            }
        };
        if real {
            return match clip::write_ex(self.main_hwnd(), &put, true) {
                Some(seq) => {
                    self.own_seq = seq;
                    true
                }
                None => false,
            };
        }
        self.write(&put, &what)
    }

    pub fn put_snippet(&mut self, id: u64) -> bool {
        let Some(s) = self.store.snippet(id).cloned() else { return false };
        self.write(&Put::Text { text: &s.text, html: None, rtf: None }, &format!("snippet {}", s.trigger))
    }

    /// The encryption setting changed: the history and its pictures are written again the new way.
    pub fn reseal(&mut self) {
        store::reseal_images(&self.dir, &self.store, self.set.encrypt);
        self.dirty = true;
        self.save();
    }
}
