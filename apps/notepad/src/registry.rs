//! What Plume Notepad writes in the registry, as plans of small operations that can be checked before they are
//! applied, and applied on a test root in tests:
//!
//! - replacing Windows Notepad: a `Debugger` value under Image File Execution Options\notepad.exe (HKLM, needs
//!   the administrator's consent). Windows then starts Plume Notepad with the path of notepad.exe and its
//!   arguments after ours. Windows 11 keeps filters in subkeys there (the Store Notepad): the ones for
//!   notepad.exe get the same value.
//! - "Open with" and Default apps: a ProgID and the app's capabilities under HKCU\Software\Classes.
//! - the pads at sign-in: a value in HKCU\...\Run.

use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW,
            RegDeleteKeyValueW, RegDeleteKeyW, RegDeleteTreeW, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegSetValueExW,
        },
    },
    core::{HSTRING, PWSTR},
};

pub const IFEO: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\notepad.exe";
pub const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE: &str = "Plume Notepad";
pub const PROGID: &str = "PlumeNotepad.txt";
const CLASSES: &str = r"Software\Classes";
const CAPS: &str = r"Software\Plume\Notepad\Capabilities";
const REGISTERED: &str = r"Software\RegisteredApplications";
pub const APP_NAME: &str = "Plume Notepad";
/// The kinds of files Plume Notepad offers to open ("Open with", Default apps).
pub const TYPES: [&str; 10] = [".txt", ".log", ".ini", ".md", ".csv", ".json", ".xml", ".cfg", ".conf", ".nfo"];

/// Where the keys go: a real hive, or a key under HKCU for the tests (`prefix`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub machine: bool,
    pub prefix: String,
}

impl Root {
    #[cfg_attr(feature = "test-hook", allow(dead_code))]
    pub fn machine() -> Root {
        Root { machine: true, prefix: String::new() }
    }
    #[cfg_attr(feature = "test-hook", allow(dead_code))]
    pub fn user() -> Root {
        Root { machine: false, prefix: String::new() }
    }
    /// HKCU\<prefix>: a test root, never the real keys.
    #[cfg_attr(not(any(test, feature = "test-hook")), allow(dead_code))]
    pub fn test(prefix: &str) -> Root {
        Root { machine: false, prefix: prefix.trim_matches('\\').to_string() }
    }
    fn hive(&self) -> HKEY {
        if self.machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER }
    }
    fn path(&self, key: &str) -> String {
        if self.prefix.is_empty() { key.to_string() } else { format!(r"{}\{}", self.prefix, key) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// A string value ("" is the key's default value).
    Set { key: String, name: String, value: String },
    /// A value removed (nothing if it is not there).
    Delete { key: String, name: String },
    /// The key removed if nothing is left in it.
    DropIfEmpty { key: String },
    /// The key removed with everything under it (only keys that are Plume Notepad's own).
    DropTree { key: String },
}

fn set(key: &str, name: &str, value: &str) -> Op {
    Op::Set { key: key.into(), name: name.into(), value: value.into() }
}

// ---- reading ----

pub fn get(root: &Root, key: &str, name: &str) -> Option<String> {
    let (k, n) = (HSTRING::from(root.path(key)), HSTRING::from(name));
    unsafe {
        let mut size = 0u32;
        if RegGetValueW(root.hive(), &k, &n, RRF_RT_REG_SZ, None, None, Some(&mut size)) != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2) + 1];
        let mut size = (buf.len() * 2) as u32;
        if RegGetValueW(root.hive(), &k, &n, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

fn open(root: &Root, key: &str) -> Option<HKEY> {
    let mut h = HKEY::default();
    unsafe { (RegOpenKeyExW(root.hive(), &HSTRING::from(root.path(key)), None, KEY_READ, &mut h) == ERROR_SUCCESS).then_some(h) }
}

pub fn exists(root: &Root, key: &str) -> bool {
    open(root, key).map(|h| unsafe { RegCloseKey(h) }).is_some()
}

/// The names of the subkeys of a key.
pub fn subkeys(root: &Root, key: &str) -> Vec<String> {
    let Some(h) = open(root, key) else { return vec![] };
    let mut out = Vec::new();
    unsafe {
        for i in 0.. {
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            if RegEnumKeyExW(h, i, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None) != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        let _ = RegCloseKey(h);
    }
    out
}

/// (values, subkeys) of a key.
fn counts(root: &Root, key: &str) -> Option<(u32, u32)> {
    let h = open(root, key)?;
    let (mut keys, mut values) = (0u32, 0u32);
    let r = unsafe { RegQueryInfoKeyW(h, None, None, None, Some(&mut keys), None, None, Some(&mut values), None, None, None, None) };
    unsafe {
        let _ = RegCloseKey(h);
    }
    (r == ERROR_SUCCESS).then_some((values, keys))
}

// ---- applying ----

fn check(r: WIN32_ERROR, what: &str) -> Result<(), String> {
    if r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(format!("{what}: error {}", r.0)) }
}

pub fn apply(root: &Root, ops: &[Op]) -> Result<(), String> {
    for op in ops {
        unsafe {
            match op {
                Op::Set { key, name, value } => {
                    let mut h = HKEY::default();
                    let r = RegCreateKeyExW(root.hive(), &HSTRING::from(root.path(key)), None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut h, None);
                    if r != ERROR_SUCCESS {
                        return Err(format!("{key}: error {}", r.0));
                    }
                    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
                    let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                    let r = RegSetValueExW(h, &HSTRING::from(name.as_str()), None, REG_SZ, Some(bytes));
                    let _ = RegCloseKey(h);
                    if r != ERROR_SUCCESS {
                        return Err(format!("{key}\\{name}: error {}", r.0));
                    }
                }
                Op::Delete { key, name } => {
                    check(RegDeleteKeyValueW(root.hive(), &HSTRING::from(root.path(key)), &HSTRING::from(name.as_str())), key)?;
                }
                Op::DropIfEmpty { key } => {
                    if counts(root, key) == Some((0, 0)) {
                        check(RegDeleteKeyW(root.hive(), &HSTRING::from(root.path(key))), key)?;
                    }
                }
                Op::DropTree { key } => {
                    if exists(root, key) {
                        check(RegDeleteTreeW(root.hive(), &HSTRING::from(root.path(key))), key)?;
                        check(RegDeleteKeyW(root.hive(), &HSTRING::from(root.path(key))), key)?;
                    }
                }
            }
        }
    }
    Ok(())
}

// ---- replacing Windows Notepad ----

/// The value Windows runs instead of notepad.exe. It adds the path of notepad.exe and its arguments after it:
/// `--ifeo` tells Plume Notepad to skip that path.
pub fn debugger_cmd(exe: &str) -> String {
    format!("\"{exe}\" --ifeo")
}

pub fn is_ours(v: &str) -> bool {
    v.to_ascii_lowercase().contains("plume-notepad")
}

/// What the notepad.exe key holds now: its Debugger, and its filters (subkey, path filtered, Debugger).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IfeoNow {
    pub exists: bool,
    pub debugger: Option<String>,
    pub filters: Vec<(String, Option<String>, Option<String>)>,
}

pub fn read_ifeo(root: &Root) -> IfeoNow {
    if !exists(root, IFEO) {
        return IfeoNow::default();
    }
    let filters = subkeys(root, IFEO)
        .into_iter()
        .map(|k| {
            let sub = format!(r"{IFEO}\{k}");
            let path = get(root, &sub, "FilterFullPath");
            let dbg = get(root, &sub, "Debugger");
            (k, path, dbg)
        })
        .collect();
    IfeoNow { exists: true, debugger: get(root, IFEO, "Debugger"), filters }
}

/// Is Plume Notepad the one Windows starts for notepad.exe?
pub fn ifeo_on(now: &IfeoNow) -> bool {
    now.debugger.as_deref().is_some_and(is_ours)
}

/// The operations that turn the redirection on (with Plume Notepad's command) or off. Turning off removes only
/// Plume Notepad's own values, and the key only if Plume Notepad left it empty.
pub fn ifeo_plan(on: bool, now: &IfeoNow, cmd: &str) -> Vec<Op> {
    let mut ops = Vec::new();
    let notepad_filter = |p: &Option<String>| p.as_deref().is_some_and(|p| p.to_ascii_lowercase().ends_with("notepad.exe"));
    if on {
        ops.push(set(IFEO, "Debugger", cmd));
        for (k, path, _) in &now.filters {
            if notepad_filter(path) {
                ops.push(set(&format!(r"{IFEO}\{k}"), "Debugger", cmd));
            }
        }
    } else {
        for (k, _, dbg) in &now.filters {
            if dbg.as_deref().is_some_and(is_ours) {
                ops.push(Op::Delete { key: format!(r"{IFEO}\{k}"), name: "Debugger".into() });
            }
        }
        if now.debugger.as_deref().is_some_and(is_ours) {
            ops.push(Op::Delete { key: IFEO.into(), name: "Debugger".into() });
            ops.push(Op::DropIfEmpty { key: IFEO.into() });
        }
    }
    ops
}

/// The rest of the command line Windows gives when it starts Plume Notepad instead of notepad.exe:
/// `<notepad.exe path> [notepad options] [file]`. Returns the file, if any (it may be unquoted, with spaces).
pub fn ifeo_file(rest: &str) -> Option<String> {
    let rest = rest.trim_start();
    // the path of notepad.exe (quoted or not)
    let after = if let Some(r) = rest.strip_prefix('"') {
        r.split_once('"').map_or("", |(_, b)| b)
    } else {
        rest.split_once(char::is_whitespace).map_or("", |(_, b)| b)
    };
    let mut s = after.trim();
    // notepad's own switches: /A /W (encoding), /P (print), /PT printer driver port, /.SETUP
    loop {
        let lower = s.to_ascii_lowercase();
        if let Some(r) = ["/a ", "/w ", "/p ", "/.setup"].iter().find_map(|sw| lower.starts_with(sw).then(|| &s[sw.len()..])) {
            s = r.trim_start();
        } else if lower.starts_with("/pt ") {
            // the file is followed by the printer: keep only the file
            s = s[4..].trim_start();
            if let Some(r) = s.strip_prefix('"') {
                return r.split_once('"').map(|(f, _)| f.to_string()).filter(|f| !f.is_empty());
            }
            return s.split_whitespace().next().map(str::to_string);
        } else {
            break;
        }
    }
    let s = s.trim().trim_matches('"').trim();
    (!s.is_empty()).then(|| s.to_string())
}

// ---- Open with, Default apps ----

/// The ProgID, "Open with" entries and the capabilities Default apps lists, for `exe`.
pub fn assoc_plan(on: bool, exe: &str) -> Vec<Op> {
    let progid = format!(r"{CLASSES}\{PROGID}");
    let app = format!(r"{CLASSES}\Applications\plume-notepad.exe");
    let open = format!("\"{exe}\" \"%1\"");
    if !on {
        let mut ops = vec![Op::DropTree { key: progid }, Op::DropTree { key: app }, Op::Delete { key: REGISTERED.into(), name: APP_NAME.into() }];
        ops.push(Op::DropTree { key: r"Software\Plume\Notepad".into() });
        ops.push(Op::DropIfEmpty { key: r"Software\Plume".into() });
        for t in TYPES {
            let key = format!(r"{CLASSES}\{t}\OpenWithProgids");
            ops.push(Op::Delete { key: key.clone(), name: PROGID.into() });
            ops.push(Op::DropIfEmpty { key });
            ops.push(Op::DropIfEmpty { key: format!(r"{CLASSES}\{t}") });
        }
        return ops;
    }
    let mut ops = vec![
        set(&progid, "", "Text document"),
        set(&format!(r"{progid}\DefaultIcon"), "", &format!("\"{exe}\",0")),
        set(&format!(r"{progid}\shell\open\command"), "", &open),
        set(&app, "FriendlyAppName", APP_NAME),
        set(&format!(r"{app}\shell\open\command"), "", &open),
        set(CAPS, "ApplicationName", APP_NAME),
        set(CAPS, "ApplicationDescription", "A fast plain-text editor, with sticky pads."),
        set(REGISTERED, APP_NAME, CAPS),
    ];
    for t in TYPES {
        ops.push(set(&format!(r"{CLASSES}\{t}\OpenWithProgids"), PROGID, ""));
        ops.push(set(&format!(r"{app}\SupportedTypes"), t, ""));
        ops.push(set(&format!(r"{CAPS}\FileAssociations"), t, PROGID));
    }
    ops
}

#[cfg_attr(not(any(test, feature = "test-hook")), allow(dead_code))]
pub fn assoc_on(root: &Root) -> bool {
    get(root, REGISTERED, APP_NAME).is_some()
}

// ---- pads at sign-in ----

pub fn run_plan(on: bool, exe: &str) -> Vec<Op> {
    if on { vec![set(RUN, RUN_VALUE, &format!("\"{exe}\" --background"))] } else { vec![Op::Delete { key: RUN.into(), name: RUN_VALUE.into() }] }
}

pub fn run_on(root: &Root) -> bool {
    get(root, RUN, RUN_VALUE).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh test root under HKCU, removed at the end.
    struct TestRoot(Root);
    impl TestRoot {
        fn new(name: &str) -> TestRoot {
            let r = Root::test(&format!(r"Software\PlumeNotepadUnitTest\{name}-{}", std::process::id()));
            let _ = apply(&Root::user(), &[Op::DropTree { key: r.prefix.clone() }]);
            TestRoot(r)
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = apply(&Root::user(), &[Op::DropTree { key: self.0.prefix.clone() }, Op::DropIfEmpty { key: r"Software\PlumeNotepadUnitTest".into() }]);
        }
    }

    const EXE: &str = r"C:\Users\x\AppData\Local\Plume\plume-notepad.exe";

    #[test]
    fn ifeo_on_and_off_on_an_empty_key() {
        let t = TestRoot::new("ifeo1");
        let root = &t.0;
        let now = read_ifeo(root);
        assert!(!now.exists && !ifeo_on(&now));
        let plan = ifeo_plan(true, &now, &debugger_cmd(EXE));
        assert_eq!(plan, vec![set(IFEO, "Debugger", &format!("\"{EXE}\" --ifeo"))]);
        apply(root, &plan).unwrap();
        let now = read_ifeo(root);
        assert!(ifeo_on(&now));
        apply(root, &ifeo_plan(false, &now, "")).unwrap();
        // the key was Plume Notepad's alone: it is gone
        assert!(!exists(root, IFEO));
        assert!(!ifeo_on(&read_ifeo(root)));
    }

    #[test]
    fn ifeo_keeps_what_windows_put_there() {
        let t = TestRoot::new("ifeo2");
        let root = &t.0;
        // like Windows 11: a filter for the Store Notepad, and another value
        apply(
            root,
            &[
                set(IFEO, "UseFilter", "1"),
                set(&format!(r"{IFEO}\0"), "FilterFullPath", r"C:\Windows\System32\notepad.exe"),
                set(&format!(r"{IFEO}\0"), "AppExecutionAliasRedirect", "1"),
                set(&format!(r"{IFEO}\1"), "FilterFullPath", r"C:\Other\tool.exe"),
            ],
        )
        .unwrap();
        let now = read_ifeo(root);
        let plan = ifeo_plan(true, &now, &debugger_cmd(EXE));
        assert_eq!(plan.len(), 2, "{plan:?}");
        apply(root, &plan).unwrap();
        assert!(get(root, &format!(r"{IFEO}\0"), "Debugger").is_some_and(|v| is_ours(&v)));
        assert_eq!(get(root, &format!(r"{IFEO}\1"), "Debugger"), None);
        apply(root, &ifeo_plan(false, &read_ifeo(root), "")).unwrap();
        let now = read_ifeo(root);
        assert!(now.exists && now.debugger.is_none());
        assert_eq!(get(root, IFEO, "UseFilter").as_deref(), Some("1"));
        assert_eq!(get(root, &format!(r"{IFEO}\0"), "AppExecutionAliasRedirect").as_deref(), Some("1"));
        assert_eq!(get(root, &format!(r"{IFEO}\0"), "Debugger"), None);
    }

    #[test]
    fn someone_elses_debugger_is_not_removed() {
        let t = TestRoot::new("ifeo3");
        let root = &t.0;
        apply(root, &[set(IFEO, "Debugger", r#""C:\Tools\other.exe" -z"#)]).unwrap();
        let plan = ifeo_plan(false, &read_ifeo(root), "");
        assert!(plan.is_empty());
    }

    #[test]
    fn command_lines_from_windows() {
        assert_eq!(ifeo_file(r#""C:\Windows\System32\notepad.exe" C:\Users\me\my notes.txt"#).as_deref(), Some(r"C:\Users\me\my notes.txt"));
        assert_eq!(ifeo_file(r#"C:\Windows\notepad.exe "D:\a b\c.log""#).as_deref(), Some(r"D:\a b\c.log"));
        assert_eq!(ifeo_file(r#"notepad.exe"#), None);
        assert_eq!(ifeo_file(r#""C:\Windows\System32\notepad.exe""#), None);
        assert_eq!(ifeo_file(r#"notepad /A file.txt"#).as_deref(), Some("file.txt"));
        assert_eq!(ifeo_file(r#"notepad /p "x y.txt""#).as_deref(), Some("x y.txt"));
        assert_eq!(ifeo_file(r#"notepad /pt "x y.txt" printer driver port"#).as_deref(), Some("x y.txt"));
    }

    #[test]
    fn run_key_and_associations() {
        let t = TestRoot::new("user");
        let root = &t.0;
        assert!(!run_on(root));
        apply(root, &run_plan(true, EXE)).unwrap();
        assert_eq!(get(root, RUN, RUN_VALUE), Some(format!("\"{EXE}\" --background")));
        apply(root, &run_plan(false, EXE)).unwrap();
        assert!(!run_on(root));
        // something else already in .txt stays
        apply(root, &[set(r"Software\Classes\.txt", "", "txtfile")]).unwrap();
        apply(root, &assoc_plan(true, EXE)).unwrap();
        assert!(assoc_on(root));
        assert_eq!(get(root, r"Software\Classes\PlumeNotepad.txt\shell\open\command", ""), Some(format!("\"{EXE}\" \"%1\"")));
        assert_eq!(get(root, r"Software\Classes\.md\OpenWithProgids", PROGID).as_deref(), Some(""));
        apply(root, &assoc_plan(false, EXE)).unwrap();
        assert!(!assoc_on(root));
        assert!(!exists(root, r"Software\Classes\PlumeNotepad.txt"));
        assert!(!exists(root, r"Software\Classes\.md"));
        assert_eq!(get(root, r"Software\Classes\.txt", "").as_deref(), Some("txtfile"));
    }
}
