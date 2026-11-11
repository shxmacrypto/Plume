//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_PASSWORDS_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//! The answers never hold a secret: passwords, codes and notes appear only as lengths.
//!   {"cmd":"state"}                         the window's state
//!   {"cmd":"hits"}                          clickable elements and their rectangles
//!   {"cmd":"click","id":"Nav(Generator)"}   a click on an element (its name as `hits` gives it)
//!   {"cmd":"rclick","id":"…"}               a right click
//!   {"cmd":"type","text":"…"}               text in the open field
//!   {"cmd":"key","vk":13}                   a key (to the open field for Enter, Esc and Tab)
//!   {"cmd":"hotkey","title":"…"}            the auto-type shortcut, as if pressed in a window with this title
//!   {"cmd":"set","key":"autolock_min","value":5}   a setting
//!   {"cmd":"import","path":"…"}             imports a CSV file
//!   {"cmd":"select","i":0}                  selects the n-th row of the list
//!   {"cmd":"scroll","y":200}                scrolls the page
//!   {"cmd":"lock"}  {"cmd":"quit"}

use crate::ui::{App, F};
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

pub fn start(win: Window, quitting: Rc<Cell<bool>>) {
    let Some(port) = std::env::var("PLUME_PASSWORDS_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
    // the window hook's "really quit" flag stays on this thread: the app reads `quitting` from it
    QUIT.with(|q| *q.borrow_mut() = Some(quitting));
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

thread_local! {
    static QUIT: std::cell::RefCell<Option<Rc<Cell<bool>>>> = const { std::cell::RefCell::new(None) };
}

fn click(app: &mut App, want: &str, right: bool, cx: &mut plume_ui::Cx) -> Value {
    match app.hits.rect_of(|id| format!("{id:?}") == want) {
        Some(r) => {
            let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
            let mods = Mods::default();
            let button = if right { Button::Right } else { Button::Left };
            app.event(Event::MouseMove { p, mods }, cx);
            app.event(Event::MouseDown { p, button, clicks: 1, mods }, cx);
            if !right {
                app.event(Event::MouseUp { p, button, mods }, cx);
            }
            json!({ "ok": true })
        }
        None => json!({ "error": "no such target" }),
    }
}

fn run(sender: &Sender, line: &str) -> String {
    let Ok(cmd) = serde_json::from_str::<Value>(line) else { return json!({ "error": "bad json" }).to_string() };
    let (tx, rx) = mpsc::channel();
    let sent = sender.send(move |h, cx| {
        let out = match h.as_any().downcast_mut::<App>() {
            Some(app) => match cmd["cmd"].as_str().unwrap_or("") {
                "state" => state(app),
                "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
                "click" => click(app, cmd["id"].as_str().unwrap_or(""), false, cx),
                "rclick" => click(app, cmd["id"].as_str().unwrap_or(""), true, cx),
                "type" => {
                    if app.edit_shown.is_none() {
                        json!({ "error": "no open field" })
                    } else if app.edit_for != app.edit_shown {
                        // the field moves at the next painting: ask again
                        {
                            cx.repaint();
                            json!({ "error": "wait", "for": format!("{:?}", app.edit_for), "shown": format!("{:?}", app.edit_shown), "paints": app.paints })
                        }
                    } else {
                        let text = cmd["text"].as_str().unwrap_or("").to_string();
                        app.win.edit_set_text(&text);
                        app.event(Event::EditChanged(text), cx);
                        json!({ "ok": true })
                    }
                }
                "key" => {
                    let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
                    let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: false };
                    if app.win.edit_visible() && matches!(vk, 0x0D | 0x1B | 0x09) {
                        app.event(Event::EditKey { vk, mods }, cx);
                    } else {
                        app.event(Event::Key { vk, mods, repeat: false }, cx);
                    }
                    json!({ "ok": true })
                }
                "hotkey" => {
                    app.on_hotkey(0, cmd["title"].as_str().unwrap_or("").to_string(), cx);
                    json!({ "ok": true })
                }
                "set" => {
                    let k = cmd["key"].as_str().unwrap_or("");
                    let mut s = serde_json::to_value(&app.set).unwrap_or_default();
                    s[k] = cmd["value"].clone();
                    match serde_json::from_value(s) {
                        Ok(new) => {
                            app.set = new;
                            app.settings_changed(cx);
                            json!({ "ok": true })
                        }
                        Err(e) => json!({ "error": e.to_string() }),
                    }
                }
                "import" => {
                    let p = std::path::PathBuf::from(cmd["path"].as_str().unwrap_or(""));
                    app.import_csv(&p, cx);
                    json!({ "ok": true })
                }
                "select" => {
                    let vis = app.visible();
                    match vis.get(cmd["i"].as_u64().unwrap_or(0) as usize) {
                        Some(&i) => {
                            let id = app.vault.data.entries[i].id.clone();
                            app.select(Some(id));
                            cx.repaint();
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no such row" }),
                    }
                }
                "scroll" => {
                    app.page_scroll = cmd["y"].as_f64().unwrap_or(0.0) as f32;
                    cx.repaint();
                    json!({ "ok": true })
                }
                "lock" => {
                    app.lock("test", cx);
                    json!({ "ok": true })
                }
                "quit" => {
                    QUIT.with(|q| {
                        if let Some(q) = q.borrow().as_ref() {
                            q.set(true);
                        }
                    });
                    app.quitting = true;
                    app.update_keep();
                    cx.after(|w| w.close());
                    json!({ "ok": true })
                }
                "repaint" => {
                    cx.repaint();
                    json!({ "ok": true })
                }
                _ => json!({ "error": "unknown command" }),
            },
            None => json!({ "error": "no app" }),
        };
        let _ = tx.send(out.to_string());
        // paint now: the next command sees the window as it is (fields placed, buttons where they are drawn)
        cx.repaint();
        cx.after(|w| unsafe {
            let _ = windows::Win32::Graphics::Gdi::UpdateWindow(w.hwnd);
        });
    });
    if !sent {
        return json!({ "error": "window closed" }).to_string();
    }
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn state(app: &mut App) -> Value {
    let sel = app.selected().cloned();
    let now = crate::vault::now();
    let entries: Vec<Value> = app
        .vault
        .data
        .entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id, "kind": format!("{:?}", e.kind), "title": e.title, "folder": e.folder, "favorite": e.favorite,
                "has_password": !e.password.is_empty(), "has_totp": !e.totp.is_empty(), "tags": e.tags.len(),
            })
        })
        .collect();
    json!({
        "screen": format!("{:?}", app.screen),
        "view": format!("{:?}", app.view),
        "busy": app.busy,
        "error": app.error,
        "unlocked": app.vault.unlocked(),
        "entries": entries,
        "visible": app.visible().len(),
        "folders": app.folders(),
        "selected": sel.as_ref().map(|e| json!({
            "id": e.id,
            "title": e.title,
            "code_digits": crate::totp::Totp::parse(&e.totp).map(|t| t.code_at(now).len()),
            "password_len": e.password.chars().count(),
        })),
        "form": app.form.as_ref().map(|f| json!({ "kind": format!("{:?}", f.e.kind), "new": f.new, "error": f.error, "title": f.e.title,
            "password_len": f.e.password.chars().count(), "has_totp": !f.e.totp.is_empty() })),
        "modal": app.modal.as_ref().map(|m| format!("{m:?}")),
        "modal_error": app.modal_error,
        "edit_for": app.edit_for.map(|f| format!("{f:?}")),
        "edit_shown": app.edit_shown.map(|f| format!("{f:?}")),
        "masked": app.edit_for.map(|f| app.masked(f)),
        "pick": app.pick.as_ref().map(|(t, ids)| json!({ "title": t, "n": ids.len() })),
        "shown": app.shown.iter().collect::<Vec<_>>(),
        "generated_len": app.generated.chars().count(),
        "generated_strength": crate::generator::strength(&app.generated).0.0,
        "report": app.report.as_ref().map(|(w, r)| json!({ "weak": w.len(), "reused_groups": r.len() })),
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "clip_pending": app.clip.is_some(),
        "hello_on": app.hello_on,
        "settings": serde_json::to_value(&app.set).unwrap_or_default(),
        "dark": app.t.dark,
        "page_h": app.page_h,
        "log": app.log,
        "search_field": app.fields.iter().any(|(f, _)| *f == F::Search),
        "hwnd": app.win.hwnd.0 as isize,
        "paints": app.paints,
    })
}
