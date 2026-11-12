//! Intel graphics, through the Intel Graphics Control Library (`ControlLib.dll`, installed with Intel's graphics
//! driver), with the structures of Intel's public header (igcl_api.h):
//! - Arc cards: graphics frequency offset (MHz), voltage offset (mV), power limit and temperature limit, within the
//!   ranges `ctlOverclockGetProperties` reports, and the fans (automatic, a fixed speed or a curve kept by the driver).
//!   Intel asks the user to accept that overclocking may void the warranty (`ctlOverclockWaiverSet`) before any of it:
//!   Plume Boost shows that text and calls it only after the user agrees.
//! - Integrated graphics (Iris Xe, UHD): readings only (clock, temperature, power from the energy counter, load).
//!   Their clocks and power belong to the processor's power management: the power plan and its processor settings.

use super::{Caps, Card, CurveCaps, Driver, Fan, Knob, Range, Telemetry, Tuning, Vendor, slug, sym, system_dll};
use crate::backend::{Fail, Res};
use std::{cell::RefCell, collections::HashMap, ffi::c_void};

type H = *mut c_void;

#[repr(C)]
struct InitArgs {
    size: u32,
    version: u8,
    app_version: u32,
    flags: u32,
    supported_version: u32,
    uid: [u8; 16],
}
const _: () = assert!(std::mem::size_of::<InitArgs>() == 36);

/// ctl_device_adapter_properties_t (version 2)
#[repr(C)]
struct DevProps {
    size: u32,
    version: u8,
    device_id: *mut c_void,
    device_id_size: u32,
    device_type: u32,
    supported: u32,
    driver_version: u64,
    firmware: [u64; 3],
    vendor: u32,
    device: u32,
    rev: u32,
    eus: u32,
    sub_slices: u32,
    slices: u32,
    name: [u8; 100],
    adapter_flags: u32,
    frequency: u32,
    subsys: u16,
    subsys_vendor: u16,
    bdf: [u8; 3],
    reserved: [u8; 112],
}
const _: () = assert!(std::mem::size_of::<DevProps>() == 320);

/// ctl_oc_control_info_t
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Control {
    supported: bool,
    relative: bool,
    reference: bool,
    units: u32,
    min: f64,
    max: f64,
    step: f64,
    default: f64,
    reference_value: f64,
}

/// ctl_oc_properties_t
#[repr(C)]
#[derive(Default)]
struct OcProps {
    size: u32,
    version: u8,
    supported: bool,
    freq_offset: Control,
    volt_offset: Control,
    vram_freq_offset: Control,
    vram_volt_offset: Control,
    power_limit: Control,
    temp_limit: Control,
}
const _: () = assert!(std::mem::size_of::<OcProps>() == 296);

/// ctl_fan_speed_t
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FanSpeed {
    size: u32,
    version: u8,
    speed: i32,
    units: u32,
}
/// ctl_fan_temp_speed_t
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TempSpeed {
    size: u32,
    version: u8,
    temperature: u32,
    speed: FanSpeed,
}
/// ctl_fan_speed_table_t
#[repr(C)]
#[derive(Clone, Copy)]
struct SpeedTable {
    size: u32,
    version: u8,
    points: i32,
    table: [TempSpeed; 32],
}
/// ctl_fan_properties_t
#[repr(C)]
#[derive(Default)]
struct FanProps {
    size: u32,
    version: u8,
    can_control: bool,
    modes: u32,
    units: u32,
    max_rpm: i32,
    max_points: i32,
}
/// ctl_fan_config_t
#[repr(C)]
struct FanConfig {
    size: u32,
    version: u8,
    mode: u32,
    fixed: FanSpeed,
    table: SpeedTable,
}
const FAN_UNITS_PERCENT: u32 = 1;
const FAN_MODE_FIXED: u32 = 1;
const FAN_MODE_TABLE: u32 = 2;

/// ctl_power_telemetry_t (version 0), read by offsets: (offset of each ctl_oc_telemetry_item_t)
const TELEMETRY_SIZE: usize = 808;
const T_TIMESTAMP: usize = 8;
const T_GPU_ENERGY: usize = 32;
const T_GPU_CLOCK: usize = 80;
const T_GPU_TEMP: usize = 104;
const T_GLOBAL_ACTIVITY: usize = 128;
const T_VRAM_CLOCK: usize = 256;
const T_CARD_ENERGY: usize = 384;
const T_FAN0: usize = 688;

/// One ctl_oc_telemetry_item_t of the telemetry buffer: (supported, value as f64).
fn item(buf: &[u8], at: usize) -> Option<f64> {
    let b = buf.get(at..at + 24)?;
    if b[0] != 1 {
        return None;
    }
    let ty = u32::from_le_bytes(b[8..12].try_into().ok()?);
    let v: [u8; 8] = b[16..24].try_into().ok()?;
    let x = match ty {
        0 => v[0] as i8 as f64,
        1 => v[0] as f64,
        2 => i16::from_le_bytes([v[0], v[1]]) as f64,
        3 => u16::from_le_bytes([v[0], v[1]]) as f64,
        4 => i32::from_le_bytes([v[0], v[1], v[2], v[3]]) as f64,
        5 => u32::from_le_bytes([v[0], v[1], v[2], v[3]]) as f64,
        6 => i64::from_le_bytes(v) as f64,
        7 => u64::from_le_bytes(v) as f64,
        8 => f32::from_le_bytes([v[0], v[1], v[2], v[3]]) as f64,
        9 => f64::from_le_bytes(v),
        _ => return None,
    };
    x.is_finite().then_some(x)
}

#[allow(clippy::type_complexity)]
struct Api {
    props: unsafe extern "C" fn(H, *mut DevProps) -> u32,
    oc_props: Option<unsafe extern "C" fn(H, *mut OcProps) -> u32>,
    waiver: Option<unsafe extern "C" fn(H) -> u32>,
    get: [Option<unsafe extern "C" fn(H, *mut f64) -> u32>; 4],
    set: [Option<unsafe extern "C" fn(H, f64) -> u32>; 4],
    telemetry: Option<unsafe extern "C" fn(H, *mut u8) -> u32>,
    enum_fans: Option<unsafe extern "C" fn(H, *mut u32, *mut H) -> u32>,
    fan_props: Option<unsafe extern "C" fn(H, *mut FanProps) -> u32>,
    fan_config: Option<unsafe extern "C" fn(H, *mut FanConfig) -> u32>,
    fan_default: Option<unsafe extern "C" fn(H) -> u32>,
    fan_fixed: Option<unsafe extern "C" fn(H, *const FanSpeed) -> u32>,
    fan_table: Option<unsafe extern "C" fn(H, *const SpeedTable) -> u32>,
}

/// The four overclocking values, in the order of `Api::get` / `Api::set`.
const OC: [(Knob, &str); 4] =
    [(Knob::CoreOffset, "GpuFrequencyOffset"), (Knob::VoltOffset, "GpuVoltageOffset"), (Knob::Power, "PowerLimit"), (Knob::TempLimit, "TemperatureLimit")];

/// A previous telemetry reading: (time s, energy J, activity s).
type Prev = (f64, Option<f64>, Option<f64>);

struct IntelCard {
    card: Card,
    dev: H,
    fans: Vec<H>,
    /// Multiplier from the driver's unit to Plume Boost's (volts → mV, mW → W), per knob.
    scale: [f64; 4],
}

pub struct Intel {
    api: Api,
    cards: Vec<IntelCard>,
    /// The previous telemetry reading of each card: (time s, energy J, activity s), for power and load.
    last: RefCell<HashMap<String, Prev>>,
}

// Handles from the driver; Plume Boost calls them from one thread at a time (a mutex).
unsafe impl Send for Intel {}

fn ok(r: u32) -> bool {
    r == 0
}

impl Intel {
    pub fn load() -> Option<Intel> {
        unsafe {
            let lib = system_dll("ControlLib.dll")?;
            let init: unsafe extern "C" fn(*mut InitArgs, *mut H) -> u32 = sym(lib, "ctlInit")?;
            let enumerate: unsafe extern "C" fn(H, *mut u32, *mut H) -> u32 = sym(lib, "ctlEnumerateDevices")?;
            let pf = |n: &str| sym::<unsafe extern "C" fn(H, f64) -> u32>(lib, &format!("ctlOverclock{n}Set"));
            let gf = |n: &str| sym::<unsafe extern "C" fn(H, *mut f64) -> u32>(lib, &format!("ctlOverclock{n}Get"));
            let api = Api {
                props: sym(lib, "ctlGetDeviceProperties")?,
                oc_props: sym(lib, "ctlOverclockGetProperties"),
                waiver: sym(lib, "ctlOverclockWaiverSet"),
                get: OC.map(|(_, n)| gf(n)),
                set: OC.map(|(_, n)| pf(n)),
                telemetry: sym(lib, "ctlPowerTelemetryGet"),
                enum_fans: sym(lib, "ctlEnumFans"),
                fan_props: sym(lib, "ctlFanGetProperties"),
                fan_config: sym(lib, "ctlFanGetConfig"),
                fan_default: sym(lib, "ctlFanSetDefaultMode"),
                fan_fixed: sym(lib, "ctlFanSetFixedSpeedMode"),
                fan_table: sym(lib, "ctlFanSetSpeedTableMode"),
            };
            // API 1.1, with Level Zero (needed for telemetry and overclocking)
            let mut args = InitArgs { size: 36, version: 0, app_version: (1 << 16) | 1, flags: 1, supported_version: 0, uid: [0; 16] };
            let mut handle: H = std::ptr::null_mut();
            if !ok(init(&mut args, &mut handle)) || handle.is_null() {
                return None;
            }
            let mut n = 0u32;
            if !ok(enumerate(handle, &mut n, std::ptr::null_mut())) || n == 0 {
                return None;
            }
            let mut devs = vec![std::ptr::null_mut(); n.min(8) as usize];
            let mut n2 = devs.len() as u32;
            if !ok(enumerate(handle, &mut n2, devs.as_mut_ptr())) {
                return None;
            }
            let mut me = Intel { api, cards: Vec::new(), last: RefCell::new(HashMap::new()) };
            for d in devs.into_iter().take(n2 as usize).filter(|d| !d.is_null()) {
                if let Some(c) = me.describe(d) {
                    me.cards.push(c);
                }
            }
            Some(me)
        }
    }

    unsafe fn describe(&self, dev: H) -> Option<IntelCard> {
        unsafe {
            let mut luid = [0u8; 8];
            let mut p: DevProps = std::mem::zeroed();
            p.size = std::mem::size_of::<DevProps>() as u32;
            p.version = 2;
            p.device_id = luid.as_mut_ptr() as *mut c_void;
            p.device_id_size = 8;
            if !ok((self.api.props)(dev, &mut p)) || p.device_type != 1 {
                return None;
            }
            let name = String::from_utf8_lossy(&p.name).trim_end_matches('\0').trim().to_string();
            let name = if name.is_empty() { "Intel Graphics".to_string() } else { name };
            let integrated = p.adapter_flags & 1 == 1;
            let mut caps = Caps::default();
            let mut scale = [1.0; 4];
            let mut fans = Vec::new();
            if !integrated && let Some(get) = self.api.oc_props {
                let mut oc = OcProps { size: 296, ..Default::default() };
                if ok(get(dev, &mut oc)) && oc.supported {
                    let controls = [oc.freq_offset, oc.volt_offset, oc.power_limit, oc.temp_limit];
                    for (i, ((knob, _), c)) in OC.iter().zip(controls).enumerate() {
                        if !c.supported || self.api.set[i].is_none() {
                            continue;
                        }
                        // units as the driver gives them: volts become mV, mW become W
                        let k = match knob {
                            Knob::VoltOffset if c.max.abs() < 5.0 => 1000.0,
                            Knob::Power if c.max > 5000.0 => 0.001,
                            _ => 1.0,
                        };
                        scale[i] = k;
                        let r = Range::new(c.min * k, c.max * k, (c.step * k).max(1.0), c.default * k);
                        if r.sane() {
                            caps.knobs.push((*knob, r));
                        }
                    }
                }
                fans = self.fans(dev);
                if let (Some(first), Some(fp)) = (fans.first(), self.api.fan_props) {
                    let mut props = FanProps { size: std::mem::size_of::<FanProps>() as u32, ..Default::default() };
                    if ok(fp(*first, &mut props)) && props.can_control {
                        if self.api.fan_fixed.is_some() {
                            caps.fan_fixed = Some(Range::new(0.0, 100.0, 1.0, 0.0));
                        }
                        if self.api.fan_table.is_some() && props.max_points >= 2 {
                            let r = Range::new;
                            caps.fan_curve = Some(CurveCaps {
                                temp: r(25.0, 100.0, 1.0, 50.0),
                                speed: r(0.0, 100.0, 1.0, 30.0),
                                points: (props.max_points as usize).min(5),
                            });
                        }
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
            let needs_waiver = caps.tunable() && self.api.waiver.is_some();
            Some(IntelCard {
                card: Card {
                    id: format!("intel-{}-{}", p.bdf[0], slug(&name)),
                    name,
                    vendor: Vendor::Intel,
                    integrated,
                    caps,
                    api: "IGCL".into(),
                    why_not,
                    needs_waiver,
                },
                dev,
                fans,
                scale,
            })
        }
    }

    unsafe fn fans(&self, dev: H) -> Vec<H> {
        let Some(en) = self.api.enum_fans else { return Vec::new() };
        unsafe {
            let mut n = 0u32;
            if !ok(en(dev, &mut n, std::ptr::null_mut())) || n == 0 {
                return Vec::new();
            }
            let mut v = vec![std::ptr::null_mut(); n.min(5) as usize];
            let mut n2 = v.len() as u32;
            if !ok(en(dev, &mut n2, v.as_mut_ptr())) {
                return Vec::new();
            }
            v.truncate(n2 as usize);
            v
        }
    }

    fn find(&self, id: &str) -> Option<&IntelCard> {
        self.cards.iter().find(|c| c.card.id == id)
    }
}

fn igcl(r: u32) -> Res {
    match r {
        0 => Ok(()),
        // CTL_RESULT_ERROR_INSUFFICIENT_PERMISSIONS
        0x4000_0003 => Err(Fail::NeedsAdmin),
        e => Err(Fail::Other(format!("Intel graphics error 0x{e:x}"))),
    }
}

impl Driver for Intel {
    fn cards(&self) -> Vec<Card> {
        self.cards.iter().map(|c| c.card.clone()).collect()
    }

    fn telemetry(&self, card: &str) -> Option<Telemetry> {
        let c = self.find(card)?;
        let get = self.api.telemetry?;
        let mut buf = vec![0u8; 4096];
        buf[0..4].copy_from_slice(&(TELEMETRY_SIZE as u32).to_le_bytes());
        if !ok(unsafe { get(c.dev, buf.as_mut_ptr()) }) {
            return None;
        }
        let ts = item(&buf, T_TIMESTAMP);
        let energy = item(&buf, T_GPU_ENERGY).or_else(|| item(&buf, T_CARD_ENERGY));
        let activity = item(&buf, T_GLOBAL_ACTIVITY);
        let mut t = Telemetry {
            core_mhz: item(&buf, T_GPU_CLOCK).filter(|v| *v > 0.0 && *v < 10_000.0),
            mem_mhz: item(&buf, T_VRAM_CLOCK).filter(|v| *v > 0.0 && *v < 30_000.0),
            temp_c: item(&buf, T_GPU_TEMP).filter(|v| *v > 0.0 && *v < 150.0),
            fan_rpm: item(&buf, T_FAN0).filter(|v| *v >= 0.0 && *v < 10_000.0),
            ..Default::default()
        };
        if let Some(now) = ts {
            let mut last = self.last.borrow_mut();
            if let Some((t0, e0, a0)) = last.get(card).copied()
                && now > t0
            {
                let dt = now - t0;
                t.power_w = energy.zip(e0).map(|(e, e0)| (e - e0) / dt).filter(|w| *w >= 0.0 && *w < 1000.0);
                t.usage_pct = activity.zip(a0).map(|(a, a0)| ((a - a0) / dt * 100.0).clamp(0.0, 100.0));
            }
            last.insert(card.to_string(), (now, energy, activity));
        }
        Some(t)
    }

    fn get(&self, card: &str) -> Option<Tuning> {
        let c = self.find(card)?;
        let mut t = Tuning::default();
        for (i, (knob, _)) in OC.iter().enumerate() {
            if c.card.caps.range(*knob).is_none() {
                continue;
            }
            let mut v = 0f64;
            if let Some(g) = self.api.get[i]
                && ok(unsafe { g(c.dev, &mut v) })
            {
                t.knobs.insert(*knob, v * c.scale[i]);
            }
        }
        if let (Some(first), Some(gc)) = (c.fans.first(), self.api.fan_config) {
            let mut cfg: FanConfig = unsafe { std::mem::zeroed() };
            cfg.size = std::mem::size_of::<FanConfig>() as u32;
            if ok(unsafe { gc(*first, &mut cfg) }) {
                t.fan = match cfg.mode {
                    FAN_MODE_FIXED => Fan::Fixed(cfg.fixed.speed.clamp(0, 100) as u32),
                    FAN_MODE_TABLE => Fan::Curve(
                        cfg.table.table[..(cfg.table.points.clamp(0, 32) as usize)]
                            .iter()
                            .map(|p| (p.temperature, p.speed.speed.clamp(0, 100) as u32))
                            .collect(),
                    ),
                    _ => Fan::Auto,
                };
            }
        }
        Some(t)
    }

    fn set(&self, card: &str, t: &Tuning) -> Res {
        let c = self.find(card).ok_or(Fail::Missing)?;
        let now = self.get(card).unwrap_or_default();
        for (i, (knob, _)) in OC.iter().enumerate() {
            if let (Some(v), Some(r), Some(set)) = (t.get(*knob), c.card.caps.range(*knob), self.api.set[i])
                && now.get(*knob) != Some(v)
            {
                igcl(unsafe { set(c.dev, r.clamp(v) / c.scale[i]) })?;
            }
        }
        if t.fan != now.fan {
            for fan in &c.fans {
                let r = unsafe {
                    match &t.fan {
                        Fan::Auto => self.api.fan_default.map(|f| f(*fan)),
                        Fan::Fixed(p) => {
                            let s =
                                FanSpeed { size: std::mem::size_of::<FanSpeed>() as u32, version: 0, speed: (*p).min(100) as i32, units: FAN_UNITS_PERCENT };
                            self.api.fan_fixed.map(|f| f(*fan, &s))
                        }
                        Fan::Curve(pts) => {
                            let mut tab = SpeedTable {
                                size: std::mem::size_of::<SpeedTable>() as u32,
                                version: 0,
                                points: pts.len().min(32) as i32,
                                table: [TempSpeed::default(); 32],
                            };
                            for (k, (tc, sp)) in pts.iter().take(32).enumerate() {
                                tab.table[k] = TempSpeed {
                                    size: std::mem::size_of::<TempSpeed>() as u32,
                                    version: 0,
                                    temperature: *tc,
                                    speed: FanSpeed {
                                        size: std::mem::size_of::<FanSpeed>() as u32,
                                        version: 0,
                                        speed: (*sp).min(100) as i32,
                                        units: FAN_UNITS_PERCENT,
                                    },
                                };
                            }
                            self.api.fan_table.map(|f| f(*fan, &tab))
                        }
                    }
                };
                igcl(r.ok_or(Fail::Missing)?)?;
            }
        }
        Ok(())
    }

    fn waiver(&self, card: &str) -> Res {
        let c = self.find(card).ok_or(Fail::Missing)?;
        igcl(unsafe { self.api.waiver.ok_or(Fail::Missing)?(c.dev) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_items_are_read_by_type() {
        let mut buf = vec![0u8; TELEMETRY_SIZE];
        // a supported double (type 9) at the clock's place, an unsupported item at the temperature's
        buf[T_GPU_CLOCK] = 1;
        buf[T_GPU_CLOCK + 8..T_GPU_CLOCK + 12].copy_from_slice(&9u32.to_le_bytes());
        buf[T_GPU_CLOCK + 16..T_GPU_CLOCK + 24].copy_from_slice(&1300.0f64.to_le_bytes());
        assert_eq!(item(&buf, T_GPU_CLOCK), Some(1300.0));
        assert_eq!(item(&buf, T_GPU_TEMP), None);
        // a u32 (type 5)
        buf[T_GPU_TEMP] = 1;
        buf[T_GPU_TEMP + 8..T_GPU_TEMP + 12].copy_from_slice(&5u32.to_le_bytes());
        buf[T_GPU_TEMP + 16..T_GPU_TEMP + 20].copy_from_slice(&61u32.to_le_bytes());
        assert_eq!(item(&buf, T_GPU_TEMP), Some(61.0));
        assert_eq!(std::mem::size_of::<SpeedTable>(), 908);
        assert_eq!(std::mem::size_of::<FanConfig>(), 936);
    }

    /// Reads (never writes) what Intel's library says about this PC's graphics: run by hand with
    /// `cargo test -p plume-boost probe_ -- --ignored --nocapture` to check the structures on real hardware.
    #[test]
    #[ignore]
    fn probe_this_pc_read_only() {
        let Some(i) = Intel::load() else {
            println!("ControlLib.dll: not loaded");
            return;
        };
        for c in i.cards() {
            println!("card {} '{}' integrated={} caps={:?} why_not={:?}", c.id, c.name, c.integrated, c.caps, c.why_not);
            for _ in 0..3 {
                println!("  telemetry {:?}", i.telemetry(&c.id));
                std::thread::sleep(std::time::Duration::from_millis(700));
            }
        }
    }
}
