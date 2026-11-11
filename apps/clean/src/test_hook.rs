//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_CLEAN_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                          the window's state (scan, suggestions, last cleanup, log)
//!   {"cmd":"hits"}                           clickable elements and their rectangles
//!   {"cmd":"click","id":"Tab(Cleanup)"}      a click on an element (its name as `hits` gives it)
//!   {"cmd":"dclick","id":"Row(0)"}           a double click
//!   {"cmd":"rclick","id":"Row(0)"}           a right click
//!   {"cmd":"key","vk":8}                     a key
//!   {"cmd":"scan","path":"C:\\some\\folder"} scans a folder (no dialog)
//!   {"cmd":"go","view":"cleanup"}            a page: drives, map, cleanup, settings
//!   {"cmd":"set","key":"old_days","value":30} a setting
//!   {"cmd":"wheel","dy":-1}                 scrolls the page's list (x, y optional)
//!   {"cmd":"repaint"}

use crate::ui::{App, View};
use plume_ui::{Button, Event, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::mpsc,
    time::Duration,
};

pub fn start(win: Window) {
    let Some(port) = std::env::var("PLUME_CLEAN_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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

fn click(app: &mut App, want: &str, how: &str, cx: &mut plume_ui::Cx) -> Value {
    match app.hits.rect_of(|id| format!("{id:?}") == want) {
        Some(r) => {
            let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
            let mods = Mods::default();
            app.event(Event::MouseMove { p, mods }, cx);
            match how {
                "right" => app.event(Event::MouseDown { p, button: Button::Right, clicks: 1, mods }, cx),
                "double" => {
                    app.event(Event::MouseDown { p, button: Button::Left, clicks: 1, mods }, cx);
                    app.event(Event::MouseUp { p, button: Button::Left, mods }, cx);
                    app.event(Event::MouseDown { p, button: Button::Left, clicks: 2, mods }, cx);
                    app.event(Event::MouseUp { p, button: Button::Left, mods }, cx);
                }
                _ => {
                    app.event(Event::MouseDown { p, button: Button::Left, clicks: 1, mods }, cx);
                    app.event(Event::MouseUp { p, button: Button::Left, mods }, cx);
                }
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
                "click" => click(app, cmd["id"].as_str().unwrap_or(""), "left", cx),
                "dclick" => click(app, cmd["id"].as_str().unwrap_or(""), "double", cx),
                "rclick" => click(app, cmd["id"].as_str().unwrap_or(""), "right", cx),
                "key" => {
                    let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
                    let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: false, alt: cmd["alt"].as_bool().unwrap_or(false) };
                    app.event(Event::Key { vk, mods, repeat: false }, cx);
                    json!({ "ok": true })
                }
                "scan" => {
                    app.start_scan(std::path::PathBuf::from(cmd["path"].as_str().unwrap_or("")), cx);
                    json!({ "ok": true })
                }
                "go" => {
                    let v = match cmd["view"].as_str().unwrap_or("") {
                        "map" => View::Map,
                        "cleanup" => View::Cleanup,
                        "settings" => View::Settings,
                        _ => View::Drives,
                    };
                    app.go(v, cx);
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
                "wheel" => {
                    let r = if app.review.is_some() {
                        app.review_rect
                    } else if app.view == View::Map {
                        app.list_rect
                    } else {
                        app.clean_rect
                    };
                    let p = pt(cmd["x"].as_f64().map_or(r.x + r.w / 2.0, |v| v as f32), cmd["y"].as_f64().map_or(r.y + r.h / 2.0, |v| v as f32));
                    let dy = cmd["dy"].as_f64().unwrap_or(-1.0) as f32;
                    app.event(Event::Wheel { p, dx: 0.0, dy, mods: Mods::default() }, cx);
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
    });
    if !sent {
        return json!({ "error": "window closed" }).to_string();
    }
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn state(app: &mut App) -> Value {
    let tree = app.tree.as_ref().map(|t| {
        json!({
            "root": t.root.to_string_lossy(),
            "entries": t.len(),
            "size": t.node(0).size,
            "files": t.node(0).files,
            "millis": t.millis,
            "cur": t.path(app.cur).to_string_lossy(),
            "cur_size": t.node(app.cur).size,
        })
    });
    let rows: Vec<Value> = app
        .tree
        .as_ref()
        .map(|t| app.rows.iter().take(20).map(|&id| json!({ "name": t.name(id), "size": t.node(id).size, "dir": t.node(id).is_dir() })).collect())
        .unwrap_or_default();
    let sugg: Vec<Value> = app
        .sugg
        .iter()
        .map(|s| {
            json!({
                "kind": s.kind.key(),
                "size": s.size,
                "count": s.count,
                "selected": s.selected,
                "selected_size": s.selected_size(),
                "blocked": s.blocked.as_ref().map(|b| format!("{b:?}")),
                "items": s.items.iter().take(50).map(|i| json!({ "path": i.path.to_string_lossy(), "size": i.size, "selected": i.selected, "group": i.group })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "view": format!("{:?}", app.view),
        "scanning": app.scanning.is_some(),
        "tree": tree,
        "mode": format!("{:?}", app.mode),
        "order": format!("{:?}", app.order),
        "rows": rows,
        "sel": app.sel,
        "measuring": app.measuring,
        "dups": format!("{:?}", app.dups),
        "cleaning": app.cleaning,
        "sandbox": app.places.sandbox.as_ref().map(|s| s.to_string_lossy().to_string()),
        "elevated": app.elevated,
        "suggestions": sugg,
        "selected_total": app.selected_total(),
        "review": app.review,
        "result": app.result.as_ref().map(|o| json!({ "freed": o.freed, "removed": o.removed, "failed": o.failed, "refused": o.refused })),
        "modal": app.modal.as_ref().map(|m| format!("{m:?}")),
        "menu": app.menu.as_ref().map(|m| m.items.iter().map(|i| i.0.clone()).collect::<Vec<_>>()),
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "drives": app.drives.iter().map(|d| json!({ "root": d.root.to_string_lossy(), "total": d.total, "free": d.free })).collect::<Vec<_>>(),
        "settings": serde_json::to_value(&app.set).unwrap_or_default(),
        "dark": app.pal.dark,
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
    })
}
