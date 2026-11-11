//! Programs that start with Windows: the Run keys of the registry, the Startup folders and the scheduled tasks that
//! run at logon. Turning one off works the way Windows' own settings do: a "StartupApproved" value next to it (the
//! entry itself stays), or the task's Enabled flag.
//!
//! Tests point Plume Task at a test registry key (PLUME_TASK_STARTUP_KEY, under HKEY_CURRENT_USER) and a temporary
//! folder (PLUME_TASK_STARTUP_DIR): then only those are read and changed, and scheduled tasks are left out.

use crate::i18n::tr;
use windows::{
    Win32::{
        Foundation::{ERROR_SUCCESS, VARIANT_FALSE, VARIANT_TRUE},
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            Registry::{
                HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_BINARY, REG_VALUE_TYPE, RRF_RT_REG_EXPAND_SZ,
                RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegEnumValueW, RegGetValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
            },
            TaskScheduler::{IExecAction, ITaskFolder, ITaskService, TASK_ACTION_EXEC, TASK_TRIGGER_LOGON, TaskScheduler},
            Variant::{VARIANT, VT_I4},
        },
    },
    core::{BSTR, HSTRING, Interface, PCWSTR, PWSTR},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// HKEY_CURRENT_USER Run key.
    RunUser,
    /// HKEY_LOCAL_MACHINE Run key (64-bit view, then the 32-bit one).
    RunMachine,
    RunMachine32,
    FolderUser,
    FolderCommon,
    Task,
}

impl Kind {
    pub fn place(self) -> &'static str {
        tr(match self {
            Kind::RunUser => "Registry, your account",
            Kind::RunMachine | Kind::RunMachine32 => "Registry, all accounts",
            Kind::FolderUser => "Startup folder",
            Kind::FolderCommon => "Startup folder, all accounts",
            Kind::Task => "Scheduled task at sign-in",
        })
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: Kind,
    /// The registry value, the file in the folder, or the task's full path.
    pub id: String,
    pub name: String,
    pub command: String,
    /// The program it starts (for its icon and publisher).
    pub exe: Option<String>,
    pub publisher: String,
    pub enabled: bool,
}

/// Where startup entries are read from.
pub struct Source {
    test_key: Option<String>,
    test_dir: Option<std::path::PathBuf>,
}

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open_key(root: HKEY, path: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<Key> {
    let mut k = HKEY::default();
    let p = w(path);
    (unsafe { RegOpenKeyExW(root, PCWSTR(p.as_ptr()), None, access | KEY_WOW64_64KEY, &mut k) } == ERROR_SUCCESS).then_some(Key(k))
}

/// A text value of the registry.
pub fn reg_string(root: HKEY, path: &str, name: &str) -> Option<String> {
    let (p, n) = (w(path), w(name));
    let mut buf = vec![0u16; 512];
    let mut size = (buf.len() * 2) as u32;
    let st = unsafe { RegGetValueW(root, PCWSTR(p.as_ptr()), PCWSTR(n.as_ptr()), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) };
    if st != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len.min(buf.len())]))
}

/// Every text value of a key: (name, data, data with %VARIABLES% expanded).
fn values(root: HKEY, path: &str) -> Vec<(String, String)> {
    let Some(k) = open_key(root, path, KEY_READ) else { return Vec::new() };
    let mut out = Vec::new();
    for i in 0.. {
        let mut name = vec![0u16; 16384];
        let mut nlen = name.len() as u32;
        let mut kind = 0u32;
        let mut data = vec![0u8; 8192];
        let mut dlen = data.len() as u32;
        let st = unsafe { RegEnumValueW(k.0, i, Some(PWSTR(name.as_mut_ptr())), &mut nlen, None, Some(&mut kind), Some(data.as_mut_ptr()), Some(&mut dlen)) };
        if st != ERROR_SUCCESS {
            break;
        }
        // REG_SZ (1) or REG_EXPAND_SZ (2)
        if kind != 1 && kind != 2 {
            continue;
        }
        let words: Vec<u16> = data[..dlen as usize].chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
        let text = String::from_utf16_lossy(&words).trim_end_matches('\0').to_string();
        let text = if kind == 2 { expand(&text) } else { text };
        out.push((String::from_utf16_lossy(&name[..nlen as usize]), text));
    }
    let _ = RRF_RT_REG_EXPAND_SZ;
    out
}

/// "%ProgramFiles%\x" → "C:\Program Files\x".
fn expand(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find('%') {
        let Some(b) = rest[a + 1..].find('%') else { break };
        out.push_str(&rest[..a]);
        let var = &rest[a + 1..a + 1 + b];
        out.push_str(&std::env::var(var).unwrap_or_else(|_| format!("%{var}%")));
        rest = &rest[a + b + 2..];
    }
    out.push_str(rest);
    out
}

/// Whether the "StartupApproved" value says the entry is on (no value: on). Odd first byte: turned off.
pub fn approved_on(data: Option<&[u8]>) -> bool {
    data.and_then(|d| d.first()).is_none_or(|b| b & 1 == 0)
}

/// The value written to turn an entry on or off (the off value also says when, like Windows writes it).
pub fn approved_value(on: bool, now_filetime: u64) -> [u8; 12] {
    let mut v = [0u8; 12];
    v[0] = if on { 2 } else { 3 };
    if !on {
        v[4..12].copy_from_slice(&now_filetime.to_le_bytes());
    }
    v
}

fn read_binary(root: HKEY, path: &str, name: &str) -> Option<Vec<u8>> {
    let k = open_key(root, path, KEY_READ)?;
    let n = w(name);
    let mut data = vec![0u8; 64];
    let mut len = data.len() as u32;
    let mut kind = REG_VALUE_TYPE::default();
    let st = unsafe { RegQueryValueExW(k.0, PCWSTR(n.as_ptr()), None, Some(&mut kind), Some(data.as_mut_ptr()), Some(&mut len)) };
    (st == ERROR_SUCCESS).then(|| {
        data.truncate(len as usize);
        data
    })
}

/// The program a command starts: `"C:\a b\x.exe" -flag` or `C:\a\x.exe -flag` → the path.
pub fn exe_of(command: &str) -> Option<String> {
    let c = command.trim();
    if let Some(rest) = c.strip_prefix('"') {
        return rest.find('"').map(|e| rest[..e].to_string());
    }
    let lower = c.to_lowercase();
    if let Some(i) = lower.find(".exe") {
        return Some(c[..i + 4].to_string());
    }
    c.split_whitespace().next().map(str::to_string)
}

impl Source {
    pub fn from_env() -> Source {
        Source {
            test_key: std::env::var("PLUME_TASK_STARTUP_KEY").ok().filter(|s| !s.is_empty()),
            test_dir: std::env::var_os("PLUME_TASK_STARTUP_DIR").map(Into::into),
        }
    }

    pub fn is_test(&self) -> bool {
        self.test_key.is_some()
    }

    /// (root, Run key, approved root, approved key) for a registry kind.
    fn run_place(&self, kind: Kind) -> Option<(HKEY, String, HKEY, String)> {
        if let Some(t) = &self.test_key {
            return (kind == Kind::RunUser).then(|| (HKEY_CURRENT_USER, format!(r"{t}\Run"), HKEY_CURRENT_USER, format!(r"{t}\StartupApproved\Run")));
        }
        match kind {
            Kind::RunUser => Some((HKEY_CURRENT_USER, RUN.into(), HKEY_CURRENT_USER, format!(r"{APPROVED}\Run"))),
            Kind::RunMachine => Some((HKEY_LOCAL_MACHINE, RUN.into(), HKEY_LOCAL_MACHINE, format!(r"{APPROVED}\Run"))),
            Kind::RunMachine32 => Some((HKEY_LOCAL_MACHINE, RUN32.into(), HKEY_LOCAL_MACHINE, format!(r"{APPROVED}\Run32"))),
            _ => None,
        }
    }

    /// (folder, approved root, approved key) for a folder kind.
    fn folder_place(&self, kind: Kind) -> Option<(std::path::PathBuf, HKEY, String)> {
        if let Some(t) = &self.test_key {
            return (kind == Kind::FolderUser)
                .then(|| self.test_dir.clone().map(|d| (d, HKEY_CURRENT_USER, format!(r"{t}\StartupApproved\StartupFolder"))))
                .flatten();
        }
        let env = |v: &str| std::env::var_os(v).map(std::path::PathBuf::from);
        match kind {
            Kind::FolderUser => {
                env("APPDATA").map(|a| (a.join(r"Microsoft\Windows\Start Menu\Programs\Startup"), HKEY_CURRENT_USER, format!(r"{APPROVED}\StartupFolder")))
            }
            Kind::FolderCommon => {
                env("ProgramData").map(|a| (a.join(r"Microsoft\Windows\Start Menu\Programs\StartUp"), HKEY_LOCAL_MACHINE, format!(r"{APPROVED}\StartupFolder")))
            }
            _ => None,
        }
    }

    /// Every startup entry. Slow-ish (the scheduled tasks): call it off the window's thread, COM ready.
    pub fn list(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        for kind in [Kind::RunUser, Kind::RunMachine, Kind::RunMachine32] {
            let Some((root, key, aroot, akey)) = self.run_place(kind) else { continue };
            for (name, cmd) in values(root, &key) {
                let exe = exe_of(&cmd);
                let enabled = approved_on(read_binary(aroot, &akey, &name).as_deref());
                out.push(Entry { kind, id: name.clone(), name, command: cmd, exe, publisher: String::new(), enabled });
            }
        }
        for kind in [Kind::FolderUser, Kind::FolderCommon] {
            let Some((dir, aroot, akey)) = self.folder_place(kind) else { continue };
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for f in rd.flatten() {
                let file = f.file_name().to_string_lossy().to_string();
                if file.eq_ignore_ascii_case("desktop.ini") || !f.path().is_file() {
                    continue;
                }
                let name = std::path::Path::new(&file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(file.clone());
                let path = f.path().to_string_lossy().to_string();
                let enabled = approved_on(read_binary(aroot, &akey, &file).as_deref());
                out.push(Entry { kind, id: file, name, command: path.clone(), exe: Some(path), publisher: String::new(), enabled });
            }
        }
        if !self.is_test() {
            out.extend(logon_tasks());
        }
        for e in &mut out {
            if let Some(exe) = &e.exe
                && let Some((desc, company)) = crate::sys::file_info(exe)
            {
                if matches!(e.kind, Kind::RunUser | Kind::RunMachine | Kind::RunMachine32) && !desc.is_empty() {
                    e.name = desc;
                }
                e.publisher = company;
            }
        }
        out.sort_by_key(|e| e.name.to_lowercase());
        out
    }

    /// Turns an entry on or off. Err: a short reason for the user.
    pub fn set_enabled(&self, e: &Entry, on: bool) -> Result<(), String> {
        let (aroot, akey) = match e.kind {
            Kind::Task => return set_task(&e.id, on),
            Kind::RunUser | Kind::RunMachine | Kind::RunMachine32 => self.run_place(e.kind).map(|(_, _, r, k)| (r, k)),
            Kind::FolderUser | Kind::FolderCommon => self.folder_place(e.kind).map(|(_, r, k)| (r, k)),
        }
        .ok_or_else(|| tr("This entry can't be changed here.").to_string())?;
        let ft = (crate::sys::now_100ns()) as u64;
        let v = approved_value(on, ft);
        let p = w(&akey);
        let mut k = HKEY::default();
        let st = unsafe {
            RegCreateKeyExW(aroot, PCWSTR(p.as_ptr()), None, PCWSTR::null(), Default::default(), KEY_SET_VALUE | KEY_WOW64_64KEY, None, &mut k, None)
        };
        if st != ERROR_SUCCESS {
            return Err(tr("Windows refused: this one needs administrator rights.").to_string());
        }
        let k = Key(k);
        let n = w(&e.id);
        let st = unsafe { RegSetValueExW(k.0, PCWSTR(n.as_ptr()), None, REG_BINARY, Some(&v)) };
        if st != ERROR_SUCCESS {
            return Err(tr("Windows refused: this one needs administrator rights.").to_string());
        }
        Ok(())
    }
}

fn variant_i4(v: i32) -> VARIANT {
    let mut x = VARIANT::default();
    unsafe {
        let inner = &mut *x.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = v;
    }
    x
}

fn service() -> Option<ITaskService> {
    unsafe {
        let s: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).ok()?;
        let e = VARIANT::default();
        s.Connect(&e, &e, &e, &e).ok()?;
        Some(s)
    }
}

/// The scheduled tasks that start a program when someone signs in (Windows' own tasks, under \Microsoft, left out).
fn logon_tasks() -> Vec<Entry> {
    let mut out = Vec::new();
    let Some(s) = service() else { return out };
    let Ok(root) = (unsafe { s.GetFolder(&BSTR::from("\\")) }) else { return out };
    walk(&root, &mut out, 0);
    out
}

fn walk(folder: &ITaskFolder, out: &mut Vec<Entry>, depth: u32) {
    unsafe {
        // 1: hidden tasks too
        if let Ok(tasks) = folder.GetTasks(1) {
            for i in 1..=tasks.Count().unwrap_or(0) {
                let Ok(t) = tasks.get_Item(&variant_i4(i)) else { continue };
                let Ok(def) = t.Definition() else { continue };
                let Ok(triggers) = def.Triggers() else { continue };
                let mut n = 0;
                let _ = triggers.Count(&mut n);
                let logon = (1..=n).any(|j| {
                    triggers.get_Item(j).is_ok_and(|tr| {
                        let mut ty = Default::default();
                        tr.Type(&mut ty).is_ok() && ty == TASK_TRIGGER_LOGON
                    })
                });
                if !logon {
                    continue;
                }
                let mut command = String::new();
                let mut exe = None;
                if let Ok(actions) = def.Actions() {
                    let mut an = 0;
                    let _ = actions.Count(&mut an);
                    if an >= 1
                        && let Ok(a) = actions.get_Item(1)
                    {
                        let mut ty = Default::default();
                        if a.Type(&mut ty).is_ok()
                            && ty == TASK_ACTION_EXEC
                            && let Ok(x) = a.cast::<IExecAction>()
                        {
                            let (mut p, mut args) = (BSTR::new(), BSTR::new());
                            let _ = x.Path(&mut p);
                            let _ = x.Arguments(&mut args);
                            let p = expand(p.to_string().trim_matches('"'));
                            command = format!("\"{p}\" {args}").trim().to_string();
                            exe = Some(p);
                        }
                    }
                }
                let name = t.Name().map(|b| b.to_string()).unwrap_or_default();
                let path = t.Path().map(|b| b.to_string()).unwrap_or_default();
                let enabled = t.Enabled().is_ok_and(|v| v.as_bool());
                out.push(Entry { kind: Kind::Task, id: path, name, command, exe, publisher: String::new(), enabled });
            }
        }
        if depth > 6 {
            return;
        }
        if let Ok(sub) = folder.GetFolders(0) {
            for i in 1..=sub.Count().unwrap_or(0) {
                let Ok(f) = sub.get_Item(&variant_i4(i)) else { continue };
                if f.Path().is_ok_and(|p| p.to_string().eq_ignore_ascii_case(r"\Microsoft")) {
                    continue;
                }
                walk(&f, out, depth + 1);
            }
        }
    }
}

fn set_task(path: &str, on: bool) -> Result<(), String> {
    let refused = || tr("Windows refused: this one needs administrator rights.").to_string();
    let s = service().ok_or_else(refused)?;
    unsafe {
        let root = s.GetFolder(&BSTR::from("\\")).map_err(|_| refused())?;
        let t = root.GetTask(&BSTR::from(path)).map_err(|_| refused())?;
        t.SetEnabled(if on { VARIANT_TRUE } else { VARIANT_FALSE }).map_err(|_| refused())
    }
}

#[allow(dead_code)]
const _H: Option<HSTRING> = None;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        assert_eq!(exe_of(r#""C:\Program Files\App\app.exe" --min"#).as_deref(), Some(r"C:\Program Files\App\app.exe"));
        assert_eq!(exe_of(r"C:\Tools\x.EXE /background").as_deref(), Some(r"C:\Tools\x.EXE"));
        assert_eq!(exe_of("rundll32 thing").as_deref(), Some("rundll32"));
        assert!(expand("%NO_SUCH_VAR_PT%\\a").starts_with("%NO_SUCH_VAR_PT%"));
    }

    #[test]
    fn approved_bytes() {
        assert!(approved_on(None));
        assert!(approved_on(Some(&[2, 0, 0, 0])));
        assert!(approved_on(Some(&[6, 0])));
        assert!(!approved_on(Some(&[3, 0, 0, 0])));
        assert!(!approved_on(Some(&[7])));
        let off = approved_value(false, 0x0102_0304_0506_0708);
        assert_eq!(off[0], 3);
        assert_eq!(&off[4..], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(approved_value(true, 99), [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_key_round_trip() {
        // only a test key of this test, removed at the end
        let key = format!(r"Software\PlumeTaskUnitTest{}", std::process::id());
        let p = w(&format!(r"{key}\Run"));
        let mut k = HKEY::default();
        unsafe {
            RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(p.as_ptr()), None, PCWSTR::null(), Default::default(), KEY_SET_VALUE, None, &mut k, None).ok().unwrap();
            let data: Vec<u8> = w(r#""C:\Nowhere\dummy.exe" -x"#).iter().flat_map(|c| c.to_le_bytes()).collect();
            let n = w("Dummy");
            RegSetValueExW(k, PCWSTR(n.as_ptr()), None, windows::Win32::System::Registry::REG_SZ, Some(&data)).ok().unwrap();
            let _ = RegCloseKey(k);
        }
        let src = Source { test_key: Some(key.clone()), test_dir: None };
        let list = src.list();
        assert_eq!(list.len(), 1);
        assert!(list[0].enabled && list[0].exe.as_deref() == Some(r"C:\Nowhere\dummy.exe"));
        src.set_enabled(&list[0], false).unwrap();
        assert!(!src.list()[0].enabled);
        src.set_enabled(&list[0], true).unwrap();
        assert!(src.list()[0].enabled);
        let kp = w(&key);
        unsafe {
            let _ = windows::Win32::System::Registry::RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(kp.as_ptr()));
        }
    }
}
