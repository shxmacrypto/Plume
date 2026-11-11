//! Test socket: a script drives the launcher without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_LAUNCHER_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//! In this mode nothing is opened, copied or registered for real (see `sys::set_test_mode`).
//!   {"cmd":"state"}                       the box's state (results, selection, time to show…)
//!   {"cmd":"show"}                        brings the box up without taking the keyboard
//!   {"cmd":"hide"}
//!   {"cmd":"type","text":"…"}             text in the search field
//!   {"cmd":"key","vk":40,"ctrl":false}    a key (Enter 13, Esc 27, Up 38, Down 40, Tab 9)
//!   {"cmd":"hits"}                        clickable elements and their rectangles
//!   {"cmd":"click","id":"Row(1)"}         a click on an element (its name as `hits` gives it)
//!   {"cmd":"set","key":"theme","value":"light"}   a setting
//!   {"cmd":"settings"}                    opens the settings window
//!   {"cmd":"swin","sub":"state|hits|click|key", …}   the same, in the settings window
//!   {"cmd":"rebuild"}                     rebuilds the file index
//!   {"cmd":"quit"}

use crate::{
    settings_ui::{self, SettingsApp},
    sys,
    ui::{App, Shared, with_app},
};
use plume_ui::{Button, Event, Gfx, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    rc::Rc,
    sync::{atomic::Ordering, mpsc},
    time::Duration,
};

thread_local! {
    static CTX: std::cell::RefCell<Option<(Rc<Gfx>, Rc<Shared>)>> = const { std::cell::RefCell::new(None) };
}

pub fn start(win: Window, gfx: Rc<Gfx>, sh: Rc<Shared>) {
    let Some(port) = std::env::var("PLUME_LAUNCHER_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
    CTX.with(|c| *c.borrow_mut() = Some((gfx, sh)));
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

/// Runs `f` on the interface thread, in the window behind `sender`, and waits for its answer.
fn ask(sender: &Sender, f: impl FnOnce(&mut dyn Handler, &mut plume_ui::Cx) -> Value + Send + 'static) -> Value {
    let (tx, rx) = mpsc::channel();
    let sent = sender.send(move |h, cx| {
        let _ = tx.send(f(h, cx));
    });
    if !sent {
        return json!({ "error": "window closed" });
    }
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| json!({ "error": "timeout" }))
}

fn click_at(h: &mut dyn Handler, r: plume_ui::Rect, cx: &mut plume_ui::Cx) {
    let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
    let mods = Mods::default();
    h.event(Event::MouseMove { p, mods }, cx);
    h.event(Event::MouseDown { p, button: Button::Left, clicks: 1, mods }, cx);
    h.event(Event::MouseUp { p, button: Button::Left, mods }, cx);
}

fn run(sender: &Sender, line: &str) -> String {
    let Ok(cmd) = serde_json::from_str::<Value>(line) else { return json!({ "error": "bad json" }).to_string() };
    if cmd["cmd"] == "swin" {
        // the settings window has its own sender
        let target = ask(sender, |h, _| json!(with_app(h).and_then(|a| a.sh.settings_win.get()).map(|w| w.hwnd.0 as isize)));
        let Some(hwnd) = target.as_i64() else { return json!({ "error": "settings window not open" }).to_string() };
        let win = Window { hwnd: windows::Win32::Foundation::HWND(hwnd as _) };
        return ask(&win.sender(), move |h, cx| match h.as_any().downcast_mut::<SettingsApp>() {
            Some(s) => settings_cmd(s, &cmd, cx),
            None => json!({ "error": "no settings window" }),
        })
        .to_string();
    }
    ask(sender, move |h, cx| match with_app(h) {
        Some(app) => popup_cmd(app, &cmd, cx),
        None => json!({ "error": "no app" }),
    })
    .to_string()
}

fn popup_cmd(app: &mut App, cmd: &Value, cx: &mut plume_ui::Cx) -> Value {
    match cmd["cmd"].as_str().unwrap_or("") {
        "state" => state(app),
        "show" => {
            app.show(cmd["activate"].as_bool().unwrap_or(false), cx);
            json!({ "ok": true })
        }
        "hide" => {
            app.hide(cx);
            json!({ "ok": true })
        }
        // forgets where the real mouse is (no hover highlight in screenshots)
        "unhover" => {
            app.event(Event::MouseLeave, cx);
            json!({ "ok": true })
        }
        "type" => {
            let text = cmd["text"].as_str().unwrap_or("").to_string();
            app.win.edit_set_text(&text);
            app.event(Event::EditChanged(text), cx);
            json!({ "ok": true })
        }
        "key" => {
            let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
            let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: false };
            app.event(Event::EditKey { vk, mods }, cx);
            json!({ "ok": true })
        }
        "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
        "click" => {
            let want = cmd["id"].as_str().unwrap_or("").to_string();
            match app.hits.rect_of(|id| format!("{id:?}") == want) {
                Some(r) => {
                    click_at(app, r, cx);
                    json!({ "ok": true })
                }
                None => json!({ "error": "no such target" }),
            }
        }
        "set" => {
            let mut s = serde_json::to_value(&*app.sh.set.borrow()).unwrap_or_default();
            s[cmd["key"].as_str().unwrap_or("")] = cmd["value"].clone();
            match serde_json::from_value(s) {
                Ok(new) => {
                    *app.sh.set.borrow_mut() = new;
                    app.sh.set.borrow().save();
                    crate::i18n::set(&app.sh.set.borrow().lang);
                    app.theme = crate::ui::theme_for(&app.sh.set.borrow());
                    if app.visible {
                        app.refresh(true, cx);
                    }
                    json!({ "ok": true })
                }
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "settings" => {
            app.hide(cx);
            cx.after(|_| {
                CTX.with(|c| {
                    if let Some((gfx, sh)) = c.borrow().as_ref() {
                        settings_ui::open(gfx, sh, false);
                    }
                })
            });
            json!({ "ok": true })
        }
        "rebuild" => {
            app.sh.files.rebuild();
            json!({ "ok": true })
        }
        "quit" => {
            app.quit(cx);
            json!({ "ok": true })
        }
        _ => json!({ "error": "unknown command" }),
    }
}

fn settings_cmd(s: &mut SettingsApp, cmd: &Value, cx: &mut plume_ui::Cx) -> Value {
    match cmd["sub"].as_str().unwrap_or("") {
        "state" => json!({
            "capturing": s.capturing,
            "note": s.note,
            "hwnd": s.win.hwnd.0 as isize,
            "autostart": sys::autostart(),
        }),
        "hits" => json!(s.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
        "click" => {
            let want = cmd["id"].as_str().unwrap_or("").to_string();
            match s.hits.iter().rev().find(|(_, id)| format!("{id:?}") == want).map(|(_, id)| *id) {
                Some(hit) => {
                    s.click(hit, cx);
                    json!({ "ok": true })
                }
                None => json!({ "error": "no such target" }),
            }
        }
        "key" => {
            let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
            let mods = Mods {
                ctrl: cmd["ctrl"].as_bool().unwrap_or(false),
                shift: cmd["shift"].as_bool().unwrap_or(false),
                alt: cmd["alt"].as_bool().unwrap_or(false),
            };
            s.event(Event::Key { vk, mods, repeat: false }, cx);
            json!({ "ok": true })
        }
        "front" => {
            use windows::Win32::UI::WindowsAndMessaging::{HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos};
            let on = cmd["on"].as_bool().unwrap_or(true);
            unsafe {
                let _ = SetWindowPos(s.win.hwnd, Some(if on { HWND_TOPMOST } else { HWND_NOTOPMOST }), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            json!({ "ok": true })
        }
        "scroll" => {
            s.event(Event::Wheel { p: pt(10.0, 100.0), dx: 0.0, dy: cmd["dy"].as_f64().unwrap_or(-3.0) as f32, mods: Mods::default() }, cx);
            json!({ "ok": true })
        }
        "close" => {
            s.win.close();
            json!({ "ok": true })
        }
        _ => json!({ "error": "unknown command" }),
    }
}

fn state(app: &mut App) -> Value {
    let f = &app.sh.files;
    json!({
        "visible": app.visible,
        "on_screen": sys::visible(app.win.hwnd),
        "query": app.query,
        "rows": app.rows.iter().map(|r| json!({ "kind": format!("{:?}", r.kind), "title": r.title, "sub": r.sub, "target": r.target })).collect::<Vec<_>>(),
        "sel": app.sel,
        "top": app.top,
        "show_ms": app.show_ms,
        "shows": app.shows,
        "height": app.height,
        "apps": app.apps.len(),
        "apps_ready": app.apps_ready,
        "file_hits": app.file_hits.len(),
        "files": { "count": f.count.load(Ordering::Relaxed), "building": f.building.load(Ordering::Relaxed), "built": f.built.load(Ordering::Relaxed) },
        "history": app.sh.history.borrow().picks.len(),
        "hotkey": app.sh.set.borrow().hotkey,
        "hotkey_ok": app.sh.hotkey_ok.get(),
        "settings": serde_json::to_value(&*app.sh.set.borrow()).unwrap_or_default(),
        "settings_open": app.sh.settings_win.get().map(|w| w.hwnd.0 as isize),
        "actions": sys::actions(),
        "dark": app.theme.dark,
        "hwnd": app.win.hwnd.0 as isize,
    })
}
