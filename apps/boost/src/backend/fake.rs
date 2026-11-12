//! The imaginary PC: a registry, power plans, services, restore points and graphics cards kept in memory and saved to
//! `<dir>/fake_system.json` after every change, so a test script can read what Plume Boost did.
//!
//! It starts like a fresh Windows 11 laptop: Balanced and High performance plans (Balanced active), Ultimate
//! Performance as a hidden template, SysMain and Windows Search automatic, no tweak applied. Like Windows, it refuses
//! machine-wide writes (HKLM, services, restore points, power setting attributes) without administrator rights.
//!   PLUME_BOOST_FAKE_ADMIN=1        starts with administrator rights
//!   PLUME_BOOST_FAKE_NO_ULTIMATE=1  the PC hides Ultimate Performance (as on some laptops)
//!   PLUME_BOOST_FAKE_NO_BATTERY=1   a desktop PC
//!   PLUME_BOOST_FAKE_GPU=nvidia,iris  its graphics cards: nvidia (RTX 4070), amd (RX 7800 XT), arc (Arc A770),
//!                                   iris (Iris Xe, readings only: the default), none
//!   PLUME_BOOST_STRESS_SECS=3       how long the imaginary stability test lasts (60 s otherwise)
//!
//! Its graphics cards have the ranges such cards really report. Each has limits beyond which it is "unstable": the
//! stability test then ends with a driver reset, counted like Windows would log it.

use super::{Backend, Fail, Hive, Plan, Raw, Res, Val, guids};
use crate::gpu::{Caps, Card, CurveCaps, Fan, Knob, Range, StressProgress, StressResult, Telemetry, Tuning, Vendor};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

/// An imaginary graphics card.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FakeGpu {
    pub card: Card,
    pub now: Tuning,
    /// Above these values the card is unstable (the stability test fails with a driver reset)…
    pub unstable_above: BTreeMap<Knob, f64>,
    /// …and below these (an undervolt that goes too far).
    pub unstable_below: BTreeMap<Knob, f64>,
    pub waiver: bool,
}

fn card(id: &str, name: &str, vendor: Vendor, integrated: bool, caps: Caps, why_not: Option<&str>, waiver: bool) -> Card {
    Card { id: id.into(), name: name.into(), vendor, integrated, caps, api: "imaginary".into(), why_not: why_not.map(Into::into), needs_waiver: waiver }
}

/// The imaginary cards, with ranges like the real ones report.
pub fn fake_gpu(kind: &str) -> Option<FakeGpu> {
    let r = Range::new;
    let mut g = match kind {
        "nvidia" => FakeGpu {
            card: card(
                "nvidia-1-rtx4070",
                "NVIDIA GeForce RTX 4070 (imaginary)",
                Vendor::Nvidia,
                false,
                Caps {
                    knobs: vec![
                        (Knob::CoreOffset, r(-1000.0, 1000.0, 1.0, 0.0)),
                        (Knob::MemOffset, r(-1000.0, 1500.0, 1.0, 0.0)),
                        (Knob::Power, r(100.0, 220.0, 1.0, 200.0)),
                    ],
                    fan_fixed: Some(r(30.0, 100.0, 1.0, 30.0)),
                    fan_curve: None,
                    power_percent: false,
                },
                None,
                false,
            ),
            now: Tuning::default(),
            unstable_above: [(Knob::CoreOffset, 180.0), (Knob::MemOffset, 1200.0)].into(),
            unstable_below: BTreeMap::new(),
            waiver: false,
        },
        "amd" => FakeGpu {
            card: card(
                "amd-3-rx7800xt",
                "AMD Radeon RX 7800 XT (imaginary)",
                Vendor::Amd,
                false,
                Caps {
                    knobs: vec![
                        (Knob::CoreMax, r(500.0, 2900.0, 1.0, 2565.0)),
                        (Knob::MemMax, r(2400.0, 2700.0, 1.0, 2425.0)),
                        (Knob::Voltage, r(700.0, 1150.0, 1.0, 1150.0)),
                        (Knob::Power, r(-10.0, 15.0, 1.0, 0.0)),
                    ],
                    fan_fixed: Some(r(20.0, 100.0, 1.0, 20.0)),
                    fan_curve: Some(CurveCaps { temp: r(25.0, 100.0, 1.0, 50.0), speed: r(20.0, 100.0, 1.0, 30.0), points: 5 }),
                    power_percent: true,
                },
                None,
                false,
            ),
            now: Tuning { fan: Fan::Curve(vec![(40, 20), (55, 30), (70, 45), (80, 60), (90, 80)]), ..Default::default() },
            unstable_above: [(Knob::CoreMax, 2750.0)].into(),
            unstable_below: [(Knob::Voltage, 1000.0)].into(),
            waiver: false,
        },
        "arc" => FakeGpu {
            card: card(
                "intel-3-arca770",
                "Intel Arc A770 Graphics (imaginary)",
                Vendor::Intel,
                false,
                Caps {
                    knobs: vec![
                        (Knob::CoreOffset, r(0.0, 300.0, 1.0, 0.0)),
                        (Knob::VoltOffset, r(0.0, 250.0, 1.0, 0.0)),
                        (Knob::Power, r(190.0, 228.0, 1.0, 190.0)),
                        (Knob::TempLimit, r(71.0, 100.0, 1.0, 90.0)),
                    ],
                    fan_fixed: Some(r(0.0, 100.0, 1.0, 0.0)),
                    fan_curve: Some(CurveCaps { temp: r(25.0, 100.0, 1.0, 50.0), speed: r(0.0, 100.0, 1.0, 30.0), points: 5 }),
                    power_percent: false,
                },
                None,
                true,
            ),
            now: Tuning::default(),
            unstable_above: [(Knob::CoreOffset, 150.0)].into(),
            unstable_below: BTreeMap::new(),
            waiver: false,
        },
        "iris" => FakeGpu {
            card: card("intel-0-irisxe", "Intel Iris Xe Graphics (imaginary)", Vendor::Intel, true, Caps::default(), Some("integrated"), false),
            now: Tuning::default(),
            unstable_above: BTreeMap::new(),
            unstable_below: BTreeMap::new(),
            waiver: false,
        },
        _ => return None,
    };
    if g.card.caps.tunable() {
        let fan = std::mem::take(&mut g.now.fan);
        g.now = Tuning { fan, ..g.card.caps.stock() };
    }
    Some(g)
}

impl FakeGpu {
    fn unstable(&self) -> bool {
        self.now.knobs.iter().any(|(k, v)| self.unstable_above.get(k).is_some_and(|lim| v > lim) || self.unstable_below.get(k).is_some_and(|lim| v < lim))
    }

    /// Readings that follow the settings: more clock with an offset, more heat and power under load.
    fn telemetry(&self, load: f64, t: f64) -> Telemetry {
        let c = &self.card;
        let wobble = (t * 0.7).sin() * 0.5 + 0.5;
        let k = |n: Knob| self.now.get(n).unwrap_or(0.0);
        let (idle_core, top_core, mem, tdp) = match c.id.as_str() {
            "nvidia-1-rtx4070" => (210.0, 2745.0 + k(Knob::CoreOffset) * 0.9, 10501.0 + k(Knob::MemOffset), k(Knob::Power).max(100.0)),
            "amd-3-rx7800xt" => (35.0, k(Knob::CoreMax).min(2765.0) * 0.98, k(Knob::MemMax), 263.0 * (1.0 + k(Knob::Power) / 100.0)),
            "intel-3-arca770" => (300.0, 2400.0 + k(Knob::CoreOffset), 2187.0, k(Knob::Power).max(190.0)),
            _ => (100.0, 1300.0, 2133.0, 15.0),
        };
        let core = idle_core + (top_core - idle_core) * load - wobble * 15.0 * load;
        let temp = 34.0 + 38.0 * load + wobble * 2.0;
        let power = if c.integrated { 2.0 + 10.0 * load } else { 14.0 + (tdp - 14.0) * load * (0.93 + wobble * 0.05) };
        let fan = match &self.now.fan {
            Fan::Fixed(p) => *p as f64,
            Fan::Auto if load > 0.2 => 35.0 + 30.0 * load,
            Fan::Auto => 0.0,
            Fan::Curve(pts) => pts.iter().rev().find(|(tc, _)| temp >= *tc as f64).or(pts.first()).map(|p| p.1 as f64).unwrap_or(30.0),
        };
        let r1 = |v: f64| (v * 10.0).round() / 10.0;
        Telemetry {
            core_mhz: Some(core.round()),
            mem_mhz: Some(if load > 0.1 { mem } else { mem * 0.1 }.round()),
            temp_c: Some(r1(temp)),
            hotspot_c: (!c.integrated).then(|| r1(temp + 11.0)),
            fan_pct: (!c.integrated).then_some(fan),
            fan_rpm: (!c.integrated).then_some((fan * 32.0).round()),
            power_w: Some(r1(power)),
            power_limit_w: (!c.integrated).then_some(if c.caps.power_percent { r1(tdp) } else { k(Knob::Power) }),
            usage_pct: Some((load * 97.0 + wobble * 3.0).min(100.0).round()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct State {
    pub admin: bool,
    pub battery: bool,
    /// "HKCU\path\name" (path in lowercase) → value.
    pub reg: BTreeMap<String, Val>,
    pub plans: Vec<Plan>,
    /// Hidden templates that can be copied (GUID → name).
    pub templates: BTreeMap<String, String>,
    pub active: String,
    /// "plan|setting|ac" or "plan|setting|dc" → value.
    pub power: BTreeMap<String, u32>,
    pub services: BTreeMap<String, u32>,
    pub restore_points: Vec<String>,
    /// Every change, in order ("set HKCU\…", "plan …", "service …", "gpu …").
    pub writes: Vec<String>,
    pub next_id: u32,
    /// Power settings' attributes (setting GUID → 1 hidden, 0 shown).
    pub attribs: BTreeMap<String, u32>,
    pub gpus: Vec<FakeGpu>,
    /// When Windows started, when it last shut down properly, and the driver resets (seconds since 1970).
    pub boot: u64,
    pub clean_shutdown: u64,
    pub resets: Vec<u64>,
}

impl Default for State {
    fn default() -> Self {
        let now = crate::engine::now();
        let mut s = State {
            admin: false,
            battery: true,
            reg: BTreeMap::new(),
            plans: vec![Plan { guid: guids::BALANCED.into(), name: "Balanced".into() }, Plan { guid: guids::HIGH.into(), name: "High performance".into() }],
            templates: BTreeMap::new(),
            active: guids::BALANCED.into(),
            power: BTreeMap::new(),
            services: BTreeMap::new(),
            restore_points: Vec::new(),
            writes: Vec::new(),
            next_id: 1,
            attribs: BTreeMap::new(),
            gpus: Vec::new(),
            boot: now.saturating_sub(3600),
            clean_shutdown: now.saturating_sub(3700),
            resets: Vec::new(),
        };
        s.templates.insert(guids::ULTIMATE.into(), "Ultimate Performance".into());
        s.templates.insert(guids::POWER_SAVER.into(), "Power saver".into());
        // processor settings as Windows ships them
        for (plan, min, max, boost, epp, park) in [
            (guids::BALANCED, (5, 5), (100, 100), (2, 2), (25, 50), (10, 10)),
            (guids::HIGH, (100, 5), (100, 100), (2, 2), (0, 25), (100, 100)),
            (guids::ULTIMATE, (100, 100), (100, 100), (2, 2), (0, 0), (100, 100)),
            (guids::POWER_SAVER, (5, 5), (100, 70), (0, 0), (50, 70), (10, 10)),
        ] {
            for (setting, (ac, dc)) in [(guids::PROC_MIN, min), (guids::PROC_MAX, max), (guids::BOOST_MODE, boost), (guids::EPP, epp), (guids::PARK_MIN, park)]
            {
                s.power.insert(pkey(plan, setting, true), ac);
                s.power.insert(pkey(plan, setting, false), dc);
            }
        }
        // hidden from Windows' advanced power options, as shipped
        for setting in [guids::BOOST_MODE, guids::EPP, guids::PARK_MIN] {
            s.attribs.insert(setting.into(), 1);
        }
        s.services.insert("sysmain".into(), 2);
        s.services.insert("wsearch".into(), 2);
        // a few values a real PC has
        s.reg.insert(rkey(Hive::Machine, r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers", "HwSchMode"), Val::Dword(1));
        s.reg.insert(rkey(Hive::Machine, r"SYSTEM\CurrentControlSet\Control\Power", "HibernateEnabled"), Val::Dword(1));
        s.reg.insert(rkey(Hive::Machine, r"SYSTEM\CurrentControlSet\Control\Session Manager\Power", "HiberbootEnabled"), Val::Dword(1));
        s.reg.insert(rkey(Hive::User, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency"), Val::Dword(1));
        s.reg.insert(rkey(Hive::User, r"System\GameConfigStore", "GameDVR_Enabled"), Val::Dword(1));
        s.reg.insert(rkey(Hive::User, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled"), Val::Dword(1));
        s.reg.insert(rkey(Hive::User, r"Control Panel\Desktop", "UserPreferencesMask"), Val::Bin(vec![0x9e, 0x1e, 0x07, 0x80, 0x12, 0x00, 0x00, 0x00]));
        s.reg.insert(rkey(Hive::User, r"Control Panel\Desktop", "WallPaper"), Val::Sz(r"C:\Windows\Web\Wallpaper\Windows\img0.jpg".into()));
        s.reg.insert(rkey(Hive::User, r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects", "VisualFXSetting"), Val::Dword(0));
        s.reg.insert(rkey(Hive::User, r"Software\Microsoft\DirectX\UserGpuPreferences", "DirectXUserGlobalSettings"), Val::Sz("AutoHDREnable=1;".into()));
        s
    }
}

pub fn rkey(hive: Hive, path: &str, name: &str) -> String {
    format!("{}\\{}\\{}", hive.short(), path.to_lowercase(), name.to_lowercase())
}

fn pkey(plan: &str, setting: &str, ac: bool) -> String {
    format!("{}|{}|{}", plan.to_lowercase(), setting.to_lowercase(), if ac { "ac" } else { "dc" })
}

pub struct Fake {
    pub dir: PathBuf,
    state: Mutex<State>,
}

impl Fake {
    /// The imaginary PC saved in `dir` (a new one when the folder has none).
    pub fn open(dir: PathBuf) -> Fake {
        let file = dir.join("fake_system.json");
        let state = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<State>(&b).ok()).unwrap_or_else(|| {
            let mut s = State::default();
            let on = |k: &str| std::env::var(k).is_ok_and(|v| v == "1");
            s.admin = on("PLUME_BOOST_FAKE_ADMIN");
            s.battery = !on("PLUME_BOOST_FAKE_NO_BATTERY");
            if on("PLUME_BOOST_FAKE_NO_ULTIMATE") {
                s.templates.remove(guids::ULTIMATE);
            }
            let kinds = std::env::var("PLUME_BOOST_FAKE_GPU").unwrap_or_else(|_| "iris".into());
            s.gpus = kinds.split(',').filter_map(|k| fake_gpu(k.trim())).collect();
            s
        });
        let f = Fake { dir, state: Mutex::new(state) };
        f.save();
        f
    }

    /// An imaginary PC that is never saved (unit tests).
    #[cfg(test)]
    pub fn memory(state: State) -> Fake {
        Fake { dir: PathBuf::new(), state: Mutex::new(state) }
    }

    #[cfg(test)]
    pub fn snapshot(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    /// Changes the imaginary PC directly (tests: a driver reset, a restart…).
    pub fn poke(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.state.lock().unwrap());
        self.save();
    }

    /// Gives or takes administrator rights (what "Restart as administrator" does in test mode).
    pub fn set_admin(&self, on: bool) {
        self.poke(|s| s.admin = on);
    }

    fn save(&self) {
        if self.dir.as_os_str().is_empty() {
            return;
        }
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(b) = serde_json::to_vec_pretty(&*self.state.lock().unwrap()) {
            let file = self.dir.join("fake_system.json");
            let tmp = file.with_extension("tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, &file);
            }
        }
    }

    fn change(&self, needs_admin: bool, what: String, f: impl FnOnce(&mut State) -> Res) -> Res {
        {
            let mut s = self.state.lock().unwrap();
            if needs_admin && !s.admin {
                return Err(Fail::NeedsAdmin);
            }
            f(&mut s)?;
            s.writes.push(what);
        }
        self.save();
        Ok(())
    }

    fn gpu_index(s: &State, card: &str) -> Option<usize> {
        s.gpus.iter().position(|g| g.card.id == card)
    }
}

impl Backend for Fake {
    fn is_test(&self) -> bool {
        true
    }
    fn is_admin(&self) -> bool {
        self.state.lock().unwrap().admin
    }
    fn has_battery(&self) -> bool {
        self.state.lock().unwrap().battery
    }

    fn reg_get(&self, hive: Hive, path: &str, name: &str) -> Option<Val> {
        self.state.lock().unwrap().reg.get(&rkey(hive, path, name)).cloned()
    }
    fn reg_set(&self, hive: Hive, path: &str, name: &str, v: &Val) -> Res {
        let k = rkey(hive, path, name);
        let what = format!("set {k} = {}", v.show());
        let v = v.clone();
        self.change(hive == Hive::Machine, what, move |s| {
            s.reg.insert(k, v);
            Ok(())
        })
    }
    fn reg_del(&self, hive: Hive, path: &str, name: &str) -> Res {
        let k = rkey(hive, path, name);
        self.change(hive == Hive::Machine, format!("delete {k}"), move |s| {
            s.reg.remove(&k);
            Ok(())
        })
    }

    fn plans(&self) -> Vec<Plan> {
        self.state.lock().unwrap().plans.clone()
    }
    fn active_plan(&self) -> Option<String> {
        Some(self.state.lock().unwrap().active.clone())
    }
    fn set_active_plan(&self, guid: &str) -> Res {
        let g = guid.to_lowercase();
        self.change(false, format!("plan active {g}"), move |s| {
            if !s.plans.iter().any(|p| p.guid == g) {
                return Err(Fail::Missing);
            }
            s.active = g;
            Ok(())
        })
    }
    fn plan_name(&self, guid: &str) -> Option<String> {
        let s = self.state.lock().unwrap();
        let g = guid.to_lowercase();
        s.plans.iter().find(|p| p.guid == g).map(|p| p.name.clone()).or_else(|| s.templates.get(&g).cloned())
    }
    fn duplicate_plan(&self, source: &str) -> Res<String> {
        let src = source.to_lowercase();
        let mut out = String::new();
        let out_ref = &mut out;
        self.change(false, format!("plan copy {src}"), move |s| {
            let name = s.plans.iter().find(|p| p.guid == src).map(|p| p.name.clone()).or_else(|| s.templates.get(&src).cloned()).ok_or(Fail::Missing)?;
            let guid = format!("00000000-0000-4000-8000-{:012x}", s.next_id);
            s.next_id += 1;
            let copies: Vec<(String, u32)> =
                s.power.iter().filter(|(k, _)| k.starts_with(&format!("{src}|"))).map(|(k, v)| (k.replacen(&src, &guid, 1), *v)).collect();
            s.power.extend(copies);
            s.plans.push(Plan { guid: guid.clone(), name });
            *out_ref = guid;
            Ok(())
        })?;
        Ok(out)
    }
    fn delete_plan(&self, guid: &str) -> Res {
        let g = guid.to_lowercase();
        self.change(false, format!("plan delete {g}"), move |s| {
            if s.active == g {
                return Err(Fail::Other("the active plan can't be deleted".into()));
            }
            s.plans.retain(|p| p.guid != g);
            s.power.retain(|k, _| !k.starts_with(&format!("{g}|")));
            Ok(())
        })
    }
    fn power_get(&self, plan: &str, _sub: &str, setting: &str, ac: bool) -> Option<u32> {
        self.state.lock().unwrap().power.get(&pkey(plan, setting, ac)).copied()
    }
    fn power_set(&self, plan: &str, _sub: &str, setting: &str, ac: bool, v: u32) -> Res {
        let k = pkey(plan, setting, ac);
        let plan = plan.to_lowercase();
        self.change(false, format!("power {k} = {v}"), move |s| {
            if !s.plans.iter().any(|p| p.guid == plan) {
                return Err(Fail::Missing);
            }
            s.power.insert(k, v);
            Ok(())
        })
    }

    fn service_start(&self, name: &str) -> Option<u32> {
        self.state.lock().unwrap().services.get(&name.to_lowercase()).copied()
    }
    fn set_service_start(&self, name: &str, start: u32) -> Res {
        let n = name.to_lowercase();
        self.change(true, format!("service {n} start = {start}"), move |s| {
            if !s.services.contains_key(&n) {
                return Err(Fail::Missing);
            }
            s.services.insert(n, start);
            Ok(())
        })
    }

    fn grant_admin_for_test(&self) -> bool {
        self.set_admin(true);
        true
    }

    fn test_event(&self, what: &str) -> bool {
        let now = crate::engine::now();
        match what {
            "reset" => self.poke(|s| s.resets.push(now)),
            "crash" => self.poke(|s| s.boot = now + 120),
            "restart" => self.poke(|s| {
                s.clean_shutdown = now + 100;
                s.boot = now + 120;
            }),
            _ => return false,
        }
        true
    }

    fn restore_point(&self, desc: &str) -> Res {
        let d = desc.to_string();
        self.change(true, format!("restore point \"{desc}\""), move |s| {
            s.restore_points.push(d);
            Ok(())
        })
    }

    // ── snapshots ────────────────────────────────────────────────────────────────────────────────────────────────

    fn reg_values(&self, hive: Hive, path: &str) -> Option<Vec<Raw>> {
        let prefix = format!("{}\\{}\\", hive.short(), path.to_lowercase());
        let s = self.state.lock().unwrap();
        let out: Vec<Raw> = s.reg.iter().filter_map(|(k, v)| k.strip_prefix(&prefix).filter(|n| !n.contains('\\')).map(|n| Raw::from_val(n, v))).collect();
        (!out.is_empty()).then_some(out)
    }
    fn reg_set_raw(&self, hive: Hive, path: &str, v: &Raw) -> Res {
        let val = v.to_val().unwrap_or_else(|| Val::Bin(v.data.clone()));
        self.reg_set(hive, path, &v.name, &val)
    }
    fn power_export(&self, plan: &str, file: &Path) -> Res {
        let g = plan.to_lowercase();
        let (name, settings) = {
            let s = self.state.lock().unwrap();
            let name = s.plans.iter().find(|p| p.guid == g).map(|p| p.name.clone()).ok_or(Fail::Missing)?;
            let settings: BTreeMap<String, u32> = s.power.iter().filter(|(k, _)| k.starts_with(&format!("{g}|"))).map(|(k, v)| (k.clone(), *v)).collect();
            (name, settings)
        };
        let body = serde_json::json!({ "imaginary powercfg export": g, "name": name, "settings": settings });
        std::fs::write(file, serde_json::to_vec_pretty(&body).unwrap_or_default()).map_err(|e| Fail::Other(e.to_string()))
    }
    fn power_import(&self, plan: &str, file: &Path) -> Res {
        let g = plan.to_lowercase();
        let body: serde_json::Value = std::fs::read(file).ok().and_then(|b| serde_json::from_slice(&b).ok()).ok_or(Fail::Missing)?;
        let name = body["name"].as_str().unwrap_or("Imported plan").to_string();
        let settings: BTreeMap<String, u32> = serde_json::from_value(body["settings"].clone()).unwrap_or_default();
        self.change(false, format!("plan import {g}"), move |s| {
            if !s.plans.iter().any(|p| p.guid == g) {
                s.plans.push(Plan { guid: g.clone(), name });
            }
            s.power.extend(settings);
            Ok(())
        })
    }
    fn power_attrib(&self, _sub: &str, setting: &str) -> Option<u32> {
        Some(self.state.lock().unwrap().attribs.get(&setting.to_lowercase()).copied().unwrap_or(0))
    }
    fn set_power_attrib(&self, _sub: &str, setting: &str, attrib: u32) -> Res {
        let k = setting.to_lowercase();
        self.change(true, format!("attrib {k} = {attrib}"), move |s| {
            s.attribs.insert(k, attrib);
            Ok(())
        })
    }

    // ── graphics cards ───────────────────────────────────────────────────────────────────────────────────────────

    fn gpus(&self) -> Vec<Card> {
        self.state.lock().unwrap().gpus.iter().map(|g| g.card.clone()).collect()
    }
    fn gpu_telemetry(&self, card: &str) -> Option<Telemetry> {
        let s = self.state.lock().unwrap();
        let g = &s.gpus[Self::gpu_index(&s, card)?];
        Some(g.telemetry(LOADS.get(card), crate::engine::now_f64()))
    }
    fn gpu_get(&self, card: &str) -> Option<Tuning> {
        let s = self.state.lock().unwrap();
        Some(s.gpus[Self::gpu_index(&s, card)?].now.clone())
    }
    fn gpu_set(&self, card: &str, t: &Tuning) -> Res {
        let id = card.to_string();
        let t = t.clone();
        let what = format!("gpu {id} = {}", t.describe());
        self.change(false, what, move |s| {
            let i = Self::gpu_index(s, &id).ok_or(Fail::Missing)?;
            let g = &mut s.gpus[i];
            if !g.card.caps.tunable() {
                return Err(Fail::Missing);
            }
            if g.card.needs_waiver && !g.waiver {
                return Err(Fail::Other("Intel's overclocking waiver was not accepted".into()));
            }
            // like the drivers: a value outside the range is refused, never stored
            for (k, v) in &t.knobs {
                let r = g.card.caps.range(*k).ok_or(Fail::Other(format!("{k:?} is not on this card")))?;
                if *v < r.min || *v > r.max {
                    return Err(Fail::Other(format!("{k:?} {v} is outside {}..{}", r.min, r.max)));
                }
            }
            g.now.knobs.extend(t.knobs.clone());
            g.now.fan = match (&t.fan, g.card.vendor) {
                // AMD's driver always has a curve: a fixed speed is a flat one
                (Fan::Fixed(p), Vendor::Amd) => Fan::Curve(vec![(40, *p), (55, *p), (70, *p), (80, *p), (90, *p)]),
                (f, _) => f.clone(),
            };
            Ok(())
        })
    }
    fn gpu_waiver(&self, card: &str) -> Res {
        let id = card.to_string();
        self.change(false, format!("gpu {card} waiver accepted"), move |s| {
            let i = Self::gpu_index(s, &id).ok_or(Fail::Missing)?;
            s.gpus[i].waiver = true;
            Ok(())
        })
    }
    fn gpu_stress(&self, card: &str, secs: f64, stop: &AtomicBool, progress: StressProgress) -> StressResult {
        let secs = std::env::var("PLUME_BOOST_STRESS_SECS").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(secs);
        let unstable = {
            let s = self.state.lock().unwrap();
            match Self::gpu_index(&s, card) {
                Some(i) => s.gpus[i].unstable(),
                None => return StressResult { failed_to_start: Some("no such card".into()), ..Default::default() },
            }
        };
        LOADS.set(card, 1.0);
        let start = std::time::Instant::now();
        let mut r = StressResult::default();
        loop {
            let done = start.elapsed().as_secs_f64();
            let tel = self.gpu_telemetry(card);
            if let Some(t) = tel {
                r.max_temp = crate::gpu::max_opt(r.max_temp, t.temp_c);
                r.max_clock = crate::gpu::max_opt(r.max_clock, t.core_mhz);
                r.max_power = crate::gpu::max_opt(r.max_power, t.power_w);
                if done > secs * 0.5 {
                    r.min_clock_late = match (r.min_clock_late, t.core_mhz) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (a, b) => a.or(b),
                    };
                }
            }
            progress(done.min(secs), secs, tel);
            if stop.load(Ordering::Relaxed) {
                r.stopped = true;
                break;
            }
            if unstable && done >= secs * 0.4 {
                // what an unstable setting does: the driver stops responding and Windows resets it (and logs it)
                r.device_removed = true;
                let now = crate::engine::now();
                self.poke(|s| s.resets.push(now));
                break;
            }
            if done >= secs {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        r.secs = start.elapsed().as_secs_f64().min(secs);
        LOADS.set(card, 0.0);
        r
    }

    fn boot_time(&self) -> u64 {
        self.state.lock().unwrap().boot
    }
    fn clean_shutdown_time(&self) -> u64 {
        self.state.lock().unwrap().clean_shutdown
    }
    fn driver_resets_since(&self, since: u64) -> u32 {
        self.state.lock().unwrap().resets.iter().filter(|t| **t >= since).count() as u32
    }
}

/// The load each imaginary card is under (1 during its stability test), shared in this process.
struct Loads(Mutex<BTreeMap<String, f64>>);

impl Loads {
    fn set(&self, card: &str, v: f64) {
        self.0.lock().unwrap().insert(card.to_string(), v);
    }
    fn get(&self, card: &str) -> f64 {
        self.0.lock().unwrap().get(card).copied().unwrap_or(0.04)
    }
}

static LOADS: Loads = Loads(Mutex::new(BTreeMap::new()));
