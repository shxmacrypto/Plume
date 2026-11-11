//! Test socket: a script drives the window without the user's mouse or keyboard. Only in a `--features test-hook`
//! build, and only when PLUME_MEDIA_TEST_PORT is set. One JSON command per line on 127.0.0.1, one answer per line.
//!   {"cmd":"state"}                          the window's state
//!   {"cmd":"hits"}                           clickable elements and their rectangles
//!   {"cmd":"click","id":"Tile(0)"}           a click on an element (its name as `hits` gives it); "clicks":2 doubles
//!   {"cmd":"rclick","id":"Tile(0)"}          a right click
//!   {"cmd":"move","x":100,"y":200}           the pointer moves there (hover, seek bar preview)
//!   {"cmd":"key","vk":39}                    a key ("ctrl"/"shift": true)
//!   {"cmd":"drag","x":600,"y":400,"x2":500,"y2":300}  a drag with the left button (pan, seek bar, volume)
//!   {"cmd":"wheel","x":600,"y":400,"dy":1}   the wheel ("ctrl": true: the touchpad's pinch)
//!   {"cmd":"open","path":"C:\\…\\a.jpg"}     opens a file as Explorer would
//!   {"cmd":"go","page":"timeline"}           timeline, videos, folders, favorites, settings, folder:<path>
//!   {"cmd":"set","key":"zoom","value":"fill"} a setting
//!   {"cmd":"seek","t":4.5}                   the video's position
//!   {"cmd":"scroll","y":800}                 the library's scroll position
//!   {"cmd":"rescan"} {"cmd":"repaint"} {"cmd":"back"}

use crate::ui::{App, Page};
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
    let Some(port) = std::env::var("PLUME_MEDIA_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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

fn click(app: &mut App, want: &str, right: bool, clicks: u8, cx: &mut plume_ui::Cx) -> Value {
    match app.hits.rect_of(|id| format!("{id:?}") == want) {
        Some(r) => {
            let p = pt(r.x + r.w / 2.0, r.y + r.h / 2.0);
            let mods = Mods::default();
            let button = if right { Button::Right } else { Button::Left };
            app.event(Event::MouseMove { p, mods }, cx);
            for k in 1..=clicks.max(1) {
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
            Some(app) => {
                let mods = Mods { ctrl: cmd["ctrl"].as_bool().unwrap_or(false), shift: cmd["shift"].as_bool().unwrap_or(false), alt: false };
                let r = match cmd["cmd"].as_str().unwrap_or("") {
                    "state" => state(app),
                    "hits" => json!(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect::<Vec<_>>()),
                    "click" => click(app, cmd["id"].as_str().unwrap_or(""), false, cmd["clicks"].as_u64().unwrap_or(1) as u8, cx),
                    "rclick" => click(app, cmd["id"].as_str().unwrap_or(""), true, 1, cx),
                    "move" => {
                        let p = pt(cmd["x"].as_f64().unwrap_or(0.0) as f32, cmd["y"].as_f64().unwrap_or(0.0) as f32);
                        app.event(Event::MouseMove { p, mods }, cx);
                        json!({ "ok": true })
                    }
                    "drag" => {
                        let a = pt(cmd["x"].as_f64().unwrap_or(0.0) as f32, cmd["y"].as_f64().unwrap_or(0.0) as f32);
                        let b = pt(cmd["x2"].as_f64().unwrap_or(0.0) as f32, cmd["y2"].as_f64().unwrap_or(0.0) as f32);
                        app.event(Event::MouseMove { p: a, mods }, cx);
                        app.event(Event::MouseDown { p: a, button: Button::Left, clicks: 1, mods }, cx);
                        for k in 1..=8 {
                            let t = k as f32 / 8.0;
                            app.event(Event::MouseMove { p: pt(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t), mods }, cx);
                        }
                        app.event(Event::MouseUp { p: b, button: Button::Left, mods }, cx);
                        json!({ "ok": true })
                    }
                    "key" => {
                        app.event(Event::Key { vk: cmd["vk"].as_u64().unwrap_or(0) as u16, mods, repeat: false }, cx);
                        json!({ "ok": true })
                    }
                    "wheel" => {
                        let p = pt(cmd["x"].as_f64().unwrap_or(0.0) as f32, cmd["y"].as_f64().unwrap_or(0.0) as f32);
                        app.event(Event::Wheel { p, dx: 0.0, dy: cmd["dy"].as_f64().unwrap_or(1.0) as f32, mods }, cx);
                        json!({ "ok": true })
                    }
                    "open" => {
                        let p = PathBuf::from(cmd["path"].as_str().unwrap_or(""));
                        if p.is_file() {
                            app.open_file(p, None, cx);
                            json!({ "ok": true })
                        } else {
                            json!({ "error": "no such file" })
                        }
                    }
                    "back" => {
                        app.close_viewer(cx);
                        json!({ "ok": true })
                    }
                    "go" => {
                        let page = match cmd["page"].as_str().unwrap_or("") {
                            "timeline" => Page::Timeline,
                            "videos" => Page::Videos,
                            "folders" => Page::Folders,
                            "favorites" => Page::Favorites,
                            "settings" => Page::Settings,
                            p => Page::Folder(PathBuf::from(p.trim_start_matches("folder:"))),
                        };
                        if app.open.is_some() {
                            app.close_viewer(cx);
                        }
                        app.go(page, cx);
                        json!({ "ok": true })
                    }
                    "set" => {
                        let k = cmd["key"].as_str().unwrap_or("");
                        let mut s = serde_json::to_value(&app.set).unwrap_or_default();
                        s[k] = cmd["value"].clone();
                        match serde_json::from_value(s) {
                            Ok(new) => {
                                let folders_changed = k == "folders";
                                app.set = new;
                                app.settings_changed(cx);
                                if folders_changed {
                                    app.rescan();
                                }
                                json!({ "ok": true })
                            }
                            Err(e) => json!({ "error": e.to_string() }),
                        }
                    }
                    "seek" => match app.player() {
                        Some(p) => {
                            p.seek(cmd["t"].as_f64().unwrap_or(0.0));
                            cx.repaint();
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no video" }),
                    },
                    "scroll" => {
                        app.scroll_to = (cmd["y"].as_f64().unwrap_or(0.0) as f32).clamp(0.0, app.max_scroll());
                        app.scroll = app.scroll_to;
                        cx.repaint();
                        json!({ "ok": true })
                    }
                    "rescan" => {
                        app.rescan();
                        json!({ "ok": true })
                    }
                    "repaint" => {
                        cx.repaint();
                        json!({ "ok": true })
                    }
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
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn state(app: &mut App) -> Value {
    let open = app.open.as_ref().map(|o| {
        let photo = o.photo.as_ref().map(|p| {
            json!({
                "decoded": p.dec.as_ref().map(|d| json!({ "w": d.w(), "h": d.h(), "full_w": d.full_w, "full_h": d.full_h, "full": d.full, "frames": d.frames.len() })),
                "error": p.error,
                "zoom": p.zoom,
                "fit": p.fit,
                "ox": p.ox,
                "oy": p.oy,
                "turns": p.turns,
                "written": p.written,
                "frame": p.frame,
            })
        });
        let video = o.video.as_ref().map(|v| {
            let pl = v.player.as_ref();
            json!({
                "loaded": v.loaded,
                "error": v.error,
                "playing": pl.is_some_and(|p| !p.paused()),
                "ended": pl.is_some_and(|p| p.ended()),
                "pos": pl.map(|p| p.position()),
                "dur": pl.map(|p| p.duration()),
                "muted": pl.map(|p| p.muted()),
                "volume": pl.map(|p| p.volume()),
                "has_video": pl.is_some_and(|p| p.has_video()),
                "size": [v.bmp_size.0, v.bmp_size.1],
                "frames": v.frames,
                "rate": v.rate,
                "tracks": v.tracks.iter().map(|t| json!({ "index": t.index, "label": t.label })).collect::<Vec<_>>(),
                "track": v.track,
                "subs": v.subs.len(),
                "sub": v.sub,
                "cues": v.cues.len(),
                "preview": v.preview.as_ref().map(|p| p.0),
            })
        });
        json!({ "path": o.path(), "pos": o.pos, "of": o.seq.len(), "photo": photo, "video": video })
    });
    json!({
        "page": format!("{:?}", app.page),
        "items": app.lib.len(),
        "list": app.list.len(),
        "videos": app.lib.iter().filter(|i| i.video).count(),
        "folders": app.folders.len(),
        "scanning": app.scanning.is_some(),
        "rows": app.rows.len(),
        "cols": app.cols,
        "tile": app.tile,
        "scroll": app.scroll,
        "content_h": app.content_h,
        "bitmaps": app.bitmaps.len(),
        "thumbs_pending": app.thumbs.values().filter(|t| **t == crate::ui::Thumb::Pending).count(),
        "open": open,
        "info": app.info.as_ref().map(|i| json!({ "path": i.path, "camera": i.exif.camera, "taken": i.exif.taken, "dims": i.dims, "size": i.size })),
        "fullscreen": app.full.is_some(),
        "slideshow": app.slideshow.is_some(),
        "menu": app.menu.as_ref().map(|m| m.items.iter().map(|i| i.0.clone()).collect::<Vec<_>>()),
        "toast": app.toast.as_ref().map(|t| t.0.clone()),
        "favorites": app.store.favorites.len(),
        "openwith": app.openwith,
        "settings": serde_json::to_value(&app.set).unwrap_or_default(),
        "dark": app.pal.dark,
        "frames": app.frames,
        "decoded_cache": app.decoded.iter().map(|d| d.0.display().to_string()).collect::<Vec<_>>(),
        "log": app.log,
        "hwnd": app.win.hwnd.0 as isize,
        "pid": std::process::id(),
    })
}
