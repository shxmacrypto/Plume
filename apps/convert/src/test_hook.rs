//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_CONVERT_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                              the window's state (queue, preset, options, settings)
//!   {"cmd":"hits"}                               clickable elements and their rectangles
//!   {"cmd":"click","id":"Preset(Gif)"}           a click on an element (its name as `hits` gives it)
//!   {"cmd":"add","paths":["C:\\…\\a.mp4"]}       files join the queue (as a drop would)
//!   {"cmd":"preset","name":"discord"}            picks a preset
//!   {"cmd":"opt","key":"discord_mb","value":10}  sets an option of the presets
//!   {"cmd":"set","key":"hw","value":false}       sets a setting
//!   {"cmd":"start"}                              "Convert"
//!   {"cmd":"cancel","id":3} / {"cmd":"cancel"}   cancels one file, or all
//!   {"cmd":"retry","id":3}  {"cmd":"remove","id":3}  {"cmd":"clear"}
//!   {"cmd":"go","view":"settings"|"main"}        {"cmd":"type","text":"0:02"}  {"cmd":"repaint"}

use crate::{
    opts::Preset,
    ui::{App, State, View},
};
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
    let Some(port) = std::env::var("PLUME_CONVERT_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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

fn click(app: &mut App, want: &str, cx: &mut plume_ui::Cx) -> Value {
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

fn run(sender: &Sender, line: &str) -> String {
    let Ok(cmd) = serde_json::from_str::<Value>(line) else { return json!({ "error": "bad json" }).to_string() };
    let (tx, rx) = mpsc::channel();
    let sent = sender.send(move |h, cx| {
        let out = match h.as_any().downcast_mut::<App>() {
            Some(app) => match cmd["cmd"].as_str().unwrap_or("") {
                "state" => state(app),
                "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
                "click" => click(app, cmd["id"].as_str().unwrap_or(""), cx),
                "add" => {
                    let paths: Vec<PathBuf> =
                        cmd["paths"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).map(PathBuf::from).collect()).unwrap_or_default();
                    app.add_paths(paths, cx);
                    json!({ "ok": true, "items": app.items.len() })
                }
                "preset" => match Preset::from_key(cmd["name"].as_str().unwrap_or("")) {
                    Some(p) => {
                        app.act(crate::ui::Hit::Preset(p), cx);
                        json!({ "ok": true })
                    }
                    None => json!({ "error": "no such preset" }),
                },
                "opt" => {
                    let mut o = serde_json::to_value(&app.set.opts).unwrap_or_default();
                    o[cmd["key"].as_str().unwrap_or("")] = cmd["value"].clone();
                    match serde_json::from_value(o) {
                        Ok(new) => {
                            app.set.opts = new;
                            app.trim_text = (
                                crate::opts::clock(app.set.opts.trim_start, true),
                                if app.set.opts.trim_end > 0.0 { crate::opts::clock(app.set.opts.trim_end, true) } else { String::new() },
                            );
                            cx.repaint();
                            json!({ "ok": true })
                        }
                        Err(e) => json!({ "error": e.to_string() }),
                    }
                }
                "set" => {
                    let mut s = serde_json::to_value(&app.set).unwrap_or_default();
                    s[cmd["key"].as_str().unwrap_or("")] = cmd["value"].clone();
                    match serde_json::from_value(s) {
                        Ok(new) => {
                            app.set = new;
                            app.settings_changed(cx);
                            json!({ "ok": true })
                        }
                        Err(e) => json!({ "error": e.to_string() }),
                    }
                }
                "start" => {
                    app.start(cx);
                    json!({ "ok": true })
                }
                "cancel" => {
                    match cmd["id"].as_u64() {
                        Some(id) => app.cancel(id, cx),
                        None => app.cancel_all(cx),
                    }
                    json!({ "ok": true })
                }
                "retry" => {
                    app.retry(cmd["id"].as_u64().unwrap_or(0), cx);
                    json!({ "ok": true })
                }
                "remove" => {
                    app.remove(cmd["id"].as_u64().unwrap_or(0), cx);
                    json!({ "ok": true })
                }
                "clear" => {
                    app.clear_done(cx);
                    json!({ "ok": true })
                }
                "go" => {
                    app.view = if cmd["view"].as_str() == Some("settings") { View::Settings } else { View::Main };
                    app.left_scroll = 0.0;
                    if app.view == View::Settings {
                        app.check_hw();
                    }
                    cx.repaint();
                    json!({ "ok": true })
                }
                "type" => {
                    let text = cmd["text"].as_str().unwrap_or("").to_string();
                    app.win.edit_set_text(&text);
                    app.event(Event::EditChanged(text), cx);
                    json!({ "ok": true })
                }
                "scroll" => {
                    app.queue_scroll = cmd["y"].as_f64().unwrap_or(0.0) as f32;
                    cx.repaint();
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
    json!({
        "view": format!("{:?}", app.view),
        "preset": app.set.preset.key(),
        "opts": serde_json::to_value(&app.set.opts).unwrap_or_default(),
        "settings": serde_json::to_value(&app.set).unwrap_or_default(),
        "tools": format!("{:?}", app.tools),
        "hw": app.hw_label,
        "menu_on": app.menu_on,
        "running": app.running.as_ref().map(|r| r.0),
        "trim_text": [app.trim_text.0, app.trim_text.1],
        "edit_for": app.edit_for,
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "dark": app.pal.dark,
        "items": app.items.iter().map(|i| json!({
            "id": i.id,
            "src": i.src,
            "state": match &i.state { State::Failed(_) => "Failed".to_string(), s => format!("{s:?}") },
            "error": match &i.state { State::Failed(e) => Some(e.clone()), _ => None },
            "frac": i.frac,
            "note": i.note,
            "out": i.out,
            "out_size": i.out_size,
            "preset": i.spec.as_ref().map(|s| s.preset.key()),
        })).collect::<Vec<_>>(),
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
    })
}
