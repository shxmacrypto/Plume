//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_TASK_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                         the window's state (pages, numbers, the list's first rows)
//!   {"cmd":"hits"}                          clickable elements and their rectangles
//!   {"cmd":"click","id":"Header(Mem)"}      a click on an element (its name as `hits` gives it)
//!   {"cmd":"dclick","id":"Row(3)"}          a double click
//!   {"cmd":"rclick","id":"Row(3)"}          a right click (opens the row's menu)
//!   {"cmd":"type","text":"…"}               text in the search field
//!   {"cmd":"key","vk":46}                   a key
//!   {"cmd":"go","page":"perf"}              apps, perf, startup, settings
//!   {"cmd":"select_pid","pid":1234}         selects a process (its program expanded) — tests end only their own dummy
//!   {"cmd":"set","key":"interval","value":500}   a setting
//!   {"cmd":"scroll","y":300}                the page's scroll

use crate::ui::{App, Page, Row, Sel};
use plume_ui::{Button, Event, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::mpsc,
    time::Duration,
};

pub fn start(win: Window) {
    let Some(port) = std::env::var("PLUME_TASK_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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

fn click(app: &mut App, want: &str, clicks: u8, right: bool, cx: &mut plume_ui::Cx) -> Value {
    match app.hits.rect_of(|id| format!("{id:?}") == want) {
        Some(r) => {
            let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
            let mods = Mods::default();
            let button = if right { Button::Right } else { Button::Left };
            app.event(Event::MouseMove { p, mods }, cx);
            for k in 1..=clicks {
                app.event(Event::MouseDown { p, button, clicks: k, mods }, cx);
                if !right {
                    app.event(Event::MouseUp { p, button, mods }, cx);
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
                "click" => click(app, cmd["id"].as_str().unwrap_or(""), 1, false, cx),
                "dclick" => click(app, cmd["id"].as_str().unwrap_or(""), 2, false, cx),
                "rclick" => click(app, cmd["id"].as_str().unwrap_or(""), 1, true, cx),
                "type" => {
                    let text = cmd["text"].as_str().unwrap_or("").to_string();
                    app.edit_for = Some("search");
                    app.win.edit_set_text(&text);
                    app.event(Event::EditChanged(text), cx);
                    json!({ "ok": true })
                }
                "key" => {
                    let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
                    let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: false, alt: false };
                    app.event(Event::Key { vk, mods, repeat: false }, cx);
                    json!({ "ok": true })
                }
                "go" => {
                    app.go(Page::from_key(cmd["page"].as_str().unwrap_or("")), cx);
                    json!({ "ok": true })
                }
                "select_pid" => {
                    let pid = cmd["pid"].as_u64().unwrap_or(0) as u32;
                    match app.groups.iter().find_map(|g| g.procs.iter().find(|p| p.pid == pid).map(|p| (g.key.clone(), p.create, g.procs.len()))) {
                        Some((key, create, n)) => {
                            if n > 1 {
                                app.expanded.insert(key);
                                app.rebuild();
                            }
                            app.sel = Some(Sel::Proc(pid, create));
                            cx.repaint();
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no such process in the list" }),
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
                "scroll" => {
                    app.set_scroll(cmd["y"].as_f64().unwrap_or(0.0) as f32);
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
    let rows: Vec<Value> = app
        .rows
        .iter()
        .take(40)
        .map(|r| match *r {
            Row::Section(l, n) => json!({ "section": l, "n": n }),
            Row::Group(gi) => {
                let g = &app.groups[gi];
                json!({ "group": g.name, "key": g.key, "procs": g.procs.len(), "cpu": g.cpu, "mem": g.mem, "disk": g.disk(), "gpu": g.gpu, "impact": g.impact, "windowed": g.windowed })
            }
            Row::Proc(gi, pi) => {
                let p = &app.groups[gi].procs[pi];
                json!({ "proc": p.name, "pid": p.pid, "cpu": p.cpu, "mem": p.mem })
            }
        })
        .collect();
    let s = &app.sys;
    json!({
        "page": app.page.key(),
        "samples": app.samples,
        "took_us": app.took_us,
        "groups": app.groups.len(),
        "rows_total": app.rows.len(),
        "rows": rows,
        "sort": format!("{:?}", app.set.sort),
        "desc": app.set.desc,
        "query": app.query,
        "expanded": app.expanded.iter().collect::<Vec<_>>(),
        "sel": app.sel.as_ref().map(|s| format!("{s:?}")),
        "summary": app.summary.as_ref().map(|s| json!({ "level": s.level, "title": s.title, "detail": s.detail, "key": s.key })),
        "sys": {
            "cpu": s.cpu, "ncpu": s.ncpu, "cpu_name": s.cpu_name, "mem_used": s.mem_used, "mem_total": s.mem_total,
            "disk_active": s.disk_active, "disk_read": s.disk_read, "disk_write": s.disk_write,
            "net_in": s.net_in, "net_out": s.net_out, "gpu": s.gpu, "has_gpu": s.has_gpu, "gpu_name": s.gpu_name,
            "uptime_s": s.uptime_s, "procs": s.procs, "threads": s.threads, "own_cpu": s.own_cpu,
        },
        "own_cpu_avg": app.own_cpu_avg(),
        "hist_len": app.hist.cpu.len(),
        "startup": app.startup.as_ref().map(|l| l.iter().map(|e| json!({ "name": e.name, "id": e.id, "kind": format!("{:?}", e.kind), "enabled": e.enabled, "exe": e.exe })).collect::<Vec<_>>()),
        "startup_test": app.startup_src.is_test(),
        "modal": app.modal.as_ref().map(|m| format!("{m:?}")),
        "menu": app.menu.as_ref().map(|m| m.items.iter().map(|i| i.0.clone()).collect::<Vec<_>>()),
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "icons": app.icons.len(),
        "settings": serde_json::to_value(&app.set).unwrap_or_default(),
        "scroll": app.scroll(),
        "content_h": app.content_h,
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
        "uptime_ms": app.started.elapsed().as_millis() as u64,
    })
}
