//! Prise d'essai : un script pilote la fenêtre sans toucher à la souris ni au clavier de l'utilisateur.
//! Elle n'existe que dans une compilation `--features test-hook`, et ne s'ouvre que si la variable
//! PLUME_MAIL_TEST_PORT est définie. Protocole : une commande JSON par ligne sur 127.0.0.1, une réponse par ligne.

use crate::app::{App, Id, forms::Sheet};
use mail_core::engine::Work;
use plume_ui::{Button, Cx, Event, Handler, Mods, Pt, Sender, Window, pt};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::mpsc,
    time::Duration,
};

pub fn start(win: Window) {
    let Some(port) = std::env::var("PLUME_MAIL_TEST_PORT").ok().and_then(|p| p.parse::<u16>().ok()) else { return };
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

/// Le point visé : le milieu de la zone `id` (« Row(2) », « SheetOk »…), ou des coordonnées.
fn point(app: &App, cmd: &Value) -> Option<Pt> {
    if let Some(id) = cmd.get("id").and_then(Value::as_str) {
        return app.hits.iter().rev().find(|(_, h)| format!("{h:?}") == id).map(|(r, _)| pt(r.x + r.w / 2.0, r.y + r.h / 2.0));
    }
    Some(pt(cmd.get("x")?.as_f64()? as f32, cmd.get("y")?.as_f64()? as f32))
}

fn state(app: &App) -> Value {
    let first = (app.list_scroll / crate::app::ROW_H).floor() as usize;
    let rows: Vec<Value> = app
        .shown
        .iter()
        .zip(app.rows.iter())
        .skip(first)
        .take(40)
        .map(|(&i, info)| (&app.messages[i], info))
        .map(|(m, info)| json!({ "uid": m.uid, "subject": m.subject, "from": m.from, "from_name": m.from_name, "to": m.to, "seen": m.flags.seen, "flagged": m.flags.flagged, "attachments": m.attachments, "thread": info.count, "child": info.child }))
        .collect();
    json!({
        "accounts": app.boxes.iter().map(|mb| json!({
            "email": mb.account.email,
            "imap": format!("{}:{}", mb.account.imap_host, mb.account.imap_port),
            "smtp": format!("{}:{}", mb.account.smtp_host, mb.account.smtp_port),
            "folders": mb.folders.iter().map(|f| json!({ "name": f.name, "label": app.folder_label(f), "role": format!("{:?}", f.role), "depth": f.depth })).collect::<Vec<_>>(),
            "work": format!("{:?}", mb.work),
            "idle": mb.work == Work::Idle,
            "error": mb.error,
        })).collect::<Vec<_>>(),
        "cur": app.cur.as_ref().map(|(b, raw)| json!({ "account": b, "folder": raw })),
        "rows": app.shown.len(),
        "total": app.messages.len(),
        "messages": rows,
        "sel": app.sel,
        "reading": app.reading.as_ref().map(|r| json!({
            "uid": r.uid,
            "loading": r.message.is_none(),
            "subject": r.message.as_ref().map(|m| m.summary.subject.clone()),
            "from": r.message.as_ref().map(|m| m.summary.from.clone()),
            "text": r.message.as_ref().map(|m| m.text.clone()),
            "from_html": r.message.as_ref().map(|m| m.from_html),
            "attachments": r.message.as_ref().map(|m| m.attachments.iter().map(|a| json!({ "name": a.name, "size": a.size })).collect::<Vec<_>>()),
            "scroll": r.scroll,
            "max_scroll": r.max_scroll,
        })),
        "links": app.links(),
        "filter": app.filter,
        "edit": app.edit.map(|e| format!("{e:?}")),
        "edit_text": app.win.edit_text(),
        "sheet": app.sheet.as_ref().map(|s| match s {
            Sheet::Account(f) => json!({ "kind": "account", "fields": f.fields.iter().enumerate().map(|(i, v)| if i == 2 { "•".repeat(v.chars().count()) } else { v.clone() }).collect::<Vec<_>>(), "active": f.active, "error": f.error, "editing": f.editing }),
            Sheet::Compose(f) => json!({ "kind": "compose", "fields": f.fields, "active": f.active, "error": f.error, "sending": f.sending, "saving": f.saving, "draft": f.draft.as_ref().map(|d| d.1), "files": f.files.iter().map(|(n, d)| json!([n, d.len()])).collect::<Vec<_>>(), "in_reply_to": f.in_reply_to }),
            Sheet::Signature { account, text } => json!({ "kind": "signature", "account": account, "text": text }),
            Sheet::Confirm { title, action, .. } => json!({ "kind": "confirm", "title": title, "action": action }),
        }),
        "suggest": app.suggest.iter().map(|c| json!([c.name, c.email])).collect::<Vec<_>>(),
        "suggest_at": app.suggest_at,
        "found": app.found.as_ref().map(|(t, uids)| json!({ "text": t, "uids": uids })),
        "threads": app.set.threads,
        "signature": app.boxes.iter().map(|mb| mb.account.signature.clone()).collect::<Vec<_>>(),
        "menu": app.menu.as_ref().map(|m| m.items.iter().map(|(label, _)| label.clone()).collect::<Vec<_>>()),
        "toast": app.toast.as_ref().map(|t| t.0.clone()),
        "hover": app.hover.map(|h| format!("{h:?}")),
        "dark": app.theme.dark,
        "lang": format!("{:?}", app.s.lang),
        "lay": { "w": app.lay.w, "h": app.lay.h },
    })
}

fn exec(app: &mut App, cmd: &Value, cx: &mut Cx) -> Value {
    let text = |k: &str| cmd.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let vk = cmd.get("vk").and_then(Value::as_u64).unwrap_or(0) as u16;
    let name = text("cmd");
    match name.as_str() {
        "state" => return state(app),
        "hits" => return Value::Array(app.hits.iter().map(|(r, id)| json!({ "id": format!("{id:?}"), "rect": [r.x, r.y, r.w, r.h] })).collect()),
        "mouse" => {
            let Some(p) = point(app, cmd) else { return json!({ "error": "no such target" }) };
            let m = mods(cmd);
            match text("type").as_str() {
                "move" => app.event(Event::MouseMove { p, mods: m }, cx),
                "wheel" => {
                    let dy = cmd.get("dy").and_then(Value::as_f64).unwrap_or(-1.0) as f32;
                    app.event(Event::Wheel { p, dx: 0.0, dy, mods: m }, cx);
                }
                _ => {
                    app.event(Event::MouseMove { p, mods: m }, cx);
                    app.event(Event::MouseDown { p, button: Button::Left, clicks: 1, mods: m }, cx);
                    app.event(Event::MouseUp { p, button: Button::Left, mods: m }, cx);
                }
            }
            return json!({ "ok": true, "x": p.x, "y": p.y });
        }
        "key" => app.event(Event::Key { vk, mods: mods(cmd), repeat: false }, cx),
        // saisie dans le champ ouvert, comme si l'utilisateur l'avait tapée
        "edit" => {
            let t = text("text");
            app.win.edit_set_text(&t);
            app.event(Event::EditChanged(t), cx);
        }
        "editkey" => app.event(Event::EditKey { vk, mods: mods(cmd) }, cx),
        // ouvre un champ d'une boîte de dialogue, comme un clic dessus (un champ sous le champ de saisie ne se clique pas)
        "field" => {
            let i = cmd.get("i").and_then(Value::as_u64).unwrap_or(0) as usize;
            app.sheet_click(Id::Field(i), cx);
        }
        "close" => cx.after(|w| w.close()),
        _ => return json!({ "error": format!("unknown command: {name}") }),
    }
    cx.repaint();
    json!({ "ok": true })
}
