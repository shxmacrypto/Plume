//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_PDF_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                               the window's state
//!   {"cmd":"hits"}                                clickable elements and their rectangles
//!   {"cmd":"click","id":"Btn(\"open\")"}          a click on an element (its name as `hits` gives it)
//!   {"cmd":"rclick","id":"Thumb(2)"}              a right click
//!   {"cmd":"type","text":"…"} / {"cmd":"key","vk":13,"ctrl":false}
//!   {"cmd":"make_samples","dir":"C:\\…"}          writes test PDFs there (text, form, outline, protected)
//!   {"cmd":"open","path":"…"} {"cmd":"password","pw":"…"}
//!   {"cmd":"zoom","z":1.5} {"cmd":"zoom","fit":"width"} {"cmd":"page","n":3} {"cmd":"scroll","y":400}
//!   {"cmd":"search","q":"…"} {"cmd":"next"} {"cmd":"tool","name":"Highlight"}
//!   {"cmd":"drag","page":0,"from":[x,y],"to":[x,y]}  mouse drag over a page (page points)
//!   {"cmd":"place","page":0,"at":[x,y]}            a click on a page (page points)
//!   {"cmd":"mdrag","from":[x,y],"to":[x,y]}        a mouse drag in window coordinates
//!   {"cmd":"wheel","at":[x,y],"dy":1,"ctrl":true}  the mouse wheel; {"cmd":"page_at","at":[x,y]} what is under a point
//!   {"cmd":"set_signature","strokes":[[[x,y],…],…]}
//!   {"cmd":"merge","paths":[…]} {"cmd":"save_as","path":"…"} {"cmd":"extract","pages":[0],"path":"…"}
//!   {"cmd":"split","ranges":"1-2,3","dir":"…"} {"cmd":"rotate","delta":90} {"cmd":"delete"} {"cmd":"select","pages":[1,2]}
//!   {"cmd":"move","pages":[3],"to":0} {"cmd":"fill","name":"…","value":"…"|true} {"cmd":"set","key":"…","value":…}
//!   {"cmd":"print"} {"cmd":"close_tab","i":0,"force":true}

use crate::{
    edit,
    settings::Signature,
    ui::{App, Fit, Tool},
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
    let Some(port) = std::env::var("PLUME_PDF_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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

fn arr2(v: &Value) -> (f32, f32) {
    (v[0].as_f64().unwrap_or(0.0) as f32, v[1].as_f64().unwrap_or(0.0) as f32)
}

fn paths(v: &Value) -> Vec<PathBuf> {
    v.as_array().map(|a| a.iter().filter_map(|p| p.as_str()).map(PathBuf::from).collect()).unwrap_or_default()
}

fn pages(v: &Value) -> Vec<usize> {
    v.as_array().map(|a| a.iter().filter_map(|p| p.as_u64()).map(|p| p as usize).collect()).unwrap_or_default()
}

fn samples(dir: &str) -> Result<Vec<String>, String> {
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let pages: Vec<String> = (1..=8)
        .map(|i| {
            format!(
                "Chapter {i}: The lighthouse keeper\n\nPage {i} of the sample report. The quick brown fox\njumps over the lazy dog near the lighthouse.\nMeasured value: {}\n\nNotes for review: keep the harbour lights on.",
                i * 37
            )
        })
        .collect();
    let refs: Vec<&str> = pages.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    let mut write = |name: &str, doc: &lopdf::Document, pw: Option<&str>| -> Result<(), String> {
        let p = dir.join(name);
        edit::save(doc, &p, pw)?;
        out.push(p.display().to_string());
        Ok(())
    };
    write("report.pdf", &edit::with_outline(edit::sample(&refs)), None)?;
    write("appendix.pdf", &edit::sample(&["Appendix A\nThe lighthouse plans", "Appendix B\nTide tables", "Appendix C\nContacts"]), None)?;
    write("form.pdf", &edit::sample_form(), None)?;
    write("protected.pdf", &edit::sample(&["Private page\nOnly with the password."]), Some("plume"))?;
    Ok(out)
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
                    let text = cmd["text"].as_str().unwrap_or("").to_string();
                    app.win.edit_set_text(&text);
                    app.event(Event::EditChanged(text), cx);
                    json!({ "ok": true })
                }
                "key" => {
                    let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
                    let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: false };
                    if (app.win.edit_visible() || app.edit_for.is_some()) && matches!(vk, 0x0D | 0x1B) {
                        app.event(Event::EditKey { vk, mods }, cx);
                    } else {
                        app.event(Event::Key { vk, mods, repeat: false }, cx);
                    }
                    json!({ "ok": true })
                }
                "make_samples" => match samples(cmd["dir"].as_str().unwrap_or("")) {
                    Ok(files) => json!({ "ok": true, "files": files }),
                    Err(e) => json!({ "error": e }),
                },
                "make_showcase" => {
                    let dir = PathBuf::from(cmd["dir"].as_str().unwrap_or(""));
                    let _ = std::fs::create_dir_all(&dir);
                    let (a, b) = (dir.join("Annual report 2026.pdf"), dir.join("Frequent traveller card.pdf"));
                    match edit::save(&crate::samples::report(), &a, None).and_then(|_| edit::save(&crate::samples::form(), &b, None)) {
                        Ok(()) => json!({ "ok": true, "files": [a, b] }),
                        Err(e) => json!({ "error": e }),
                    }
                }
                "open" => {
                    app.open_paths(vec![PathBuf::from(cmd["path"].as_str().unwrap_or(""))], cx);
                    json!({ "ok": true })
                }
                "password" => {
                    app.modal_text = cmd["pw"].as_str().unwrap_or("").to_string();
                    app.modal_ok(cx);
                    json!({ "ok": true })
                }
                "zoom" => {
                    match cmd["fit"].as_str() {
                        Some("width") => app.set_fit(Fit::Width, cx),
                        Some("page") => app.set_fit(Fit::Page, cx),
                        _ => {
                            let z = cmd["z"].as_f64().unwrap_or(1.0) as f32;
                            app.zoom_to(z, None, cx);
                        }
                    }
                    json!({ "ok": true, "zoom": app.tab().map(|t| t.zoom) })
                }
                "page" => {
                    app.go_page(cmd["n"].as_u64().unwrap_or(0) as usize, cx);
                    json!({ "ok": true, "page": app.current_page() })
                }
                "scroll" => {
                    let y = cmd["y"].as_f64().unwrap_or(0.0) as f32;
                    let cur = app.tab().map(|t| t.sy).unwrap_or(0.0);
                    app.scroll_by(0.0, y - cur, cx);
                    json!({ "ok": true })
                }
                "search" => {
                    app.search_open = true;
                    app.query = cmd["q"].as_str().unwrap_or("").to_string();
                    app.run_search(true, cx);
                    json!({ "ok": true, "found": app.tab().map(|t| t.found.len()) })
                }
                "next" => {
                    app.next_found(cmd["back"].as_bool().unwrap_or(false), cx);
                    json!({ "ok": true, "i": app.tab().map(|t| t.found_i), "page": app.current_page() })
                }
                "tool" => {
                    app.tool = match cmd["name"].as_str().unwrap_or("") {
                        "Highlight" => Tool::Highlight,
                        "Note" => Tool::Note,
                        "Ink" => Tool::Ink,
                        "Text" => Tool::Text,
                        "Sign" => Tool::Sign,
                        _ => Tool::Hand,
                    };
                    cx.repaint();
                    json!({ "ok": true })
                }
                "drag" | "place" => {
                    let page = cmd["page"].as_u64().unwrap_or(0) as usize;
                    let (a, b) = if cmd["cmd"] == "drag" { (arr2(&cmd["from"]), arr2(&cmd["to"])) } else { (arr2(&cmd["at"]), arr2(&cmd["at"])) };
                    let mods = Mods::default();
                    let pa = app.to_screen(page, a.0, a.1);
                    if !app.view.contains(pa) {
                        return_json(&tx, json!({ "error": "point not on screen", "screen": [pa.x, pa.y] }));
                        return;
                    }
                    app.event(Event::MouseMove { p: pa, mods }, cx);
                    app.event(Event::MouseDown { p: pa, button: Button::Left, clicks: 1, mods }, cx);
                    let steps = 8;
                    for s in 1..=steps {
                        let t = s as f32 / steps as f32;
                        let q = app.to_screen(
                            page,
                            a.0 + (b.0 - a.0) * t,
                            a.1 + (b.1 - a.1) * t + (t * std::f32::consts::PI).sin() * cmd["wave"].as_f64().unwrap_or(0.0) as f32,
                        );
                        app.event(Event::MouseMove { p: q, mods }, cx);
                    }
                    let pb = app.to_screen(page, b.0, b.1);
                    app.event(Event::MouseUp { p: pb, button: Button::Left, mods }, cx);
                    json!({ "ok": true, "annots": app.tab().map(|t| t.annots.len()), "modal": app.modal.as_ref().map(|m| format!("{m:?}")) })
                }
                "mdrag" => {
                    // a mouse drag in window coordinates (thumbnails, scrollbar…)
                    let (a, b) = (arr2(&cmd["from"]), arr2(&cmd["to"]));
                    let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: false, alt: false };
                    app.event(Event::MouseMove { p: pt(a.0, a.1), mods }, cx);
                    app.event(Event::MouseDown { p: pt(a.0, a.1), button: Button::Left, clicks: 1, mods }, cx);
                    for s in 1..=10 {
                        let t = s as f32 / 10.0;
                        app.event(Event::MouseMove { p: pt(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t), mods }, cx);
                    }
                    app.event(Event::MouseUp { p: pt(b.0, b.1), button: Button::Left, mods }, cx);
                    json!({ "ok": true })
                }
                "wheel" => {
                    let (x, y) = arr2(&cmd["at"]);
                    let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: false };
                    app.event(Event::Wheel { p: pt(x, y), dx: 0.0, dy: cmd["dy"].as_f64().unwrap_or(1.0) as f32, mods }, cx);
                    json!({ "ok": true, "zoom": app.tab().map(|t| t.zoom), "page_at": app.page_at(pt(x, y)).map(|(i, (px, py))| json!([i, px, py])) })
                }
                "page_at" => {
                    let (x, y) = arr2(&cmd["at"]);
                    json!({ "ok": true, "page_at": app.page_at(pt(x, y)).map(|(i, (px, py))| json!([i, px, py])) })
                }
                "set_signature" => {
                    let strokes: Vec<Vec<(f32, f32)>> = cmd["strokes"]
                        .as_array()
                        .map(|a| a.iter().map(|s| s.as_array().map(|s| s.iter().map(arr2).collect()).unwrap_or_default()).collect())
                        .unwrap_or_default();
                    match Signature::from_pad(&strokes) {
                        Some(s) => {
                            s.save();
                            app.sig = Some(s);
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no strokes" }),
                    }
                }
                "merge" => {
                    app.merge_paths(paths(&cmd["paths"]), cx);
                    json!({ "ok": true, "pages": app.tab().map(|t| t.pages()) })
                }
                "save_as" => json!({ "ok": app.save_to(PathBuf::from(cmd["path"].as_str().unwrap_or("")), cx) }),
                "extract" => json!({ "ok": app.extract_to(&pages(&cmd["pages"]), PathBuf::from(cmd["path"].as_str().unwrap_or("")), cx) }),
                "split" => {
                    let n = app.split_to(cmd["ranges"].as_str().unwrap_or(""), &PathBuf::from(cmd["dir"].as_str().unwrap_or("")), cx);
                    json!({ "ok": n > 0, "files": n })
                }
                "rotate" => {
                    app.rotate(cmd["delta"].as_i64().unwrap_or(90) as i32, cx);
                    json!({ "ok": true })
                }
                "delete" => {
                    app.delete_pages(cx);
                    json!({ "ok": true, "pages": app.tab().map(|t| t.pages()) })
                }
                "select" => {
                    let sel = pages(&cmd["pages"]);
                    if let Some(t) = app.tab_mut() {
                        t.sel = sel;
                    }
                    cx.repaint();
                    json!({ "ok": true })
                }
                "move" => {
                    app.move_pages(pages(&cmd["pages"]), cmd["to"].as_u64().unwrap_or(0) as usize, cx);
                    json!({ "ok": true })
                }
                "fill" => {
                    let name = cmd["name"].as_str().unwrap_or("");
                    let i = app.tab().and_then(|t| t.fields.iter().position(|f| f.name == name));
                    match i {
                        Some(i) => {
                            let v = match &cmd["value"] {
                                Value::Bool(b) => edit::FieldValue::Check(*b),
                                v => edit::FieldValue::Text(v.as_str().unwrap_or("").to_string()),
                            };
                            app.set_field(i, v, cx);
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no such field" }),
                    }
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
                "print" => {
                    app.print(cx);
                    json!({ "ok": true })
                }
                "close_tab" => {
                    let i = cmd["i"].as_u64().unwrap_or(0) as usize;
                    app.close_tab(i, cmd["force"].as_bool().unwrap_or(false), cx);
                    json!({ "ok": true, "tabs": app.tabs.len() })
                }
                "switch" => {
                    app.switch_to(cmd["i"].as_u64().unwrap_or(0) as usize, cx);
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
    rx.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn return_json(tx: &mpsc::Sender<String>, v: Value) {
    let _ = tx.send(v.to_string());
}

fn state(app: &mut App) -> Value {
    let cur_page = app.current_page();
    let tabs: Vec<Value> = app
        .tabs
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "path": t.path,
                "status": format!("{:?}", t.status),
                "pages": t.pages(),
                "editable": t.editable(),
                "readonly": t.readonly,
                "dirty": t.dirty,
                "zoom": t.zoom,
                "fit": format!("{:?}", t.fit),
                "sx": t.sx, "sy": t.sy,
                "ver": t.ver,
                "texts_ready": t.texts_ready,
                "chars": t.texts.iter().map(|x| x.chars.len()).sum::<usize>(),
                "outline": t.outline.iter().map(|o| json!({ "title": o.title, "page": o.page, "level": o.level })).collect::<Vec<_>>(),
                "fields": t.fields.iter().map(|f| json!({ "name": f.name, "check": f.check, "value": f.value, "on": f.on })).collect::<Vec<_>>(),
                "pending_values": t.values.len(),
                "annots": t.annots.iter().map(|a| json!({ "kind": format!("{:?}", a.kind), "page": a.page, "rect": a.rect, "text": a.text })).collect::<Vec<_>>(),
                "file_annots": t.doc.as_ref().map(edit::read_annots).unwrap_or_default(),
                "rotations": t.geoms.iter().map(|g| g.rotate).collect::<Vec<_>>(),
                "page_texts": t.texts.iter().map(|x| x.string().lines().next().unwrap_or("").to_string()).collect::<Vec<_>>(),
                "sel": t.sel,
                "found": t.found.len(),
                "found_i": t.found_i,
                "undo": t.undo.len(),
            })
        })
        .collect();
    json!({
        "tabs": tabs,
        "cur": app.cur,
        "page": cur_page,
        "tool": format!("{:?}", app.tool),
        "selected": app.selected,
        "modal": app.modal.as_ref().map(|m| format!("{m:?}")),
        "menu": app.menu.as_ref().map(|m| m.items.iter().map(|i| i.0.clone()).collect::<Vec<_>>()),
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "search_open": app.search_open,
        "query": app.query,
        "settings_open": app.settings_open,
        "settings": serde_json::to_value(&app.set).unwrap_or_default(),
        "signature": app.sig.is_some(),
        "edit_for": app.edit_for.map(|e| format!("{e:?}")),
        "inflight": app.inflight.len(),
        "render_pending": app.renderer.pending(),
        "view": [app.view.x, app.view.y, app.view.w, app.view.h],
        "dark": app.pal.dark,
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
    })
}
