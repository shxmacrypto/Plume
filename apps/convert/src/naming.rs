//! Where results go: next to the original or in the chosen folder, named "name (converted).ext". Nothing is ever
//! overwritten: the name is taken by creating the file (or folder) first, and a number is added when it exists.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// "clip (converted).mp4", then "clip (converted 2).mp4", "clip (converted 3).mp4"…
pub fn candidate(src: &Path, dir: &Path, label: &str, ext: Option<&str>, n: u32) -> PathBuf {
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let tag = if n <= 1 { format!("{stem} ({label})") } else { format!("{stem} ({label} {n})") };
    match ext {
        Some(e) => dir.join(format!("{tag}.{e}")),
        None => dir.join(tag),
    }
}

/// The folder results go to.
pub fn out_dir(src: &Path, chosen: Option<&Path>) -> PathBuf {
    match chosen {
        Some(d) if d.is_dir() => d.to_path_buf(),
        _ => src.parent().map(Path::to_path_buf).unwrap_or_default(),
    }
}

/// Takes a free name by creating an empty file there (FFmpeg then writes into it). Never an existing file.
pub fn reserve_file(src: &Path, dir: &Path, ext: &str) -> io::Result<PathBuf> {
    for n in 1..10_000 {
        let p = candidate(src, dir, "converted", Some(ext), n);
        if same(&p, src) {
            continue;
        }
        match fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(_) => return Ok(p),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no free name"))
}

/// Takes a free folder name ("clip (frames)", "clip (frames 2)"…) by creating it.
pub fn reserve_folder(src: &Path, dir: &Path, label: &str) -> io::Result<PathBuf> {
    for n in 1..10_000 {
        let p = candidate(src, dir, label, None, n);
        match fs::create_dir(&p) {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no free name"))
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

/// "3.4 MB", "820 KB".
pub fn size_text(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.2} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB", b / 1e3)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("plume-convert-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn names() {
        let src = Path::new(r"C:\v\Holiday.MOV");
        assert_eq!(candidate(src, Path::new(r"C:\o"), "converted", Some("mp4"), 1), PathBuf::from(r"C:\o\Holiday (converted).mp4"));
        assert_eq!(candidate(src, Path::new(r"C:\o"), "converted", Some("mp4"), 3), PathBuf::from(r"C:\o\Holiday (converted 3).mp4"));
        assert_eq!(candidate(src, Path::new(r"C:\o"), "frames", None, 2), PathBuf::from(r"C:\o\Holiday (frames 2)"));
        assert_eq!(out_dir(src, None), PathBuf::from(r"C:\v"));
        assert_eq!(out_dir(src, Some(Path::new(r"Z:\does\not\exist"))), PathBuf::from(r"C:\v"));
    }

    #[test]
    fn never_overwrites() {
        let d = temp("naming");
        let src = d.join("clip.mp4");
        fs::write(&src, b"original").unwrap();
        let a = reserve_file(&src, &d, "mp4").unwrap();
        let b = reserve_file(&src, &d, "mp4").unwrap();
        assert_eq!(a.file_name().unwrap(), "clip (converted).mp4");
        assert_eq!(b.file_name().unwrap(), "clip (converted 2).mp4");
        // something already there under the next name stays untouched
        fs::write(d.join("clip (converted 3).mp4"), b"keep me").unwrap();
        let c = reserve_file(&src, &d, "mp4").unwrap();
        assert_eq!(c.file_name().unwrap(), "clip (converted 4).mp4");
        assert_eq!(fs::read(d.join("clip (converted 3).mp4")).unwrap(), b"keep me");
        assert_eq!(fs::read(&src).unwrap(), b"original");
        let f1 = reserve_folder(&src, &d, "frames").unwrap();
        let f2 = reserve_folder(&src, &d, "frames").unwrap();
        assert!(f1.is_dir() && f2.is_dir() && f1 != f2);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn sizes() {
        assert_eq!(size_text(512), "512 B");
        assert_eq!(size_text(820_000), "820 KB");
        assert_eq!(size_text(3_400_000), "3.4 MB");
        assert_eq!(size_text(2_500_000_000), "2.50 GB");
    }
}
