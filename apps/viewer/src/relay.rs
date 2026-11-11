//! The link to the relay: a WebSocket kept open while Plume Viewer runs, so that viewers can find this PC by its ID.
//! It reconnects by itself (network change, sleep). Only small JSON messages go through it: the screen does not.

use serde_json::{Value, json};
use std::{
    io::ErrorKind,
    net::TcpStream,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

#[derive(Debug, Clone)]
pub enum FromRelay {
    Online,
    /// Not linked (no internet, relay down…). The text says why, for the logs.
    Offline(String),
    /// Another PC owns this ID (the relay refused our token): we need a new ID.
    Taken,
    Join(String),
    Gone(String),
    Msg(String, Value),
}

/// Sends to the relay; cloneable, usable from any thread.
#[derive(Clone)]
pub struct Relay {
    tx: Sender<String>,
}

impl Relay {
    pub fn send(&self, to: &str, m: Value, close: bool) {
        let _ = self.tx.send(json!({ "to": to, "m": m, "close": close }).to_string());
    }
    /// Stops the link (the thread ends at its next turn).
    pub fn stop(&self) {
        let _ = self.tx.send(String::new());
    }
}

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

fn set_timeout(ws: &Ws, d: Duration) {
    match ws.get_ref() {
        MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(Some(d));
        }
        MaybeTlsStream::NativeTls(t) => {
            let _ = t.get_ref().set_read_timeout(Some(d));
        }
        _ => {}
    }
}

/// `base`: https://… or http://… (local tests). `on` is called on the relay thread.
pub fn start(base: &str, id: &str, token: &str, on: impl Fn(FromRelay) + Send + 'static) -> Relay {
    let url = format!("{}/ws/host?id={id}&token={token}", base.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1));
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::Builder::new().name("relay".into()).spawn(move || run(url, rx, on)).expect("relay thread");
    Relay { tx }
}

fn run(url: String, rx: Receiver<String>, on: impl Fn(FromRelay)) {
    let mut wait = Duration::from_secs(1);
    loop {
        // drop what was queued while offline (those viewers are gone), and stop if asked
        loop {
            match rx.try_recv() {
                Ok(s) if s.is_empty() => return,
                Ok(_) => {}
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        match tungstenite::connect(url.as_str()) {
            Ok((ws, _)) => {
                wait = Duration::from_secs(1);
                on(FromRelay::Online);
                match session(ws, &rx, &on) {
                    Ok(()) => return,
                    Err(e) => on(FromRelay::Offline(e)),
                }
            }
            Err(tungstenite::Error::Http(r)) if r.status() == 403 => {
                on(FromRelay::Taken);
                return;
            }
            Err(e) => on(FromRelay::Offline(e.to_string())),
        }
        // wait before trying again, but stop at once if asked
        match rx.recv_timeout(wait) {
            Ok(s) if s.is_empty() => return,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
            _ => {}
        }
        wait = (wait * 2).min(Duration::from_secs(30));
    }
}

/// One connection. Ok: asked to stop. Err: the link broke.
fn session(mut ws: Ws, rx: &Receiver<String>, on: &impl Fn(FromRelay)) -> Result<(), String> {
    set_timeout(&ws, Duration::from_millis(150));
    let mut last_ping = Instant::now();
    let mut last_heard = Instant::now();
    loop {
        // what the app wants to send
        loop {
            match rx.try_recv() {
                Ok(s) if s.is_empty() => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return Ok(());
                }
                Ok(s) => ws.send(Message::text(s)).map_err(|e| e.to_string())?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        if last_ping.elapsed() > Duration::from_secs(30) {
            ws.send(Message::text(r#"{"t":"ping"}"#)).map_err(|e| e.to_string())?;
            last_ping = Instant::now();
        }
        if last_heard.elapsed() > Duration::from_secs(75) {
            return Err("no answer from the relay".into());
        }
        match ws.read() {
            Ok(Message::Text(t)) => {
                last_heard = Instant::now();
                let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                match v["t"].as_str() {
                    Some("pong") => {}
                    Some("join") => on(FromRelay::Join(v["from"].as_str().unwrap_or("").into())),
                    Some("gone") => on(FromRelay::Gone(v["from"].as_str().unwrap_or("").into())),
                    _ => {
                        if let Some(from) = v["from"].as_str() {
                            on(FromRelay::Msg(from.into(), v["m"].clone()));
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => return Err("closed by the relay".into()),
            Ok(_) => last_heard = Instant::now(),
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}
