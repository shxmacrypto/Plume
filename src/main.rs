// Plume — navigateur ultra-léger : Rust + WebView2 système, UI style iOS, onglets avec économiseur de mémoire,
// coffre à mots de passe chiffré, import de profils, extensions Chrome, bloqueur de pubs, mode lecture…
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adblock;
mod bicon;
mod devtools;
mod ext;
mod import;
mod media;
mod sqlite;
mod vault;

use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    platform::windows::{IconExtWindows, WindowBuilderExtWindows, WindowExtWindows},
    window::{Icon, ResizeDirection, Window, WindowBuilder},
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler, BytesReceivedChangedEventHandler,
    DownloadStartingEventHandler, FaviconChangedEventHandler, FocusChangedEventHandler, WindowCloseRequestedEventHandler, HistoryChangedEventHandler,
    Microsoft::Web::WebView2::Win32::*, NavigationCompletedEventHandler, ProfileAddBrowserExtensionCompletedHandler,
    ProfileGetBrowserExtensionsCompletedHandler, SourceChangedEventHandler, StateChangedEventHandler,
    TrySuspendCompletedHandler, WebResourceRequestedEventHandler,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        UI::{Controls::Dialogs::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::*,
};
use wry::{
    MemoryUsageLevel, NewWindowResponse, PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder,
    WebViewBuilderExtWindows, WebViewExtWindows,
    dpi::{PhysicalPosition, PhysicalSize},
};

type Res<T> = std::result::Result<T, String>;

/// Interface assemblée à la compilation (un seul document servi par plume://).
const UI_HTML: &str = concat!(
    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Plume UI</title><style>",
    include_str!("ui/style.css"),
    "</style></head><body>",
    include_str!("ui/body.html"),
    "<script>",
    include_str!("ui/i18n.js"),
    include_str!("ui/app.js"),
    include_str!("ui/settings.js"),
    include_str!("ui/devtools.js"),
    include_str!("ui/boot.js"),
    "</script></body></html>"
);
const PAGE_JS: &str = include_str!("page.js");

/// Version (une seule source : Cargo.toml) et dépôt GitHub (vérification des mises à jour).
const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "shxmacrypto/plume";

/// Les extensions peuvent demander le panneau latéral (chrome.sidePanel), absent de WebView2 : on le fournit.
const SIDE_PANEL_JS: &str = r#"(() => {
  if (location.protocol !== 'chrome-extension:' || !window.chrome) return;
  const ask = (o) => { try { window.ipc.postMessage(JSON.stringify({ c: 'side-open', page: (o && o.path) || '' })); } catch (e) {} return Promise.resolve(); };
  const api = { open: ask, setOptions: () => Promise.resolve(), getOptions: () => Promise.resolve({ enabled: true }),
    setPanelBehavior: () => Promise.resolve(), getPanelBehavior: () => Promise.resolve({ openPanelOnActionClick: false }) };
  try { if (chrome.sidePanel) Object.assign(chrome.sidePanel, { open: ask }); else Object.defineProperty(chrome, 'sidePanel', { value: api, configurable: true }); } catch (e) {}
})();"#;

fn search_url(engine: &str) -> &'static str {
    match engine {
        "bing" => "https://www.bing.com/search?q=",
        "ddg" => "https://duckduckgo.com/?q=",
        "brave" => "https://search.brave.com/search?q=",
        "ecosia" => "https://www.ecosia.org/search?q=",
        "qwant" => "https://www.qwant.com/?q=",
        _ => "https://www.google.com/search?q=",
    }
}

/// Réglages par défaut (mode « Complet »).
fn defaults() -> Value {
    json!({
        "onboarded": false, "name": "", "mode": "full", "lang": "auto", "search": "google",
        "theme": "dark", "accent": "#0a84ff", "tabs": "h", "bmbar": false, "restore": true,
        "uiScale": 1.0, "pageZoom": 1.0, "contrast": false, "motion": false, "bigTargets": false, "focusRings": true,
        "ab": true, "tracking": "balanced", "allow": [], "dark": [], "light": [], "darkAll": false, "never": [],
        "sleep": 10, "eco": false, "ext": true, "autolock": 15, "updates": true,
        "readerSize": 20, "readerTheme": "auto", "ttsRate": 1.0,
    })
}

/// Préréglages proposés à la première configuration (modifiables ensuite dans les réglages).
fn preset(mode: &str) -> Value {
    if mode == "ultra" {
        json!({"mode": "ultra", "eco": true, "ext": false, "tracking": "strict", "sleep": 3, "motion": true, "ab": true})
    } else {
        json!({"mode": "full", "eco": false, "ext": true, "tracking": "balanced", "sleep": 10, "motion": false, "ab": true})
    }
}

#[derive(Debug, Clone, Copy)]
enum Cmd {
    NewTab,
    Private,
    Close,
    Reopen,
    Next,
    Prev,
    Nth(usize),
    Last,
    Focus,
    Star,
    BmBar,
    Dev,
    History,
    Downloads,
    Reader,
    UiDev,
    NewWindow,
    PageFocus,
}

/// Origine d'un raccourci clavier : l'interface d'une fenêtre ou un onglet.
#[derive(Debug, Clone, Copy)]
enum Who {
    Win(u32),
    Tab(u32),
}

pub(crate) enum Ev {
    Ui(u32, String),
    Page(u32, String, String),
    Title(u32, String),
    Url(u32, String),
    Loading(u32, bool),
    Fav(u32, String),
    Nav(u32, bool, bool),
    Blocked(u32),
    Open(u32, String, bool),
    Cmd(Who, Cmd),
    Active(u32, bool),
    Tick,
    Media(Value),
    PopupSize(u32, f64, f64),
    Dl(Value),
    Imported(Value),
    ExtFetched(Res<(String, PathBuf)>),
    ExtList(Vec<Value>),
    ExtAdded(Res<(String, String, PathBuf)>),
    Dev(devtools::Msg),
}

thread_local! {
    /// Téléchargements en cours (objets COM, thread UI uniquement).
    static DLS: RefCell<HashMap<u32, ICoreWebView2DownloadOperation>> = RefCell::new(HashMap::new());
}
pub static DL_ID: AtomicU32 = AtomicU32::new(0);

struct Tab {
    id: u32,
    view: Option<WebView>,
    url: String,
    title: String,
    fav: String,
    loading: bool,
    back: bool,
    fwd: bool,
    last: Instant,
    private: bool,
    group: u32,
    blocked: u32,
    info: Option<Value>,
    speaking: bool,
    pinned: bool,
}

impl Tab {
    fn new(id: u32, url: String, title: String, fav: String) -> Self {
        Tab {
            id, view: None, url, title, fav, loading: false, back: false, fwd: false, last: Instant::now(),
            private: false, group: 0, blocked: 0, info: None, speaking: false, pinned: false,
        }
    }
}

/// État propre à une fenêtre (les autres fenêtres sont rangées ici, la fenêtre courante vit dans App).
struct Win {
    ui: WebView,
    tabs: Vec<Tab>,
    window: Window,
    wid: u32,
    active: usize,
    ui_h: f64,
    side_w: f64,
    overlay: bool,
    ui_ready: bool,
    was_max: bool,
    dev: devtools::DevTools,
    private_win: bool,
    popup: Option<WebView>,
    sidep: Option<WebView>,
    sidep_ext: Value,
    side_r: f64,
}

/// Demande d'ouverture de fenêtre (traitée dans la boucle, qui seule peut créer des fenêtres).
struct WinReq {
    url: Option<String>,
    private: bool,
    tab: Option<Tab>,
    restore: Vec<Value>,
    active: usize,
}

struct App {
    window: Window,
    wid: u32,
    next_wid: u32,
    private_win: bool,
    popup: Option<WebView>,
    sidep: Option<WebView>,
    sidep_ext: Value,
    side_r: f64,
    popup_anchor: f64,
    popup_ext: String,
    popup_at: Instant,
    ovr_full: bool,
    ovr_rects: Vec<[f64; 4]>,
    others: Vec<Win>,
    want_window: Vec<WinReq>,
    proxy: EventLoopProxy<Ev>,
    ui: WebView,
    env: ICoreWebView2Environment,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u32,
    closed: Vec<String>,
    bookmarks: Vec<Value>,
    history: Vec<Value>,
    groups: Vec<Value>,
    downloads: Vec<Value>,
    cfg: Value,
    vault: vault::Vault,
    pending_pw: Option<Value>,
    pw_hosts: Vec<String>,
    pw_used: Instant,
    exts: Vec<Value>,
    ext_meta: Value,
    ext_dir: PathBuf,
    ext_on: bool,
    eco: bool,
    ui_h: f64,
    side_w: f64,
    overlay: bool,
    file: Option<PathBuf>,
    quit: bool,
    ui_ready: bool,
    was_max: bool,
    dev: devtools::DevTools,
    _ctx: WebContext,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--restarted") {
        std::thread::sleep(Duration::from_millis(1500)); // laisse l'ancienne instance libérer le profil
    }
    let cli_url = args.iter().find(|a| !a.starts_with("--")).cloned();

    let dir = std::env::var("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Plume")).unwrap_or_else(|_| std::env::temp_dir().join("Plume"));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("plume.json");
    let saved: Value = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
        .unwrap_or(json!({}));
    let mut cfg = defaults();
    if let (Some(c), Some(s)) = (cfg.as_object_mut(), saved["cfg"].as_object()) {
        for (k, v) in s {
            c.insert(k.clone(), v.clone());
        }
    }
    if saved["cfg"].is_null() && saved["bmbar"].is_boolean() {
        cfg["bmbar"] = saved["bmbar"].clone(); // réglage d'une version précédente
    }
    let eco = args.iter().any(|a| a == "--eco") || cfg["eco"].as_bool().unwrap_or(false);
    let ext_on = cfg["ext"].as_bool().unwrap_or(true);
    adblock::ON.store(cfg["ab"].as_bool().unwrap_or(true), Ordering::Relaxed);
    *adblock::ALLOW.write().unwrap() = strs(&cfg["allow"]);

    let event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = make_window(&event_loop, None, cfg["theme"] != "light", &proxy, 1);

    let mut ctx = WebContext::new(Some(dir.clone()));

    // Flags Chromium : moins de RAM / CPU / réseau en arrière-plan
    let mut flags = String::from(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeTranslate,\
Translate,MediaRouter,OptimizationHints,SpareRendererForSitePerProcess,\
AutofillServerCommunication,InterestFeedContentSuggestions \
--renderer-process-limit=3 --process-per-site --disable-background-networking \
--disable-component-update --disable-sync --disable-breakpad --disable-domain-reliability \
--no-pings --disable-default-apps --allow-insecure-localhost",
    );
    if eco {
        flags.push_str(" --enable-low-end-device-mode --renderer-process-limit=2 --disable-gpu-compositing");
    }

    let base = WebViewBuilder::new_with_web_context(&mut ctx).with_additional_browser_args(&flags).with_browser_extensions_enabled(ext_on);
    let ui = build_ui(&window, base, &proxy, 1);
    let env = ui.environment();
    let list = |k: &str| saved[k].as_array().cloned().unwrap_or_default();
    let ext_dir = dir.join("extensions");
    let ext_meta = std::fs::read_to_string(ext_dir.join("meta.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}));

    let mut app = App {
        window,
        wid: 1,
        next_wid: 2,
        private_win: false,
        popup: None,
        sidep: None,
        sidep_ext: Value::Null,
        side_r: 0.0,
        popup_anchor: 0.0,
        popup_ext: String::new(),
        popup_at: Instant::now(),
        ovr_full: true,
        ovr_rects: vec![],
        others: vec![],
        want_window: vec![],
        proxy: proxy.clone(),
        ui,
        env,
        tabs: vec![],
        active: 0,
        next_id: 1,
        closed: vec![],
        bookmarks: list("bookmarks"),
        history: list("history"),
        groups: list("groups"),
        downloads: vec![],
        vault: vault::Vault::new(dir.join("vault.bin")),
        pending_pw: None,
        pw_hosts: strs(&saved["pwhosts"]),
        pw_used: Instant::now(),
        exts: vec![],
        ext_meta,
        ext_dir,
        ext_on,
        eco,
        ui_h: 84.0,
        side_w: 0.0,
        overlay: true, // l'écran de démarrage recouvre la fenêtre
        file: Some(file),
        quit: false,
        ui_ready: false,
        was_max: false,
        dev: devtools::DevTools::default(),
        cfg,
        _ctx: ctx,
    };
    app.apply_profile();
    let _ = app.ui.zoom(app.ui_scale());

    // Session précédente : onglets restaurés "endormis" (0 Mo) sauf l'actif
    if app.cfg["restore"].as_bool().unwrap_or(true) {
        for t in list("session") {
            let u = str_of(&t["u"]);
            if !u.is_empty() {
                let id = app.next();
                let mut tab = Tab::new(id, u, str_of(&t["t"]), str_of(&t["f"]));
                tab.group = t["g"].as_u64().unwrap_or(0) as u32;
                tab.pinned = t["pin"] == true;
                app.tabs.push(tab);
            }
        }
    }
    app.groups.retain(|g| app.tabs.iter().any(|t| t.group as u64 == g["id"].as_u64().unwrap_or(0)));
    let restored_active = saved["active"].as_u64().unwrap_or(0) as usize;
    match cli_url {
        Some(u) => {
            let u = app.to_url(&u);
            app.new_tab(Some(u), true, false)
        }
        None if !app.tabs.is_empty() => app.activate(restored_active.min(app.tabs.len() - 1)),
        None => app.new_tab(None, true, false),
    }
    // autres fenêtres de la session précédente
    if app.cfg["restore"].as_bool().unwrap_or(true) {
        for w in list("windows") {
            let s = w["session"].as_array().cloned().unwrap_or_default();
            if !s.is_empty() {
                app.want_window.push(WinReq { url: None, private: false, tab: None, restore: s, active: w["active"].as_u64().unwrap_or(0) as usize });
            }
        }
    }
    app.refresh_exts();

    // Petit fil qui réveille la boucle chaque minute (économiseur de mémoire, verrouillage auto du coffre)
    let tp = app.proxy.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            if tp.send_event(Ev::Tick).is_err() {
                break;
            }
        }
    });

    event_loop.run(move |event, target, flow| {
        *flow = ControlFlow::Wait; // ~0 % CPU au repos
        match event {
            Event::WindowEvent { window_id, event, .. } => {
                if app.focus_window_id(window_id) {
                    match event {
                        WindowEvent::CloseRequested => app.close_window(),
                        WindowEvent::Focused(true) => {
                            app.refocus();
                            app.push();
                        }
                        WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => app.relayout(),
                        _ => {}
                    }
                }
            }
            Event::UserEvent(ev) => app.on_event(ev),
            _ => {}
        }
        while let Some(req) = app.want_window.pop() {
            app.open_window(target, req);
        }
        if app.quit {
            app.vault.lock();
            app.save();
            *flow = ControlFlow::Exit;
        }
    });
}

impl App {
    /* ---------- Fenêtres : la fenêtre courante vit dans App, les autres dans others ---------- */

    fn swap_fields(&mut self, o: &mut Win) {
        use std::mem::swap;
        swap(&mut self.ui, &mut o.ui);
        swap(&mut self.tabs, &mut o.tabs);
        swap(&mut self.window, &mut o.window);
        swap(&mut self.wid, &mut o.wid);
        swap(&mut self.active, &mut o.active);
        swap(&mut self.ui_h, &mut o.ui_h);
        swap(&mut self.side_w, &mut o.side_w);
        swap(&mut self.overlay, &mut o.overlay);
        swap(&mut self.ui_ready, &mut o.ui_ready);
        swap(&mut self.was_max, &mut o.was_max);
        swap(&mut self.dev, &mut o.dev);
        swap(&mut self.private_win, &mut o.private_win);
        swap(&mut self.popup, &mut o.popup);
        swap(&mut self.sidep, &mut o.sidep);
        swap(&mut self.sidep_ext, &mut o.sidep_ext);
        swap(&mut self.side_r, &mut o.side_r);
    }

    fn bring(&mut self, i: usize) {
        let mut o = self.others.swap_remove(i);
        self.swap_fields(&mut o);
        self.others.push(o);
    }

    fn focus_win(&mut self, wid: u32) -> bool {
        if self.wid == wid {
            return true;
        }
        match self.others.iter().position(|w| w.wid == wid) {
            Some(i) => {
                self.bring(i);
                true
            }
            None => false,
        }
    }

    fn focus_tab(&mut self, id: u32) -> bool {
        if self.idx(id).is_some() {
            return true;
        }
        match self.others.iter().position(|w| w.tabs.iter().any(|t| t.id == id)) {
            Some(i) => {
                self.bring(i);
                true
            }
            None => false,
        }
    }

    fn focus_window_id(&mut self, id: tao::window::WindowId) -> bool {
        if self.window.id() == id {
            return true;
        }
        match self.others.iter().position(|w| w.window.id() == id) {
            Some(i) => {
                self.bring(i);
                true
            }
            None => false,
        }
    }

    /// Crée une fenêtre (vide, avec un onglet déplacé, ou restaurée) et en fait la fenêtre courante.
    fn open_window(&mut self, target: &EventLoopWindowTarget<Ev>, req: WinReq) {
        let pos = self.window.outer_position().ok().map(|p| tao::dpi::PhysicalPosition::new(p.x + 36, p.y + 36));
        let wid = self.next_wid;
        self.next_wid += 1;
        let window = make_window(target, pos, self.cfg["theme"] != "light", &self.proxy, wid);
        let base = WebViewBuilder::new().with_environment(self.env.clone());
        let ui = build_ui(&window, base, &self.proxy, wid);
        let mut w = Win {
            ui, tabs: vec![], window, wid, active: 0, ui_h: 84.0, side_w: 0.0, overlay: false,
            ui_ready: false, was_max: false, dev: devtools::DevTools::default(), private_win: req.private,
            popup: None, sidep: None, sidep_ext: Value::Null, side_r: 0.0,
        };
        self.swap_fields(&mut w);
        self.others.push(w);
        let _ = self.ui.zoom(self.ui_scale());
        if let Some(t) = req.tab {
            if let Some(v) = &t.view {
                let _ = v.reparent(self.window.hwnd() as isize);
            }
            self.tabs.push(t);
            self.active = usize::MAX;
            self.activate(0);
        } else if !req.restore.is_empty() {
            for t in &req.restore {
                let id = self.next();
                let mut tab = Tab::new(id, str_of(&t["u"]), str_of(&t["t"]), str_of(&t["f"]));
                tab.group = t["g"].as_u64().unwrap_or(0) as u32;
                tab.pinned = t["pin"] == true;
                self.tabs.push(tab);
            }
            self.activate(req.active.min(self.tabs.len() - 1));
        } else {
            let p = req.private;
            self.new_tab(req.url, true, p);
        }
        self.window.set_focus(); // la nouvelle fenêtre passe devant
    }

    /// Ferme la fenêtre courante (la dernière ferme Plume).
    fn close_window(&mut self) {
        if self.others.is_empty() {
            self.quit = true;
            return;
        }
        self.dev.close();
        for t in self.tabs.drain(..) {
            if !t.url.is_empty() && !t.private {
                self.closed.push(t.url);
            }
        }
        let mut o = self.others.pop().unwrap();
        self.swap_fields(&mut o);
        drop(o); // libère l'interface puis la fenêtre
        self.gc_groups();
        self.relayout();
        self.push();
        self.save();
    }

    fn next(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// N'exécute du JS dans l'UI qu'une fois sa page chargée.
    fn js(&self, code: &str) {
        if self.ui_ready {
            let _ = self.ui.evaluate_script(code);
        }
    }

    fn toast(&self, msg: &str) {
        self.js(&format!("toast({})", json!(msg)));
    }

    fn scale(&self) -> f64 {
        self.window.scale_factor()
    }

    fn ui_scale(&self) -> f64 {
        self.cfg["uiScale"].as_f64().unwrap_or(1.0).clamp(0.8, 2.0)
    }

    fn idx(&self, id: u32) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    fn blank(&self) -> bool {
        self.tabs.get(self.active).is_none_or(|t| t.url.is_empty())
    }

    fn to_url(&self, s: &str) -> String {
        to_url(s, search_url(self.cfg["search"].as_str().unwrap_or("google")))
    }

    /// Zone de la page : sous l'en-tête, à droite des onglets verticaux, au-dessus des DevTools.
    fn content_rect(&self) -> Rect {
        let s = self.window.inner_size();
        let k = self.ui_scale() * self.scale();
        let top = (self.ui_h * k) as u32;
        let left = (self.side_w * k) as u32;
        let bottom = (self.dev.height() * k) as u32;
        let right = (self.side_r * k) as u32;
        Rect {
            position: PhysicalPosition::new(left as i32, top as i32).into(),
            size: PhysicalSize::new(s.width.saturating_sub(left + right), s.height.saturating_sub(top + bottom)).into(),
        }
    }

    /// L'UI recouvre toute la fenêtre quand un panneau est ouvert, sur un onglet vide,
    /// avec les onglets verticaux ou les DevTools ; sinon seulement la bande du haut.
    fn relayout(&mut self) {
        if self.window.is_minimized() {
            if let Some(v) = self.active_view() {
                let _ = v.set_visible(false);
            }
            return;
        }
        let max = self.window.is_maximized();
        if max != self.was_max {
            self.was_max = max;
            self.push();
        }
        let hide = self.blank();
        let full = self.overlay || self.blank() || self.side_w > 0.0 || self.dev.height() > 0.0 || self.side_r > 0.0;
        let s = self.window.inner_size();
        let h = if full { s.height } else { (self.ui_h * self.ui_scale() * self.scale()) as u32 };
        let _ = self.ui.set_bounds(Rect { position: PhysicalPosition::new(0, 0).into(), size: PhysicalSize::new(s.width, h).into() });
        let r = self.content_rect();
        if let Some(v) = self.active_view() {
            let _ = v.set_bounds(r);
            let _ = v.set_visible(!hide);
        }
        // panneau latéral d'extension : à droite, sous son en-tête (caché quand un menu est ouvert)
        if let Some(p) = &self.sidep {
            let k = self.ui_scale() * self.scale();
            let (w, top) = ((self.side_r * k) as u32, ((self.ui_h + 38.0) * k) as u32);
            let _ = p.set_bounds(Rect {
                position: PhysicalPosition::new(s.width.saturating_sub(w) as i32, top as i32).into(),
                size: PhysicalSize::new(w, s.height.saturating_sub(top)).into(),
            });
            let _ = p.set_visible(!(self.overlay && self.ovr_full));
        }
        self.apply_ui_region();
    }

    /// Menus sans clignotement : l'interface passe au-dessus de la page mais n'est visible
    /// (et cliquable) que sur la barre du haut et la forme exacte du menu ouvert.
    fn apply_ui_region(&self) {
        unsafe {
            let mut host = HWND::default();
            if self.ui.controller().ParentWindow(&mut host).is_err() || host.is_invalid() {
                return;
            }
            let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
            if !self.overlay {
                let _ = SetWindowRgn(host, None, true);
                let _ = SetWindowPos(host, Some(HWND_BOTTOM), 0, 0, 0, 0, flags);
                return;
            }
            if self.ovr_full || self.blank() {
                let _ = SetWindowRgn(host, None, true);
            } else {
                let k = self.ui_scale() * self.scale();
                let s = self.window.inner_size();
                let px = |v: f64| (v * k).round() as i32;
                let rgn = CreateRectRgn(0, 0, s.width as i32, px(self.ui_h));
                let add = |r: HRGN| {
                    let _ = CombineRgn(Some(rgn), Some(rgn), Some(r), RGN_OR);
                    let _ = DeleteObject(r.into());
                };
                if self.side_w > 0.0 {
                    add(CreateRectRgn(0, 0, px(self.side_w), s.height as i32));
                }
                for r in &self.ovr_rects {
                    let (x, y, w, h) = (px(r[0]), px(r[1]), px(r[2]), px(r[3]));
                    add(CreateRoundRectRgn(x, y, x + w + 1, y + h + 1, px(28.0), px(28.0)));
                }
                let _ = SetWindowRgn(host, Some(rgn), true); // Windows garde la région
            }
            let _ = SetWindowPos(host, Some(HWND_TOP), 0, 0, 0, 0, flags);
        }
    }

    /// Page d'extension (fenêtre « popup » flottante ou panneau latéral).
    fn ext_view(&mut self, url: &str, rect: Rect, popup: bool) -> Option<WebView> {
        let (wid, p1, p2, p3, p4) = (self.wid, self.proxy.clone(), self.proxy.clone(), self.proxy.clone(), self.proxy.clone());
        let v = WebViewBuilder::new()
            .with_environment(self.env.clone())
            .with_url(url)
            .with_bounds(rect)
            .with_devtools(true)
            .with_new_window_req_handler(move |u, f| {
                if is_popup(&f) {
                    return NewWindowResponse::Allow; // vraie fenêtre (connexion, approbation…)
                }
                let _ = p1.send_event(Ev::Ui(wid, json!({"c": "open", "u": u}).to_string()));
                NewWindowResponse::Deny
            })
            .with_initialization_script(SIDE_PANEL_JS)
            .with_ipc_handler(move |r| {
                let body: Value = serde_json::from_str(r.body()).unwrap_or_default();
                if body["c"] == "side-open" {
                    let m = json!({"c": "side-open", "uri": r.uri().to_string(), "page": body["page"]});
                    let _ = p4.send_event(Ev::Ui(wid, m.to_string()));
                }
            })
            .with_on_page_load_handler(move |e, _| {
                if popup && matches!(e, PageLoadEvent::Finished) {
                    let _ = p2.send_event(Ev::Ui(wid, r#"{"c":"popup-measure"}"#.into()));
                }
            })
            .build_as_child(&self.window)
            .ok()?;
        clip_siblings(&v);
        unsafe {
            let mut tok = 0i64;
            let core = v.webview();
            // window.close() dans l'extension → on ferme
            let pc = p3.clone();
            let msg = if popup { r#"{"c":"popup-close"}"# } else { r#"{"c":"side-close"}"# };
            let _ = core.add_WindowCloseRequested(
                &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                    let _ = pc.send_event(Ev::Ui(wid, msg.into()));
                    Ok(())
                })),
                &mut tok,
            );
            if popup {
                // clic ailleurs → la popup se ferme (comme Chrome)
                let _ = v.controller().add_LostFocus(
                    &FocusChangedEventHandler::create(Box::new(move |_, _| {
                        let _ = p3.send_event(Ev::Ui(wid, r#"{"c":"popup-close","lost":true}"#.into()));
                        Ok(())
                    })),
                    &mut tok,
                );
            }
        }
        let _ = v.focus();
        Some(v)
    }

    fn open_ext_popup(&mut self, url: &str, anchor_right: f64) {
        self.popup = None;
        let k = self.ui_scale() * self.scale();
        let s = self.window.inner_size();
        let (w, h) = ((360.0 * k) as u32, (520.0 * k) as u32);
        let right = ((anchor_right + 4.0) * k).min(s.width as f64) as i32;
        let x = (right - w as i32).max(4);
        let y = ((self.ui_h - 4.0) * k) as i32;
        let rect = Rect { position: PhysicalPosition::new(x, y).into(), size: PhysicalSize::new(w, h).into() };
        self.popup = self.ext_view(url, rect, true);
        self.popup_anchor = anchor_right;
        self.popup_at = Instant::now();
    }

    fn open_side_panel(&mut self, url: &str, ext: Value) {
        self.popup = None;
        self.side_r = 380.0;
        let rect = Rect { position: PhysicalPosition::new(0, 0).into(), size: PhysicalSize::new(10u32, 10u32).into() };
        self.sidep = self.ext_view(url, rect, false);
        self.sidep_ext = ext;
        self.relayout();
        self.push();
    }

    fn close_side_panel(&mut self) {
        self.sidep = None;
        self.sidep_ext = Value::Null;
        self.side_r = 0.0;
        self.relayout();
        self.push();
    }

    fn cfg_script(&self) -> String {
        let c = &self.cfg;
        let o = json!({"ab": c["ab"], "allow": c["allow"], "dark": c["dark"], "light": c["light"], "darkAll": c["darkAll"]});
        format!("window.__plumeCfg={o};window.__plume&&window.__plume.apply();")
    }

    fn build_view(&mut self, i: usize) {
        let id = self.tabs[i].id;
        let private = self.tabs[i].private;
        let url = self.tabs[i].url.clone();
        let (p1, p2, p3, p4) = (self.proxy.clone(), self.proxy.clone(), self.proxy.clone(), self.proxy.clone());
        let built = WebViewBuilder::new()
            .with_environment(self.env.clone())
            .with_incognito(private)
            .with_url(&url)
            .with_bounds(self.content_rect())
            .with_visible(false)
            .with_devtools(true)
            .with_initialization_script_for_main_only(self.cfg_script(), true)
            .with_initialization_script_for_main_only(PAGE_JS, true)
            .with_document_title_changed_handler(move |t| {
                let _ = p1.send_event(Ev::Title(id, t));
            })
            .with_on_page_load_handler(move |e, _| {
                let _ = p2.send_event(Ev::Loading(id, matches!(e, PageLoadEvent::Started)));
            })
            // Comme Chrome : window.open avec une taille (connexion Google/GitHub/Netlify, paiement…)
            // → vraie fenêtre popup liée à la page (window.opener conservé) ; un simple lien → onglet.
            .with_new_window_req_handler(move |u, f| {
                if is_popup(&f) {
                    return NewWindowResponse::Allow;
                }
                let _ = p3.send_event(Ev::Open(id, u, private));
                NewWindowResponse::Deny
            })
            .with_ipc_handler(move |r| {
                let _ = p4.send_event(Ev::Page(id, r.uri().to_string(), r.body().clone()));
            })
            .build_as_child(&self.window);
        if let Ok(v) = built {
            clip_siblings(&v);
            unsafe {
                let _ = hook_tab(&v, id, &self.proxy);
            }
            let z = self.cfg["pageZoom"].as_f64().unwrap_or(1.0);
            if z != 1.0 {
                let _ = v.zoom(z);
            }
            self.tabs[i].view = Some(v);
        }
    }

    /// Onglet en arrière-plan : caché + suspendu (scripts et rendu arrêtés) + mémoire minimale.
    fn sleep(&mut self, i: usize) {
        let t = &mut self.tabs[i];
        t.last = Instant::now();
        if let Some(v) = &t.view {
            let _ = v.set_visible(false);
            let _ = v.set_memory_usage_level(MemoryUsageLevel::Low);
            unsafe {
                if let Ok(c3) = v.webview().cast::<ICoreWebView2_3>() {
                    let _ = c3.TrySuspend(&TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(()))));
                }
            }
        }
    }

    fn activate(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.popup = None;
        if i != self.active && self.active < self.tabs.len() {
            self.sleep(self.active);
        }
        self.active = i;
        self.overlay = false;
        if self.tabs[i].view.is_none() && !self.tabs[i].url.is_empty() {
            self.build_view(i);
        }
        let t = &mut self.tabs[i];
        t.last = Instant::now();
        if let Some(v) = &t.view {
            let _ = v.set_memory_usage_level(MemoryUsageLevel::Normal);
        }
        self.relayout();
        if let Some(v) = &self.tabs[i].view {
            let _ = v.focus();
        } else if self.ui_ready {
            let _ = self.ui.focus();
            self.js("focusBar()");
        }
        let title = self.tabs[i].title.clone();
        self.window.set_title(if title.is_empty() { "Plume" } else { &title });
        self.dev_retarget();
        self.push();
        self.push_lists();
    }

    fn new_tab(&mut self, url: Option<String>, fg: bool, private: bool) {
        let id = self.next();
        // jamais au milieu des onglets épinglés
        let pins = self.tabs.iter().filter(|t| t.pinned).count();
        let pos = if self.tabs.is_empty() { 0 } else { (self.active + 1).max(pins) };
        let mut t = Tab::new(id, url.clone().unwrap_or_default(), String::new(), String::new());
        t.private = private;
        // un onglet ouvert depuis un groupe reste dans le groupe
        t.group = self.tabs.get(self.active).map_or(0, |a| if url.is_some() { a.group } else { 0 });
        self.tabs.insert(pos, t);
        if !fg && url.is_some() {
            self.build_view(pos);
            self.sleep(pos);
        }
        if fg || self.tabs.len() == 1 {
            self.activate(pos);
        } else {
            self.push();
        }
    }

    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        let t = self.tabs.remove(i);
        if !t.url.is_empty() && !t.private {
            self.closed.push(t.url.clone());
            if self.closed.len() > 25 {
                self.closed.remove(0);
            }
        }
        if self.dev.target == t.id {
            self.dev_close();
        }
        drop(t.view); // libère immédiatement le processus de rendu
        self.gc_groups();
        if self.tabs.is_empty() {
            if !self.others.is_empty() {
                self.close_window();
            } else {
                let p = self.private_win;
                self.new_tab(None, true, p);
            }
            return;
        }
        if i < self.active || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        let a = self.active;
        self.active = usize::MAX;
        self.activate(a);
    }

    fn go(&mut self, text: &str) {
        let url = self.to_url(text);
        if url.is_empty() {
            return;
        }
        let i = self.active;
        self.overlay = false;
        self.tabs[i].url = url.clone();
        match &self.tabs[i].view {
            Some(v) => {
                let _ = v.load_url(&url);
            }
            None => self.build_view(i),
        }
        self.relayout();
        if let Some(v) = &self.tabs[i].view {
            let _ = v.set_memory_usage_level(MemoryUsageLevel::Normal);
            let _ = v.focus();
        }
        self.dev_retarget();
        self.push();
    }

    /// Économiseur de mémoire : décharge les onglets inactifs depuis longtemps (sauf s'ils jouent du son).
    fn discard_idle(&mut self) {
        let mins = self.cfg["sleep"].as_u64().unwrap_or(10);
        let mut changed = false;
        if mins > 0 {
            let limit = Duration::from_secs(mins * 60);
            let dev = self.dev.target;
            for (i, t) in self.tabs.iter_mut().enumerate() {
                if i == self.active || t.view.is_none() || t.last.elapsed() < limit || t.private || t.id == dev {
                    continue;
                }
                let playing = t.view.as_ref().is_some_and(|v| unsafe {
                    let mut b = BOOL(0);
                    v.webview().cast::<ICoreWebView2_8>().and_then(|c| c.IsDocumentPlayingAudio(&mut b)).is_ok() && b.as_bool()
                });
                if !playing {
                    t.view = None;
                    t.loading = false;
                    changed = true;
                }
            }
        }
        // verrouillage automatique du coffre
        let lock = self.cfg["autolock"].as_u64().unwrap_or(15);
        if lock > 0 && self.vault.unlocked() && self.pw_used.elapsed() > Duration::from_secs(lock * 60) {
            self.vault.lock();
            changed = true;
        }
        if changed {
            self.push();
        }
        self.save();
    }

    fn refocus(&self) {
        match self.active_view() {
            Some(v) if !self.overlay => {
                let _ = v.focus();
            }
            _ => {
                let _ = self.ui.focus();
                // jamais par-dessus un panneau ouvert (réglages, historique…)
                if self.blank() && !self.overlay {
                    self.js("focusBar()");
                }
            }
        }
    }

    fn active_view(&self) -> Option<&WebView> {
        self.tabs.get(self.active).and_then(|t| t.view.as_ref())
    }

    fn core(&self) -> Option<ICoreWebView2> {
        self.active_view().map(|v| v.webview())
    }

    fn eval_tab(&self, i: usize, js: &str) {
        if let Some(v) = self.tabs.get(i).and_then(|t| t.view.as_ref()) {
            let _ = v.evaluate_script(js);
        }
    }

    fn zoom(&self) -> f64 {
        self.active_view()
            .map(|v| unsafe {
                let mut z = 1.0;
                let _ = v.controller().ZoomFactor(&mut z);
                z
            })
            .unwrap_or(1.0)
    }

    fn starred(&self) -> bool {
        let u = &self.tabs[self.active].url;
        !u.is_empty() && self.bookmarks.iter().any(|b| b["u"] == *u)
    }

    fn push(&self) {
        let tabs: Vec<Value> = self
            .tabs
            .iter()
            .map(|t| {
                json!({"id": t.id, "t": t.title, "u": t.url, "f": t.fav, "l": t.loading, "p": t.private, "g": t.group, "pin": t.pinned,
                       "s": t.view.is_none() && !t.url.is_empty()})
            })
            .collect();
        let t = &self.tabs[self.active];
        // titre de la fenêtre (barre des tâches, Alt+Tab) = onglet actif, toujours à jour
        self.window.set_title(if !t.title.is_empty() { &t.title } else if t.private { "Plume (InPrivate)" } else { "Plume" });
        let host = import::host_of(&t.url);
        let exts: Vec<&Value> = self.exts.iter().filter(|e| e["on"] == true).collect();
        let state = json!({
            "tabs": tabs, "a": t.id, "back": t.back, "fwd": t.fwd, "star": self.starred(), "groups": self.groups,
            "bm": self.bookmarks, "eco": self.eco, "max": self.window.is_maximized(),
            "z": self.zoom(), "closed": !self.closed.is_empty(), "cfg": self.cfg,
            "asleep": self.tabs.iter().filter(|t| t.view.is_none() && !t.url.is_empty()).count(),
            "info": t.info, "priv": t.private, "blocked": t.blocked, "speaking": t.speaking, "host": host,
            "abSite": !adblock::allowed(&host), "darkSite": self.dark_on(&host),
            "vault": {"exists": self.vault.exists(), "unlocked": self.vault.unlocked(), "n": self.vault.items.len()},
            "dl": self.downloads.iter().filter(|d| d["state"] == "run").count(), "dlAny": !self.downloads.is_empty(),
            "exts": exts, "extOn": self.ext_on, "restart": self.needs_restart(), "adlist": adblock::count(),
            "dev": self.dev.height() > 0.0, "lang": self.lang(), "version": VERSION, "repo": REPO, "sidep": self.sidep_ext, "sideR": self.side_r, "popup": self.popup.is_some(),
        });
        self.js(&format!("U({state})"));
    }

    fn push_lists(&self) {
        let h: Vec<&Value> = self.history.iter().take(400).collect();
        self.js(&format!("L({})", json!({ "hist": h })));
    }

    fn push_downloads(&self) {
        self.js(&format!("D({})", Value::Array(self.downloads.clone())));
    }

    fn push_exts(&self) {
        let rec: Vec<Value> = ext::RECOMMENDED.iter().map(|(id, n, k)| json!({"store": id, "name": n, "k": k})).collect();
        self.js(&format!("X({})", json!({"list": self.exts, "rec": rec, "on": self.ext_on})));
    }

    fn push_vault(&self) {
        let list: Vec<Value> = if self.vault.unlocked() {
            self.vault.items.iter().enumerate().map(|(i, it)| json!({"i": i, "o": it["o"], "u": it["u"], "t": it["t"]})).collect()
        } else {
            vec![]
        };
        self.js(&format!("P({})", Value::Array(list)));
    }

    fn needs_restart(&self) -> bool {
        self.cfg["ext"].as_bool() != Some(self.ext_on) || (self.cfg["eco"].as_bool() != Some(self.eco) && !std::env::args().any(|a| a == "--eco"))
    }

    fn dark_on(&self, host: &str) -> bool {
        if self.cfg["darkAll"] == true {
            !strs(&self.cfg["light"]).iter().any(|h| h == host)
        } else {
            strs(&self.cfg["dark"]).iter().any(|h| h == host)
        }
    }

    fn save(&self) {
        let Some(f) = &self.file else { return };
        let win = |tabs: &[Tab], a: usize| {
            let keep = |t: &&Tab| !t.url.is_empty() && !t.private;
            let s: Vec<Value> = tabs.iter().filter(keep).map(|t| json!({"u": t.url, "t": t.title, "f": t.fav, "g": t.group, "pin": t.pinned})).collect();
            json!({"session": s, "active": tabs[..a.min(tabs.len())].iter().filter(keep).count()})
        };
        let mut wins: Vec<Value> = std::iter::once(win(&self.tabs, self.active))
            .chain(self.others.iter().map(|w| win(&w.tabs, w.active)))
            .filter(|w| w["session"].as_array().is_some_and(|s| !s.is_empty()))
            .collect();
        let first = if wins.is_empty() { json!({"session": [], "active": 0}) } else { wins.remove(0) };
        let data = json!({"bookmarks": self.bookmarks, "history": self.history, "session": first["session"], "windows": wins,
                          "groups": self.groups, "active": first["active"], "cfg": self.cfg, "pwhosts": self.pw_hosts});
        let tmp = f.with_extension("tmp");
        if std::fs::write(&tmp, data.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, f);
        }
    }

    fn record_history(&mut self, i: usize) {
        let t = &self.tabs[i];
        if !t.url.starts_with("http") || t.private {
            return;
        }
        let (u, title, fav) = (t.url.clone(), t.title.clone(), t.fav.clone());
        let count = match self.history.iter().position(|h| h["u"] == u) {
            Some(p) => self.history.remove(p)["c"].as_u64().unwrap_or(1) + 1,
            None => 1,
        };
        self.history.insert(0, json!({"u": u, "t": title, "f": fav, "n": now_ms(), "c": count}));
        self.history.truncate(3000);
    }

    fn toggle_star(&mut self) {
        let t = &self.tabs[self.active];
        if t.url.is_empty() {
            return;
        }
        if let Some(p) = self.bookmarks.iter().position(|b| b["u"] == t.url) {
            self.bookmarks.remove(p);
        } else {
            let title = if t.title.is_empty() { t.url.clone() } else { t.title.clone() };
            self.bookmarks.push(json!({"u": t.url, "t": title, "f": t.fav}));
        }
        self.save();
        self.push();
    }

    /// Panneau / menu ouvert par-dessus la page : on prend d'abord une photo de la page
    /// pour l'afficher figée dessous (la page ne « disparaît » plus), puis on la masque.
    fn set_overlay(&mut self, on: bool, focus_page: bool) {
        if on {
            self.popup = None;
        }
        if on != self.overlay {
            self.overlay = on;
            if on {
                self.push_lists();
            } else {
                self.ovr_full = false;
                self.ovr_rects.clear();
            }
            self.relayout();
        }
        if !on && focus_page {
            if let Some(v) = self.active_view() {
                let _ = v.focus();
            }
        }
    }

    fn cmd(&mut self, c: Cmd) {
        let n = self.tabs.len();
        match c {
            Cmd::NewTab => {
                let p = self.private_win;
                self.new_tab(None, true, p)
            }
            Cmd::NewWindow => self.want_window.push(WinReq { url: None, private: false, tab: None, restore: vec![], active: 0 }),
            Cmd::Private => self.want_window.push(WinReq { url: None, private: true, tab: None, restore: vec![], active: 0 }),
            Cmd::Close => self.close_tab(self.active),
            Cmd::Reopen => {
                if let Some(u) = self.closed.pop() {
                    self.new_tab(Some(u), true, false)
                }
            }
            Cmd::Next => self.activate((self.active + 1) % n),
            Cmd::Prev => self.activate((self.active + n - 1) % n),
            Cmd::Nth(k) => self.activate(k.min(n - 1)),
            Cmd::Last => self.activate(n - 1),
            Cmd::Focus => {
                let _ = self.ui.focus();
                self.js("focusBar(true)");
            }
            Cmd::Star => self.toggle_star(),
            Cmd::BmBar => {
                let on = self.cfg["bmbar"] != true;
                self.cfg["bmbar"] = json!(on);
                self.push();
            }
            Cmd::Dev => self.dev_toggle(),
            Cmd::UiDev => self.ui.open_devtools(),
            Cmd::PageFocus => {
                if self.overlay {
                    self.js("pageClicked()");
                }
            }
            Cmd::History => {
                let _ = self.ui.focus();
                self.js("openPanel('hist')");
            }
            Cmd::Downloads => {
                let _ = self.ui.focus();
                self.js("openPanel('dl')");
            }
            Cmd::Reader => self.reader(),
        }
    }

    fn reader(&self) {
        let c = &self.cfg;
        let o = json!({"size": c["readerSize"], "theme": c["readerTheme"], "lang": self.lang(), "rate": c["ttsRate"]});
        self.eval_tab(self.active, &format!("window.__plume&&__plume.reader({o})"));
    }

    fn lang(&self) -> &str {
        match self.cfg["lang"].as_str() {
            Some("fr") => "fr",
            Some("en") => "en",
            _ => sys_lang(),
        }
    }

    /* ---------- Réglages ---------- */

    fn set_cfg(&mut self, k: &str, v: Value) {
        self.cfg[k] = v;
        match k {
            "ab" => adblock::ON.store(self.cfg["ab"].as_bool().unwrap_or(true), Ordering::Relaxed),
            "allow" => *adblock::ALLOW.write().unwrap() = strs(&self.cfg["allow"]),
            "tracking" | "theme" => {
                self.apply_profile();
                unsafe {
                    let dark: i32 = (self.cfg["theme"] != "light") as i32;
                    let _ = DwmSetWindowAttribute(HWND(self.window.hwnd() as _), DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
                }
            }
            "uiScale" => {
                let _ = self.ui.zoom(self.ui_scale());
                self.relayout();
            }
            "pageZoom" => {
                let z = self.cfg["pageZoom"].as_f64().unwrap_or(1.0);
                for t in &self.tabs {
                    if let Some(v) = &t.view {
                        let _ = v.zoom(z);
                    }
                }
            }
            _ => {}
        }
        if matches!(k, "ab" | "allow" | "dark" | "light" | "darkAll") {
            self.push_page_cfg();
        }
        self.save();
        self.push();
    }

    /// Envoie la configuration (pubs, mode sombre) aux pages ouvertes et aux suivantes.
    fn push_page_cfg(&self) {
        let js = self.cfg_script();
        let h = HSTRING::from(js.as_str());
        for t in &self.tabs {
            if let Some(v) = &t.view {
                unsafe {
                    let _ = v.webview().AddScriptToExecuteOnDocumentCreated(
                        &h,
                        &AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(|_, _| Ok(()))),
                    );
                }
                let _ = v.evaluate_script(&js);
            }
        }
    }

    fn apply_profile(&self) {
        unsafe {
            let Ok(p) = self.ui.webview().cast::<ICoreWebView2_13>().and_then(|c| c.Profile()) else { return };
            let scheme = match self.cfg["theme"].as_str() {
                Some("light") => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT,
                Some("system") => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO,
                _ => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK,
            };
            let _ = p.SetPreferredColorScheme(scheme);
            if let Ok(p3) = p.cast::<ICoreWebView2Profile3>() {
                let lvl = match self.cfg["tracking"].as_str() {
                    Some("off") => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE,
                    Some("basic") => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BASIC,
                    Some("strict") => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT,
                    _ => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BALANCED,
                };
                let _ = p3.SetPreferredTrackingPreventionLevel(lvl);
            }
        }
    }

    fn toggle_list(&mut self, key: &str, item: &str) {
        let mut l = strs(&self.cfg[key]);
        if let Some(p) = l.iter().position(|h| h == item) {
            l.remove(p);
        } else {
            l.push(item.to_string());
        }
        self.set_cfg(key, json!(l));
    }

    fn restart(&mut self) {
        self.save();
        if let Ok(exe) = std::env::current_exe() {
            let mut a: Vec<String> = std::env::args().skip(1).filter(|a| a != "--restarted" && a != "--eco").collect();
            a.push("--restarted".into());
            if std::process::Command::new(exe).args(a).spawn().is_ok() {
                self.quit = true;
            }
        }
    }

    /* ---------- Groupes d'onglets ---------- */

    /// Les onglets épinglés restent toujours en tête (ordre conservé).
    fn order_pins(&mut self) {
        let aid = self.tabs.get(self.active).map(|t| t.id);
        self.tabs.sort_by_key(|t| !t.pinned);
        if let Some(a) = aid.and_then(|a| self.idx(a)) {
            self.active = a;
        }
    }

    fn gc_groups(&mut self) {
        let (tabs, others) = (&self.tabs, &self.others);
        self.groups.retain(|g| {
            let id = g["id"].as_u64().unwrap_or(0);
            tabs.iter().chain(others.iter().flat_map(|w| w.tabs.iter())).any(|t| t.group as u64 == id)
        });
    }

    /// Range l'onglet juste après le dernier onglet de son groupe (les groupes restent contigus).
    fn regroup(&mut self, i: usize) {
        let g = self.tabs[i].group;
        let aid = self.tabs[self.active].id;
        let t = self.tabs.remove(i);
        let pos = if g == 0 { i.min(self.tabs.len()) } else { self.tabs.iter().rposition(|x| x.group == g).map_or(i.min(self.tabs.len()), |p| p + 1) };
        self.tabs.insert(pos, t);
        self.active = self.idx(aid).unwrap_or(0);
    }

    /* ---------- Mots de passe ---------- */

    fn fill(&mut self, i: usize, item: &Value) {
        self.pw_used = Instant::now();
        self.eval_tab(i, &format!("window.__plume&&__plume.fill({},{})", item["u"], item["p"]));
    }

    /// Une page demande des identifiants (champ de connexion sélectionné).
    fn pw_request(&mut self, i: usize, host: &str) {
        if !self.vault.exists() {
            return;
        }
        if self.vault.unlocked() {
            let m: Vec<Value> = self.vault.matches(host).into_iter().cloned().collect();
            match m.len() {
                0 => {}
                1 => self.fill(i, &m[0]),
                _ => self.tabs[i].info = Some(json!({"k": "choose", "host": host, "list": m.iter().map(|x| x["u"].clone()).collect::<Vec<_>>()})),
            }
        } else if self.pw_hosts.iter().any(|h| h == host || host.ends_with(&format!(".{h}"))) {
            self.tabs[i].info = Some(json!({"k": "unlock", "host": host}));
        }
    }

    fn after_unlock(&mut self) {
        self.pw_used = Instant::now();
        if let Some(p) = self.pending_pw.take() {
            if self.vault.upsert(p) {
                let _ = self.vault.save();
                self.toast("pw-saved");
            }
        }
        self.sync_pw_hosts();
        let i = self.active;
        let host = import::host_of(&self.tabs[i].url);
        if matches!(self.tabs[i].info.as_ref().and_then(|x| x["k"].as_str()), Some("unlock" | "save")) {
            self.tabs[i].info = None;
            self.pw_request(i, &host);
        }
    }

    fn sync_pw_hosts(&mut self) {
        if self.vault.unlocked() {
            let mut h: Vec<String> = self.vault.items.iter().filter_map(|x| x["o"].as_str().map(String::from)).collect();
            h.sort();
            h.dedup();
            self.pw_hosts = h;
            self.save();
        }
    }

    fn import_csv(&mut self) {
        if !self.vault.unlocked() {
            return;
        }
        let Some(path) = open_file(HWND(self.window.hwnd() as _)) else { return };
        let items = std::fs::read_to_string(&path).map(|t| import::parse_csv(&t)).unwrap_or_default();
        let mut n = 0;
        for it in items {
            n += self.vault.upsert(it) as usize;
        }
        let _ = self.vault.save();
        self.sync_pw_hosts();
        self.js(&format!("pwImported({n})"));
        self.push_vault();
        self.push();
    }

    /* ---------- Extensions ---------- */

    fn profile7(&self) -> Option<ICoreWebView2Profile7> {
        unsafe { self.ui.webview().cast::<ICoreWebView2_13>().ok()?.Profile().ok()?.cast().ok() }
    }

    fn refresh_exts(&self) {
        if !self.ext_on {
            return;
        }
        let Some(p7) = self.profile7() else { return };
        let p = self.proxy.clone();
        unsafe {
            let _ = p7.GetBrowserExtensions(&ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |_, list| {
                let mut out = vec![];
                if let Some(l) = list {
                    let mut n = 0u32;
                    l.Count(&mut n)?;
                    for i in 0..n {
                        let e = l.GetValueAtIndex(i)?;
                        let (mut id, mut name, mut on) = (PWSTR::null(), PWSTR::null(), BOOL(0));
                        e.Id(&mut id)?;
                        e.Name(&mut name)?;
                        e.IsEnabled(&mut on)?;
                        out.push(json!({"id": webview2_com::take_pwstr(id), "name": webview2_com::take_pwstr(name), "on": on.as_bool()}));
                    }
                }
                let _ = p.send_event(Ev::ExtList(out));
                Ok(())
            })));
        }
    }

    /// Applique une action à une extension installée (activer / désactiver / supprimer).
    fn with_ext(&self, id: String, action: &'static str, on: bool) {
        let wid = self.wid;
        let Some(p7) = self.profile7() else { return };
        let p = self.proxy.clone();
        unsafe {
            let _ = p7.GetBrowserExtensions(&ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |_, list| {
                let Some(l) = list else { return Ok(()) };
                let mut n = 0u32;
                l.Count(&mut n)?;
                for i in 0..n {
                    let e = l.GetValueAtIndex(i)?;
                    let mut eid = PWSTR::null();
                    e.Id(&mut eid)?;
                    if webview2_com::take_pwstr(eid) != id {
                        continue;
                    }
                    let p2 = p.clone();
                    let done = Box::new(move |_| {
                        let _ = p2.send_event(Ev::Ui(wid, r#"{"c":"ext-list"}"#.into()));
                        Ok(())
                    });
                    if action == "remove" {
                        e.Remove(&BrowserExtensionRemoveCompletedHandler::create(done))?;
                    } else {
                        e.Enable(on, &BrowserExtensionEnableCompletedHandler::create(done))?;
                    }
                }
                Ok(())
            })));
        }
    }

    fn install_ext(&mut self, link: &str) {
        if !self.ext_on {
            self.toast("ext-off");
            return;
        }
        let Some(id) = ext::store_id(link) else {
            self.toast("ext-bad");
            return;
        };
        self.toast("ext-dl");
        let (p, root) = (self.proxy.clone(), self.ext_dir.clone());
        std::thread::spawn(move || {
            let r = ext::fetch(&id, &root).map(|d| (id, d));
            let _ = p.send_event(Ev::ExtFetched(r));
        });
    }

    fn save_ext_meta(&self) {
        let _ = std::fs::create_dir_all(&self.ext_dir);
        let _ = std::fs::write(self.ext_dir.join("meta.json"), self.ext_meta.to_string());
    }

    /* ---------- DevTools intégrés ---------- */

    fn dev_toggle(&mut self) {
        if self.dev.height() > 0.0 {
            self.dev_close();
        } else if let Some(t) = self.tabs.get(self.active).filter(|t| t.view.is_some()) {
            let id = t.id;
            let core = t.view.as_ref().unwrap().webview();
            self.dev.open(id, &core, &self.proxy);
            self.js("devOpen(true)");
            self.relayout();
            self.push();
        }
    }

    fn dev_close(&mut self) {
        self.dev.close();
        self.js("devOpen(false)");
        self.relayout();
        self.push();
    }

    /// Les DevTools suivent l'onglet actif.
    fn dev_retarget(&mut self) {
        if self.dev.height() <= 0.0 {
            return;
        }
        let id = self.tabs[self.active].id;
        if self.dev.target == id {
            return;
        }
        match self.tabs[self.active].view.as_ref().map(|v| v.webview()) {
            Some(core) => {
                self.dev.open(id, &core, &self.proxy);
                self.js("devReset()");
            }
            None => self.dev_close(),
        }
    }

    fn dev_core(&self) -> Option<ICoreWebView2> {
        self.idx(self.dev.target).and_then(|i| self.tabs[i].view.as_ref()).map(|v| v.webview())
    }

    /* ---------- Événements ---------- */

    fn on_event(&mut self, ev: Ev) {
        // chaque événement est traité dans la fenêtre qu'il concerne
        let ok = match &ev {
            Ev::Ui(w, _) | Ev::Active(w, _) | Ev::Cmd(Who::Win(w), _) | Ev::PopupSize(w, ..) => self.focus_win(*w),
            Ev::Page(id, ..) | Ev::Title(id, _) | Ev::Url(id, _) | Ev::Loading(id, _) | Ev::Fav(id, _) | Ev::Nav(id, ..)
            | Ev::Blocked(id) | Ev::Open(id, ..) | Ev::Cmd(Who::Tab(id), _) => self.focus_tab(*id),
            Ev::Dev(m) => self.focus_tab(m.tab()),
            _ => true,
        };
        if !ok {
            return;
        }
        match ev {
            Ev::Ui(_, msg) => self.on_ui(&msg),
            Ev::Media(v) => self.js(&format!("MEDIA({v})")),
            Ev::PopupSize(_, w, h) => {
                if let Some(p) = &self.popup {
                    let k = self.ui_scale() * self.scale();
                    let s = self.window.inner_size();
                    let (w, h) = (w.clamp(200.0, 800.0) * k, h.clamp(120.0, 600.0) * k);
                    let right = ((self.popup_anchor + 4.0) * k).min(s.width as f64);
                    let x = (right - w).max(4.0) as i32;
                    let y = ((self.ui_h - 4.0) * k) as i32;
                    let _ = p.set_bounds(Rect { position: PhysicalPosition::new(x, y).into(), size: PhysicalSize::new(w as u32, h as u32).into() });
                }
            }
            Ev::Page(id, uri, body) => self.on_page(id, &uri, &body),
            Ev::Cmd(_, c) => self.cmd(c),
            Ev::Tick => self.discard_idle(),
            Ev::Open(_, u, private) => self.new_tab(Some(u), true, private),
            Ev::Dev(m) => {
                if let Some(js) = self.dev.on_msg(m) {
                    self.js(&js);
                }
            }
            Ev::Active(_, on) => {
                if let Some(v) = self.active_view() {
                    let _ = v.set_memory_usage_level(if on { MemoryUsageLevel::Normal } else { MemoryUsageLevel::Low });
                }
            }
            Ev::Blocked(id) => {
                if let Some(i) = self.idx(id) {
                    self.tabs[i].blocked += 1;
                    if i == self.active {
                        self.js(&format!("blk({})", self.tabs[i].blocked));
                    }
                }
            }
            Ev::Title(id, t) => {
                if let Some(i) = self.idx(id) {
                    if let Some(h) = self.history.first_mut() {
                        if h["u"] == self.tabs[i].url {
                            h["t"] = json!(t);
                        }
                    }
                    self.tabs[i].title = t;
                    if i == self.active {
                        let title = &self.tabs[i].title;
                        self.window.set_title(if title.is_empty() { "Plume" } else { title });
                    }
                    self.push();
                }
            }
            Ev::Url(id, u) => {
                // page « nouvel onglet » interne de Chromium → onglet vide de Plume
                let u = if u.starts_with("chrome-search://") || u.starts_with("edge://newtab") { String::new() } else { u };
                if let Some(i) = self.idx(id) {
                    if u != "about:blank" || self.tabs[i].url.is_empty() {
                        if host_of(&u) != host_of(&self.tabs[i].url) {
                            self.tabs[i].fav.clear();
                            self.tabs[i].info = None;
                        }
                        // Chrome Web Store : proposer d'ajouter l'extension à Plume
                        if (u.contains("chromewebstore.google.com/detail/") || u.contains("chrome.google.com/webstore/detail/"))
                            && let Some(sid) = ext::store_id(&u)
                        {
                            self.tabs[i].info = Some(json!({"k": "store", "id": sid}));
                        }
                        self.tabs[i].url = u;
                    }
                    self.push();
                }
            }
            Ev::Loading(id, l) => {
                if let Some(i) = self.idx(id) {
                    self.tabs[i].loading = l;
                    if l {
                        self.tabs[i].blocked = 0;
                        self.tabs[i].speaking = false;
                    } else {
                        self.record_history(i);
                    }
                    self.push();
                }
            }
            Ev::Fav(id, f) => {
                if let Some(i) = self.idx(id) {
                    let u = self.tabs[i].url.clone();
                    for h in self.history.iter_mut().chain(self.bookmarks.iter_mut()).filter(|h| h["u"] == u.as_str()) {
                        h["f"] = json!(f);
                    }
                    self.tabs[i].fav = f;
                    self.push();
                }
            }
            Ev::Nav(id, b, f) => {
                if let Some(i) = self.idx(id) {
                    self.tabs[i].back = b;
                    self.tabs[i].fwd = f;
                    if i == self.active {
                        self.push();
                    }
                }
            }
            Ev::Dl(v) => {
                let id = v["id"].as_u64();
                match self.downloads.iter_mut().find(|d| d["id"].as_u64() == id) {
                    Some(d) => {
                        for (k, x) in v.as_object().into_iter().flatten() {
                            d[k] = x.clone();
                        }
                    }
                    None => {
                        self.downloads.insert(0, v.clone());
                        self.js(&format!("dlStarted({})", v["name"]));
                    }
                }
                if v["state"] != "run" || v.get("recv").is_none() {
                    self.push();
                }
                self.push_downloads();
            }
            Ev::Imported(r) => {
                let (mut nb, mut nh) = (0, 0);
                for b in r["bookmarks"].as_array().into_iter().flatten() {
                    if !self.bookmarks.iter().any(|x| x["u"] == b["u"]) {
                        self.bookmarks.push(b.clone());
                        nb += 1;
                    }
                }
                for h in r["history"].as_array().into_iter().flatten() {
                    match self.history.iter_mut().find(|x| x["u"] == h["u"]) {
                        Some(x) => {
                            let c = x["c"].as_u64().unwrap_or(1).max(h["c"].as_u64().unwrap_or(1));
                            x["c"] = json!(c);
                        }
                        None => {
                            self.history.push(h.clone());
                            nh += 1;
                        }
                    }
                }
                self.history.sort_by_key(|h| std::cmp::Reverse(h["n"].as_i64().unwrap_or(0)));
                self.history.truncate(3000);
                self.save();
                self.push();
                self.push_lists();
                self.js(&format!("impDone({})", json!({"b": nb, "h": nh, "errors": r["errors"]})));
            }
            Ev::ExtFetched(r) => match r {
                Ok((store, dir)) => {
                    let Some(p7) = self.profile7() else { return };
                    let p = self.proxy.clone();
                    let d2 = dir.clone();
                    unsafe {
                        let res = p7.AddBrowserExtension(
                            &HSTRING::from(dir.as_path()),
                            &ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |res, e| {
                                let out = match (res, e) {
                                    (Ok(()), Some(e)) => {
                                        let (mut id, mut name) = (PWSTR::null(), PWSTR::null());
                                        e.Id(&mut id)?;
                                        e.Name(&mut name)?;
                                        Ok((webview2_com::take_pwstr(id), webview2_com::take_pwstr(name), d2))
                                    }
                                    (r, _) => Err(format!("{:?}", r.err())),
                                };
                                let _ = p.send_event(Ev::ExtAdded(out));
                                Ok(())
                            })),
                        );
                        if let Err(e) = res {
                            self.js(&format!("extErr({})", json!(e.message())));
                        }
                    }
                    let _ = store;
                }
                Err(e) => self.js(&format!("extErr({})", json!(e))),
            },
            Ev::ExtAdded(r) => match r {
                Ok((id, name, dir)) => {
                    let mut m = ext::manifest_info(&dir);
                    m["dir"] = json!(dir);
                    m["store"] = json!(dir.file_name().map(|f| f.to_string_lossy().into_owned()));
                    self.ext_meta[&id] = m;
                    self.save_ext_meta();
                    self.js(&format!("extAdded({})", json!(name)));
                    self.refresh_exts();
                }
                Err(e) => self.js(&format!("extErr({})", json!(e))),
            },
            Ev::ExtList(list) => {
                // seulement les extensions installées par Plume (pas les composants internes d'Edge)
                self.exts = list
                    .into_iter()
                    .filter(|e| self.ext_meta[e["id"].as_str().unwrap_or("")].is_object())
                    .map(|mut e| {
                        let m = &self.ext_meta[e["id"].as_str().unwrap_or("")];
                        if m["dir"].is_string() && m.get("side").is_none() {
                            e["side"] = ext::manifest_info(std::path::Path::new(m["dir"].as_str().unwrap_or("")))["side"].clone();
                        }
                        for k in ["icon", "popup", "options", "store", "side"] {
                            e[k] = m[k].clone();
                        }
                        e
                    })
                    .collect();
                self.push_exts();
                self.push();
            }
        }
    }

    /// Messages des pages web (script page.js). L'origine vient de WebView2, pas de la page.
    fn on_page(&mut self, id: u32, uri: &str, body: &str) {
        let Some(i) = self.idx(id) else { return };
        let v: Value = serde_json::from_str(body).unwrap_or_default();
        let host = import::host_of(uri);
        let web = uri.starts_with("https://") || uri.starts_with("http://");
        match v["c"].as_str().unwrap_or("") {
            "pw-get" if web && !self.tabs[i].private => self.pw_request(i, &host),
            "pw-save" if web && !self.tabs[i].private => {
                let (u, p) = (str_of(&v["u"]), str_of(&v["p"]));
                if p.is_empty() || p.len() > 512 || strs(&self.cfg["never"]).contains(&host) {
                    return;
                }
                let mut upd = false;
                if self.vault.unlocked() {
                    match self.vault.matches(&host).iter().find(|x| x["u"] == u.as_str()) {
                        Some(x) if x["p"] == p.as_str() => return,
                        Some(_) => upd = true,
                        None => {}
                    }
                }
                self.pending_pw = Some(json!({"o": host, "u": u, "p": p, "t": now_ms()}));
                self.tabs[i].info = Some(json!({"k": "save", "host": host, "u": u, "upd": upd}));
            }
            "reader-pref" => {
                self.cfg["readerSize"] = v["size"].clone();
                self.cfg["readerTheme"] = v["theme"].clone();
            }
            "speak" => self.tabs[i].speaking = v["on"] == true,
            _ => return,
        }
        if i == self.active {
            self.push();
        }
    }

    fn on_ui(&mut self, msg: &str) {
        let v: Value = serde_json::from_str(msg).unwrap_or_default();
        let id = v["id"].as_u64().unwrap_or(0) as u32;
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        match v["c"].as_str().unwrap_or("") {
            "ready" => {
                self.ui_ready = true;
                self.relayout();
                self.push();
                self.push_lists();
                self.push_exts();
                if self.blank() {
                    self.js("focusBar()");
                }
            }
            "go" => self.go(&s("u")),
            "new" => self.cmd(Cmd::NewTab),
            "private" => self.cmd(Cmd::Private),
            "window" => self.cmd(Cmd::NewWindow),
            // déplacer un onglet dans une nouvelle fenêtre
            "tab-window" => {
                if let Some(i) = self.idx(id).filter(|_| self.tabs.len() > 1) {
                    let t = self.tabs.remove(i);
                    if let Some(v) = &t.view {
                        let _ = v.set_visible(false);
                    }
                    let private = t.private;
                    let a = if i < self.active { self.active - 1 } else { self.active.min(self.tabs.len() - 1) };
                    self.active = usize::MAX;
                    self.activate(a);
                    self.want_window.push(WinReq { url: None, private, tab: Some(t), restore: vec![], active: 0 });
                }
            }
            "media-probe" | "media-install" => {
                let (u, p, install) = (s("u"), self.proxy.clone(), v["c"] == "media-install");
                if !install && !media::have_ytdlp() {
                    self.js(&format!("MEDIA({})", json!({"state": "need", "url": u})));
                } else {
                    media::self_update();
                    std::thread::spawn(move || {
                        let r = if install { media::install(true, &p) } else { Ok(()) }.and_then(|_| media::probe(&u));
                        let _ = p.send_event(Ev::Media(match r {
                            Ok(info) => json!({"state": "info", "info": info}),
                            Err(e) => json!({"state": "error", "msg": e}),
                        }));
                    });
                }
            }
            "media-get" => {
                media::start(s("u"), s("kind"), s("fmt"), v["h"].as_u64().unwrap_or(0), s("title"), self.proxy.clone());
            }
            "open" => {
                let u = self.to_url(&s("u"));
                let p = self.tabs.get(self.active).is_some_and(|t| t.private);
                self.new_tab(Some(u), true, p)
            }
            "close" => {
                if let Some(i) = self.idx(id) {
                    self.close_tab(i)
                }
            }
            "act" => {
                if let Some(i) = self.idx(id) {
                    if i != self.active {
                        self.activate(i)
                    }
                }
            }
            "move" => {
                let to = v["to"].as_u64().unwrap_or(0) as usize;
                if let Some(i) = self.idx(id) {
                    let aid = self.tabs[self.active].id;
                    let t = self.tabs.remove(i);
                    self.tabs.insert(to.min(self.tabs.len()), t);
                    self.active = self.idx(aid).unwrap_or(0);
                    // déposé au milieu d'un groupe → rejoint ce groupe ; sinon le quitte
                    let pos = self.idx(id).unwrap_or(0);
                    let nb = |o: Option<&Tab>| o.map_or(0, |t| t.group);
                    let (a, b) = (nb(pos.checked_sub(1).and_then(|p| self.tabs.get(p))), nb(self.tabs.get(pos + 1)));
                    self.tabs[pos].group = if a == b { a } else if self.tabs[pos].group == a || self.tabs[pos].group == b { self.tabs[pos].group } else { 0 };
                    self.order_pins();
                    self.gc_groups();
                    self.push();
                }
            }
            // épingler / détacher un onglet
            "pin" => {
                if let Some(i) = self.idx(id) {
                    let t = &mut self.tabs[i];
                    t.pinned = !t.pinned;
                    t.group = 0;
                    let pinned = t.pinned;
                    let aid = self.tabs[self.active].id;
                    let t = self.tabs.remove(i);
                    // épinglé → à la fin des épinglés ; détaché → juste après eux
                    let pins = self.tabs.iter().filter(|t| t.pinned).count();
                    self.tabs.insert(if pinned { pins } else { pins.min(self.tabs.len()) }, t);
                    self.active = self.idx(aid).unwrap_or(0);
                    self.gc_groups();
                    self.save();
                    self.push();
                }
            }
            "dup" => {
                if let Some(i) = self.idx(id) {
                    let (u, p) = (self.tabs[i].url.clone(), self.tabs[i].private);
                    if !u.is_empty() {
                        self.active = i;
                        self.new_tab(Some(u), true, p);
                    }
                }
            }
            "close-others" | "close-right" => {
                if let Some(i) = self.idx(id) {
                    let right = v["c"] == "close-right";
                    let keep: Vec<u32> = self.tabs.iter().enumerate().filter(|(k, _)| *k == i || (right && *k < i)).map(|(_, t)| t.id).collect();
                    self.tabs.retain(|t| keep.contains(&t.id));
                    self.active = usize::MAX;
                    self.gc_groups();
                    let a = self.idx(id).unwrap_or(0);
                    self.activate(a);
                }
            }
            "reload-tab" => {
                if let Some(v) = self.idx(id).and_then(|i| self.tabs[i].view.as_ref()) {
                    let _ = v.reload();
                }
            }
            "grp-new" => {
                if let Some(i) = self.idx(id) {
                    let gid = self.groups.iter().filter_map(|g| g["id"].as_u64()).max().unwrap_or(0) as u32 + 1;
                    const COLORS: [&str; 8] = ["blue", "purple", "pink", "red", "orange", "yellow", "green", "teal"];
                    self.groups.push(json!({"id": gid, "name": "", "color": COLORS[(gid as usize - 1) % 8], "col": false}));
                    self.tabs[i].group = gid;
                    self.regroup(i);
                    self.gc_groups();
                    self.push();
                    self.js(&format!("renameGroup({gid})"));
                }
            }
            "grp-add" => {
                if let Some(i) = self.idx(id) {
                    self.tabs[i].group = v["g"].as_u64().unwrap_or(0) as u32;
                    self.regroup(i);
                    self.gc_groups();
                    self.push();
                }
            }
            "grp-set" => {
                let g = v["g"].as_u64();
                if let Some(gr) = self.groups.iter_mut().find(|x| x["id"].as_u64() == g) {
                    for k in ["name", "color", "col"] {
                        if !v[k].is_null() {
                            gr[k] = v[k].clone();
                        }
                    }
                }
                self.save();
                self.push();
            }
            "grp-ungroup" | "grp-close" => {
                let g = v["g"].as_u64().unwrap_or(0) as u32;
                if v["c"] == "grp-close" {
                    let aid = self.tabs[self.active].id;
                    let closing_active = self.tabs[self.active].group == g;
                    self.tabs.retain(|t| t.group != g);
                    if self.tabs.is_empty() {
                        self.groups.clear();
                        self.new_tab(None, true, false);
                        return;
                    }
                    self.active = usize::MAX;
                    let a = if closing_active { 0 } else { self.idx(aid).unwrap_or(0) };
                    self.activate(a);
                } else {
                    for t in self.tabs.iter_mut().filter(|t| t.group == g) {
                        t.group = 0;
                    }
                }
                self.gc_groups();
                self.push();
            }
            "back" => unsafe {
                if let Some(c) = self.core() {
                    let _ = c.GoBack();
                }
            },
            "fwd" => unsafe {
                if let Some(c) = self.core() {
                    let _ = c.GoForward();
                }
            },
            "reload" => {
                if let Some(w) = self.active_view() {
                    let _ = w.reload();
                }
            }
            "stop" => unsafe {
                if let Some(c) = self.core() {
                    let _ = c.Stop();
                }
            },
            "drag" => {
                let _ = self.window.drag_window();
            }
            "max" => {
                self.window.set_maximized(!self.window.is_maximized());
                self.push();
            }
            "min" => self.window.set_minimized(true),
            "quit" => self.close_window(),
            "resize" => {
                let d = match v["d"].as_str().unwrap_or("n") {
                    "nw" => ResizeDirection::NorthWest,
                    "ne" => ResizeDirection::NorthEast,
                    _ => ResizeDirection::North,
                };
                let _ = self.window.drag_resize_window(d);
            }
            "ov" => self.set_overlay(v["v"].as_bool().unwrap_or(false), v["f"].as_bool().unwrap_or(false)),
            "h" => {
                self.ui_h = v["h"].as_f64().unwrap_or(84.0).clamp(30.0, 300.0);
                self.side_w = v["w"].as_f64().unwrap_or(0.0).clamp(0.0, 600.0);
                if let Some(d) = v["dh"].as_f64() {
                    self.dev.set_height(d);
                }
                self.relayout();
            }
            "star" => self.toggle_star(),
            // favoris réordonnés par glisser-déposer
            "bm-move" => {
                let (from, to) = (v["from"].as_u64().unwrap_or(0) as usize, v["to"].as_u64().unwrap_or(0) as usize);
                if from < self.bookmarks.len() && to < self.bookmarks.len() {
                    let b = self.bookmarks.remove(from);
                    self.bookmarks.insert(to, b);
                    self.save();
                    self.push();
                }
            }
            "bmdel" => {
                let u = s("u");
                self.bookmarks.retain(|b| b["u"] != u.as_str());
                self.save();
                self.push();
            }
            "bmbar" => self.cmd(Cmd::BmBar),
            "reopen" => self.cmd(Cmd::Reopen),
            "dev" => self.cmd(Cmd::Dev),
            "zoom" => {
                let d = v["d"].as_f64().unwrap_or(0.0);
                let z = if d == 0.0 { 1.0 } else { (self.zoom() + d * 0.1).clamp(0.25, 5.0) };
                if let Some(w) = self.active_view() {
                    let _ = w.zoom(z);
                }
                self.push();
            }
            "print" => {
                if let Some(w) = self.active_view() {
                    let _ = w.print();
                }
            }
            "histdel" => {
                self.history.clear();
                self.save();
                self.push_lists();
            }
            "histrm" => {
                let u = s("u");
                self.history.retain(|h| h["u"] != u.as_str());
                self.save();
                self.push_lists();
            }
            "clear" => {
                let _ = self.ui.clear_all_browsing_data();
                self.history.clear();
                self.closed.clear();
                self.save();
                self.push_lists();
                self.toast("cleared");
            }
            // --- fonctions de page ---
            "reader" => self.reader(),
            "speak" => {
                let r = self.cfg["ttsRate"].as_f64().unwrap_or(1.0);
                self.eval_tab(self.active, &format!("window.__plume&&__plume.speak({r})"));
            }
            "translate" => {
                let u = self.tabs[self.active].url.clone();
                if u.starts_with("http") && !u.contains("translate.goog") {
                    let tl = self.lang().to_string();
                    let enc: String = u.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
                    self.go(&format!("https://translate.google.com/translate?sl=auto&tl={tl}&u={enc}"));
                }
            }
            "dark-site" => {
                let h = import::host_of(&self.tabs[self.active].url);
                if !h.is_empty() {
                    let key = if self.cfg["darkAll"] == true { "light" } else { "dark" };
                    self.toggle_list(key, &h);
                }
            }
            "ab-site" => {
                let h = import::host_of(&self.tabs[self.active].url);
                if !h.is_empty() {
                    self.toggle_list("allow", &h);
                    if let Some(w) = self.active_view() {
                        let _ = w.reload();
                    }
                }
            }
            // --- barre d'info (mots de passe, Web Store) ---
            "info" => {
                let i = self.active;
                let info = self.tabs[i].info.take().unwrap_or_default();
                match s("a").as_str() {
                    "save" => {
                        if self.vault.unlocked() {
                            self.after_unlock();
                        } else {
                            self.tabs[i].info = Some(info); // garde la barre jusqu'au déverrouillage
                            self.js(if self.vault.exists() { "openUnlock()" } else { "openPanel('set','pw')" });
                        }
                    }
                    "never" => {
                        self.pending_pw = None;
                        if let Some(h) = info["host"].as_str() {
                            let h = h.to_string();
                            self.toggle_list("never", &h);
                        }
                    }
                    "unlock" => {
                        self.tabs[i].info = Some(info);
                        self.js("openUnlock()");
                    }
                    "fill" => {
                        let host = str_of(&info["host"]);
                        let k = v["i"].as_u64().unwrap_or(0) as usize;
                        let pick = self.vault.matches(&host).get(k).map(|x| (*x).clone());
                        if let Some(x) = pick {
                            self.fill(i, &x);
                        }
                    }
                    "store" => self.install_ext(&str_of(&info["id"])),
                    "ext-enable" => {
                        self.cfg["ext"] = json!(true);
                        self.restart();
                    }
                    _ => self.pending_pw = None,
                }
                self.push();
            }
            // --- coffre ---
            "vault-create" => {
                let r = self.vault.create(&s("pw"));
                self.js(&format!("vaultRes({})", json!({"ok": r.is_ok(), "err": r.err()})));
                if self.vault.unlocked() {
                    self.after_unlock();
                }
                self.push_vault();
                self.push();
            }
            "vault-unlock" => {
                let r = self.vault.unlock(&s("pw"));
                self.js(&format!("vaultRes({})", json!({"ok": r.is_ok(), "err": r.as_ref().err()})));
                if r.is_ok() {
                    self.after_unlock();
                }
                self.push_vault();
                self.push();
            }
            "vault-lock" => {
                self.vault.lock();
                self.push_vault();
                self.push();
            }
            "vault-list" => self.push_vault(),
            "vault-reveal" => {
                self.pw_used = Instant::now();
                let k = v["i"].as_u64().unwrap_or(0) as usize;
                if let Some(p) = self.vault.items.get(k).map(|x| x["p"].clone()) {
                    self.js(&format!("R({k},{p})"));
                }
            }
            "vault-del" => {
                let k = v["i"].as_u64().unwrap_or(0) as usize;
                if self.vault.unlocked() && k < self.vault.items.len() {
                    self.vault.items.remove(k);
                    let _ = self.vault.save();
                    self.sync_pw_hosts();
                }
                self.push_vault();
                self.push();
            }
            "vault-add" => {
                if self.vault.unlocked() {
                    let host = import::host_of(&s("o"));
                    if !host.is_empty() && !s("p").is_empty() {
                        self.vault.upsert(json!({"o": host, "u": s("u"), "p": s("p"), "t": now_ms()}));
                        let _ = self.vault.save();
                        self.sync_pw_hosts();
                    }
                }
                self.push_vault();
                self.push();
            }
            "vault-reset" => {
                let _ = self.vault.delete();
                self.pw_hosts.clear();
                self.save();
                self.push_vault();
                self.push();
            }
            "pw-csv" => self.import_csv(),
            // --- import ---
            "imp-detect" => {
                // vraies icônes, lues dans l'exécutable de chaque navigateur installé
                let mut l = import::detect();
                for x in &mut l {
                    if let Some(icon) = bicon::exe_for(x["browser"].as_str().unwrap_or("")).and_then(|e| bicon::data_uri(&e, 64)) {
                        x["icon"] = json!(icon);
                    }
                }
                self.js(&format!("IMPL({})", Value::Array(l)));
            }
            "imp-run" => {
                let all = import::detect();
                let ids: Vec<u64> = v["ids"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()).collect();
                let srcs: Vec<Value> = all.into_iter().filter(|x| x["id"].as_u64().is_some_and(|i| ids.contains(&i))).collect();
                let (h, b, p) = (v["h"] != false, v["b"] != false, self.proxy.clone());
                std::thread::spawn(move || {
                    let r = import::run(&srcs, h, b);
                    let _ = p.send_event(Ev::Imported(r));
                });
            }
            // --- extensions ---
            "ext-list" => self.refresh_exts(),
            "ext-install" => self.install_ext(&s("id")),
            "ext-toggle" => self.with_ext(s("id"), "enable", v["on"] == true),
            "ext-remove" => {
                let eid = s("id");
                if let Some(d) = self.ext_meta[&eid]["dir"].as_str() {
                    let d = PathBuf::from(d);
                    // le dossier est supprimé au prochain démarrage s'il est encore utilisé
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(2));
                        let _ = std::fs::remove_dir_all(d);
                    });
                }
                if let Some(m) = self.ext_meta.as_object_mut() {
                    m.remove(&eid);
                }
                self.save_ext_meta();
                self.with_ext(eid, "remove", false);
            }
            "ext-open" => {
                let (eid, page) = (s("id"), s("page"));
                if page.is_empty() {
                    return;
                }
                let url = format!("chrome-extension://{eid}/{page}");
                match s("how").as_str() {
                    "popup" => {
                        if self.popup.is_some() && self.popup_ext == eid {
                            self.popup = None; // 2e clic sur le bouton → ferme
                            self.popup_ext.clear();
                        } else {
                            self.open_ext_popup(&url, v["x"].as_f64().unwrap_or(10000.0));
                            self.popup_ext = eid;
                        }
                    }
                    "side" => {
                        let e = self.exts.iter().find(|e| e["id"] == eid.as_str()).cloned().unwrap_or(json!({"id": eid}));
                        self.open_side_panel(&url, json!({"id": e["id"], "name": e["name"], "icon": e["icon"], "page": page}));
                    }
                    _ => self.new_tab(Some(url), true, false),
                }
            }
            "popup-close" => {
                // la popup perd le focus juste après sa création : on l'ignore
                if v["lost"] == true && self.popup_at.elapsed() < Duration::from_millis(900) {
                    return;
                }
                self.popup = None;
                self.popup_ext.clear();
            }
            "popup-measure" => {
                if let Some(p) = &self.popup {
                    let (px, wid) = (self.proxy.clone(), self.wid);
                    let _ = p.evaluate_script_with_callback(
                        "(()=>{const d=document.documentElement,b=document.body||d;return [Math.max(b.scrollWidth,d.scrollWidth),Math.max(b.scrollHeight,d.scrollHeight)]})()",
                        move |r| {
                            let a: Vec<f64> = serde_json::from_str(&r).unwrap_or_default();
                            if a.len() == 2 {
                                let _ = px.send_event(Ev::PopupSize(wid, a[0], a[1]));
                            }
                        },
                    );
                }
            }
            "popup-to-side" => {
                if let Some(p) = self.popup.take() {
                    let url = p.url().unwrap_or_default();
                    let eid = std::mem::take(&mut self.popup_ext);
                    let e = self.exts.iter().find(|e| e["id"] == eid.as_str()).cloned().unwrap_or(json!({"id": eid}));
                    self.open_side_panel(&url, json!({"id": e["id"], "name": e["name"], "icon": e["icon"]}));
                }
            }
            "side-close" => self.close_side_panel(),
            "ovr" => {
                self.ovr_full = v["full"] == true;
                self.ovr_rects = v["rects"].as_array().into_iter().flatten()
                    .filter_map(|r| Some([r[0].as_f64()?, r[1].as_f64()?, r[2].as_f64()?, r[3].as_f64()?]))
                    .collect();
                self.relayout();
            }
            // chrome.sidePanel.open() appelé par une extension (ex. bouton « panneau latéral » de Phantom)
            "side-open" => {
                let uri = s("uri");
                let eid = uri.trim_start_matches("chrome-extension://").split('/').next().unwrap_or("").to_string();
                if let Some(e) = self.exts.iter().find(|e| e["id"] == eid.as_str()).cloned() {
                    let page = [s("page"), str_of(&e["side"]), str_of(&e["popup"])].into_iter().find(|p| !p.is_empty()).unwrap_or_default();
                    self.popup = None;
                    self.popup_ext.clear();
                    let url = format!("chrome-extension://{eid}/{}", page.trim_start_matches('/'));
                    self.open_side_panel(&url, json!({"id": e["id"], "name": e["name"], "icon": e["icon"], "page": page}));
                }
            }
            "side-tab" => {
                if let Some(p) = &self.sidep {
                    let url = p.url().unwrap_or_default();
                    self.close_side_panel();
                    self.new_tab(Some(url), true, false);
                }
            }
            // --- téléchargements ---
            "dl-list" => self.push_downloads(),
            "dl-cancel" => {
                if media::cancel(id) {
                    return;
                }
                DLS.with(|m| {
                    if let Some(op) = m.borrow().get(&id) {
                        unsafe {
                            let _ = op.Cancel();
                        }
                    }
                });
            }
            "dl-open" | "dl-show" => {
                if let Some(d) = self.downloads.iter().find(|d| d["id"].as_u64() == Some(id as u64)) {
                    let path = str_of(&d["path"]);
                    unsafe {
                        if v["c"] == "dl-open" {
                            ShellExecuteW(None, w!("open"), &HSTRING::from(path), None, None, SW_SHOWNORMAL);
                        } else {
                            ShellExecuteW(None, w!("open"), w!("explorer.exe"), &HSTRING::from(format!("/select,\"{path}\"")), None, SW_SHOWNORMAL);
                        }
                    }
                }
            }
            "dl-clear" => {
                self.downloads.retain(|d| d["state"] == "run");
                self.push_downloads();
                self.push();
            }
            // --- réglages ---
            "set" => {
                let k = s("k");
                if !k.is_empty() && k != "onboarded" {
                    self.set_cfg(&k, v["v"].clone());
                }
            }
            "onboard" => {
                self.cfg["name"] = json!(s("name").chars().take(40).collect::<String>());
                for (k, x) in preset(&s("mode")).as_object().unwrap() {
                    self.cfg[k] = x.clone();
                }
                self.cfg["onboarded"] = json!(true);
                self.apply_profile();
                adblock::ON.store(true, Ordering::Relaxed);
                self.save();
                self.push();
            }
            "mode" => {
                for (k, x) in preset(&s("mode")).as_object().unwrap() {
                    self.cfg[k] = x.clone();
                }
                adblock::ON.store(true, Ordering::Relaxed);
                self.apply_profile();
                self.save();
                self.push();
            }
            "restart" => self.restart(),
            // --- DevTools intégrés ---
            "dt" => {
                if let Some(core) = self.dev_core() {
                    if let Some(js) = self.dev.on_ui(&v, &core, &self.proxy) {
                        self.js(&js);
                    }
                }
            }
            "dt-close" => self.dev_close(),
            "dt-edge" => {
                if let Some(w) = self.active_view() {
                    w.open_devtools();
                }
            }
            _ => {}
        }
    }
}

/// Fenêtre Plume sans bordure (coins arrondis, mode sombre, icône).
fn make_window(target: &EventLoopWindowTarget<Ev>, pos: Option<tao::dpi::PhysicalPosition<i32>>, dark: bool, proxy: &EventLoopProxy<Ev>, wid: u32) -> Window {
    let mut b = WindowBuilder::new()
        .with_title("Plume")
        .with_window_icon(Icon::from_resource(1, None).ok())
        .with_taskbar_icon(Icon::from_resource(1, None).ok())
        .with_decorations(false)
        .with_undecorated_shadow(true)
        .with_drag_and_drop(false)
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(480.0, 320.0));
    if let Some(p) = pos {
        b = b.with_position(p);
    }
    let window = b.build(target).expect("fenêtre");
    let hwnd = HWND(window.hwnd() as _);
    unsafe {
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as _, 4);
        let dark: i32 = dark as i32;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
        let data = Box::into_raw(Box::new((proxy.clone(), wid))) as usize;
        let _ = SetWindowSubclass(hwnd, Some(parent_proc), 1, data);
    }
    window
}

/// Interface (onglets, barre d'adresse, panneaux) d'une fenêtre, servie par plume://.
fn build_ui(window: &Window, base: WebViewBuilder<'_>, proxy: &EventLoopProxy<Ev>, wid: u32) -> WebView {
    let p = proxy.clone();
    let ui = base
        .with_custom_protocol("plume".into(), |_, _| {
            wry::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(std::borrow::Cow::Borrowed(UI_HTML.as_bytes()))
                .unwrap()
        })
        .with_url("plume://ui/")
        .with_bounds(Rect { position: PhysicalPosition::new(0, 0).into(), size: window.inner_size().into() })
        .with_background_color((28, 28, 30, 255))
        .with_devtools(cfg!(debug_assertions))
        .with_default_context_menus(false)
        .with_browser_accelerator_keys(false)
        .with_hotkeys_zoom(false)
        .with_ipc_handler(move |r| {
            let _ = p.send_event(Ev::Ui(wid, r.body().clone()));
        })
        .build_as_child(window)
        .expect("ui");
    clip_siblings(&ui);
    unsafe {
        let _ = hook_accel(&ui.controller(), proxy, Who::Win(wid));
    }
    ui
}

/// Demande de fenêtre avec dimensions ou position = popup (et non un nouvel onglet).
fn is_popup(f: &wry::NewWindowFeatures) -> bool {
    f.size.is_some() || f.position.is_some()
}

fn host_of(u: &str) -> &str {
    let rest = u.split_once("://").map_or(u, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or("")
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect()
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Langue de Windows : français si l'interface est en français, sinon anglais.
fn sys_lang() -> &'static str {
    let mut buf = [0u16; 85];
    let n = unsafe { windows::Win32::Globalization::GetUserDefaultLocaleName(&mut buf) };
    if String::from_utf16_lossy(&buf[..n.max(1) as usize - 1]).starts_with("fr") { "fr" } else { "en" }
}

/// Boîte de dialogue Windows « Ouvrir » (fichiers CSV).
fn open_file(owner: HWND) -> Option<PathBuf> {
    let mut buf = [0u16; 1024];
    let filter: Vec<u16> = "CSV (*.csv)\0*.csv\0*.*\0*.*\0\0".encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if unsafe { GetOpenFileNameW(&mut ofn) }.as_bool() {
        let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
    } else {
        None
    }
}

/// Raccourcis clavier façon Chrome (interceptés avant la page).
fn accel(vk: u32, ctrl: bool, shift: bool, alt: bool) -> Option<Cmd> {
    Some(match (ctrl, shift, alt, vk) {
        (true, false, false, 0x54) => Cmd::NewTab,                     // Ctrl+T
        (true, false, false, 0x4E) => Cmd::NewWindow,                  // Ctrl+N
        (true, true, false, 0x4E) => Cmd::Private,                     // Ctrl+Maj+N
        (true, false, false, 0x57 | 0x73) => Cmd::Close,               // Ctrl+W / Ctrl+F4
        (true, true, false, 0x54) => Cmd::Reopen,                      // Ctrl+Maj+T
        (true, false, false, 0x09 | 0x22) => Cmd::Next,                // Ctrl+Tab / Ctrl+PgDn
        (true, true, false, 0x09) | (true, false, false, 0x21) => Cmd::Prev,
        (true, false, false, 0x31..=0x38) => Cmd::Nth((vk - 0x31) as usize),
        (true, false, false, 0x39) => Cmd::Last,
        (true, false, false, 0x4C | 0x4B | 0x45) | (false, false, false, 0x75) | (false, false, true, 0x44) => {
            Cmd::Focus // Ctrl+L / Ctrl+K / Ctrl+E / F6 / Alt+D
        }
        (true, false, false, 0x44) => Cmd::Star,                       // Ctrl+D
        (true, true, false, 0x42) => Cmd::BmBar,                       // Ctrl+Maj+B
        (true, false, false, 0x48) => Cmd::History,                    // Ctrl+H
        (true, false, false, 0x4A) => Cmd::Downloads,                  // Ctrl+J
        (false, false, false, 0x78) => Cmd::Reader,                    // F9 (comme Edge)
        (true, true, false, 0x4A) if cfg!(debug_assertions) => Cmd::UiDev,
        (false, false, false, 0x7B) | (true, true, false, 0x49) => Cmd::Dev, // F12 / Ctrl+Maj+I
        _ => return None,
    })
}

unsafe fn hook_accel(ctrl: &ICoreWebView2Controller, p: &EventLoopProxy<Ev>, who: Who) -> Result<()> {
    let p = p.clone();
    let mut tok = 0i64;
    unsafe {
        ctrl.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(a) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                a.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                    return Ok(());
                }
                let mut vk = 0u32;
                a.VirtualKey(&mut vk)?;
                let down = |k: VIRTUAL_KEY| GetKeyState(k.0 as i32) < 0;
                if let Some(c) = accel(vk, down(VK_CONTROL), down(VK_SHIFT), down(VK_MENU)) {
                    a.SetHandled(true)?;
                    let _ = p.send_event(Ev::Cmd(who, c));
                }
                Ok(())
            })),
            &mut tok,
        )
    }
}

/// Bloqueur de pubs, téléchargements, suivi URL / favicon / historique, raccourcis : via l'API WebView2.
unsafe fn hook_tab(v: &WebView, id: u32, proxy: &EventLoopProxy<Ev>) -> Result<()> {
    unsafe {
        let core = v.webview();
        hook_accel(&v.controller(), proxy, Who::Tab(id))?;
        let pf = proxy.clone();
        let mut tok0 = 0i64;
        v.controller().add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                let _ = pf.send_event(Ev::Cmd(Who::Tab(id), Cmd::PageFocus));
                Ok(())
            })),
            &mut tok0,
        )?;
        // Toutes les sous-requêtes passent par le filtre (HashSet : coût négligeable)
        match core.cast::<ICoreWebView2_22>() {
            Ok(c) => c.AddWebResourceRequestedFilterWithRequestSourceKinds(
                w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?,
            Err(_) => core.AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?,
        }
        let env = v.environment();
        let mut tok = 0i64;
        let p = proxy.clone();
        core.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |s, args| {
                let (Some(s), Some(a)) = (s, args) else { return Ok(()) };
                let mut ctx = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
                a.ResourceContext(&mut ctx)?;
                if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT {
                    return Ok(()); // on ne bloque jamais la page elle-même
                }
                let mut u = PWSTR::null();
                a.Request()?.Uri(&mut u)?;
                let u = webview2_com::take_pwstr(u);
                if !adblock::listed(host_of(&u).split(':').next().unwrap_or("")) {
                    return Ok(());
                }
                let mut src = PWSTR::null();
                s.Source(&mut src)?;
                if adblock::allowed(&import::host_of(&webview2_com::take_pwstr(src))) {
                    return Ok(());
                }
                let r = env.CreateWebResourceResponse(None, 403, w!("Blocked"), w!(""))?;
                a.SetResponse(&r)?;
                let _ = p.send_event(Ev::Blocked(id));
                Ok(())
            })),
            &mut tok,
        )?;
        let p = proxy.clone();
        core.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |s, _| {
                if let Some(s) = s {
                    let mut u = PWSTR::null();
                    s.Source(&mut u)?;
                    let _ = p.send_event(Ev::Url(id, webview2_com::take_pwstr(u)));
                }
                Ok(())
            })),
            &mut tok,
        )?;
        // https://localhost… sur un serveur de dev en HTTP → on réessaie en http:// (comme Chrome)
        core.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |s, args| {
                if let (Some(s), Some(args)) = (s, args) {
                    let mut ok = BOOL(0);
                    let mut st = COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN;
                    args.IsSuccess(&mut ok)?;
                    args.WebErrorStatus(&mut st)?;
                    if !ok.as_bool() && st != COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
                        let mut u = PWSTR::null();
                        s.Source(&mut u)?;
                        let u = webview2_com::take_pwstr(u);
                        if let Some(rest) = u.strip_prefix("https://").filter(|r| is_local(r)) {
                            s.Navigate(&HSTRING::from(format!("http://{rest}")))?;
                        }
                    }
                }
                Ok(())
            })),
            &mut tok,
        )?;
        let p = proxy.clone();
        core.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |s, _| {
                if let Some(s) = s {
                    let (mut b, mut f) = (BOOL(0), BOOL(0));
                    s.CanGoBack(&mut b)?;
                    s.CanGoForward(&mut f)?;
                    let _ = p.send_event(Ev::Nav(id, b.as_bool(), f.as_bool()));
                }
                Ok(())
            })),
            &mut tok,
        )?;
        if let Ok(c15) = core.cast::<ICoreWebView2_15>() {
            let p = proxy.clone();
            c15.add_FaviconChanged(
                &FaviconChangedEventHandler::create(Box::new(move |s, _| {
                    if let Some(s) = s.and_then(|s| s.cast::<ICoreWebView2_15>().ok()) {
                        let mut u = PWSTR::null();
                        s.FaviconUri(&mut u)?;
                        let _ = p.send_event(Ev::Fav(id, webview2_com::take_pwstr(u)));
                    }
                    Ok(())
                })),
                &mut tok,
            )?;
        }
        // Téléchargements : notre propre panneau au lieu de la bulle d'Edge
        if let Ok(c4) = core.cast::<ICoreWebView2_4>() {
            let p = proxy.clone();
            c4.add_DownloadStarting(
                &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                    let Some(a) = args else { return Ok(()) };
                    a.SetHandled(true)?;
                    let op = a.DownloadOperation()?;
                    let mut path = PWSTR::null();
                    a.ResultFilePath(&mut path)?;
                    let path = webview2_com::take_pwstr(path);
                    let mut uri = PWSTR::null();
                    op.Uri(&mut uri)?;
                    let mut total = 0i64;
                    op.TotalBytesToReceive(&mut total)?;
                    let did = DL_ID.fetch_add(1, Ordering::Relaxed) + 1;
                    let name = std::path::Path::new(&path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                    let _ = p.send_event(Ev::Dl(json!({"id": did, "name": name, "path": path, "url": webview2_com::take_pwstr(uri),
                        "total": total, "recv": 0, "state": "run", "t": now_ms()})));
                    let mut t2 = 0i64;
                    let (p2, last) = (p.clone(), Cell::new(Instant::now()));
                    op.add_BytesReceivedChanged(
                        &BytesReceivedChangedEventHandler::create(Box::new(move |s, _| {
                            if let Some(s) = s.filter(|_| last.get().elapsed() > Duration::from_millis(300)) {
                                last.set(Instant::now());
                                let (mut r, mut t) = (0i64, 0i64);
                                s.BytesReceived(&mut r)?;
                                s.TotalBytesToReceive(&mut t)?;
                                let _ = p2.send_event(Ev::Dl(json!({"id": did, "recv": r, "total": t})));
                            }
                            Ok(())
                        })),
                        &mut t2,
                    )?;
                    let p3 = p.clone();
                    op.add_StateChanged(
                        &StateChangedEventHandler::create(Box::new(move |s, _| {
                            if let Some(s) = s {
                                let mut st = COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS;
                                s.State(&mut st)?;
                                let (mut r, mut t) = (0i64, 0i64);
                                s.BytesReceived(&mut r)?;
                                s.TotalBytesToReceive(&mut t)?;
                                let state = match st {
                                    COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED => "done",
                                    COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED => "fail",
                                    _ => "run",
                                };
                                let _ = p3.send_event(Ev::Dl(json!({"id": did, "state": state, "recv": r, "total": t})));
                                if state != "run" {
                                    DLS.with(|m| m.borrow_mut().remove(&did));
                                }
                            }
                            Ok(())
                        })),
                        &mut t2,
                    )?;
                    DLS.with(|m| m.borrow_mut().insert(did, op));
                    Ok(())
                })),
                &mut tok,
            )?;
        }
        Ok(())
    }
}

unsafe extern "system" fn parent_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, data: usize) -> LRESULT {
    unsafe {
        let (proxy, wid) = &*(data as *const (EventLoopProxy<Ev>, u32));
        match msg {
            WM_ERASEBKGND => {
                let mut rc = RECT::default();
                let _ = GetClientRect(h, &mut rc);
                let br = CreateSolidBrush(COLORREF(0x001E1C1C)); // #1c1c1e
                FillRect(HDC(wp.0 as _), &rc, br);
                let _ = DeleteObject(br.into());
                return LRESULT(1);
            }
            WM_ACTIVATE => {
                let _ = proxy.send_event(Ev::Active(*wid, (wp.0 & 0xffff) as u32 != WA_INACTIVE));
            }
            _ => {}
        }
        DefSubclassProc(h, msg, wp, lp)
    }
}

/// Texte tapé → URL (ou recherche)
fn to_url(s: &str, search: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    if s.contains("://") || s.starts_with("about:") || s.starts_with("file:") {
        return s.into();
    }
    if !s.contains(' ') {
        // Serveurs locaux / réseau privé : en HTTP comme Chrome (localhost:3000, 192.168.1.10, monpc:8080)
        let port = s.split(['/', '?', '#']).next().unwrap_or("").rsplit_once(':')
            .is_some_and(|(h, p)| !h.is_empty() && !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
        if is_local(s) || (port && !s.contains('.')) {
            return format!("http://{s}");
        }
        if s.contains('.') {
            return format!("https://{s}");
        }
    }
    let mut q = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => q.push(b as char),
            b' ' => q.push('+'),
            _ => q.push_str(&format!("%{b:02X}")),
        }
    }
    format!("{search}{q}")
}

/// Hôte local ou réseau privé (accepte « hôte[:port][/chemin] » sans schéma).
fn is_local(s: &str) -> bool {
    let a = s.split(['/', '?', '#']).next().unwrap_or("");
    let h = if let Some(r) = a.strip_prefix('[') {
        r.split(']').next().unwrap_or("")
    } else {
        a.split(':').next().unwrap_or("")
    }
    .to_ascii_lowercase();
    let second = h.split('.').nth(1).and_then(|x| x.parse::<u8>().ok()).unwrap_or(0);
    h == "localhost"
        || h.ends_with(".localhost")
        || h.ends_with(".local")
        || h == "::1"
        || h == "0.0.0.0"
        || h.starts_with("127.")
        || h.starts_with("10.")
        || h.starts_with("192.168.")
        || (h.starts_with("172.") && (16..32).contains(&second))
}

/// Les vues de wry ne se découpent pas entre elles (pas de WS_CLIPSIBLINGS) : sans ça, la page
/// se dessine par-dessus l'interface même quand celle-ci est devant. On l'ajoute à chaque vue.
fn clip_siblings(v: &WebView) {
    unsafe {
        let mut host = HWND::default();
        if v.controller().ParentWindow(&mut host).is_ok() && !host.is_invalid() {
            let st = GetWindowLongPtrW(host, GWL_STYLE);
            SetWindowLongPtrW(host, GWL_STYLE, st | WS_CLIPSIBLINGS.0 as isize);
        }
    }
}
