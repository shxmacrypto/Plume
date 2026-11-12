//! AMD cards, through ADLX (`amdadlx64.dll`, installed with AMD Software), its public C interface: objects are
//! pointers to a table of functions, in the order of AMD's published headers (ISystem.h, IGPUTuning.h,
//! IGPUManualGFXTuning.h, IGPUManualVRAMTuning.h, IGPUManualFanTuning.h, IGPUManualPowerTuning.h,
//! IPerformanceMonitoring.h). Plume Boost reads and sets, within the ranges ADLX reports:
//! - the highest graphics clock (MHz) and the graphics voltage (mV: lower is an undervolt) — "manual graphics tuning 2",
//!   the interface of Radeon RX 5000 and later;
//! - the memory clock (MHz);
//! - the power limit (% around stock);
//! - the fan curve (temperature / speed points, kept by the driver itself; a fixed speed is a flat curve);
//! - readings: clocks, temperatures, power, fan, load.
//!
//! "Reset" asks AMD's own reset to factory settings.
//!
//! Nothing here was run on a real AMD card during development: every range read is checked before it is used.

use super::{Caps, Card, CurveCaps, Driver, Fan, Knob, Range, Telemetry, Tuning, Vendor, slug, sym, system_dll};
use crate::backend::{Fail, Res};
use std::ffi::{c_char, c_void};

type Obj = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct IntRange {
    min: i32,
    max: i32,
    step: i32,
}

impl IntRange {
    fn range(self, default: i32) -> Option<Range> {
        let r = Range::new(self.min as f64, self.max as f64, self.step.max(1) as f64, default as f64);
        r.sane().then_some(r)
    }
}

/// The function at `index` in the object's table.
unsafe fn f<T: Copy>(obj: Obj, index: usize) -> T {
    unsafe {
        let table = *(obj as *const *const usize);
        std::mem::transmute_copy(&*table.add(index))
    }
}

fn succeeded(r: i32) -> bool {
    // ADLX_OK, ADLX_ALREADY_ENABLED, ADLX_ALREADY_INITIALIZED
    (0..=2).contains(&r)
}

// IADLXInterface
const ACQUIRE_RELEASE: usize = 1;
const QUERY_INTERFACE: usize = 2;

unsafe fn release(o: Obj) {
    if !o.is_null() {
        unsafe { f::<unsafe extern "system" fn(Obj) -> i32>(o, ACQUIRE_RELEASE)(o) };
    }
}

unsafe fn query(o: Obj, iid: &str) -> Option<Obj> {
    let w: Vec<u16> = iid.encode_utf16().chain([0]).collect();
    let mut out: Obj = std::ptr::null_mut();
    let r = unsafe { f::<unsafe extern "system" fn(Obj, *const u16, *mut Obj) -> i32>(o, QUERY_INTERFACE)(o, w.as_ptr(), &mut out) };
    (succeeded(r) && !out.is_null()).then_some(out)
}

/// An interface obtained for a call, released afterwards.
struct Held(Obj);
impl Drop for Held {
    fn drop(&mut self) {
        unsafe { release(self.0) }
    }
}

// IADLXSystem (no reference counting)
const SYS_GET_GPUS: usize = 1;
const SYS_GPU_TUNING: usize = 8;
const SYS_PERF: usize = 9;
// IADLXList / IADLXGPUList
const LIST_SIZE: usize = 3;
const GPULIST_AT: usize = 11;
const FANLIST_AT: usize = 11;
// IADLXGPU
const GPU_TYPE: usize = 5;
const GPU_NAME: usize = 7;
const GPU_UNIQUE_ID: usize = 18;
// IADLXGPUTuningServices
const TS_RESET: usize = 5;
const TS_SUPPORTED_GFX: usize = 8;
const TS_SUPPORTED_VRAM: usize = 9;
const TS_SUPPORTED_FAN: usize = 10;
const TS_SUPPORTED_POWER: usize = 11;
const TS_GFX: usize = 14;
const TS_VRAM: usize = 15;
const TS_FAN: usize = 16;
const TS_POWER: usize = 17;
// IADLXManualGraphicsTuning2
const GFX_MAX_RANGE: usize = 6;
const GFX_MAX_GET: usize = 7;
const GFX_MAX_SET: usize = 8;
const GFX_VOLT_RANGE: usize = 9;
const GFX_VOLT_GET: usize = 10;
const GFX_VOLT_SET: usize = 11;
// IADLXManualVRAMTuning2
const VRAM_RANGE: usize = 7;
const VRAM_GET: usize = 8;
const VRAM_SET: usize = 9;
// IADLXManualPowerTuning
const PWR_RANGE: usize = 3;
const PWR_GET: usize = 4;
const PWR_SET: usize = 5;
// IADLXManualFanTuning and its states
const FAN_RANGES: usize = 3;
const FAN_STATES: usize = 4;
const FAN_EMPTY: usize = 5;
const FAN_VALID: usize = 6;
const FAN_SET: usize = 7;
const STATE_SPEED_GET: usize = 3;
const STATE_SPEED_SET: usize = 4;
const STATE_TEMP_GET: usize = 5;
const STATE_TEMP_SET: usize = 6;
// IADLXPerformanceMonitoringServices / IADLXGPUMetrics
const PERF_GPU_METRICS: usize = 18;
const M_USAGE: usize = 4;
const M_CLOCK: usize = 5;
const M_VRAM_CLOCK: usize = 6;
const M_TEMP: usize = 7;
const M_HOTSPOT: usize = 8;
const M_POWER: usize = 9;
const M_FAN: usize = 11;

struct AmdCard {
    card: Card,
    gpu: Obj,
}

pub struct Amd {
    system: Obj,
    tuning: Obj,
    perf: Obj,
    cards: Vec<AmdCard>,
}

// The pointers are ADLX's objects; Plume Boost calls them from one thread at a time (a mutex).
unsafe impl Send for Amd {}

type GetInt = unsafe extern "system" fn(Obj, *mut i32) -> i32;
type SetInt = unsafe extern "system" fn(Obj, i32) -> i32;
type GetRange = unsafe extern "system" fn(Obj, *mut IntRange) -> i32;
type GpuBool = unsafe extern "system" fn(Obj, Obj, *mut bool) -> i32;
type GpuIface = unsafe extern "system" fn(Obj, Obj, *mut Obj) -> i32;

impl Amd {
    pub fn load() -> Option<Amd> {
        unsafe {
            let lib = system_dll("amdadlx64.dll")?;
            let version: unsafe extern "system" fn(*mut u64) -> i32 = sym(lib, "ADLXQueryFullVersion")?;
            let init: unsafe extern "system" fn(u64, *mut Obj) -> i32 = sym(lib, "ADLXInitialize")?;
            let mut v = 0u64;
            if !succeeded(version(&mut v)) {
                return None;
            }
            let mut system: Obj = std::ptr::null_mut();
            if !succeeded(init(v, &mut system)) || system.is_null() {
                return None;
            }
            let mut tuning: Obj = std::ptr::null_mut();
            let _ = f::<unsafe extern "system" fn(Obj, *mut Obj) -> i32>(system, SYS_GPU_TUNING)(system, &mut tuning);
            let mut perf: Obj = std::ptr::null_mut();
            let _ = f::<unsafe extern "system" fn(Obj, *mut Obj) -> i32>(system, SYS_PERF)(system, &mut perf);
            let mut me = Amd { system, tuning, perf, cards: Vec::new() };
            let mut list: Obj = std::ptr::null_mut();
            if !succeeded(f::<unsafe extern "system" fn(Obj, *mut Obj) -> i32>(system, SYS_GET_GPUS)(system, &mut list)) || list.is_null() {
                return Some(me);
            }
            let list = Held(list);
            let n = f::<unsafe extern "system" fn(Obj) -> u32>(list.0, LIST_SIZE)(list.0);
            for i in 0..n.min(8) {
                let mut gpu: Obj = std::ptr::null_mut();
                if !succeeded(f::<unsafe extern "system" fn(Obj, u32, *mut Obj) -> i32>(list.0, GPULIST_AT)(list.0, i, &mut gpu)) || gpu.is_null() {
                    continue;
                }
                let card = me.describe(gpu);
                me.cards.push(AmdCard { card, gpu });
            }
            Some(me)
        }
    }

    unsafe fn describe(&self, gpu: Obj) -> Card {
        unsafe {
            let mut name: *const c_char = std::ptr::null();
            let _ = f::<unsafe extern "system" fn(Obj, *mut *const c_char) -> i32>(gpu, GPU_NAME)(gpu, &mut name);
            let name = if name.is_null() { "AMD Radeon".to_string() } else { std::ffi::CStr::from_ptr(name).to_string_lossy().trim().to_string() };
            let mut ty = 0i32;
            let _ = f::<GetInt>(gpu, GPU_TYPE)(gpu, &mut ty);
            let mut uid = 0i32;
            let _ = f::<GetInt>(gpu, GPU_UNIQUE_ID)(gpu, &mut uid);
            let integrated = ty == 1;
            let mut caps = Caps { power_percent: true, ..Default::default() };
            if !self.tuning.is_null() && !integrated {
                if self.supported(gpu, TS_SUPPORTED_GFX)
                    && let Some(g) = self.iface(gpu, TS_GFX, "IADLXManualGraphicsTuning2")
                {
                    for (rng, get, knob) in [(GFX_MAX_RANGE, GFX_MAX_GET, Knob::CoreMax), (GFX_VOLT_RANGE, GFX_VOLT_GET, Knob::Voltage)] {
                        if let Some(r) = read_range(g.0, rng, get) {
                            caps.knobs.push((knob, r));
                        }
                    }
                }
                if self.supported(gpu, TS_SUPPORTED_VRAM)
                    && let Some(v) = self.iface(gpu, TS_VRAM, "IADLXManualVRAMTuning2")
                    && let Some(r) = read_range(v.0, VRAM_RANGE, VRAM_GET)
                {
                    caps.knobs.push((Knob::MemMax, r));
                }
                if self.supported(gpu, TS_SUPPORTED_POWER)
                    && let Some(p) = self.iface(gpu, TS_POWER, "IADLXManualPowerTuning")
                {
                    let mut rg = IntRange::default();
                    if succeeded(f::<GetRange>(p.0, PWR_RANGE)(p.0, &mut rg))
                        && let Some(r) = rg.range(0)
                    {
                        caps.knobs.push((Knob::Power, r));
                    }
                }
                if self.supported(gpu, TS_SUPPORTED_FAN)
                    && let Some(fan) = self.iface(gpu, TS_FAN, "IADLXManualFanTuning")
                {
                    let (mut sp, mut tp) = (IntRange::default(), IntRange::default());
                    let points = fan_states(fan.0).map(|s| s.len()).unwrap_or(0);
                    if succeeded(f::<unsafe extern "system" fn(Obj, *mut IntRange, *mut IntRange) -> i32>(fan.0, FAN_RANGES)(fan.0, &mut sp, &mut tp))
                        && let (Some(speed), Some(temp)) = (sp.range(sp.min), tp.range(tp.min))
                        && points >= 2
                        && speed.max <= 100.0
                    {
                        caps.fan_curve = Some(CurveCaps { temp, speed, points });
                        caps.fan_fixed = Some(speed);
                    }
                }
            }
            let why_not = if integrated {
                Some("integrated".to_string())
            } else if !caps.tunable() {
                Some("driver".to_string())
            } else {
                None
            };
            Card { id: format!("amd-{uid}-{}", slug(&name)), name, vendor: Vendor::Amd, integrated, caps, api: "ADLX".into(), why_not, needs_waiver: false }
        }
    }

    unsafe fn supported(&self, gpu: Obj, index: usize) -> bool {
        let mut b = false;
        unsafe { succeeded(f::<GpuBool>(self.tuning, index)(self.tuning, gpu, &mut b)) && b }
    }

    /// One of the manual tuning interfaces of a card.
    unsafe fn iface(&self, gpu: Obj, index: usize, iid: &str) -> Option<Held> {
        unsafe {
            let mut base: Obj = std::ptr::null_mut();
            if !succeeded(f::<GpuIface>(self.tuning, index)(self.tuning, gpu, &mut base)) || base.is_null() {
                return None;
            }
            let base = Held(base);
            query(base.0, iid).map(Held)
        }
    }

    fn find(&self, id: &str) -> Option<&AmdCard> {
        self.cards.iter().find(|c| c.card.id == id)
    }
}

unsafe fn read_range(o: Obj, range: usize, get: usize) -> Option<Range> {
    unsafe {
        let mut rg = IntRange::default();
        let mut cur = 0i32;
        if !succeeded(f::<GetRange>(o, range)(o, &mut rg)) || !succeeded(f::<GetInt>(o, get)(o, &mut cur)) {
            return None;
        }
        // stock is unknown to ADLX: the value found when Plume Boost first read the card stands for it
        rg.range(cur.clamp(rg.min, rg.max))
    }
}

/// The fan curve as the driver has it: (°C, %) points.
unsafe fn fan_states(fan: Obj) -> Option<Vec<(u32, u32)>> {
    unsafe {
        let mut list: Obj = std::ptr::null_mut();
        if !succeeded(f::<unsafe extern "system" fn(Obj, *mut Obj) -> i32>(fan, FAN_STATES)(fan, &mut list)) || list.is_null() {
            return None;
        }
        let list = Held(list);
        let n = f::<unsafe extern "system" fn(Obj) -> u32>(list.0, LIST_SIZE)(list.0);
        let mut out = Vec::new();
        for i in 0..n.min(16) {
            let mut st: Obj = std::ptr::null_mut();
            if !succeeded(f::<unsafe extern "system" fn(Obj, u32, *mut Obj) -> i32>(list.0, FANLIST_AT)(list.0, i, &mut st)) || st.is_null() {
                return None;
            }
            let st = Held(st);
            let (mut s, mut t) = (0i32, 0i32);
            let _ = f::<GetInt>(st.0, STATE_SPEED_GET)(st.0, &mut s);
            let _ = f::<GetInt>(st.0, STATE_TEMP_GET)(st.0, &mut t);
            out.push((t.max(0) as u32, s.max(0) as u32));
        }
        Some(out)
    }
}

unsafe fn set_fan_states(fan: Obj, points: &[(u32, u32)]) -> Res {
    unsafe {
        let mut list: Obj = std::ptr::null_mut();
        if !succeeded(f::<unsafe extern "system" fn(Obj, *mut Obj) -> i32>(fan, FAN_EMPTY)(fan, &mut list)) || list.is_null() {
            return Err(Fail::Missing);
        }
        let list = Held(list);
        let n = f::<unsafe extern "system" fn(Obj) -> u32>(list.0, LIST_SIZE)(list.0) as usize;
        if n != points.len() {
            return Err(Fail::Other(format!("the driver takes {n} fan points")));
        }
        for (i, (t, s)) in points.iter().enumerate() {
            let mut st: Obj = std::ptr::null_mut();
            if !succeeded(f::<unsafe extern "system" fn(Obj, u32, *mut Obj) -> i32>(list.0, FANLIST_AT)(list.0, i as u32, &mut st)) || st.is_null() {
                return Err(Fail::Missing);
            }
            let st = Held(st);
            let _ = f::<SetInt>(st.0, STATE_SPEED_SET)(st.0, *s as i32);
            let _ = f::<SetInt>(st.0, STATE_TEMP_SET)(st.0, *t as i32);
        }
        let mut bad = -1i32;
        let v = f::<unsafe extern "system" fn(Obj, Obj, *mut i32) -> i32>(fan, FAN_VALID)(fan, list.0, &mut bad);
        if !succeeded(v) || bad >= 0 {
            return Err(Fail::Other(format!("the driver refused fan point {}", bad + 1)));
        }
        let r = f::<unsafe extern "system" fn(Obj, Obj) -> i32>(fan, FAN_SET)(fan, list.0);
        if succeeded(r) { Ok(()) } else { Err(Fail::Other(format!("ADLX error {r}"))) }
    }
}

fn adlx(r: i32) -> Res {
    if succeeded(r) { Ok(()) } else { Err(Fail::Other(format!("ADLX error {r}"))) }
}

impl Driver for Amd {
    /// AMD's own "reset to factory settings" for a card's tuning.
    fn reset(&self, card: &str) -> Res {
        let c = self.find(card).ok_or(Fail::Missing)?;
        adlx(unsafe { f::<unsafe extern "system" fn(Obj, Obj) -> i32>(self.tuning, TS_RESET)(self.tuning, c.gpu) })
    }

    fn cards(&self) -> Vec<Card> {
        self.cards.iter().map(|c| c.card.clone()).collect()
    }

    fn telemetry(&self, card: &str) -> Option<Telemetry> {
        let c = self.find(card)?;
        if self.perf.is_null() {
            return None;
        }
        unsafe {
            let mut m: Obj = std::ptr::null_mut();
            if !succeeded(f::<GpuIface>(self.perf, PERF_GPU_METRICS)(self.perf, c.gpu, &mut m)) || m.is_null() {
                return None;
            }
            let m = Held(m);
            let dbl = |i: usize| {
                let mut v = 0f64;
                succeeded(f::<unsafe extern "system" fn(Obj, *mut f64) -> i32>(m.0, i)(m.0, &mut v)).then_some(v)
            };
            let int = |i: usize| {
                let mut v = 0i32;
                succeeded(f::<GetInt>(m.0, i)(m.0, &mut v)).then_some(v as f64)
            };
            Some(Telemetry {
                core_mhz: int(M_CLOCK),
                mem_mhz: int(M_VRAM_CLOCK),
                temp_c: dbl(M_TEMP),
                hotspot_c: dbl(M_HOTSPOT),
                fan_pct: None,
                fan_rpm: int(M_FAN),
                power_w: dbl(M_POWER),
                power_limit_w: None,
                usage_pct: dbl(M_USAGE),
            })
        }
    }

    fn get(&self, card: &str) -> Option<Tuning> {
        let c = self.find(card)?;
        let caps = &c.card.caps;
        let mut t = Tuning::default();
        unsafe {
            let int = |o: Obj, i: usize| {
                let mut v = 0i32;
                succeeded(f::<GetInt>(o, i)(o, &mut v)).then_some(v as f64)
            };
            if (caps.range(Knob::CoreMax).is_some() || caps.range(Knob::Voltage).is_some())
                && let Some(g) = self.iface(c.gpu, TS_GFX, "IADLXManualGraphicsTuning2")
            {
                if let Some(v) = int(g.0, GFX_MAX_GET).filter(|_| caps.range(Knob::CoreMax).is_some()) {
                    t.knobs.insert(Knob::CoreMax, v);
                }
                if let Some(v) = int(g.0, GFX_VOLT_GET).filter(|_| caps.range(Knob::Voltage).is_some()) {
                    t.knobs.insert(Knob::Voltage, v);
                }
            }
            if caps.range(Knob::MemMax).is_some()
                && let Some(v) = self.iface(c.gpu, TS_VRAM, "IADLXManualVRAMTuning2")
                && let Some(x) = int(v.0, VRAM_GET)
            {
                t.knobs.insert(Knob::MemMax, x);
            }
            if caps.range(Knob::Power).is_some()
                && let Some(p) = self.iface(c.gpu, TS_POWER, "IADLXManualPowerTuning")
                && let Some(x) = int(p.0, PWR_GET)
            {
                t.knobs.insert(Knob::Power, x);
            }
            if caps.fan_curve.is_some()
                && let Some(fan) = self.iface(c.gpu, TS_FAN, "IADLXManualFanTuning")
                && let Some(pts) = fan_states(fan.0)
            {
                t.fan = Fan::Curve(pts);
            }
        }
        Some(t)
    }

    fn set(&self, card: &str, t: &Tuning) -> Res {
        let c = self.find(card).ok_or(Fail::Missing)?;
        let caps = &c.card.caps;
        let now = self.get(card).unwrap_or_default();
        let changed = |k: Knob| t.get(k).filter(|v| now.get(k) != Some(*v)).and_then(|v| caps.range(k).map(|r| r.clamp(v) as i32));
        unsafe {
            let (core, volt) = (changed(Knob::CoreMax), changed(Knob::Voltage));
            if core.is_some() || volt.is_some() {
                let g = self.iface(c.gpu, TS_GFX, "IADLXManualGraphicsTuning2").ok_or(Fail::Missing)?;
                if let Some(v) = core {
                    adlx(f::<SetInt>(g.0, GFX_MAX_SET)(g.0, v))?;
                }
                if let Some(v) = volt {
                    adlx(f::<SetInt>(g.0, GFX_VOLT_SET)(g.0, v))?;
                }
            }
            if let Some(v) = changed(Knob::MemMax) {
                let o = self.iface(c.gpu, TS_VRAM, "IADLXManualVRAMTuning2").ok_or(Fail::Missing)?;
                adlx(f::<SetInt>(o.0, VRAM_SET)(o.0, v))?;
            }
            if let Some(v) = changed(Knob::Power) {
                let o = self.iface(c.gpu, TS_POWER, "IADLXManualPowerTuning").ok_or(Fail::Missing)?;
                adlx(f::<SetInt>(o.0, PWR_SET)(o.0, v))?;
            }
            if t.fan != now.fan && t.fan != Fan::Auto {
                let fc = caps.fan_curve.as_ref().ok_or(Fail::Missing)?;
                let fan = self.iface(c.gpu, TS_FAN, "IADLXManualFanTuning").ok_or(Fail::Missing)?;
                let now_pts = match &now.fan {
                    Fan::Curve(p) => p.clone(),
                    _ => Vec::new(),
                };
                let pts: Vec<(u32, u32)> = match &t.fan {
                    // a fixed speed: the same temperatures, every point at that speed
                    Fan::Fixed(p) => now_pts.iter().map(|(tc, _)| (*tc, fc.speed.clamp(*p as f64) as u32)).collect(),
                    Fan::Curve(p) => p.clone(),
                    Fan::Auto => Vec::new(),
                };
                set_fan_states(fan.0, &pts)?;
            }
        }
        Ok(())
    }
}

impl Drop for Amd {
    fn drop(&mut self) {
        unsafe {
            for c in &self.cards {
                release(c.gpu);
            }
            release(self.tuning);
            release(self.perf);
        }
        let _ = self.system;
    }
}
