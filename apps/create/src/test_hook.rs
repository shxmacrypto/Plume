//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_CREATE_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                              the window's state (document, layers, tool, view…)
//!   {"cmd":"hits"}                               clickable elements and their rectangles
//!   {"cmd":"click","id":"Tool(Crop)"}            a click on an element (as `hits` names it); "clicks":2 doubles
//!   {"cmd":"move","x":100,"y":200}               the pointer moves there
//!   {"cmd":"drag","x":..,"y":..,"x2":..,"y2":..} a drag with the left button ("shift"/"ctrl"/"alt")
//!   {"cmd":"key","vk":67,"ctrl":true}            a key
//!   {"cmd":"wheel","x":..,"y":..,"dy":1}         the wheel
//!   {"cmd":"type","text":"Hello"}                text typed in the open field, then Enter (multi-line: closes it)
//!   {"cmd":"open","path":"C:\\…"}                opens a photo or a project, as Explorer would
//!   {"cmd":"place","path":"C:\\…"}               puts a photo in as a layer
//!   {"cmd":"template","key":"story-sale"}        starts from a template; {"cmd":"preset","key":"story"}
//!   {"cmd":"slider","sl":"Adj(0)","value":40}    sets a slider's value (as shown), one undo step
//!   {"cmd":"export","path":"…","format":"Jpeg","w":1080,"quality":90}   writes the picture (never over a file)
//!   {"cmd":"save","path":"…"}                    saves the project there
//!   {"cmd":"pixel","x":10,"y":20}                the document's colour at a point
//!   {"cmd":"doc_to_win","x":..,"y":..}           where a document point is in the window
//!   {"cmd":"repaint"}

use crate::{
    engine::{doc::Kind, project},
    ui::{App, Sl},
    wic::Format,
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
    let Some(port) = std::env::var("PLUME_CREATE_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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

fn click(app: &mut App, want: &str, clicks: u8, mods: Mods, cx: &mut plume_ui::Cx) -> Value {
    match app.hits.rect_of(|id| format!("{id:?}") == want) {
        Some(r) => {
            let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
            app.event(Event::MouseMove { p, mods }, cx);
            for k in 1..=clicks.max(1) {
                app.event(Event::MouseDown { p, button: Button::Left, clicks: k, mods }, cx);
                app.event(Event::MouseUp { p, button: Button::Left, mods }, cx);
            }
            json!({ "ok": true, "at": [p.x, p.y] })
        }
        None => json!({ "error": "no such target" }),
    }
}

fn slider_of(name: &str) -> Option<Sl> {
    Some(match name {
        "BrushSize" => Sl::BrushSize,
        "BrushHard" => Sl::BrushHard,
        "BrushOpacity" => Sl::BrushOpacity,
        "FillTol" => Sl::FillTol,
        "CutoutTol" => Sl::CutoutTol,
        "Opacity" => Sl::Opacity,
        "Rotation" => Sl::Rotation,
        "LookAmount" => Sl::LookAmount,
        "TextSize" => Sl::TextSize,
        "TextSpacing" => Sl::TextSpacing,
        "TextLine" => Sl::TextLine,
        "OutlineW" => Sl::OutlineW,
        "ShadowBlur" => Sl::ShadowBlur,
        "ShadowY" => Sl::ShadowY,
        "StrokeW" => Sl::StrokeW,
        "Radius" => Sl::Radius,
        "GradAngle" => Sl::GradAngle,
        "Straighten" => Sl::Straighten,
        "ExportQuality" => Sl::ExportQuality,
        s if s.starts_with("Adj(") => Sl::Adj(s.trim_start_matches("Adj(").trim_end_matches(')').parse().ok()?),
        _ => return None,
    })
}

fn run(sender: &Sender, line: &str) -> String {
    let Ok(cmd) = serde_json::from_str::<Value>(line) else { return json!({ "error": "bad json" }).to_string() };
    let (tx, rx) = mpsc::channel();
    let sent = sender.send(move |h, cx| {
        let out = match h.as_any().downcast_mut::<App>() {
            Some(app) => {
                let b = |k: &str| cmd[k].as_bool().unwrap_or(false);
                let f = |k: &str| cmd[k].as_f64().unwrap_or(0.0) as f32;
                let mods = Mods { ctrl: b("ctrl"), shift: b("shift"), alt: b("alt") };
                let r = match cmd["cmd"].as_str().unwrap_or("") {
                    "state" => state(app),
                    "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
                    "click" => click(app, cmd["id"].as_str().unwrap_or(""), cmd["clicks"].as_u64().unwrap_or(1) as u8, mods, cx),
                    "move" => {
                        app.event(Event::MouseMove { p: pt(f("x"), f("y")), mods }, cx);
                        json!({ "ok": true })
                    }
                    "drag" => {
                        let (a, z) = (pt(f("x"), f("y")), pt(f("x2"), f("y2")));
                        let steps = cmd["steps"].as_u64().unwrap_or(12).max(1);
                        app.event(Event::MouseMove { p: a, mods }, cx);
                        app.event(Event::MouseDown { p: a, button: Button::Left, clicks: 1, mods }, cx);
                        for k in 1..=steps {
                            let t = k as f32 / steps as f32;
                            app.event(Event::MouseMove { p: pt(a.x + (z.x - a.x) * t, a.y + (z.y - a.y) * t), mods }, cx);
                        }
                        app.event(Event::MouseUp { p: z, button: Button::Left, mods }, cx);
                        json!({ "ok": true })
                    }
                    "key" => {
                        app.event(Event::Key { vk: cmd["vk"].as_u64().unwrap_or(0) as u16, mods, repeat: false }, cx);
                        json!({ "ok": true })
                    }
                    "wheel" => {
                        app.event(Event::Wheel { p: pt(f("x"), f("y")), dx: 0.0, dy: cmd["dy"].as_f64().unwrap_or(1.0) as f32, mods }, cx);
                        json!({ "ok": true })
                    }
                    "type" => {
                        let text = cmd["text"].as_str().unwrap_or("").to_string();
                        if app.field.is_none() {
                            json!({ "error": "no field open" })
                        } else {
                            app.win.edit_set_text(&text);
                            app.event(Event::EditChanged(text), cx);
                            let multi = app.field_is_multiline();
                            app.event(Event::EditKey { vk: 0x0D, mods: Mods { ctrl: multi, shift: false, alt: false } }, cx);
                            json!({ "ok": true })
                        }
                    }
                    "open" | "place" => {
                        let p = PathBuf::from(cmd["path"].as_str().unwrap_or(""));
                        let ok = if cmd["cmd"] == "open" { app.open_path(&p, cx) } else { app.place_path(&p, cx) };
                        if ok { json!({ "ok": true }) } else { json!({ "error": "could not open" }) }
                    }
                    "template" => {
                        if app.start_template(cmd["key"].as_str().unwrap_or("")) {
                            json!({ "ok": true })
                        } else {
                            json!({ "error": "no such template" })
                        }
                    }
                    "preset" => match crate::engine::templates::preset(cmd["key"].as_str().unwrap_or("")) {
                        Some(p) => {
                            app.new_design(p.w, p.h, p.name);
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no such preset" }),
                    },
                    "slider" => match slider_of(cmd["sl"].as_str().unwrap_or("")) {
                        Some(sl) => {
                            let before = app.doc.clone();
                            app.slider_set(sl, f("value"));
                            if App::slider_on_doc(sl) {
                                app.commit(before, "slider");
                            }
                            let (v, ..) = app.slider_get(sl);
                            json!({ "ok": true, "value": v })
                        }
                        None => json!({ "error": "no such slider" }),
                    },
                    "export" => {
                        let format = match cmd["format"].as_str().unwrap_or("Png") {
                            "Jpeg" | "jpeg" | "jpg" => Format::Jpeg,
                            "Webp" | "webp" => Format::Webp,
                            _ => Format::Png,
                        };
                        let w = cmd["w"].as_u64().map(|v| v as u32).unwrap_or(app.doc.w);
                        let q = cmd["quality"].as_u64().unwrap_or(90) as u32;
                        match app.export_to(&PathBuf::from(cmd["path"].as_str().unwrap_or("")), format, w, q) {
                            Ok(p) => json!({ "ok": true, "path": p.display().to_string() }),
                            Err(e) => json!({ "error": e }),
                        }
                    }
                    "save" => {
                        let p = PathBuf::from(cmd["path"].as_str().unwrap_or(""));
                        match project::save(&app.doc, &p) {
                            Ok(()) => json!({ "ok": true }),
                            Err(e) => json!({ "error": e }),
                        }
                    }
                    "pixel" => {
                        let c = app.renderer.pick(&app.doc, f("x"), f("y"));
                        json!({ "rgba": c })
                    }
                    "doc_to_win" => {
                        let p = app.to_win(f("x"), f("y"));
                        json!({ "x": p.x, "y": p.y })
                    }
                    "repaint" => json!({ "ok": true }),
                    _ => json!({ "error": "unknown command" }),
                };
                cx.repaint();
                r
            }
            None => json!({ "error": "no app" }),
        };
        let _ = tx.send(out.to_string());
    });
    if !sent {
        return json!({ "error": "window closed" }).to_string();
    }
    rx.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn state(app: &mut App) -> Value {
    let layers: Vec<Value> = app
        .doc
        .all()
        .iter()
        .map(|l| {
            let kind = match &l.kind {
                Kind::Image(i) => json!({ "image": [i.px.w, i.px.h] }),
                Kind::Text(t) => json!({ "text": t.text, "font": t.font, "size": t.size, "weight": t.weight }),
                Kind::Shape(s) => json!({ "shape": format!("{:?}", s.kind) }),
                Kind::Group(c) => json!({ "group": c.len() }),
            };
            json!({
                "id": l.id, "name": l.name, "visible": l.visible, "locked": l.locked, "opacity": l.opacity,
                "blend": format!("{:?}", l.blend), "xf": [l.xf.cx, l.xf.cy, l.xf.w, l.xf.h, l.xf.rot],
                "look": format!("{:?}", l.adj.look), "adjusted": !l.adj.is_identity(), "mask": l.mask.is_some(), "kind": kind,
            })
        })
        .collect();
    let selection = app.selection.as_ref().map(|s| s.bounds()).map(|b| [b.0, b.1, b.2, b.3]);
    let stage = app.stage();
    json!({
        "editing": app.editing,
        "doc": [app.doc.w, app.doc.h],
        "name": app.name,
        "layers": layers,
        "sel": app.sel,
        "tool": format!("{:?}", app.tool),
        "tab": format!("{:?}", app.tab),
        "selection": selection,
        "crop": app.crop.map(|c| [c.cx, c.cy, c.w, c.h, c.angle]),
        "history": [app.hist.len().0, app.hist.len().1],
        "ver": app.ver,
        "dirty": app.dirty,
        "view": [app.view.zoom, app.view.ox, app.view.oy],
        "stage": [stage.x, stage.y, stage.w, stage.h],
        "dialog": app.dialog.as_ref().map(|d| format!("{d:?}")),
        "popup": app.popup.as_ref().map(|p| format!("{p:?}").chars().take(80).collect::<String>()),
        "field": app.field.as_ref().map(|f| format!("{f:?}")),
        "toast": app.toast.as_ref().map(|t| t.0.clone()),
        "colour": app.colour.to_hex(),
        "fonts": app.fonts.len(),
        "render_ms": app.last_render_ms,
        "frames": app.frames,
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
        "pid": std::process::id(),
    })
}
