//! Test socket: a script drives the window without touching the user's mouse or keyboard. Only in a
//! `--features test-hook` build, and only when PLUME_BACKUP_TEST_PORT is set. One JSON command per line on
//! 127.0.0.1, one JSON answer per line.
//!   {"cmd":"state"}                              the window's state (plans, runs, restore view, toast, log)
//!   {"cmd":"hits"}                               the clickable elements and their rectangles
//!   {"cmd":"click","id":"Run(0)"}                a click on an element (its name as `hits` gives it)
//!   {"cmd":"dblclick","id":"Row(2)"}             a double click
//!   {"cmd":"type","text":"…"}                    text in the open field
//!   {"cmd":"key","vk":13}                        a key in the open field (Enter closes it)
//!   {"cmd":"wheel","x":600,"y":400,"dy":-3}      the mouse wheel
//!   {"cmd":"leave"}                              the mouse leaves the window (no hover left in a screenshot)
//!   {"cmd":"env","key":"PLUME_TEST_FOLDER","value":"…"}   what the next folder dialog answers
//!   {"cmd":"wait","timeout":30}                  waits until no backup runs or waits (and no restore)
//!   {"cmd":"schedules"}                          checks the schedules now
//!   {"cmd":"devices"}                            as if a drive was plugged in
//!   {"cmd":"quit"}                               quits for good (as "Quit" in the icon's menu)

use crate::{sys, ui::App};
use plume_ui::{Button, Event, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};

thread_local! {
    static QUIT: std::cell::RefCell<Option<Rc<Cell<bool>>>> = const { std::cell::RefCell::new(None) };
}

pub fn start(win: Window, quitting: Rc<Cell<bool>>) {
    QUIT.with(|q| *q.borrow_mut() = Some(quitting));
    let Some(port) = std::env::var("PLUME_BACKUP_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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

fn on_ui(sender: &Sender, f: impl FnOnce(&mut App, &mut plume_ui::Cx) -> Value + Send + 'static) -> Value {
    let (tx, rx) = mpsc::channel();
    let sent = sender.send(move |h, cx| {
        let out = match h.as_any().downcast_mut::<App>() {
            Some(app) => f(app, cx),
            None => json!({ "error": "no app" }),
        };
        let _ = tx.send(out);
    });
    if !sent {
        return json!({ "error": "window closed" });
    }
    rx.recv_timeout(Duration::from_secs(15)).unwrap_or_else(|_| json!({ "error": "timeout" }))
}

fn click(app: &mut App, cx: &mut plume_ui::Cx, want: &str, clicks: u8) -> Value {
    match app.hits.rect_of(|id| format!("{id:?}") == want) {
        Some(r) => {
            let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
            let mods = Mods::default();
            app.event(Event::MouseMove { p, mods }, cx);
            app.event(Event::MouseDown { p, button: Button::Left, clicks: 1, mods }, cx);
            app.event(Event::MouseUp { p, button: Button::Left, mods }, cx);
            if clicks == 2 {
                app.event(Event::MouseDown { p, button: Button::Left, clicks: 2, mods }, cx);
                app.event(Event::MouseUp { p, button: Button::Left, mods }, cx);
            }
            json!({ "ok": true })
        }
        None => json!({ "error": "no such target", "id": want }),
    }
}

fn run(sender: &Sender, line: &str) -> String {
    let Ok(cmd) = serde_json::from_str::<Value>(line) else { return json!({ "error": "bad json" }).to_string() };
    let name = cmd["cmd"].as_str().unwrap_or("").to_string();
    let out = match name.as_str() {
        "wait" => {
            let limit = Duration::from_secs_f64(cmd["timeout"].as_f64().unwrap_or(60.0));
            let start = Instant::now();
            loop {
                // a repaint in between, so the hits follow what changed
                let busy = on_ui(sender, |app, _| json!(app.running.is_some() || !app.queue.is_empty() || app.restoring));
                if busy == json!(false) {
                    break json!({ "ok": true, "secs": start.elapsed().as_secs_f64() });
                }
                if start.elapsed() > limit || busy.get("error").is_some() {
                    break json!({ "error": "still busy" });
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        "env" => {
            let (k, v) = (cmd["key"].as_str().unwrap_or(""), cmd["value"].as_str().unwrap_or(""));
            if k.starts_with("PLUME_TEST_") {
                // SAFETY: test builds only; the folder dialog reads it on the UI thread later
                unsafe { std::env::set_var(k, v) };
                json!({ "ok": true })
            } else {
                json!({ "error": "only PLUME_TEST_ variables" })
            }
        }
        _ => on_ui(sender, move |app, cx| match name.as_str() {
            "state" => state(app),
            "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
            "click" => click(app, cx, cmd["id"].as_str().unwrap_or(""), 1),
            "dblclick" => click(app, cx, cmd["id"].as_str().unwrap_or(""), 2),
            "type" => {
                let text = cmd["text"].as_str().unwrap_or("").to_string();
                app.win.edit_set_text(&text);
                let open = app.win.edit_visible();
                // edit_set_text is quiet: the change is told to the app as typing would
                app.event(Event::EditChanged(text), cx);
                json!({ "ok": open })
            }
            "key" => {
                let vk = cmd["vk"].as_u64().unwrap_or(13) as u16;
                app.event(Event::EditKey { vk, mods: Mods::default() }, cx);
                json!({ "ok": true })
            }
            "wheel" => {
                let p = pt(cmd["x"].as_f64().unwrap_or(0.0) as f32, cmd["y"].as_f64().unwrap_or(0.0) as f32);
                app.event(Event::Wheel { p, dx: 0.0, dy: cmd["dy"].as_f64().unwrap_or(-1.0) as f32, mods: Mods::default() }, cx);
                json!({ "ok": true })
            }
            "leave" => {
                app.event(Event::MouseLeave, cx);
                json!({ "ok": true })
            }
            "schedules" => {
                app.check_schedules();
                json!({ "ok": true })
            }
            "devices" => {
                app.event(Event::Devices, cx);
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
            _ => json!({ "error": "unknown command" }),
        }),
    };
    out.to_string()
}

fn state(app: &mut App) -> Value {
    let plans: Vec<Value> = app
        .store
        .plans
        .iter()
        .map(|p| {
            let st = app.store.state(p.id);
            json!({
                "id": p.id,
                "name": p.name,
                "folder": p.folder,
                "sources": p.sources,
                "dest": p.dest,
                "serial": p.dest_serial,
                "mode": format!("{:?}", p.mode),
                "schedule": format!("{:?}", p.schedule),
                "hour": p.hour,
                "keep_versions": p.keep_versions,
                "keep_days": p.keep_days,
                "deleted_days": p.deleted_days,
                "excludes": p.excludes,
                "skip_temp": p.skip_temp,
                "verify": p.verify,
                "paused": p.paused,
                "present": app.present.get(&p.id),
                "busy": app.busy(p.id),
                "last_ok": st.last_ok,
                "last": st.last.as_ref().map(|l| json!({ "ok": l.ok, "fail": l.fail, "report": l.report, "time": l.time })),
            })
        })
        .collect();
    json!({
        "view": format!("{:?}", app.view),
        "plans": plans,
        "running": app.running.as_ref().map(|r| json!({ "id": r.id, "done": r.prog.done, "total": r.prog.total })),
        "queue": app.queue,
        "draft": app.draft.as_ref().map(|d| json!({
            "index": d.index,
            "name": d.plan.name,
            "sources": d.plan.sources,
            "dest": d.plan.dest,
            "mode": format!("{:?}", d.plan.mode),
            "schedule": format!("{:?}", d.plan.schedule),
            "excludes": d.excludes,
            "keep_versions": d.plan.keep_versions,
        })),
        "field": app.field.map(|f| format!("{f:?}")),
        "edit_open": app.win.edit_visible(),
        "confirm_delete": app.confirm_delete,
        "restore": {
            "plan": app.rplan,
            "browse": format!("{:?}", app.browse),
            "snaps": app.snaps.iter().map(|s| json!({ "stamp": s.stamp, "kind": format!("{:?}", s.kind), "time": s.time, "files": s.files, "path": s.path })).collect::<Vec<_>>(),
            "snap": app.snap,
            "path": app.path,
            "rows": app.rows.iter().map(|e| json!({ "name": e.name, "dir": e.dir, "size": e.size })).collect::<Vec<_>>(),
            "sel": app.sel,
        },
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "autostart": app.autostart,
        "settings": { "lang": app.set.lang, "theme": app.set.theme, "background": app.set.background, "notify_ok": app.set.notify_ok, "notify_fail": app.set.notify_fail },
        "keep": app.keep.get(),
        "visible": sys::visible(app.win.hwnd),
        "dark": app.theme.dark,
        "tip": app.tray_text(),
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
    })
}
