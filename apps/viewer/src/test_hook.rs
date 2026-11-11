//! Test socket: a script drives the window without touching the user's mouse or keyboard. Only in a
//! `--features test-hook` build, and only when PLUME_VIEWER_TEST_PORT is set. One JSON command per line on
//! 127.0.0.1, one JSON answer per line.
//!   {"cmd":"state"}                          the window's state
//!   {"cmd":"click","id":"Allow"}             a click on an element (its name as `hits` gives it)
//!   {"cmd":"hits"}                           the clickable elements and their rectangles
//!   {"cmd":"type","text":"…"}                text in the open field (the password)
//!   {"cmd":"quit"}                           quits for good (as "Quit" in the icon's menu)

use crate::ui::{App, Net};
use plume_ui::{Button, Event, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

thread_local! {
    static QUIT: std::cell::RefCell<Option<Rc<Cell<bool>>>> = const { std::cell::RefCell::new(None) };
}

pub fn start(win: Window, quitting: Rc<Cell<bool>>) {
    QUIT.with(|q| *q.borrow_mut() = Some(quitting));
    let Some(port) = std::env::var("PLUME_VIEWER_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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
                "quit" => {
                    QUIT.with(|q| {
                        if let Some(q) = q.borrow().as_ref() {
                            q.set(true);
                        }
                    });
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
        "id": app.store.id,
        "code": app.code,
        "qr": app.qr_text(),
        "net": match app.net { Net::Online => "online", Net::Connecting => "connecting", Net::Offline => "offline" },
        "asks": app.asks.iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
        "live": app.live.as_ref().map(|l| json!({ "name": l.name, "linked": l.linked, "mode": format!("{:?}", l.mode) })),
        "password": app.store.has_password(),
        "autostart": app.autostart,
        "editing": app.editing,
        "locked": app.gate.locked(),
        "failures": app.gate.failures(),
        "keep": app.keep.get(),
        "visible": crate::sys::visible(app.win.hwnd),
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
        "dark": app.theme.dark,
        "stats": crate::capture::stats::get(),
    })
}
