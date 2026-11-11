//! Prise d'essai : un script pilote la fenêtre sans toucher à la souris ni au clavier de l'utilisateur.
//! Elle n'existe que dans une compilation `--features test-hook`, et ne s'ouvre que si PLUME_SUITE_TEST_PORT est
//! défini. Protocole : une commande JSON par ligne sur 127.0.0.1, une réponse JSON par ligne.
//!   {"cmd":"state"}                       l'état de la fenêtre
//!   {"cmd":"click","id":"Install(1)"}     un clic sur un élément (son nom tel que `hits` le donne)
//!   {"cmd":"hits"}                        les éléments cliquables et leur rectangle
//!   {"cmd":"refresh"}                     relit ce qui est installé

use crate::ui::{Hub, Upd};
use plume_ui::{Button, Event, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::mpsc,
    time::Duration,
};

pub fn start(win: Window) {
    let Some(port) = std::env::var("PLUME_SUITE_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
    let sender = win.sender();
    std::thread::spawn(move || {
        let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) else { return };
        for stream in listener.incoming().flatten() {
            let Ok(read) = stream.try_clone() else { continue };
            let mut out = stream;
            let mut reader = BufReader::new(read);
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if writeln!(out, "{}", run(&sender, line.trim())).is_err() {
                    break;
                }
            }
        }
    });
}

fn run(sender: &Sender, line: &str) -> String {
    let Ok(cmd) = serde_json::from_str::<Value>(line) else { return json!({ "error": "bad json" }).to_string() };
    let (tx, rx) = mpsc::channel();
    let sent = sender.send(move |h, cx| {
        let out = match h.as_any().downcast_mut::<Hub>() {
            Some(hub) => match cmd["cmd"].as_str().unwrap_or("") {
                "state" => state(hub),
                "refresh" => {
                    hub.refresh();
                    cx.repaint();
                    state(hub)
                }
                "hits" => json!(hub.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
                "click" => {
                    let want = cmd["id"].as_str().unwrap_or("");
                    match hub.hits.rect_of(|id| format!("{id:?}") == want) {
                        Some(r) => {
                            let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
                            let mods = Mods::default();
                            hub.event(Event::MouseMove { p, mods }, cx);
                            hub.event(Event::MouseDown { p, button: Button::Left, clicks: 1, mods }, cx);
                            hub.event(Event::MouseUp { p, button: Button::Left, mods }, cx);
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no such target" }),
                    }
                }
                _ => json!({ "error": "unknown command" }),
            },
            None => json!({ "error": "no app" }),
        };
        let _ = tx.send(out.to_string());
    });
    if !sent {
        return json!({ "error": "window closed" }).to_string();
    }
    rx.recv_timeout(Duration::from_secs(15)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn state(hub: &Hub) -> Value {
    let upd = match &hub.upd {
        Upd::Dev => json!({ "kind": "dev" }),
        Upd::Checking => json!({ "kind": "checking" }),
        Upd::UpToDate => json!({ "kind": "up-to-date" }),
        Upd::Offline => json!({ "kind": "offline" }),
        Upd::Available(v) => json!({ "kind": "available", "version": v }),
        Upd::Downloading(v, p) => json!({ "kind": "downloading", "version": v, "percent": p }),
        Upd::Installing => json!({ "kind": "installing" }),
        Upd::Done => json!({ "kind": "done" }),
        Upd::Failed(e) => json!({ "kind": "failed", "error": e }),
    };
    json!({
        "installed": hub.installed,
        "busy": hub.busy.map(|(i, install)| json!({ "app": i, "install": install })),
        "update": upd,
        "toast": hub.toast.as_ref().map(|(t, _)| t.clone()),
        "hover": hub.hover.map(|id| format!("{id:?}")),
        "hwnd": hub.win.hwnd.0 as isize,
        "dark": hub.theme.dark,
        "version": plume_update::VERSION,
    })
}
