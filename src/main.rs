// Plume — navigateur ultra-léger : Rust + WebView2 système, UI style iOS, onglets avec économiseur de mémoire,
// coffre à mots de passe chiffré, import de profils, extensions Chrome, bloqueur de pubs, mode lecture…
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "512"]

mod adblock;
mod bicon;
mod defbrowser;
mod devtools;
mod ext;
mod import;
mod look07;
mod media;
mod hello;
mod perf;
mod priv07;
mod pwgen;
mod settings;
mod setup;
mod sqlite;
mod tabs07;
mod tools07;
mod feats07;
mod update;
mod vault;

use serde_json::{Value, json};
use std::sync::RwLock;
use tabs07::WinX;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
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
    DownloadStartingEventHandler, FaviconChangedEventHandler, GetProcessExtendedInfosCompletedHandler, NavigationStartingEventHandler, FocusChangedEventHandler, WindowCloseRequestedEventHandler, HistoryChangedEventHandler,
    ContainsFullScreenElementChangedEventHandler, Microsoft::Web::WebView2::Win32::*, NavigationCompletedEventHandler, ProfileAddBrowserExtensionCompletedHandler,
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
    include_str!("ui/style07.css"),
    "</style></head><body>",
    include_str!("ui/body.html"),
    "<script>const SCHEMA=",
    include_str!("settings.json"),
    ";",
    include_str!("ui/i18n.js"),
    include_str!("ui/i18n07.js"),
    include_str!("ui/app.js"),
    include_str!("ui/settings.js"),
    include_str!("ui/pure07.js"),
    include_str!("ui/feat_core.js"),
    include_str!("ui/feat_perf.js"),
    include_str!("ui/feat_tabs.js"),
    include_str!("ui/feat_nav.js"),
    include_str!("ui/feat_priv.js"),
    include_str!("ui/feat_look.js"),
    include_str!("ui/feat_tools.js"),
    include_str!("ui/devtools.js"),
    include_str!("ui/boot.js"),
    "</script></body></html>"
);
const PAGE_JS: &str = include_str!("page.js");

/// Version (une seule source : Cargo.toml) et dépôt GitHub (vérification des mises à jour).
const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "shxmacrypto/Plume";

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

/// Données de profil hors registre ; tous les réglages viennent de src/settings.json (module settings).
fn defaults() -> Value {
    json!({
        "onboarded": false, "name": "", "mode": "full", "allow": [], "dark": [], "light": [], "never": [],
        "readerSize": 20, "readerTheme": "auto",
    })
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
    Tasks,
    Palette,
    TabSearch,
    Split,
    Zen,
    Fullscreen,
    Space(usize),
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
    Blocked(u32, String),
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
    Upd(Value),
    /// Mesure mémoire / CPU (badge des onglets, gestionnaire de tâches)
    RamTick,
    Procs(Vec<(u32, i32, Vec<u32>, Vec<String>)>),
    /// Branchement / débranchement du secteur
    Power,
    /// Suite d'un travail fait hors du fil de l'interface (Windows Hello, fuites, cookies…)
    Do(Box<dyn FnOnce(&mut App) + Send>),
    /// Une page entre ou sort du plein écran (vidéo)
    PageFs(u32, bool),
    /// Son qui démarre ou s'arrête dans un onglet (#33)
    Audio(u32, bool),
    /// Navigation vers un site bloqué pendant une session de focus (#34)
    FocusBlock(u32, String),
    /// Téléchargement en attente du choix du dossier (« Enregistrer sous »)
    DlAsk(u32),
    /// Lien passé par un autre lancement de Plume (lien cliqué dans une autre application) : (fenêtre, adresse)
    Remote(u32, String),
}

/// Fréquence du relevé mémoire (s) ; 0 = arrêté. Un seul fil, créé seulement quand on en a besoin.
static RAM_EVERY: AtomicU32 = AtomicU32::new(0);
static POLLER: AtomicBool = AtomicBool::new(false);

/// Téléchargement qui attend « Enregistrer sous » : arguments de l'événement, report, opération, entrée de la liste.
type DlWait = (ICoreWebView2DownloadStartingEventArgs, ICoreWebView2Deferral, ICoreWebView2DownloadOperation, Value);

thread_local! {
    /// Téléchargements en cours (objets COM, thread UI uniquement).
    static DLS: RefCell<HashMap<u32, ICoreWebView2DownloadOperation>> = RefCell::new(HashMap::new());
    static DL_WAIT: RefCell<HashMap<u32, DlWait>> = RefCell::new(HashMap::new());
}
pub static DL_ID: AtomicU32 = AtomicU32::new(0);
/// Demander où enregistrer chaque fichier (réglage dlAsk), lu par l'événement DownloadStarting.
static DL_ASK: AtomicBool = AtomicBool::new(true);

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
    /// formulaire commencé et pas envoyé (#3)
    dirty: bool,
    /// caméra ou micro en cours d'utilisation (#3)
    call: bool,
    /// mémoire du processus de rendu (#1), en octets
    ram: u64,
    /// dernière visite (ms), gardée dans la session (#8)
    lv: u64,
    /// conteneur (#19), 0 = aucun
    cont: u32,
    /// traqueurs bloqués sur la page : (domaine, nombre) (#21)
    bl: Vec<(String, u32)>,
    /// couleur du site (theme-color) pour la teinte adaptative (#26)
    tint: Option<String>,
    /// son (#33) : 0 aucun, 1 en lecture, 2 en pause après avoir joué
    audio: u8,
    muted: bool,
    /// coupé d'office à l'ouverture en arrière-plan (retrouve le son à la première visite)
    auto_muted: bool,
    /// vidéo en Picture-in-Picture (#32) : l'onglet ne doit pas être suspendu
    pip: bool,
    /// onglet ouvert par un lien d'un autre onglet (fermé s'il ne sert qu'à un téléchargement)
    opener: u32,
}

impl Tab {
    fn new(id: u32, url: String, title: String, fav: String) -> Self {
        Tab {
            id, view: None, url, title, fav, loading: false, back: false, fwd: false, last: Instant::now(),
            private: false, group: 0, blocked: 0, info: None, speaking: false, pinned: false,
            dirty: false, call: false, ram: 0, lv: now_ms(), cont: 0, bl: vec![], tint: None, audio: 0, muted: false, auto_muted: false, pip: false,
            opener: 0,
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
    x: WinX,
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
    /// état 0.7 propre à la fenêtre courante (espace, écran partagé, coup d'œil)
    x: WinX,
    /// espaces de travail (#7), archive (#8), sessions (#12), notes (#13 ; None = coffre verrouillé)
    spaces: Vec<Value>,
    archive: Vec<Value>,
    sessions: Vec<Value>,
    notes: Option<Value>,
    others: Vec<Win>,
    want_window: Vec<WinReq>,
    proxy: EventLoopProxy<Ev>,
    ui: WebView,
    env: ICoreWebView2Environment,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u32,
    closed: Vec<Value>,
    /// conteneur imposé au prochain onglet (#19)
    open_cont: Option<u32>,
    /// conteneurs dont le profil a reçu les extensions cette session
    cont_ext: std::collections::HashSet<u32>,
    /// sites visités (conteneur, domaine) à effacer à la fermeture (#20)
    visited: std::collections::HashSet<(u32, String)>,
    hello_busy: bool,
    /// image du jour (#27) : {date, credit, link}
    daily: Value,
    daily_busy: bool,
    daily_try: Option<Instant>,
    /// change quand l'image de la nouvelle page change (l'interface la recharge)
    ntp_v: u64,
    /// météo en cache (#28) : (ville|unité|langue, quand, données)
    wx: Option<(String, Instant, Value)>,
    wx_busy: bool,
    /// liste de tâches de la nouvelle page (#28), chargée à la première demande
    todo: Value,
    /// CSS / JS par site (#29) : {domaine: {css, js, on}}
    custom: Value,
    /// session de focus (#34) : {phase, end} ; génération du minuteur en cours
    focus: Value,
    focus_gen: u64,
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
    /// Dernier état de la mise à jour (rejoué aux fenêtres qui s'ouvrent)
    upd: Value,
    upd_busy: bool,
    /// la vérification automatique du démarrage a eu lieu
    upd_auto: bool,
    /// Réglages au démarrage : ceux marqués « rs » ne s'appliquent qu'après un redémarrage
    boot: Value,
    /// Statistiques d'économies (#6), stats.json
    stats: Value,
    stats_dirty: bool,
    /// Économie de batterie active (#2) / connexion limitée
    batt: bool,
    metered: bool,
    tasks_open: bool,
    cpu_prev: HashMap<u32, (u64, Instant)>,
    procs: Value,
    /// Plume ouvre les liens https (navigateur par défaut), relu quand une fenêtre reprend le focus
    is_default: bool,
    /// onglet d'origine du prochain onglet ouvert par un lien
    next_opener: u32,
    /// pages d'onglets fermés gardées (cachées) jusqu'à la fin de leurs téléchargements : (fenêtre, onglet, vue)
    dl_keep: Vec<(u32, u32, WebView)>,
    _ctx: WebContext,
}

fn main() {
    // Lien ou fichier ouvert depuis une autre application : tout ce qui suit --single-argument est l'adresse,
    // et aucune option n'est lue (un lien piégé ne peut pas déclencher --uninstall)
    let single = defbrowser::single_argument();
    let args: Vec<String> = if single.is_some() { vec![] } else { std::env::args().skip(1).collect() };
    let flag = |f: &str| args.iter().any(|a| a == f);
    // Lancé par PlumeSetup.exe ou par Paramètres › Applications : rien d'autre à faire
    if flag("--register") {
        std::process::exit(if setup::register(flag("--desktop")) { 0 } else { 1 });
    }
    if flag("--uninstall") {
        std::process::exit(setup::uninstall(flag("--quiet")));
    }
    let cli_url = single.or_else(|| args.iter().find(|a| !a.starts_with("--")).cloned()).filter(|u| !u.trim().is_empty());
    // un seul démarrage à la fois par profil : si un autre Plume démarre, on attend que sa fenêtre soit prête
    let start = defbrowser::start_lock();
    // Plume déjà ouvert sur ce profil : il ouvre le lien (ou passe au premier plan) et ce lancement s'arrête
    if !flag("--restarted") && defbrowser::hand_off(cli_url.as_deref().unwrap_or("")) {
        return;
    }
    setup::mark_running();
    if setup::installed() {
        std::thread::spawn(setup::cleanup);
    }
    if flag("--restarted") {
        std::thread::sleep(Duration::from_millis(1500)); // laisse l'ancienne instance libérer le profil
    }

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
    if saved["cfg"].is_null() {
        cfg["cfgV"] = json!(settings::SCHEMA); // premier lancement : rien à migrer
    }
    settings::migrate(&mut cfg);
    let eco = args.iter().any(|a| a == "--eco") || cfg["eco"].as_bool().unwrap_or(false);
    let ext_on = cfg["ext"].as_bool().unwrap_or(true);
    adblock::ON.store(cfg["ab"].as_bool().unwrap_or(true), Ordering::Relaxed);
    *adblock::ALLOW.write().unwrap() = strs(&cfg["allow"]);
    DL_ASK.store(cfg["dlAsk"] != false, Ordering::Relaxed);

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
    priv07::doh_prefs(&dir, &cfg); // DNS sécurisé (#25) : réglage du moteur, lu à son démarrage
    App::cont_gc(&dir, &cfg);

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
        x: WinX::default(),
        spaces: saved["spaces"].as_array().cloned().unwrap_or_default(),
        archive: tabs07::load_json("archive.json", json!([])).as_array().cloned().unwrap_or_default(),
        sessions: tabs07::load_json("sessions.json", json!([])).as_array().cloned().unwrap_or_default(),
        notes: None,
        others: vec![],
        want_window: vec![],
        proxy: proxy.clone(),
        ui,
        env,
        tabs: vec![],
        active: 0,
        next_id: 1,
        closed: vec![],
        open_cont: None,
        cont_ext: Default::default(),
        visited: Default::default(),
        hello_busy: false,
        daily: tabs07::load_json("ntp-daily.json", Value::Null),
        daily_busy: false,
        daily_try: None,
        ntp_v: now_ms(),
        wx: None,
        wx_busy: false,
        todo: Value::Null,
        custom: json!({}),
        focus: Value::Null,
        focus_gen: 0,
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
        upd: Value::Null,
        upd_busy: false,
        upd_auto: false,
        boot: cfg.clone(),
        stats: std::fs::read_to_string(dir.join("stats.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({"days": {}})),
        stats_dirty: false,
        batt: false,
        metered: false,
        tasks_open: false,
        cpu_prev: HashMap::new(),
        procs: json!([]),
        is_default: defbrowser::is_default(),
        next_opener: 0,
        dl_keep: vec![],
        cfg,
        _ctx: ctx,
    };
    app.apply_profile();
    let _ = app.ui.zoom(app.ui_scale());
    app.sync_lite();
    app.update_power();
    app.poller();
    app.load_notes();
    app.sync_keys();
    app.cc_load();
    app.daily_check();

    // Session précédente : onglets restaurés "endormis" (0 Mo) sauf l'actif ; ceux des autres espaces sont rangés
    app.x.space = if app.cfg["spaces"] == true { saved["space"].as_u64().unwrap_or(0) as u32 } else { 0 };
    if app.cfg["restore"].as_bool().unwrap_or(true) {
        for t in list("session") {
            let u = str_of(&t["u"]);
            if !u.is_empty() {
                let id = app.next();
                let mut tab = Tab::new(id, u, str_of(&t["t"]), str_of(&t["f"]));
                tab.group = t["g"].as_u64().unwrap_or(0) as u32;
                tab.pinned = t["pin"] == true;
                tab.lv = t["lv"].as_u64().unwrap_or_else(now_ms);
                let c = t["c"].as_u64().unwrap_or(0) as u32;
                tab.cont = if app.cont_ok(c) { c } else { 0 };
                let sp = if app.cfg["spaces"] == true { t["sp"].as_u64().unwrap_or(0) as u32 } else { 0 };
                if sp == app.x.space {
                    app.tabs.push(tab);
                } else {
                    match app.x.stash.iter_mut().find(|s| s.0 == sp) {
                        Some(s) => s.1.push(tab),
                        None => app.x.stash.push((sp, vec![tab], 0)),
                    }
                }
            }
        }
    }
    app.groups.retain(|g| app.tabs.iter().any(|t| t.group as u64 == g["id"].as_u64().unwrap_or(0)));
    let restored_active = saved["active"].as_u64().unwrap_or(0) as usize;
    match cli_url {
        Some(u) => {
            let u = app.open_arg(&u);
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

    // fenêtre prête et boucle sur le point de tourner : les lancements en attente peuvent lui passer leur lien
    drop(start);
    event_loop.run(move |event, target, flow| {
        *flow = ControlFlow::Wait; // ~0 % CPU au repos
        match event {
            Event::WindowEvent { window_id, event, .. } => {
                if app.focus_window_id(window_id) {
                    match event {
                        WindowEvent::CloseRequested => app.close_window(),
                        WindowEvent::Focused(true) => {
                            app.check_default(); // retour des Paramètres Windows : Plume est peut-être devenu le navigateur par défaut
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
            app.on_quit();
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
        swap(&mut self.x, &mut o.x);
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
            popup: None, sidep: None, sidep_ext: Value::Null, side_r: 0.0, x: WinX::default(),
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
                let c = t["c"].as_u64().unwrap_or(0) as u32;
                tab.cont = if self.cont_ok(c) { c } else { 0 };
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
            // les téléchargements en cours s'arrêteraient avec Plume : on demande d'abord
            if self.dl_count() > 0 && !self.confirm_quit() {
                return;
            }
            self.quit = true;
            return;
        }
        self.dev.close();
        // téléchargements lancés dans cette fenêtre (tous ses espaces) : leurs pages passent, cachées, dans la fenêtre suivante
        let busy: Vec<u32> = self.tabs.iter().chain(self.x.stash.iter().flat_map(|s| s.1.iter())).map(|t| t.id).filter(|id| self.dl_running(*id)).collect();
        let mut moving: Vec<(u32, WebView)> = vec![];
        for t in self.tabs.iter_mut().chain(self.x.stash.iter_mut().flat_map(|s| s.1.iter_mut())).filter(|t| busy.contains(&t.id)) {
            if let Some(v) = t.view.take() {
                moving.push((t.id, v));
            }
        }
        for t in self.tabs.drain(..) {
            if !t.url.is_empty() && !t.private {
                self.closed.push(json!({"u": t.url, "t": t.title, "f": t.fav}));
            }
        }
        let gone = self.wid;
        let mut o = self.others.pop().unwrap();
        self.swap_fields(&mut o);
        let (to, hwnd) = (self.wid, self.window.hwnd() as isize);
        for k in self.dl_keep.iter_mut().filter(|k| k.0 == gone) {
            let _ = k.2.reparent(hwnd);
            k.0 = to;
        }
        for (id, v) in moving {
            let _ = v.set_visible(false);
            let _ = v.reparent(hwnd);
            self.dl_keep.push((to, id, v));
        }
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

    /// Même chose dans toutes les fenêtres.
    fn js_all(&self, code: &str) {
        self.js(code);
        for w in self.others.iter().filter(|w| w.ui_ready) {
            let _ = w.ui.evaluate_script(code);
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
        let top = (self.top_h() * k) as u32;
        let (fs, zen) = (self.x.page_fs, self.x.zen);
        let left = if fs || zen { 0 } else { (self.side_w * k) as u32 };
        let bottom = if fs { 0 } else { (self.dev.height() * k) as u32 };
        let right = if fs { 0 } else { (self.side_r * k) as u32 };
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
        let full = !self.x.page_fs && (self.overlay || self.blank() || self.side_w > 0.0 || self.dev.height() > 0.0 || self.side_r > 0.0 || self.x.split.is_some());
        let s = self.window.inner_size();
        let h = if full { s.height } else { (self.top_h() * self.ui_scale() * self.scale()) as u32 };
        let _ = self.ui.set_bounds(Rect { position: PhysicalPosition::new(0, 0).into(), size: PhysicalSize::new(s.width, h).into() });
        let r = self.content_rect();
        match self.x.split {
            Some(sp) if !hide => {
                let (ra, rb) = self.split_rects(r, sp);
                for (id, rr) in [(sp.a, ra), (sp.b, rb)] {
                    if let Some(v) = self.idx(id).and_then(|i| self.tabs[i].view.as_ref()) {
                        let _ = v.set_bounds(rr);
                        let _ = v.set_visible(true);
                    }
                }
            }
            _ => {
                if let Some(v) = self.active_view() {
                    let _ = v.set_bounds(r);
                    let _ = v.set_visible(!hide);
                }
            }
        }
        if let Some(p) = &self.x.peek {
            let _ = p.set_bounds(self.peek_rect());
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
        // préchargement (#4) : coupé sur batterie ou connexion limitée si demandé
        let pf = if c["prefetch"] == true && !(c["prefetchSave"] == true && (self.batt || self.metered)) { c["prefetchMode"].clone() } else { json!("off") };
        let lite_auto: Vec<String> = match adblock::LITE.read().ok().and_then(|l| l.clone()) {
            Some((d, m)) => {
                let mut v: Vec<String> = m.iter().filter(|(_, f)| *f & adblock::LITE_AUTO != 0).map(|(h, _)| h.clone()).collect();
                if d & adblock::LITE_AUTO != 0 {
                    v.push("*".into());
                    v.extend(m.iter().filter(|(_, f)| *f & adblock::LITE_AUTO == 0).map(|(h, _)| format!("-{h}")));
                }
                v
            }
            None => vec![],
        };
        let (cc, ccjs) = self.cc_page_cfg();
        let o = json!({"ab": c["ab"], "allow": c["allow"], "tint": c["adaptive"] == true, "cc": cc, "ccjs": ccjs, "dark": c["dark"], "light": c["light"], "darkAll": c["darkAll"],
            "pf": pf, "pfd": c["prefetchDelay"], "pwo": c["pwgen"] == true && c["pwOffer"] == true, "form": c["sleepWl"] == true && c["sleepForms"] == true,
            "call": c["sleepWl"] == true && c["sleepCalls"] == true, "noAuto": lite_auto,
            "peek": if c["peek"] == true { c["peekTrigger"].clone() } else { json!("") }, "peekSites": c["peekSites"],
            "gest": if c["gestures"] == true { json!({"trail": c["gestTrail"], "sens": c["gestSens"]}) } else { Value::Null },
            "navSide": c["navBtns"] == true && c["navSide"] == true});
        format!("window.__plumeCfg={o};window.__plume&&window.__plume.apply();")
    }

    fn build_view(&mut self, i: usize) {
        let id = self.tabs[i].id;
        let private = self.tabs[i].private;
        let url = self.tabs[i].url.clone();
        // article enregistré (#38) : servi depuis le disque par un nom d'hôte virtuel, posé avant la navigation
        let rl = url.starts_with("https://reading.plume/");
        let cont = if private { 0 } else { self.tabs[i].cont };
        let (p1, p2, p3, p4) = (self.proxy.clone(), self.proxy.clone(), self.proxy.clone(), self.proxy.clone());
        let mut b = WebViewBuilder::new().with_environment(self.env.clone()).with_incognito(private);
        if cont > 0 {
            b = b.with_profile_name(format!("plume-c{cont}")); // conteneur : cookies et stockage à part (#19)
        }
        let built = b
            .with_back_forward_navigation_gestures(self.cfg["navBtns"] == true && self.cfg["navSwipe"] == true)
            .with_url(if rl { "about:blank" } else { url.as_str() })
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
                App::hook_audio(&v, id, &self.proxy);
                if rl && let Ok(w3) = v.webview().cast::<ICoreWebView2_3>() {
                    let _ = w3.SetVirtualHostNameToFolderMapping(
                        w!("reading.plume"),
                        &HSTRING::from(App::rl_dir().as_path()),
                        COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS,
                    );
                }
            }
            let z = self.cfg["pageZoom"].as_f64().unwrap_or(1.0);
            if z != 1.0 {
                let _ = v.zoom(z);
            }
            if cont > 0 {
                self.cont_exts(cont, &v);
            }
            if rl {
                let _ = v.load_url(&url);
            }
            self.tabs[i].view = Some(v);
        }
    }

    /// Onglet protégé de la veille (#3) : site en liste blanche, formulaire en cours, appel en cours.
    fn protected(&self, t: &Tab) -> bool {
        let c = &self.cfg;
        c["sleepWl"] == true
            && (strs(&c["sleepList"]).iter().any(|h| *h == import::host_of(&t.url))
                || (c["sleepForms"] == true && t.dirty)
                || (c["sleepCalls"] == true && t.call))
    }

    /// Téléchargement en cours lancé depuis cet onglet : sa page doit rester ouverte, sinon on ne le suit plus.
    fn dl_running(&self, tab: u32) -> bool {
        self.downloads.iter().any(|d| d["tab"].as_u64() == Some(tab as u64) && d["state"] == "run")
    }

    fn dl_count(&self) -> usize {
        self.downloads.iter().filter(|d| d["state"] == "run").count()
    }

    /// Page d'un onglet qui disparaît : libérée tout de suite (processus de rendu), sauf si un téléchargement
    /// en cours en dépend : elle reste alors cachée et muette jusqu'à la fin, sinon on ne le suivrait plus.
    fn release_view(&mut self, id: u32, v: Option<WebView>) {
        match v {
            Some(v) if self.dl_running(id) => {
                let _ = v.set_visible(false);
                unsafe {
                    if let Ok(c8) = v.webview().cast::<ICoreWebView2_8>() {
                        let _ = c8.SetIsMuted(true);
                    }
                }
                self.dl_keep.push((self.wid, id, v));
            }
            v => drop(v),
        }
    }

    /// Retire de la fenêtre les onglets choisis, en passant leurs pages par release_view.
    fn drop_tabs(&mut self, gone: impl Fn(&Tab) -> bool) {
        let mut i = 0;
        while i < self.tabs.len() {
            if gone(&self.tabs[i]) {
                let t = self.tabs.remove(i);
                self.release_view(t.id, t.view);
            } else {
                i += 1;
            }
        }
    }

    /// Fermeture de Plume pendant un téléchargement : Oui ferme quand même.
    fn confirm_quit(&self) -> bool {
        let n = self.dl_count();
        let msg = match (self.lang() == "fr", n) {
            (true, 1) => "Un téléchargement est en cours. Il s'arrêtera si vous fermez Plume.\n\nFermer Plume quand même ?".to_string(),
            (true, _) => format!("{n} téléchargements sont en cours. Ils s'arrêteront si vous fermez Plume.\n\nFermer Plume quand même ?"),
            (false, 1) => "A download is in progress. It will stop if you close Plume.\n\nClose Plume anyway?".to_string(),
            (false, _) => format!("{n} downloads are in progress. They will stop if you close Plume.\n\nClose Plume anyway?"),
        };
        unsafe { MessageBoxW(Some(HWND(self.window.hwnd() as _)), &HSTRING::from(msg), w!("Plume"), MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2) == IDYES }
    }

    /// Onglet en arrière-plan : caché + suspendu (scripts et rendu arrêtés) + mémoire minimale.
    fn sleep(&mut self, i: usize) {
        let keep = self.protected(&self.tabs[i]) || self.tabs[i].pip || self.dl_running(self.tabs[i].id);
        let t = &mut self.tabs[i];
        t.last = Instant::now();
        if let Some(v) = &t.view {
            let _ = v.set_visible(false);
            if keep {
                return; // liste blanche : caché mais jamais suspendu
            }
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
        if i != self.active {
            self.leave_page_fs();
        }
        let id_i = self.tabs[i].id;
        if let Some(sp) = self.x.split.filter(|sp| sp.a != id_i && sp.b != id_i) {
            let _ = sp;
            self.unsplit();
        }
        let in_split = self.x.split.is_some();
        if i != self.active && self.active < self.tabs.len() {
            self.auto_pip(self.active); // la vidéo continue en PiP (#32)
        }
        if i != self.active && self.active < self.tabs.len() && !in_split {
            self.sleep(self.active);
        }
        self.active = i;
        self.pip_back(i);
        if self.tabs[i].auto_muted {
            self.set_mute(i, false);
        }
        self.overlay = false;
        if self.tabs[i].view.is_none() && !self.tabs[i].url.is_empty() {
            self.build_view(i);
        }
        let t = &mut self.tabs[i];
        t.last = Instant::now();
        t.lv = now_ms();
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
        t.opener = std::mem::take(&mut self.next_opener);
        t.cont = if private { 0 } else { self.cont_for(url.as_deref()) };
        // un onglet ouvert depuis un groupe reste dans le groupe
        t.group = self.tabs.get(self.active).map_or(0, |a| if url.is_some() { a.group } else { 0 });
        self.tabs.insert(pos, t);
        if !fg && url.is_some() {
            self.build_view(pos);
            self.bg_mute(pos);
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
        let cid = self.tabs[i].id;
        if self.x.split.is_some_and(|sp| sp.a == cid || sp.b == cid) {
            self.unsplit();
        }
        let t = self.tabs.remove(i);
        self.clear_if_last(&t);
        if !t.url.is_empty() && !t.private {
            self.closed.push(json!({"u": t.url, "t": t.title, "f": t.fav}));
            if self.closed.len() > 25 {
                self.closed.remove(0);
            }
        }
        if self.dev.target == t.id {
            self.dev_close();
        }
        self.release_view(t.id, t.view);
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
        // site toujours ouvert dans un conteneur (#19)
        if let Some(c) = self.cont_site(&url).filter(|c| *c != self.tabs[i].cont && !self.tabs[i].private) {
            if self.tabs[i].view.is_some() {
                self.open_cont = Some(c);
                return self.new_tab(Some(url), true, false);
            }
            self.tabs[i].cont = c;
        }
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
        self.update_power();
        // sur batterie (#2), les onglets s'endorment plus vite
        let mins = if self.batt { self.cfg["battSleep"].as_u64().unwrap_or(2) } else { self.cfg["sleep"].as_u64().unwrap_or(10) };
        let mut changed = false;
        if mins > 0 {
            let limit = Duration::from_secs(mins * 60);
            let dev = self.dev.target;
            let audio_ok = self.cfg["sleepWl"] != true || self.cfg["sleepAudio"] != false;
            let mut freed = 0u64;
            for i in 0..self.tabs.len() {
                let skip = {
                    let t = &self.tabs[i];
                    i == self.active || t.view.is_none() || t.last.elapsed() < limit || t.private || t.id == dev || self.protected(t) || t.pip
                        || self.x.split.is_some_and(|sp| sp.a == t.id || sp.b == t.id) || self.dl_running(t.id)
                };
                if skip {
                    continue;
                }
                let t = &mut self.tabs[i];
                let playing = audio_ok
                    && t.view.as_ref().is_some_and(|v| unsafe {
                        let mut b = BOOL(0);
                        v.webview().cast::<ICoreWebView2_8>().and_then(|c| c.IsDocumentPlayingAudio(&mut b)).is_ok() && b.as_bool()
                    });
                if !playing {
                    // mémoire libérée (#6) : la dernière mesure, sinon une estimation prudente
                    freed += if t.ram > 0 { t.ram } else { 60 << 20 };
                    t.view = None;
                    t.loading = false;
                    t.ram = 0;
                    changed = true;
                }
            }
            if freed > 0 {
                self.stat_add("f", (freed >> 20) as f64);
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
        if self.cfg["stats"] == true && self.cfg["statsHome"] == true {
            self.push_stats();
        }
        self.auto_close();
        self.save();
    }

    /* ---------- Performance (0.7) ---------- */

    /// Relevé mémoire / CPU : seulement quand le badge des onglets ou le gestionnaire de tâches en a besoin.
    fn poller(&self) {
        let every = if self.tasks_open { 2 } else if self.cfg["ram"] == true { self.cfg["ramEvery"].as_u64().unwrap_or(5).clamp(1, 60) as u32 } else { 0 };
        RAM_EVERY.store(every, Ordering::Relaxed);
        if every > 0 && !POLLER.swap(true, Ordering::Relaxed) {
            let p = self.proxy.clone();
            std::thread::spawn(move || {
                loop {
                    // d'abord attendre : WebView2 plante si on demande ses processus pendant son démarrage
                    std::thread::sleep(Duration::from_secs(RAM_EVERY.load(Ordering::Relaxed).max(2) as u64));
                    let n = RAM_EVERY.load(Ordering::Relaxed);
                    if n == 0 || p.send_event(Ev::RamTick).is_err() {
                        break;
                    }
                }
                POLLER.store(false, Ordering::Relaxed);
            });
        }
    }

    /// Processus WebView2 (navigateur, rendu, GPU…) avec les cadres qu'ils affichent.
    fn poll_procs(&self) {
        if !self.ui_ready {
            return;
        }
        let Ok(e13) = self.env.cast::<ICoreWebView2Environment13>() else { return };
        let p = self.proxy.clone();
        unsafe {
            let _ = e13.GetProcessExtendedInfos(&GetProcessExtendedInfosCompletedHandler::create(Box::new(move |r, list| {
                r?;
                let Some(list) = list else { return Ok(()) };
                let mut n = 0u32;
                list.Count(&mut n)?;
                let mut out = vec![];
                for i in 0..n {
                    let x = list.GetValueAtIndex(i)?;
                    let info = x.ProcessInfo()?;
                    let (mut pid, mut kind) = (0i32, COREWEBVIEW2_PROCESS_KIND::default());
                    info.ProcessId(&mut pid)?;
                    info.Kind(&mut kind)?;
                    let (mut frames, mut srcs) = (vec![], vec![]);
                    // seuls les processus de rendu ont des cadres (les autres font planter WebView2)
                    if kind == COREWEBVIEW2_PROCESS_KIND_RENDERER && let Ok(coll) = x.AssociatedFrameInfos() {
                        let it = coll.GetIterator()?;
                        let mut has = BOOL(0);
                        it.HasCurrent(&mut has)?;
                        while has.as_bool() {
                            let f = it.GetCurrent()?;
                            let f = std::mem::ManuallyDrop::new(f);
                            if let Ok(f2) = f.cast::<ICoreWebView2FrameInfo2>() {
                                let mut id = 0u32;
                                f2.FrameId(&mut id)?;
                                frames.push(id);
                            }
                            let mut src = PWSTR::null();
                            f.Source(&mut src)?;
                            srcs.push(webview2_com::take_pwstr(src));
                            it.MoveNext(&mut has)?;
                        }
                        std::mem::forget(it);
                        std::mem::forget(coll);
                    }
                    out.push((pid as u32, kind.0, frames, srcs));
                }
                let _ = p.send_event(Ev::Procs(out));
                Ok(())
            })));
        }
    }

    fn on_procs(&mut self, list: Vec<(u32, i32, Vec<u32>, Vec<String>)>) {
        let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
        let now = Instant::now();
        // cadre principal de chaque onglet (toutes les fenêtres) → onglet
        let mut by_frame: HashMap<u32, (u32, String)> = HashMap::new();
        for t in self.tabs.iter().chain(self.others.iter().flat_map(|w| w.tabs.iter())) {
            if let Some(fid) = t.view.as_ref().and_then(main_frame) {
                by_frame.insert(fid, (t.id, if t.title.is_empty() { t.url.clone() } else { t.title.clone() }));
            }
        }
        let mut per_tab: HashMap<u32, (u64, usize)> = HashMap::new();
        let mut procs = vec![];
        let mut seen = vec![];
        for (pid, kind, frames, srcs) in list {
            let (mem, cpu) = perf::stat(pid).unwrap_or((0, 0));
            let pct = match self.cpu_prev.get(&pid) {
                Some((c0, t0)) => {
                    let dt = now.duration_since(*t0).as_nanos() as f64 / 100.0;
                    if dt > 0.0 { (cpu.saturating_sub(*c0) as f64 / dt / ncpu * 100.0).min(100.0) } else { 0.0 }
                }
                None => 0.0,
            };
            self.cpu_prev.insert(pid, (cpu, now));
            seen.push(pid);
            let mut tabs: Vec<u32> = vec![];
            let mut titles: Vec<String> = vec![];
            for f in &frames {
                if let Some((id, title)) = by_frame.get(f).filter(|(id, _)| !tabs.contains(id)) {
                    tabs.push(*id);
                    titles.push(title.clone());
                }
            }
            let ui = srcs.iter().any(|s| s.starts_with("http://plume.ui") || s.starts_with("plume:"));
            let exts: Vec<Value> = srcs
                .iter()
                .filter_map(|s| s.strip_prefix("chrome-extension://").and_then(|r| r.split('/').next()))
                .map(|id| self.exts.iter().find(|e| e["id"] == id).map(|e| e["name"].clone()).unwrap_or(json!(id)))
                .collect();
            for id in &tabs {
                per_tab.insert(*id, (mem, tabs.len()));
            }
            let kind = match COREWEBVIEW2_PROCESS_KIND(kind) {
                COREWEBVIEW2_PROCESS_KIND_BROWSER => "browser",
                COREWEBVIEW2_PROCESS_KIND_RENDERER => "renderer",
                COREWEBVIEW2_PROCESS_KIND_GPU => "gpu",
                COREWEBVIEW2_PROCESS_KIND_UTILITY => "utility",
                _ => "other",
            };
            procs.push(json!({"pid": pid, "kind": kind, "mem": mem, "cpu": (pct * 10.0).round() / 10.0, "tabs": tabs, "titles": titles, "ext": exts, "ui": ui}));
        }
        self.cpu_prev.retain(|p, _| seen.contains(p));
        for t in self.tabs.iter_mut() {
            t.ram = per_tab.get(&t.id).map_or(0, |x| x.0);
        }
        for w in self.others.iter_mut() {
            for t in w.tabs.iter_mut() {
                t.ram = per_tab.get(&t.id).map_or(0, |x| x.0);
            }
        }
        // Plume lui-même (l'hôte des WebView2)
        let me = std::process::id();
        if let Some((mem, _)) = perf::stat(me) {
            procs.insert(0, json!({"pid": me, "kind": "plume", "mem": mem, "cpu": 0, "tabs": [], "titles": [], "ext": []}));
        }
        self.procs = json!(procs);
        let tabs: serde_json::Map<String, Value> = per_tab.iter().map(|(id, (m, n))| (id.to_string(), json!([m, *n > 1]))).collect();
        self.js_all(&format!("RAM({})", json!({"tabs": tabs, "procs": self.procs})));
    }

    /// Économie de batterie (#2) et connexion limitée (préchargement, #4).
    fn update_power(&mut self) {
        let c = &self.cfg;
        let pw = perf::power();
        let on = c["batt"] == true
            && pw.on_battery
            && (c["battWhen"] != "below" || (pw.percent as u64) <= c["battPct"].as_u64().unwrap_or(20));
        let metered = c["prefetch"] == true && c["prefetchSave"] == true && perf::metered();
        if on != self.batt || metered != self.metered {
            self.batt = on;
            self.metered = metered;
            self.push_page_cfg();
            self.push();
        }
    }

    /// Règles Lite (#5) pour le filtre des requêtes.
    fn sync_lite(&self) {
        let c = &self.cfg;
        let mut l = adblock::LITE.write().unwrap();
        if c["lite"] != true {
            *l = None;
            return;
        }
        let bit = |k: &str, b: u8| if c[k] == true { b } else { 0 };
        let def = bit("liteJs", adblock::LITE_JS) | bit("liteImg", adblock::LITE_IMG) | bit("liteFont", adblock::LITE_FONT) | bit("liteAutoplay", adblock::LITE_AUTO);
        let sites = c["liteSites"].as_object().map(|m| m.iter().map(|(h, f)| (h.clone(), f.as_u64().unwrap_or(0) as u8)).collect()).unwrap_or_default();
        *l = Some((def, sites));
    }

    fn lite_set(&mut self, bit: u8, on: bool) {
        let host = import::host_of(&self.tabs[self.active].url);
        if host.is_empty() {
            return;
        }
        let cur = adblock::lite(&host);
        let next = if on { cur | bit } else { cur & !bit };
        let mut sites = self.cfg["liteSites"].as_object().cloned().unwrap_or_default();
        sites.insert(host, json!(next));
        self.cfg["liteSites"] = Value::Object(sites);
        self.sync_lite();
        self.push_page_cfg();
        if let Some(v) = self.active_view() {
            unsafe {
                if let Ok(st) = v.webview().Settings() {
                    let _ = st.SetIsScriptEnabled(next & adblock::LITE_JS == 0);
                }
            }
            let _ = v.reload();
        }
        self.save();
        self.push();
    }

    /// Statistiques (#6) : compteurs par jour (heure locale), jamais envoyés.
    fn stat_add(&mut self, k: &str, n: f64) {
        if self.cfg["stats"] != true {
            return;
        }
        let day = today();
        let e = &mut self.stats["days"][&day];
        e[k] = json!(e[k].as_f64().unwrap_or(0.0) + n);
        self.stats_dirty = true;
    }

    fn push_stats(&self) {
        self.js_all(&format!("STATS({})", self.stats));
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
                       "s": t.view.is_none() && !t.url.is_empty(), "ram": t.ram,
                       "ns": self.cfg["sleepWl"] == true && strs(&self.cfg["sleepList"]).contains(&import::host_of(&t.url)),
                       "nt": self.has_note(&t.url), "c": t.cont, "au": t.audio, "mu": t.muted, "pip": t.pip})
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
            "vault": {"exists": self.vault.exists(), "unlocked": self.vault.unlocked(), "n": self.vault.items.len(), "hello": self.hello_ready()},
            "dl": self.downloads.iter().filter(|d| d["state"] == "run").count(), "dlAny": !self.downloads.is_empty(),
            "exts": exts, "extOn": self.ext_on, "restart": self.needs_restart(), "adlist": adblock::count(),
            "dev": self.dev.height() > 0.0, "lang": self.lang(), "version": VERSION, "repo": REPO, "installed": setup::installed(), "isDefault": self.is_default, "batt": self.batt,
            "spaces": self.spaces_json(), "space": self.x.space, "notesLocked": self.notes.is_none(),
            "split": self.x.split.map(|sp| json!({"a": sp.a, "b": sp.b, "r": sp.ratio})),
            "tint": if self.cfg["adaptive"] == true { json!(t.tint) } else { Value::Null }, "zen": self.x.zen, "pageFs": self.x.page_fs,
            "fs": self.window.fullscreen().is_some(), "daily": if self.cfg["ntpBg"] == "daily" { self.daily.clone() } else { Value::Null },
            "ntpV": self.ntp_v, "ccList": self.cc_list(), "focus": self.focus, "peek": self.x.peek_url, "lite": adblock::lite(&host), "liteSite": self.cfg["liteSites"].get(&host).is_some(), "sidep": self.sidep_ext, "sideR": self.side_r, "popup": self.popup.is_some(),
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

    /// « Enregistrer sous » avant un téléchargement (réglage dlAsk) : dossier et nom au choix, ou annulation.
    /// La boîte s'ouvre ici, dans la boucle, et non pendant l'événement de WebView2 (qui attend grâce au report).
    fn dl_ask(&mut self, did: u32) {
        let Some((args, wait, op, mut start)) = DL_WAIT.with(|m| m.borrow_mut().remove(&did)) else { return };
        let path = PathBuf::from(str_of(&start["path"]));
        let name = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| "download".into());
        // dernier dossier choisi, sinon celui de Windows (Téléchargements)
        let dir = self.cfg["dlLastDir"].as_str().map(PathBuf::from).filter(|d| d.is_dir()).or_else(|| path.parent().map(PathBuf::from)).unwrap_or_default();
        let ext = std::path::Path::new(&name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let all = if self.lang() == "fr" { "Tous les fichiers" } else { "All files" };
        let filter = if ext.is_empty() { format!("{all} (*.*)\0*.*\0\0") } else { format!("{} (*.{ext})\0*.{ext}\0{all} (*.*)\0*.*\0\0", ext.to_uppercase()) };
        let chosen = tools07::save_dialog(HWND(self.window.hwnd() as _), &dir.join(&name).to_string_lossy(), &filter, &ext);
        let origin = json!({"tab": start["tab"], "url": start["url"]});
        let cancelled = chosen.is_none();
        // InPrivate : le dossier choisi n'est pas retenu
        let private = start["tab"].as_u64().and_then(|t| self.idx(t as u32)).is_some_and(|i| self.tabs[i].private);
        unsafe {
            match chosen {
                Some(f) => {
                    let _ = args.SetResultFilePath(&HSTRING::from(f.as_path()));
                    if let Some(d) = f.parent().filter(|_| !private) {
                        self.cfg["dlLastDir"] = json!(d.to_string_lossy());
                    }
                    let fs = f.to_string_lossy().into_owned();
                    start["name"] = json!(dl_name(&fs));
                    start["path"] = json!(fs);
                    let _ = track_dl(&op, did, &self.proxy, start);
                }
                None => {
                    let _ = args.SetCancel(true);
                }
            }
            let _ = wait.Complete();
        }
        // annulé : l'onglet vide ouvert par le lien n'a plus de raison d'être
        if cancelled {
            self.dl_tab_cleanup(&origin);
        }
    }

    /// Lien ouvert dans un nouvel onglet qui n'était qu'un téléchargement : l'onglet vide se ferme et on revient
    /// à l'onglet d'origine, comme dans Chrome (sa page reste ouverte, cachée, jusqu'à la fin du téléchargement).
    fn dl_tab_cleanup(&mut self, s: &Value) {
        let Some(i) = s["tab"].as_u64().and_then(|t| self.idx(t as u32)) else { return };
        let t = &self.tabs[i];
        let empty = t.opener != 0 && !t.back && (t.url == str_of(&s["url"]) || t.title.is_empty() || t.title == t.url);
        if !empty || self.tabs.len() < 2 {
            return;
        }
        let (id, opener, url) = (t.id, t.opener, t.url.clone());
        if i != self.active {
            // ouvert en arrière-plan (clic du milieu) : il disparaît sans toucher à l'onglet affiché
            if self.x.split.is_some_and(|sp| sp.a == id || sp.b == id) {
                self.unsplit();
            }
            let t = self.tabs.remove(i);
            if i < self.active {
                self.active -= 1;
            }
            self.release_view(t.id, t.view);
            self.gc_groups();
            self.push();
            return;
        }
        self.close_tab(i);
        // pas dans « Rouvrir l'onglet fermé » : ce n'était qu'un lien de téléchargement
        if self.closed.last().is_some_and(|c| c["u"] == url.as_str()) {
            self.closed.pop();
        }
        if let Some(k) = self.idx(opener) {
            self.activate(k);
        }
    }

    /// Argument de lancement → adresse : un fichier existant (page .html ouverte depuis l'Explorateur) devient file:///.
    fn open_arg(&self, s: &str) -> String {
        let p = std::path::Path::new(s);
        if p.is_absolute() && p.is_file() {
            let mut u = String::from("file:///");
            for c in p.to_string_lossy().chars() {
                match c {
                    '\\' => u.push('/'),
                    ' ' => u.push_str("%20"),
                    '%' => u.push_str("%25"),
                    '#' => u.push_str("%23"),
                    '?' => u.push_str("%3F"),
                    c => u.push(c),
                }
            }
            return u;
        }
        self.to_url(s)
    }

    /// Lien reçu d'un autre lancement de Plume (lien cliqué dans une autre application) : nouvel onglet au premier plan.
    fn remote_open(&mut self, u: &str) {
        let url = self.open_arg(u.trim());
        // jamais dans une fenêtre InPrivate : une fenêtre normale déjà ouverte le reçoit (sinon une nouvelle, plus bas)
        if !url.is_empty() && self.private_win && let Some(i) = self.others.iter().position(|w| !w.private_win) {
            self.bring(i);
        }
        self.window.set_minimized(false);
        self.window.set_visible(true);
        self.window.set_focus();
        unsafe {
            let _ = SetForegroundWindow(HWND(self.window.hwnd() as _));
        }
        if url.is_empty() {
            return; // Plume relancé sans lien : la fenêtre passe simplement devant
        }
        if self.private_win {
            self.want_window.push(WinReq { url: Some(url), private: false, tab: None, restore: vec![], active: 0 });
        } else {
            // un panneau ouvert (réglages, menu) se referme : la page du lien s'affiche dessous
            self.js("mode && setMode(null)");
            self.open_cont = self.cont_site(&url);
            self.new_tab(Some(url), true, false);
        }
    }

    /// Relu quand une fenêtre reprend le focus (retour des Paramètres Windows).
    fn check_default(&mut self) {
        let d = defbrowser::is_default();
        if d != self.is_default {
            self.is_default = d;
            if d {
                self.toast("def.done");
            }
        }
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
        let forced_eco = std::env::args().any(|a| a == "--eco");
        settings::items().iter().any(|i| i.rs && self.cfg[&i.k] != self.boot[&i.k] && !(i.k == "eco" && forced_eco))
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
        let win = |tabs: &[Tab], a: usize, x: &WinX| {
            let keep = |t: &&Tab| !t.url.is_empty() && !t.private;
            let one = |t: &Tab, sp: u32| json!({"u": t.url, "t": t.title, "f": t.fav, "g": t.group, "pin": t.pinned, "lv": t.lv, "sp": sp, "c": t.cont});
            let mut s: Vec<Value> = tabs.iter().filter(keep).map(|t| one(t, x.space)).collect();
            for (sp, st, _) in &x.stash {
                s.extend(st.iter().filter(keep).map(|t| one(t, *sp)));
            }
            json!({"session": s, "active": tabs[..a.min(tabs.len())].iter().filter(keep).count(), "space": x.space})
        };
        let mut wins: Vec<Value> = std::iter::once(win(&self.tabs, self.active, &self.x))
            .chain(self.others.iter().map(|w| win(&w.tabs, w.active, &w.x)))
            .filter(|w| w["session"].as_array().is_some_and(|s| !s.is_empty()))
            .collect();
        let first = if wins.is_empty() { json!({"session": [], "active": 0}) } else { wins.remove(0) };
        let data = json!({"bookmarks": self.bookmarks, "history": self.history, "session": first["session"], "windows": wins,
                          "groups": self.groups, "active": first["active"], "space": first["space"], "cfg": self.cfg, "pwhosts": self.pw_hosts,
                          "spaces": self.spaces});
        let tmp = f.with_extension("tmp");
        if std::fs::write(&tmp, data.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, f);
        }
        if self.stats_dirty {
            let _ = std::fs::write(f.with_file_name("stats.json"), self.stats.to_string());
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
            let mut b = json!({"u": t.url, "t": title, "f": t.fav});
            // favoris séparés par espace (#7)
            if self.cfg["spaces"] == true && self.cfg["spacesBm"] == "separate" {
                b["sp"] = json!(self.x.space);
            }
            self.bookmarks.push(b);
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
                if let Some(u) = self.closed.pop().map(|c| str_of(&c["u"])) {
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
            Cmd::Tasks => {
                let _ = self.ui.focus();
                self.js("openPage('tasks')");
            }
            Cmd::Palette => {
                let _ = self.ui.focus();
                self.js("openPalette()");
            }
            Cmd::TabSearch => {
                let _ = self.ui.focus();
                self.js("openTabSearch()");
            }
            Cmd::Split => self.split_toggle(),
            Cmd::Zen => self.toggle_zen(),
            Cmd::Fullscreen => self.toggle_fullscreen(),
            Cmd::Space(n) => {
                if self.cfg["spaces"] == true {
                    self.space_nth(n)
                }
            }
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
        self.cfg_effect(k);
        if matches!(k, "ab" | "allow" | "dark" | "light" | "darkAll") {
            self.push_page_cfg();
        }
        self.save();
        self.push();
    }

    /// Effet immédiat d'un réglage (au-delà de l'interface, qui lit la config à chaque état).
    fn cfg_effect(&mut self, k: &str) {
        if matches!(k, "palette" | "palKey" | "tabSearch" | "tsKey" | "split" | "splitKey" | "zen" | "zenKey") {
            self.sync_keys();
        }
        if k == "split" && self.cfg["split"] != true {
            self.unsplit();
        }
        if k == "notesScope" || k == "notes" {
            self.push();
        }
        if k.starts_with("ram") || k == "ram" {
            self.poller();
        }
        if k.starts_with("lite") {
            self.sync_lite();
            self.push_page_cfg();
        }
        if k.starts_with("prefetch") || k.starts_with("sleep") || k.starts_with("peek") || k.starts_with("gest") || k.starts_with("nav") || k.starts_with("pw")
            || matches!(k, "adaptive" | "customCss" | "ccMode")
        {
            self.push_page_cfg();
        }
        if k.starts_with("batt") {
            self.update_power();
        }
        match k {
            "zen" if self.cfg["zen"] == "off" || (self.cfg["zen"] == "fullscreen" && self.window.fullscreen().is_none()) => {
                self.x.zen = false;
                self.relayout();
            }
            "ntpBg" => self.daily_check(),
            "focus" if self.cfg["focus"] != true => self.focus_stop(),
            "focusSites" => {
                if let Ok(mut f) = tools07::FOCUS.write() {
                    if let Some(f) = f.as_mut() {
                        f.sites = strs(&self.cfg["focusSites"]);
                    }
                }
            }
            "hello" => self.hello_toggle(),
            "dlAsk" => DL_ASK.store(self.cfg["dlAsk"] != false, Ordering::Relaxed),
            "containers" | "contList" | "privReport" | "prCount" => self.push(),
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
    }

    /// Après un changement de mode ou une réinitialisation : réapplique tout.
    fn cfg_effect_all(&mut self) {
        for k in ["ab", "allow", "tracking", "uiScale", "pageZoom", "ram", "lite", "batt", "palette", "zen", "ntpBg", "dlAsk"] {
            self.cfg_effect(k);
        }
        self.push_page_cfg();
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
        // un téléchargement en cours s'arrêterait : on demande d'abord
        if self.dl_count() > 0 && !self.confirm_quit() {
            return;
        }
        self.save();
        // version installée : toujours le plume.exe du dossier (une mise à jour a pu le remplacer)
        if let Ok(exe) = if setup::installed() { Ok(setup::exe()) } else { std::env::current_exe() } {
            // sans le lien d'un lancement « --single-argument » (la session rouvre les onglets)
            let mut a: Vec<String> =
                std::env::args().skip(1).take_while(|a| a != "--single-argument").filter(|a| a != "--restarted" && a != "--eco").collect();
            a.push("--restarted".into());
            if std::process::Command::new(exe).args(a).spawn().is_ok() {
                self.quit = true;
            }
        }
    }

    /* ---------- Mises à jour (GitHub Releases) ---------- */

    /// Version installée = dernière version publiée ? Vérifié à chaque démarrage (une fois, quel que soit le
    /// nombre de fenêtres) et sur demande, par la redirection de releases/latest : pas d'appel à l'API.
    fn update_check(&mut self, auto: bool) {
        if auto && std::mem::replace(&mut self.upd_auto, true) {
            return; // le résultat de la première vérification arrive dans toutes les fenêtres
        }
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let v = match update::latest_tag() {
                Ok((tag, page)) if update::newer(&tag, VERSION) => json!({"state": "available", "tag": tag, "page": page}),
                Ok((tag, page)) => json!({"state": "latest", "tag": tag, "page": page}),
                Err(e) => json!({"state": "error", "err": e}),
            };
            let _ = p.send_event(Ev::Upd(v));
        });
    }

    /// Télécharge PlumeSetup.exe, vérifie son SHA-256 et l'installe ; il ne reste qu'à redémarrer.
    fn update_install(&mut self) {
        if self.upd_busy || !setup::installed() {
            return;
        }
        self.upd_busy = true;
        let p = self.proxy.clone();
        let dir = setup::data_dir().join("updates");
        std::thread::spawn(move || {
            let send = |v: Value| {
                let _ = p.send_event(Ev::Upd(v));
            };
            let res = (|| -> Res<String> {
                // fichier et empreinte via l'API ; « Réessayer » reprend sa réponse récente au lieu de la redemander
                let r = update::latest_cached()?;
                if !update::newer(&r.tag, VERSION) {
                    return Err("Plume is already up to date".into());
                }
                let a = r.asset.ok_or("this release has no installer")?;
                send(json!({"state": "downloading", "tag": r.tag, "pct": 0, "page": r.page}));
                // recv : les données arrivent (avant, l'interface affiche « Connexion à GitHub… »)
                let file = update::download(&a, &r.tag, &dir, &mut |pct| {
                    send(json!({"state": "downloading", "tag": r.tag, "pct": pct, "page": r.page, "recv": true}))
                })?;
                send(json!({"state": "installing", "tag": r.tag, "page": r.page}));
                update::install(&file)?;
                let _ = std::fs::remove_file(&file);
                Ok(r.tag)
            })();
            send(match res {
                Ok(tag) => json!({"state": "ready", "tag": tag}),
                Err(e) => json!({"state": "failed", "err": e}),
            });
        });
    }

    fn update_restart(&mut self) {
        if self.upd["state"] == "ready" {
            self.restart();
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
        if self.hello_gate(i, item) {
            return; // Windows Hello d'abord (#24)
        }
        self.fill_now(i, item);
    }

    fn fill_now(&mut self, i: usize, item: &Value) {
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
        self.load_notes();
        self.pw_used = Instant::now();
        if let Some(p) = self.pending_pw.take() {
            let pw = str_of(&p["p"]);
            if self.vault.upsert(p) {
                let _ = self.vault.save();
                self.toast("pw-saved");
                self.pw_hibp_one(&pw);
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
            Ev::Ui(w, _) | Ev::Active(w, _) | Ev::Cmd(Who::Win(w), _) | Ev::PopupSize(w, ..) | Ev::Remote(w, _) => self.focus_win(*w),
            Ev::Page(id, ..) | Ev::Title(id, _) | Ev::Url(id, _) | Ev::Loading(id, _) | Ev::Fav(id, _) | Ev::Nav(id, ..)
            | Ev::Blocked(id, _) | Ev::PageFs(id, _) | Ev::Audio(id, _) | Ev::FocusBlock(id, _) | Ev::Open(id, ..) | Ev::Cmd(Who::Tab(id), _) => self.focus_tab(*id),
            Ev::Dev(m) => self.focus_tab(m.tab()),
            // téléchargement : traité dans la fenêtre de son onglet (boîte « Enregistrer sous », onglet vide), toujours traité
            Ev::Dl(v) => {
                if let Some(t) = v["tab"].as_u64() {
                    self.focus_tab(t as u32);
                }
                true
            }
            Ev::DlAsk(did) => {
                if let Some(t) = DL_WAIT.with(|m| m.borrow().get(did).and_then(|w| w.3["tab"].as_u64())) {
                    self.focus_tab(t as u32);
                }
                true
            }
            _ => true,
        };
        if !ok {
            return;
        }
        match ev {
            Ev::Ui(_, msg) => self.on_ui(&msg),
            Ev::Media(v) => self.js(&format!("MEDIA({v})")),
            Ev::Upd(v) => {
                let st = v["state"].as_str().unwrap_or("");
                // une vérification qui se termine pendant l'installation (ou après) n'efface pas son état
                let stale = matches!(st, "available" | "latest" | "error") && (self.upd_busy || self.upd["state"] == "ready");
                if !stale {
                    if matches!(st, "ready" | "failed") {
                        self.upd_busy = false;
                    }
                    self.js_all(&format!("UPDS({v})"));
                    self.upd = v;
                }
            }
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
            Ev::Cmd(Who::Tab(id), Cmd::PageFocus) => {
                if self.x.split.is_some_and(|sp| sp.a == id || sp.b == id) && self.tabs.get(self.active).is_some_and(|t| t.id != id) {
                    if let Some(i) = self.idx(id) {
                        self.active = i;
                        self.tabs[i].last = Instant::now();
                        self.tabs[i].lv = now_ms();
                        self.dev_retarget();
                        self.push();
                    }
                }
                self.cmd(Cmd::PageFocus)
            }
            Ev::Cmd(_, c) => self.cmd(c),
            Ev::Tick => {
                self.discard_idle();
                self.daily_check();
            }
            Ev::RamTick => self.poll_procs(),
            Ev::Procs(list) => self.on_procs(list),
            Ev::Power => self.update_power(),
            Ev::Open(oid, u, private) => {
                // un lien ouvert depuis un conteneur y reste, sauf site associé à un autre conteneur (#19)
                let oc = self.idx(oid).map_or(0, |k| self.tabs[k].cont);
                self.open_cont = self.cont_site(&u).or((oc > 0).then_some(oc));
                self.next_opener = oid;
                self.new_tab(Some(u), true, private)
            }
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
            Ev::Do(f) => f(self),
            Ev::PageFs(id, on) => self.page_fullscreen(id, on),
            Ev::Audio(id, on) => self.on_audio(id, on),
            Ev::FocusBlock(id, u) => self.focus_block(id, &u),
            Ev::Blocked(id, h) => {
                self.stat_add("b", 1.0);
                if let Some(i) = self.idx(id) {
                    self.tabs[i].blocked += 1;
                    let bl = &mut self.tabs[i].bl;
                    match bl.iter().position(|x| x.0 == h) {
                        Some(k) => bl[k].1 += 1,
                        None if bl.len() < 300 => bl.push((h, 1)),
                        None => {}
                    }
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
                    if !self.tabs[i].private {
                        let c = self.tabs[i].cont;
                        self.note_visit(c, &u);
                    }
                    if u != "about:blank" || self.tabs[i].url.is_empty() {
                        // un document s'affiche (site, PDF, image) : l'onglet n'est plus un simple lien de téléchargement
                        if !u.is_empty() && u != "about:blank" {
                            self.tabs[i].opener = 0;
                        }
                        if host_of(&u) != host_of(&self.tabs[i].url) {
                            self.tabs[i].fav.clear();
                            self.tabs[i].info = None;
                            self.tabs[i].tint = None;
                            if self.tabs[i].audio == 2 {
                                self.tabs[i].audio = 0;
                            }
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
                        self.tabs[i].bl.clear();
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
                let mut started = false;
                match self.downloads.iter_mut().find(|d| d["id"].as_u64() == id) {
                    Some(d) => {
                        for (k, x) in v.as_object().into_iter().flatten() {
                            d[k] = x.clone();
                        }
                    }
                    // une mise à jour d'un téléchargement retiré de la liste n'en recrée pas un sans nom
                    None if v.get("name").is_some() => {
                        self.downloads.insert(0, v.clone());
                        started = true;
                    }
                    None => return,
                }
                if matches!(v["state"].as_str(), Some("done" | "fail")) {
                    // pages d'onglets fermés gardées pour ce téléchargement : libérées une fois tout terminé
                    let alive: Vec<u32> = self.dl_keep.iter().map(|k| k.1).filter(|t| self.dl_running(*t)).collect();
                    self.dl_keep.retain(|k| alive.contains(&k.1));
                }
                if v["state"] != "run" || v.get("recv").is_none() {
                    self.push();
                }
                self.push_downloads();
                if started {
                    // l'onglet vide d'abord (changer d'onglet referme les petits panneaux), puis la bulle
                    self.dl_tab_cleanup(&v);
                    self.js(&format!("dlStarted({},{})", v["name"], v["id"]));
                }
            }
            Ev::DlAsk(did) => self.dl_ask(did),
            Ev::Remote(_, u) => self.remote_open(&u),
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
            "form" => {
                self.tabs[i].dirty = v["on"] == true;
                return;
            }
            "call" => {
                self.tabs[i].call = v["on"] == true;
                return;
            }
            other => {
                let c = other.to_string();
                self.on_page07(i, &c, &v);
                return;
            }
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
                if !self.upd.is_null() {
                    self.js(&format!("UPDS({})", self.upd));
                }
                if self.blank() {
                    self.js("focusBar()");
                }
            }
            "update-check" => self.update_check(v["auto"] == true),
            // --- Performance (0.7) ---
            "tasks" => {
                self.tasks_open = v["open"] == true;
                self.poller();
                if self.tasks_open {
                    self.poll_procs();
                }
            }
            "proc-end" => {
                let pid = v["pid"].as_u64().unwrap_or(0) as u32;
                // jamais le processus principal de WebView2, le GPU ni Plume lui-même
                let ok = self.procs.as_array().into_iter().flatten().any(|p| p["pid"] == pid && (p["kind"] == "renderer" || p["kind"] == "utility") && p["ui"] != true);
                if ok && perf::kill(pid) {
                    self.poll_procs();
                }
            }
            "tab-unload" => {
                if let Some(i) = self.idx(id).filter(|i| *i != self.active && !self.dl_running(id)) {
                    let t = &mut self.tabs[i];
                    t.view = None;
                    t.ram = 0;
                    self.push();
                    self.poll_procs();
                }
            }
            "lite-set" => {
                let bit = match s("k").as_str() {
                    "js" => adblock::LITE_JS,
                    "img" => adblock::LITE_IMG,
                    "font" => adblock::LITE_FONT,
                    _ => adblock::LITE_AUTO,
                };
                self.lite_set(bit, v["on"] == true);
            }
            "lite-rm" => {
                if let Some(m) = self.cfg["liteSites"].as_object_mut() {
                    m.remove(&s("host"));
                }
                self.sync_lite();
                self.push_page_cfg();
                self.save();
                self.push();
            }
            "stats-get" => self.push_stats(),
            "stats-reset" => {
                self.stats = json!({"days": {}});
                self.stats_dirty = true;
                self.save();
                self.push_stats();
            }
            "nosleep" => {
                if let Some(i) = self.idx(id) {
                    let h = import::host_of(&self.tabs[i].url);
                    if !h.is_empty() {
                        self.toggle_list("sleepList", &h);
                    }
                }
            }
            "update-install" => self.update_install(),
            "update-restart" => self.update_restart(),
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
                    self.drop_tabs(|t| !keep.contains(&t.id));
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
                    self.drop_tabs(|t| t.group == g);
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
                    // mot de passe solide proposé sur un formulaire d'inscription (#22)
                    "gen" => self.eval_tab(i, &format!("window.__plume&&__plume.fillNew&&__plume.fillNew({})", info["pw"])),
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
                let _ = std::fs::remove_file(self.vault.hello_path()); // nouvelle clé : l'ancienne protection Hello ne vaut plus
                self.cfg["hello"] = json!(false);
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
                        self.pw_hibp_one(&s("p"));
                        self.sync_pw_hosts();
                    }
                }
                self.push_vault();
                self.push();
            }
            "vault-reset" => {
                let _ = self.vault.delete();
                let _ = std::fs::remove_file(self.vault.hello_path());
                self.cfg["hello"] = json!(false);
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
                let op = DLS.with(|m| m.borrow().get(&id).cloned());
                let ok = op.is_some_and(|op| unsafe { op.Cancel().is_ok() });
                // plus d'opération à annuler (suivi perdu) : l'entrée passe en échec, la liste peut alors être vidée
                if !ok && let Some(d) = self.downloads.iter_mut().find(|d| d["id"].as_u64() == Some(id as u64) && d["state"] == "run") {
                    d["state"] = json!("fail");
                    self.push_downloads();
                    self.push();
                }
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
            // --- navigateur par défaut ---
            "def-set" => {
                // version installée : (ré)inscrite dans Applications par défaut, puis la page des Paramètres Windows
                if setup::installed() {
                    defbrowser::register(&setup::exe());
                }
                defbrowser::open_settings();
                self.cfg["defLast"] = json!(now_ms());
                self.save();
                self.push();
            }
            "def-later" => {
                self.cfg["defLast"] = json!(now_ms());
                self.save();
                self.push();
            }
            "def-never" => {
                settings::mark(&mut self.cfg, "defAsk");
                self.set_cfg("defAsk", json!(false));
            }
            // --- réglages ---
            "set" => {
                let k = s("k");
                if !k.is_empty() && k != "onboarded" && k != "mod" && k != "cfgV" {
                    settings::mark(&mut self.cfg, &k);
                    self.set_cfg(&k, v["v"].clone());
                }
            }
            "onboard" => {
                self.cfg["name"] = json!(s("name").chars().take(40).collect::<String>());
                settings::apply_mode(&mut self.cfg, &s("mode"));
                self.cfg["onboarded"] = json!(true);
                self.cfg_effect_all();
            }
            "mode" => {
                settings::apply_mode(&mut self.cfg, &s("mode"));
                self.cfg_effect_all();
            }
            // « Réinitialiser » une section, ou tout (sec vide)
            "reset" => {
                let sec = s("sec");
                settings::reset(&mut self.cfg, (!sec.is_empty()).then_some(sec.as_str()));
                self.cfg_effect_all();
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
            other => {
                let c = other.to_string();
                self.on_ui07(&c, &v);
            }
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
    // un lien ouvert depuis une autre application trouve cette fenêtre (même profil)
    defbrowser::mark_window(hwnd);
    window
}

/// Interface (onglets, barre d'adresse, panneaux) d'une fenêtre, servie par plume://.
fn build_ui(window: &Window, base: WebViewBuilder<'_>, proxy: &EventLoopProxy<Ev>, wid: u32) -> WebView {
    let p = proxy.clone();
    let ui = base
        .with_custom_protocol("plume".into(), |_, req| {
            // images de la nouvelle page (#27) : plume://ui/ntp/image, plume://ui/ntp/daily
            if let Some((data, mime)) = req.uri().path().strip_prefix("/ntp/").and_then(look07::ntp_file) {
                return wry::http::Response::builder()
                    .header("Content-Type", mime)
                    .header("Cache-Control", "no-store")
                    .body(std::borrow::Cow::Owned(data))
                    .unwrap();
            }
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

/// Identifiant du cadre principal d'une page (relie les processus WebView2 aux onglets).
fn main_frame(v: &WebView) -> Option<u32> {
    unsafe {
        let c = v.webview().cast::<ICoreWebView2_20>().ok()?;
        let mut id = 0u32;
        c.FrameId(&mut id).ok()?;
        Some(id)
    }
}

/// Date locale « AAAA-MM-JJ » (statistiques par jour).
fn today() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay)
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
    if let Some(c) = tabs07::custom_accel(vk, ctrl, shift, alt) {
        return Some(c);
    }
    Some(match (ctrl, shift, alt, vk) {
        (true, false, true, 0x31..=0x39) => Cmd::Space((vk - 0x31) as usize), // Ctrl+Alt+1…9 : espaces
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
        (false, false, false, 0x7A) => Cmd::Fullscreen,                // F11
        (false, true, false, 0x1B) => Cmd::Tasks,                      // Maj+Échap (gestionnaire de tâches)
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
                if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE || ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT {
                    let mut src = PWSTR::null();
                    s.Source(&mut src)?;
                    let f = adblock::lite(&import::host_of(&webview2_com::take_pwstr(src)));
                    let bit = if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE { adblock::LITE_IMG } else { adblock::LITE_FONT };
                    if f & bit != 0 {
                        a.SetResponse(&env.CreateWebResourceResponse(None, 403, w!("Lite"), w!(""))?)?;
                        return Ok(());
                    }
                }
                let mut u = PWSTR::null();
                a.Request()?.Uri(&mut u)?;
                let u = webview2_com::take_pwstr(u);
                let bh = host_of(&u).split(':').next().unwrap_or("").to_string();
                if !adblock::listed(&bh) {
                    return Ok(());
                }
                let mut src = PWSTR::null();
                s.Source(&mut src)?;
                if adblock::allowed(&import::host_of(&webview2_com::take_pwstr(src))) {
                    return Ok(());
                }
                let r = env.CreateWebResourceResponse(None, 403, w!("Blocked"), w!(""))?;
                a.SetResponse(&r)?;
                let _ = p.send_event(Ev::Blocked(id, bh));
                Ok(())
            })),
            &mut tok,
        )?;
        // une vidéo passe en plein écran → la fenêtre aussi (#30)
        let pf = proxy.clone();
        core.add_ContainsFullScreenElementChanged(
            &ContainsFullScreenElementChangedEventHandler::create(Box::new(move |s, _| {
                if let Some(s) = s {
                    let mut on = BOOL(0);
                    s.ContainsFullScreenElement(&mut on)?;
                    let _ = pf.send_event(Ev::PageFs(id, on.as_bool()));
                }
                Ok(())
            })),
            &mut tok,
        )?;
        let pfb = proxy.clone();
        core.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |s, args| {
                if let (Some(s), Some(args)) = (s, args) {
                    let mut u = PWSTR::null();
                    args.Uri(&mut u)?;
                    let uri = webview2_com::take_pwstr(u);
                    let host = import::host_of(&uri);
                    // session de focus : site bloqué → page de blocage douce (#34)
                    if uri.starts_with("http") && tools07::focus_blocked(&host) {
                        args.SetCancel(true)?;
                        let _ = pfb.send_event(Ev::FocusBlock(id, uri));
                        return Ok(());
                    }
                    let js_off = adblock::lite(&host) & adblock::LITE_JS != 0;
                    let st = s.Settings()?;
                    let mut on = BOOL(1);
                    st.IsScriptEnabled(&mut on)?;
                    if on.as_bool() == js_off {
                        st.SetIsScriptEnabled(!js_off)?;
                    }
                }
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
        // Téléchargements : notre propre panneau au lieu de la bulle d'Edge, « Enregistrer sous » si demandé
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
                    // "tab" : l'onglet d'origine garde sa page ouverte tant que le téléchargement dure
                    let start = json!({"id": did, "name": dl_name(&path), "path": path, "url": webview2_com::take_pwstr(uri),
                        "total": total, "recv": 0, "state": "run", "t": now_ms(), "tab": id});
                    if DL_ASK.load(Ordering::Relaxed) {
                        // la boîte « Enregistrer sous » s'ouvre depuis la boucle ; le téléchargement attend grâce au report
                        let wait = a.GetDeferral()?;
                        DL_WAIT.with(|m| m.borrow_mut().insert(did, (a, wait, op, start)));
                        let _ = p.send_event(Ev::DlAsk(did));
                        return Ok(());
                    }
                    track_dl(&op, did, &p, start)
                })),
                &mut tok,
            )?;
        }
        Ok(())
    }
}

fn dl_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Téléchargement accepté : il entre dans la liste, sa progression et sa fin arrivent à la boucle,
/// et l'opération est gardée pour pouvoir l'annuler.
unsafe fn track_dl(op: &ICoreWebView2DownloadOperation, did: u32, p: &EventLoopProxy<Ev>, start: Value) -> Result<()> {
    let _ = p.send_event(Ev::Dl(start));
    let mut tok = 0i64;
    let (p2, last) = (p.clone(), Cell::new(Instant::now()));
    unsafe {
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
            &mut tok,
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
                    let mut m = json!({"id": did, "state": state, "recv": r, "total": t});
                    if state == "done" {
                        // chemin final (Windows a pu ajouter « (1) » au nom)
                        let mut fp = PWSTR::null();
                        if s.ResultFilePath(&mut fp).is_ok() {
                            let f = webview2_com::take_pwstr(fp);
                            if !f.is_empty() {
                                m["name"] = json!(dl_name(&f));
                                m["path"] = json!(f);
                            }
                        }
                    }
                    let _ = p3.send_event(Ev::Dl(m));
                    if state != "run" {
                        DLS.with(|m| m.borrow_mut().remove(&did));
                    }
                }
                Ok(())
            })),
            &mut tok,
        )?;
    }
    DLS.with(|m| m.borrow_mut().insert(did, op.clone()));
    Ok(())
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
            WM_POWERBROADCAST => {
                let _ = proxy.send_event(Ev::Power);
            }
            WM_ACTIVATE => {
                let _ = proxy.send_event(Ev::Active(*wid, (wp.0 & 0xffff) as u32 != WA_INACTIVE));
            }
            // lien envoyé par un autre lancement de Plume (navigateur par défaut)
            WM_COPYDATA => {
                if let Some(u) = defbrowser::received(lp) {
                    let _ = proxy.send_event(Ev::Remote(*wid, u));
                    return LRESULT(1);
                }
            }
            WM_NCDESTROY => defbrowser::unmark_window(h),
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
