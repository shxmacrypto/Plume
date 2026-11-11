//! The installed apps: the shortcuts of the Start menu (for everyone and for this user), then the apps Windows
//! lists in its Apps folder (Store apps, and the rest). Read on a worker thread (COM initialized there).

use crate::rank::fold;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{
            BHID_EnumItems, FOLDERID_CommonPrograms, FOLDERID_Programs, IEnumShellItems, IShellItem, KF_FLAG_DEFAULT, SHCreateItemFromParsingName,
            SHGetKnownFolderPath, SIGDN, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
        },
    },
    core::{GUID, HSTRING, PWSTR},
};

#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub name: String,
    /// Folded name, for matching.
    pub folded: String,
    /// What opens it: the shortcut's path, or "shell:AppsFolder\<id>".
    pub target: String,
    /// The shortcut itself (Ctrl+Enter shows it in its folder).
    pub path: Option<PathBuf>,
}

pub fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.map(PathBuf::from)
    }
}

/// Shortcuts that are not apps to open.
fn unwanted(name: &str) -> bool {
    let n = fold(name);
    ["uninstall", "desinstall", "readme", "read me", "release notes", "license", "licence", "website", "site web", "help and support", "documentation"]
        .iter()
        .any(|w| n.contains(w))
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            if depth < 6 {
                walk(&p, depth + 1, out);
            }
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
            out.push(p);
        }
    }
}

/// The Start menu folders (tests: PLUME_LAUNCHER_APPS_DIR, one folder, and no Apps folder).
fn start_menus() -> Vec<PathBuf> {
    if let Some(d) = std::env::var_os("PLUME_LAUNCHER_APPS_DIR") {
        return vec![PathBuf::from(d)];
    }
    [known_folder(&FOLDERID_Programs), known_folder(&FOLDERID_CommonPrograms)].into_iter().flatten().collect()
}

fn display(item: &IShellItem, how: SIGDN) -> Option<String> {
    unsafe {
        let p: PWSTR = item.GetDisplayName(how).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s
    }
}

/// The apps of the Apps folder: (name, parsing name).
fn apps_folder() -> Vec<(String, String)> {
    let mut out = vec![];
    unsafe {
        let Ok(folder) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from("shell:AppsFolder"), None) else { return out };
        let Ok(list) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return out };
        loop {
            let mut one = [None];
            let mut got = 0u32;
            if list.Next(&mut one, Some(&mut got)).is_err() || got == 0 {
                break;
            }
            let Some(item) = one[0].take() else { break };
            if let (Some(name), Some(id)) = (display(&item, SIGDN_NORMALDISPLAY), display(&item, SIGDN_PARENTRELATIVEPARSING)) {
                out.push((name, id));
            }
        }
    }
    out
}

/// Every app, each name once. To call on a thread where COM is initialized.
pub fn load() -> Vec<App> {
    let mut apps = vec![];
    let mut seen = HashSet::new();
    let mut links = vec![];
    for dir in start_menus() {
        walk(&dir, 0, &mut links);
    }
    // this user's shortcuts first: they win over the shared ones of the same name
    for p in links {
        let Some(name) = p.file_stem().map(|s| s.to_string_lossy().to_string()) else { continue };
        if unwanted(&name) || !seen.insert(fold(&name)) {
            continue;
        }
        apps.push(App { folded: fold(&name), name, target: p.display().to_string(), path: Some(p) });
    }
    if std::env::var_os("PLUME_LAUNCHER_APPS_DIR").is_none() {
        for (name, id) in apps_folder() {
            let low = id.to_lowercase();
            // documents and web links listed there are not apps
            if [".txt", ".chm", ".url", ".htm", ".html", ".pdf", ".rtf"].iter().any(|x| low.ends_with(x)) || low.starts_with("http") {
                continue;
            }
            if unwanted(&name) || !seen.insert(fold(&name)) {
                continue;
            }
            apps.push(App { folded: fold(&name), name, target: format!("shell:AppsFolder\\{id}"), path: None });
        }
    }
    apps.sort_by(|a, b| a.folded.cmp(&b.folded));
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_apps() {
        assert!(unwanted("Uninstall Foo"));
        assert!(unwanted("Désinstaller Bar"));
        assert!(unwanted("Foo Website"));
        assert!(!unwanted("Visual Studio Code"));
    }

    #[test]
    fn reads_a_start_menu_folder() {
        let dir = std::env::temp_dir().join(format!("plume-launcher-apps-{}", std::process::id()));
        let sub = dir.join("Tools");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("Alpha.lnk"), b"x").unwrap();
        std::fs::write(sub.join("Beta.lnk"), b"x").unwrap();
        std::fs::write(sub.join("Uninstall Beta.lnk"), b"x").unwrap();
        std::fs::write(sub.join("notes.txt"), b"x").unwrap();
        let mut found = vec![];
        walk(&dir, 0, &mut found);
        let mut names: Vec<String> = found.iter().filter_map(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).filter(|n| !unwanted(n)).collect();
        names.sort();
        assert_eq!(names, ["Alpha", "Beta"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
