//! Test socket: a script drives the window and the pads without the user's mouse, keyboard or clipboard. Only in a
//! `--features test-hook` build, and only when PLUME_NOTEPAD_TEST_PORT is set. One JSON command per line on
//! 127.0.0.1, one JSON answer per line.
//!   {"cmd":"state"}  {"cmd":"text"}  {"cmd":"hits"}  {"cmd":"log"}
//!   {"cmd":"click","id":"Tool(Find)"}               a click on an element (its name as `hits` gives it)
//!   {"cmd":"type","text":"…"}                        typed in the text (or in the field being typed in)
//!   {"cmd":"key","vk":13,"ctrl":true,"shift":false}  a key
//!   {"cmd":"open","path":"…"}  {"cmd":"new_tab"}  {"cmd":"tab","i":1}  {"cmd":"close_tab","i":0}
//!   {"cmd":"save"}  {"cmd":"save_as","path":"…","enc":"utf16le","eol":"lf"}  {"cmd":"reopen","enc":"ansi"}
//!   {"cmd":"set_enc","enc":"utf8bom"}  {"cmd":"set_eol","eol":"crlf"}
//!   {"cmd":"find","text":"…","case":false,"word":false,"regex":false,"replace":true}  {"cmd":"with","text":"…"}
//!   {"cmd":"find_next","back":false}  {"cmd":"replace_one"}  {"cmd":"replace_all"}  {"cmd":"goto","line":"12"}
//!   {"cmd":"select","a":[0,0],"b":[0,5]}  {"cmd":"dialog","i":2}  {"cmd":"menu","id":5}
//!   {"cmd":"setting","key":"wrap","value":false}  {"cmd":"settings","open":true}
//!   {"cmd":"toggle","name":"default"}  {"cmd":"button","name":"default_apps"}  {"cmd":"scroll","y":2}
//!   {"cmd":"pad_new"}  {"cmd":"pads"}  {"cmd":"pad","id":1,"do":"type|key|open|delete|state|hits|click","hit":"Pin",…}
//!   {"cmd":"tray_menu"}  {"cmd":"tray","id":1}  {"cmd":"show"}  {"cmd":"quit"}

use crate::{
    codec::{Encoding, Eol},
    model::{self, Shared},
    pads::{self, PadView},
    registry, sys,
    text::{Sel, pos},
    ui::{App, Field, with_app},
};
use plume_ui::{Button, Cx, Event, Handler, Mods, Window, pt};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

thread_local! {
    static SHARED: RefCell<Option<Rc<RefCell<Shared>>>> = const { RefCell::new(None) };
}

pub fn start(main: Window, sh: Rc<RefCell<Shared>>) {
    let Some(port) = std::env::var("PLUME_NOTEPAD_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
    SHARED.with(|s| *s.borrow_mut() = Some(sh));
    let ms = main.sender();
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
                let Ok(cmd) = serde_json::from_str::<Value>(line.trim()) else {
                    let _ = writeln!(out, "{}", json!({ "error": "bad json" }));
                    continue;
                };
                let (tx, rx) = mpsc::channel();
                let sent = ms.send(move |h, cx| {
                    let _ = tx.send(run(h, &cmd, cx).to_string());
                });
                let answer = if sent {
                    rx.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
                } else {
                    json!({ "error": "window closed" }).to_string()
                };
                if writeln!(out, "{answer}").is_err() {
                    break;
                }
            }
        }
    });
}

fn mods(cmd: &Value) -> Mods {
    Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: cmd["alt"].as_bool().unwrap_or(false) }
}

fn click_at(h: &mut dyn Handler, r: Option<plume_ui::Rect>, m: Mods, cx: &mut Cx) -> Value {
    let Some(r) = r else { return json!({ "error": "no such target" }) };
    let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.event(Event::MouseMove { p, mods: m }, cx);
    h.event(Event::MouseDown { p, button: Button::Left, clicks: 1, mods: m }, cx);
    h.event(Event::MouseUp { p, button: Button::Left, mods: m }, cx);
    json!({ "ok": true })
}

/// Text typed as keys: line breaks and tabs are keys, the rest characters.
fn type_into(h: &mut dyn Handler, text: &str, cx: &mut Cx) {
    for ch in text.chars() {
        match ch {
            '\n' => h.event(Event::Key { vk: 0x0D, mods: Mods::default(), repeat: false }, cx),
            '\t' => h.event(Event::Key { vk: 0x09, mods: Mods::default(), repeat: false }, cx),
            c => h.event(Event::Char(c), cx),
        }
    }
}

fn shared() -> Option<Rc<RefCell<Shared>>> {
    SHARED.with(|s| s.borrow().clone())
}

fn run(h: &mut dyn Handler, cmd: &Value, cx: &mut Cx) -> Value {
    let Some(app) = with_app(h) else { return json!({ "error": "no app" }) };
    let s = |k: &str| cmd[k].as_str().unwrap_or("").to_string();
    match cmd["cmd"].as_str().unwrap_or("") {
        "state" => state(app),
        "text" => json!({ "text": app.tab().ed.doc.text() }),
        "log" => json!(app.sh.borrow().log),
        "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
        "click" => {
            let want = s("id");
            let r = app.hits.rect_of(|id| format!("{id:?}") == want);
            click_at(app, r, mods(cmd), cx)
        }
        "type" => {
            let text = s("text");
            if app.field_focus && app.field.is_some() && app.win.edit_visible() {
                let f = app.field.expect("checked");
                app.win.edit_set_text(&text);
                app.field_changed(f, text, cx);
                json!({ "ok": true, "field": format!("{f:?}") })
            } else {
                type_into(app, &text, cx);
                json!({ "ok": true })
            }
        }
        "key" => {
            let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
            if app.field_focus && app.win.edit_visible() && matches!(vk, 0x0D | 0x1B | 0x09 | 0x26 | 0x28) {
                app.event(Event::EditKey { vk, mods: mods(cmd) }, cx);
            } else {
                app.event(Event::Key { vk, mods: mods(cmd), repeat: false }, cx);
            }
            json!({ "ok": true })
        }
        "open" => json!({ "ok": app.open_path(&PathBuf::from(s("path")), cx) }),
        "new_tab" => {
            app.new_tab();
            app.touch(cx);
            json!({ "ok": true })
        }
        "tab" => {
            app.active = (cmd["i"].as_u64().unwrap_or(0) as usize).min(app.tabs.len() - 1);
            cx.repaint();
            json!({ "ok": true })
        }
        "close_tab" => {
            let i = cmd["i"].as_u64().map_or(app.active, |i| i as usize);
            app.close_tab(i, cx);
            json!({ "ok": true, "dialog": app.dialog.as_ref().map(|d| d.title.clone()) })
        }
        "save" => {
            let i = app.active;
            json!({ "ok": app.save_tab(i, false, false, cx), "dialog": app.dialog.as_ref().map(|d| d.title.clone()) })
        }
        "save_as" => {
            if let Some(e) = cmd["enc"].as_str().and_then(Encoding::from_key) {
                app.tab_mut().enc = e;
            }
            if let Some(e) = cmd["eol"].as_str().and_then(Eol::from_key) {
                app.tab_mut().eol = e;
            }
            let i = app.active;
            json!({ "ok": app.save_to(i, PathBuf::from(s("path")), false, cx), "dialog": app.dialog.as_ref().map(|d| d.title.clone()) })
        }
        "set_enc" => {
            let e = Encoding::from_key(&s("enc")).unwrap_or_default();
            app.set_encoding(e, cx);
            json!({ "ok": true })
        }
        "set_eol" => {
            let e = Eol::from_key(&s("eol")).unwrap_or_default();
            app.set_eol(e, cx);
            json!({ "ok": true })
        }
        "reopen" => {
            let e = Encoding::from_key(&s("enc")).unwrap_or_default();
            app.reopen_as(e, cx);
            json!({ "ok": true, "text": app.tab().ed.doc.text() })
        }
        "find" => {
            app.find.q.case = cmd["case"].as_bool().unwrap_or(false);
            app.find.q.word = cmd["word"].as_bool().unwrap_or(false);
            app.find.q.regex = cmd["regex"].as_bool().unwrap_or(false);
            app.open_find(cmd["replace"].as_bool().unwrap_or(false), cx);
            app.field = Some(Field::Find);
            app.field_shown = None;
            app.find.origin = pos(0, 0);
            app.field_changed(Field::Find, s("text"), cx);
            let count = app.match_count();
            json!({ "count": count, "sel": sel(app), "error": app.find.matcher.as_ref().and_then(|m| m.as_ref().err().cloned()) })
        }
        "with" => {
            app.field_changed(Field::Replace, s("text"), cx);
            json!({ "ok": true })
        }
        "find_next" => json!({ "found": app.find_step(cmd["back"].as_bool().unwrap_or(false), None, cx), "sel": sel(app) }),
        "replace_one" => {
            app.replace_one(cx);
            json!({ "text": app.tab().ed.doc.text(), "sel": sel(app) })
        }
        "replace_all" => {
            app.replace_all(cx);
            json!({ "text": app.tab().ed.doc.text() })
        }
        "goto" => json!({ "ok": app.goto(&s("line"), cx), "sel": sel(app) }),
        "select" => {
            let p = |v: &Value| pos(v[0].as_u64().unwrap_or(0) as usize, v[1].as_u64().unwrap_or(0) as usize);
            let ed = app.ed();
            ed.doc.sel = Sel { anchor: ed.doc.clamp(p(&cmd["a"])), caret: ed.doc.clamp(p(&cmd["b"])) };
            ed.ensure_visible();
            cx.repaint();
            json!({ "sel": sel(app) })
        }
        "dialog" => {
            app.dialog_choose(cmd["i"].as_u64().unwrap_or(0) as usize, cx);
            json!({ "ok": true, "dialog": app.dialog.as_ref().map(|d| d.title.clone()) })
        }
        "menu" => {
            app.menu_command(cmd["id"].as_u64().unwrap_or(0) as u32, cx);
            json!({ "ok": true })
        }
        "setting" => {
            let key = s("key");
            let value = cmd["value"].clone();
            app.change_setting(
                |set| {
                    let mut v = serde_json::to_value(&*set).unwrap_or_default();
                    v[key.as_str()] = value;
                    if let Ok(n) = serde_json::from_value(v) {
                        *set = n;
                    }
                },
                cx,
            );
            json!({ "settings": serde_json::to_value(&app.sh.borrow().set).unwrap_or_default() })
        }
        "settings" => {
            app.open_settings(cmd["open"].as_bool().unwrap_or(true), cx);
            json!({ "ok": true })
        }
        "toggle" => {
            app.toggle(&s("name"), cx);
            json!({ "ok": true, "default_on": app.default_on, "autostart": registry::run_on(&model::user_root()) })
        }
        "button" => {
            app.button(&s("name"), cx);
            json!({ "ok": true })
        }
        "scroll" => {
            if app.settings {
                app.settings_scroll = cmd["y"].as_f64().unwrap_or(0.0) as f32;
            } else {
                let n = cmd["y"].as_i64().unwrap_or(0) as isize;
                let ed = app.ed();
                ed.top = ed.offset_rows(ed.top, n);
                ed.clamp_top();
            }
            cx.repaint();
            json!({ "ok": true })
        }
        "pad_new" => {
            let sh = app.sh.clone();
            let id = pads::new_pad(&sh, false);
            let hwnd = sh.borrow().pad_wins.get(&id).map(|w| w.hwnd.0 as isize);
            json!({ "id": id, "hwnd": hwnd })
        }
        "pads" => pads_state(&app.sh.borrow()),
        "pad" => pad_cmd(app, cmd),
        "tray_menu" => {
            let items = pads::tray_items(&app.sh.borrow());
            json!(items.iter().map(|i| json!({ "id": i.id, "label": i.label, "checked": i.checked })).collect::<Vec<_>>())
        }
        "tray" => {
            let sh = app.sh.clone();
            let id = cmd["id"].as_u64().unwrap_or(0) as u32;
            cx.after(move |_| pads::tray_command(&sh, id));
            json!({ "ok": true })
        }
        "show" => {
            sys::show(app.win.hwnd, true, false);
            cx.repaint();
            json!({ "ok": true })
        }
        "repaint" => {
            cx.repaint();
            json!({ "ok": true })
        }
        "flush" => {
            app.flush();
            json!({ "ok": true })
        }
        "quit" => {
            if let Some(sh) = shared() {
                crate::quit(&sh);
            }
            json!({ "ok": true })
        }
        _ => json!({ "error": "unknown command" }),
    }
}

fn sel(app: &App) -> Value {
    let s = app.tab().ed.doc.sel;
    json!({ "a": [s.anchor.line, s.anchor.col], "b": [s.caret.line, s.caret.col], "text": app.tab().ed.doc.selected() })
}

fn topmost(w: Window) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_TOPMOST};
    unsafe { GetWindowLongPtrW(w.hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0 }
}

fn pads_state(sh: &Shared) -> Value {
    json!(
        sh.pads
            .list
            .iter()
            .map(|p| {
                let w = sh.pad_wins.get(&p.id).copied();
                json!({
                    "id": p.id, "text": p.text, "color": p.color, "on_top": p.on_top, "open": p.open, "rect": [p.rect.0, p.rect.1, p.rect.2, p.rect.3],
                    "hwnd": w.map(|w| w.hwnd.0 as isize), "topmost": w.map(topmost), "visible": w.map(|w| sys::visible(w.hwnd)),
                })
            })
            .collect::<Vec<_>>()
    )
}

fn pad_cmd(app: &mut App, cmd: &Value) -> Value {
    let id = cmd["id"].as_u64().unwrap_or(0);
    let sh = app.sh.clone();
    let what = cmd["do"].as_str().unwrap_or("state").to_string();
    match what.as_str() {
        "open" => {
            pads::open_pad(&sh, id, false);
            return json!({ "ok": true });
        }
        "delete" => {
            pads::delete_pad(&sh, id);
            return json!({ "ok": true });
        }
        _ => {}
    }
    let Some(w) = sh.borrow().pad_wins.get(&id).copied() else { return json!({ "error": "pad not open" }) };
    let cmd = cmd.clone();
    w.with(move |h, cx| {
        let Some(p) = h.as_any().downcast_mut::<PadView>() else { return json!({ "error": "no pad" }) };
        match what.as_str() {
            "type" => {
                type_into(p, cmd["text"].as_str().unwrap_or(""), cx);
                json!({ "text": p.ed.doc.text() })
            }
            "key" => {
                p.event(Event::Key { vk: cmd["vk"].as_u64().unwrap_or(0) as u16, mods: mods(&cmd), repeat: false }, cx);
                json!({ "text": p.ed.doc.text() })
            }
            "hits" => json!(p.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
            "click" => {
                let want = cmd["hit"].as_str().unwrap_or("").to_string();
                let r = p.hits.rect_of(|id| format!("{id:?}") == want);
                click_at(p, r, Mods::default(), cx)
            }
            "state" => json!({ "text": p.ed.doc.text(), "picking": p.picking, "confirm_delete": p.confirm_delete, "topmost": topmost(p.win) }),
            _ => json!({ "error": "unknown pad command" }),
        }
    })
    .unwrap_or_else(|| json!({ "error": "pad busy" }))
}

fn state(app: &mut App) -> Value {
    let count = app.match_count();
    let s = app.sh.borrow();
    let tabs: Vec<Value> = app
        .tabs
        .iter()
        .map(|t| {
            json!({
                "id": t.id, "name": t.name(), "path": t.path.as_ref().map(|p| p.display().to_string()), "dirty": t.dirty(),
                "enc": t.enc.key(), "eol": t.eol.key(), "chars": t.ed.doc.chars(), "lines": t.ed.doc.line_count(),
                "caret": [t.ed.doc.sel.caret.line, t.ed.doc.sel.caret.col], "top": [t.ed.top.0, t.ed.top.1], "wrap": t.ed.wrap,
            })
        })
        .collect();
    let machine = model::machine_root();
    let user = model::user_root();
    json!({
        "tabs": tabs,
        "active": app.active,
        "sel": sel(app),
        "find": { "open": app.find.open, "replace": app.find.replace, "text": app.find.q.text, "with": app.find.with, "count": count },
        "field": app.field.map(|f| format!("{f:?}")),
        "field_focus": app.field_focus,
        "dialog": app.dialog.as_ref().map(|d| json!({ "title": d.title, "buttons": d.buttons.iter().map(|b| b.0.clone()).collect::<Vec<_>>() })),
        "settings_open": app.settings,
        "settings": serde_json::to_value(&s.set).unwrap_or_default(),
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "title": app.last_title,
        "dark": s.theme.dark,
        "hwnd": app.win.hwnd.0 as isize,
        "visible": sys::visible(app.win.hwnd),
        "dir": s.dir.display().to_string(),
        "pads": pads_state(&s),
        "ifeo": serde_json::to_value(format!("{:?}", registry::read_ifeo(&machine))).unwrap_or_default(),
        "ifeo_on": registry::ifeo_on(&registry::read_ifeo(&machine)),
        "ifeo_root": machine.prefix,
        "run_on": registry::run_on(&user),
        "run_value": registry::get(&user, registry::RUN, registry::RUN_VALUE),
        "assoc_on": registry::assoc_on(&user),
        "user_root": user.prefix,
        "zoom": s.set.zoom,
    })
}
