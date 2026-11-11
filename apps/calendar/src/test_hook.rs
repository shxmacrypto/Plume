//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_CALENDAR_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                                   the window's state (view, dates, occurrences, overlays, log)
//!   {"cmd":"hits"}                                    clickable elements and their rectangles
//!   {"cmd":"click","id":"Tab(Week)"}                  a click on an element (its name as `hits` gives it); "dclick", "rclick"
//!   {"cmd":"point","x":500,"y":300,"clicks":1}        a click at a point
//!   {"cmd":"drag","from":"Occ(2)","to":[640,410]}     a drag (from an element or a point, to an element or a point)
//!   {"cmd":"type","text":"…"}                         text in the open field
//!   {"cmd":"key","vk":13}                             a key (to the field when it is open)
//!   {"cmd":"go","date":"2026-11-11","view":"week"}    shows a date in a view
//!   {"cmd":"add","title":"…","start":"2026-11-12T09:00","end":"…","all_day":false,"rrule":"FREQ=WEEKLY","alarms":[600]}
//!   {"cmd":"quick","text":"Lunch with Max tomorrow 12:30"}
//!   {"cmd":"import","path":"C:\\…\\x.ics"}            imports a file
//!   {"cmd":"subscribe","url":"https://…"}             subscribes (the download runs in the background)
//!   {"cmd":"now","t":"2026-11-12T12:20"}              moves the clock (local time), then {"cmd":"tick"} checks reminders
//!   {"cmd":"set","key":"theme","value":"light"}       a setting
//!   {"cmd":"scroll","y":400}                          the hour grid's scroll
//!   {"cmd":"quit"}                                    really quits

use crate::{
    date::{DAY, Date, Wall},
    model::{Event, new_uid},
    recur::RRule,
    tz,
    ui::{App, Drag, View},
};
use plume_ui::{Button, Cx, Event as UiEvent, Handler, Mods, Pt, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

pub fn start(win: Window, quitting: Rc<Cell<bool>>) {
    let Some(port) = std::env::var("PLUME_CALENDAR_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
    // the window hook's flag, set on the window's thread by a "quit" command
    QUIT.with(|q| *q.borrow_mut() = Some(quitting));
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

thread_local! {
    static QUIT: std::cell::RefCell<Option<Rc<Cell<bool>>>> = const { std::cell::RefCell::new(None) };
}

/// "2026-11-12T09:30" (local) or "2026-11-12".
fn local(s: &str) -> Option<(Date, Option<u32>)> {
    let (d, t) = match s.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let date = Date::parse_iso(d)?;
    let secs = match t {
        Some(t) => {
            let mut it = t.split(':');
            let h: u32 = it.next()?.parse().ok()?;
            let m: u32 = it.next().unwrap_or("0").parse().ok()?;
            Some(h * 3600 + m * 60)
        }
        None => None,
    };
    Some((date, secs))
}

fn target(app: &App, v: &Value) -> Option<Pt> {
    if let Some(a) = v.as_array() {
        return Some(pt(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32));
    }
    let want = v.as_str()?;
    let r = app.hits.rect_of(|id| format!("{id:?}") == want)?;
    Some(pt(r.x + r.w / 2.0, r.y + r.h / 2.0))
}

fn click_at(app: &mut App, p: Pt, button: Button, clicks: u8, cx: &mut Cx) {
    let mods = Mods::default();
    app.event(UiEvent::MouseMove { p, mods }, cx);
    app.event(UiEvent::MouseDown { p, button, clicks: 1, mods }, cx);
    if button == Button::Left {
        app.event(UiEvent::MouseUp { p, button, mods }, cx);
    }
    if clicks >= 2 {
        app.event(UiEvent::MouseDown { p, button, clicks: 2, mods }, cx);
        app.event(UiEvent::MouseUp { p, button, mods }, cx);
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
        let _ = tx.send(out.to_string());
    });
    if !sent {
        return json!({ "error": "window closed" }).to_string();
    }
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn command(app: &mut App, cmd: &Value, cx: &mut Cx) -> Value {
    let ok = json!({ "ok": true });
    match cmd["cmd"].as_str().unwrap_or("") {
        "state" => state(app),
        "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
        c @ ("click" | "dclick" | "rclick") => match target(app, &cmd["id"]) {
            Some(p) => {
                let (button, clicks) = match c {
                    "rclick" => (Button::Right, 1),
                    "dclick" => (Button::Left, 2),
                    _ => (Button::Left, 1),
                };
                click_at(app, p, button, clicks, cx);
                ok
            }
            None => json!({ "error": "no such target" }),
        },
        "point" => {
            let p = pt(cmd["x"].as_f64().unwrap_or(0.0) as f32, cmd["y"].as_f64().unwrap_or(0.0) as f32);
            click_at(app, p, Button::Left, cmd["clicks"].as_u64().unwrap_or(1) as u8, cx);
            ok
        }
        "drag" => {
            let (Some(a), Some(b)) = (target(app, &cmd["from"]), target(app, &cmd["to"])) else { return json!({ "error": "no such target" }) };
            let mods = Mods::default();
            app.event(UiEvent::MouseMove { p: a, mods }, cx);
            app.event(UiEvent::MouseDown { p: a, button: Button::Left, clicks: 1, mods }, cx);
            let steps = cmd["steps"].as_u64().unwrap_or(8).max(1);
            for k in 1..=steps {
                let f = k as f32 / steps as f32;
                app.event(UiEvent::MouseMove { p: pt(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f), mods }, cx);
            }
            let during = format!("{:?}", app.drag);
            app.event(UiEvent::MouseUp { p: b, button: Button::Left, mods }, cx);
            json!({ "ok": true, "drag": during, "after": format!("{:?}", matches!(app.drag, Drag::None)) })
        }
        "type" => {
            let text = cmd["text"].as_str().unwrap_or("").to_string();
            app.win.edit_set_text(&text);
            app.event(UiEvent::EditChanged(text), cx);
            ok
        }
        "key" => {
            let vk = cmd["vk"].as_u64().unwrap_or(0) as u16;
            let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: false };
            if app.edit_for.is_some() && matches!(vk, 0x0D | 0x1B | 0x09) {
                app.event(UiEvent::EditKey { vk, mods }, cx);
            } else {
                app.event(UiEvent::Key { vk, mods, repeat: false }, cx);
            }
            ok
        }
        "go" => {
            let view = match cmd["view"].as_str().unwrap_or("") {
                "day" => View::Day,
                "week" => View::Week,
                "agenda" => View::Agenda,
                "month" => View::Month,
                _ => app.view,
            };
            let date = cmd["date"].as_str().and_then(local).map(|x| x.0).unwrap_or(app.anchor);
            app.go(view, date);
            cx.repaint();
            ok
        }
        "add" => {
            let z = app.zone();
            let all_day = cmd["all_day"].as_bool().unwrap_or(false);
            let Some((sd, st)) = cmd["start"].as_str().and_then(local) else { return json!({ "error": "start" }) };
            let (ed, et) = cmd["end"].as_str().and_then(local).unwrap_or((sd, st.map(|s| s + 3600)));
            let (start, end) = if all_day {
                (sd.days() * DAY, ed.max(sd).days() * DAY + DAY)
            } else {
                (z.to_utc(Wall { date: sd, secs: st.unwrap_or(9 * 3600) }), z.to_utc(Wall { date: ed, secs: et.unwrap_or(st.unwrap_or(9 * 3600) + 3600) }))
            };
            let cal = cmd["cal"].as_u64().map(|c| c as usize).or(app.store.default_cal()).unwrap_or(0);
            let ev = Event {
                uid: new_uid(),
                title: cmd["title"].as_str().unwrap_or("Event").into(),
                location: cmd["location"].as_str().unwrap_or("").into(),
                notes: cmd["notes"].as_str().unwrap_or("").into(),
                color: cmd["color"].as_str().map(String::from),
                all_day,
                start,
                end,
                tzid: if all_day { String::new() } else { z.tzid() },
                rrule: cmd["rrule"].as_str().and_then(RRule::parse),
                alarms: cmd["alarms"].as_array().map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default(),
                stamp: tz::now(),
                ..Default::default()
            };
            app.store.merge(cal, vec![ev]);
            cx.repaint();
            ok
        }
        "quick" => {
            let text = cmd["text"].as_str().unwrap_or("").to_string();
            json!({ "ok": app.quick_add(&text, cx) })
        }
        "import" => {
            app.import_file(std::path::Path::new(cmd["path"].as_str().unwrap_or("")), cx);
            ok
        }
        "subscribe" => {
            app.subscribe(cmd["url"].as_str().unwrap_or(""), cx);
            ok
        }
        "now" => {
            app.now_override = cmd["t"].as_str().and_then(local).map(|(d, s)| app.zone().to_utc(Wall { date: d, secs: s.unwrap_or(0) }));
            if let Some(t) = app.now_override
                && cmd["reset"].as_bool().unwrap_or(true)
            {
                app.last_check = t - 1;
            }
            cx.repaint();
            ok
        }
        "hover" => {
            let p = pt(cmd["x"].as_f64().unwrap_or(0.0) as f32, cmd["y"].as_f64().unwrap_or(0.0) as f32);
            app.event(UiEvent::MouseMove { p, mods: Mods::default() }, cx);
            ok
        }
        "leave" => {
            app.event(UiEvent::MouseLeave, cx);
            ok
        }
        "place" => {
            // moves the window (pixels), without activating it: screenshots need it whole on the screen
            use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos};
            let (x, y) = (cmd["x"].as_i64().unwrap_or(0) as i32, cmd["y"].as_i64().unwrap_or(0) as i32);
            let _ = unsafe { SetWindowPos(app.win.hwnd, None, x, y, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER) };
            cx.repaint();
            ok
        }
        "dismiss_toast" => {
            app.toast = None;
            cx.repaint();
            ok
        }
        "tick" => {
            app.check_reminders(cx);
            cx.repaint();
            json!({ "ok": true, "alerts": app.alerts.len() })
        }
        "set" => {
            let k = cmd["key"].as_str().unwrap_or("").to_string();
            let v = cmd["value"].as_str().unwrap_or("").to_string();
            app.change_setting(&k, &v, cx);
            ok
        }
        "scroll" => {
            app.scroll = cmd["y"].as_f64().unwrap_or(0.0) as f32;
            cx.repaint();
            ok
        }
        "repaint" => {
            cx.repaint();
            ok
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
            ok
        }
        _ => json!({ "error": "unknown command" }),
    }
}

fn state(app: &mut App) -> Value {
    app.refresh_occs();
    let z = app.zone();
    let h24 = app.h24();
    let fmt = |t: i64| {
        let w = z.to_wall(t);
        format!("{}T{}", w.date.iso(), crate::i18n::time(w.secs, true))
    };
    let occs: Vec<Value> = app
        .occs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let e = app.store.event(o);
            json!({ "i": i, "title": e.title, "start": fmt(o.start), "end": fmt(o.end), "all_day": o.all_day, "cal": app.store.cals[o.cal].name, "series": e.rrule.is_some() || e.recurrence_id.is_some(), "location": e.location })
        })
        .collect();
    let (a, b) = app.range();
    json!({
        "view": format!("{:?}", app.view),
        "anchor": app.anchor.iso(),
        "selected": app.selected.iso(),
        "range": [a.iso(), b.iso()],
        "today": app.today().iso(),
        "title": app.period_title(),
        "occs": occs,
        "calendars": app.store.cals.iter().zip(app.store.events.iter()).map(|(c, e)| json!({ "name": c.name, "color": c.color, "visible": c.visible, "url": c.url, "events": e.len(), "error": c.error, "refreshed": c.refreshed })).collect::<Vec<_>>(),
        "refreshing": app.refreshing,
        "editor": app.editor.as_ref().map(|e| json!({ "title": e.title, "all_day": e.all_day, "start": format!("{}T{}", e.start.date.iso(), crate::i18n::time(e.start.secs, true)), "end": format!("{}T{}", e.end.date.iso(), crate::i18n::time(e.end.secs, true)), "freq": e.freq, "rule": e.rule().map(|r| r.to_ics()), "error": e.error, "new": e.target.is_none() })),
        "peek": app.peek.as_ref().map(|p| app.store.events[p.occ.cal][p.occ.ev].title.clone()),
        "dialog": app.dialog.as_ref().map(|d| format!("{:?}: {}", d.kind, d.text)),
        "scope": app.scope.is_some(),
        "confirm": app.confirm.as_ref().map(|c| c.text.clone()),
        "menu": app.menu.as_ref().map(|m| m.items.iter().map(|i| i.0.clone()).collect::<Vec<_>>()),
        "toast": app.toast.as_ref().map(|t| t.text.clone()),
        "alerts": app.alerts.iter().map(|d| json!({ "title": d.title, "before": d.before, "at": fmt(d.at) })).collect::<Vec<_>>(),
        "snoozed": app.snoozed.len(),
        "popup": app.popup.is_some(),
        "edit_for": app.edit_for.map(|f| format!("{f:?}")),
        "settings_open": app.settings_open,
        "settings": serde_json::to_value(&app.set).unwrap_or_default(),
        "autostart": app.autostart,
        "h24": h24,
        "scroll": app.scroll,
        "drag": format!("{:?}", app.drag),
        "log": app.log,
        "tz": z.tzid(),
        "hwnd": app.win.hwnd.0 as isize,
        "pid": std::process::id(),
    })
}
