//! The PC itself, through Windows' own APIs: the registry (advapi32), power plans (powrprof), services (the Service
//! Control Manager) and System Restore (SRSetRestorePointW). Nothing is run through a command line.
//!
//! In a `test-hook` build and in unit tests every write returns an error before touching Windows: only the
//! imaginary PC of `fake.rs` can be changed while Plume Boost is being developed.

use super::{Backend, Fail, Hive, Plan, Raw, Res, Val};
use crate::gpu::{Card, Driver, StressProgress, StressResult, Telemetry, Tuning, Vendor};
use std::sync::{Mutex, atomic::AtomicBool};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, HANDLE, HLOCAL, LocalFree, WIN32_ERROR},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::{
            Power::{
                ACCESS_SCHEME, GetSystemPowerStatus, PowerDeleteScheme, PowerDuplicateScheme, PowerEnumerate, PowerGetActiveScheme, PowerReadACValueIndex,
                PowerReadDCValueIndex, PowerReadFriendlyName, PowerSetActiveScheme, PowerWriteACValueIndex, PowerWriteDCValueIndex, SYSTEM_POWER_STATUS,
            },
            Registry::{
                HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_BINARY, REG_DWORD, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ,
                REG_VALUE_TYPE, RRF_RT_ANY, RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegGetValueW, RegSetValueExW,
            },
            Restore::{BEGIN_SYSTEM_CHANGE, END_SYSTEM_CHANGE, MODIFY_SETTINGS, RESTOREPOINTINFOW, SRSetRestorePointW, STATEMGRSTATUS},
            Services::{
                ChangeServiceConfigW, CloseServiceHandle, ENUM_SERVICE_TYPE, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT, SERVICE_CHANGE_CONFIG,
                SERVICE_ERROR, SERVICE_NO_CHANGE, SERVICE_START_TYPE,
            },
            Threading::{GetCurrentProcess, OpenProcessToken},
        },
    },
    core::{GUID, HSTRING, PCWSTR},
};

pub struct Real {
    admin: bool,
    gpu: Mutex<Option<Gpus>>,
}

impl Real {
    pub fn new() -> Real {
        Real { admin: elevated(), gpu: Mutex::new(None) }
    }
}

/// Plume Boost runs elevated (administrator rights granted through UAC).
pub fn elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut e = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(token, TokenElevation, Some(&mut e as *mut _ as *mut _), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut len).is_ok();
        let _ = CloseHandle(token);
        ok && e.TokenIsElevated != 0
    }
}

/// Writes are refused in test builds: developing Plume Boost never changes the PC it runs on.
fn may_write() -> Res {
    if cfg!(feature = "test-hook") || cfg!(test) { Err(Fail::Other("test build: changes to this PC are switched off".into())) } else { Ok(()) }
}

fn win(e: WIN32_ERROR) -> Res {
    match e {
        ERROR_SUCCESS => Ok(()),
        ERROR_ACCESS_DENIED => Err(Fail::NeedsAdmin),
        ERROR_FILE_NOT_FOUND => Err(Fail::Missing),
        e => Err(Fail::Other(format!("Windows error {}", e.0))),
    }
}

fn hres(e: windows::core::Error) -> Fail {
    if e.code() == ERROR_ACCESS_DENIED.to_hresult() { Fail::NeedsAdmin } else { Fail::Other(e.message()) }
}

fn root(h: Hive) -> HKEY {
    match h {
        Hive::User => HKEY_CURRENT_USER,
        Hive::Machine => HKEY_LOCAL_MACHINE,
    }
}

pub fn parse_guid(s: &str) -> Option<GUID> {
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 {
        return None;
    }
    u128::from_str_radix(&hex, 16).ok().map(GUID::from_u128)
}

pub fn show_guid(g: &GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1, g.data2, g.data3, g.data4[0], g.data4[1], g.data4[2], g.data4[3], g.data4[4], g.data4[5], g.data4[6], g.data4[7]
    )
}

/// Reads a registry value; usable without administrator rights (also by the live view).
pub fn read_value(hive: Hive, path: &str, name: &str) -> Option<Val> {
    unsafe {
        let (p, n) = (HSTRING::from(path), HSTRING::from(name));
        let mut ty = REG_VALUE_TYPE::default();
        let mut size = 0u32;
        if RegGetValueW(root(hive), &p, &n, RRF_RT_ANY, Some(&mut ty), None, Some(&mut size)) != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u8; size as usize + 2];
        let mut size = buf.len() as u32;
        if RegGetValueW(root(hive), &p, &n, RRF_RT_ANY, Some(&mut ty), Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) != ERROR_SUCCESS {
            return None;
        }
        buf.truncate(size as usize);
        match ty {
            REG_DWORD if buf.len() >= 4 => Some(Val::Dword(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))),
            REG_SZ | REG_EXPAND_SZ => {
                let w: Vec<u16> = buf.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
                Some(Val::Sz(String::from_utf16_lossy(&w)))
            }
            REG_BINARY => Some(Val::Bin(buf)),
            _ => None,
        }
    }
}

impl Backend for Real {
    fn is_test(&self) -> bool {
        false
    }
    fn is_admin(&self) -> bool {
        self.admin
    }
    fn has_battery(&self) -> bool {
        let mut s = SYSTEM_POWER_STATUS::default();
        unsafe { GetSystemPowerStatus(&mut s).is_ok() && s.BatteryFlag != 128 && s.BatteryFlag != 255 }
    }

    fn reg_get(&self, hive: Hive, path: &str, name: &str) -> Option<Val> {
        read_value(hive, path, name)
    }
    fn reg_set(&self, hive: Hive, path: &str, name: &str, v: &Val) -> Res {
        may_write()?;
        let (ty, data): (REG_VALUE_TYPE, Vec<u8>) = match v {
            Val::Dword(d) => (REG_DWORD, d.to_le_bytes().to_vec()),
            Val::Sz(s) => (REG_SZ, s.encode_utf16().chain([0]).flat_map(|c| c.to_le_bytes()).collect()),
            Val::Bin(b) => (REG_BINARY, b.clone()),
        };
        unsafe {
            let mut key = HKEY::default();
            win(RegCreateKeyExW(root(hive), &HSTRING::from(path), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut key, None))?;
            let r = RegSetValueExW(key, &HSTRING::from(name), None, ty, Some(&data));
            let _ = RegCloseKey(key);
            win(r)
        }
    }
    fn reg_del(&self, hive: Hive, path: &str, name: &str) -> Res {
        may_write()?;
        match win(unsafe { RegDeleteKeyValueW(root(hive), &HSTRING::from(path), &HSTRING::from(name)) }) {
            Err(Fail::Missing) => Ok(()),
            r => r,
        }
    }

    fn plans(&self) -> Vec<Plan> {
        let mut out = Vec::new();
        for i in 0..64 {
            let mut g = GUID::zeroed();
            let mut size = std::mem::size_of::<GUID>() as u32;
            if unsafe { PowerEnumerate(None, None, None, ACCESS_SCHEME, i, Some(&mut g as *mut GUID as *mut u8), &mut size) } != ERROR_SUCCESS {
                break;
            }
            let guid = show_guid(&g);
            let name = self.plan_name(&guid).unwrap_or_else(|| guid.clone());
            out.push(Plan { guid, name });
        }
        out
    }
    fn active_plan(&self) -> Option<String> {
        unsafe {
            let mut p: *mut GUID = std::ptr::null_mut();
            if PowerGetActiveScheme(None, &mut p) != ERROR_SUCCESS || p.is_null() {
                return None;
            }
            let s = show_guid(&*p);
            let _ = LocalFree(Some(HLOCAL(p as *mut _)));
            Some(s)
        }
    }
    fn set_active_plan(&self, guid: &str) -> Res {
        may_write()?;
        let g = parse_guid(guid).ok_or(Fail::Missing)?;
        win(unsafe { PowerSetActiveScheme(None, Some(&g)) })
    }
    fn plan_name(&self, guid: &str) -> Option<String> {
        let g = parse_guid(guid)?;
        unsafe {
            let mut size = 0u32;
            if PowerReadFriendlyName(None, Some(&g), None, None, None, &mut size) != ERROR_SUCCESS || size < 2 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            if PowerReadFriendlyName(None, Some(&g), None, None, Some(buf.as_mut_ptr()), &mut size) != ERROR_SUCCESS {
                return None;
            }
            let w: Vec<u16> = buf.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
            Some(String::from_utf16_lossy(&w))
        }
    }
    fn duplicate_plan(&self, source: &str) -> Res<String> {
        may_write()?;
        let src = parse_guid(source).ok_or(Fail::Missing)?;
        unsafe {
            let mut p: *mut GUID = std::ptr::null_mut();
            win(PowerDuplicateScheme(None, &src, &mut p))?;
            if p.is_null() {
                return Err(Fail::Other("Windows gave no new plan".into()));
            }
            let s = show_guid(&*p);
            let _ = LocalFree(Some(HLOCAL(p as *mut _)));
            Ok(s)
        }
    }
    fn delete_plan(&self, guid: &str) -> Res {
        may_write()?;
        let g = parse_guid(guid).ok_or(Fail::Missing)?;
        win(unsafe { PowerDeleteScheme(None, &g) })
    }
    fn power_get(&self, plan: &str, sub: &str, setting: &str, ac: bool) -> Option<u32> {
        let (p, su, se) = (parse_guid(plan)?, parse_guid(sub)?, parse_guid(setting)?);
        let mut v = 0u32;
        let r = unsafe {
            if ac {
                PowerReadACValueIndex(None, Some(&p), Some(&su), Some(&se), &mut v)
            } else {
                WIN32_ERROR(PowerReadDCValueIndex(None, Some(&p), Some(&su), Some(&se), &mut v))
            }
        };
        (r == ERROR_SUCCESS).then_some(v)
    }
    fn power_set(&self, plan: &str, sub: &str, setting: &str, ac: bool, v: u32) -> Res {
        may_write()?;
        let (p, su, se) = (parse_guid(plan).ok_or(Fail::Missing)?, parse_guid(sub).ok_or(Fail::Missing)?, parse_guid(setting).ok_or(Fail::Missing)?);
        unsafe {
            win(if ac {
                PowerWriteACValueIndex(None, &p, Some(&su), Some(&se), v)
            } else {
                WIN32_ERROR(PowerWriteDCValueIndex(None, &p, Some(&su), Some(&se), v))
            })?;
            // a plan's values are read again by Windows when the plan is (re)activated
            if self.active_plan().as_deref() == Some(plan) {
                win(PowerSetActiveScheme(None, Some(&p)))?;
            }
        }
        Ok(())
    }

    fn service_start(&self, name: &str) -> Option<u32> {
        match read_value(Hive::Machine, &format!(r"SYSTEM\CurrentControlSet\Services\{name}"), "Start")? {
            Val::Dword(d) => Some(d),
            _ => None,
        }
    }
    fn set_service_start(&self, name: &str, start: u32) -> Res {
        may_write()?;
        unsafe {
            let scm = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT).map_err(hres)?;
            let svc = OpenServiceW(scm, &HSTRING::from(name), SERVICE_CHANGE_CONFIG);
            let r = match svc {
                Ok(h) => {
                    let r = ChangeServiceConfigW(
                        h,
                        ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
                        SERVICE_START_TYPE(start),
                        SERVICE_ERROR(SERVICE_NO_CHANGE),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        None,
                        PCWSTR::null(),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        PCWSTR::null(),
                    )
                    .map_err(hres);
                    let _ = CloseServiceHandle(h);
                    r
                }
                Err(e) => Err(hres(e)),
            };
            let _ = CloseServiceHandle(scm);
            r
        }
    }

    fn restore_point(&self, desc: &str) -> Res {
        may_write()?;
        let mut info = RESTOREPOINTINFOW { dwEventType: BEGIN_SYSTEM_CHANGE, dwRestorePtType: MODIFY_SETTINGS, llSequenceNumber: 0, szDescription: [0; 256] };
        for (i, c) in desc.encode_utf16().take(255).enumerate() {
            info.szDescription[i] = c;
        }
        let mut st = STATEMGRSTATUS::default();
        unsafe {
            if !SRSetRestorePointW(&info, &mut st).as_bool() {
                let code = st.nStatus;
                return Err(match code.0 {
                    1058 => Fail::Other(crate::i18n::tr("System Restore is turned off on this PC.").into()),
                    5 => Fail::NeedsAdmin,
                    c => Fail::Other(format!("System Restore error {c}")),
                });
            }
            let end = RESTOREPOINTINFOW {
                dwEventType: END_SYSTEM_CHANGE,
                dwRestorePtType: MODIFY_SETTINGS,
                llSequenceNumber: st.llSequenceNumber,
                szDescription: [0; 256],
            };
            let mut st2 = STATEMGRSTATUS::default();
            let _ = SRSetRestorePointW(&end, &mut st2);
        }
        Ok(())
    }

    fn reg_values(&self, hive: Hive, path: &str) -> Option<Vec<Raw>> {
        self.reg_values_impl(hive, path)
    }
    fn reg_set_raw(&self, hive: Hive, path: &str, v: &Raw) -> Res {
        may_write()?;
        unsafe {
            let mut key = HKEY::default();
            win(RegCreateKeyExW(root(hive), &HSTRING::from(path), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut key, None))?;
            let r = RegSetValueExW(key, &HSTRING::from(v.name.as_str()), None, REG_VALUE_TYPE(v.ty), Some(&v.data));
            let _ = RegCloseKey(key);
            win(r)
        }
    }
    fn power_export(&self, plan: &str, file: &std::path::Path) -> Res {
        // reads the plan into a file of Plume Boost's: no change to the PC
        powercfg(&["/export".as_ref(), file.as_os_str(), plan.as_ref()])
    }
    fn power_import(&self, plan: &str, file: &std::path::Path) -> Res {
        may_write()?;
        powercfg(&["/import".as_ref(), file.as_os_str(), plan.as_ref()])
    }
    fn power_attrib(&self, sub: &str, setting: &str) -> Option<u32> {
        let (su, se) = (parse_guid(sub)?, parse_guid(setting)?);
        Some(unsafe { windows::Win32::System::Power::PowerReadSettingAttributes(Some(&su), Some(&se)) })
    }
    fn set_power_attrib(&self, sub: &str, setting: &str, attrib: u32) -> Res {
        may_write()?;
        let (su, se) = (parse_guid(sub).ok_or(Fail::Missing)?, parse_guid(setting).ok_or(Fail::Missing)?);
        win(unsafe { windows::Win32::System::Power::PowerWriteSettingAttributes(Some(&su), Some(&se), attrib) })
    }

    fn gpus(&self) -> Vec<Card> {
        self.with_gpus(|g| g.cards.clone())
    }
    fn gpu_telemetry(&self, card: &str) -> Option<Telemetry> {
        self.with_driver(card, |d| d.telemetry(card))
    }
    fn gpu_get(&self, card: &str) -> Option<Tuning> {
        self.with_driver(card, |d| d.get(card))
    }
    fn gpu_set(&self, card: &str, t: &Tuning) -> Res {
        may_write()?;
        self.with_driver(card, |d| Some(d.set(card, t))).unwrap_or(Err(Fail::Missing))
    }
    fn gpu_reset(&self, card: &str) -> Res {
        may_write()?;
        self.with_driver(card, |d| Some(d.reset(card))).unwrap_or(Err(Fail::Missing))
    }
    fn gpu_waiver(&self, card: &str) -> Res {
        may_write()?;
        self.with_driver(card, |d| Some(d.waiver(card))).unwrap_or(Err(Fail::Missing))
    }
    fn gpu_stress(&self, card: &str, secs: f64, stop: &AtomicBool, progress: StressProgress) -> StressResult {
        let Some(c) = self.gpus().into_iter().find(|c| c.id == card) else {
            return StressResult { failed_to_start: Some("no such card".into()), ..Default::default() };
        };
        crate::gpu::stress::run(c.vendor, &c.name, secs, stop, &|| self.gpu_telemetry(card), progress)
    }

    fn boot_time(&self) -> u64 {
        let up = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } / 1000;
        crate::engine::now().saturating_sub(up)
    }
    fn clean_shutdown_time(&self) -> u64 {
        match read_value(Hive::Machine, r"SYSTEM\CurrentControlSet\Control\Windows", "ShutdownTime") {
            Some(Val::Bin(b)) if b.len() >= 8 => filetime_to_unix(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])),
            _ => 0,
        }
    }
    fn driver_resets_since(&self, since: u64) -> u32 {
        use windows::Win32::System::EventLog::{EVT_HANDLE, EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath};
        let q = format!("*[System[Provider[@Name='Display'] and (EventID=4101) and TimeCreated[@SystemTime>='{}']]]", iso_utc(since));
        unsafe {
            let Ok(h) = EvtQuery(None, &HSTRING::from("System"), &HSTRING::from(q), EvtQueryChannelPath.0) else { return 0 };
            let mut n = 0u32;
            loop {
                let mut evs = [0isize; 16];
                let mut got = 0u32;
                if EvtNext(h, &mut evs, 0, 0, &mut got).is_err() || got == 0 {
                    break;
                }
                for e in evs.iter().take(got as usize) {
                    let _ = EvtClose(EVT_HANDLE(*e));
                }
                n += got;
            }
            let _ = EvtClose(h);
            n
        }
    }
}

// ── snapshots, graphics cards and the crash guard's sensors ──────────────────────────────────────────────────────

/// The graphics drivers, loaded the first time a card is asked about, and the cards found.
pub struct Gpus {
    drivers: Vec<Box<dyn Driver>>,
    cards: Vec<Card>,
}

fn load_gpus() -> Gpus {
    let mut drivers: Vec<Box<dyn Driver>> = Vec::new();
    if let Some(d) = crate::gpu::nvidia::Nvidia::load() {
        drivers.push(Box::new(d));
    }
    if let Some(d) = crate::gpu::amd::Amd::load() {
        drivers.push(Box::new(d));
    }
    if let Some(d) = crate::gpu::intel::Intel::load() {
        drivers.push(Box::new(d));
    }
    let mut cards: Vec<Card> = drivers.iter().flat_map(|d| d.cards()).collect();
    // cards Windows sees but no maker's library answered for: shown, with why
    for (name, vendor, integrated) in dxgi_cards() {
        if !cards.iter().any(|c| c.vendor == vendor) {
            cards.push(Card {
                id: format!("{}-x-{}", vendor.name().to_lowercase(), crate::gpu::slug(&name)),
                name,
                vendor,
                integrated,
                caps: Default::default(),
                api: "DXGI".into(),
                why_not: Some(if integrated { "integrated".into() } else { "no-library".into() }),
                needs_waiver: false,
            });
        }
    }
    Gpus { drivers, cards }
}

/// The graphics cards Windows lists (DXGI): name, maker, integrated (little dedicated memory).
fn dxgi_cards() -> Vec<(String, Vendor, bool)> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};
    let mut out = Vec::new();
    unsafe {
        let Ok(f) = CreateDXGIFactory1::<IDXGIFactory1>() else { return out };
        for i in 0..8 {
            let Ok(a) = f.EnumAdapters1(i) else { break };
            let Ok(d) = a.GetDesc1() else { continue };
            if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let vendor = match d.VendorId {
                0x10DE => Vendor::Nvidia,
                0x1002 => Vendor::Amd,
                0x8086 => Vendor::Intel,
                _ => continue,
            };
            let name = String::from_utf16_lossy(&d.Description).trim_end_matches('\0').trim().to_string();
            if !out.iter().any(|(n, _, _)| *n == name) {
                out.push((name, vendor, d.DedicatedVideoMemory < 512 * 1024 * 1024));
            }
        }
    }
    out
}

impl Real {
    fn with_gpus<R>(&self, f: impl FnOnce(&Gpus) -> R) -> R {
        let mut g = self.gpu.lock().unwrap_or_else(|e| e.into_inner());
        let gpus = g.get_or_insert_with(load_gpus);
        f(gpus)
    }

    fn with_driver<R>(&self, card: &str, f: impl FnOnce(&dyn Driver) -> Option<R>) -> Option<R> {
        self.with_gpus(|g| g.drivers.iter().find(|d| d.cards().iter().any(|c| c.id == card)).and_then(|d| f(d.as_ref())))
    }

    fn reg_values_impl(&self, hive: Hive, path: &str) -> Option<Vec<Raw>> {
        use windows::Win32::System::Registry::{KEY_READ, RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW};
        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(root(hive), &HSTRING::from(path), None, KEY_READ, &mut key) != ERROR_SUCCESS {
                return None;
            }
            let (mut count, mut max_name, mut max_data) = (0u32, 0u32, 0u32);
            let _ = RegQueryInfoKeyW(key, None, None, None, None, None, None, Some(&mut count), Some(&mut max_name), Some(&mut max_data), None, None);
            let mut out = Vec::new();
            for i in 0..count {
                let mut name = vec![0u16; max_name as usize + 2];
                let mut name_len = name.len() as u32;
                let mut data = vec![0u8; max_data as usize + 2];
                let mut data_len = data.len() as u32;
                let mut ty = 0u32;
                let r = RegEnumValueW(
                    key,
                    i,
                    Some(windows::core::PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut ty),
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len),
                );
                if r != ERROR_SUCCESS {
                    continue;
                }
                data.truncate(data_len as usize);
                out.push(Raw { name: String::from_utf16_lossy(&name[..name_len as usize]), ty, data });
            }
            let _ = RegCloseKey(key);
            Some(out)
        }
    }
}

fn powercfg(args: &[&std::ffi::OsStr]) -> Res {
    use std::os::windows::process::CommandExt;
    let exe = std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into()).join("System32").join("powercfg.exe");
    let out = std::process::Command::new(exe).args(args).creation_flags(0x0800_0000).output().map_err(|e| Fail::Other(e.to_string()))?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stdout).trim().to_string() + String::from_utf8_lossy(&out.stderr).trim();
        let low = msg.to_lowercase();
        if low.contains("access") || low.contains("administrat") { Err(Fail::NeedsAdmin) } else { Err(Fail::Other(msg)) }
    }
}

/// Seconds since 1970 as "2026-11-11T10:00:00.000Z" (the event log's time format).
pub fn iso_utc(t: u64) -> String {
    let days = (t / 86_400) as i64;
    let rem = t % 86_400;
    // civil date from a day count (Howard Hinnant's public-domain algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

pub fn filetime_to_unix(ft: u64) -> u64 {
    (ft / 10_000_000).saturating_sub(11_644_473_600)
}

#[cfg(test)]
mod tests {
    #[test]
    fn event_log_times() {
        assert_eq!(super::iso_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(super::iso_utc(1_762_855_200), "2025-11-11T10:00:00.000Z");
        assert_eq!(super::filetime_to_unix(116_444_736_000_000_000), 0);
    }
}
