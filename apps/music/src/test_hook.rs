//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_MUSIC_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                        the window's state
//!   {"cmd":"hits"}                         clickable elements and their rectangles
//!   {"cmd":"click","id":"Nav(Songs)"}      a click on an element (its name as `hits` gives it)
//!   {"cmd":"rclick","id":"Row(0)"}         a right click
//!   {"cmd":"type","text":"…"}              text in the open field (search, dialog)
//!   {"cmd":"key","vk":13}                  a key, given to the field when it is open
//!   {"cmd":"set","key":"background","value":"blur"}   a setting
//!   {"cmd":"import","link":"https://…"}    imports a link
//!   {"cmd":"search","q":"…"}               searches
//!   {"cmd":"play_result","i":0}            plays the n-th row of the page

use crate::ui::{App, View};
use plume_ui::{Button, Event, Handler, Mods, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::mpsc,
    time::Duration,
};

pub fn start(win: Window) {
    let Some(port) = std::env::var("PLUME_MUSIC_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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
                    if app.win.edit_visible() && matches!(vk, 0x0D | 0x1B) {
                        app.event(Event::EditKey { vk, mods: Mods::default() }, cx);
                    } else {
                        app.event(
                            Event::Key {
                                vk,
                                mods: Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: false },
                                repeat: false,
                            },
                            cx,
                        );
                    }
                    json!({ "ok": true })
                }
                "set" => {
                    let k = cmd["key"].as_str().unwrap_or("");
                    let v = &cmd["value"];
                    let mut s = serde_json::to_value(&app.set).unwrap_or_default();
                    s[k] = v.clone();
                    match serde_json::from_value(s) {
                        Ok(new) => {
                            app.set = new;
                            app.gif = None;
                            app.gif_failed = false;
                            app.settings_changed(cx);
                            json!({ "ok": true })
                        }
                        Err(e) => json!({ "error": e.to_string() }),
                    }
                }
                "go" => {
                    let v = match cmd["view"].as_str().unwrap_or("") {
                        "home" => View::Home,
                        "search" => View::Search,
                        "songs" => View::Songs,
                        "albums" => View::Albums,
                        "artists" => View::Artists,
                        "liked" => View::Liked,
                        "downloads" => View::Downloads,
                        "settings" => View::Settings,
                        p => View::Playlist(p.trim_start_matches("playlist:").to_string()),
                    };
                    app.go(v, cx);
                    json!({ "ok": true })
                }
                "now" => {
                    app.now_open = cmd["open"].as_bool().unwrap_or(true);
                    cx.repaint();
                    json!({ "ok": true })
                }
                "search" => {
                    app.query = cmd["q"].as_str().unwrap_or("").to_string();
                    app.go(View::Search, cx);
                    app.local_results = app.lib.search(&app.query);
                    app.run_search(cx);
                    json!({ "ok": true })
                }
                "import" => {
                    app.open_import(Some(cmd["link"].as_str().unwrap_or("").to_string()), cx);
                    json!({ "ok": true })
                }
                "play_result" => {
                    let i = cmd["i"].as_u64().unwrap_or(0) as usize;
                    app.play_rows(i, cx);
                    json!({ "ok": true })
                }
                "seek" => {
                    app.engine.send(crate::audio::Cmd::Seek(cmd["t"].as_f64().unwrap_or(0.0)));
                    json!({ "ok": true })
                }
                "scroll" => {
                    app.scroll_to = cmd["y"].as_f64().unwrap_or(0.0) as f32;
                    app.scroll = app.scroll_to;
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
    let cur = app.current().cloned();
    let modal = app.modal.as_ref().map(|m| format!("{m:?}"));
    json!({
        "view": format!("{:?}", app.view),
        "query": app.query,
        "results": app.results.len(),
        "local_results": app.local_results.len(),
        "searching": app.searching,
        "current": cur.as_ref().map(|t| json!({ "key": t.key, "title": t.title, "artist": t.artist, "liked": t.liked, "local": t.local() })),
        "loading": app.loading,
        "playing": app.st.playing,
        "pos": app.st.pos,
        "dur": app.st.dur,
        "buffering": app.st.buffering,
        "device_ok": app.st.device_ok,
        "queue": app.queue.len(),
        "qpos": app.qpos,
        "prepared": app.prepared.as_ref().map(|p| p.1.clone()),
        "now_open": app.now_open,
        "panel": format!("{:?}", app.panel),
        "lyrics": app.lyrics.as_ref().map(|(k, l)| json!({ "key": k, "found": l.is_some(), "synced": l.as_ref().is_some_and(|l| l.synced), "lines": l.as_ref().map(|l| l.lines.len()) })),
        "motion": app.motion.as_ref().map(|(k, m)| json!({ "album": k, "frames": m.frames.len() })),
        "gif": app.gif.as_ref().map(|g| json!({ "w": g.w, "h": g.h, "count": g.count })),
        "modal": modal,
        "menu": app.menu.as_ref().map(|m| m.items.iter().map(|i| i.0.clone()).collect::<Vec<_>>()),
        "toast": app.toast.as_ref().map(|(t, _)| t.clone()),
        "playlists": app.lib.playlists.iter().map(|p| json!({ "id": p.id, "name": p.name, "n": p.tracks.len() })).collect::<Vec<_>>(),
        "tracks": app.lib.tracks.len(),
        "liked": app.lib.liked().len(),
        "downloads": app.downloads.iter().map(|(k, d)| json!({ "key": k, "state": format!("{d:?}") })).collect::<Vec<_>>(),
        "settings": serde_json::to_value(&app.set).unwrap_or_default(),
        "accent": format!("{:?}", app.pal.accent),
        "dark": app.pal.dark,
        "scroll": app.scroll,
        "content_h": app.content_h,
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
    })
}
