//! Plume DevTools : panneau intégré en bas de la fenêtre (F12), dessiné par l'UI de Plume.
//! Ce module n'est qu'un pont vers le protocole Chrome DevTools (CDP) de l'onglet inspecté :
//! l'UI appelle des méthodes CDP (liste blanche) et reçoit les événements utiles.
//! Rien n'est actif tant que le panneau est fermé.
use serde_json::{Value, json};
use tao::event_loop::EventLoopProxy;
use webview2_com::{
    CallDevToolsProtocolMethodCompletedHandler, DevToolsProtocolEventReceivedEventHandler,
    Microsoft::Web::WebView2::Win32::*,
};
use windows::core::{HSTRING, PWSTR};

use crate::Ev;

pub enum Msg {
    Res(u32, u64, String),
    Event(u32, &'static str, String),
}

impl Msg {
    /// Onglet inspecté d'où vient le message.
    pub fn tab(&self) -> u32 {
        match self {
            Msg::Res(t, ..) | Msg::Event(t, ..) => *t,
        }
    }
}

/// Événements CDP relayés à l'UI.
const EVENTS: &[&str] = &[
    "Runtime.consoleAPICalled",
    "Runtime.exceptionThrown",
    "Runtime.executionContextsCleared",
    "Log.entryAdded",
    "Network.requestWillBeSent",
    "Network.responseReceived",
    "Network.loadingFinished",
    "Network.loadingFailed",
    "DOM.setChildNodes",
    "DOM.documentUpdated",
    "Overlay.inspectNodeRequested",
    "Page.frameNavigated",
];

/// Domaines CDP que l'UI a le droit d'appeler.
const ALLOWED: &[&str] = &["HeapProfiler.collectGarbage", "Runtime.", "DOM.", "CSS.", "Overlay.", "Network.", "Log.", "Page.reload", "Performance.", "DOMStorage.", "Emulation."];

#[derive(Default)]
pub struct DevTools {
    pub target: u32,
    open: bool,
    h: f64,
    subs: Vec<(ICoreWebView2DevToolsProtocolEventReceiver, i64)>,
    core: Option<ICoreWebView2>,
}

fn call(core: &ICoreWebView2, method: &str, params: &str, done: Option<(u32, u64, EventLoopProxy<Ev>)>) {
    unsafe {
        let _ = core.CallDevToolsProtocolMethod(
            &HSTRING::from(method),
            &HSTRING::from(params),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |res, out| {
                if let Some((tab, r, p)) = done {
                    let body = match res {
                        Ok(()) if !out.is_empty() => out,
                        Ok(()) => "{}".into(),
                        Err(e) => json!({"error": e.message().to_string()}).to_string(),
                    };
                    let _ = p.send_event(Ev::Dev(Msg::Res(tab, r, body)));
                }
                Ok(())
            })),
        );
    }
}

impl DevTools {
    /// Hauteur du panneau (px CSS de l'UI), 0 si fermé.
    pub fn height(&self) -> f64 {
        if self.open { if self.h > 0.0 { self.h } else { 300.0 } } else { 0.0 }
    }

    pub fn set_height(&mut self, h: f64) {
        self.h = h.clamp(0.0, 2000.0);
    }

    pub fn open(&mut self, id: u32, core: &ICoreWebView2, proxy: &EventLoopProxy<Ev>) {
        self.close();
        self.target = id;
        self.open = true;
        unsafe {
            for &name in EVENTS {
                let Ok(r) = core.GetDevToolsProtocolEventReceiver(&HSTRING::from(name)) else { continue };
                let p = proxy.clone();
                let mut tok = 0i64;
                let ok = r.add_DevToolsProtocolEventReceived(
                    &DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
                        if let Some(a) = args {
                            let mut s = PWSTR::null();
                            a.ParameterObjectAsJson(&mut s)?;
                            let _ = p.send_event(Ev::Dev(Msg::Event(id, name, webview2_com::take_pwstr(s))));
                        }
                        Ok(())
                    })),
                    &mut tok,
                );
                if ok.is_ok() {
                    self.subs.push((r, tok));
                }
            }
        }
        for m in ["Runtime.enable", "Log.enable", "Network.enable", "DOM.enable", "CSS.enable", "Overlay.enable", "Performance.enable"] {
            call(core, m, "{}", None);
        }
        self.core = Some(core.clone());
    }

    pub fn close(&mut self) {
        for (r, tok) in self.subs.drain(..) {
            unsafe {
                let _ = r.remove_DevToolsProtocolEventReceived(tok);
            }
        }
        if let Some(c) = self.core.take() {
            call(&c, "Overlay.setInspectMode", r#"{"mode":"none"}"#, None);
            call(&c, "Overlay.hideHighlight", "{}", None);
            for m in ["Network.disable", "Performance.disable", "Log.disable", "Overlay.disable", "CSS.disable", "DOM.disable", "Runtime.disable"] {
                call(&c, m, "{}", None);
            }
        }
        self.open = false;
        self.target = 0;
    }

    /// Requête de l'UI : {c:"dt", m: méthode, p: paramètres, r: n° de réponse}
    pub fn on_ui(&mut self, v: &Value, core: &ICoreWebView2, proxy: &EventLoopProxy<Ev>) -> Option<String> {
        let m = v["m"].as_str().unwrap_or("");
        let r = v["r"].as_u64().unwrap_or(0);
        if !ALLOWED.iter().any(|a| m.starts_with(a)) {
            return Some(format!("DT.res({r},{})", json!({"error": "not allowed"})));
        }
        let params = if v["p"].is_object() { v["p"].to_string() } else { "{}".into() };
        call(core, m, &params, Some((self.target, r, proxy.clone())));
        None
    }

    pub fn on_msg(&mut self, m: Msg) -> Option<String> {
        if !self.open {
            return None;
        }
        Some(match m {
            Msg::Res(t, r, j) if t == self.target => format!("DT.res({r},{j})"),
            Msg::Event(t, n, j) if t == self.target => format!("DT.ev({},{j})", json!(n)),
            _ => return None,
        })
    }
}
