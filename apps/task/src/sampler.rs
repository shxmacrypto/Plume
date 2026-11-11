//! The sampling thread: every interval it reads the processes and the PC, turns totals into rates, groups the
//! processes into programs, loads the icons of new programs, and sends the result to the window.
//! While the window is minimized it reads only the few numbers of the whole PC (the graphs keep going).

use crate::{
    model::{self, Group, Proc, Sys},
    pdh, sys,
};
use plume_ui::Sender;
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub struct Sample {
    pub sys: Sys,
    /// None while the window is minimized (the list is not read).
    pub groups: Option<Vec<Group>>,
    /// New icons: (key, width, height, premultiplied BGRA).
    pub icons: Vec<(u64, u32, u32, Vec<u8>)>,
    /// How long reading took, in microseconds (for the test socket).
    pub took_us: u64,
}

/// What the window tells the thread.
pub struct Control {
    /// Milliseconds between samples; 0: paused.
    pub interval: AtomicU32,
    pub visible: AtomicBool,
    wake: mpsc::Sender<()>,
}

impl Control {
    /// Reads again now (after ending a task, a new interval, the window coming back).
    pub fn wake(&self) {
        let _ = self.wake.send(());
    }
}

pub fn icon_key(path: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.to_lowercase().hash(&mut h);
    h.finish() | 1 << 63
}

/// Work for the window's thread, made from a sample.
pub type Deliver = Box<dyn FnOnce(&mut dyn plume_ui::Handler, &mut plume_ui::Cx) + Send>;
/// An icon: width, height, premultiplied BGRA pixels.
pub type Pixels = (u32, u32, Vec<u8>);

pub fn start(sender: Sender, interval_ms: u32, deliver: fn(Sample) -> Deliver) -> Arc<Control> {
    let (tx, rx) = mpsc::channel();
    let ctl = Arc::new(Control { interval: AtomicU32::new(interval_ms), visible: AtomicBool::new(true), wake: tx });
    let c = ctl.clone();
    std::thread::Builder::new()
        .name("sampler".into())
        .spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
            }
            let mut s = Sampler::new();
            loop {
                let t = Instant::now();
                let visible = c.visible.load(Ordering::Relaxed);
                let mut sample = s.sample(visible);
                sample.took_us = t.elapsed().as_micros() as u64;
                let f = deliver(sample);
                if !sender.send(move |h, cx| f(h, cx)) {
                    return;
                }
                let ms = c.interval.load(Ordering::Relaxed);
                let wait = if ms == 0 { Duration::from_secs(3600) } else { Duration::from_millis(ms as u64).saturating_sub(t.elapsed()) };
                match rx.recv_timeout(wait) {
                    Ok(()) => {
                        // several wake-ups at once count as one
                        while rx.try_recv().is_ok() {}
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .expect("sampling thread");
    ctl
}

struct Prev {
    busy: i64,
    read: u64,
    write: u64,
}

struct Sampler {
    buf: Vec<u8>,
    ncpu: u32,
    cpu_name: String,
    gpu_name: String,
    gpu_mem: u64,
    last: Option<Instant>,
    last_wall: i64,
    sys_times: (u64, u64, u64),
    own_busy: i64,
    net: (u64, u64),
    prev: HashMap<(u32, i64), Prev>,
    paths: HashMap<(u32, i64), Option<String>>,
    names: HashMap<String, Option<(String, String)>>,
    icons: HashSet<String>,
    impact: HashMap<String, f32>,
    gpu_q: Option<pdh::Query>,
    disk_q: Option<pdh::Query>,
    pdh_buf: Vec<u8>,
    items: Vec<(String, f64)>,
}

impl Sampler {
    fn new() -> Sampler {
        let (gpu_name, gpu_mem) = sys::gpu_info();
        let gpu_q = pdh::Query::new(&[r"\GPU Engine(*)\Utilization Percentage"]).filter(|q| q.has(0));
        let disk_q = pdh::Query::new(&[
            r"\PhysicalDisk(_Total)\Disk Read Bytes/sec",
            r"\PhysicalDisk(_Total)\Disk Write Bytes/sec",
            r"\PhysicalDisk(_Total)\% Idle Time",
        ]);
        Sampler {
            buf: Vec::new(),
            ncpu: sys::ncpu(),
            cpu_name: sys::cpu_name(),
            gpu_name,
            gpu_mem,
            last: None,
            last_wall: 0,
            sys_times: sys::system_times(),
            own_busy: 0,
            net: sys::net_bytes(),
            prev: HashMap::new(),
            paths: HashMap::new(),
            names: HashMap::new(),
            icons: HashSet::new(),
            impact: HashMap::new(),
            gpu_q,
            disk_q,
            pdh_buf: Vec::new(),
            items: Vec::new(),
        }
    }

    fn sample(&mut self, visible: bool) -> Sample {
        let now = Instant::now();
        let wall = sys::now_100ns();
        let dt = self.last.map(|l| now.duration_since(l).as_secs_f64()).unwrap_or(0.0);
        let dwall = if self.last_wall > 0 { wall - self.last_wall } else { 0 };
        self.last = Some(now);
        self.last_wall = wall;

        let mut out = Sys { ncpu: self.ncpu, cpu_name: self.cpu_name.clone(), gpu_name: self.gpu_name.clone(), gpu_mem: self.gpu_mem, ..Default::default() };
        // the whole PC
        let t = sys::system_times();
        let o = self.sys_times;
        out.cpu = model::system_cpu((o.0, t.0), (o.1, t.1), (o.2, t.2));
        self.sys_times = t;
        let m = sys::memory();
        (out.mem_used, out.mem_total, out.commit, out.commit_limit, out.cached) = (m.used, m.total, m.commit, m.commit_limit, m.cached);
        out.uptime_s = sys::uptime_s();
        let net = sys::net_bytes();
        if dt > 0.0 {
            out.net_in = net.0.saturating_sub(self.net.0) as f64 / dt;
            out.net_out = net.1.saturating_sub(self.net.1) as f64 / dt;
        }
        self.net = net;
        if let Some(q) = &self.disk_q {
            q.collect();
            out.disk_read = q.value(0).unwrap_or(0.0).max(0.0);
            out.disk_write = q.value(1).unwrap_or(0.0).max(0.0);
            out.disk_active = (100.0 - q.value(2).unwrap_or(100.0)).clamp(0.0, 100.0) as f32;
        }
        let own = own_busy();
        if dwall > 0 {
            out.own_cpu = model::cpu_percent(self.own_busy, own, dwall, self.ncpu);
        }
        self.own_busy = own;

        // graphics: every engine of every process
        let mut gpu_pid = HashMap::new();
        if let Some(q) = &self.gpu_q {
            out.has_gpu = true;
            if visible {
                q.collect();
                q.array(0, &mut self.pdh_buf, &mut self.items);
                let (per, total) = pdh::gpu_split(&self.items);
                gpu_pid = per;
                out.gpu = total;
            }
        }

        if !visible {
            // nothing per process: the next visible sample starts the rates again
            self.prev.clear();
            return Sample { sys: out, groups: None, icons: Vec::new(), took_us: 0 };
        }

        let raw = sys::processes(&mut self.buf);
        let windows = sys::window_pids();
        let mut procs = Vec::with_capacity(raw.len());
        let mut seen = HashSet::with_capacity(raw.len());
        for r in raw {
            let id = (r.pid, r.create);
            seen.insert(id);
            out.threads += r.threads;
            out.handles += r.handles;
            let (cpu, read, write) = match self.prev.get(&id) {
                Some(p) if dwall > 0 => (
                    model::cpu_percent(p.busy, r.busy, dwall, self.ncpu),
                    r.read.saturating_sub(p.read) as f64 / dt.max(0.001),
                    r.write.saturating_sub(p.write) as f64 / dt.max(0.001),
                ),
                _ => (0.0, 0.0, 0.0),
            };
            self.prev.insert(id, Prev { busy: r.busy, read: r.read, write: r.write });
            let path = self.paths.entry(id).or_insert_with(|| sys::image_path(r.pid)).clone();
            procs.push(Proc {
                pid: r.pid,
                ppid: r.ppid,
                name: r.name,
                path,
                create: r.create,
                session: r.session,
                threads: r.threads,
                handles: r.handles,
                cpu,
                mem: r.mem,
                read,
                write,
                gpu: gpu_pid.get(&r.pid).copied().unwrap_or(0.0),
                windowed: windows.contains(&r.pid),
            });
        }
        out.procs = procs.len() as u32;
        self.prev.retain(|k, _| seen.contains(k));
        self.paths.retain(|k, _| seen.contains(k));

        let names = &mut self.names;
        let mut groups = model::group(procs, &mut |path| names.entry(path.to_lowercase()).or_insert_with(|| sys::file_info(path)).clone());
        // battery impact: smoothed over about twenty seconds
        let k = (dt / 20.0).clamp(0.05, 1.0) as f32;
        let mut keep = HashSet::new();
        for g in &mut groups {
            let score = model::impact_score(g.cpu, g.gpu, g.disk());
            let e = self.impact.entry(g.key.clone()).or_insert(score);
            *e += (score - *e) * k;
            g.impact = *e;
            keep.insert(g.key.clone());
        }
        self.impact.retain(|k, _| keep.contains(k));

        // icons of programs seen for the first time (a few per sample, so the first one stays quick)
        let mut icons = Vec::new();
        for g in &groups {
            let Some(path) = &g.path else { continue };
            if icons.len() >= 24 || self.icons.contains(&g.key) {
                continue;
            }
            self.icons.insert(g.key.clone());
            if let Some((w, h, px)) = sys::icon_pixels(path) {
                icons.push((icon_key(path), w, h, px));
            }
        }
        Sample { sys: out, groups: Some(groups), icons, took_us: 0 }
    }
}

/// This process's user + kernel time, 100 ns.
fn own_busy() -> i64 {
    use windows::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    let (mut a, mut b, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut k, &mut u);
    }
    let v = |f: FILETIME| ((f.dwHighDateTime as i64) << 32) | f.dwLowDateTime as i64;
    v(k) + v(u)
}
