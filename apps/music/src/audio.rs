//! The audio engine. Each song decodes on its own thread (Symphonia, pure Rust: MP3, AAC/M4A, FLAC, ALAC, Ogg Vorbis,
//! WAV), is resampled once to the rate Windows mixes at (Rubato), and waits in a two-second buffer. The output thread
//! mixes the current song and the next one (crossfade, or gapless), applies the equalizer and the volume, and feeds
//! WASAPI. A song still downloading is read as it arrives (`Growth`).

use crate::wasapi::Output;
use rubato::{FftFixedIn, Resampler};
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::DecoderOptions,
    errors::Error as SymError,
    formats::{FormatOptions, SeekMode, SeekTo},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
    probe::Hint,
    units::Time,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

/// Seconds decoded ahead of what is heard.
const AHEAD: f64 = 2.0;
pub const EQ_FREQS: [f32; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

// ───────────────────────── a file still downloading ─────────────────────────

/// How far a download is: the reader waits for bytes that are not there yet, until `done` (or `failed`).
#[derive(Default)]
pub struct Growth {
    pub done: AtomicBool,
    pub failed: AtomicBool,
}

struct GrowingFile {
    f: File,
    g: Arc<Growth>,
    stop: Arc<AtomicBool>,
}

impl Read for GrowingFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let start = Instant::now();
        loop {
            let n = self.f.read(buf)?;
            if n > 0 || self.g.done.load(Ordering::Acquire) {
                // read once more after `done`: the last bytes may have landed in between
                return if n > 0 { Ok(n) } else { self.f.read(buf) };
            }
            if self.g.failed.load(Ordering::Acquire) || self.stop.load(Ordering::Acquire) {
                return Err(std::io::Error::other("download stopped"));
            }
            if start.elapsed() > Duration::from_secs(60) {
                return Err(std::io::Error::other("download stalled"));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Seek for GrowingFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        if let SeekFrom::End(_) = pos {
            // the end is only known once the download is over
            while !self.g.done.load(Ordering::Acquire) {
                if self.g.failed.load(Ordering::Acquire) || self.stop.load(Ordering::Acquire) {
                    return Err(std::io::Error::other("download stopped"));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
        self.f.seek(pos)
    }
}

impl MediaSource for GrowingFile {
    /// Not while it grows: the MP4 reader then reads it as a stream, up to the first media data, without asking
    /// for its total size (unknown yet).
    fn is_seekable(&self) -> bool {
        self.g.done.load(Ordering::Acquire)
    }
    fn byte_len(&self) -> Option<u64> {
        if self.g.done.load(Ordering::Acquire) { self.f.metadata().ok().map(|m| m.len()) } else { None }
    }
}

// ───────────────────────── one song ─────────────────────────

#[derive(Clone)]
pub struct Spec {
    /// Who asked for it (the player's queue entry), given back in the notes.
    pub id: u64,
    pub path: PathBuf,
    /// Some: the file is still being written.
    pub growth: Option<Arc<Growth>>,
    /// ReplayGain or loudness correction, in dB.
    pub gain_db: f32,
    pub start_at: f64,
}

struct Inner {
    buf: VecDeque<f32>,
    /// A seek waiting for the decoder (seconds), and the generation of the data in `buf`.
    seek: Option<f64>,
    epoch: u64,
    /// Everything decoded (the end of the song is in `buf`).
    finished: bool,
    error: Option<String>,
    duration: Option<f64>,
}

struct Shared {
    inner: Mutex<Inner>,
    cv: Condvar,
    stop: Arc<AtomicBool>,
}

struct Source {
    id: u64,
    spec: Spec,
    sh: Arc<Shared>,
    /// Stereo frames heard so far, at the output rate (moves on seek).
    pos: u64,
    gain: f32,
    rate: u32,
}

impl Drop for Source {
    fn drop(&mut self) {
        self.sh.stop.store(true, Ordering::Release);
        self.sh.cv.notify_all();
    }
}

impl Source {
    fn start(spec: Spec, rate: u32) -> Source {
        let sh = Arc::new(Shared {
            inner: Mutex::new(Inner {
                buf: VecDeque::new(),
                seek: (spec.start_at > 0.0).then_some(spec.start_at),
                epoch: 0,
                finished: false,
                error: None,
                duration: None,
            }),
            cv: Condvar::new(),
            stop: Arc::new(AtomicBool::new(false)),
        });
        let s2 = sh.clone();
        let sp = spec.clone();
        std::thread::Builder::new()
            .name("decode".into())
            .spawn(move || {
                if let Err(e) = decode(&sp, rate, &s2) {
                    let mut g = s2.inner.lock().unwrap();
                    g.error = Some(e);
                    g.finished = true;
                }
            })
            .expect("decode thread");
        Source { id: spec.id, spec: spec.clone(), sh, pos: (spec.start_at.max(0.0) * rate as f64) as u64, gain: 10f32.powf(spec.gain_db / 20.0), rate }
    }

    fn seek(&mut self, t: f64) {
        let mut g = self.sh.inner.lock().unwrap();
        g.buf.clear();
        g.seek = Some(t.max(0.0));
        g.epoch += 1;
        g.finished = false;
        self.pos = (t.max(0.0) * self.rate as f64) as u64;
        self.sh.cv.notify_all();
    }

    fn duration(&self) -> Option<f64> {
        self.sh.inner.lock().unwrap().duration
    }

    /// Frames left, if the length is known.
    fn remaining(&self) -> Option<u64> {
        self.duration().map(|d| ((d * self.rate as f64) as u64).saturating_sub(self.pos))
    }

    /// Takes up to `n` stereo frames into `out` (added, multiplied by `k`). Returns the frames taken.
    fn take(&mut self, out: &mut [f32], n: usize, k: f32) -> usize {
        let mut g = self.sh.inner.lock().unwrap();
        if g.seek.is_some() {
            return 0;
        }
        let have = (g.buf.len() / 2).min(n);
        let k = k * self.gain;
        for (i, s) in g.buf.drain(..have * 2).enumerate() {
            out[i] += s * k;
        }
        self.pos += have as u64;
        drop(g);
        self.sh.cv.notify_all();
        have
    }

    /// Everything is decoded and heard.
    fn over(&self) -> bool {
        let g = self.sh.inner.lock().unwrap();
        g.finished && g.buf.is_empty() && g.seek.is_none()
    }

    /// Enough is decoded to start without a gap.
    fn ready(&self) -> bool {
        let g = self.sh.inner.lock().unwrap();
        g.seek.is_none() && (g.buf.len() >= self.rate as usize / 5 || g.finished)
    }

    fn error(&self) -> Option<String> {
        self.sh.inner.lock().unwrap().error.clone()
    }
}

type Opened = (Box<dyn symphonia::core::formats::FormatReader>, Box<dyn symphonia::core::codecs::Decoder>, symphonia::core::formats::Track);

/// Opens the song's file (as a stream while it is still downloading).
fn open(spec: &Spec, sh: &Shared) -> Result<Opened, String> {
    let file = File::open(&spec.path).map_err(|e| e.to_string())?;
    let growing = spec.growth.as_ref().is_some_and(|g| !g.done.load(Ordering::Acquire));
    let src: Box<dyn MediaSource> = match (&spec.growth, growing) {
        (Some(g), true) => Box::new(GrowingFile { f: file, g: g.clone(), stop: sh.stop.clone() }),
        _ => Box::new(file),
    };
    let mss = MediaSourceStream::new(src, Default::default());
    let mut hint = Hint::new();
    if let Some(e) = spec.path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(e);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
        .map_err(|e| format!("format: {e}"))?;
    let format = probed.format;
    let track = format.default_track().ok_or("no audio track")?.clone();
    let dec = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| format!("codec: {e}"))?;
    Ok((format, dec, track))
}

/// The decoding thread of a song: until the end, or until the song is dropped.
fn decode(spec: &Spec, rate: u32, sh: &Shared) -> Result<(), String> {
    let (mut format, mut dec, track) = open(spec, sh)?;
    let mut streaming = spec.growth.as_ref().is_some_and(|g| !g.done.load(Ordering::Acquire));
    let in_rate = track.codec_params.sample_rate.unwrap_or(44_100);
    if let Some(n) = track.codec_params.n_frames {
        sh.inner.lock().unwrap().duration = Some(n as f64 / in_rate as f64);
    }
    const CHUNK: usize = 1024;
    let mut rs = if in_rate != rate { FftFixedIn::<f32>::new(in_rate as usize, rate as usize, CHUNK, 2, 2).ok() } else { None };
    let mut pending: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    let mut epoch = 0u64;
    let max = (AHEAD * rate as f64) as usize * 2;
    loop {
        // wait while the buffer is full (or the song is over), unless a seek comes
        {
            let mut g = sh.inner.lock().unwrap();
            loop {
                if sh.stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                if g.seek.is_some() || (!g.finished && g.buf.len() < max) {
                    break;
                }
                g = sh.cv.wait_timeout(g, Duration::from_millis(200)).unwrap().0;
            }
            if let Some(t) = g.seek.take() {
                epoch = g.epoch;
                drop(g);
                // a song still downloading cannot jump ahead: wait for the end of the download, then open it again
                if streaming {
                    if let Some(g) = &spec.growth {
                        while !g.done.load(Ordering::Acquire) {
                            if g.failed.load(Ordering::Acquire) {
                                return Err("download failed".into());
                            }
                            if sh.stop.load(Ordering::Acquire) {
                                return Ok(());
                            }
                            std::thread::sleep(Duration::from_millis(50));
                        }
                    }
                    let (f2, d2, _) = open(spec, sh)?;
                    format = f2;
                    dec = d2;
                    streaming = false;
                }
                let to = SeekTo::Time { time: Time::from(t), track_id: Some(track.id) };
                let _ = format.seek(SeekMode::Coarse, to);
                dec.reset();
                pending[0].clear();
                pending[1].clear();
                if let Some(r) = rs.as_mut() {
                    r.reset();
                }
            }
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // the end: what is left in the resampler, then the song is finished
                if let Some(r) = rs.as_mut()
                    && !pending[0].is_empty()
                {
                    let n = pending[0].len();
                    if let Ok(out) = r.process_partial(Some(&pending), None) {
                        let keep = ((n as f64) * rate as f64 / in_rate as f64) as usize;
                        push(sh, epoch, &out, keep);
                    }
                    pending[0].clear();
                    pending[1].clear();
                }
                let mut g = sh.inner.lock().unwrap();
                if g.epoch == epoch {
                    g.finished = true;
                }
                continue;
            }
            Err(SymError::ResetRequired) => {
                dec.reset();
                continue;
            }
            Err(e) => {
                if sh.stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                return Err(format!("read: {e}"));
            }
        };
        if packet.track_id() != track.id {
            continue;
        }
        let decoded = match dec.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue, // a damaged frame: skip it
            Err(e) => return Err(format!("decode: {e}")),
        };
        let spec_ = *decoded.spec();
        let frames = decoded.frames();
        if sbuf.as_ref().is_none_or(|b| b.capacity() < frames * spec_.channels.count()) {
            sbuf = Some(SampleBuffer::new(decoded.capacity() as u64, spec_));
        }
        let sb = sbuf.as_mut().unwrap();
        sb.copy_interleaved_ref(decoded);
        let ch = spec_.channels.count().max(1);
        let s = sb.samples();
        // to stereo (mono doubled, more channels: the first two)
        for f in 0..frames {
            let l = s[f * ch];
            let r = if ch > 1 { s[f * ch + 1] } else { l };
            pending[0].push(l);
            pending[1].push(r);
        }
        match rs.as_mut() {
            None => {
                let out = [std::mem::take(&mut pending[0]), std::mem::take(&mut pending[1])];
                push(sh, epoch, &out, out[0].len());
            }
            Some(r) => {
                while pending[0].len() >= CHUNK {
                    let chunk = [pending[0][..CHUNK].to_vec(), pending[1][..CHUNK].to_vec()];
                    pending[0].drain(..CHUNK);
                    pending[1].drain(..CHUNK);
                    if let Ok(out) = r.process(&chunk, None) {
                        let n = out[0].len();
                        push(sh, epoch, &out, n);
                    }
                }
            }
        }
    }
}

/// Adds planar stereo to the buffer, unless a seek made it stale.
fn push(sh: &Shared, epoch: u64, planar: &[Vec<f32>], n: usize) {
    let mut g = sh.inner.lock().unwrap();
    if g.epoch != epoch || g.seek.is_some() {
        return;
    }
    let n = n.min(planar[0].len()).min(planar[1].len());
    g.buf.reserve(n * 2);
    for (l, r) in planar[0][..n].iter().zip(&planar[1][..n]) {
        g.buf.push_back(*l);
        g.buf.push_back(*r);
    }
}

// ───────────────────────── equalizer ─────────────────────────

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z: [[f32; 2]; 2],
}

impl Biquad {
    /// A peaking band (RBJ cookbook), Q 1.1.
    fn peak(f: f32, gain_db: f32, rate: f32) -> Biquad {
        let a = 10f32.powf(gain_db / 40.0);
        let w = 2.0 * std::f32::consts::PI * (f / rate).min(0.49);
        let alpha = w.sin() / (2.0 * 1.1);
        let a0 = 1.0 + alpha / a;
        Biquad {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * w.cos() / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * w.cos() / a0,
            a2: (1.0 - alpha / a) / a0,
            z: [[0.0; 2]; 2],
        }
    }

    #[inline]
    fn run(&mut self, x: f32, c: usize) -> f32 {
        let y = self.b0 * x + self.z[c][0];
        self.z[c][0] = self.b1 * x - self.a1 * y + self.z[c][1];
        self.z[c][1] = self.b2 * x - self.a2 * y;
        y
    }
}

// ───────────────────────── the engine ─────────────────────────

pub enum Cmd {
    /// Play this song now (the current one stops).
    Load(Spec, bool),
    /// The song after the current one, prepared ahead (gapless, crossfade). None: nothing after.
    Next(Option<Spec>),
    Play,
    Pause,
    Seek(f64),
    Volume(f32),
    /// Gains of the 10 bands in dB and the preamp (None: equalizer off).
    Eq(Option<([f32; 10], f32)>),
    Crossfade(f32),
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub id: Option<u64>,
    pub pos: f64,
    pub dur: f64,
    pub playing: bool,
    pub buffering: bool,
    pub rate: u32,
    /// Peak of the last output block (for a small level meter), 0..1.
    pub level: f32,
    pub device_ok: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    /// This song is now heard (after a Load, or the next one taking over).
    Started(u64),
    /// This song played to its end.
    Ended(u64),
    Failed(u64, String),
}

pub struct Engine {
    tx: Sender<Cmd>,
    pub status: Arc<Mutex<Status>>,
}

impl Engine {
    pub fn start(notes: impl Fn(Note) + Send + 'static) -> Engine {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let st = status.clone();
        std::thread::Builder::new().name("audio".into()).spawn(move || run(rx, st, notes)).expect("audio thread");
        Engine { tx, status }
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
}

fn run(rx: Receiver<Cmd>, status: Arc<Mutex<Status>>, notes: impl Fn(Note)) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut out: Option<Output> = None;
    let mut retry = Instant::now();
    let mut cur: Option<Source> = None;
    let mut next: Option<Source> = None;
    let mut next_spec: Option<Spec> = None;
    let mut playing = false;
    let mut volume = 0.8f32;
    let mut gain = 0.0f32; // smoothed (no clicks on pause, play, seek)
    let mut eq: Option<([f32; 10], f32)> = None;
    let mut bands: Vec<Biquad> = Vec::new();
    let mut preamp = 1.0f32;
    let mut xfade = 0.0f32;
    let mut mix: Vec<f32> = Vec::new();
    let mut rate = 48_000u32;
    let mut last_status = Instant::now();
    let mut failed_reported: Option<u64> = None;
    loop {
        // commands
        loop {
            let c = match rx.try_recv() {
                Ok(c) => c,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            };
            match c {
                Cmd::Load(spec, play) => {
                    let id = spec.id;
                    // the song prepared as "next" is this one: keep its decoding
                    cur = match next.take() {
                        Some(n) if n.id == id && spec.start_at == 0.0 => Some(n),
                        _ => Some(Source::start(spec, rate)),
                    };
                    next_spec = None;
                    playing = play;
                    gain = 0.0;
                    failed_reported = None;
                    notes(Note::Started(id));
                }
                Cmd::Next(spec) => {
                    if next_spec.as_ref().map(|s| s.id) != spec.as_ref().map(|s| s.id) {
                        next = None;
                        next_spec = spec;
                    }
                }
                Cmd::Play => playing = cur.is_some(),
                Cmd::Pause => playing = false,
                Cmd::Seek(t) => {
                    if let Some(c) = cur.as_mut() {
                        c.seek(t);
                        gain = 0.0;
                    }
                }
                Cmd::Volume(v) => volume = v.clamp(0.0, 1.0),
                Cmd::Eq(e) => {
                    eq = e;
                    bands.clear();
                }
                Cmd::Crossfade(s) => xfade = s.clamp(0.0, 12.0),
            }
        }
        // the output (again after a device change)
        if out.is_none() {
            if retry > Instant::now() {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            match Output::open().and_then(|o| o.start().map(|_| o)) {
                Ok(o) => {
                    if o.rate != rate {
                        // songs already decoding were resampled for the old rate: start them again where they are
                        rate = o.rate;
                        cur = cur.take().map(|c| {
                            let mut s = c.spec.clone();
                            s.start_at = c.pos as f64 / c.rate as f64;
                            Source::start(s, rate)
                        });
                        next = None;
                    }
                    bands.clear();
                    out = Some(o);
                    status.lock().unwrap().device_ok = true;
                }
                Err(_) => {
                    status.lock().unwrap().device_ok = false;
                    retry = Instant::now() + Duration::from_secs(1);
                    continue;
                }
            }
        }
        // the next song starts decoding once the current one is well under way
        if next.is_none()
            && let Some(spec) = next_spec.clone()
            && cur.as_ref().is_none_or(|c| c.remaining().is_none_or(|r| r < (rate as u64) * (xfade as u64 + 25)))
        {
            next = Some(Source::start(spec, rate));
        }
        if bands.is_empty()
            && let Some((g, pre)) = eq
        {
            bands = EQ_FREQS.iter().zip(g.iter()).map(|(f, db)| Biquad::peak(*f, *db, rate as f32)).collect();
            preamp = 10f32.powf(pre / 20.0);
        }
        let o = out.as_ref().unwrap();
        let mut ended: Vec<Note> = Vec::new();
        let mut level = 0.0f32;
        let mut buffering = false;
        let fed = o.feed(100, &mut |buf: &mut [f32]| {
            let n = buf.len() / 2;
            mix.clear();
            mix.resize(buf.len(), 0.0);
            let target = if playing { volume * volume } else { 0.0 };
            if let Some(c) = cur.as_mut() {
                if playing || gain > 0.0005 {
                    // crossfade: the end of this song under the start of the next
                    let xf = (xfade * rate as f32) as u64;
                    let fading = xf > 0 && next.as_ref().is_some_and(|nx| nx.ready()) && c.remaining().is_some_and(|r| r <= xf);
                    if fading {
                        let r = c.remaining().unwrap_or(0) as f32;
                        let t = (1.0 - r / xf as f32).clamp(0.0, 1.0);
                        let k_out = (t * std::f32::consts::FRAC_PI_2).cos();
                        let k_in = (t * std::f32::consts::FRAC_PI_2).sin();
                        c.take(&mut mix, n, k_out);
                        if let Some(nx) = next.as_mut() {
                            nx.take(&mut mix, n, k_in);
                        }
                    } else {
                        let got = c.take(&mut mix, n, 1.0);
                        if got < n {
                            if c.over() {
                                // gapless: the next song goes on in the same block
                                if let Some(nx) = next.as_mut().filter(|nx| nx.ready()) {
                                    let rest = &mut mix[got * 2..];
                                    nx.take(rest, n - got, 1.0);
                                }
                            } else if playing {
                                buffering = true;
                            }
                        }
                    }
                }
                // smoothed gain, equalizer, soft limit
                let step = 1.0 / (0.012 * rate as f32);
                for i in 0..n {
                    gain += (target - gain).clamp(-step, step);
                    for ch in 0..2 {
                        let mut x = mix[i * 2 + ch];
                        if !bands.is_empty() {
                            x *= preamp;
                            for b in bands.iter_mut() {
                                x = b.run(x, ch);
                            }
                        }
                        x *= gain;
                        let a = x.abs();
                        if a > 0.9 {
                            x = x.signum() * (0.9 + 0.1 * ((a - 0.9) / 0.1).tanh());
                        }
                        level = level.max(x.abs());
                        buf[i * 2 + ch] = x;
                    }
                }
            } else {
                buf.fill(0.0);
            }
            // the current song is over: the next one takes its place (it already played if crossfading)
            let over = cur.as_ref().is_some_and(|c| c.over());
            if over {
                let done = cur.take().map(|c| (c.id, c.error()));
                match done {
                    Some((id, Some(e))) => ended.push(Note::Failed(id, e)),
                    Some((id, None)) => ended.push(Note::Ended(id)),
                    None => {}
                }
                if let Some(nx) = next.take() {
                    ended.push(Note::Started(nx.id));
                    cur = Some(nx);
                    next_spec = None;
                } else {
                    playing = false;
                }
            }
        });
        if fed.is_err() {
            out = None;
            retry = Instant::now() + Duration::from_millis(300);
            continue;
        }
        for n in ended {
            notes(n);
        }
        if let Some(c) = &cur
            && let Some(e) = c.error()
            && failed_reported != Some(c.id)
        {
            failed_reported = Some(c.id);
            notes(Note::Failed(c.id, e));
        }
        if last_status.elapsed() > Duration::from_millis(30) {
            last_status = Instant::now();
            let queued = out.as_ref().map(|o| o.queued()).unwrap_or(0) as u64;
            let mut s = status.lock().unwrap();
            s.id = cur.as_ref().map(|c| c.id);
            s.pos = cur.as_ref().map(|c| c.pos.saturating_sub(queued) as f64 / rate as f64).unwrap_or(0.0);
            s.dur = cur.as_ref().and_then(|c| c.duration()).unwrap_or(0.0);
            s.playing = playing && cur.is_some();
            s.buffering = buffering;
            s.rate = rate;
            s.level = s.level * 0.7 + level * 0.3;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biquad_flat_at_zero_db() {
        let mut b = Biquad::peak(1000.0, 0.0, 48_000.0);
        let mut last = 0.0;
        for i in 0..2000 {
            last = b.run(if i % 2 == 0 { 0.5 } else { -0.5 }, 0);
        }
        assert!((last.abs() - 0.5).abs() < 0.01, "{last}");
    }

    #[test]
    fn decodes_a_generated_wav() {
        // one second of a 440 Hz tone at 44.1 kHz, resampled to 48 kHz
        let dir = std::env::temp_dir().join("plume-music-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tone.wav");
        let (rate, n) = (44_100u32, 44_100usize);
        let mut w = Vec::new();
        let data_len = (n * 4) as u32;
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data_len).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 4).to_le_bytes());
        w.extend_from_slice(&4u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..n {
            let s = ((i as f32 * 440.0 * 2.0 * std::f32::consts::PI / rate as f32).sin() * 12000.0) as i16;
            w.extend_from_slice(&s.to_le_bytes());
            w.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(&path, w).unwrap();
        let mut src = Source::start(Spec { id: 1, path, growth: None, gain_db: 0.0, start_at: 0.0 }, 48_000);
        let mut out = vec![0f32; 96_000];
        let mut got = 0;
        let t = Instant::now();
        while got < 47_000 && t.elapsed() < Duration::from_secs(5) {
            got += src.take(&mut out[got * 2..], 48_000 - got, 1.0);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(got >= 47_000, "{got}");
        assert!((src.duration().unwrap() - 1.0).abs() < 0.01);
        let peak = out.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.3 && peak < 0.4, "{peak}");
    }
}

#[cfg(test)]
mod growing_tests {
    use super::*;

    /// A YouTube file read while it is still being written (as yt-dlp does).
    #[test]
    #[ignore] // needs target/musictest/yt140.m4a
    fn decodes_while_downloading() {
        let src = std::fs::read("../../target/musictest/yt140.m4a").or_else(|_| std::fs::read("target/musictest/yt140.m4a")).expect("sample");
        let dir = std::env::temp_dir().join("plume-music-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("growing.m4a");
        let _ = std::fs::remove_file(&path);
        let g = Arc::new(Growth::default());
        let (p2, g2) = (path.clone(), g.clone());
        let first = 70 * 1024;
        std::fs::write(&path, &src[..first]).unwrap();
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().append(true).open(&p2).unwrap();
            for chunk in src[first..].chunks(32 * 1024) {
                f.write_all(chunk).unwrap();
                f.flush().unwrap();
                std::thread::sleep(Duration::from_millis(2));
            }
            g2.done.store(true, Ordering::Release);
        });
        let mut s = Source::start(Spec { id: 1, path, growth: Some(g), gain_db: 0.0, start_at: 0.0 }, 48_000);
        let mut out = vec![0f32; 96_000 * 2];
        let mut got = 0;
        let t = Instant::now();
        while got < 96_000 && t.elapsed() < Duration::from_secs(10) {
            got += s.take(&mut out[got * 2..], 96_000 - got, 1.0);
            if let Some(e) = s.error() {
                panic!("error: {e} after {got} frames");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        writer.join().unwrap();
        assert!(got >= 96_000, "{got}");
        assert!(s.duration().unwrap() > 300.0);
    }
}
