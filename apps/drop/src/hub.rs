//! The server on the local network. Phones open its page (from the QR code), then talk to it with small JSON
//! requests: who they are, what the PC offers, and the files and texts they send. Every API request carries the
//! code from the QR code; a phone seen for the first time waits until the person at the PC lets it in.
//!
//! One thread per connection, one request per connection. Files are streamed to and from the disk in 64 KB pieces,
//! never held whole in memory.

use crate::{
    http::{self, Request},
    settings::{Kind, read_json, write_json},
    util::{human_size, is_image, mime, now, safe_name, same_secret, unique_path, url_encode},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    net::{IpAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const PAGE: &str = include_str!("web/index.html");
const SCRIPT: &str = include_str!("web/app.js");
const STYLE: &str = include_str!("web/app.css");
const ICON: &[u8] = include_bytes!("../../../assets/brand/plume-drop.svg");

/// A phone polls at least this often while its page is open.
const POLL_WAIT: Duration = Duration::from_secs(20);
/// Seen this recently: connected.
pub const CONNECTED_FOR: Duration = Duration::from_secs(45);
/// A phone waiting for an answer that has gone away for this long is forgotten.
const PENDING_FOR: Duration = Duration::from_secs(90);
const MAX_CONNECTIONS: usize = 48;
/// Wrong codes from one address before it is refused for a while.
const MAX_FAILS: u32 = 30;
const FAIL_WINDOW: Duration = Duration::from_secs(600);
const CHUNK: usize = 64 * 1024;
const TEXT_MAX: u64 = 1024 * 1024;
const OFFERS_MAX: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevState {
    /// Waits for the person at the PC.
    Pending,
    Allowed,
    /// Refused this time; the phone may ask again.
    Refused,
    /// Disconnected from the PC; the phone may ask again.
    Revoked,
}

impl DevState {
    pub fn word(self) -> &'static str {
        match self {
            DevState::Pending => "pending",
            DevState::Allowed => "allowed",
            DevState::Refused => "refused",
            DevState::Revoked => "revoked",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub state: DevState,
    pub seen: Instant,
    pub ip: Option<IpAddr>,
    asked: Instant,
}

/// A device let in once: it comes back without asking.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Known {
    pub id: String,
    pub name: String,
    pub added: u64,
}

/// What the PC offers to the phones.
#[derive(Clone, Debug)]
pub struct Offer {
    pub id: u64,
    pub kind: Kind,
    pub name: String,
    pub path: Option<PathBuf>,
    pub size: u64,
    pub at: u64,
}

/// What the window hears from the server (from its threads).
#[derive(Debug)]
pub enum HubEvent {
    /// A new device asks to connect.
    Ask {
        device: String,
        name: String,
    },
    /// The device list changed.
    Devices,
    Incoming {
        id: u64,
        name: String,
        size: u64,
        device: String,
    },
    Progress {
        id: u64,
        done: u64,
    },
    Received {
        id: u64,
        path: PathBuf,
    },
    Failed {
        id: u64,
    },
    Text {
        id: u64,
        text: String,
        device: String,
    },
    /// A phone saved something the PC offered (`done`: the whole file went through).
    Saved {
        offer: u64,
        device: String,
        done: bool,
    },
}

/// A device as the window lists it.
#[derive(Clone, Debug)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub state: DevState,
    pub connected: bool,
    pub known: bool,
}

struct Inner {
    token: String,
    folder: PathBuf,
    devices: HashMap<String, Device>,
    known: Vec<Known>,
    known_file: PathBuf,
    offers: Vec<Offer>,
    /// Grows at each change the phones must hear about; starts at the clock so it keeps growing across launches.
    rev: u64,
    next: u64,
    fails: HashMap<IpAddr, (u32, Instant)>,
    pc: String,
}

pub struct Hub {
    inner: Mutex<Inner>,
    cv: Condvar,
    notify: Box<dyn Fn(HubEvent) + Send + Sync>,
    active: AtomicUsize,
}

fn valid_device_id(id: &str) -> bool {
    (16..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn clean_device_name(name: &str) -> String {
    let n: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    let n = n.trim().to_string();
    if n.is_empty() { "Device".into() } else { n }
}

impl Inner {
    fn bump(&mut self) {
        self.rev += 1;
    }

    fn save_known(&self) {
        write_json(&self.known_file, &self.known);
    }

    /// A request from device `id`: its state now, and whether the PC must be asked.
    fn touch(&mut self, id: &str, name: Option<&str>, ip: Option<IpAddr>, retry: bool) -> (DevState, bool) {
        let now = Instant::now();
        let known = self.known.iter().find(|k| k.id == id).cloned();
        let mut ask = false;
        let mut changed = false;
        let dev = self.devices.entry(id.to_string()).or_insert_with(|| {
            changed = true;
            let (state, label) = match &known {
                Some(k) => (DevState::Allowed, k.name.clone()),
                None => {
                    ask = true;
                    (DevState::Pending, clean_device_name(name.unwrap_or("")))
                }
            };
            Device { id: id.to_string(), name: label, state, seen: now, ip, asked: now }
        });
        dev.seen = now;
        dev.ip = ip;
        if let Some(n) = name {
            let n = clean_device_name(n);
            if n != dev.name {
                dev.name = n;
                changed = true;
            }
        }
        if retry && matches!(dev.state, DevState::Refused | DevState::Revoked) && now.duration_since(dev.asked) > Duration::from_secs(3) {
            dev.state = DevState::Pending;
            dev.asked = now;
            ask = true;
            changed = true;
        }
        let state = dev.state;
        let label = dev.name.clone();
        if let Some(k) = self.known.iter_mut().find(|k| k.id == id)
            && k.name != label
        {
            k.name = label;
            self.save_known();
        }
        if changed {
            self.bump();
        }
        (state, ask)
    }

    /// Any other request from device `id`: its state, if it said hello before (or was let in on an earlier launch).
    /// Never asks the PC.
    fn seen(&mut self, id: &str, ip: Option<IpAddr>) -> Option<DevState> {
        if let Some(d) = self.devices.get_mut(id) {
            d.seen = Instant::now();
            d.ip = ip;
            return Some(d.state);
        }
        let k = self.known.iter().find(|k| k.id == id)?.clone();
        let now = Instant::now();
        self.devices.insert(id.to_string(), Device { id: k.id, name: k.name, state: DevState::Allowed, seen: now, ip, asked: now });
        self.bump();
        Some(DevState::Allowed)
    }

    fn answer(&mut self, id: &str, allow: bool) -> bool {
        let Some(d) = self.devices.get_mut(id) else { return false };
        if d.state != DevState::Pending {
            return false;
        }
        d.state = if allow { DevState::Allowed } else { DevState::Refused };
        d.asked = Instant::now();
        if allow && !self.known.iter().any(|k| k.id == id) {
            let name = d.name.clone();
            self.known.push(Known { id: id.to_string(), name, added: now() });
            self.save_known();
        }
        self.bump();
        true
    }

    fn revoke(&mut self, id: &str) {
        let before = self.known.len();
        self.known.retain(|k| k.id != id);
        if self.known.len() != before {
            self.save_known();
        }
        if let Some(d) = self.devices.get_mut(id) {
            d.state = DevState::Revoked;
            d.asked = Instant::now();
        }
        self.bump();
    }

    fn views(&self) -> Vec<DeviceView> {
        let mut out: Vec<DeviceView> = self
            .devices
            .values()
            .filter(|d| matches!(d.state, DevState::Allowed | DevState::Pending))
            .map(|d| DeviceView {
                id: d.id.clone(),
                name: d.name.clone(),
                state: d.state,
                connected: d.seen.elapsed() < CONNECTED_FOR,
                known: self.known.iter().any(|k| k.id == d.id),
            })
            .collect();
        for k in &self.known {
            if !self.devices.contains_key(&k.id) {
                out.push(DeviceView { id: k.id.clone(), name: k.name.clone(), state: DevState::Allowed, connected: false, known: true });
            }
        }
        out.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        out
    }

    /// Too many wrong codes from this address lately.
    fn blocked(&mut self, ip: Option<IpAddr>) -> bool {
        let Some(ip) = ip else { return false };
        match self.fails.get(&ip) {
            Some((n, since)) if since.elapsed() < FAIL_WINDOW => *n >= MAX_FAILS,
            Some(_) => {
                self.fails.remove(&ip);
                false
            }
            None => false,
        }
    }

    fn failed(&mut self, ip: Option<IpAddr>) {
        if let Some(ip) = ip {
            let e = self.fails.entry(ip).or_insert((0, Instant::now()));
            if e.1.elapsed() >= FAIL_WINDOW {
                *e = (0, Instant::now());
            }
            e.0 += 1;
        }
        if self.fails.len() > 1000 {
            self.fails.clear();
        }
    }
}

impl Hub {
    pub fn new(token: String, folder: PathBuf, known_file: PathBuf, notify: impl Fn(HubEvent) + Send + Sync + 'static) -> Arc<Hub> {
        let known: Vec<Known> = read_json(&known_file);
        let pc = std::env::var("COMPUTERNAME").ok().filter(|n| !n.is_empty()).unwrap_or_else(|| "PC".into());
        let rev = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(1);
        Arc::new(Hub {
            inner: Mutex::new(Inner { token, folder, devices: HashMap::new(), known, known_file, offers: Vec::new(), rev, next: 1, fails: HashMap::new(), pc }),
            cv: Condvar::new(),
            notify: Box::new(notify),
            active: AtomicUsize::new(0),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // a thread that panicked holding the lock leaves the data usable
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// After a change the phones must see: wake their waiting requests.
    fn changed(&self) {
        self.cv.notify_all();
    }

    /// Starts listening: `port` if free, else one of the next ten, else any. Returns the port.
    pub fn start(self: &Arc<Self>, ip: IpAddr, port: u16) -> io::Result<u16> {
        let listener = (0..10u16)
            .filter_map(|k| port.checked_add(k))
            .chain(Some(0))
            .find_map(|p| TcpListener::bind((ip, p)).ok())
            .ok_or_else(|| io::Error::other("no port"))?;
        let port = listener.local_addr()?.port();
        let hub = self.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                if hub.active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    continue; // dropped: closes it
                }
                hub.active.fetch_add(1, Ordering::Relaxed);
                let h = hub.clone();
                std::thread::spawn(move || {
                    let _ = h.serve(stream);
                    h.active.fetch_sub(1, Ordering::Relaxed);
                });
            }
        });
        Ok(port)
    }

    pub fn next_id(&self) -> u64 {
        let mut i = self.lock();
        i.next += 1;
        i.next
    }

    pub fn set_token(&self, token: &str) {
        self.lock().token = token.to_string();
        self.changed();
    }

    pub fn set_folder(&self, folder: &Path) {
        self.lock().folder = folder.to_path_buf();
    }

    pub fn answer(&self, id: &str, allow: bool) -> bool {
        let ok = self.lock().answer(id, allow);
        self.changed();
        ok
    }

    pub fn revoke(&self, id: &str) {
        self.lock().revoke(id);
        self.changed();
        (self.notify)(HubEvent::Devices);
    }

    pub fn devices(&self) -> Vec<DeviceView> {
        self.lock().views()
    }

    pub fn device_name(&self, id: &str) -> Option<String> {
        self.lock().devices.get(id).map(|d| d.name.clone())
    }

    /// Phones that wait for an answer and have gone away are forgotten. True if something changed.
    pub fn prune(&self) -> bool {
        let mut i = self.lock();
        let before = i.devices.len();
        i.devices.retain(|_, d| match d.state {
            DevState::Pending => d.seen.elapsed() < PENDING_FOR,
            _ => d.seen.elapsed() < Duration::from_secs(3600),
        });
        let changed = i.devices.len() != before;
        if changed {
            i.bump();
        }
        changed
    }

    /// Offers files to the phones; returns (offer id, name, size) for each file that exists.
    pub fn offer_files(&self, paths: &[PathBuf]) -> Vec<Offer> {
        let mut out = Vec::new();
        {
            let mut i = self.lock();
            for p in paths {
                let Ok(meta) = std::fs::metadata(p) else { continue };
                if !meta.is_file() {
                    continue;
                }
                i.next += 1;
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
                let o = Offer { id: i.next, kind: Kind::File, name, path: Some(p.clone()), size: meta.len(), at: now() };
                i.offers.push(o.clone());
                out.push(o);
            }
            let extra = i.offers.len().saturating_sub(OFFERS_MAX);
            i.offers.drain(..extra);
            i.bump();
        }
        self.changed();
        out
    }

    pub fn offer_text(&self, text: &str) -> Offer {
        let o = {
            let mut i = self.lock();
            i.next += 1;
            let kind = if crate::util::is_link(text) { Kind::Link } else { Kind::Text };
            let o = Offer { id: i.next, kind, name: text.trim().to_string(), path: None, size: text.len() as u64, at: now() };
            i.offers.push(o.clone());
            let extra = i.offers.len().saturating_sub(OFFERS_MAX);
            i.offers.drain(..extra);
            i.bump();
            o
        };
        self.changed();
        o
    }

    /// No longer offered.
    pub fn withdraw(&self, id: u64) {
        let mut i = self.lock();
        i.offers.retain(|o| o.id != id);
        i.bump();
        drop(i);
        self.changed();
    }

    // ───────────────────────── requests ─────────────────────────

    fn serve(&self, stream: TcpStream) -> io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(60)))?;
        stream.set_write_timeout(Some(Duration::from_secs(60)))?;
        let _ = stream.set_nodelay(true);
        let ip = stream.peer_addr().ok().map(|a| a.ip());
        let raw = stream.try_clone()?;
        let mut reader = BufReader::with_capacity(CHUNK, stream.try_clone()?);
        let mut out = BufWriter::with_capacity(CHUNK, stream);
        let Some(req) = http::read_head(&mut reader)? else { return Ok(()) };
        let r = self.route(&req, &mut reader, &mut out, ip);
        let _ = out.flush();
        drop(out);
        // An answer sent before the body was read (wrong code, refused device): closing with unread data would reset
        // the connection, and the phone could lose the answer. Say we are done, then read what is left (a little).
        let _ = raw.shutdown(std::net::Shutdown::Write);
        let _ = raw.set_read_timeout(Some(Duration::from_secs(2)));
        let mut buf = [0u8; 16 * 1024];
        let mut left: usize = 4 * 1024 * 1024;
        while left > 0 {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => left = left.saturating_sub(n),
            }
        }
        r
    }

    fn route(&self, req: &Request, body: &mut impl Read, w: &mut impl Write, ip: Option<IpAddr>) -> io::Result<()> {
        let page_headers = [(
            "Content-Security-Policy",
            "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'".to_string(),
        )];
        match (req.method.as_str(), req.path.as_str()) {
            // the page itself holds nothing secret: the code stays after # in the address, which browsers never send
            ("GET", "/" | "/index.html") => return http::respond(w, 200, "text/html; charset=utf-8", PAGE.as_bytes(), &page_headers),
            ("GET", "/app.js") => return http::respond(w, 200, "text/javascript; charset=utf-8", SCRIPT.as_bytes(), &[]),
            ("GET", "/app.css") => return http::respond(w, 200, "text/css; charset=utf-8", STYLE.as_bytes(), &[]),
            ("GET", "/icon.svg") => return http::respond(w, 200, "image/svg+xml", ICON, &[]),
            (_, p) if !p.starts_with("/api/") => return http::respond(w, 404, "text/plain", b"Not found", &[]),
            _ => {}
        }
        // every API request: the code, then the device
        let token = req.header("x-drop-token").filter(|t| !t.is_empty()).or_else(|| req.param("t")).unwrap_or("");
        let good = {
            let mut i = self.lock();
            if i.blocked(ip) {
                drop(i);
                return http::json(w, 429, &json!({ "status": "busy" }));
            }
            let good = same_secret(token, &i.token);
            if !good {
                i.failed(ip);
            }
            good
        };
        if !good {
            // slows down guessing (the code has 131 bits anyway)
            std::thread::sleep(Duration::from_millis(300));
            return http::json(w, 401, &json!({ "status": "badcode" }));
        }
        let device = req.header("x-drop-device").filter(|d| !d.is_empty()).or_else(|| req.param("d")).unwrap_or("").to_string();
        if !valid_device_id(&device) {
            return http::json(w, 400, &json!({ "status": "nodevice" }));
        }
        match (req.method.as_str(), req.path.as_str()) {
            ("POST", "/api/hello") => self.hello(req, body, w, &device, ip),
            ("GET", "/api/poll") => self.poll(req, w, &device, ip),
            (method, path) => {
                let state = self.lock().seen(&device, ip);
                if state != Some(DevState::Allowed) {
                    return http::json(w, 403, &json!({ "status": state.map_or("unknown", DevState::word) }));
                }
                let name = self.device_name(&device).unwrap_or_default();
                match (method, path) {
                    ("POST", "/api/upload") => self.upload(req, body, w, &name),
                    ("POST", "/api/text") => self.text(req, body, w, &name),
                    ("GET", p) if p.starts_with("/api/file/") => self.download(req, w, &name, p),
                    _ => http::json(w, 404, &json!({ "status": "notfound" })),
                }
            }
        }
    }

    fn after_touch(&self, device: &str, ask: bool) {
        if ask {
            let name = self.device_name(device).unwrap_or_default();
            (self.notify)(HubEvent::Ask { device: device.to_string(), name });
            (self.notify)(HubEvent::Devices);
            self.changed();
        }
    }

    fn hello(&self, req: &Request, body: &mut impl Read, w: &mut impl Write, device: &str, ip: Option<IpAddr>) -> io::Result<()> {
        let v: serde_json::Value = http::read_body(body, req, 4096).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let name = v["name"].as_str();
        let retry = v["retry"].as_bool().unwrap_or(false);
        let (state, ask) = self.lock().touch(device, name, ip, retry);
        self.after_touch(device, ask);
        if !ask {
            (self.notify)(HubEvent::Devices);
        }
        let pc = self.lock().pc.clone();
        http::json(w, 200, &json!({ "status": state.word(), "pc": pc }))
    }

    /// Waits (up to 20 s) for something newer than `since`, then tells the device where it stands and what is offered.
    fn poll(&self, req: &Request, w: &mut impl Write, device: &str, ip: Option<IpAddr>) -> io::Result<()> {
        let since: u64 = req.param("since").and_then(|s| s.parse().ok()).unwrap_or(0);
        if self.lock().seen(device, ip).is_none() {
            // only the page's hello asks the PC: the page says hello again
            return http::json(w, 200, &json!({ "status": "unknown", "rev": 0 }));
        }
        let guard = self.lock();
        let (mut i, _) =
            self.cv.wait_timeout_while(guard, POLL_WAIT, |i| since != 0 && since >= i.rev && since <= i.rev + 1_000_000).unwrap_or_else(|e| e.into_inner());
        let state = i.devices.get(device).map_or(DevState::Pending, |d| d.state);
        if let Some(d) = i.devices.get_mut(device) {
            d.seen = Instant::now();
        }
        let offers: Vec<serde_json::Value> = if state == DevState::Allowed {
            i.offers
                .iter()
                .rev()
                .map(|o| {
                    json!({
                        "id": o.id,
                        "kind": match o.kind { Kind::File => "file", Kind::Text => "text", Kind::Link => "link" },
                        "name": o.name,
                        "size": o.size,
                        "sizeText": human_size(o.size),
                        "image": o.kind == Kind::File && is_image(&o.name),
                        "at": o.at,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let v = json!({ "status": state.word(), "rev": i.rev, "pc": i.pc, "offers": offers });
        drop(i);
        http::json(w, 200, &v)
    }

    fn upload(&self, req: &Request, body: &mut impl Read, w: &mut impl Write, device: &str) -> io::Result<()> {
        let Some(len) = req.content_length() else { return http::json(w, 411, &json!({ "status": "length" })) };
        let name = safe_name(req.param("name").unwrap_or("file"));
        let folder = self.lock().folder.clone();
        if std::fs::create_dir_all(&folder).is_err() {
            return http::json(w, 500, &json!({ "status": "folder" }));
        }
        let id = self.next_id();
        (self.notify)(HubEvent::Incoming { id, name: name.clone(), size: len, device: device.to_string() });
        let tmp = folder.join(format!(".plumedrop-{id}.part"));
        let result = (|| -> io::Result<u64> {
            let mut file = BufWriter::with_capacity(256 * 1024, File::create(&tmp)?);
            let mut buf = vec![0u8; CHUNK];
            let mut done = 0u64;
            let mut last = Instant::now();
            let mut src = body.take(len);
            loop {
                let n = src.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n])?;
                done += n as u64;
                if last.elapsed() > Duration::from_millis(200) {
                    last = Instant::now();
                    (self.notify)(HubEvent::Progress { id, done });
                }
            }
            file.flush()?;
            Ok(done)
        })();
        match result {
            Ok(done) if done == len => {
                // the final name is chosen under the lock: two uploads of "IMG_1.jpg" at once get two names
                let path = {
                    let _i = self.lock();
                    let path = unique_path(&folder, &name);
                    std::fs::rename(&tmp, &path).map(|_| path)
                };
                match path {
                    Ok(path) => {
                        let saved = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        (self.notify)(HubEvent::Received { id, path });
                        http::json(w, 200, &json!({ "status": "ok", "name": saved }))
                    }
                    Err(_) => {
                        let _ = std::fs::remove_file(&tmp);
                        (self.notify)(HubEvent::Failed { id });
                        http::json(w, 500, &json!({ "status": "save" }))
                    }
                }
            }
            _ => {
                let _ = std::fs::remove_file(&tmp);
                (self.notify)(HubEvent::Failed { id });
                // the phone may be gone already: the answer is best effort
                http::json(w, 400, &json!({ "status": "cut" }))
            }
        }
    }

    fn text(&self, req: &Request, body: &mut impl Read, w: &mut impl Write, device: &str) -> io::Result<()> {
        let Some(b) = http::read_body(body, req, TEXT_MAX) else { return http::json(w, 413, &json!({ "status": "toolong" })) };
        let v: serde_json::Value = serde_json::from_slice(&b).unwrap_or_default();
        let text = v["text"].as_str().unwrap_or("").to_string();
        if text.trim().is_empty() {
            return http::json(w, 400, &json!({ "status": "empty" }));
        }
        let id = self.next_id();
        (self.notify)(HubEvent::Text { id, text, device: device.to_string() });
        http::json(w, 200, &json!({ "status": "ok" }))
    }

    fn download(&self, req: &Request, w: &mut impl Write, device: &str, path: &str) -> io::Result<()> {
        let id: u64 = path.trim_start_matches("/api/file/").parse().unwrap_or(0);
        let offer = self.lock().offers.iter().find(|o| o.id == id).cloned();
        let Some(Offer { name, path: Some(file_path), .. }) = offer else { return http::json(w, 404, &json!({ "status": "gone" })) };
        let Ok(mut file) = File::open(&file_path) else { return http::json(w, 404, &json!({ "status": "gone" })) };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        // a preview of a picture is not a save
        let inline = req.param("inline") == Some("1") && is_image(&name);
        let ascii: String = name.chars().map(|c| if c.is_ascii() && c != '"' && c != '\\' && !c.is_ascii_control() { c } else { '_' }).collect();
        let disposition = format!("{}; filename=\"{ascii}\"; filename*=UTF-8''{}", if inline { "inline" } else { "attachment" }, url_encode(&name));
        http::write_head(w, 200, mime(&name), len, &[("Content-Disposition", disposition)])?;
        if !inline {
            (self.notify)(HubEvent::Saved { offer: id, device: device.to_string(), done: false });
        }
        let mut buf = vec![0u8; CHUNK];
        let mut sent = 0u64;
        while sent < len {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n])?;
            sent += n as u64;
        }
        w.flush()?;
        if !inline && sent == len {
            (self.notify)(HubEvent::Saved { offer: id, device: device.to_string(), done: true });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inner() -> Inner {
        let file = std::env::temp_dir().join(format!("plume-drop-known-{}.json", crate::util::random_text(8)));
        Inner {
            token: "t".repeat(22),
            folder: std::env::temp_dir(),
            devices: HashMap::new(),
            known: Vec::new(),
            known_file: file,
            offers: Vec::new(),
            rev: 100,
            next: 1,
            fails: HashMap::new(),
            pc: "PC".into(),
        }
    }

    const PHONE: &str = "abcdefghijklmnop0123";

    #[test]
    fn first_time_asks_then_remembers() {
        let mut i = inner();
        let (s, ask) = i.touch(PHONE, Some("Pixel 8"), None, false);
        assert_eq!((s, ask), (DevState::Pending, true));
        // asking again does not ask twice
        let (s, ask) = i.touch(PHONE, None, None, true);
        assert_eq!((s, ask), (DevState::Pending, false));
        assert!(i.answer(PHONE, true));
        assert_eq!(i.touch(PHONE, None, None, false).0, DevState::Allowed);
        assert_eq!(i.known.len(), 1);
        assert_eq!(i.known[0].name, "Pixel 8");
        // after a restart (a new list of devices), the known phone comes straight in
        i.devices.clear();
        assert_eq!(i.touch(PHONE, None, None, false), (DevState::Allowed, false));
        let _ = std::fs::remove_file(&i.known_file);
    }

    #[test]
    fn only_hello_asks() {
        let mut i = inner();
        assert_eq!(i.seen(PHONE, None), None);
        assert!(i.devices.is_empty());
        i.known.push(Known { id: PHONE.into(), name: "Old phone".into(), added: 0 });
        assert_eq!(i.seen(PHONE, None), Some(DevState::Allowed));
        assert_eq!(i.devices[PHONE].name, "Old phone");
    }

    #[test]
    fn refused_can_ask_again_later() {
        let mut i = inner();
        i.touch(PHONE, Some("Phone"), None, false);
        assert!(i.answer(PHONE, false));
        assert_eq!(i.touch(PHONE, None, None, true), (DevState::Refused, false));
        i.devices.get_mut(PHONE).unwrap().asked -= Duration::from_secs(5);
        assert_eq!(i.touch(PHONE, None, None, false), (DevState::Refused, false));
        assert_eq!(i.touch(PHONE, None, None, true), (DevState::Pending, true));
        assert!(i.known.is_empty());
    }

    #[test]
    fn disconnect_forgets() {
        let mut i = inner();
        i.touch(PHONE, Some("Phone"), None, false);
        i.answer(PHONE, true);
        i.revoke(PHONE);
        assert!(i.known.is_empty());
        assert_eq!(i.touch(PHONE, None, None, false).0, DevState::Revoked);
        assert!(i.views().is_empty());
        let _ = std::fs::remove_file(&i.known_file);
    }

    #[test]
    fn changes_move_the_revision() {
        let mut i = inner();
        let r = i.rev;
        i.touch(PHONE, Some("A"), None, false);
        assert!(i.rev > r);
        let r = i.rev;
        i.touch(PHONE, None, None, false);
        assert_eq!(i.rev, r);
        i.touch(PHONE, Some("B"), None, false);
        assert!(i.rev > r);
    }

    #[test]
    fn wrong_codes_lock_an_address() {
        let mut i = inner();
        let ip: IpAddr = "192.168.1.9".parse().unwrap();
        for _ in 0..MAX_FAILS {
            assert!(!i.blocked(Some(ip)));
            i.failed(Some(ip));
        }
        assert!(i.blocked(Some(ip)));
        assert!(!i.blocked(Some("192.168.1.10".parse().unwrap())));
    }

    #[test]
    fn device_ids_and_names() {
        assert!(valid_device_id(PHONE));
        assert!(!valid_device_id("short"));
        assert!(!valid_device_id("abcdefghijklmnop/../x"));
        assert_eq!(clean_device_name("  \u{7}  "), "Device");
        assert_eq!(clean_device_name(&"x".repeat(80)).len(), 40);
    }
}
