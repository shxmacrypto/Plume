//! Test socket: a script drives both windows without the user's mouse, keyboard or clipboard. Only in a
//! `--features test-hook` build, and only when PLUME_CLIPBOARD_TEST_PORT is set. One JSON command per line on
//! 127.0.0.1, one JSON answer per line. `"win":"panel"` sends a command to the panel instead of the main window.
//!   {"cmd":"state"}  {"cmd":"state","win":"panel"}     the window's state
//!   {"cmd":"hits"}                                       clickable elements and their rectangles
//!   {"cmd":"click","id":"Nav(Snippets)"}                 a click on an element (its name as `hits` gives it)
//!   {"cmd":"rclick","id":"Row(3)"}                       a right click
//!   {"cmd":"type","text":"…"}                            text in the open field (main) or typed in the panel
//!   {"cmd":"key","vk":13,"shift":true}                   a key
//!   {"cmd":"open_panel","x":600,"y":300,"target":0}      opens the panel at a point of the screen
//!   {"cmd":"add","kind":"text|link|image|files",…}       an item, as if copied (the clipboard is not touched);
//!                                                        "ago": seconds, to backdate it
//!   {"cmd":"snippet","name":"…","trigger":";sig","text":"…"}   {"cmd":"pin","id":3,"on":true}
//!   {"cmd":"capture","text":"…","app":"x.exe","private":true}   the copy decision (private, ignored apps, pause)
//!   {"cmd":"capture_image","w":800,"h":500}              a picture through the real path (DIB → PNG on a thread)
//!   {"cmd":"roundtrip","id":3}                           the real clipboard: saved, item written and read back,
//!                                                        then put back exactly as it was
//!   {"cmd":"set","key":"theme","value":"light"}          a setting
//!   {"cmd":"go","page":"settings"}  {"cmd":"scroll","y":300}  {"cmd":"show"}  {"cmd":"quit"}

use crate::{
    clip::{self, Captured},
    image,
    store::{self, Entry},
    ui::{App, Page, panel::Panel},
};
use plume_ui::{Button, Cx, Event, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::mpsc,
    time::Duration,
};

pub fn start(main: Window, panel: Window) {
    let Some(port) = std::env::var("PLUME_CLIPBOARD_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
    let (ms, ps) = (main.sender(), panel.sender());
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
                if writeln!(out, "{}", run(&ms, &ps, line.trim())).is_err() {
                    break;
                }
            }
        }
    });
}

fn mods(cmd: &Value) -> Mods {
    Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: false }
}

fn click(h: &mut dyn Handler, rect: Option<plume_ui::Rect>, right: bool, m: Mods, cx: &mut Cx) -> Value {
    let Some(r) = rect else { return json!({ "error": "no such target" }) };
    let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
    let button = if right { Button::Right } else { Button::Left };
    h.event(Event::MouseMove { p, mods: m }, cx);
    h.event(Event::MouseDown { p, button, clicks: 1, mods: m }, cx);
    if !right {
        h.event(Event::MouseUp { p, button, mods: m }, cx);
    }
    json!({ "ok": true })
}

/// Half a click: the button goes down (and stays) or comes back up, to hold the eye of a sensitive item.
fn press(h: &mut dyn Handler, rect: Option<plume_ui::Rect>, down: bool, cx: &mut Cx) -> Value {
    let Some(r) = rect else { return json!({ "error": "no such target" }) };
    let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
    let m = Mods::default();
    if down {
        h.event(Event::MouseMove { p, mods: m }, cx);
        h.event(Event::MouseDown { p, button: Button::Left, clicks: 1, mods: m }, cx);
    } else {
        h.event(Event::MouseUp { p, button: Button::Left, mods: m }, cx);
    }
    json!({ "ok": true })
}

fn run(main: &Sender, panel: &Sender, line: &str) -> String {
    let Ok(cmd) = serde_json::from_str::<Value>(line) else { return json!({ "error": "bad json" }).to_string() };
    let (tx, rx) = mpsc::channel();
    let to_panel = cmd["win"].as_str() == Some("panel");
    let sender = if to_panel { panel } else { main };
    let sent = sender.send(move |h, cx| {
        let out = if to_panel { panel_cmd(h, &cmd, cx) } else { main_cmd(h, &cmd, cx) };
        let _ = tx.send(out.to_string());
    });
    if !sent {
        return json!({ "error": "window closed" }).to_string();
    }
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn panel_cmd(h: &mut dyn Handler, cmd: &Value, cx: &mut Cx) -> Value {
    let Some(p) = h.as_any().downcast_mut::<Panel>() else { return json!({ "error": "no panel" }) };
    match cmd["cmd"].as_str().unwrap_or("") {
        "state" => {
            p.refresh();
            let m = p.model.borrow();
            let list: Vec<Value> = p
                .list
                .iter()
                .map(|e| match e {
                    Entry::Item(id) => {
                        json!({
                            "item": id,
                            "text": m.store.get(*id).map(|i| i.preview(60)),
                            "pin": m.store.get(*id).is_some_and(|i| i.pin.is_some()),
                            "secret": m.store.get(*id).and_then(|i| i.secret).map(|s| format!("{s:?}")),
                            "shown": p.reveal.shown(*id),
                        })
                    }
                    Entry::Snippet(id) => json!({ "snippet": id, "trigger": m.store.snippet(*id).map(|s| s.trigger.clone()) }),
                })
                .collect();
            json!({
                "open": p.open,
                "visible": crate::sys::visible(p.win.hwnd),
                "query": p.query,
                "sel": p.sel,
                "list": list,
                "last": p.last,
                "target": p.target,
                "notice": p.notice,
                "hwnd": p.win.hwnd.0 as isize,
            })
        }
        "hits" => json!(p.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
        "click" | "rclick" => {
            let want = cmd["id"].as_str().unwrap_or("").to_string();
            let r = p.hits.rect_of(|id| format!("{id:?}") == want);
            click(p, r, cmd["cmd"] == "rclick", mods(cmd), cx)
        }
        "press" | "release" => {
            let want = cmd["id"].as_str().unwrap_or("").to_string();
            let r = p.hits.rect_of(|id| format!("{id:?}") == want);
            press(p, r, cmd["cmd"] == "press", cx)
        }
        "type" => {
            for ch in cmd["text"].as_str().unwrap_or("").chars() {
                p.char(ch, cx);
            }
            json!({ "ok": true })
        }
        "key" => {
            p.key(cmd["vk"].as_u64().unwrap_or(0) as u16, mods(cmd), cx);
            json!({ "ok": true })
        }
        "open" => {
            let at = (cmd["x"].as_i64().unwrap_or(500) as i32, cmd["y"].as_i64().unwrap_or(300) as i32);
            p.open(cmd["target"].as_i64().unwrap_or(0) as isize, Some(at), cx);
            json!({ "ok": true })
        }
        _ => json!({ "error": "unknown command" }),
    }
}

fn main_cmd(h: &mut dyn Handler, cmd: &Value, cx: &mut Cx) -> Value {
    let Some(app) = h.as_any().downcast_mut::<App>() else { return json!({ "error": "no app" }) };
    match cmd["cmd"].as_str().unwrap_or("") {
        "state" => state(app),
        "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
        "click" | "rclick" => {
            let want = cmd["id"].as_str().unwrap_or("").to_string();
            let r = app.hits.rect_of(|id| format!("{id:?}") == want);
            click(app, r, cmd["cmd"] == "rclick", mods(cmd), cx)
        }
        "press" | "release" => {
            let want = cmd["id"].as_str().unwrap_or("").to_string();
            let r = app.hits.rect_of(|id| format!("{id:?}") == want);
            press(app, r, cmd["cmd"] == "press", cx)
        }
        "prune" => {
            let mut m = app.model.borrow_mut();
            let gone = m.prune_now();
            if gone {
                m.changed();
            }
            json!({ "removed": gone })
        }
        "type" => {
            let text = cmd["text"].as_str().unwrap_or("").to_string();
            app.win.edit_set_text(&text);
            app.event(Event::EditChanged(text), cx);
            json!({ "ok": app.win.edit_visible() })
        }
        "key" => {
            let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
            if app.win.edit_visible() && matches!(vk, 0x0D | 0x1B | 0x09) {
                app.event(Event::EditKey { vk, mods: mods(cmd) }, cx);
            } else {
                app.event(Event::Key { vk, mods: mods(cmd), repeat: false }, cx);
            }
            json!({ "ok": true })
        }
        "open_panel" => {
            let panel = app.model.borrow().panel;
            let at = (cmd["x"].as_i64().unwrap_or(500) as i32, cmd["y"].as_i64().unwrap_or(300) as i32);
            let target = cmd["target"].as_i64().unwrap_or(0) as isize;
            let ok = panel
                .and_then(|w| {
                    w.with(|h, cx| {
                        if let Some(p) = h.as_any().downcast_mut::<Panel>() {
                            p.open(target, Some(at), cx);
                        }
                    })
                })
                .is_some();
            json!({ "ok": ok })
        }
        "add" => {
            let app_name = cmd["app"].as_str().unwrap_or("").to_string();
            let mut m = app.model.borrow_mut();
            let id = match cmd["kind"].as_str().unwrap_or("text") {
                "files" => {
                    let files: Vec<String> =
                        cmd["files"].as_array().map(|a| a.iter().filter_map(|f| f.as_str().map(String::from)).collect()).unwrap_or_default();
                    Some(m.add_files(files, &app_name))
                }
                "image" => {
                    let (w, h) = (cmd["w"].as_u64().unwrap_or(640) as u32, cmd["h"].as_u64().unwrap_or(400) as u32);
                    let b = image::sample(w, h, cmd["hue"].as_u64().unwrap_or(60) as u8);
                    let hash = store::hash(store::Kind::Image, &b.px);
                    match (image::encode_png(&b), image::thumb_png(&b)) {
                        (Some(png), Some(thumb)) => m.add_image(w, h, hash, &png, &thumb, &app_name),
                        _ => None,
                    }
                }
                _ => {
                    let text = cmd["text"].as_str().unwrap_or("").to_string();
                    let html = cmd["html"].as_str().map(String::from);
                    m.add_text(text, html, None, &app_name)
                }
            };
            // backdated (pictures of the window: not everything copied "now")
            if let (Some(id), Some(ago)) = (id, cmd["ago"].as_u64())
                && let Some(it) = m.store.items.iter_mut().find(|i| i.id == id)
            {
                it.at = store::now().saturating_sub(ago);
                m.changed();
            }
            json!({ "id": id })
        }
        "snippet" => {
            let mut m = app.model.borrow_mut();
            let id = m.store.save_snippet(None, cmd["name"].as_str().unwrap_or(""), cmd["trigger"].as_str().unwrap_or(""), cmd["text"].as_str().unwrap_or(""));
            m.changed();
            json!({ "id": id })
        }
        "pin" => {
            app.model.borrow_mut().set_pin(cmd["id"].as_u64().unwrap_or(0), cmd["on"].as_bool().unwrap_or(true));
            json!({ "ok": true })
        }
        "capture" => {
            let private = cmd["private"].as_bool().unwrap_or(false);
            let app_name = cmd["app"].as_str().unwrap_or("").to_string();
            // the same decision as a real copy: private formats first, then the pause and the ignored apps
            if clip::is_private(private, None, None, false) {
                app.model.borrow_mut().note("skipped: private");
                return json!({ "kept": false, "why": "private" });
            }
            let before = app.model.borrow().version;
            app.ingest(Captured::Text { text: cmd["text"].as_str().unwrap_or("").to_string(), html: None, rtf: None }, app_name, cx);
            json!({ "kept": app.model.borrow().version != before })
        }
        "capture_image" => {
            let (w, h) = (cmd["w"].as_u64().unwrap_or(800) as u32, cmd["h"].as_u64().unwrap_or(500) as u32);
            let dib = image::to_dib(&image::sample(w, h, 140));
            app.ingest(Captured::Image { data: dib, png: false }, cmd["app"].as_str().unwrap_or("").to_string(), cx);
            json!({ "ok": true })
        }
        "roundtrip" => roundtrip(app, cmd["id"].as_u64().unwrap_or(0)),
        "set" => {
            let old = app.model.borrow().set.clone();
            let mut v = serde_json::to_value(&old).unwrap_or_default();
            v[cmd["key"].as_str().unwrap_or("")] = cmd["value"].clone();
            match serde_json::from_value(v) {
                Ok(new) => {
                    app.model.borrow_mut().set = new;
                    app.settings_changed(&old, cx);
                    json!({ "ok": true })
                }
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "go" => {
            let p = match cmd["page"].as_str().unwrap_or("") {
                "text" => Page::History(store::Filter::Text),
                "links" => Page::History(store::Filter::Links),
                "images" => Page::History(store::Filter::Images),
                "files" => Page::History(store::Filter::Files),
                "pinned" => Page::History(store::Filter::Pinned),
                "snippets" => Page::Snippets,
                "settings" => Page::Settings,
                _ => Page::History(store::Filter::All),
            };
            app.go(p, cx);
            json!({ "ok": true })
        }
        "scroll" => {
            app.scroll = cmd["y"].as_f64().unwrap_or(0.0) as f32;
            cx.repaint();
            json!({ "ok": true })
        }
        "show" => {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(app.win.hwnd, windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE);
            }
            cx.repaint();
            json!({ "ok": true })
        }
        "repaint" => {
            cx.repaint();
            json!({ "ok": true })
        }
        "quit" => {
            app.quit(cx);
            json!({ "ok": true })
        }
        _ => json!({ "error": "unknown command" }),
    }
}

/// The real clipboard, for a moment: what is on it is saved, the item is written and read back, and what was there
/// is put back. The answer says whether the clipboard is exactly as before.
fn roundtrip(app: &mut App, id: u64) -> Value {
    let hwnd = app.win.hwnd;
    let Some(item) = app.model.borrow().store.get(id).cloned() else { return json!({ "error": "no such item" }) };
    let Some(before) = clip::saved::save(hwnd) else { return json!({ "error": "clipboard busy" }) };
    let written = app.model.borrow_mut().put_item(&item, false, true);
    // marked private, so Plume's own reader must refuse it; read again without that check to compare
    let refused = clip::read(hwnd, true, true, 64 << 20) == clip::Read::Skip("private");
    let read = clip::read_any(hwnd, true, true, 64 << 20, false);
    let restored = clip::saved::restore(hwnd, &before);
    let after = clip::saved::save(hwnd);
    let same = after.as_ref() == Some(&before);
    let read_back = match read {
        clip::Read::Got(Captured::Text { text, .. }) => json!({ "text": text }),
        clip::Read::Got(Captured::Files(f)) => json!({ "files": f }),
        clip::Read::Got(Captured::Image { data, png }) => {
            let b = if png { image::decode(&data) } else { image::decode_dib(&data) };
            json!({ "image": b.map(|b| [b.w, b.h]) })
        }
        other => json!({ "other": format!("{other:?}") }),
    };
    json!({
        "written": written,
        "private_refused": refused,
        "read": read_back,
        "restored": restored,
        "same_as_before": same,
        "formats_before": before.0.iter().map(|(f, d)| [*f as usize, d.len()]).collect::<Vec<_>>(),
    })
}

fn state(app: &mut App) -> Value {
    app.refresh_rows();
    let m = app.model.borrow();
    json!({
        "page": format!("{:?}", app.page),
        "query": app.query,
        "rows": app.rows,
        "items": m.store.items.iter().map(|i| json!({ "id": i.id, "kind": format!("{:?}", i.kind), "text": i.preview(60), "pin": i.pin, "app": i.app, "count": i.count, "html": i.html.is_some(), "secret": i.secret.map(|s| format!("{s:?}")), "shown": app.reveal.shown(i.id) })).collect::<Vec<_>>(),
        "dialog": app.dialog,
        "snippets": m.store.snippets.iter().map(|s| json!({ "id": s.id, "name": s.name, "trigger": s.trigger, "text": s.text })).collect::<Vec<_>>(),
        "settings": serde_json::to_value(&m.set).unwrap_or_default(),
        "autostart": m.autostart,
        "hotkey": m.hotkey.map(|h| h.label()),
        "hotkey_ok": m.hotkey_ok,
        "edit_for": format!("{:?}", app.edit_for),
        "draft": app.draft.as_ref().map(|d| json!({ "id": d.id, "name": d.name, "trigger": d.trigger, "text": d.text })),
        "capturing": app.capturing,
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "dark": m.theme.dark,
        "visible": crate::sys::visible(app.win.hwnd),
        "hwnd": app.win.hwnd.0 as isize,
        "panel_hwnd": m.panel.map(|p| p.hwnd.0 as isize),
        "dir": m.dir.display().to_string(),
        "dirty": m.dirty,
        "content_h": app.content_h,
        "scroll": app.scroll,
        "log": m.log,
    })
}
