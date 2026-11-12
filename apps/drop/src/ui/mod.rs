//! The window of Plume Drop: the QR code for the phones, what to send them, the connected devices and the history.
//! The server runs on its own threads and reports here through the window's sender.

mod paint;

use crate::{
    hub::{DevState, DeviceView, Hub, HubEvent},
    i18n::{self, fill, tr},
    net,
    settings::{self, Dir, Entry, Kind, Settings, State},
    util,
};
use plume_ui::{Area, Button, Canvas, Cursor, Cx, Event, Gfx, Handler, Hits, PaintCx, Pt, Theme, Window, dialogs, m};
use std::{
    any::Any,
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

const TICK: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    /// The title bar (grabs the window).
    Title,
    Min,
    Max,
    Close,
    Settings,
    Back,
    CopyLink,
    NewCode,
    SendFiles,
    SendText,
    ComposeSend,
    ComposeCancel,
    Disconnect(usize),
    ClearHistory,
    Open(u64),
    Folder(u64),
    Copy(u64),
    Stop(u64),
    Allow,
    Refuse,
    ChangeFolder,
    OpenFolder,
    CopyTextSwitch,
    OpenLinksSwitch,
    Lang(u8),
    /// A history row and a device row (hover only).
    Row(u64),
    DevRow(usize),
    /// The buttons of the empty lists.
    EmptyCopyLink,
    EmptySendFiles,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Home,
    Settings,
}

/// A device that asks to connect.
pub struct Ask {
    pub device: String,
    pub name: String,
}

pub struct App {
    pub win: Window,
    gfx: Rc<Gfx>,
    pub theme: Theme,
    pub hits: Hits<Id>,
    pub hover: Option<Id>,
    pressed: Option<Id>,
    pub settings: Settings,
    pub hub: Arc<Hub>,
    /// The port the server listens on (None: it could not start).
    pub port: Option<u16>,
    pub ip: Option<Ipv4Addr>,
    ip_checked: Instant,
    /// Newest first.
    pub history: Vec<Entry>,
    history_dirty: bool,
    pub devices: Vec<DeviceView>,
    pub asks: Vec<Ask>,
    pub view: View,
    pub composing: bool,
    compose_pending: bool,
    pub toast: Option<(String, Instant)>,
    copied: Option<Instant>,
    pub scroll: f32,
    content_h: f32,
    /// What happened, for the tests.
    pub log: Vec<String>,
}

pub fn with_app(h: &mut dyn Handler) -> Option<&mut App> {
    h.as_any().downcast_mut::<App>()
}

/// Opens the file's folder, the file selected (in Plume FEx when it opens folders).
fn show_in_folder(p: &Path) {
    plume_ui::folder::show(p);
}

/// The address the server listens on: every network (phones), or only this PC in tests.
fn bind_ip() -> IpAddr {
    #[cfg(feature = "test-hook")]
    if let Some(ip) = std::env::var("PLUME_DROP_BIND").ok().and_then(|s| s.parse().ok()) {
        return ip;
    }
    IpAddr::V4(Ipv4Addr::UNSPECIFIED)
}

impl App {
    pub fn new(win: Window, gfx: Rc<Gfx>, theme: Theme) -> App {
        let mut settings = Settings::load();
        #[cfg(feature = "test-hook")]
        if let Ok(t) = std::env::var("PLUME_DROP_TOKEN") {
            settings.token = t;
        }
        #[cfg(feature = "test-hook")]
        if let Some(p) = std::env::var("PLUME_DROP_PORT").ok().and_then(|p| p.parse().ok()) {
            settings.port = p;
        }
        if settings.port == 0 {
            settings.port = settings::PORT;
        }
        i18n::set(&settings.lang);
        let sender = win.sender();
        let hub = Hub::new(settings.token.clone(), settings.folder.clone(), settings::data_dir().join("devices.json"), move |ev| {
            sender.send(move |h, cx| {
                if let Some(app) = with_app(h) {
                    app.on_hub(ev, cx);
                }
            });
        });
        let port = hub.start(bind_ip(), settings.port).ok();
        let devices = hub.devices();
        win.timer(TICK, Some(1000));
        App {
            win,
            gfx,
            theme,
            hits: Hits::default(),
            hover: None,
            pressed: None,
            settings,
            hub,
            port,
            ip: net::lan_ip(),
            ip_checked: Instant::now(),
            history: settings::load_history(),
            history_dirty: false,
            devices,
            asks: Vec::new(),
            view: View::Home,
            composing: false,
            compose_pending: false,
            toast: None,
            copied: None,
            scroll: 0.0,
            content_h: 0.0,
            log: Vec::new(),
        }
    }

    /// The page for the phones, with the code after # (browsers keep that part to themselves).
    pub fn link(&self) -> Option<String> {
        let (ip, port) = (self.ip?, self.port?);
        Some(format!("http://{ip}:{port}/#{}", self.settings.token))
    }

    fn note(&mut self, what: String) {
        if self.log.len() > 300 {
            self.log.remove(0);
        }
        self.log.push(what);
    }

    pub fn say(&mut self, text: String, cx: &mut Cx) {
        self.toast = Some((text, Instant::now()));
        cx.repaint();
    }

    fn entry(&mut self, id: u64) -> Option<&mut Entry> {
        self.history.iter_mut().find(|e| e.id == id)
    }

    fn save_history(&mut self) {
        settings::save_history(&self.history);
        self.history_dirty = false;
    }

    fn refresh_devices(&mut self) {
        self.devices = self.hub.devices();
        // a question whose device is no longer waiting is gone
        let waiting: Vec<String> = self.devices.iter().filter(|d| d.state == DevState::Pending).map(|d| d.id.clone()).collect();
        self.asks.retain(|a| waiting.contains(&a.device));
    }

    // ───────────────────────── the server ─────────────────────────

    pub fn on_hub(&mut self, ev: HubEvent, cx: &mut Cx) {
        match ev {
            HubEvent::Ask { device, name } => {
                self.note(format!("ask {name}"));
                if !self.asks.iter().any(|a| a.device == device) {
                    self.asks.push(Ask { device, name });
                }
                if std::env::var_os("PLUME_DROP_NOACTIVATE").is_none() {
                    self.win.bring_to_front();
                }
                self.refresh_devices();
            }
            HubEvent::Devices => self.refresh_devices(),
            HubEvent::Incoming { id, name, size, device } => {
                self.note(format!("incoming {name} {size}"));
                self.history.insert(
                    0,
                    Entry { id, dir: Dir::In, kind: Kind::File, name, path: None, size, done: 0, state: State::Running, device, at: util::now(), saves: 0 },
                );
            }
            HubEvent::Progress { id, done } => {
                if let Some(e) = self.entry(id) {
                    e.done = done;
                }
            }
            HubEvent::Received { id, path } => {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.note(format!("received {name}"));
                if let Some(e) = self.entry(id) {
                    e.state = State::Done;
                    e.done = e.size;
                    e.name = name.clone();
                    e.path = Some(path);
                }
                self.say(fill(tr("Received {}"), &[&name]), cx);
                self.save_history();
            }
            HubEvent::Failed { id } => {
                self.note(format!("failed {id}"));
                if let Some(e) = self.entry(id) {
                    e.state = State::Failed;
                }
                self.save_history();
            }
            HubEvent::Text { id, text, device } => {
                let link = util::is_link(&text);
                self.note(format!("text {}", if link { "link" } else { "text" }));
                let text = if link { text.trim().to_string() } else { text };
                let size = text.len() as u64;
                let kind = if link { Kind::Link } else { Kind::Text };
                self.history.insert(
                    0,
                    Entry { id, dir: Dir::In, kind, name: text.clone(), path: None, size, done: size, state: State::Done, device, at: util::now(), saves: 0 },
                );
                if link {
                    if self.settings.open_links {
                        self.win.open(&text);
                    }
                    self.say(tr("Link received").into(), cx);
                } else if self.settings.copy_text && self.win.copy_text(&text) {
                    self.say(tr("Text copied to the clipboard").into(), cx);
                } else {
                    self.say(tr("Received text").into(), cx);
                }
                self.save_history();
            }
            HubEvent::Saved { offer, device, done } => {
                self.note(format!("saved {offer} {done}"));
                if let Some(e) = self.entry(offer) {
                    e.device = device;
                    if done {
                        e.saves += 1;
                    }
                }
                self.history_dirty = true;
            }
        }
        cx.repaint();
    }

    /// Offers files to the phones (picked, dropped on the window, or given on the command line).
    pub fn share(&mut self, paths: Vec<PathBuf>, cx: &mut Cx) {
        let offers = self.hub.offer_files(&paths);
        if offers.is_empty() {
            return;
        }
        let n = offers.len();
        for o in offers {
            self.history.insert(
                0,
                Entry {
                    id: o.id,
                    dir: Dir::Out,
                    kind: Kind::File,
                    name: o.name,
                    path: o.path,
                    size: o.size,
                    done: 0,
                    state: State::Shared,
                    device: String::new(),
                    at: util::now(),
                    saves: 0,
                },
            );
        }
        self.note(format!("shared {n}"));
        self.view = View::Home;
        self.scroll = 0.0;
        self.say(fill(tr("{} file(s) offered to your phones"), &[&n.to_string()]), cx);
        self.save_history();
    }

    pub fn share_text(&mut self, text: &str, cx: &mut Cx) {
        if text.trim().is_empty() {
            return;
        }
        let o = self.hub.offer_text(text);
        self.history.insert(
            0,
            Entry {
                id: o.id,
                dir: Dir::Out,
                kind: o.kind,
                name: o.name,
                path: None,
                size: o.size,
                done: 0,
                state: State::Shared,
                device: String::new(),
                at: o.at,
                saves: 0,
            },
        );
        self.note("shared text".into());
        self.say(tr("Offered to your phones").into(), cx);
        self.save_history();
    }

    fn answer(&mut self, allow: bool, cx: &mut Cx) {
        if self.asks.is_empty() {
            return;
        }
        let a = self.asks.remove(0);
        self.note(format!("{} {}", if allow { "allow" } else { "refuse" }, a.name));
        self.hub.answer(&a.device, allow);
        self.refresh_devices();
        cx.repaint();
    }

    fn close_compose(&mut self, cx: &mut Cx) {
        self.composing = false;
        cx.after(|w| {
            w.edit_set_text("");
            w.edit_hide();
        });
    }

    pub fn set_lang(&mut self, lang: &str) {
        self.settings.lang = lang.to_string();
        i18n::set(lang);
        self.settings.save();
    }

    // ───────────────────────── actions ─────────────────────────

    pub fn act(&mut self, id: Id, cx: &mut Cx) {
        match id {
            Id::Title | Id::Row(_) | Id::DevRow(_) => {}
            Id::EmptyCopyLink => self.act(Id::CopyLink, cx),
            Id::EmptySendFiles => self.act(Id::SendFiles, cx),
            Id::Min => cx.after(|w| w.minimize()),
            Id::Max => cx.after(|w| w.toggle_maximize()),
            Id::Close => cx.after(|w| w.close()),
            Id::Settings => {
                if self.composing {
                    self.close_compose(cx);
                }
                self.view = View::Settings;
                self.scroll = 0.0;
            }
            Id::Back => {
                self.view = View::Home;
                self.scroll = 0.0;
            }
            Id::CopyLink => {
                if let Some(l) = self.link()
                    && self.win.copy_text(&l)
                {
                    self.copied = Some(Instant::now());
                }
            }
            Id::NewCode => {
                self.settings.token = util::new_token();
                self.settings.save();
                self.hub.set_token(&self.settings.token);
                self.note("new code".into());
                self.say(tr("New code: phones must scan again").into(), cx);
            }
            Id::SendFiles => {
                let title = tr("Choose files to send");
                cx.after(move |w| {
                    let paths = dialogs::pick_files(w.hwnd, title);
                    w.with(|h, cx| {
                        if let Some(app) = with_app(h) {
                            app.share(paths, cx);
                        }
                    });
                });
            }
            Id::SendText => {
                self.composing = true;
                self.compose_pending = true;
            }
            Id::ComposeSend => {
                let text = self.win.edit_text().unwrap_or_default();
                if !text.trim().is_empty() {
                    self.share_text(&text, cx);
                    self.close_compose(cx);
                }
            }
            Id::ComposeCancel => self.close_compose(cx),
            Id::Disconnect(i) => {
                if let Some(d) = self.devices.get(i).cloned() {
                    self.hub.revoke(&d.id);
                    self.note(format!("disconnect {}", d.name));
                    self.refresh_devices();
                    self.say(fill(tr("{} disconnected"), &[&d.name]), cx);
                }
            }
            Id::ClearHistory => {
                self.history.retain(|e| matches!(e.state, State::Running | State::Shared));
                self.save_history();
            }
            Id::Open(id) => {
                if let Some(e) = self.history.iter().find(|e| e.id == id) {
                    match (&e.path, e.kind) {
                        (Some(p), Kind::File) => {
                            self.win.open(&p.to_string_lossy());
                        }
                        (_, Kind::Link) => {
                            self.win.open(&e.name);
                        }
                        _ => {}
                    }
                }
            }
            Id::Folder(id) => {
                if let Some(p) = self.history.iter().find(|e| e.id == id).and_then(|e| e.path.clone()) {
                    show_in_folder(&p);
                }
            }
            Id::Copy(id) => {
                if let Some(text) = self.history.iter().find(|e| e.id == id).map(|e| e.name.clone())
                    && self.win.copy_text(&text)
                {
                    self.say(tr("Copied").into(), cx);
                }
            }
            Id::Stop(id) => {
                self.hub.withdraw(id);
                if let Some(e) = self.entry(id) {
                    e.state = State::Done;
                }
                self.save_history();
            }
            Id::Allow => self.answer(true, cx),
            Id::Refuse => self.answer(false, cx),
            Id::ChangeFolder => {
                let title = tr("Choose where received files go");
                cx.after(move |w| {
                    if let Some(p) = dialogs::pick_folder(w.hwnd, title) {
                        w.with(|h, cx| {
                            if let Some(app) = with_app(h) {
                                app.hub.set_folder(&p);
                                app.settings.folder = p;
                                app.settings.save();
                                cx.repaint();
                            }
                        });
                    }
                });
            }
            Id::OpenFolder => {
                let _ = std::fs::create_dir_all(&self.settings.folder);
                self.win.open(&self.settings.folder.to_string_lossy());
            }
            Id::CopyTextSwitch => {
                self.settings.copy_text = !self.settings.copy_text;
                self.settings.save();
            }
            Id::OpenLinksSwitch => {
                self.settings.open_links = !self.settings.open_links;
                self.settings.save();
            }
            Id::Lang(k) => self.set_lang(["system", "en", "fr"][k.min(2) as usize]),
        }
        cx.repaint();
    }

    fn tick(&mut self, cx: &mut Cx) {
        if self.hub.prune() {
            self.note("pruned".into());
        }
        self.refresh_devices();
        if self.ip_checked.elapsed() > Duration::from_secs(10) {
            self.ip_checked = Instant::now();
            self.ip = net::lan_ip();
        }
        if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(4)) {
            self.toast = None;
        }
        if self.history_dirty {
            self.save_history();
        }
        cx.repaint();
    }

    pub fn copied_recently(&self) -> bool {
        self.copied.is_some_and(|at| at.elapsed() < Duration::from_secs(2))
    }

    pub fn gfx(&self) -> &Rc<Gfx> {
        &self.gfx
    }

    pub fn max_scroll(&self, view_h: f32) -> f32 {
        (self.content_h - view_h).max(0.0)
    }
}

impl Handler for App {
    fn paint(&mut self, c: &mut Canvas, cx: &mut PaintCx) {
        paint::paint(self, c, cx);
    }

    fn event(&mut self, ev: Event, cx: &mut Cx) {
        match ev {
            Event::MouseMove { p, .. } => {
                let h = self.hits.at(p).copied();
                if h != self.hover {
                    self.hover = h;
                    cx.repaint();
                }
            }
            Event::MouseLeave => {
                self.hover = None;
                cx.repaint();
            }
            Event::MouseDown { p, button: Button::Left, .. } => {
                self.pressed = self.hits.at(p).copied();
                cx.capture(true);
                cx.repaint();
            }
            Event::MouseUp { p, button: Button::Left, .. } => {
                cx.capture(false);
                let up = self.hits.at(p).copied();
                if let (Some(a), Some(b)) = (self.pressed.take(), up)
                    && a == b
                {
                    // while a device asks, only the answer (and the window buttons) count
                    if self.asks.is_empty() || matches!(a, Id::Allow | Id::Refuse | Id::Min | Id::Max | Id::Close) {
                        self.act(a, cx);
                    }
                }
                cx.repaint();
            }
            Event::Wheel { dy, .. } => {
                self.scroll = (self.scroll - dy * 64.0).max(0.0);
                cx.repaint();
            }
            Event::EditKey { vk: 0x0D, .. } => self.act(Id::ComposeSend, cx),
            Event::EditKey { vk: 0x1B, .. } => self.act(Id::ComposeCancel, cx),
            Event::Key { vk: 0x1B, .. } if !self.asks.is_empty() => self.answer(false, cx),
            Event::Key { vk: 0x1B, .. } if self.view == View::Settings => self.act(Id::Back, cx),
            Event::Key { vk: 0x56, mods, .. } if mods.ctrl && self.view == View::Home => {
                // Ctrl+V: files copied in Explorer are offered
                let paths = clipboard_files(self.win.hwnd);
                if !paths.is_empty() {
                    self.share(paths, cx);
                }
            }
            Event::Timer(TICK) => self.tick(cx),
            Event::Opened(text) => {
                let paths: Vec<PathBuf> = parse_args(&text).into_iter().map(PathBuf::from).collect();
                if !paths.is_empty() {
                    self.share(paths, cx);
                }
                self.win.bring_to_front();
            }
            Event::Close => {
                self.save_history();
            }
            Event::Resized | Event::Scale | Event::Active(_) | Event::SystemTheme => cx.repaint(),
            _ => {}
        }
    }

    fn area(&self, p: Pt) -> Area {
        match self.hits.at(p) {
            Some(Id::Max) => Area::MaxButton,
            Some(Id::Min | Id::Close | Id::Settings | Id::Back) => Area::Control,
            Some(Id::Title) => Area::Caption,
            _ if p.y < m::TITLE_H => Area::Caption,
            _ => Area::Client,
        }
    }

    fn cursor(&self, p: Pt) -> Cursor {
        match self.hits.at(p) {
            Some(Id::Min | Id::Max | Id::Close | Id::Title | Id::Row(_) | Id::DevRow(_)) | None => Cursor::Arrow,
            Some(_) => Cursor::Hand,
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

/// `"a b.txt" "c.txt"` → the paths (as `main` joins them for a running Plume Drop).
pub fn parse_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Files copied in Explorer (Ctrl+C), if the clipboard holds some.
fn clipboard_files(hwnd: windows::Win32::Foundation::HWND) -> Vec<PathBuf> {
    use windows::Win32::{
        System::DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard},
        UI::Shell::HDROP,
    };
    const CF_HDROP: u32 = 15;
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP).is_err() || OpenClipboard(Some(hwnd)).is_err() {
            return Vec::new();
        }
        let out = match GetClipboardData(CF_HDROP) {
            Ok(h) => dropped_files(HDROP(h.0)),
            Err(_) => Vec::new(),
        };
        let _ = CloseClipboard();
        out
    }
}

/// The paths in a drop handle (dropped on the window, or copied in Explorer).
pub fn dropped_files(drop: windows::Win32::UI::Shell::HDROP) -> Vec<PathBuf> {
    use windows::Win32::UI::Shell::DragQueryFileW;
    let mut out = Vec::new();
    unsafe {
        let n = DragQueryFileW(drop, u32::MAX, None);
        for i in 0..n {
            let len = DragQueryFileW(drop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            let got = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
            out.push(PathBuf::from(String::from_utf16_lossy(&buf[..got])));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args() {
        assert_eq!(parse_args(r#""C:\a b\c.txt" "D:\x.jpg""#), vec![r"C:\a b\c.txt", r"D:\x.jpg"]);
        assert_eq!(parse_args(""), Vec::<String>::new());
    }
}
