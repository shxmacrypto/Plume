//! Who may connect. A viewer says hello, the PC sends a challenge, the viewer answers with its WebRTC offer signed
//! with the key of the one-time code or of the permanent password. A good signature with the code still needs the
//! person at the PC to accept; with the password it goes through. Five bad signatures lock the PC for 5 minutes.

use crate::crypto;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub const MAX_FAILURES: u32 = 5;
pub const LOCK: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Code,
    Password,
}

/// A viewer that has said hello.
struct Pending {
    name: String,
    nv: String,
    nh: String,
    /// Offer checked, waiting for the person at the PC.
    asked: Option<Ready>,
    since: Instant,
}

/// Everything a session needs to answer the viewer.
#[derive(Clone, Debug)]
pub struct Ready {
    pub from: String,
    pub name: String,
    pub sdp: String,
    pub key: Vec<u8>,
    pub nv: String,
    pub nh: String,
    pub mode: Mode,
}

#[derive(Debug)]
pub enum Action {
    /// To the viewer, through the relay.
    Send { to: String, m: Value, close: bool },
    /// Ask the person at the PC.
    Ask { from: String, name: String },
    /// Open the session (signature good, and accepted or password).
    Start(Ready),
}

/// What the PC knows to check a viewer.
pub struct Secrets<'a> {
    pub id: &'a str,
    pub code: &'a str,
    /// (salt in hex, key) of the permanent password.
    pub password: Option<(&'a str, &'a [u8])>,
}

#[derive(Default)]
pub struct Gate {
    pending: HashMap<String, Pending>,
    failures: u32,
    locked_until: Option<Instant>,
    /// A session is open (one at a time).
    pub busy: bool,
}

fn refuse(to: &str, why: &str) -> Action {
    Action::Send { to: to.into(), m: json!({ "t": "refused", "why": why }), close: true }
}

/// The name a viewer gives itself, made safe to show.
fn clean_name(s: &str) -> String {
    let n: String = s.chars().filter(|c| !c.is_control()).take(40).collect();
    let n = n.trim().to_string();
    if n.is_empty() { "?".into() } else { n }
}

impl Gate {
    pub fn locked(&mut self) -> bool {
        match self.locked_until {
            Some(t) if Instant::now() < t => true,
            Some(_) => {
                self.locked_until = None;
                self.failures = 0;
                false
            }
            None => false,
        }
    }

    #[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
    pub fn failures(&self) -> u32 {
        self.failures
    }

    pub fn on_message(&mut self, from: &str, m: &Value, sec: &Secrets) -> Vec<Action> {
        // forget viewers that never finished (a minute is plenty; the person at the PC has 60 s to answer)
        self.pending.retain(|_, p| p.since.elapsed() < Duration::from_secs(120));
        match m["t"].as_str() {
            Some("hello") => {
                if self.busy {
                    return vec![refuse(from, "busy")];
                }
                if self.locked() {
                    return vec![refuse(from, "locked")];
                }
                if self.pending.len() >= 8 {
                    return vec![refuse(from, "busy")];
                }
                let nv = m["nv"].as_str().unwrap_or("").to_string();
                if nv.len() < 16 || nv.len() > 64 {
                    return vec![refuse(from, "wrong")];
                }
                let nh = crypto::hex(&crypto::random(16));
                let mut modes = vec!["code"];
                // a salt is always sent, so a viewer cannot tell from it whether a password exists
                let salt = match sec.password {
                    Some((salt, _)) => {
                        modes.push("password");
                        salt.to_string()
                    }
                    None => crypto::hex(&crypto::sign(sec.id.as_bytes(), "salt").as_bytes()[..16]),
                };
                self.pending.insert(
                    from.into(),
                    Pending { name: clean_name(m["name"].as_str().unwrap_or("")), nv, nh: nh.clone(), asked: None, since: Instant::now() },
                );
                vec![Action::Send { to: from.into(), m: json!({ "t": "challenge", "nh": nh, "salt": salt, "modes": modes }), close: false }]
            }
            Some("offer") => {
                let Some((nh, nv, name, asked)) = self.pending.get(from).map(|p| (p.nh.clone(), p.nv.clone(), p.name.clone(), p.asked.is_some())) else {
                    return vec![refuse(from, "wrong")];
                };
                if asked {
                    return vec![];
                }
                if self.busy {
                    self.pending.remove(from);
                    return vec![refuse(from, "busy")];
                }
                if self.locked() {
                    self.pending.remove(from);
                    return vec![refuse(from, "locked")];
                }
                let sdp = m["sdp"].as_str().unwrap_or("");
                let mac = m["mac"].as_str().unwrap_or("");
                let (mode, key) = match m["mode"].as_str() {
                    Some("password") => match sec.password {
                        Some((_, k)) => (Mode::Password, k.to_vec()),
                        None => {
                            self.pending.remove(from);
                            return vec![refuse(from, "wrong")];
                        }
                    },
                    _ => (Mode::Code, crypto::code_key(sec.id, sec.code).to_vec()),
                };
                let text = format!("offer\n{nh}\n{nv}\n{sdp}");
                if sdp.is_empty() || sdp.len() > 20_000 || !crypto::verify(&key, &text, mac) {
                    self.pending.remove(from);
                    self.failures += 1;
                    if self.failures >= MAX_FAILURES {
                        self.locked_until = Some(Instant::now() + LOCK);
                        return vec![refuse(from, "locked")];
                    }
                    return vec![refuse(from, "wrong")];
                }
                self.failures = 0;
                let ready = Ready { from: from.into(), name: name.clone(), sdp: sdp.into(), key, nv, nh, mode };
                if mode == Mode::Password {
                    self.pending.remove(from);
                    self.busy = true;
                    return vec![Action::Start(ready)];
                }
                if let Some(p) = self.pending.get_mut(from) {
                    p.asked = Some(ready);
                    p.since = Instant::now();
                }
                vec![Action::Send { to: from.into(), m: json!({ "t": "wait" }), close: false }, Action::Ask { from: from.into(), name }]
            }
            Some("bye") => {
                self.pending.remove(from);
                vec![]
            }
            _ => vec![],
        }
    }

    /// The person at the PC said yes.
    pub fn accept(&mut self, from: &str) -> Vec<Action> {
        match self.pending.remove(from).and_then(|p| p.asked) {
            Some(r) if !self.busy => {
                self.busy = true;
                vec![Action::Start(r)]
            }
            Some(_) => vec![refuse(from, "busy")],
            None => vec![],
        }
    }

    /// The person at the PC said no (or did not answer).
    pub fn deny(&mut self, from: &str) -> Vec<Action> {
        match self.pending.remove(from) {
            Some(_) => vec![refuse(from, "denied")],
            None => vec![],
        }
    }

    /// The viewer left the relay before the end.
    pub fn gone(&mut self, from: &str) -> bool {
        self.pending.remove(from).is_some_and(|p| p.asked.is_some())
    }
}

/// The answer to send, signed with the session key.
pub fn answer(r: &Ready, sdp: &str) -> Value {
    json!({ "t": "answer", "sdp": sdp, "mac": crypto::sign(&r.key, &format!("answer\n{}\n{}\n{}", r.nv, r.nh, sdp)) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(g: &mut Gate, from: &str, sec: &Secrets) -> String {
        let a = g.on_message(from, &json!({ "t": "hello", "name": "Phone", "nv": "00112233445566778899aabbccddeeff" }), sec);
        match &a[0] {
            Action::Send { m, .. } if m["t"] == "challenge" => m["nh"].as_str().unwrap().to_string(),
            other => panic!("{other:?}"),
        }
    }

    fn offer(key: &[u8], nh: &str, mode: &str) -> Value {
        let sdp = "v=0\r\n";
        json!({ "t": "offer", "mode": mode, "sdp": sdp, "mac": crypto::sign(key, &format!("offer\n{nh}\n00112233445566778899aabbccddeeff\n{sdp}")) })
    }

    #[test]
    fn code_needs_a_yes() {
        let sec = Secrets { id: "123456789", code: "abcdefgh", password: None };
        let mut g = Gate::default();
        let nh = hello(&mut g, "c-1", &sec);
        let a = g.on_message("c-1", &offer(&crypto::code_key("123456789", "abcdefgh"), &nh, "code"), &sec);
        assert!(matches!(&a[1], Action::Ask { name, .. } if name == "Phone"));
        assert!(!g.busy);
        let a = g.accept("c-1");
        assert!(matches!(&a[0], Action::Start(r) if r.mode == Mode::Code));
        assert!(g.busy);
        // a second viewer is told the PC is busy
        let a = g.on_message("c-2", &json!({ "t": "hello", "nv": "00112233445566778899aabbccddeeff" }), &sec);
        assert!(matches!(&a[0], Action::Send { m, close: true, .. } if m["why"] == "busy"));
    }

    #[test]
    fn password_goes_through() {
        let key = crypto::password_key("hunter22", b"salt-salt-salt-1");
        let sec = Secrets { id: "123456789", code: "abcdefgh", password: Some(("73616c74", &key)) };
        let mut g = Gate::default();
        let nh = hello(&mut g, "c-1", &sec);
        let a = g.on_message("c-1", &offer(&key, &nh, "password"), &sec);
        assert!(matches!(&a[0], Action::Start(r) if r.mode == Mode::Password));
    }

    #[test]
    fn wrong_code_then_lock() {
        let sec = Secrets { id: "123456789", code: "abcdefgh", password: None };
        let mut g = Gate::default();
        for i in 0..MAX_FAILURES {
            let from = format!("c-{i}");
            let nh = hello(&mut g, &from, &sec);
            let a = g.on_message(&from, &offer(&crypto::code_key("123456789", "zzzzzzzz"), &nh, "code"), &sec);
            let why = if i + 1 == MAX_FAILURES { "locked" } else { "wrong" };
            assert!(matches!(&a[0], Action::Send { m, close: true, .. } if m["why"] == why), "{i}");
        }
        let a = g.on_message("c-x", &json!({ "t": "hello", "nv": "00112233445566778899aabbccddeeff" }), &sec);
        assert!(matches!(&a[0], Action::Send { m, .. } if m["why"] == "locked"));
    }

    #[test]
    fn password_mode_without_password_is_wrong() {
        let sec = Secrets { id: "123456789", code: "abcdefgh", password: None };
        let mut g = Gate::default();
        let nh = hello(&mut g, "c-1", &sec);
        let a = g.on_message("c-1", &offer(b"whatever", &nh, "password"), &sec);
        assert!(matches!(&a[0], Action::Send { m, .. } if m["why"] == "wrong"));
    }

    #[test]
    fn deny_and_answer_signature() {
        let sec = Secrets { id: "123456789", code: "abcdefgh", password: None };
        let mut g = Gate::default();
        let nh = hello(&mut g, "c-1", &sec);
        g.on_message("c-1", &offer(&crypto::code_key("123456789", "abcdefgh"), &nh, "code"), &sec);
        assert!(matches!(&g.deny("c-1")[0], Action::Send { m, .. } if m["why"] == "denied"));
        let r = Ready { from: "c".into(), name: "n".into(), sdp: String::new(), key: vec![1; 32], nv: "a".into(), nh: "b".into(), mode: Mode::Code };
        let v = answer(&r, "SDP");
        assert!(crypto::verify(&[1; 32], "answer\na\nb\nSDP", v["mac"].as_str().unwrap()));
    }
}
