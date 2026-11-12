//! Plume apps as Windows default apps, for the current user only.
//!
//! Since Windows 8 a program cannot make itself the default (UserChoice is protected by a hash): it can only
//! register as a capable handler, then send the user to Windows' own Default apps page, where one click on
//! "Set default" (Windows 11 23H2 and later) gives it every type it declared. This crate does the registration:
//!
//! - `Software\Classes\<ProgID>`: name, icon and open command of each file type;
//! - `Software\Classes\.<ext>\OpenWithProgids`: the ProgID offered for the extension;
//! - `Software\Plume\<App>\Capabilities`: ApplicationName, ApplicationDescription, ApplicationIcon and
//!   FileAssociations (what Default apps lists);
//! - `Software\RegisteredApplications`: the app's name pointing at those capabilities.
//!
//! It never writes UserChoice: it only reads it, to tell whether Windows opens the app's types with it ([`state`]).
//! The apps call [`switch`] first thing in `main` (`--register-defaults`, `--unregister-defaults`); Plume Suite reads
//! [`state`] and opens [`Spec::settings_link`]. `PLUME_DEFAULTS_ROOT` moves every key under another key of HKCU
//! (tests write there, never into the real classes, and Explorer is not told).

use std::{collections::HashSet, path::Path};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        Graphics::Imaging::{CLSID_WICImagingFactory, IWICBitmapCodecInfo, IWICImagingFactory, WICComponentEnumerateDefault, WICDecoder},
        System::{
            Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW,
                RegDeleteKeyW, RegDeleteTreeW, RegGetValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegSetValueExW,
            },
        },
        UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify},
    },
    core::{HSTRING, Interface},
};

/// The environment variable that moves every key under `HKCU\<value>` (tests).
pub const ROOT_VAR: &str = "PLUME_DEFAULTS_ROOT";
const CLASSES: &str = r"Software\Classes";
const REGISTERED: &str = r"Software\RegisteredApplications";
const FILE_EXTS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts";

/// One file type of an app: its ProgID, the name Windows shows for it, and the extensions it covers.
#[derive(Debug)]
pub struct ProgId {
    pub id: &'static str,
    pub name: &'static str,
    pub exts: &'static [&'static str],
}

/// What an app claims.
#[derive(Debug)]
pub struct Spec {
    /// The name shown in Default apps, and the value name in RegisteredApplications ("Plume Media").
    pub app: &'static str,
    /// `Software\Plume\<key>\Capabilities`.
    pub key: &'static str,
    pub exe: &'static str,
    pub description: &'static str,
    pub types: &'static [ProgId],
    /// The everyday types: the app is "the default" when Windows opens all of these with it.
    pub primary: &'static [&'static str],
    /// Types claimed only when a Windows Imaging Component decoder reads them (HEIC, AVIF, camera RAW…).
    pub needs_codec: &'static [&'static str],
}

/// Plume Media: every photo WIC can read (the codec-dependent ones only when the codec is there) and every video
/// Media Foundation plays. The same ProgIDs as its "Open with" entries (apps/media/src/openwith.rs), and the same
/// lists as apps/media/src/image.rs (PHOTO_EXT, VIDEO_EXT).
pub const MEDIA: Spec = Spec {
    app: "Plume Media",
    key: "Media",
    exe: "plume-media.exe",
    description: "Your photos and videos: an instant viewer, a timeline and a player.",
    types: &[
        ProgId {
            id: "PlumeMedia.Photo",
            name: "Photo (Plume Media)",
            exts: &[
                "jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "dib", "tif", "tiff", "webp", "heic", "heif", "avif", "ico", "jxr", "wdp", "cr2", "cr3",
                "nef", "nrw", "arw", "dng", "orf", "rw2", "raf", "pef", "srw",
            ],
        },
        ProgId {
            id: "PlumeMedia.Video",
            name: "Video (Plume Media)",
            exts: &["mp4", "m4v", "mov", "mkv", "avi", "wmv", "webm", "mpg", "mpeg", "ts", "m2ts", "mts", "3gp", "3g2", "asf"],
        },
    ],
    primary: &["jpg", "jpeg", "png", "gif", "webp", "mp4", "mov", "mkv"],
    needs_codec: &["webp", "heic", "heif", "avif", "cr2", "cr3", "nef", "nrw", "arw", "dng", "orf", "rw2", "raf", "pef", "srw"],
};

/// Plume Music: the files its library reads (apps/music/src/library.rs, AUDIO_EXT, without mp4: videos go to
/// Plume Media).
pub const MUSIC: Spec = Spec {
    app: "Plume Music",
    key: "Music",
    exe: "plume-music.exe",
    description: "Your music and YouTube Music, playlists and synced lyrics.",
    types: &[ProgId { id: "PlumeMusic.Audio", name: "Audio (Plume Music)", exts: &["mp3", "m4a", "aac", "flac", "ogg", "oga", "wav", "alac", "aiff"] }],
    primary: &["mp3", "m4a", "flac", "wav"],
    needs_codec: &[],
};

pub const PDF: Spec = Spec {
    app: "Plume PDF",
    key: "PDF",
    exe: "plume-pdf.exe",
    description: "Read, annotate, sign, merge and split PDF files.",
    types: &[ProgId { id: "PlumePDF.Document", name: "PDF document (Plume PDF)", exts: &["pdf"] }],
    primary: &["pdf"],
    needs_codec: &[],
};

/// Plume Notepad: the same keys as its own registry plan (apps/notepad/src/registry.rs, `assoc_plan`).
pub const NOTEPAD: Spec = Spec {
    app: "Plume Notepad",
    key: "Notepad",
    exe: "plume-notepad.exe",
    description: "A fast plain-text editor, with sticky pads.",
    types: &[ProgId { id: "PlumeNotepad.txt", name: "Text document", exts: &["txt", "log", "ini", "md", "csv", "json", "xml", "cfg", "conf", "nfo"] }],
    primary: &["txt"],
    needs_codec: &[],
};

/// Plume Cut's projects (apps/cut/src/project.rs, EXT).
pub const CUT: Spec = Spec {
    app: "Plume Cut",
    key: "Cut",
    exe: "plume-cut.exe",
    description: "Edit your videos: cut, arrange, add titles and music.",
    types: &[ProgId { id: "PlumeCut.Project", name: "Plume Cut project", exts: &["plumecut"] }],
    primary: &["plumecut"],
    needs_codec: &[],
};

/// Plume Create's projects.
pub const CREATE: Spec = Spec {
    app: "Plume Create",
    key: "Create",
    exe: "plume-create.exe",
    description: "Edit and create pictures, with layers.",
    types: &[ProgId { id: "PlumeCreate.Project", name: "Plume Create project", exts: &["plumecreate"] }],
    primary: &["plumecreate"],
    needs_codec: &[],
};

impl Spec {
    /// (extension, ProgID) for every type the app can claim.
    pub fn pairs(&self) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
        self.types.iter().flat_map(|t| t.exts.iter().map(move |e| (*e, t.id)))
    }
    pub fn caps_key(&self) -> String {
        format!(r"Software\Plume\{}\Capabilities", self.key)
    }
    /// Windows 11's Default apps page for this app (where "Set default" is).
    pub fn settings_link(&self) -> String {
        format!("ms-settings:defaultapps?registeredAppUser={}", self.app.replace(' ', "%20"))
    }
    /// Is this ProgID one of the app's?
    pub fn owns(&self, progid: &str) -> bool {
        self.types.iter().any(|t| t.id.eq_ignore_ascii_case(progid))
    }
}

// ───────────────────────── where the keys go ─────────────────────────

/// HKCU itself, or a key of HKCU standing for it (tests).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Root {
    prefix: String,
}

impl Root {
    pub fn user() -> Root {
        Root::default()
    }
    pub fn test(prefix: &str) -> Root {
        Root { prefix: prefix.trim_matches('\\').to_string() }
    }
    /// `PLUME_DEFAULTS_ROOT` when it is set, otherwise HKCU.
    pub fn from_env() -> Root {
        std::env::var(ROOT_VAR).ok().filter(|s| !s.trim().is_empty()).map_or_else(Root::user, |p| Root::test(&p))
    }
    pub fn is_test(&self) -> bool {
        !self.prefix.is_empty()
    }
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
    fn path(&self, key: &str) -> String {
        if self.prefix.is_empty() { key.to_string() } else { format!(r"{}\{}", self.prefix, key) }
    }
}

// ───────────────────────── operations ─────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// A string value ("" is the key's default value), creating the key.
    Set { key: String, name: String, value: String },
    /// A value removed (nothing if it is not there).
    Delete { key: String, name: String },
    /// The key removed if nothing is left in it.
    DropIfEmpty { key: String },
    /// The key removed with everything under it (only keys that are the app's own).
    DropTree { key: String },
}

fn set(key: &str, name: &str, value: &str) -> Op {
    Op::Set { key: key.into(), name: name.into(), value: value.into() }
}

/// The keys that make `exe` a capable handler of the spec's types. `keep` says which extensions to claim; the
/// others lose their FileAssociations entry (a codec removed since the last registration).
pub fn register_plan(spec: &Spec, exe: &str, keep: &dyn Fn(&str) -> bool) -> Vec<Op> {
    let caps = spec.caps_key();
    let icon = format!("\"{exe}\",0");
    let open = format!("\"{exe}\" \"%1\"");
    let mut ops = Vec::new();
    for t in spec.types {
        let p = format!(r"{CLASSES}\{}", t.id);
        ops.push(set(&p, "", t.name));
        ops.push(set(&p, "FriendlyTypeName", t.name));
        ops.push(set(&format!(r"{p}\DefaultIcon"), "", &icon));
        ops.push(set(&format!(r"{p}\shell\open"), "FriendlyAppName", spec.app));
        ops.push(set(&format!(r"{p}\shell\open\command"), "", &open));
    }
    ops.push(set(&caps, "ApplicationName", spec.app));
    ops.push(set(&caps, "ApplicationDescription", spec.description));
    ops.push(set(&caps, "ApplicationIcon", &icon));
    for (ext, id) in spec.pairs() {
        if keep(ext) {
            ops.push(set(&format!(r"{CLASSES}\.{ext}\OpenWithProgids"), id, ""));
            ops.push(set(&format!(r"{caps}\FileAssociations"), &format!(".{ext}"), id));
        } else {
            ops.push(Op::Delete { key: format!(r"{caps}\FileAssociations"), name: format!(".{ext}") });
        }
    }
    ops.push(set(REGISTERED, spec.app, &caps));
    ops
}

/// Everything [`register_plan`] wrote, and only that: keys someone else also uses stay when they still hold
/// anything (an extension's own default value, another app's OpenWithProgids entry).
pub fn unregister_plan(spec: &Spec) -> Vec<Op> {
    let mut ops = vec![Op::Delete { key: REGISTERED.into(), name: spec.app.into() }];
    let caps = spec.caps_key();
    ops.push(Op::DropTree { key: caps });
    ops.push(Op::DropIfEmpty { key: format!(r"Software\Plume\{}", spec.key) });
    ops.push(Op::DropIfEmpty { key: r"Software\Plume".into() });
    for t in spec.types {
        ops.push(Op::DropTree { key: format!(r"{CLASSES}\{}", t.id) });
    }
    for (ext, id) in spec.pairs() {
        let owp = format!(r"{CLASSES}\.{ext}\OpenWithProgids");
        ops.push(Op::Delete { key: owp.clone(), name: id.into() });
        ops.push(Op::DropIfEmpty { key: owp });
        ops.push(Op::DropIfEmpty { key: format!(r"{CLASSES}\.{ext}") });
    }
    ops
}

fn check(r: WIN32_ERROR, what: &str) -> Result<(), String> {
    if r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(format!("{what}: error {}", r.0)) }
}

pub fn apply(root: &Root, ops: &[Op]) -> Result<(), String> {
    for op in ops {
        unsafe {
            match op {
                Op::Set { key, name, value } => {
                    let mut h = HKEY::default();
                    let r =
                        RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(root.path(key)), None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut h, None);
                    if r != ERROR_SUCCESS {
                        return Err(format!("{key}: error {}", r.0));
                    }
                    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
                    let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                    let r = RegSetValueExW(h, &HSTRING::from(name.as_str()), None, REG_SZ, Some(bytes));
                    let _ = RegCloseKey(h);
                    if r != ERROR_SUCCESS {
                        return Err(format!(r"{key}\{name}: error {}", r.0));
                    }
                }
                Op::Delete { key, name } => check(RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(root.path(key)), &HSTRING::from(name.as_str())), key)?,
                Op::DropIfEmpty { key } => {
                    if counts(root, key) == Some((0, 0)) {
                        check(RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(root.path(key))), key)?;
                    }
                }
                Op::DropTree { key } => {
                    if exists(root, key) {
                        check(RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(root.path(key))), key)?;
                        check(RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(root.path(key))), key)?;
                    }
                }
            }
        }
    }
    Ok(())
}

// ───────────────────────── reading ─────────────────────────

pub fn get(root: &Root, key: &str, name: &str) -> Option<String> {
    let (k, n) = (HSTRING::from(root.path(key)), HSTRING::from(name));
    unsafe {
        let mut size = 0u32;
        if RegGetValueW(HKEY_CURRENT_USER, &k, &n, RRF_RT_REG_SZ, None, None, Some(&mut size)) != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2) + 1];
        let mut size = (buf.len() * 2) as u32;
        if RegGetValueW(HKEY_CURRENT_USER, &k, &n, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

fn open(root: &Root, key: &str) -> Option<HKEY> {
    let mut h = HKEY::default();
    unsafe { (RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(root.path(key)), None, KEY_READ, &mut h) == ERROR_SUCCESS).then_some(h) }
}

pub fn exists(root: &Root, key: &str) -> bool {
    open(root, key).map(|h| unsafe { RegCloseKey(h) }).is_some()
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

/// The ProgID Windows opens `.ext` with for this user: UserChoiceLatest (Windows 11 24H2 and later) first, then
/// UserChoice. Read only.
pub fn user_choice(root: &Root, ext: &str) -> Option<String> {
    let base = format!(r"{FILE_EXTS}\.{}", ext.trim_start_matches('.'));
    get(root, &format!(r"{base}\UserChoiceLatest\ProgId"), "ProgId").or_else(|| get(root, &format!(r"{base}\UserChoice"), "ProgId")).filter(|p| !p.is_empty())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Not offered to Windows yet.
    NotRegistered,
    /// Offered (listed in Default apps), but Windows opens the everyday types with something else.
    Registered,
    /// Windows opens every everyday type with it.
    Default,
}

pub fn is_registered(spec: &Spec, root: &Root) -> bool {
    get(root, REGISTERED, spec.app).is_some()
}

pub fn state(spec: &Spec, root: &Root) -> State {
    if spec.primary.iter().all(|e| user_choice(root, e).is_some_and(|p| spec.owns(&p))) {
        State::Default
    } else if is_registered(spec, root) {
        State::Registered
    } else {
        State::NotRegistered
    }
}

// ───────────────────────── codecs ─────────────────────────

/// The extensions the installed Windows Imaging Component decoders read (lower case, without the dot).
pub fn wic_extensions() -> HashSet<String> {
    let mut out = HashSet::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(factory) = CoCreateInstance::<_, IWICImagingFactory>(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) else { return out };
        let Ok(list) = factory.CreateComponentEnumerator(WICDecoder.0 as u32, WICComponentEnumerateDefault.0 as u32) else { return out };
        loop {
            let mut one = [None];
            let mut n = 0u32;
            if list.Next(&mut one, Some(&mut n)).is_err() || n == 0 {
                break;
            }
            let Some(info) = one[0].take().and_then(|u| u.cast::<IWICBitmapCodecInfo>().ok()) else { continue };
            let mut len = 0u32;
            let _ = info.GetFileExtensions(&mut [], &mut len);
            if len == 0 {
                continue;
            }
            let mut buf = vec![0u16; len as usize + 1];
            if info.GetFileExtensions(&mut buf, &mut len).is_ok() {
                let text = String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())]);
                out.extend(text.split(',').map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase()).filter(|e| !e.is_empty()));
            }
        }
    }
    out
}

/// The extensions of the spec this PC can open (the codec-dependent ones only when a decoder reads them).
pub fn claimable(spec: &Spec) -> Vec<&'static str> {
    let wic = if spec.needs_codec.is_empty() { HashSet::new() } else { wic_extensions() };
    spec.pairs().map(|(e, _)| e).filter(|e| !spec.needs_codec.contains(e) || wic.contains(*e)).collect()
}

// ───────────────────────── the two switches ─────────────────────────

fn notify(root: &Root) {
    // only the real classes concern Explorer
    if !root.is_test() {
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    }
}

/// Registers `exe` for the types this PC can open. Returns the number of types claimed.
pub fn register(spec: &Spec, root: &Root, exe: &Path) -> Result<usize, String> {
    let keep = claimable(spec);
    let r = apply(root, &register_plan(spec, &exe.display().to_string(), &|e| keep.contains(&e)));
    notify(root);
    r.map(|_| keep.len())
}

pub fn unregister(spec: &Spec, root: &Root) -> Result<(), String> {
    let r = apply(root, &unregister_plan(spec));
    notify(root);
    r
}

/// `--register-defaults` / `--unregister-defaults` as the first argument: does it and returns the exit code. Any
/// other command line: None (the app starts as usual).
pub fn switch(spec: &Spec, args: &[String]) -> Option<i32> {
    let root = Root::from_env();
    match args.first().map(|a| a.to_ascii_lowercase()).as_deref() {
        Some("--register-defaults") => {
            let exe = std::env::current_exe().unwrap_or_default();
            Some(if register(spec, &root, &exe).is_ok() { 0 } else { 1 })
        }
        Some("--unregister-defaults") => Some(if unregister(spec, &root).is_ok() { 0 } else { 1 }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh test root under HKCU\Software\PlumeDefaultsUnitTest, removed at the end.
    struct TestRoot(Root);
    impl TestRoot {
        fn new(name: &str) -> TestRoot {
            let r = Root::test(&format!(r"Software\PlumeDefaultsUnitTest\{name}-{}", std::process::id()));
            let _ = apply(&Root::user(), &[Op::DropTree { key: r.prefix.clone() }]);
            TestRoot(r)
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = apply(&Root::user(), &[Op::DropTree { key: self.0.prefix.clone() }, Op::DropIfEmpty { key: r"Software\PlumeDefaultsUnitTest".into() }]);
        }
    }

    const ALL: [&Spec; 6] = [&MEDIA, &MUSIC, &PDF, &NOTEPAD, &CUT, &CREATE];

    /// Every value under a key, with its path relative to the root.
    fn values(root: &Root, key: &str, out: &mut Vec<String>) {
        let Some(h) = open(root, key) else { return };
        let (mut nv, mut nk) = (0u32, 0u32);
        unsafe {
            let _ = RegQueryInfoKeyW(h, None, None, None, Some(&mut nk), None, None, Some(&mut nv), None, None, None, None);
            for i in 0..nv {
                let mut buf = [0u16; 512];
                let mut len = buf.len() as u32;
                if windows::Win32::System::Registry::RegEnumValueW(h, i, Some(windows::core::PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None)
                    == ERROR_SUCCESS
                {
                    out.push(format!(r"{key}\{}", String::from_utf16_lossy(&buf[..len as usize])));
                }
            }
            let mut subs = Vec::new();
            for i in 0..nk {
                let mut buf = [0u16; 256];
                let mut len = buf.len() as u32;
                if windows::Win32::System::Registry::RegEnumKeyExW(h, i, Some(windows::core::PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None)
                    == ERROR_SUCCESS
                {
                    subs.push(String::from_utf16_lossy(&buf[..len as usize]));
                }
            }
            let _ = RegCloseKey(h);
            for s in subs {
                values(root, &if key.is_empty() { s.clone() } else { format!(r"{key}\{s}") }, out);
            }
        }
    }

    fn all_values(root: &Root) -> Vec<String> {
        let mut v = Vec::new();
        values(root, "", &mut v);
        v
    }

    const EXE: &str = r"C:\Users\x\AppData\Local\Programs\Plume\plume-media.exe";

    #[test]
    fn specs_are_consistent() {
        for spec in ALL {
            let exts: Vec<_> = spec.pairs().map(|(e, _)| e).collect();
            let unique: HashSet<_> = exts.iter().collect();
            assert_eq!(unique.len(), exts.len(), "{} claims an extension twice", spec.app);
            for e in spec.primary.iter().chain(spec.needs_codec) {
                assert!(exts.contains(e), "{}: {e}", spec.app);
            }
            for e in &exts {
                assert!(!e.starts_with('.') && *e == e.to_ascii_lowercase(), "{e}");
            }
            assert!(spec.types.iter().all(|t| spec.owns(t.id) && spec.owns(&t.id.to_uppercase())));
            assert!(!spec.owns("jpegfile"));
            assert!(spec.exe.starts_with("plume-") && spec.exe.ends_with(".exe"));
        }
        assert_eq!(MEDIA.settings_link(), "ms-settings:defaultapps?registeredAppUser=Plume%20Media");
        // no two apps claim the same extension
        let mut seen = HashSet::new();
        for spec in ALL {
            for (e, _) in spec.pairs() {
                assert!(seen.insert(e), "{e} claimed twice");
            }
        }
    }

    #[test]
    fn media_plan() {
        let plan = register_plan(&MEDIA, EXE, &|e| e != "heic");
        assert!(plan.contains(&set(r"Software\Classes\PlumeMedia.Photo\shell\open\command", "", &format!("\"{EXE}\" \"%1\""))));
        assert!(plan.contains(&set(r"Software\Classes\PlumeMedia.Video\DefaultIcon", "", &format!("\"{EXE}\",0"))));
        assert!(plan.contains(&set(r"Software\Plume\Media\Capabilities\FileAssociations", ".jpg", "PlumeMedia.Photo")));
        assert!(plan.contains(&set(r"Software\Plume\Media\Capabilities\FileAssociations", ".mkv", "PlumeMedia.Video")));
        assert!(plan.contains(&set(r"Software\Classes\.webm\OpenWithProgids", "PlumeMedia.Video", "")));
        assert!(plan.contains(&set(r"Software\RegisteredApplications", "Plume Media", r"Software\Plume\Media\Capabilities")));
        assert!(plan.contains(&set(r"Software\Plume\Media\Capabilities", "ApplicationIcon", &format!("\"{EXE}\",0"))));
        // a type without its codec is not claimed, and loses an old claim
        assert!(!plan.iter().any(|o| matches!(o, Op::Set { name, .. } if name == ".heic")));
        assert!(plan.contains(&Op::Delete { key: r"Software\Plume\Media\Capabilities\FileAssociations".into(), name: ".heic".into() }));
        // never UserChoice, never another hive's key
        for op in &plan {
            let Op::Set { key, .. } = op else { continue };
            assert!(!key.contains("UserChoice") && !key.contains("FileExts"), "{key}");
            assert!(key.starts_with(r"Software\Classes\") || key.starts_with(r"Software\Plume\Media") || key == REGISTERED, "{key}");
        }
    }

    #[test]
    fn register_then_unregister_leaves_nothing() {
        let t = TestRoot::new("roundtrip");
        let root = &t.0;
        for spec in ALL {
            let exe = format!(r"C:\Plume\{}", spec.exe);
            assert_eq!(state(spec, root), State::NotRegistered);
            apply(root, &register_plan(spec, &exe, &|_| true)).unwrap();
            // twice: the same keys
            let once = all_values(root);
            apply(root, &register_plan(spec, &exe, &|_| true)).unwrap();
            assert_eq!(all_values(root), once);
            assert!(is_registered(spec, root));
            assert_eq!(state(spec, root), State::Registered);
            assert_eq!(get(root, &format!(r"{}\FileAssociations", spec.caps_key()), &format!(".{}", spec.primary[0])).as_deref(), Some(spec.types[0].id));
            apply(root, &unregister_plan(spec)).unwrap();
            assert!(all_values(root).is_empty(), "{}: {:?}", spec.app, all_values(root));
            assert!(!exists(root, CLASSES) || counts(root, CLASSES) == Some((0, 0)), "{}", spec.app);
            assert!(!exists(root, r"Software\Plume"));
            // and again: nothing to remove is fine
            apply(root, &unregister_plan(spec)).unwrap();
        }
    }

    #[test]
    fn what_was_there_stays() {
        let t = TestRoot::new("others");
        let root = &t.0;
        apply(
            root,
            &[
                set(r"Software\Classes\.jpg", "", "jpegfile"),
                set(r"Software\Classes\.mp4\OpenWithProgids", "WMP11.AssocFile.MP4", ""),
                set(REGISTERED, "Other App", r"Software\Other\Capabilities"),
                set(r"Software\Plume\Web", "x", "1"),
            ],
        )
        .unwrap();
        apply(root, &register_plan(&MEDIA, EXE, &|_| true)).unwrap();
        apply(root, &unregister_plan(&MEDIA)).unwrap();
        assert_eq!(get(root, r"Software\Classes\.jpg", "").as_deref(), Some("jpegfile"));
        assert_eq!(get(root, r"Software\Classes\.mp4\OpenWithProgids", "WMP11.AssocFile.MP4").as_deref(), Some(""));
        assert_eq!(get(root, r"Software\Classes\.mp4\OpenWithProgids", "PlumeMedia.Video"), None);
        assert_eq!(get(root, REGISTERED, "Other App").as_deref(), Some(r"Software\Other\Capabilities"));
        assert_eq!(get(root, r"Software\Plume\Web", "x").as_deref(), Some("1"));
        assert!(!exists(root, r"Software\Plume\Media"));
        assert!(!exists(root, r"Software\Classes\.png"));
    }

    #[test]
    fn default_state_follows_user_choice() {
        let t = TestRoot::new("choice");
        let root = &t.0;
        apply(root, &register_plan(&PDF, r"C:\Plume\plume-pdf.exe", &|_| true)).unwrap();
        let uc = format!(r"{FILE_EXTS}\.pdf\UserChoice");
        apply(root, &[set(&uc, "ProgId", "MSEdgePDF")]).unwrap();
        assert_eq!(user_choice(root, ".pdf").as_deref(), Some("MSEdgePDF"));
        assert_eq!(state(&PDF, root), State::Registered);
        apply(root, &[set(&uc, "ProgId", "PlumePDF.Document")]).unwrap();
        assert_eq!(state(&PDF, root), State::Default);
        // Windows 11 24H2: UserChoiceLatest wins
        apply(root, &[set(&format!(r"{FILE_EXTS}\.pdf\UserChoiceLatest\ProgId"), "ProgId", "AcroExch.Document.DC")]).unwrap();
        assert_eq!(state(&PDF, root), State::Registered);
        apply(root, &[set(&format!(r"{FILE_EXTS}\.pdf\UserChoiceLatest\ProgId"), "ProgId", "PlumePDF.Document")]).unwrap();
        assert_eq!(state(&PDF, root), State::Default);
        // Media: every everyday type must be Plume's
        for (i, e) in MEDIA.primary.iter().enumerate() {
            assert_ne!(state(&MEDIA, root), State::Default, "{e}");
            let id = if i < 5 { "PlumeMedia.Photo" } else { "PlumeMedia.Video" };
            apply(root, &[set(&format!(r"{FILE_EXTS}\.{e}\UserChoice"), "ProgId", id)]).unwrap();
        }
        assert_eq!(state(&MEDIA, root), State::Default);
    }

    #[test]
    fn switches_and_codecs() {
        let t = TestRoot::new("switch");
        // SAFETY: the only test that reads PLUME_DEFAULTS_ROOT (the others pass their root)
        unsafe { std::env::set_var(ROOT_VAR, t.0.prefix()) };
        assert_eq!(Root::from_env(), t.0);
        assert_eq!(switch(&MUSIC, &["song.mp3".into()]), None);
        assert_eq!(switch(&MUSIC, &[]), None);
        assert_eq!(switch(&MUSIC, &["--register-defaults".into()]), Some(0));
        assert!(is_registered(&MUSIC, &t.0));
        assert_eq!(switch(&MUSIC, &["--Unregister-Defaults".into()]), Some(0));
        assert!(all_values(&t.0).is_empty());
        unsafe { std::env::remove_var(ROOT_VAR) };
        assert!(!Root::from_env().is_test());
        // this PC's decoders: the built-in ones always read JPEG and PNG
        let wic = wic_extensions();
        assert!(wic.contains("jpg") && wic.contains("png"), "{wic:?}");
        let media = claimable(&MEDIA);
        assert!(media.contains(&"jpg") && media.contains(&"mp4") && media.contains(&"mkv"));
        assert!(media.iter().all(|e| !MEDIA.needs_codec.contains(e) || wic.contains(*e)));
        assert_eq!(claimable(&PDF), ["pdf"]);
    }
}
