//! The live numbers: all read from Windows, none estimated. Processor clock (PDH: Processor Frequency × % Processor
//! Performance), timer resolution (NtQueryTimerResolution), memory in use, the active power plan, graphics cards
//! (DXGI) and, on NVIDIA cards, their clocks (nvml.dll from the driver, read only). The wake-up delay is measured on
//! demand: how late a 1 ms high-resolution timer actually wakes a thread.

use crate::backend::{Hive, Shared, Val, guids};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use windows::{
    Win32::{
        Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1},
        System::{
            LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW},
            Performance::{
                PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterValue,
                PdhOpenQueryW, QueryPerformanceCounter, QueryPerformanceFrequency,
            },
            SystemInformation::{GetSystemDirectoryW, GlobalMemoryStatusEx, MEMORYSTATUSEX},
            Threading::{CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, SetWaitableTimer, TIMER_ALL_ACCESS, WaitForSingleObject},
        },
    },
    core::{HSTRING, PCWSTR, s, w},
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gpu {
    pub name: String,
    /// Graphics clock now and its maximum, MHz (NVIDIA only).
    pub clock: Option<u32>,
    pub clock_max: Option<u32>,
    pub mem_clock: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Live {
    pub cpu_name: String,
    /// The processor's base clock, MHz (what Windows calls its nominal frequency).
    pub base_mhz: Option<f32>,
    /// The clock it runs at now, averaged over all cores, MHz.
    pub cpu_mhz: Option<f32>,
    pub peak_mhz: Option<f32>,
    /// Timer resolution, ms: now, the finest possible, the coarsest.
    pub timer: Option<(f32, f32, f32)>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub plan: String,
    pub plan_guid: String,
    pub gpus: Vec<Gpu>,
    pub samples: u64,
    /// Processor load, % (all cores).
    pub cpu_pct: Option<f32>,
    /// The readings of the card chosen on the Graphics page.
    pub card: String,
    pub tel: Option<crate::gpu::Telemetry>,
    /// The last two minutes, one value a second, oldest first (the monitor's graphs).
    pub hist: History,
}

pub const HISTORY: usize = 120;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    pub cpu_pct: Vec<f32>,
    pub cpu_mhz: Vec<f32>,
    pub gpu_pct: Vec<f32>,
    pub gpu_mhz: Vec<f32>,
    pub gpu_temp: Vec<f32>,
    pub gpu_w: Vec<f32>,
    pub gpu_fan: Vec<f32>,
}

impl History {
    fn push(v: &mut Vec<f32>, x: Option<f64>) {
        v.push(x.map(|x| x as f32).unwrap_or(f32::NAN));
        if v.len() > HISTORY {
            v.remove(0);
        }
    }
    pub fn add(&mut self, l: &Live) {
        Self::push(&mut self.cpu_pct, l.cpu_pct.map(f64::from));
        Self::push(&mut self.cpu_mhz, l.cpu_mhz.map(f64::from));
        let t = l.tel.unwrap_or_default();
        Self::push(&mut self.gpu_pct, t.usage_pct);
        Self::push(&mut self.gpu_mhz, t.core_mhz);
        Self::push(&mut self.gpu_temp, t.temp_c);
        Self::push(&mut self.gpu_w, t.power_w);
        Self::push(&mut self.gpu_fan, t.fan_pct.or(t.fan_rpm));
    }
}

impl Live {
    pub fn mem_pct(&self) -> f32 {
        if self.mem_total == 0 { 0.0 } else { self.mem_used as f32 * 100.0 / self.mem_total as f32 }
    }
}

/// The wake-up delay: how late a 1 ms timer wakes a thread, in µs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wake {
    pub median_us: f32,
    pub p99_us: f32,
    pub max_us: f32,
    pub n: usize,
}

pub fn cpu_name() -> String {
    match crate::backend::real::read_value(Hive::Machine, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString") {
        Some(Val::Sz(s)) => s.trim().to_string(),
        _ => String::new(),
    }
}

/// (current, finest, coarsest) timer resolution in ms.
pub fn timer_resolution() -> Option<(f32, f32, f32)> {
    type Query = unsafe extern "system" fn(*mut u32, *mut u32, *mut u32) -> i32;
    unsafe {
        let ntdll = GetModuleHandleW(w!("ntdll.dll")).ok()?;
        let f = GetProcAddress(ntdll, s!("NtQueryTimerResolution"))?;
        let q: Query = std::mem::transmute(f);
        let (mut coarse, mut fine, mut cur) = (0u32, 0u32, 0u32);
        if q(&mut coarse, &mut fine, &mut cur) != 0 {
            return None;
        }
        Some((cur as f32 / 10_000.0, fine as f32 / 10_000.0, coarse as f32 / 10_000.0))
    }
}

pub fn memory() -> (u64, u64) {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok() { (m.ullTotalPhys - m.ullAvailPhys, m.ullTotalPhys) } else { (0, 0) }
}

/// Measures the wake-up delay (about 0.4 s of a sleeping thread, no processor load).
pub fn measure_wake(rounds: usize) -> Option<Wake> {
    unsafe {
        let t = CreateWaitableTimerExW(None, PCWSTR::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0).ok()?;
        let mut freq = 0i64;
        QueryPerformanceFrequency(&mut freq).ok()?;
        let mut late: Vec<f32> = Vec::with_capacity(rounds);
        for _ in 0..rounds {
            let due: i64 = -10_000; // 1 ms, relative
            let mut a = 0i64;
            let _ = QueryPerformanceCounter(&mut a);
            if SetWaitableTimer(t, &due, 0, None, None, false).is_err() {
                break;
            }
            WaitForSingleObject(t, 100);
            let mut b = 0i64;
            let _ = QueryPerformanceCounter(&mut b);
            let us = (b - a) as f64 * 1e6 / freq as f64 - 1000.0;
            late.push(us.max(0.0) as f32);
        }
        let _ = windows::Win32::Foundation::CloseHandle(t);
        if late.is_empty() {
            return None;
        }
        late.sort_by(|a, b| a.total_cmp(b));
        let n = late.len();
        Some(Wake { median_us: late[n / 2], p99_us: late[((n * 99) / 100).min(n - 1)], max_us: late[n - 1], n })
    }
}

/// The graphics cards Windows knows (not the software renderer).
pub fn gpu_names() -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let Ok(f) = CreateDXGIFactory1::<IDXGIFactory1>() else { return out };
        for i in 0..8 {
            let Ok(a) = f.EnumAdapters1(i) else { break };
            let Ok(d) = a.GetDesc1() else { continue };
            if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let name = String::from_utf16_lossy(&d.Description).trim_end_matches('\0').trim().to_string();
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// A function of a loaded library, as the function type `T` the caller declares.
unsafe fn sym<T: Copy>(lib: windows::Win32::Foundation::HMODULE, name: windows::core::PCSTR) -> Option<T> {
    let f = unsafe { GetProcAddress(lib, name) }?;
    debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of_val(&f));
    Some(unsafe { std::mem::transmute_copy(&f) })
}

/// NVIDIA's management library, loaded from System32 by full path (never from the current folder). Read only:
/// Plume Boost never calls anything that changes clocks.
struct Nvml {
    count: unsafe extern "C" fn(*mut u32) -> i32,
    handle: unsafe extern "C" fn(u32, *mut *mut core::ffi::c_void) -> i32,
    name: unsafe extern "C" fn(*mut core::ffi::c_void, *mut u8, u32) -> i32,
    clock: unsafe extern "C" fn(*mut core::ffi::c_void, u32, *mut u32) -> i32,
    max_clock: unsafe extern "C" fn(*mut core::ffi::c_void, u32, *mut u32) -> i32,
}

impl Nvml {
    fn load() -> Option<Nvml> {
        unsafe {
            let mut buf = [0u16; 260];
            let n = GetSystemDirectoryW(Some(&mut buf)) as usize;
            if n == 0 {
                return None;
            }
            let path = format!("{}\\nvml.dll", String::from_utf16_lossy(&buf[..n]));
            if !std::path::Path::new(&path).is_file() {
                return None;
            }
            let lib = LoadLibraryW(&HSTRING::from(path)).ok()?;
            let init: unsafe extern "C" fn() -> i32 = sym(lib, s!("nvmlInit_v2"))?;
            if init() != 0 {
                return None;
            }
            Some(Nvml {
                count: sym(lib, s!("nvmlDeviceGetCount_v2"))?,
                handle: sym(lib, s!("nvmlDeviceGetHandleByIndex_v2"))?,
                name: sym(lib, s!("nvmlDeviceGetName"))?,
                clock: sym(lib, s!("nvmlDeviceGetClockInfo"))?,
                max_clock: sym(lib, s!("nvmlDeviceGetMaxClockInfo"))?,
            })
        }
    }

    fn read(&self) -> Vec<Gpu> {
        let mut out = Vec::new();
        unsafe {
            let mut n = 0u32;
            if (self.count)(&mut n) != 0 {
                return out;
            }
            for i in 0..n.min(4) {
                let mut dev = std::ptr::null_mut();
                if (self.handle)(i, &mut dev) != 0 {
                    continue;
                }
                let mut name = [0u8; 96];
                let _ = (self.name)(dev, name.as_mut_ptr(), name.len() as u32);
                let name = String::from_utf8_lossy(&name).trim_end_matches('\0').to_string();
                let get = |f: unsafe extern "C" fn(*mut core::ffi::c_void, u32, *mut u32) -> i32, kind: u32| {
                    let mut v = 0u32;
                    (f(dev, kind, &mut v) == 0).then_some(v)
                };
                // 0 = graphics clock, 2 = memory clock
                out.push(Gpu { name, clock: get(self.clock, 0), clock_max: get(self.max_clock, 0), mem_clock: get(self.clock, 2) });
            }
        }
        out
    }
}

struct Pdh {
    q: PDH_HQUERY,
    freq: PDH_HCOUNTER,
    perf: PDH_HCOUNTER,
    load: PDH_HCOUNTER,
}

impl Pdh {
    fn open() -> Option<Pdh> {
        unsafe {
            let mut q = PDH_HQUERY::default();
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut q) != 0 {
                return None;
            }
            let (mut freq, mut perf, mut load) = (PDH_HCOUNTER::default(), PDH_HCOUNTER::default(), PDH_HCOUNTER::default());
            if PdhAddEnglishCounterW(q, w!(r"\Processor Information(_Total)\Processor Frequency"), 0, &mut freq) != 0
                || PdhAddEnglishCounterW(q, w!(r"\Processor Information(_Total)\% Processor Performance"), 0, &mut perf) != 0
            {
                return None;
            }
            let _ = PdhAddEnglishCounterW(q, w!(r"\Processor(_Total)\% Processor Time"), 0, &mut load);
            let _ = PdhCollectQueryData(q);
            Some(Pdh { q, freq, perf, load })
        }
    }
    fn value(c: PDH_HCOUNTER) -> Option<f64> {
        unsafe {
            let mut v = PDH_FMT_COUNTERVALUE::default();
            (PdhGetFormattedCounterValue(c, PDH_FMT_DOUBLE, None, &mut v) == 0 && v.CStatus == 0).then_some(v.Anonymous.doubleValue)
        }
    }
    /// (base MHz, effective MHz)
    fn read(&self) -> (Option<f32>, Option<f32>) {
        unsafe {
            let _ = PdhCollectQueryData(self.q);
        }
        let base = Self::value(self.freq).filter(|v| *v > 0.0);
        let perf = Self::value(self.perf);
        (base.map(|b| b as f32), base.zip(perf).map(|(b, p)| (b * p / 100.0) as f32))
    }
    /// Processor load since the last reading, %.
    fn load(&self) -> Option<f32> {
        Self::value(self.load).map(|v| v.clamp(0.0, 100.0) as f32)
    }
}

/// The processor's clock now, MHz: two readings half a second apart (PDH needs two to compute a rate).
pub fn cpu_now() -> Option<f32> {
    let p = Pdh::open()?;
    std::thread::sleep(Duration::from_millis(500));
    p.read().1
}

/// The active plan and its name.
pub fn plan(be: &Shared) -> (String, String) {
    let g = be.active_plan().unwrap_or_default();
    let name = be.plan_name(&g).unwrap_or_else(|| g.clone());
    (g, name)
}

/// The processor settings of a plan: (min, max, boost, EPP, parking) plugged in, then on battery.
pub fn limits(be: &Shared, plan: &str) -> [[Option<u32>; 5]; 2] {
    let read =
        |ac: bool| [guids::PROC_MIN, guids::PROC_MAX, guids::BOOST_MODE, guids::EPP, guids::PARK_MIN].map(|s| be.power_get(plan, guids::SUB_PROCESSOR, s, ac));
    [read(true), read(false)]
}

pub struct Control {
    pub stop: AtomicBool,
    /// The graphics card whose readings are taken (its id; empty: none).
    pub card: std::sync::Mutex<String>,
}

/// Reads the numbers once a second on its own thread and hands them to `deliver`.
pub fn start(be: Shared, deliver: impl Fn(Live) + Send + 'static) -> Arc<Control> {
    let ctl = Arc::new(Control { stop: AtomicBool::new(false), card: std::sync::Mutex::new(String::new()) });
    let c = ctl.clone();
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        let pdh = Pdh::open();
        let nvml = Nvml::load();
        let mut live = Live { cpu_name: cpu_name(), ..Default::default() };
        let names = gpu_names();
        let mut tick = 0u64;
        while !c.stop.load(Ordering::Relaxed) {
            if let Some(p) = &pdh {
                let (base, now) = p.read();
                live.base_mhz = base.or(live.base_mhz);
                live.cpu_mhz = now;
                if let Some(n) = now {
                    live.peak_mhz = Some(live.peak_mhz.map_or(n, |p| p.max(n)));
                }
                live.cpu_pct = p.load();
            }
            live.card = c.card.lock().map(|g| g.clone()).unwrap_or_default();
            live.tel = if live.card.is_empty() { None } else { be.gpu_telemetry(&live.card) };
            let snapshot = live.clone();
            live.hist.add(&snapshot);
            live.timer = timer_resolution();
            (live.mem_used, live.mem_total) = memory();
            (live.plan_guid, live.plan) = plan(&be);
            if tick.is_multiple_of(2) {
                let nv = nvml.as_ref().map(Nvml::read).unwrap_or_default();
                live.gpus = names
                    .iter()
                    .map(|n| {
                        nv.iter()
                            .find(|g| n.contains(g.name.trim_start_matches("NVIDIA ")) || g.name.contains(n.as_str()))
                            .cloned()
                            .map_or_else(|| Gpu { name: n.clone(), ..Default::default() }, |g| Gpu { name: n.clone(), ..g })
                    })
                    .collect();
            }
            live.samples += 1;
            tick += 1;
            deliver(live.clone());
            std::thread::sleep(Duration::from_millis(1000));
        }
    });
    ctl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_are_real_and_sane() {
        // reading only: nothing here changes the PC
        let (used, total) = memory();
        assert!(total > 0 && used <= total);
        if let Some((cur, fine, coarse)) = timer_resolution() {
            assert!(fine > 0.0 && fine <= cur && cur <= coarse + 0.01, "{fine} <= {cur} <= {coarse}");
        }
        let w = measure_wake(20).expect("a high-resolution timer");
        assert_eq!(w.n, 20);
        assert!(w.median_us <= w.p99_us && w.p99_us <= w.max_us);
    }
}
