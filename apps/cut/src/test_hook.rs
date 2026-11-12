//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_CUT_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                                   screen, project (as saved), selection, playhead, export…
//!   {"cmd":"hits"}                                    clickable elements and their rectangles
//!   {"cmd":"click","id":"Tool(\"split\")"}            a click on an element (its name as `hits` gives it)
//!   {"cmd":"new"}  {"cmd":"open","path":"…"}  {"cmd":"home"}
//!   {"cmd":"import","paths":[…],"append":true}        files into the bin (and the timeline)
//!   {"cmd":"add","media":7,"track":2,"at":90}         a media on a track at a frame
//!   {"cmd":"music","media":9}                         a sound on the music track at 0
//!   {"cmd":"text","at":30,"text":"Hello","y":0.3}     a text clip (other fields of TextStyle may follow)
//!   {"cmd":"select","ids":[12]}  {"cmd":"seek","frame":45}  {"cmd":"split"}  {"cmd":"delete","ripple":true}
//!   {"cmd":"trim","id":12,"start":30} / "end":…     {"cmd":"move","ids":[12],"delta":30,"shift":0}
//!   {"cmd":"set","id":12,"clip":{"speed":2}}          fields of a clip (JSON merge)
//!   {"cmd":"project","set":{"duck":true}}             fields of the project
//!   {"cmd":"transition","id":12,"kind":"crossfade","len":20}
//!   {"cmd":"undo"} {"cmd":"redo"} {"cmd":"save"} {"cmd":"play"} {"cmd":"stop"}
//!   {"cmd":"key","vk":83,"ctrl":false,"shift":false}
//!   {"cmd":"export","res":1080,"aspect":"9:16","codec":"h264","hw":false}
//!   {"cmd":"dialog","name":"export"|"help"|"settings"|null}  {"cmd":"tab","bin":"text"}  {"cmd":"prop_tab","i":1}
//!   {"cmd":"setting","key":"theme","value":"light"}  {"cmd":"repaint"}  {"cmd":"zoom","fit":true}

use crate::{
    model::{TextStyle, TransKind, Transition},
    ui::{App, BinTab, Dialog, ExportState, Screen},
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
    let Some(port) = std::env::var("PLUME_CUT_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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

fn ids(v: &Value) -> Vec<u64> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_u64()).collect()).unwrap_or_default()
}

fn merge(base: &mut Value, patch: &Value) {
    if let (Some(b), Some(p)) = (base.as_object_mut(), patch.as_object()) {
        for (k, v) in p {
            b.insert(k.clone(), v.clone());
        }
    }
}

fn run(sender: &Sender, line: &str) -> String {
    let Ok(cmd) = serde_json::from_str::<Value>(line) else { return json!({ "error": "bad json" }).to_string() };
    let (tx, rx) = mpsc::channel();
    let sent = sender.send(move |h, cx| {
        let out = match h.as_any().downcast_mut::<App>() {
            Some(app) => command(app, &cmd, cx),
            None => json!({ "error": "no app" }),
        };
        cx.repaint();
        let _ = tx.send(out.to_string());
    });
    if !sent {
        return json!({ "error": "window closed" }).to_string();
    }
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn command(app: &mut App, cmd: &Value, cx: &mut plume_ui::Cx) -> Value {
    let ok = json!({ "ok": true });
    match cmd["cmd"].as_str().unwrap_or("") {
        "state" => state(app),
        "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
        "click" => click(app, cmd["id"].as_str().unwrap_or(""), cx),
        // a mouse drag from an element (at `fx`, `fy` of its box, the centre by default) by dx, dy, in steps
        "drag" => {
            let want = cmd["id"].as_str().unwrap_or("").to_string();
            let Some(r) = app.hits.rect_of(|id| format!("{id:?}") == want) else { return json!({ "error": "no such target" }) };
            let p0 = pt(r.x + r.w * cmd["fx"].as_f64().unwrap_or(0.5) as f32, r.y + r.h * cmd["fy"].as_f64().unwrap_or(0.5) as f32);
            let (dx, dy) = (cmd["dx"].as_f64().unwrap_or(0.0) as f32, cmd["dy"].as_f64().unwrap_or(0.0) as f32);
            let steps = cmd["steps"].as_u64().unwrap_or(6).max(1) as usize;
            let mods = Mods {
                ctrl: cmd["ctrl"].as_bool().unwrap_or(false),
                shift: cmd["shift"].as_bool().unwrap_or(false),
                alt: cmd["alt"].as_bool().unwrap_or(false),
            };
            let clicks = cmd["clicks"].as_u64().unwrap_or(1) as u8;
            app.event(Event::MouseMove { p: p0, mods }, cx);
            app.event(Event::MouseDown { p: p0, button: Button::Left, clicks, mods }, cx);
            for k in 1..=steps {
                let f = k as f32 / steps as f32;
                app.event(Event::MouseMove { p: pt(p0.x + dx * f, p0.y + dy * f), mods }, cx);
            }
            app.event(Event::MouseUp { p: pt(p0.x + dx, p0.y + dy), button: Button::Left, mods }, cx);
            json!({ "ok": true, "from": [p0.x, p0.y] })
        }
        "new" => json!({ "ok": app.new_project(cx), "file": app.file }),
        "open" => {
            app.open_project(&PathBuf::from(cmd["path"].as_str().unwrap_or("")), cx);
            ok
        }
        "home" => {
            app.go_home(cx);
            ok
        }
        "import" => {
            let paths: Vec<PathBuf> = cmd["paths"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).map(PathBuf::from).collect()).unwrap_or_default();
            app.import(paths, cmd["append"].as_bool().unwrap_or(false), cx);
            ok
        }
        "args" => {
            app.open_args(cmd["line"].as_str().unwrap_or(""), cx);
            ok
        }
        "add" => {
            let (m, t, at) = (cmd["media"].as_u64().unwrap_or(0), cmd["track"].as_u64().unwrap_or(0) as usize, cmd["at"].as_i64().unwrap_or(0));
            match app.edit(|p| p.add_media(m, t, at)) {
                Ok(id) => json!({ "ok": true, "id": id }),
                Err(e) => json!({ "error": e }),
            }
        }
        "music" => {
            let m = cmd["media"].as_u64().unwrap_or(0);
            match app.edit(|p| p.add_music(m, cmd["at"].as_i64().unwrap_or(0))) {
                Ok(id) => json!({ "ok": true, "id": id }),
                Err(e) => json!({ "error": e }),
            }
        }
        "text" => {
            let mut st = serde_json::to_value(TextStyle::default()).unwrap_or_default();
            merge(&mut st, cmd);
            let st: TextStyle = serde_json::from_value(st).unwrap_or_default();
            app.playhead = cmd["at"].as_i64().unwrap_or(app.playhead);
            let id = app.add_text(st, cx);
            if let Some(y) = cmd["y"].as_f64() {
                app.edit(|p| {
                    if let Some(c) = p.clip_mut(id) {
                        c.y = y;
                    }
                });
            }
            json!({ "ok": true, "id": id })
        }
        "select" => {
            app.selected = ids(&cmd["ids"]);
            ok
        }
        "seek" => {
            app.seek(cmd["frame"].as_i64().unwrap_or(0), cx);
            ok
        }
        "split" => {
            app.split(cx);
            ok
        }
        "delete" => {
            app.delete(cmd["ripple"].as_bool().unwrap_or(false), cx);
            ok
        }
        "trim" => {
            let id = cmd["id"].as_u64().unwrap_or(0);
            let r = if let Some(s) = cmd["start"].as_i64() {
                app.edit(|p| p.trim_start(id, s))
            } else {
                app.edit(|p| p.trim_end(id, cmd["end"].as_i64().unwrap_or(0)))
            };
            json!({ "ok": r.is_some(), "at": r })
        }
        "move" => {
            let list = ids(&cmd["ids"]);
            match app.edit(|p| p.move_clips(&list, cmd["delta"].as_i64().unwrap_or(0), cmd["shift"].as_i64().unwrap_or(0) as i32)) {
                Ok(()) => ok,
                Err(e) => json!({ "error": e }),
            }
        }
        "set" => {
            let id = cmd["id"].as_u64().unwrap_or(0);
            let Some(c) = app.project.clip(id).cloned() else { return json!({ "error": "no such clip" }) };
            let mut v = serde_json::to_value(&c).unwrap_or_default();
            merge(&mut v, &cmd["clip"]);
            match serde_json::from_value::<crate::model::Clip>(v) {
                Ok(new) => {
                    let speed = new.speed;
                    app.edit(|p| {
                        if let Some(c) = p.clip_mut(id) {
                            *c = crate::model::Clip { speed: c.speed, ..new };
                        }
                        if (speed - c.speed).abs() > 1e-9 {
                            p.set_speed(id, speed);
                        }
                    });
                    ok
                }
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "project" => {
            let mut v = serde_json::to_value(&app.project).unwrap_or_default();
            merge(&mut v, &cmd["set"]);
            match serde_json::from_value::<crate::model::Project>(v) {
                Ok(mut new) => {
                    for (m, old) in new.media.iter_mut().zip(&app.project.media) {
                        m.path = old.path.clone();
                        m.missing = old.missing;
                    }
                    app.edit(|p| *p = new);
                    ok
                }
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "transition" => {
            let id = cmd["id"].as_u64().unwrap_or(0);
            let kind = match cmd["kind"].as_str() {
                Some("crossfade") => Some(TransKind::Crossfade),
                Some("dip") => Some(TransKind::Dip),
                _ => None,
            };
            let len = cmd["len"].as_i64().unwrap_or(15);
            match app.edit(|p| p.set_transition(id, kind.map(|k| Transition { kind: k, len }))) {
                Ok(()) => ok,
                Err(e) => json!({ "error": e }),
            }
        }
        "undo" => {
            app.undo(cx);
            ok
        }
        "redo" => {
            app.redo(cx);
            ok
        }
        "save" => {
            app.dirty = Some(std::time::Instant::now());
            app.save_now();
            json!({ "ok": true, "file": app.file })
        }
        "play" => {
            app.play(cx);
            json!({ "ok": app.player.is_some() })
        }
        "stop" => {
            app.stop_play();
            ok
        }
        "key" => {
            let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: false };
            app.event(Event::Key { vk: cmd["vk"].as_u64().unwrap_or(0) as u16, mods, repeat: false }, cx);
            ok
        }
        "export" => {
            if let Some(r) = cmd["res"].as_u64() {
                app.set.export_res = r as u32;
            }
            if let Some(a) = cmd["aspect"].as_str() {
                app.set.export_aspect = a.to_string();
            }
            if let Some(c) = cmd["codec"].as_str() {
                app.set.export_codec = c.to_string();
            }
            if let Some(h) = cmd["hw"].as_bool() {
                app.set.hw = h;
            }
            app.start_export(cx);
            ok
        }
        "cancel_export" => {
            app.cancel_export();
            ok
        }
        "dialog" => {
            app.dialog = match cmd["name"].as_str() {
                Some("export") => Some(Dialog::Export),
                Some("help") => Some(Dialog::Help),
                Some("settings") => Some(Dialog::Settings),
                _ => None,
            };
            if app.dialog == Some(Dialog::Export) {
                app.check_hw(cx);
            }
            ok
        }
        "tab" => {
            app.bin_tab = match cmd["bin"].as_str() {
                Some("text") => BinTab::Text,
                Some("transitions") => BinTab::Transitions,
                _ => BinTab::Media,
            };
            ok
        }
        "prop_tab" => {
            app.chip("tab", cmd["i"].as_u64().unwrap_or(0) as usize, cx);
            ok
        }
        "setting" => {
            let mut s = serde_json::to_value(&app.set).unwrap_or_default();
            s[cmd["key"].as_str().unwrap_or("")] = cmd["value"].clone();
            match serde_json::from_value(s) {
                Ok(new) => {
                    app.set = new;
                    app.settings_changed(cx);
                    ok
                }
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "zoom" => {
            if cmd["fit"].as_bool() == Some(true) {
                app.fit_zoom();
            }
            if let Some(z) = cmd["zoom"].as_f64() {
                app.zoom = z as f32;
            }
            ok
        }
        "repaint" => ok,
        _ => json!({ "error": "unknown command" }),
    }
}

fn state(app: &mut App) -> Value {
    let file = app.file.clone().unwrap_or_default();
    json!({
        "screen": if app.screen == Screen::Start { "start" } else { "edit" },
        "dialog": app.dialog.map(|d| format!("{d:?}")),
        "file": app.file,
        "project": serde_json::from_str::<Value>(&crate::project::to_json(&app.project, &file)).unwrap_or_default(),
        "duration": app.project.duration(),
        "selected": app.selected,
        "playhead": app.playhead,
        "playing": app.playing(),
        "importing": app.importing,
        "tools": format!("{:?}", app.tools),
        "can_undo": app.history.can_undo(),
        "can_redo": app.history.can_redo(),
        "thumbs": app.thumbs.len(),
        "strips": app.strips.len(),
        "frames_pending": app.cache.pending.len(),
        "export": match &app.export {
            ExportState::Idle => json!({ "state": "idle" }),
            ExportState::Running { frac, note, .. } => json!({ "state": "running", "frac": frac, "note": note }),
            ExportState::Done { path, size } => json!({ "state": "done", "path": path, "size": size }),
            ExportState::Failed(e) => json!({ "state": "failed", "error": e }),
        },
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "dark": app.pal.dark,
        "snap": app.set.snap,
        "zoom": app.zoom,
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
    })
}
