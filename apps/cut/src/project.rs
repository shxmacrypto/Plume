//! Project files (`.plumecut`, JSON). Media paths are saved relative to the project file, so a folder holding the
//! project and its clips can be moved or copied to another PC; the absolute path is kept too, as a fallback.
//! Writes go to a temporary file renamed over the old one: a crash never leaves half a project.

use crate::model::Project;
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

pub const EXT: &str = "plumecut";

/// `target` written relative to the folder `base` ("..\Videos\clip.mp4"); None across drives.
pub fn relative(target: &Path, base: &Path) -> Option<PathBuf> {
    let t: Vec<Component> = target.components().collect();
    let b: Vec<Component> = base.components().collect();
    let same = |x: &Component, y: &Component| x.as_os_str().to_string_lossy().eq_ignore_ascii_case(&y.as_os_str().to_string_lossy());
    // same drive (the prefix) or no relative path
    if t.is_empty() || b.is_empty() || !same(&t[0], &b[0]) {
        return None;
    }
    let common = t.iter().zip(&b).take_while(|(x, y)| same(x, y)).count();
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &t[common..] {
        out.push(c.as_os_str());
    }
    Some(out)
}

/// Removes "." and "a\.." from a path without touching the disk.
pub fn tidy(p: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir if matches!(out.last(), Some(Component::Normal(_))) => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out.iter().map(|c| c.as_os_str()).collect()
}

/// The project as JSON, its media paths made relative to `file`'s folder.
pub fn to_json(p: &Project, file: &Path) -> String {
    let mut p = p.clone();
    let dir = file.parent().unwrap_or(Path::new(""));
    for m in &mut p.media {
        m.abs = m.path.to_string_lossy().into_owned();
        m.rel = relative(&m.path, dir).map(|r| r.to_string_lossy().into_owned()).unwrap_or_default();
    }
    serde_json::to_string_pretty(&p).unwrap_or_default()
}

/// A project from its JSON; media found next to the project first, then where they were.
pub fn from_json(json: &str, file: &Path) -> Result<Project, String> {
    let mut p: Project = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let dir = file.parent().unwrap_or(Path::new(""));
    for m in &mut p.media {
        let rel = (!m.rel.is_empty()).then(|| tidy(&dir.join(&m.rel)));
        let abs = (!m.abs.is_empty()).then(|| PathBuf::from(&m.abs));
        m.path = match (rel, abs) {
            (Some(r), _) if r.is_file() => r,
            (_, Some(a)) if a.is_file() => a,
            (Some(r), _) => r,
            (None, Some(a)) => a,
            (None, None) => PathBuf::new(),
        };
        m.missing = !m.path.is_file();
    }
    if p.fps == 0 {
        p.fps = 30;
    }
    if p.aspect[0] == 0 || p.aspect[1] == 0 {
        p.aspect = [16, 9];
    }
    let max = p.media.iter().map(|m| m.id).chain(p.tracks.iter().map(|t| t.id)).chain(p.tracks.iter().flat_map(|t| t.clips.iter().map(|c| c.id))).max();
    p.next_id = p.next_id.max(max.unwrap_or(0) + 1);
    Ok(p)
}

pub fn save(p: &Project, file: &Path) -> io::Result<()> {
    if let Some(d) = file.parent() {
        fs::create_dir_all(d)?;
    }
    let tmp = file.with_extension("plumecut.tmp");
    fs::write(&tmp, to_json(p, file))?;
    fs::rename(&tmp, file)
}

pub fn load(file: &Path) -> Result<Project, String> {
    let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
    from_json(&text, file)
}

/// The folder new projects go to: Documents\Plume Cut (PLUME_CUT_PROJECTS for tests).
pub fn projects_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_CUT_PROJECTS") {
        return PathBuf::from(d);
    }
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Documents").join("Plume Cut")
}

/// "Project 1.plumecut", "Project 2.plumecut"… the first free name, taken by creating it.
pub fn new_file(dir: &Path, base: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    for n in 1..100_000 {
        let p = dir.join(format!("{base} {n}.{EXT}"));
        match fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(_) => return Ok(p),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no free name"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Media, MediaKind};

    #[test]
    fn relative_paths() {
        let r = relative(Path::new(r"C:\Users\a\Videos\clip.mp4"), Path::new(r"C:\Users\a\Documents\Plume Cut")).unwrap();
        assert_eq!(r, PathBuf::from(r"..\..\Videos\clip.mp4"));
        assert_eq!(relative(Path::new(r"C:\p\x\c.mp4"), Path::new(r"c:\P")).unwrap(), PathBuf::from(r"x\c.mp4"));
        assert!(relative(Path::new(r"D:\c.mp4"), Path::new(r"C:\p")).is_none());
        assert_eq!(tidy(Path::new(r"C:\a\b\..\.\c.mp4")), PathBuf::from(r"C:\a\c.mp4"));
    }

    #[test]
    fn save_load_and_move_the_folder() {
        let root = std::env::temp_dir().join(format!("plume-cut-proj-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (a, b) = (root.join("one"), root.join("two"));
        fs::create_dir_all(a.join("media")).unwrap();
        fs::write(a.join("media").join("clip.mp4"), b"x").unwrap();
        let mut p = crate::model::tests::sample();
        p.media.truncate(1);
        p.media[0] = Media { id: 100, kind: MediaKind::Video, duration: 10.0, path: a.join("media").join("clip.mp4"), ..Media::default() };
        let c = p.append(100).unwrap();
        p.split(100, None);
        p.name = "Trip".into();
        let file = a.join("Trip.plumecut");
        save(&p, &file).unwrap();
        let back = load(&file).unwrap();
        assert_eq!(back.media[0].rel, r"media\clip.mp4");
        assert_eq!(back.media[0].path, a.join("media").join("clip.mp4"));
        assert!(!back.media[0].missing);
        assert_eq!(back.tracks, p.tracks);
        assert_eq!(back.clip(c), p.clip(c));
        // the whole folder moved: the clip is found next to the project
        fs::rename(&a, &b).unwrap();
        let moved = load(&b.join("Trip.plumecut")).unwrap();
        assert_eq!(moved.media[0].path, b.join("media").join("clip.mp4"));
        assert!(!moved.media[0].missing);
        // gone everywhere: still opens, marked missing
        fs::remove_file(b.join("media").join("clip.mp4")).unwrap();
        assert!(load(&b.join("Trip.plumecut")).unwrap().media[0].missing);
        // ids keep growing after a load
        let mut m = moved.clone();
        assert!(m.new_id() > c);
        // a new name never replaces a file
        let f1 = new_file(&root, "Project").unwrap();
        let f2 = new_file(&root, "Project").unwrap();
        assert_ne!(f1, f2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn old_or_partial_files_open() {
        let p = from_json(r#"{"name":"x","tracks":[{"id":1,"kind":"video","clips":[{"id":5,"start":3,"len":9}]}]}"#, Path::new(r"C:\p\x.plumecut")).unwrap();
        assert_eq!(p.fps, 30);
        assert_eq!(p.aspect, [16, 9]);
        assert_eq!(p.clip(5).unwrap().speed, 1.0);
        assert!(p.next_id > 5);
        assert!(from_json("nope", Path::new("x")).is_err());
    }
}
