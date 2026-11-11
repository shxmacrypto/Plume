//! A session: the direct, encrypted link with one viewer (WebRTC data channels over UDP, DTLS). The screen goes out
//! on "screen", the viewer's mouse and keys come in on "input". At most two frames travel at once: the viewer
//! acknowledges each one, so a slow phone network gets fewer images instead of a growing delay.

use crate::{
    capture::{self, Frame, Geom},
    input::Injector,
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    io::ErrorKind,
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use str0m::{
    Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig,
    change::SdpOffer,
    channel::ChannelId,
    net::{Protocol, Receive},
};

const STUN: &str = "stun.cloudflare.com:3478";
const IN_FLIGHT: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// The viewer sees the screen.
    Linked,
    /// The session is over (the text says why, for the logs).
    Ended(String),
}

/// The address this PC uses to reach the internet (no packet is sent: the system only picks a route).
fn local_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("1.1.1.1:53").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// The public address of `sock` as the internet sees it (STUN binding request).
pub fn public_addr(sock: &UdpSocket) -> Option<SocketAddr> {
    let server = STUN.to_socket_addrs().ok()?.find(|a| a.is_ipv4())?;
    let tid = crate::crypto::random(12);
    let mut req = vec![0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42];
    req.extend_from_slice(&tid);
    let old = sock.read_timeout().ok().flatten();
    let _ = sock.set_read_timeout(Some(Duration::from_millis(250)));
    let mut buf = [0u8; 600];
    let mut found = None;
    'tries: for _ in 0..3 {
        if sock.send_to(&req, server).is_err() {
            break;
        }
        let until = Instant::now() + Duration::from_millis(400);
        while Instant::now() < until {
            let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
            if from != server || n < 20 || buf[0..2] != [0x01, 0x01] || buf[8..20] != tid[..] {
                continue;
            }
            // attributes: XOR-MAPPED-ADDRESS (0x0020), or MAPPED-ADDRESS (0x0001)
            let mut i = 20;
            while i + 4 <= n {
                let (t, l) = (u16::from_be_bytes([buf[i], buf[i + 1]]), u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize);
                let v = &buf[i + 4..(i + 4 + l).min(n)];
                if v.len() >= 8 && v[1] == 0x01 && (t == 0x0020 || t == 0x0001) {
                    let x = t == 0x0020;
                    let port = u16::from_be_bytes([v[2], v[3]]) ^ if x { 0x2112 } else { 0 };
                    let mut ip = [v[4], v[5], v[6], v[7]];
                    if x {
                        for (k, b) in ip.iter_mut().enumerate() {
                            *b ^= [0x21, 0x12, 0xA4, 0x42][k];
                        }
                    }
                    found = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), port));
                    break 'tries;
                }
                i += 4 + l.div_ceil(4) * 4;
            }
        }
    }
    let _ = sock.set_read_timeout(old);
    found
}

pub struct Session {
    pub stop: Arc<AtomicBool>,
    /// Set when the session's thread has said goodbye to the viewer and stopped.
    done: Arc<AtomicBool>,
}

impl Session {
    pub fn end(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Ends the session and waits (at most `max`) until the viewer has been told: Plume Viewer is quitting.
    pub fn end_and_wait(&self, max: Duration) {
        self.end();
        let until = Instant::now() + max;
        while !self.done.load(Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Opens a session from the viewer's offer. `answer` receives the answer to sign and send (or the error);
/// `events` hears how it goes. Everything runs on a thread of its own.
pub fn start(offer: String, answer: impl FnOnce(Result<String, String>) + Send + 'static, events: impl Fn(SessionEvent) + Send + 'static) -> Session {
    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let (st, dn) = (stop.clone(), done.clone());
    std::thread::Builder::new()
        .name("session".into())
        .spawn(move || {
            let why = match run(&offer, answer, &events, &st) {
                Ok(()) => "ended".to_string(),
                Err(e) => e,
            };
            dn.store(true, Ordering::SeqCst);
            events(SessionEvent::Ended(why));
        })
        .expect("session thread");
    Session { stop, done }
}

struct Link<'a> {
    rtc: Rtc,
    sock: UdpSocket,
    host: SocketAddr,
    stop: &'a AtomicBool,
    geom: Arc<Mutex<Geom>>,
    injector: Injector,
    screen: Option<ChannelId>,
    input: Option<ChannelId>,
    capture: Option<(mpsc::Sender<capture::Want>, mpsc::Receiver<Frame>)>,
    /// Frames sent, acknowledged by the viewer, and asked from the capture.
    sent: u32,
    acked: u32,
    asked: u32,
    disconnected_at: Option<Instant>,
    failed: Option<String>,
}

impl Link<'_> {
    /// Takes everything str0m has to say, until its next deadline (after every change, as str0m requires).
    fn drain(&mut self) -> Instant {
        loop {
            match self.rtc.poll_output() {
                Ok(Output::Timeout(t)) => return t,
                Ok(Output::Transmit(t)) => {
                    let _ = self.sock.send_to(&t.contents, t.destination);
                }
                Ok(Output::Event(e)) => self.event(e),
                Err(e) => {
                    self.failed = Some(e.to_string());
                    self.stop.store(true, Ordering::SeqCst);
                    return Instant::now();
                }
            }
        }
    }

    fn event(&mut self, e: Event) {
        match e {
            Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                self.disconnected_at.get_or_insert_with(Instant::now);
            }
            Event::IceConnectionStateChange(_) => self.disconnected_at = None,
            Event::ChannelOpen(id, label) => match label.as_str() {
                "screen" => self.screen = Some(id),
                "input" => self.input = Some(id),
                _ => {}
            },
            Event::ChannelData(d) if Some(d.id) == self.input => {
                let Ok(m) = serde_json::from_slice::<Value>(&d.data) else { return };
                match m["t"].as_str() {
                    Some("ack") => self.acked = (self.acked + 1).min(self.sent),
                    Some("bye") => self.stop.store(true, Ordering::SeqCst),
                    Some("full") => {
                        if let Some((want, _)) = &self.capture
                            && want.send(true).is_ok()
                        {
                            self.asked += 1;
                        }
                    }
                    _ => {
                        let g = *self.geom.lock().unwrap();
                        self.injector.handle(&g, &m);
                    }
                }
            }
            Event::ChannelClose(id) if Some(id) == self.screen || Some(id) == self.input => self.stop.store(true, Ordering::SeqCst),
            _ => {}
        }
    }

    /// Sends what the capture made, as long as the channel takes it, then gives the capture its credits.
    fn pump(&mut self, queue: &mut VecDeque<Vec<u8>>) {
        let Some(id) = self.screen else { return };
        loop {
            if queue.is_empty() {
                let Some((_, frames)) = &self.capture else { return };
                match frames.try_recv() {
                    Ok(f) => {
                        if f.counted {
                            self.sent += 1;
                            self.asked = self.asked.saturating_sub(1);
                        }
                        queue.extend(f.msgs);
                        continue;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.failed = Some("capture stopped".into());
                        self.stop.store(true, Ordering::SeqCst);
                        return;
                    }
                }
            }
            let Some(m) = queue.front() else { break };
            let written = match self.rtc.channel(id) {
                Some(mut ch) => ch.write(true, m),
                None => return,
            };
            let full = match written {
                Ok(true) => {
                    queue.pop_front();
                    false
                }
                Ok(false) => true,
                Err(e) => {
                    eprintln!("write: {e}");
                    queue.pop_front();
                    false
                }
            };
            self.drain();
            if full || self.stop.load(Ordering::SeqCst) {
                break; // the channel is full: later
            }
        }
        // credits: frames in flight (sent, not acknowledged) and frames asked stay under the limit
        if let Some((want, _)) = &self.capture {
            while self.sent.saturating_sub(self.acked) + self.asked < IN_FLIGHT {
                if want.send(false).is_err() {
                    break;
                }
                self.asked += 1;
            }
        }
    }
}

fn run(offer: &str, answer: impl FnOnce(Result<String, String>), events: &impl Fn(SessionEvent), stop: &AtomicBool) -> Result<(), String> {
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    let port = sock.local_addr().map_err(|e| e.to_string())?.port();
    let ip = local_ip().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let host = SocketAddr::new(ip, port);
    let mut rtc: Rtc = RtcConfig::new().set_crypto_provider(Arc::new(str0m::crypto::from_feature_flags())).build(Instant::now());
    let offer = match SdpOffer::from_sdp_string(offer) {
        Ok(o) => o,
        Err(e) => {
            answer(Err(e.to_string()));
            return Err("bad offer".into());
        }
    };
    if let Ok(c) = Candidate::host(host, "udp") {
        rtc.add_local_candidate(c);
    }
    if std::env::var_os("PLUME_VIEWER_NO_STUN").is_none()
        && let Some(public) = public_addr(&sock)
        && public != host
        && let Ok(c) = Candidate::server_reflexive(public, host, "udp")
    {
        rtc.add_local_candidate(c);
    }
    let sdp = match rtc.sdp_api().accept_offer(offer) {
        Ok(a) => a.to_sdp_string(),
        Err(e) => {
            answer(Err(e.to_string()));
            return Err("offer refused".into());
        }
    };
    answer(Ok(sdp));

    let mut l = Link {
        rtc,
        sock,
        host,
        stop,
        geom: Arc::new(Mutex::new(Geom::default())),
        injector: Injector::new(),
        screen: None,
        input: None,
        capture: None,
        sent: 0,
        acked: 0,
        asked: 0,
        disconnected_at: None,
        failed: None,
    };
    let mut queue: VecDeque<Vec<u8>> = VecDeque::new();
    let started = Instant::now();
    let mut linked = false;
    let mut buf = vec![0u8; 2000];

    let result = loop {
        let timeout = l.drain();
        if stop.load(Ordering::SeqCst) {
            break l.failed.take().map_or(Ok(()), Err);
        }
        if !l.rtc.is_alive() || l.disconnected_at.is_some_and(|t| t.elapsed() > Duration::from_secs(8)) {
            break Err("link lost".to_string());
        }
        if !linked && started.elapsed() > Duration::from_secs(25) {
            break Err("no link".to_string());
        }
        // both channels open: the screen starts
        if !linked && l.screen.is_some() && l.input.is_some() {
            linked = true;
            let (want_tx, want_rx) = mpsc::channel();
            let (frame_tx, frame_rx) = mpsc::sync_channel(2);
            let g = l.geom.clone();
            std::thread::Builder::new().name("capture".into()).spawn(move || capture::run(want_rx, frame_tx, g)).map_err(|e| e.to_string())?;
            let _ = want_tx.send(true);
            l.asked = 1;
            l.capture = Some((want_tx, frame_rx));
            events(SessionEvent::Linked);
        }
        l.pump(&mut queue);
        // wait for the network (briefly while frames are coming)
        let cap = Duration::from_millis(if linked { 4 } else { 50 });
        let wait = timeout.saturating_duration_since(Instant::now()).clamp(Duration::from_millis(1), cap);
        let _ = l.sock.set_read_timeout(Some(wait));
        buf.resize(2000, 0);
        let input = match l.sock.recv_from(&mut buf) {
            Ok((n, source)) => match buf[..n].try_into() {
                Ok(contents) => Input::Receive(Instant::now(), Receive { proto: Protocol::Udp, source, destination: l.host, contents }),
                Err(_) => continue,
            },
            // Windows reports an unreachable port of an earlier packet this way: nothing to do
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset) => Input::Timeout(Instant::now()),
            Err(e) => break Err(e.to_string()),
        };
        if let Err(e) = l.rtc.handle_input(input) {
            eprintln!("rtc input: {e}");
        }
    };
    l.injector.release_all();
    l.capture = None;
    // goodbye on the screen channel, so the viewer knows at once (closing the link alone says nothing to it)
    if let Some(id) = l.screen
        && let Some(mut ch) = l.rtc.channel(id)
        && ch.write(true, &[4]).unwrap_or(false)
    {
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            let t = l.drain();
            let _ = l.sock.set_read_timeout(Some(t.saturating_duration_since(Instant::now()).clamp(Duration::from_millis(1), Duration::from_millis(20))));
            buf.resize(2000, 0);
            let input = match l.sock.recv_from(&mut buf) {
                Ok((n, source)) => match buf[..n].try_into() {
                    Ok(contents) => Input::Receive(Instant::now(), Receive { proto: Protocol::Udp, source, destination: l.host, contents }),
                    Err(_) => continue,
                },
                Err(_) => Input::Timeout(Instant::now()),
            };
            let _ = l.rtc.handle_input(input);
        }
    }
    l.rtc.disconnect();
    let _ = l.drain();
    result
}

/// The offers of every browser are accepted: Safari's and Chrome's hide the phone's address behind a `.local` name,
/// add a reflexive address with `raddr 0.0.0.0`, IPv6 or TCP candidates.
#[cfg(test)]
mod offers {
    use std::{sync::Arc, time::Instant};
    use str0m::{RtcConfig, change::SdpOffer};

    fn try_offer(cands: &str) -> Result<(), String> {
        let sdp = format!(
            "v=0\r\no=- 4155234960385128463 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=group:BUNDLE 0\r\na=extmap-allow-mixed\r\na=msid-semantic: WMS\r\n\
m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\nc=IN IP4 0.0.0.0\r\n{cands}a=ice-ufrag:Zx1y\r\na=ice-pwd:7Hq3sX9bL2mN5pR8tV1wY4zA\r\na=ice-options:trickle\r\n\
a=fingerprint:sha-256 6B:8B:5D:EA:59:04:20:23:29:C8:87:1C:CD:87:32:BE:DD:8C:66:A5:8E:50:55:EA:8C:D3:B6:5C:09:5E:D6:BC\r\na=setup:actpass\r\na=mid:0\r\na=sctp-port:5000\r\na=max-message-size:262144\r\n"
        );
        let offer = SdpOffer::from_sdp_string(&sdp).map_err(|e| format!("parse: {e}"))?;
        let mut rtc = RtcConfig::new().set_crypto_provider(Arc::new(str0m::crypto::from_feature_flags())).build(Instant::now());
        rtc.sdp_api().accept_offer(offer).map(|_| ()).map_err(|e| format!("accept: {e}"))
    }

    #[test]
    fn every_browser_offer_is_accepted() {
        let cases = [
            ("no candidates", ""),
            ("ip host", "a=candidate:1 1 udp 2113937151 192.168.1.20 51234 typ host generation 0 network-cost 999\r\n"),
            ("mdns host", "a=candidate:1 1 udp 2113937151 7b0d1c2e-3f40-4a5b-8c6d-7e8f9a0b1c2d.local 51234 typ host generation 0 network-cost 999\r\n"),
            ("srflx raddr 0", "a=candidate:2 1 udp 1677729535 203.0.113.5 51234 typ srflx raddr 0.0.0.0 rport 0 generation 0 network-cost 999\r\n"),
            ("ipv6 host", "a=candidate:3 1 udp 2122262783 2001:db8::1 51235 typ host generation 0 network-cost 999\r\n"),
            ("tcp host", "a=candidate:4 1 tcp 1518283007 192.168.1.20 9 typ host tcptype active generation 0 network-cost 999\r\n"),
        ];
        for (name, c) in cases {
            assert_eq!(try_offer(c), Ok(()), "{name}");
        }
    }
}
