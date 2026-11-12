//! Everything Plume Boost reads from or changes in Windows goes through one [`Backend`]: the registry, the power
//! plans and their processor settings, the start type of two services, and System Restore.
//!
//! Two backends exist:
//! - [`real::Real`], the PC itself. In a `test-hook` build it can still read, but every write is refused.
//! - [`fake::Fake`], an imaginary PC kept in a JSON file (PLUME_BOOST_FAKE). Unit tests, the test socket and every
//!   test build use it, so developing Plume Boost never changes the PC it runs on.

pub mod fake;
pub mod real;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The two registry roots Plume Boost touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hive {
    /// HKEY_CURRENT_USER: this account only, no administrator rights needed.
    User,
    /// HKEY_LOCAL_MACHINE: the whole PC, administrator rights needed to write.
    Machine,
}

impl Hive {
    pub fn short(self) -> &'static str {
        match self {
            Hive::User => "HKCU",
            Hive::Machine => "HKLM",
        }
    }
}

/// A registry value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Val {
    Dword(u32),
    Sz(String),
    Bin(Vec<u8>),
}

impl Val {
    pub fn show(&self) -> String {
        match self {
            Val::Dword(d) => d.to_string(),
            Val::Sz(s) => format!("\"{s}\""),
            Val::Bin(b) => b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" "),
        }
    }
}

/// A power plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Lowercase GUID, without braces.
    pub guid: String,
    pub name: String,
}

/// Why a change did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fail {
    /// Windows refused: the change needs administrator rights.
    NeedsAdmin,
    /// The PC doesn't have this (a power plan Windows hides on this model, no battery…).
    Missing,
    /// Anything else, with Windows' words or code.
    Other(String),
}

impl Fail {
    pub fn text(&self) -> String {
        match self {
            Fail::NeedsAdmin => crate::i18n::tr("Windows asks for administrator rights for this.").to_string(),
            Fail::Missing => crate::i18n::tr("This PC doesn't have it.").to_string(),
            Fail::Other(s) => s.clone(),
        }
    }
}

pub type Res<T = ()> = Result<T, Fail>;

/// Well-known GUIDs of the power settings (the same on every Windows 10 and 11).
pub mod guids {
    pub const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
    pub const HIGH: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
    /// The hidden template of "Ultimate Performance": copied to make a plan the user can choose.
    pub const ULTIMATE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";
    pub const POWER_SAVER: &str = "a1841308-3541-4fab-bc81-f71556f20b4a";
    /// Processor power management (SUB_PROCESSOR).
    pub const SUB_PROCESSOR: &str = "54533251-82be-4824-96c1-47b60b740d00";
    /// Minimum processor state, % (PROCTHROTTLEMIN).
    pub const PROC_MIN: &str = "893dee8e-2bef-41e0-89c6-b55d0929964c";
    /// Maximum processor state, % (PROCTHROTTLEMAX).
    pub const PROC_MAX: &str = "bc5038f7-23e0-4960-96da-33abaf5935ec";
    /// Processor performance boost mode (PERFBOOSTMODE): 0 off, 1 on, 2 aggressive, 3 efficient, 4 efficient aggressive.
    pub const BOOST_MODE: &str = "be337238-0d82-4146-a960-4f3749d470c7";
    /// Processor energy performance preference, 0 (performance) to 100 (energy saving) (PERFEPP). Used by
    /// processors that pick their own clocks (Intel Speed Shift / HWP, AMD CPPC).
    pub const EPP: &str = "36687f9e-e3a5-4dbf-b1dc-15eb381c6863";
    /// Processor performance core parking min cores, % of cores kept unparked (CPMINCORES).
    pub const PARK_MIN: &str = "0cc5b647-c1df-4637-891a-dec35c318583";
}

/// A registry value as Windows stores it: its type (REG_SZ 1, REG_EXPAND_SZ 2, REG_BINARY 3, REG_DWORD 4,
/// REG_MULTI_SZ 7, REG_QWORD 11…) and its bytes. Used by the snapshots (.reg export and import).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Raw {
    pub name: String,
    pub ty: u32,
    pub data: Vec<u8>,
}

impl Raw {
    pub fn from_val(name: &str, v: &Val) -> Raw {
        let (ty, data) = match v {
            Val::Dword(d) => (4, d.to_le_bytes().to_vec()),
            Val::Sz(s) => (1, s.encode_utf16().chain([0]).flat_map(|c| c.to_le_bytes()).collect()),
            Val::Bin(b) => (3, b.clone()),
        };
        Raw { name: name.into(), ty, data }
    }
    /// The value as Plume Boost's own types, when it is one of them.
    pub fn to_val(&self) -> Option<Val> {
        match self.ty {
            4 if self.data.len() >= 4 => Some(Val::Dword(u32::from_le_bytes([self.data[0], self.data[1], self.data[2], self.data[3]]))),
            1 | 2 => {
                let w: Vec<u16> = self.data.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
                Some(Val::Sz(String::from_utf16_lossy(&w)))
            }
            3 => Some(Val::Bin(self.data.clone())),
            _ => None,
        }
    }
}

/// The Run value that applies a graphics profile when Windows starts.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_NAME: &str = "PlumeBoostGpu";

/// Access to the PC (or to the imaginary one).
pub trait Backend: Send + Sync {
    /// True for the imaginary PC of the tests.
    fn is_test(&self) -> bool;
    /// Plume Boost runs with administrator rights.
    fn is_admin(&self) -> bool;
    /// The PC has a battery (a laptop).
    fn has_battery(&self) -> bool;

    fn reg_get(&self, hive: Hive, path: &str, name: &str) -> Option<Val>;
    fn reg_set(&self, hive: Hive, path: &str, name: &str, v: &Val) -> Res;
    /// Removes a value (absent already counts as done).
    fn reg_del(&self, hive: Hive, path: &str, name: &str) -> Res;

    /// The plans the user can choose.
    fn plans(&self) -> Vec<Plan>;
    fn active_plan(&self) -> Option<String>;
    fn set_active_plan(&self, guid: &str) -> Res;
    /// The name of a plan or of a hidden template (None: unknown on this PC).
    fn plan_name(&self, guid: &str) -> Option<String>;
    /// Copies a plan or a hidden template into a new plan; returns the new plan's GUID.
    fn duplicate_plan(&self, source: &str) -> Res<String>;
    fn delete_plan(&self, guid: &str) -> Res;
    /// A processor setting of a plan, plugged in (`ac`) or on battery.
    fn power_get(&self, plan: &str, sub: &str, setting: &str, ac: bool) -> Option<u32>;
    /// Writes a setting; when `plan` is the active plan Windows uses the new value at once.
    fn power_set(&self, plan: &str, sub: &str, setting: &str, ac: bool, v: u32) -> Res;

    /// A service's start type: 2 automatic, 3 manual, 4 disabled.
    fn service_start(&self, name: &str) -> Option<u32>;
    fn set_service_start(&self, name: &str, start: u32) -> Res;

    /// A System Restore point named `desc`.
    fn restore_point(&self, desc: &str) -> Res;

    // ── snapshots ────────────────────────────────────────────────────────────────────────────────────────────────

    /// Every value of a registry key (not its subkeys); None when the key doesn't exist.
    fn reg_values(&self, hive: Hive, path: &str) -> Option<Vec<Raw>>;
    /// Writes a value of any type (a snapshot being restored).
    fn reg_set_raw(&self, hive: Hive, path: &str, v: &Raw) -> Res;
    /// `powercfg /export <file> <plan>`: the whole plan in Windows' own format.
    fn power_export(&self, plan: &str, file: &std::path::Path) -> Res;
    /// `powercfg /import <file> <plan>`: brings back a plan that was deleted.
    fn power_import(&self, plan: &str, file: &std::path::Path) -> Res;
    /// A power setting's attributes (1: hidden from Windows' advanced power options).
    fn power_attrib(&self, sub: &str, setting: &str) -> Option<u32>;
    /// Shows (0) or hides (1) a power setting in Windows' advanced power options. Machine-wide: administrator.
    fn set_power_attrib(&self, sub: &str, setting: &str, attrib: u32) -> Res;

    // ── graphics cards ───────────────────────────────────────────────────────────────────────────────────────────

    /// The graphics cards and what can be changed on each (read once, then kept).
    fn gpus(&self) -> Vec<crate::gpu::Card>;
    fn gpu_telemetry(&self, card: &str) -> Option<crate::gpu::Telemetry>;
    /// What the card is set to now.
    fn gpu_get(&self, card: &str) -> Option<crate::gpu::Tuning>;
    /// Sets the card (values already inside its ranges: the engine clamps them).
    fn gpu_set(&self, card: &str, t: &crate::gpu::Tuning) -> Res;
    /// Back to the card's stock settings (AMD: the driver's own reset to factory settings).
    fn gpu_reset(&self, card: &str) -> Res {
        let stock = self.gpus().into_iter().find(|c| c.id == card).map(|c| c.caps.stock()).ok_or(Fail::Missing)?;
        self.gpu_set(card, &stock)
    }
    /// Intel's overclocking waiver, accepted by the user in Plume Boost.
    fn gpu_waiver(&self, _card: &str) -> Res {
        Ok(())
    }
    /// A stability test: a Direct3D 11 compute load for `secs`, checked for wrong results and driver resets.
    fn gpu_stress(&self, card: &str, secs: f64, stop: &std::sync::atomic::AtomicBool, progress: crate::gpu::StressProgress) -> crate::gpu::StressResult;

    // ── what the crash guard looks at ────────────────────────────────────────────────────────────────────────────

    /// When Windows started (seconds since 1970).
    fn boot_time(&self) -> u64;
    /// When Windows last shut down properly (seconds since 1970).
    fn clean_shutdown_time(&self) -> u64;
    /// Graphics driver resets (TDR, "Display driver stopped responding and has recovered") since `since`.
    fn driver_resets_since(&self, since: u64) -> u32;

    /// Test builds: things that happen to the imaginary PC ("reset": a driver reset now; "crash": Windows started
    /// again without a clean shutdown; "restart": a clean restart). The real PC answers false.
    #[cfg_attr(not(feature = "test-hook"), allow(dead_code))]
    fn test_event(&self, _what: &str) -> bool {
        false
    }

    /// The imaginary PC's stand-in for "Restart as administrator": it gets the rights and says so. The real PC
    /// answers false (only a real UAC prompt can give them).
    fn grant_admin_for_test(&self) -> bool {
        false
    }
}

pub type Shared = Arc<dyn Backend>;

/// The backend this run uses: the imaginary PC when PLUME_BOOST_FAKE is set, and always in a test build. Also gives
/// the imaginary PC's folder, where the settings and the undo log then live too.
pub fn choose() -> (Shared, Option<std::path::PathBuf>) {
    let fake_dir = std::env::var_os("PLUME_BOOST_FAKE")
        .map(std::path::PathBuf::from)
        .or_else(|| cfg!(feature = "test-hook").then(|| std::env::temp_dir().join("plume-boost-fake")));
    match fake_dir {
        Some(d) => (Arc::new(fake::Fake::open(d.clone())), Some(d)),
        None => (Arc::new(real::Real::new()), None),
    }
}
