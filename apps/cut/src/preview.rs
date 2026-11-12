//! The preview's pictures and sound.
//!
//! Paused or scrubbing, the window draws the frame itself from one picture per clip (FFmpeg decodes exactly the
//! frame asked, at the preview's size; the newest request wins, so scrubbing never queues up). Playing, FFmpeg
//! renders the edit from the playhead with the export's own plan, small, as raw frames, while a second run renders
//! its sound to the speakers (waveOut); the sound's clock paces the pictures.
//!
//! Why FFmpeg and not Media Foundation (which Plume Media plays with): the preview then decodes every file the
//! export can read (MKV, VP9, AV1, HEVC without Windows' paid extension…), with the same filters, so what plays is
//! what will be exported; it all runs outside the window's process, so a broken file cannot bring the editor down;
//! and it needs no COM callbacks, just pipes, which keeps it simple on this toolchain.

use crate::{
    export,
    ffmpeg::{self, Frame},
    model::{Id, Project},
    render::{self, Target},
    tools,
};
use std::{
    collections::{HashMap, VecDeque},
    io::Read,
    path::PathBuf,
    process::{Child, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Instant,
};

// ───────────────────────── single frames ─────────────────────────

/// A frame of a media file at the preview's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub media: Id,
    /// Milliseconds into the media.
    pub ms: i64,
    pub w: u32,
    pub h: u32,
}

pub struct Request {
    pub key: Key,
    pub path: PathBuf,
}

/// Decodes requested frames on two worker threads; the newest requests replace the waiting ones.
pub struct Grabber {
    queue: Arc<(Mutex<VecDeque<Request>>, Condvar)>,
    stop: Arc<AtomicBool>,
}

impl Grabber {
    /// `done` gets each decoded frame (on a worker thread).
    pub fn new(done: impl Fn(Key, Option<Frame>) + Send + Sync + 'static) -> Grabber {
        let queue: Arc<(Mutex<VecDeque<Request>>, Condvar)> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(done);
        for _ in 0..2 {
            let (q, s, d) = (queue.clone(), stop.clone(), done.clone());
            std::thread::spawn(move || {
                loop {
                    let req = {
                        let (lock, cv) = &*q;
                        let mut g = lock.lock().unwrap();
                        loop {
                            if s.load(Ordering::Relaxed) {
                                return;
                            }
                            if let Some(r) = g.pop_front() {
                                break r;
                            }
                            g = cv.wait(g).unwrap();
                        }
                    };
                    let f = ffmpeg::grab(&req.path, req.key.ms as f64 / 1000.0, req.key.w, req.key.h);
                    d(req.key, f);
                }
            });
        }
        Grabber { queue, stop }
    }

    /// The frames needed now (what was still waiting is dropped).
    pub fn want(&self, list: Vec<Request>) {
        let (lock, cv) = &*self.queue;
        let mut g = lock.lock().unwrap();
        g.clear();
        g.extend(list);
        cv.notify_all();
    }
}

impl Drop for Grabber {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.queue.1.notify_all();
    }
}

/// Frames kept for the preview (about 50 MB at most).
#[derive(Default)]
pub struct Cache {
    map: HashMap<Key, Arc<Frame>>,
    order: VecDeque<Key>,
    /// Asked, not back yet.
    pub pending: std::collections::HashSet<Key>,
    /// The last frame each media showed, drawn while the exact one is on its way.
    pub last: HashMap<Id, Arc<Frame>>,
}

impl Cache {
    pub fn get(&self, k: &Key) -> Option<Arc<Frame>> {
        self.map.get(k).cloned()
    }
    pub fn put(&mut self, k: Key, f: Frame) {
        self.pending.remove(&k);
        let f = Arc::new(f);
        self.last.insert(k.media, f.clone());
        if self.map.insert(k, f).is_none() {
            self.order.push_back(k);
        }
        let mut bytes: usize = self.map.values().map(|f| f.data.len()).sum();
        while bytes > 50_000_000 {
            let Some(old) = self.order.pop_front() else { break };
            if let Some(f) = self.map.remove(&old) {
                bytes -= f.data.len();
            }
        }
    }
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.pending.clear();
        self.last.clear();
    }
}

/// Brightness, contrast and saturation on a BGRA picture (the preview's version of FFmpeg's `eq`).
pub fn adjust(f: &Frame, brightness: f64, contrast: f64, saturation: f64) -> Frame {
    let (b, c, s) = (brightness as f32, contrast as f32, saturation as f32);
    let mut out = f.data.clone();
    for px in out.as_chunks_mut::<4>().0 {
        let ch = |v: u8| (v as f32 / 255.0 - 0.5) * c + 0.5 + b;
        let (bl, gr, rd) = (ch(px[0]), ch(px[1]), ch(px[2]));
        let y = 0.299 * rd + 0.587 * gr + 0.114 * bl;
        let put = |v: f32| ((y + (v - y) * s).clamp(0.0, 1.0) * 255.0).round() as u8;
        px[0] = put(bl);
        px[1] = put(gr);
        px[2] = put(rd);
    }
    Frame { w: f.w, h: f.h, data: out }
}

// ───────────────────────── playing ─────────────────────────

fn spawn(args: &[String], dir: &std::path::Path) -> Option<Child> {
    ffmpeg::command(&tools::ffmpeg()).args(args).current_dir(dir).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()
}

/// Plays the edit from a frame: pictures (and sound unless muted).
pub struct Player {
    pub from: i64,
    fps: f64,
    started: Instant,
    frames: Receiver<Frame>,
    video: Arc<Mutex<Option<Child>>>,
    sound: Option<Sound>,
    /// Frames shown so far (index of the next one in `frames`).
    next: i64,
    pub current: Option<Frame>,
    dirs: Vec<PathBuf>,
    /// The end of what plays (frames from `from`).
    pub len: i64,
}

impl Player {
    /// `w` × `h`: the preview's size. None when FFmpeg cannot start.
    pub fn start(p: &Project, from: i64, w: u32, h: u32, muted: bool) -> Option<Player> {
        let end = p.duration();
        if from >= end {
            return None;
        }
        let sub = p.sub_range(from, end);
        let plan = render::plan(&sub, w, h, &Target::Frames);
        let dir = export::prepare(&plan, "play").ok()?;
        let mut child = spawn(&plan.args, &dir)?;
        let mut out = child.stdout.take()?;
        let video = Arc::new(Mutex::new(Some(child)));
        let (tx, rx): (SyncSender<Frame>, Receiver<Frame>) = sync_channel(6);
        let size = (w * h * 4) as usize;
        std::thread::spawn(move || {
            loop {
                let mut buf = vec![0u8; size];
                if out.read_exact(&mut buf).is_err() || tx.send(Frame { w, h, data: buf }).is_err() {
                    return;
                }
            }
        });
        let mut dirs = vec![dir];
        let sound = if muted {
            None
        } else {
            let plan = render::plan(&sub, 16, 16, &Target::Sound);
            export::prepare(&plan, "sound").ok().and_then(|d| {
                dirs.push(d.clone());
                Sound::start(&plan.args, &d)
            })
        };
        Some(Player { from, fps: p.fps(), started: Instant::now(), frames: rx, video, sound, next: 0, current: None, dirs, len: end - from })
    }

    /// Frames played since the start (the sound's clock when there is sound).
    pub fn elapsed(&self) -> i64 {
        let secs = match &self.sound {
            Some(s) if s.started() => s.seconds(),
            Some(_) => 0.0,
            None => self.started.elapsed().as_secs_f64(),
        };
        (secs * self.fps) as i64
    }

    /// Takes the pictures due by now. Returns the timeline frame shown, and whether the end is reached.
    pub fn tick(&mut self) -> (i64, bool) {
        let due = self.elapsed();
        while self.next <= due {
            match self.frames.try_recv() {
                Ok(f) => {
                    self.current = Some(f);
                    self.next += 1;
                }
                Err(_) => break,
            }
        }
        let shown = (self.next - 1).max(0);
        let ended = due >= self.len;
        (self.from + shown.min(self.len - 1), ended)
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.video.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(s) = self.sound.take() {
            s.stop();
        }
        for d in self.dirs.drain(..) {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The edit's sound through waveOut, fed from FFmpeg on a thread of its own.
struct Sound {
    stop: Arc<AtomicBool>,
    /// Bytes the speakers have played.
    played: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Sound {
    fn start(args: &[String], dir: &std::path::Path) -> Option<Sound> {
        let mut child = spawn(args, dir)?;
        let out = child.stdout.take()?;
        let stop = Arc::new(AtomicBool::new(false));
        let played = Arc::new(AtomicU64::new(0));
        let running = Arc::new(AtomicBool::new(false));
        let (s, p, r) = (stop.clone(), played.clone(), running.clone());
        let thread = std::thread::spawn(move || {
            wave::play(out, &s, &p, &r);
            let _ = child.kill();
            let _ = child.wait();
        });
        Some(Sound { stop, played, running, thread: Some(thread) })
    }
    fn started(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
    fn seconds(&self) -> f64 {
        self.played.load(Ordering::Relaxed) as f64 / (48_000.0 * 4.0)
    }
    fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

mod wave {
    use std::{
        io::Read,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
    };
    use windows::Win32::Media::{
        Audio::{
            CALLBACK_NULL, HWAVEOUT, WAVE_FORMAT_PCM, WAVE_MAPPER, WAVEFORMATEX, WAVEHDR, WHDR_DONE, waveOutClose, waveOutGetPosition, waveOutOpen,
            waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader, waveOutWrite,
        },
        MMTIME, TIME_BYTES,
    };

    const BUFS: usize = 6;
    /// 50 ms of 48 kHz stereo 16-bit.
    const CHUNK: usize = 48_000 * 4 / 20;

    pub fn play(mut src: impl Read, stop: &AtomicBool, played: &AtomicU64, running: &AtomicBool) {
        let fmt = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: 2,
            nSamplesPerSec: 48_000,
            nAvgBytesPerSec: 48_000 * 4,
            nBlockAlign: 4,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let mut h = HWAVEOUT::default();
        if unsafe { waveOutOpen(Some(&mut h), WAVE_MAPPER, &fmt, None, None, CALLBACK_NULL) } != 0 {
            return;
        }
        let mut bufs: Vec<Vec<u8>> = (0..BUFS).map(|_| vec![0u8; CHUNK]).collect();
        let mut hdrs: Vec<WAVEHDR> = (0..BUFS).map(|_| WAVEHDR::default()).collect();
        let mut busy = [false; BUFS];
        let size = std::mem::size_of::<WAVEHDR>() as u32;
        let mut ended = false;
        while !stop.load(Ordering::Relaxed) {
            let mut t = MMTIME { wType: TIME_BYTES, ..Default::default() };
            if unsafe { waveOutGetPosition(h, &mut t, std::mem::size_of::<MMTIME>() as u32) } == 0 {
                let cb = unsafe { t.u.cb } as u64;
                if cb > 0 {
                    running.store(true, Ordering::Relaxed);
                }
                played.store(cb, Ordering::Relaxed);
            }
            let free = (0..BUFS).find(|&i| !busy[i] || hdrs[i].dwFlags & WHDR_DONE != 0);
            match free {
                Some(i) if !ended => {
                    if busy[i] {
                        unsafe { waveOutUnprepareHeader(h, &mut hdrs[i], size) };
                        busy[i] = false;
                    }
                    let mut n = 0;
                    while n < CHUNK {
                        match src.read(&mut bufs[i][n..]) {
                            Ok(0) | Err(_) => break,
                            Ok(k) => n += k,
                        }
                    }
                    n -= n % 4;
                    if n == 0 {
                        ended = true;
                        continue;
                    }
                    hdrs[i] = WAVEHDR { lpData: windows::core::PSTR(bufs[i].as_mut_ptr()), dwBufferLength: n as u32, ..Default::default() };
                    unsafe {
                        waveOutPrepareHeader(h, &mut hdrs[i], size);
                        waveOutWrite(h, &mut hdrs[i], size);
                    }
                    busy[i] = true;
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
            if ended && busy.iter().enumerate().all(|(i, b)| !b || hdrs[i].dwFlags & WHDR_DONE != 0) {
                // keep the clock running past the end of the sound (the pictures may go on)
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        unsafe {
            waveOutReset(h);
            for i in 0..BUFS {
                if busy[i] {
                    waveOutUnprepareHeader(h, &mut hdrs[i], size);
                }
            }
            waveOutClose(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_adjust() {
        let f = Frame { w: 1, h: 1, data: vec![255, 128, 0, 255] };
        assert_eq!(adjust(&f, 0.0, 1.0, 1.0).data, f.data);
        // no saturation: grey
        let g = adjust(&f, 0.0, 1.0, 0.0).data;
        assert!(g[0] == g[1] && g[1] == g[2]);
        // brighter
        assert!(adjust(&f, 0.2, 1.0, 1.0).data[1] > 128);
        assert_eq!(adjust(&f, 0.0, 1.0, 1.0).data[3], 255);
    }

    #[test]
    fn cache_forgets_the_oldest() {
        let mut c = Cache::default();
        for i in 0..80 {
            c.put(Key { media: 1, ms: i, w: 640, h: 360 }, Frame { w: 640, h: 360, data: vec![0; 640 * 360 * 4] });
        }
        assert!(c.get(&Key { media: 1, ms: 0, w: 640, h: 360 }).is_none());
        assert!(c.get(&Key { media: 1, ms: 79, w: 640, h: 360 }).is_some());
        assert!(c.last.contains_key(&1));
    }
}
