//! Prise d'essai : un script pilote la fenêtre sans toucher à la souris ni au clavier de l'utilisateur.
//! Elle n'existe que dans une compilation `--features test-hook`, et ne s'ouvre que si la variable
//! PLUME_FEX_TEST_PORT est définie. Protocole : une commande JSON par ligne sur 127.0.0.1, une réponse par ligne.

use crate::{
    app::{
        App, LIST_TOP, ROW_H,
        model::{Load, Location},
    },
    ui::{Button, Cx, Event, Handler, Mods, Pt, Sender, Window, pt},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

pub fn start(win: Window) {
    let Some(port) = std::env::var("PLUME_FEX_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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
                let reply = run(&sender, line.trim());
                if writeln!(out, "{reply}").is_err() {
                    break;
                }
            }
        }
    });
}

/// Exécute une commande sur le fil de l'interface et attend sa réponse.
fn run(sender: &Sender, line: &str) -> String {
    let Ok(cmd) = serde_json::from_str::<Value>(line) else { return json!({ "error": "bad json" }).to_string() };
    let (tx, rx) = mpsc::channel();
    let sent = sender.send(move |h, cx| {
        let out = match h.as_any().downcast_mut::<App>() {
            Some(app) => exec(app, &cmd, cx),
            None => json!({ "error": "no app" }),
        };
        let _ = tx.send(out.to_string());
    });
    if !sent {
        return json!({ "error": "window closed" }).to_string();
    }
    rx.recv_timeout(Duration::from_secs(15)).unwrap_or_else(|_| json!({ "error": "timeout" }).to_string())
}

fn mods(cmd: &Value) -> Mods {
    let b = |k: &str| cmd.get(k).and_then(Value::as_bool).unwrap_or(false);
    Mods { ctrl: b("ctrl"), shift: b("shift"), alt: b("alt") }
}

/// Le point visé : `id` (« Back », « Nav(2) », « Row(5) »…) ou `x`, `y`.
fn point(app: &App, cmd: &Value) -> Option<Pt> {
    if let Some(id) = cmd.get("id").and_then(Value::as_str) {
        if let Some(n) = id.strip_prefix("Row(").and_then(|r| r.strip_suffix(')')).and_then(|n| n.parse::<usize>().ok()) {
            let l = app.lay.list;
            // en vignettes : le milieu de la case
            if app.set.thumbs {
                let r = app.tile_rect(n);
                return Some(pt(r.x + r.w / 2.0, r.y + r.h / 2.0));
            }
            let y = l.y + LIST_TOP + n as f32 * ROW_H - app.tab().scroll + ROW_H / 2.0;
            // « blank » : dans le vide à droite du nom plutôt que sur le nom
            let x = if cmd.get("blank").and_then(Value::as_bool).unwrap_or(false) { l.right() - 60.0 } else { l.x + 44.0 };
            return Some(pt(x, y));
        }
        if id == "List" {
            let l = app.lay.list;
            return Some(pt(l.x + l.w / 2.0, l.bottom() - 12.0));
        }
        return app.hits.iter().rev().find(|(_, h)| format!("{h:?}") == id).map(|(r, _)| pt(r.x + r.w / 2.0, r.y + r.h / 2.0));
    }
    Some(pt(cmd.get("x")?.as_f64()? as f32, cmd.get("y")?.as_f64()? as f32))
}

fn loc_text(loc: &Location) -> String {
    match loc {
        Location::ThisPc => "pc".into(),
        Location::Path(p) => p.display().to_string(),
        Location::Search { query, .. } => format!("search:{query}"),
        Location::Remote { server, path } => Location::remote_url(server, path),
    }
}

fn state(app: &App) -> Value {
    let tab = app.tab();
    let rows = tab.rows();
    let first = ((tab.scroll - LIST_TOP) / ROW_H).floor().max(0.0) as usize;
    let visible: Vec<&str> = (first..rows.min(first + 60)).map(|r| tab.item(r).name.as_str()).collect();
    let r4 = |r: crate::ui::Rect| json!([r.x, r.y, r.w, r.h]);
    json!({
        "tabs": app.tabs.iter().map(|t| json!({ "title": app.title_of(&t.loc), "loc": loc_text(&t.loc) })).collect::<Vec<_>>(),
        "cur": app.cur,
        "loc": loc_text(&tab.loc),
        "title": app.title_of(&tab.loc),
        "load": match &tab.load { Load::Ready => "ready".to_string(), Load::Loading(_) => "loading".to_string(), Load::Failed(m) => format!("failed: {m}") },
        "stale": tab.stale,
        "rows": rows,
        "items": tab.items.len(),
        "sel": tab.selected_names(),
        "sel_count": tab.sel_count,
        "sel_size": tab.sel_size,
        "focus": tab.focus,
        "focus_name": tab.focus.filter(|&f| f < rows).map(|f| tab.item(f).name.clone()),
        "anchor": tab.anchor,
        "scroll": tab.scroll,
        "target": tab.target,
        "max_scroll": app.lay.max_scroll,
        "filter": tab.filter,
        "sort": format!("{:?}", tab.sort),
        "search_sort": format!("{:?}", tab.search_sort.0),
        "search_asc": tab.search_sort.1,
        "cols": app.columns().iter().map(|(col, x, w)| json!([format!("{col:?}"), x, w])).collect::<Vec<_>>(),
        "folder_sizes": app.set.folder_sizes,
        "thumbs": app.set.thumbs,
        "icon_size": app.set.icon_size,
        "tile": [app.tile().w, app.tile().h],
        "transfer": app.transfer.as_ref().map(|tr| json!({ "done": tr.done.load(std::sync::atomic::Ordering::Relaxed), "total": tr.total.load(std::sync::atomic::Ordering::Relaxed) })),
        "grid_cols": app.grid_cols(),
        "thumbs_ready": app.thumbs_ready(),
        "preview": {
            "on": app.set.preview,
            "shown": app.lay.preview.w > 0.0,
            "path": app.preview.path.as_ref().map(|p| p.display().to_string()),
            "kind": match &app.preview.content {
                crate::app::preview::Content::Loading => "loading",
                crate::app::preview::Content::Image(..) => "image",
                crate::app::preview::Content::Text(_) => "text",
                crate::app::preview::Content::Folder(_) => "folder",
                crate::app::preview::Content::Info => "info",
            },
            "text": match &app.preview.content { crate::app::preview::Content::Text(t) => t.chars().take(200).collect::<String>(), _ => String::new() },
            "size": match &app.preview.content { crate::app::preview::Content::Image(_, w, h, _) => vec![*w, *h], _ => vec![] },
            "count": match &app.preview.content { crate::app::preview::Content::Folder(n) => *n, _ => None },
        },
        "sizes": (0..rows.min(first + 60)).skip(first).map(|r| { let it = tab.item(r); (!it.is_dir() || it.sized).then_some(it.size) }).collect::<Vec<_>>(),
        "hwnd": app.win.hwnd.0 as isize,
        "maximized": unsafe { windows::Win32::UI::WindowsAndMessaging::IsZoomed(app.win.hwnd).as_bool() },
        "asc": tab.asc,
        "edit": app.edit.as_ref().map(|e| format!("{e:?}")),
        "edit_text": app.win.edit_text(),
        "first_row": first,
        "visible": visible,
        "nav": app.nav_rows.iter().map(|n| n.label.clone()).collect::<Vec<_>>(),
        "nav_kinds": app.nav_rows.iter().map(|n| format!("{:?}", n.kind)).collect::<Vec<_>>(),
        "nav_dim": app.nav_rows.iter().map(|n| n.dim).collect::<Vec<_>>(),
        "nav_scroll": app.nav_scroll,
        "recent": app.set.recent.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "show_recent": app.set.show_recent,
        "quick_order": app.set.quick_order.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "pinned": app.set.pinned.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "categories": app.set.categories.iter().map(|c| json!({
            "name": c.name, "color": c.color, "open": c.open,
            "folders": c.folders.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "panels": app.panels.iter().map(|p| json!({
            "id": p.id,
            "dir": p.dir.display().to_string(),
            "rows": p.items.iter().map(|it| it.name.clone()).collect::<Vec<_>>(),
            "sel": p.items.iter().filter(|it| it.sel).map(|it| it.name.clone()).collect::<Vec<_>>(),
            "ready": p.load == crate::app::panel::PLoad::Ready,
            "rect": r4(p.rect),
        })).collect::<Vec<_>>(),
        "undo": app.undo.len(),
        "toast": app.toast.as_ref().map(|t| t.0.clone()),
        "took_ms": tab.took_ms,
        "total": tab.total,
        "busy": app.busy.as_ref().map(|b| b.0.clone()),
        "sheet": app.sheet.as_ref().map(|s| json!({
            "kind": match &s.kind {
                crate::app::sheet::SheetKind::Lock { format, rar, .. } => format!("lock:{format:?}:{rar}"),
                crate::app::sheet::SheetKind::Unlock { .. } => "unlock".to_string(),
                crate::app::sheet::SheetKind::Connect => "connect".to_string(),
                crate::app::sheet::SheetKind::Secret { server, prompt, secret, .. } => format!("secret:{server}:{secret}:{prompt}"),
                crate::app::sheet::SheetKind::Delete { names, .. } => format!("delete:{}", names.len()),
            },
            "error": s.error,
            "reveal": s.reveal,
        })),
        "index_ready": app.ix.is_ready(),
        "index_asleep": app.ix.sleeping(),
        "index_count": app.ix.progress.entries.load(std::sync::atomic::Ordering::Relaxed),
        "everywhere": app.set.search_everywhere,
        "search_kind": format!("{:?}", app.search_opts.kind),
        "search_match": { "case": app.search_opts.case, "word": app.search_opts.word, "path": app.search_opts.path, "regex": app.search_opts.regex, "content": app.search_opts.content },
        "folders": (0..rows.min(first + 60)).skip(first).map(|r| tab.item(r).folder.as_ref().map(|f| f.display().to_string())).collect::<Vec<_>>(),
        "can_back": !tab.back.is_empty(),
        "can_forward": !tab.fwd.is_empty(),
        "hover": app.hover.map(|h| format!("{h:?}")),
        "drag": format!("{:?}", app.drag).split([' ', '{', '(']).next().unwrap_or("").to_string(),
        "dark": app.theme.dark,
        "lang": app.s.lang.code(),
        "show_hidden": app.set.show_hidden,
        "lay": { "w": app.lay.w, "nav": r4(app.lay.nav), "list": r4(app.lay.list), "addr": r4(app.lay.addr), "search": r4(app.lay.search) },
    })
}

fn exec(app: &mut App, cmd: &Value, cx: &mut Cx) -> Value {
    let name = cmd.get("cmd").and_then(Value::as_str).unwrap_or("");
    let text = |k: &str| cmd.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let vk = cmd.get("vk").and_then(Value::as_u64).unwrap_or(0) as u16;
    match name {
        "state" => return state(app),
        "hits" => {
            return Value::Array(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect());
        }
        "names" => {
            let tab = app.tab();
            let from = cmd.get("from").and_then(Value::as_u64).unwrap_or(0) as usize;
            let count = cmd.get("count").and_then(Value::as_u64).unwrap_or(100) as usize;
            return Value::Array((from..tab.rows().min(from + count)).map(|r| json!(tab.item(r).name)).collect());
        }
        "mouse" => {
            let Some(p) = point(app, cmd) else { return json!({ "error": "no such target" }) };
            let m = mods(cmd);
            let button = match text("button").as_str() {
                "right" => Button::Right,
                "middle" => Button::Middle,
                "back" => Button::Back,
                "forward" => Button::Forward,
                _ => Button::Left,
            };
            match text("type").as_str() {
                "move" => app.event(Event::MouseMove { p, mods: m }, cx),
                "down" => {
                    app.event(Event::MouseMove { p, mods: m }, cx);
                    app.event(Event::MouseDown { p, button, clicks: 1, mods: m }, cx);
                }
                "up" => app.event(Event::MouseUp { p, button, mods: m }, cx),
                "dblclick" => {
                    app.event(Event::MouseMove { p, mods: m }, cx);
                    app.event(Event::MouseDown { p, button, clicks: 1, mods: m }, cx);
                    app.event(Event::MouseUp { p, button, mods: m }, cx);
                    app.event(Event::MouseDown { p, button, clicks: 2, mods: m }, cx);
                    app.event(Event::MouseUp { p, button, mods: m }, cx);
                }
                "wheel" => {
                    let dy = cmd.get("dy").and_then(Value::as_f64).unwrap_or(-1.0) as f32;
                    app.event(Event::Wheel { p, dx: 0.0, dy, mods: m }, cx);
                }
                _ => {
                    app.event(Event::MouseMove { p, mods: m }, cx);
                    app.event(Event::MouseDown { p, button, clicks: 1, mods: m }, cx);
                    app.event(Event::MouseUp { p, button, mods: m }, cx);
                }
            }
            return json!({ "ok": true, "x": p.x, "y": p.y });
        }
        "key" => app.event(Event::Key { vk, mods: mods(cmd), repeat: false }, cx),
        "char" => text("s").chars().for_each(|ch| app.event(Event::Char(ch), cx)),
        // saisie dans le champ ouvert, comme si l'utilisateur l'avait tapée
        "edit" => {
            let t = text("text");
            app.win.edit_set_text(&t);
            app.event(Event::EditChanged(t), cx);
        }
        "editkey" => app.event(Event::EditKey { vk, mods: mods(cmd) }, cx),
        "nav" => {
            let p = text("path");
            app.navigate(if p == "pc" { Location::ThisPc } else { Location::Path(PathBuf::from(p)) });
        }
        "active" => app.event(Event::Active(cmd.get("on").and_then(Value::as_bool).unwrap_or(true)), cx),
        // les menus ne s'affichent plus : ils notent leurs entrées et « cliquent » celle désignée par `choose`
        "menu" => crate::shell::menu::test::TEST.with(|t| {
            let mut t = t.borrow_mut();
            t.on = true;
            t.choose = cmd.get("choose").and_then(Value::as_str).map(str::to_string);
            t.last.clear();
        }),
        // glisser-déposer : le dossier qui recevrait un dépôt au point visé (rien n'est déposé)
        "drop_at" => {
            let p = if cmd.get("leave").is_some() { None } else { point(app, cmd) };
            let path = app.drop_at(p);
            return json!({ "path": path.map(|p| p.display().to_string()), "highlight": app.drop_hl.map(|h| format!("{h:?}")) });
        }
        "last_drag" => return crate::shell::dnd::LAST_DRAG.with(|d| json!(d.borrow().iter().map(|p| p.display().to_string()).collect::<Vec<_>>())),
        "menu_last" => return crate::shell::menu::test::TEST.with(|t| json!(t.borrow().last)),
        "shell_labels" => return json!(crate::shell::menu::shell_menu_labels(&app.win, &app.tab().selected_paths())),
        "close" => cx.after(|w| w.close()),
        _ => return json!({ "error": format!("unknown command: {name}") }),
    }
    cx.repaint();
    json!({ "ok": true })
}
