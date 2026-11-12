//! NVIDIA cards, through the two libraries the driver installs in System32:
//! - NVAPI (`nvapi64.dll`): its only export is `nvapi_QueryInterface`, which gives each function from its published
//!   32-bit id. Plume Boost uses it for the clock offsets, through the "P-state 2.0" interface: the offset of the
//!   graphics clock and of the memory clock in the P0 state, with the range the driver reports for each.
//! - NVML (`nvml.dll`, NVIDIA's documented management library): readings (clocks, temperature, fan, power, load),
//!   the power limit (between the minimum and maximum the card reports; Windows asks for administrator rights) and
//!   the fan speed (a fixed speed or back to automatic, drivers 520 and later).
//!
//! The NVIDIA driver keeps no fan curve of its own: a curve needs a program running all the time, which Plume Boost
//! doesn't do. Offsets and the power limit go back to stock when Windows restarts; "Apply at sign-in" sets them again.
//!
//! Nothing here was run on a real NVIDIA card during development (the PC it was written on has Intel graphics): the
//! structures follow NVIDIA's public headers, and every value read is checked before it is used.

use super::{Caps, Card, Driver, Fan, Knob, Range, Telemetry, Tuning, Vendor, slug, sym, system_dll};
use crate::backend::{Fail, Res};
use std::ffi::c_void;

// NVAPI function ids (from NVIDIA's nvapi_interface.h)
const NVAPI_INITIALIZE: u32 = 0x0150_E828;
const NVAPI_ENUM_PHYSICAL_GPUS: u32 = 0xE5AC_921F;
const NVAPI_GPU_GET_FULL_NAME: u32 = 0xCEEE_8E9F;
const NVAPI_GPU_GET_BUS_ID: u32 = 0x1BE0_B8E5;
const NVAPI_GPU_GET_PSTATES20: u32 = 0x6FF8_1213;
const NVAPI_GPU_SET_PSTATES20: u32 = 0x0F4D_AE6B;

const NVAPI_INVALID_USER_PRIVILEGE: i32 = -137;
/// NV_GPU_PUBLIC_CLOCK_ID
const CLOCK_GRAPHICS: u32 = 0;
const CLOCK_MEMORY: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Delta {
    value: i32,
    min: i32,
    max: i32,
}

/// NV_GPU_PSTATE20_CLOCK_ENTRY_V1 (44 bytes)
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ClockEntry {
    domain: u32,
    type_id: u32,
    editable: u32,
    delta_khz: Delta,
    data: [u32; 5],
}

/// NV_GPU_PSTATE20_BASE_VOLTAGE_ENTRY_V1 (24 bytes)
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct VoltEntry {
    domain: u32,
    editable: u32,
    volt_uv: u32,
    delta_uv: Delta,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct PState {
    id: u32,
    editable: u32,
    clocks: [ClockEntry; 8],
    volts: [VoltEntry; 4],
}

/// NV_GPU_PERF_PSTATES20_INFO_V2 (7416 bytes); version 1 is the same without the last two fields (7316 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
struct P20 {
    version: u32,
    editable: u32,
    num_pstates: u32,
    num_clocks: u32,
    num_base_voltages: u32,
    pstates: [PState; 16],
    ov_num: u32,
    ov: [VoltEntry; 4],
}

const P20_V1_SIZE: u32 = 7316;
const _: () = assert!(std::mem::size_of::<P20>() == 7416);
const _: () = assert!(std::mem::size_of::<ClockEntry>() == 44);

impl P20 {
    fn zeroed() -> P20 {
        // all-zero is a valid value for every field
        unsafe { std::mem::zeroed() }
    }
    /// The P0 entry of a clock domain.
    fn p0(&self, domain: u32) -> Option<ClockEntry> {
        let n = (self.num_pstates as usize).min(16);
        let nc = (self.num_clocks as usize).min(8);
        let p = self.pstates[..n].iter().find(|p| p.id == 0)?;
        p.clocks[..nc].iter().find(|c| c.domain == domain).copied()
    }
}

type Qi = unsafe extern "C" fn(u32) -> *mut c_void;
type Handle = *mut c_void;

struct NvApi {
    get_p20: unsafe extern "C" fn(Handle, *mut P20) -> i32,
    set_p20: unsafe extern "C" fn(Handle, *const P20) -> i32,
    /// The P20 version this driver answered to.
    version: std::cell::Cell<u32>,
}

/// NVML's types (nvml.h).
type Dev = *mut c_void;
#[repr(C)]
struct PciInfo {
    bus_id_legacy: [u8; 16],
    domain: u32,
    bus: u32,
    device: u32,
    pci_device_id: u32,
    pci_subsystem_id: u32,
    bus_id: [u8; 32],
}
#[repr(C)]
struct Utilization {
    gpu: u32,
    memory: u32,
}

const NVML_ERROR_NOT_SUPPORTED: i32 = 3;
const NVML_ERROR_NO_PERMISSION: i32 = 4;

#[allow(clippy::type_complexity)]
struct Nvml {
    count: unsafe extern "C" fn(*mut u32) -> i32,
    handle: unsafe extern "C" fn(u32, *mut Dev) -> i32,
    pci: Option<unsafe extern "C" fn(Dev, *mut PciInfo) -> i32>,
    clock: unsafe extern "C" fn(Dev, u32, *mut u32) -> i32,
    temp: Option<unsafe extern "C" fn(Dev, u32, *mut u32) -> i32>,
    fan: Option<unsafe extern "C" fn(Dev, *mut u32) -> i32>,
    power: Option<unsafe extern "C" fn(Dev, *mut u32) -> i32>,
    limit: Option<unsafe extern "C" fn(Dev, *mut u32) -> i32>,
    limit_range: Option<unsafe extern "C" fn(Dev, *mut u32, *mut u32) -> i32>,
    limit_default: Option<unsafe extern "C" fn(Dev, *mut u32) -> i32>,
    set_limit: Option<unsafe extern "C" fn(Dev, u32) -> i32>,
    util: Option<unsafe extern "C" fn(Dev, *mut Utilization) -> i32>,
    fans: Option<unsafe extern "C" fn(Dev, *mut u32) -> i32>,
    fan_range: Option<unsafe extern "C" fn(Dev, *mut u32, *mut u32) -> i32>,
    set_fan: Option<unsafe extern "C" fn(Dev, u32, u32) -> i32>,
    set_fan_auto: Option<unsafe extern "C" fn(Dev, u32) -> i32>,
    fan_policy: Option<unsafe extern "C" fn(Dev, u32, *mut u32) -> i32>,
    fan_target: Option<unsafe extern "C" fn(Dev, u32, *mut u32) -> i32>,
}

struct NvCard {
    card: Card,
    handle: Option<Handle>,
    dev: Option<Dev>,
    nvml_index: u32,
}

pub struct Nvidia {
    api: Option<NvApi>,
    nvml: Option<Nvml>,
    cards: Vec<NvCard>,
}

// The handles are plain numbers given by the driver; Plume Boost calls them from one thread at a time (a mutex).
unsafe impl Send for Nvidia {}

fn ok(r: i32) -> bool {
    r == 0
}

impl Nvidia {
    pub fn load() -> Option<Nvidia> {
        let api = unsafe { load_nvapi() };
        let nvml = unsafe { load_nvml() };
        let (api_handles, nvml_devs) = (api.as_ref().map(|a| a.1.clone()).unwrap_or_default(), nvml.as_ref().map(|n| n.1.clone()).unwrap_or_default());
        let api = api.map(|a| a.0);
        let nvml = nvml.map(|n| n.0);
        if api_handles.is_empty() && nvml_devs.is_empty() {
            return None;
        }
        let mut me = Nvidia { api, nvml, cards: Vec::new() };
        // one card per NVAPI handle, matched to its NVML device by PCI bus; NVML-only cards too
        let mut used = vec![false; nvml_devs.len()];
        for (h, name, bus) in &api_handles {
            let j = nvml_devs.iter().position(|(_, _, b)| Some(*b) == *bus);
            if let Some(j) = j {
                used[j] = true;
            }
            let dev = j.map(|j| (nvml_devs[j].0, j as u32));
            me.cards.push(me.describe(Some(*h), dev, name, bus.unwrap_or(0)));
        }
        for (j, (dev, name, bus)) in nvml_devs.iter().enumerate() {
            if !used[j] {
                me.cards.push(me.describe(None, Some((*dev, j as u32)), name, *bus));
            }
        }
        Some(me)
    }

    fn describe(&self, handle: Option<Handle>, dev: Option<(Dev, u32)>, name: &str, bus: u32) -> NvCard {
        let mut caps = Caps::default();
        if let (Some(api), Some(h)) = (&self.api, handle)
            && let Some(p) = api.read(h)
        {
            for (domain, knob) in [(CLOCK_GRAPHICS, Knob::CoreOffset), (CLOCK_MEMORY, Knob::MemOffset)] {
                if let Some(c) = p.p0(domain).filter(|c| c.editable & 1 == 1) {
                    let r = Range::new(c.delta_khz.min as f64 / 1000.0, c.delta_khz.max as f64 / 1000.0, 1.0, 0.0);
                    // a range that doesn't include 0 (stock) or is absurd is not trusted
                    if r.sane() && r.min <= 0.0 && r.max >= 0.0 && r.max - r.min <= 6000.0 {
                        caps.knobs.push((knob, r));
                    }
                }
            }
        }
        if let (Some(n), Some((d, _))) = (&self.nvml, dev) {
            unsafe {
                if let (Some(range), Some(def)) = (n.limit_range, n.limit_default) {
                    let (mut lo, mut hi, mut df) = (0u32, 0u32, 0u32);
                    if ok(range(d, &mut lo, &mut hi)) && ok(def(d, &mut df)) && n.set_limit.is_some() {
                        let r = Range::new(lo as f64 / 1000.0, hi as f64 / 1000.0, 1.0, df as f64 / 1000.0);
                        if r.sane() && r.max <= 2000.0 {
                            caps.knobs.push((Knob::Power, r));
                        }
                    }
                }
                if let (Some(fr), Some(_), Some(_)) = (n.fan_range, n.set_fan, n.set_fan_auto) {
                    let (mut lo, mut hi) = (0u32, 0u32);
                    if ok(fr(d, &mut lo, &mut hi)) && hi > lo && hi <= 100 {
                        caps.fan_fixed = Some(Range::new(lo as f64, hi as f64, 1.0, lo as f64));
                    }
                }
            }
        }
        let api = match (handle.is_some() && self.api.is_some(), dev.is_some()) {
            (true, true) => "NVAPI + NVML",
            (true, false) => "NVAPI",
            _ => "NVML",
        };
        let why_not = (!caps.tunable()).then(|| "driver".to_string());
        NvCard {
            card: Card {
                id: format!("nvidia-{bus}-{}", slug(name)),
                name: name.to_string(),
                vendor: Vendor::Nvidia,
                integrated: false,
                caps,
                api: api.into(),
                why_not,
                needs_waiver: false,
            },
            handle,
            dev: dev.map(|d| d.0),
            nvml_index: dev.map(|d| d.1).unwrap_or(0),
        }
    }

    fn find(&self, id: &str) -> Option<&NvCard> {
        self.cards.iter().find(|c| c.card.id == id)
    }
}

/// Each card NVAPI lists: handle, name, PCI bus.
type ApiCards = Vec<(Handle, String, Option<u32>)>;
/// Each device NVML lists: handle, name, PCI bus.
type NvmlCards = Vec<(Dev, String, u32)>;

unsafe fn load_nvapi() -> Option<(NvApi, ApiCards)> {
    unsafe {
        let lib = system_dll("nvapi64.dll")?;
        let qi: Qi = sym(lib, "nvapi_QueryInterface")?;
        let get = |id: u32| {
            let p = qi(id);
            (!p.is_null()).then_some(p)
        };
        let init: unsafe extern "C" fn() -> i32 = std::mem::transmute(get(NVAPI_INITIALIZE)?);
        if init() != 0 {
            return None;
        }
        let enum_gpus: unsafe extern "C" fn(*mut [Handle; 64], *mut u32) -> i32 = std::mem::transmute(get(NVAPI_ENUM_PHYSICAL_GPUS)?);
        let full_name: Option<unsafe extern "C" fn(Handle, *mut [u8; 64]) -> i32> = get(NVAPI_GPU_GET_FULL_NAME).map(|p| std::mem::transmute(p));
        let bus_id: Option<unsafe extern "C" fn(Handle, *mut u32) -> i32> = get(NVAPI_GPU_GET_BUS_ID).map(|p| std::mem::transmute(p));
        let api = NvApi {
            get_p20: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(Handle, *mut P20) -> i32>(get(NVAPI_GPU_GET_PSTATES20)?),
            set_p20: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(Handle, *const P20) -> i32>(get(NVAPI_GPU_SET_PSTATES20)?),
            version: std::cell::Cell::new(0),
        };
        let mut hs = [std::ptr::null_mut(); 64];
        let mut n = 0u32;
        if enum_gpus(&mut hs, &mut n) != 0 {
            return None;
        }
        let mut out = Vec::new();
        for h in hs.iter().take((n as usize).min(64)) {
            let mut name = [0u8; 64];
            let name = match full_name {
                Some(f) if f(*h, &mut name) == 0 => String::from_utf8_lossy(&name).trim_end_matches('\0').trim().to_string(),
                _ => "NVIDIA GPU".into(),
            };
            let mut bus = 0u32;
            let bus = bus_id.and_then(|f| (f(*h, &mut bus) == 0).then_some(bus));
            out.push((*h, name, bus));
        }
        Some((api, out))
    }
}

impl NvApi {
    /// The P-state 2.0 table, asking with the newest version first.
    fn read(&self, h: Handle) -> Option<P20> {
        let mut p = P20::zeroed();
        let size = std::mem::size_of::<P20>() as u32;
        for v in [(3 << 16) | size, (2 << 16) | size, (1 << 16) | P20_V1_SIZE] {
            if self.version.get() != 0 && self.version.get() != v {
                continue;
            }
            p.version = v;
            if unsafe { (self.get_p20)(h, &mut p) } == 0 {
                self.version.set(v);
                return Some(p);
            }
        }
        None
    }

    /// Sets the P0 offsets of these clock domains (kHz).
    fn write(&self, h: Handle, offsets: &[(u32, i32)], template: &P20) -> Res {
        let mut p = P20::zeroed();
        p.version = self.version.get();
        p.num_pstates = 1;
        p.num_clocks = offsets.len() as u32;
        p.pstates[0].id = 0;
        for (i, (domain, khz)) in offsets.iter().enumerate() {
            let read = template.p0(*domain).ok_or(Fail::Missing)?;
            p.pstates[0].clocks[i] =
                ClockEntry { domain: *domain, type_id: read.type_id, editable: 1, delta_khz: Delta { value: *khz, min: 0, max: 0 }, data: [0; 5] };
        }
        match unsafe { (self.set_p20)(h, &p) } {
            0 => Ok(()),
            NVAPI_INVALID_USER_PRIVILEGE => Err(Fail::NeedsAdmin),
            e => Err(Fail::Other(format!("NVAPI error {e}"))),
        }
    }
}

unsafe fn load_nvml() -> Option<(Nvml, NvmlCards)> {
    unsafe {
        let lib = system_dll("nvml.dll")?;
        let init: unsafe extern "C" fn() -> i32 = sym(lib, "nvmlInit_v2")?;
        if init() != 0 {
            return None;
        }
        let n = Nvml {
            count: sym(lib, "nvmlDeviceGetCount_v2")?,
            handle: sym(lib, "nvmlDeviceGetHandleByIndex_v2")?,
            pci: sym(lib, "nvmlDeviceGetPciInfo_v3"),
            clock: sym(lib, "nvmlDeviceGetClockInfo")?,
            temp: sym(lib, "nvmlDeviceGetTemperature"),
            fan: sym(lib, "nvmlDeviceGetFanSpeed"),
            power: sym(lib, "nvmlDeviceGetPowerUsage"),
            limit: sym(lib, "nvmlDeviceGetEnforcedPowerLimit"),
            limit_range: sym(lib, "nvmlDeviceGetPowerManagementLimitConstraints"),
            limit_default: sym(lib, "nvmlDeviceGetPowerManagementDefaultLimit"),
            set_limit: sym(lib, "nvmlDeviceSetPowerManagementLimit"),
            util: sym(lib, "nvmlDeviceGetUtilizationRates"),
            fans: sym(lib, "nvmlDeviceGetNumFans"),
            fan_range: sym(lib, "nvmlDeviceGetMinMaxFanSpeed"),
            set_fan: sym(lib, "nvmlDeviceSetFanSpeed_v2"),
            set_fan_auto: sym(lib, "nvmlDeviceSetDefaultFanSpeed_v2"),
            fan_policy: sym(lib, "nvmlDeviceGetFanControlPolicy_v2"),
            fan_target: sym(lib, "nvmlDeviceGetTargetFanSpeed"),
        };
        let name: Option<unsafe extern "C" fn(Dev, *mut u8, u32) -> i32> = sym(lib, "nvmlDeviceGetName");
        let mut count = 0u32;
        if (n.count)(&mut count) != 0 {
            return None;
        }
        let mut out = Vec::new();
        for i in 0..count.min(8) {
            let mut d: Dev = std::ptr::null_mut();
            if (n.handle)(i, &mut d) != 0 {
                continue;
            }
            let mut buf = [0u8; 96];
            let nm = match name {
                Some(f) if f(d, buf.as_mut_ptr(), buf.len() as u32) == 0 => String::from_utf8_lossy(&buf).trim_end_matches('\0').trim().to_string(),
                _ => "NVIDIA GPU".into(),
            };
            let mut pci: PciInfo = std::mem::zeroed();
            let bus = match n.pci {
                Some(f) if f(d, &mut pci) == 0 => pci.bus,
                _ => u32::MAX - i,
            };
            out.push((d, nm, bus));
        }
        Some((n, out))
    }
}

impl Nvml {
    fn fan_count(&self, d: Dev) -> u32 {
        let mut n = 1u32;
        match self.fans {
            Some(f) if unsafe { f(d, &mut n) } == 0 => n.min(8),
            _ => 1,
        }
    }
    fn nvml_err(code: i32) -> Fail {
        match code {
            NVML_ERROR_NO_PERMISSION => Fail::NeedsAdmin,
            NVML_ERROR_NOT_SUPPORTED => Fail::Missing,
            e => Fail::Other(format!("NVML error {e}")),
        }
    }
}

/// `nvidia-smi -i <n> -pl <watts>`: NVIDIA's own tool, where NVML's call isn't allowed on this card.
fn smi_power_limit(index: u32, watts: f64) -> Res {
    use std::os::windows::process::CommandExt;
    let exe = std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_default().join("System32").join("nvidia-smi.exe");
    if !exe.is_file() {
        return Err(Fail::Missing);
    }
    let out = std::process::Command::new(exe)
        .args(["-i", &index.to_string(), "-pl", &format!("{watts:.0}")])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| Fail::Other(e.to_string()))?;
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase() + &String::from_utf8_lossy(&out.stderr).to_lowercase();
    if out.status.success() {
        Ok(())
    } else if text.contains("permission") || text.contains("administrator") {
        Err(Fail::NeedsAdmin)
    } else {
        Err(Fail::Other(text.trim().to_string()))
    }
}

impl Driver for Nvidia {
    fn cards(&self) -> Vec<Card> {
        self.cards.iter().map(|c| c.card.clone()).collect()
    }

    fn telemetry(&self, card: &str) -> Option<Telemetry> {
        let c = self.find(card)?;
        let (n, d) = (self.nvml.as_ref()?, c.dev?);
        let mut t = Telemetry::default();
        unsafe {
            let read = |f: Option<unsafe extern "C" fn(Dev, *mut u32) -> i32>| {
                let mut v = 0u32;
                f.and_then(|f| (f(d, &mut v) == 0).then_some(v as f64))
            };
            let mut v = 0u32;
            t.core_mhz = ((n.clock)(d, 0, &mut v) == 0).then_some(v as f64);
            t.mem_mhz = ((n.clock)(d, 2, &mut v) == 0).then_some(v as f64);
            if let Some(f) = n.temp {
                t.temp_c = (f(d, 0, &mut v) == 0).then_some(v as f64);
            }
            t.fan_pct = read(n.fan);
            t.power_w = read(n.power).map(|mw| mw / 1000.0);
            t.power_limit_w = read(n.limit).map(|mw| mw / 1000.0);
            if let Some(f) = n.util {
                let mut u = Utilization { gpu: 0, memory: 0 };
                t.usage_pct = (f(d, &mut u) == 0).then_some(u.gpu as f64);
            }
        }
        Some(t)
    }

    fn get(&self, card: &str) -> Option<Tuning> {
        let c = self.find(card)?;
        let mut t = Tuning::default();
        if let (Some(api), Some(h)) = (&self.api, c.handle)
            && let Some(p) = api.read(h)
        {
            for (domain, knob) in [(CLOCK_GRAPHICS, Knob::CoreOffset), (CLOCK_MEMORY, Knob::MemOffset)] {
                if c.card.caps.range(knob).is_some()
                    && let Some(e) = p.p0(domain)
                {
                    t.knobs.insert(knob, e.delta_khz.value as f64 / 1000.0);
                }
            }
        }
        if let (Some(n), Some(d)) = (&self.nvml, c.dev) {
            unsafe {
                if c.card.caps.range(Knob::Power).is_some() {
                    let mut mw = 0u32;
                    if let Some(f) = n.limit
                        && f(d, &mut mw) == 0
                    {
                        t.knobs.insert(Knob::Power, (mw as f64 / 1000.0).round());
                    }
                }
                // manual fan control (policy 1) with its target speed; otherwise automatic
                if let (Some(pol), Some(tgt)) = (n.fan_policy, n.fan_target) {
                    let (mut p, mut s) = (0u32, 0u32);
                    if pol(d, 0, &mut p) == 0 && p == 1 && tgt(d, 0, &mut s) == 0 {
                        t.fan = Fan::Fixed(s);
                    }
                }
            }
        }
        Some(t)
    }

    fn set(&self, card: &str, t: &Tuning) -> Res {
        let c = self.find(card).ok_or(Fail::Missing)?;
        let now = self.get(card).unwrap_or_default();
        // clocks: only what changes, inside the range read from the driver
        let mut offsets = Vec::new();
        for (domain, knob) in [(CLOCK_GRAPHICS, Knob::CoreOffset), (CLOCK_MEMORY, Knob::MemOffset)] {
            if let (Some(v), Some(r)) = (t.get(knob), c.card.caps.range(knob))
                && now.get(knob) != Some(v)
            {
                offsets.push((domain, (r.clamp(v) * 1000.0) as i32));
            }
        }
        if !offsets.is_empty() {
            let (api, h) = (self.api.as_ref().ok_or(Fail::Missing)?, c.handle.ok_or(Fail::Missing)?);
            let template = api.read(h).ok_or(Fail::Missing)?;
            api.write(h, &offsets, &template)?;
        }
        if let (Some(v), Some(r)) = (t.get(Knob::Power), c.card.caps.range(Knob::Power))
            && now.get(Knob::Power) != Some(v)
        {
            let (n, d) = (self.nvml.as_ref().ok_or(Fail::Missing)?, c.dev.ok_or(Fail::Missing)?);
            let mw = (r.clamp(v) * 1000.0) as u32;
            let code = unsafe { n.set_limit.map(|f| f(d, mw)).unwrap_or(NVML_ERROR_NOT_SUPPORTED) };
            match code {
                0 => {}
                NVML_ERROR_NOT_SUPPORTED => smi_power_limit(c.nvml_index, r.clamp(v))?,
                e => return Err(Nvml::nvml_err(e)),
            }
        }
        if t.fan != now.fan {
            let (n, d) = (self.nvml.as_ref().ok_or(Fail::Missing)?, c.dev.ok_or(Fail::Missing)?);
            for i in 0..n.fan_count(d) {
                let code = unsafe {
                    match &t.fan {
                        Fan::Fixed(p) => {
                            let r = c.card.caps.fan_fixed.ok_or(Fail::Missing)?;
                            n.set_fan.map(|f| f(d, i, r.clamp(*p as f64) as u32))
                        }
                        _ => n.set_fan_auto.map(|f| f(d, i)),
                    }
                }
                .unwrap_or(NVML_ERROR_NOT_SUPPORTED);
                if code != 0 {
                    return Err(Nvml::nvml_err(code));
                }
            }
        }
        Ok(())
    }
}
