//! Graphics card tuning: what a card can change (the ranges its driver reports), what it is set to, what it reads
//! now, saved profiles, and the crash guard.
//!
//! Each maker has its own driver interface, loaded at run time from System32 (never from the current folder) with
//! LoadLibrary / GetProcAddress, so Plume Boost needs no SDK and starts on any PC:
//! - NVIDIA: NVAPI (`nvapi64.dll`, clock offsets through the P-state 2.0 interface) and NVML (`nvml.dll`, the
//!   documented management library: readings, power limit, fan speed). See `nvidia.rs`.
//! - AMD: ADLX (`amdadlx64.dll`, the public C interface of AMD Software). See `amd.rs`.
//! - Intel: the Intel Graphics Control Library (`ControlLib.dll`). Arc cards can be tuned; integrated graphics
//!   (Iris Xe, UHD) only give readings. See `intel.rs`.
//!
//! A value is never sent outside the range the driver reports (`Caps`), and every change goes through the engine,
//! which saves what was there before so it can be put back.

pub mod amd;
pub mod intel;
pub mod nvidia;
pub mod stress;

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
}

impl Vendor {
    pub fn name(self) -> &'static str {
        match self {
            Vendor::Nvidia => "NVIDIA",
            Vendor::Amd => "AMD",
            Vendor::Intel => "Intel",
        }
    }
}

/// One thing a card lets you change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Knob {
    /// Added to every graphics clock of the card's curve, MHz (NVIDIA, Intel Arc).
    CoreOffset,
    /// Added to the memory clock, MHz (NVIDIA).
    MemOffset,
    /// Highest graphics clock the card may reach, MHz (AMD).
    CoreMax,
    /// Memory clock, MHz (AMD).
    MemMax,
    /// Highest graphics voltage, mV (AMD: lower = undervolt).
    Voltage,
    /// Added to the graphics voltage, mV (Intel Arc).
    VoltOffset,
    /// Power limit: watts (NVIDIA, Intel) or % around the stock limit (AMD).
    Power,
    /// Temperature the card holds itself under, °C (Intel Arc).
    TempLimit,
}

#[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
impl Knob {
    pub const ALL: [Knob; 8] = [Knob::CoreOffset, Knob::CoreMax, Knob::MemOffset, Knob::MemMax, Knob::Voltage, Knob::VoltOffset, Knob::Power, Knob::TempLimit];
    pub fn key(self) -> &'static str {
        match self {
            Knob::CoreOffset => "core_offset",
            Knob::MemOffset => "mem_offset",
            Knob::CoreMax => "core_max",
            Knob::MemMax => "mem_max",
            Knob::Voltage => "voltage",
            Knob::VoltOffset => "volt_offset",
            Knob::Power => "power",
            Knob::TempLimit => "temp_limit",
        }
    }
    pub fn from_key(k: &str) -> Option<Knob> {
        Knob::ALL.into_iter().find(|n| n.key() == k)
    }
}

/// What the driver allows for a knob.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// The card's own value (stock).
    pub default: f64,
}

impl Range {
    pub fn new(min: f64, max: f64, step: f64, default: f64) -> Range {
        Range { min, max, step: if step > 0.0 { step } else { 1.0 }, default: default.clamp(min, max) }
    }
    /// A range the driver gave that can be trusted: finite, min below max, the stock value inside.
    pub fn sane(&self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.min < self.max && self.default >= self.min && self.default <= self.max
    }
    /// `v` brought inside the range and onto its steps.
    pub fn clamp(&self, v: f64) -> f64 {
        let v = v.clamp(self.min, self.max);
        let k = ((v - self.min) / self.step).round();
        (self.min + k * self.step).clamp(self.min, self.max)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Fan {
    /// The card's own automatic curve.
    #[default]
    Auto,
    /// A fixed speed, %.
    Fixed(u32),
    /// A curve: (°C, %) points, lowest temperature first. Kept by the driver itself (AMD, Intel Arc).
    Curve(Vec<(u32, u32)>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurveCaps {
    pub temp: Range,
    pub speed: Range,
    pub points: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Caps {
    pub knobs: Vec<(Knob, Range)>,
    /// A fixed speed, % (min..max the driver allows).
    pub fan_fixed: Option<Range>,
    pub fan_curve: Option<CurveCaps>,
    /// The power limit is in % around the stock value (AMD) rather than watts.
    pub power_percent: bool,
}

impl Caps {
    pub fn range(&self, k: Knob) -> Option<Range> {
        self.knobs.iter().find(|(n, _)| *n == k).map(|(_, r)| *r)
    }
    pub fn tunable(&self) -> bool {
        !self.knobs.is_empty() || self.fan_fixed.is_some() || self.fan_curve.is_some()
    }
    /// The card's stock settings.
    pub fn stock(&self) -> Tuning {
        Tuning { knobs: self.knobs.iter().map(|(k, r)| (*k, r.default)).collect(), fan: Fan::Auto }
    }
    /// `t` with every value inside what the driver allows (knobs it doesn't have are dropped).
    pub fn clamp(&self, t: &Tuning) -> Tuning {
        let knobs = t.knobs.iter().filter_map(|(k, v)| self.range(*k).map(|r| (*k, r.clamp(*v)))).collect();
        let fan = match &t.fan {
            Fan::Auto => Fan::Auto,
            Fan::Fixed(p) => match &self.fan_fixed {
                Some(r) => Fan::Fixed(r.clamp(*p as f64) as u32),
                None => Fan::Auto,
            },
            Fan::Curve(pts) => match &self.fan_curve {
                Some(c) => {
                    let mut v: Vec<(u32, u32)> =
                        pts.iter().take(c.points).map(|(t, s)| (c.temp.clamp(*t as f64) as u32, c.speed.clamp(*s as f64) as u32)).collect();
                    v.sort();
                    // a fan curve never goes down when it gets hotter
                    for i in 1..v.len() {
                        v[i].1 = v[i].1.max(v[i - 1].1);
                    }
                    Fan::Curve(v)
                }
                None => Fan::Auto,
            },
        };
        Tuning { knobs, fan }
    }
}

/// What a card is set to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tuning {
    pub knobs: BTreeMap<Knob, f64>,
    pub fan: Fan,
}

impl Tuning {
    pub fn get(&self, k: Knob) -> Option<f64> {
        self.knobs.get(&k).copied()
    }
    /// A short line for the Changes page.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = self.knobs.iter().map(|(k, v)| format!("{} {}", knob_short(*k), show(*k, *v, false))).collect();
        parts.push(match &self.fan {
            Fan::Auto => format!("{} auto", crate::i18n::tf("fan", "ventilateur")),
            Fan::Fixed(p) => format!("{} {p} %", crate::i18n::tf("fan", "ventilateur")),
            Fan::Curve(v) => format!("{} {} pts", crate::i18n::tf("fan curve", "courbe ventilateur"), v.len()),
        });
        parts.join(" · ")
    }
}

pub fn knob_short(k: Knob) -> &'static str {
    use crate::i18n::tf;
    match k {
        Knob::CoreOffset => tf("core", "cœur"),
        Knob::MemOffset => tf("memory", "mémoire"),
        Knob::CoreMax => tf("core max", "cœur max"),
        Knob::MemMax => tf("memory", "mémoire"),
        Knob::Voltage => tf("voltage", "tension"),
        Knob::VoltOffset => tf("voltage", "tension"),
        Knob::Power => tf("power", "puissance"),
        Knob::TempLimit => tf("temp. limit", "limite temp."),
    }
}

/// A knob's value with its unit ("+150 MHz", "220 W", "+5 %", "1050 mV").
pub fn show(k: Knob, v: f64, power_percent: bool) -> String {
    match k {
        Knob::CoreOffset | Knob::MemOffset => format!("{:+.0} MHz", v),
        Knob::CoreMax | Knob::MemMax => format!("{v:.0} MHz"),
        Knob::Voltage => format!("{v:.0} mV"),
        Knob::VoltOffset => format!("{v:+.0} mV"),
        Knob::Power if power_percent => format!("{v:+.0} %"),
        Knob::Power => format!("{v:.0} W"),
        Knob::TempLimit => format!("{v:.0} °C"),
    }
}

/// A graphics card and what Plume Boost can do with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Card {
    /// Stable name: maker, PCI bus and model ("nvidia-1-RTX 4070"), used by profiles and the undo log.
    pub id: String,
    pub name: String,
    pub vendor: Vendor,
    pub integrated: bool,
    pub caps: Caps,
    /// The driver interface used ("NVAPI + NVML", "ADLX", "IGCL", "imaginary").
    pub api: String,
    /// Why it can't be tuned (integrated graphics, driver too old…); None when it can.
    pub why_not: Option<String>,
    /// Intel asks the user to accept its overclocking waiver once before anything is changed.
    pub needs_waiver: bool,
}

/// What the card reads now. None: the driver doesn't say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
    pub core_mhz: Option<f64>,
    pub mem_mhz: Option<f64>,
    pub temp_c: Option<f64>,
    pub hotspot_c: Option<f64>,
    pub fan_pct: Option<f64>,
    pub fan_rpm: Option<f64>,
    pub power_w: Option<f64>,
    pub power_limit_w: Option<f64>,
    pub usage_pct: Option<f64>,
}

/// How a stability test went.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StressResult {
    pub secs: f64,
    /// The driver reset the card (TDR) or Direct3D lost it.
    pub device_removed: bool,
    /// Results the card computed wrongly (checked against the processor's own answer).
    pub errors: u64,
    pub max_temp: Option<f64>,
    pub max_clock: Option<f64>,
    /// The lowest clock in the last part of the test, under load.
    pub min_clock_late: Option<f64>,
    pub max_power: Option<f64>,
    /// Stopped by the user.
    pub stopped: bool,
    /// Couldn't start (no Direct3D 11 device…).
    pub failed_to_start: Option<String>,
}

impl StressResult {
    /// Passed: no reset, no wrong result, not stopped.
    pub fn passed(&self) -> bool {
        !self.device_removed && self.errors == 0 && !self.stopped && self.failed_to_start.is_none()
    }
    /// The clock fell by more than a quarter under load (heat or power throttling).
    pub fn throttled(&self) -> bool {
        matches!((self.max_clock, self.min_clock_late), (Some(a), Some(b)) if a > 0.0 && b < a * 0.75)
    }
}

/// Progress of a stability test: (seconds done, total, the latest readings).
pub type StressProgress<'a> = &'a mut dyn FnMut(f64, f64, Option<Telemetry>);

// ───────────────────────── profiles ─────────────────────────

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// Card id → its settings.
    pub cards: BTreeMap<String, Tuning>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profiles {
    pub list: Vec<Profile>,
    /// The profile applied when Windows starts (by name), if any.
    pub signin: Option<String>,
}

impl Profiles {
    fn file() -> PathBuf {
        crate::settings::data_dir().join("gpu_profiles.json")
    }
    pub fn load() -> Profiles {
        std::fs::read(Self::file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        crate::settings::save_json(&Self::file(), self);
    }
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.list.iter().find(|p| p.name == name)
    }
    /// A free name: "Profile 1", "Profile 2"…
    pub fn next_name(&self) -> String {
        let base = crate::i18n::tf("Profile", "Profil");
        (1..).map(|n| format!("{base} {n}")).find(|n| self.get(n).is_none()).unwrap_or_default()
    }
}

// ───────────────────────── the crash guard ─────────────────────────

/// How long after a change a crash, an unexpected restart or a driver reset is blamed on it.
pub const GUARD_SECS: u64 = 10 * 60;

/// Written when a card's settings change, removed once they have run for GUARD_SECS without trouble.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Guard {
    pub card: String,
    pub changed_at: u64,
    /// What the card had before: put back if anything goes wrong.
    pub previous: Tuning,
    /// Windows' start time then (seconds since 1970): another one means the PC restarted.
    pub boot: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// No guard, or still inside its minutes with nothing wrong.
    Wait,
    /// The minutes passed without trouble (or the PC was shut down cleanly): the guard is removed.
    Stable,
    /// Put the previous settings back: the driver reset, or the PC restarted without a clean shutdown.
    Revert(Reason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reason {
    DriverReset,
    UncleanRestart,
}

impl Reason {
    pub fn text(self) -> &'static str {
        use crate::i18n::tf;
        match self {
            Reason::DriverReset => tf(
                "The graphics driver stopped responding and was reset by Windows shortly after the change.",
                "Le pilote graphique a cessé de répondre et Windows l'a réinitialisé peu après le changement.",
            ),
            Reason::UncleanRestart => tf(
                "The PC restarted without shutting down properly shortly after the change.",
                "Le PC a redémarré sans s'arrêter correctement peu après le changement.",
            ),
        }
    }
}

impl Guard {
    fn file() -> PathBuf {
        crate::settings::data_dir().join("gpu_guard.json")
    }
    pub fn load() -> Option<Guard> {
        std::fs::read(Self::file()).ok().and_then(|b| serde_json::from_slice(&b).ok())
    }
    pub fn save(&self) {
        crate::settings::save_json(&Self::file(), self);
    }
    pub fn clear() {
        let _ = std::fs::remove_file(Self::file());
    }

    /// What to do with this guard now. `boot`: Windows' start time; `clean_shutdown`: when Windows last shut down
    /// properly; `resets`: driver resets since the change.
    pub fn verdict(&self, now: u64, boot: u64, clean_shutdown: u64, resets: u32) -> Verdict {
        if resets > 0 {
            return Verdict::Revert(Reason::DriverReset);
        }
        // a start of Windows since the change (a minute of tolerance: the boot time is computed)
        if boot > self.boot + 60 {
            return if clean_shutdown >= self.changed_at { Verdict::Stable } else { Verdict::Revert(Reason::UncleanRestart) };
        }
        if now >= self.changed_at + GUARD_SECS { Verdict::Stable } else { Verdict::Wait }
    }
}

// ───────────────────────── loading the drivers' libraries ─────────────────────────

/// A driver's interface to the cards of one maker (real PC only).
pub trait Driver: Send {
    fn cards(&self) -> Vec<Card>;
    fn telemetry(&self, card: &str) -> Option<Telemetry>;
    fn get(&self, card: &str) -> Option<Tuning>;
    fn set(&self, card: &str, t: &Tuning) -> crate::backend::Res;
    fn waiver(&self, _card: &str) -> crate::backend::Res {
        Ok(())
    }
    /// Back to the card's stock settings.
    fn reset(&self, card: &str) -> crate::backend::Res {
        let stock = self.cards().into_iter().find(|c| c.id == card).map(|c| c.caps.stock()).ok_or(crate::backend::Fail::Missing)?;
        self.set(card, &stock)
    }
}

/// A library of System32, loaded by its full path (never from the current folder or the PATH).
pub fn system_dll(name: &str) -> Option<windows::Win32::Foundation::HMODULE> {
    use windows::Win32::System::{LibraryLoader::LoadLibraryW, SystemInformation::GetSystemDirectoryW};
    let mut buf = [0u16; 260];
    let n = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if n == 0 {
        return None;
    }
    let path = format!("{}\\{name}", String::from_utf16_lossy(&buf[..n]));
    if !std::path::Path::new(&path).is_file() {
        return None;
    }
    unsafe { LoadLibraryW(&windows::core::HSTRING::from(path)).ok() }
}

/// A function of a loaded library, as the function type `T` the caller declares.
///
/// # Safety
/// `T` must be the function's real signature.
pub unsafe fn sym<T: Copy>(lib: windows::Win32::Foundation::HMODULE, name: &str) -> Option<T> {
    let cname = std::ffi::CString::new(name).ok()?;
    let f = unsafe { windows::Win32::System::LibraryLoader::GetProcAddress(lib, windows::core::PCSTR(cname.as_ptr() as *const u8)) }?;
    debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of_val(&f));
    Some(unsafe { std::mem::transmute_copy(&f) })
}

/// The larger of two readings, either of which may be missing.
pub fn max_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

/// A short, file-friendly form of a model name for card ids ("GeForce RTX 4070" → "geforcertx4070").
pub fn slug(name: &str) -> String {
    name.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).take(40).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> Caps {
        Caps {
            knobs: vec![(Knob::CoreOffset, Range::new(-500.0, 1000.0, 1.0, 0.0)), (Knob::Power, Range::new(100.0, 220.0, 1.0, 200.0))],
            fan_fixed: Some(Range::new(30.0, 100.0, 1.0, 30.0)),
            fan_curve: Some(CurveCaps { temp: Range::new(25.0, 95.0, 1.0, 50.0), speed: Range::new(0.0, 100.0, 1.0, 30.0), points: 5 }),
            power_percent: false,
        }
    }

    #[test]
    fn values_stay_inside_what_the_driver_allows() {
        let c = caps();
        let mut t = Tuning::default();
        t.knobs.insert(Knob::CoreOffset, 5000.0);
        t.knobs.insert(Knob::Power, 12.3);
        t.knobs.insert(Knob::MemMax, 3000.0);
        t.fan = Fan::Fixed(5);
        let k = c.clamp(&t);
        assert_eq!(k.get(Knob::CoreOffset), Some(1000.0));
        assert_eq!(k.get(Knob::Power), Some(100.0));
        assert_eq!(k.get(Knob::MemMax), None, "a knob the card doesn't have is dropped");
        assert_eq!(k.fan, Fan::Fixed(30));
        // a curve: sorted, inside the ranges, never lower when hotter, no more points than the driver takes
        let t = Tuning { fan: Fan::Curve(vec![(90, 50), (20, 10), (60, 70), (70, 40), (99, 120), (80, 80)]), ..Default::default() };
        assert_eq!(c.clamp(&t).fan, Fan::Curve(vec![(25, 10), (60, 70), (70, 70), (90, 70), (95, 100)]));
        assert_eq!(Range::new(0.0, 100.0, 5.0, 50.0).clamp(42.0), 40.0);
        assert!(!Range { min: 5.0, max: 1.0, step: 1.0, default: 2.0 }.sane());
    }

    #[test]
    fn stock_settings() {
        let s = caps().stock();
        assert_eq!((s.get(Knob::CoreOffset), s.get(Knob::Power), s.fan), (Some(0.0), Some(200.0), Fan::Auto));
    }

    #[test]
    fn the_guard_blames_only_trouble_after_the_change() {
        let g = Guard { card: "c".into(), changed_at: 10_000, previous: Tuning::default(), boot: 5_000 };
        assert_eq!(g.verdict(10_100, 5_000, 4_000, 0), Verdict::Wait, "same boot, minutes not over");
        assert_eq!(g.verdict(10_000 + GUARD_SECS, 5_000, 4_000, 0), Verdict::Stable);
        assert_eq!(g.verdict(10_100, 5_000, 4_000, 1), Verdict::Revert(Reason::DriverReset));
        // restarted after the change: clean shutdown after the change = fine, otherwise a crash
        assert_eq!(g.verdict(10_300, 10_200, 10_150, 0), Verdict::Stable);
        assert_eq!(g.verdict(10_300, 10_200, 4_000, 0), Verdict::Revert(Reason::UncleanRestart));
    }

    #[test]
    fn stress_verdicts() {
        let ok = StressResult { secs: 60.0, max_clock: Some(2700.0), min_clock_late: Some(2600.0), ..Default::default() };
        assert!(ok.passed() && !ok.throttled());
        assert!(!StressResult { device_removed: true, ..ok.clone() }.passed());
        assert!(!StressResult { errors: 3, ..ok.clone() }.passed());
        assert!(StressResult { min_clock_late: Some(1500.0), ..ok }.throttled());
    }
}
