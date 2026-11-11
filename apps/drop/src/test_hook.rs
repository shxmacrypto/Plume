//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_DROP_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                         the window's state (link, port, devices, asks, history, log)
//!   {"cmd":"hits"}                          clickable elements and their rectangles
//!   {"cmd":"click","id":"Allow"}            a click on an element (its name as `hits` gives it)
//!   {"cmd":"type","text":"…"}               text in the open field (the text to send)
//!   {"cmd":"share","paths":["C:\\…"]}       offers files, as if dropped on the window
//!   {"cmd":"scroll","y":300}                scrolls the page
//!   {"cmd":"set","key":"lang","value":"fr"} a setting (lang, copy_text, open_links, folder)
//!   {"cmd":"quit"}                          closes the window

use crate::ui::{App, View};
use plume_ui::{Button, Event, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

pub fn start(win: Window) {
    let Some(port) = std::env::var("PLUME_DROP_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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
        let out = match h.as_any().downcast_mut::<App>() {
            Some(app) => match cmd["cmd"].as_str().unwrap_or("") {
                "state" => state(app),
                "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
                "click" => {
                    let want = cmd["id"].as_str().unwrap_or("");
                    match app.hits.rect_of(|id| format!("{id:?}") == want) {
                        Some(r) => {
                            let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
                            let mods = Mods::default();
                            app.event(Event::MouseMove { p, mods }, cx);
                            app.event(Event::MouseDown { p, button: Button::Left, clicks: 1, mods }, cx);
                            app.event(Event::MouseUp { p, button: Button::Left, mods }, cx);
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no such target" }),
                    }
                }
                "type" => {
                    let text = cmd["text"].as_str().unwrap_or("").to_string();
                    app.win.edit_set_text(&text);
                    json!({ "ok": app.win.edit_visible() })
                }
                "share" => {
                    let paths: Vec<PathBuf> =
                        cmd["paths"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(PathBuf::from)).collect()).unwrap_or_default();
                    app.share(paths, cx);
                    json!({ "ok": true })
                }
                "scroll" => {
                    app.scroll = cmd["y"].as_f64().unwrap_or(0.0) as f32;
                    cx.repaint();
                    json!({ "ok": true })
                }
                "set" => {
                    let v = &cmd["value"];
                    let known = match cmd["key"].as_str().unwrap_or("") {
                        "lang" => {
                            app.set_lang(v.as_str().unwrap_or("system"));
                            true
                        }
                        "copy_text" => {
                            app.settings.copy_text = v.as_bool().unwrap_or(true);
                            true
                        }
                        "open_links" => {
                            app.settings.open_links = v.as_bool().unwrap_or(false);
                            true
                        }
                        "folder" => {
                            let p = PathBuf::from(v.as_str().unwrap_or(""));
                            app.hub.set_folder(&p);
                            app.settings.folder = p;
                            true
                        }
                        _ => false,
                    };
                    cx.repaint();
                    if known { json!({ "ok": true }) } else { json!({ "error": "unknown setting" }) }
                }
                "quit" => {
                    cx.after(|w| w.close());
                    json!({ "ok": true })
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

fn state(app: &mut App) -> Value {
    json!({
        "link": app.link(),
        "port": app.port,
        "ip": app.ip.map(|i| i.to_string()),
        "view": match app.view { View::Home => "home", View::Settings => "settings" },
        "folder": app.settings.folder,
        "lang": app.settings.lang,
        "copy_text": app.settings.copy_text,
        "open_links": app.settings.open_links,
        "composing": app.composing,
        "asks": app.asks.iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
        "devices": app.devices.iter().map(|d| json!({ "name": d.name, "state": d.state.word(), "connected": d.connected, "known": d.known })).collect::<Vec<_>>(),
        "history": app.history.iter().take(50).map(|e| json!({
            "id": e.id, "dir": format!("{:?}", e.dir), "kind": format!("{:?}", e.kind), "name": e.name, "path": e.path,
            "size": e.size, "state": format!("{:?}", e.state), "device": e.device, "saves": e.saves,
        })).collect::<Vec<_>>(),
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "scroll": app.scroll,
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
        "dark": app.theme.dark,
    })
}
