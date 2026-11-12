//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build (which always works on the imaginary PC, never on the real one), and only when PLUME_BOOST_TEST_PORT is set.
//! One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                        the window's state (page, ticks, statuses, log, dialog, live numbers)
//!   {"cmd":"hits"}                         clickable elements and their rectangles
//!   {"cmd":"click","id":"Tick(3)"}         a click on an element (its name as `hits` gives it)
//!   {"cmd":"key","vk":27}                  a key (Esc closes a dialog, Enter confirms it)
//!   {"cmd":"go","page":"tweaks"}           overview, tweaks, cpu, changes, settings
//!   {"cmd":"expand","id":"hags"}           opens a tweak's details
//!   {"cmd":"set","key":"lang","value":"fr"} a setting
//!   {"cmd":"scroll","y":300}               the page's scroll
//!   {"cmd":"knob","key":"core_offset","value":150}   a graphics card control (as if its slider was dragged)
//!   {"cmd":"event","what":"reset"}         the imaginary PC: "reset" (driver reset), "crash", "restart"
//!   {"cmd":"guard"}                        the crash guard checks now

use crate::{
    engine::Status,
    tweaks::TWEAKS,
    ui::{App, Page},
};
use plume_ui::{Button, Event, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::mpsc,
    time::Duration,
};

pub fn start(win: Window) {
    let Some(port) = std::env::var("PLUME_BOOST_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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
                "key" => {
                    let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
                    app.event(Event::Key { vk, mods: Mods::default(), repeat: false }, cx);
                    json!({ "ok": true })
                }
                "go" => {
                    app.go(Page::from_key(cmd["page"].as_str().unwrap_or("")), cx);
                    json!({ "ok": true })
                }
                "expand" => {
                    let id = cmd["id"].as_str().unwrap_or("");
                    match TWEAKS.iter().find(|t| t.id == id) {
                        Some(t) => {
                            app.expanded.insert(t.id);
                            cx.repaint();
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no such tweak" }),
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
                "knob" => match crate::gpu::Knob::from_key(cmd["key"].as_str().unwrap_or("")) {
                    Some(k) => {
                        app.knob_set(k, cmd["value"].as_f64().unwrap_or(0.0));
                        cx.repaint();
                        json!({ "ok": true })
                    }
                    None => json!({ "error": "no such knob" }),
                },
                "event" => json!({ "ok": app.be.test_event(cmd["what"].as_str().unwrap_or("")) }),
                "guard" => {
                    app.check_guard(cx);
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
    let statuses: serde_json::Map<String, Value> = TWEAKS
        .iter()
        .zip(&app.statuses)
        .map(|(t, s)| {
            (
                t.id.to_string(),
                json!(match s {
                    Status::On => "on",
                    Status::Off => "off",
                    Status::Partly => "partly",
                    Status::Unavailable => "unavailable",
                }),
            )
        })
        .collect();
    let l = &app.live;
    json!({
        "page": app.page.key(),
        "test": app.be.is_test(),
        "admin": app.admin,
        "battery": app.battery,
        "profile": app.set.profile,
        "ticked": app.ticked,
        "to_apply": app.to_apply(),
        "statuses": statuses,
        "log_entries": app.log.entries.len(),
        "changed": app.changed(),
        "restore_points": app.log.restore_points.len(),
        "modal": app.modal.as_ref().map(|m| format!("{m:?}")),
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "busy": app.busy,
        "snaps": app.snaps.as_ref().map(|(a, b)| json!({ "before_plan": a.plan, "after_plan": b.plan, "before_timer": a.timer.map(|t| t.0), "after_timer": b.timer.map(|t| t.0), "before_wake": a.wake.map(|w| w.median_us), "after_wake": b.wake.map(|w| w.median_us) })),
        "wake": app.wake.map(|w| json!({ "median_us": w.median_us, "p99_us": w.p99_us, "n": w.n })),
        "live": { "samples": l.samples, "plan": l.plan, "cpu": l.cpu_name, "cpu_mhz": l.cpu_mhz, "base_mhz": l.base_mhz, "timer": l.timer.map(|t| vec![t.0, t.1, t.2]), "mem_used": l.mem_used, "mem_total": l.mem_total, "gpus": l.gpus.iter().map(|g| json!({ "name": g.name, "clock": g.clock })).collect::<Vec<_>>() },
        "limits": app.limits.iter().map(|r| r.to_vec()).collect::<Vec<_>>(),
        "expanded": app.expanded.iter().collect::<Vec<_>>(),
        "settings": serde_json::to_value(&app.set).unwrap_or_default(),
        "scroll": app.scroll(),
        "content_h": app.content_h,
        "notes": app.notes,
        "hwnd": app.win.hwnd.0 as isize,
        "gpu": {
            "cards": app.cards.iter().map(|c| json!({ "id": c.id, "name": c.name, "tunable": c.caps.tunable(), "why_not": c.why_not, "knobs": c.caps.knobs.iter().map(|(k, r)| json!([k.key(), r.min, r.max, r.default])).collect::<Vec<_>>() })).collect::<Vec<_>>(),
            "card": app.set.gpu_card,
            "now": app.gpu_now,
            "draft": app.draft,
            "changed": app.draft_changed(),
            "trying": app.trying.as_ref().map(|t| json!({ "card": t.card, "secs_left": app.keep_secs_left(), "result": t.result })),
            "stress": app.stress.as_ref().map(|s| json!({ "done": s.done, "total": s.total })),
            "profiles": app.profiles.list.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            "signin": app.profiles.signin,
            "signin_blocked": app.set.gpu_signin_blocked,
            "notice": app.set.gpu_notice,
            "guard": crate::gpu::Guard::load().is_some(),
            "tel": app.live.tel,
            "monitor": app.monitor.as_ref().map(|w| w.hwnd.0 as isize),
        },
        "snapshots": app.snaps_list.iter().map(|(d, m)| json!({ "dir": d.display().to_string(), "why": format!("{:?}", m.why), "reg": m.reg_values, "plans": m.plans, "services": m.services, "gpus": m.gpus })).collect::<Vec<_>>(),
        "backup_line": app.backup_line,
        "backup_installed": crate::backup_link::exe().is_some(),
        "attribs_shown": app.attribs_shown,
        "uptime_ms": app.started.elapsed().as_millis() as u64,
    })
}
