//! Plume PDF's settings and memory, in %LOCALAPPDATA%\Plume\PDF (PLUME_PDF_DIR for tests):
//! settings.json (choices), state.json (recent files, last page read in each) and signature.json (the signature
//! drawn once). Nothing leaves the computer.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_PDF_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Plume").join("PDF")
}

fn load<T: for<'a> Deserialize<'a> + Default>(name: &str) -> T {
    std::fs::read(data_dir().join(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn store<T: Serialize>(name: &str, v: &T) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let tmp = dir.join(format!("{name}.tmp"));
    if let Ok(b) = serde_json::to_vec_pretty(v)
        && std::fs::write(&tmp, b).is_ok()
    {
        let _ = std::fs::rename(&tmp, dir.join(name));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "system", "dark", "light".
    pub theme: String,
    /// "system", "en", "fr".
    pub lang: String,
    /// How a file opens: "width" (fit the width), "page" (whole page), "100".
    pub zoom: String,
    /// Pages in dark colors (inverted).
    pub dark_pages: bool,
    /// Reopen each file where it was left.
    pub remember_page: bool,
    pub sidebar: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: "system".into(), lang: "system".into(), zoom: "width".into(), dark_pages: false, remember_page: true, sidebar: true }
    }
}

impl Settings {
    pub fn load() -> Settings {
        load("settings.json")
    }
    pub fn save(&self) {
        store("settings.json", self);
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Memory {
    pub recent: Vec<PathBuf>,
    /// Last page read, by file path.
    pub pages: HashMap<String, usize>,
}

impl Memory {
    pub fn load() -> Memory {
        load("state.json")
    }
    pub fn save(&self) {
        store("state.json", self);
    }
    pub fn opened(&mut self, p: &Path) {
        self.recent.retain(|r| r != p);
        self.recent.insert(0, p.to_path_buf());
        self.recent.truncate(12);
    }
    pub fn key(p: &Path) -> String {
        p.to_string_lossy().to_lowercase()
    }
}

/// The signature: strokes as fractions of its box (0..1, y down) and the box's width / height.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Signature {
    pub strokes: Vec<Vec<[f32; 2]>>,
    pub aspect: f32,
}

impl Signature {
    pub fn load() -> Option<Signature> {
        let s: Signature = load("signature.json");
        (!s.strokes.is_empty() && s.aspect > 0.0).then_some(s)
    }
    pub fn save(&self) {
        store("signature.json", self);
    }
    pub fn forget() {
        let _ = std::fs::remove_file(data_dir().join("signature.json"));
    }

    /// From strokes drawn in a pad (points in any unit): cropped to the drawing, with a small margin.
    pub fn from_pad(strokes: &[Vec<(f32, f32)>]) -> Option<Signature> {
        let pts: Vec<(f32, f32)> = strokes.iter().flatten().copied().collect();
        if pts.len() < 2 {
            return None;
        }
        let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (y0, y1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let m = 4.0;
        let (w, h) = ((x1 - x0) + 2.0 * m, (y1 - y0) + 2.0 * m);
        let strokes = strokes.iter().filter(|s| !s.is_empty()).map(|s| s.iter().map(|p| [(p.0 - x0 + m) / w, (p.1 - y0 + m) / h]).collect()).collect();
        Some(Signature { strokes, aspect: w / h })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_cropped_to_its_drawing() {
        let s = Signature::from_pad(&[vec![(10.0, 10.0), (50.0, 30.0)], vec![]]).unwrap();
        assert_eq!(s.strokes.len(), 1);
        assert!((s.aspect - 48.0 / 28.0).abs() < 1e-4);
        assert!((s.strokes[0][0][0] - 4.0 / 48.0).abs() < 1e-4 && (s.strokes[0][1][1] - 24.0 / 28.0).abs() < 1e-4);
        assert!(Signature::from_pad(&[vec![(1.0, 1.0)]]).is_none());
    }

    #[test]
    fn recent_files_stay_unique_and_short() {
        let mut m = Memory::default();
        for i in 0..20 {
            m.opened(Path::new(&format!("C:\\f{i}.pdf")));
        }
        m.opened(Path::new("C:\\f5.pdf"));
        assert_eq!(m.recent.len(), 12);
        assert_eq!(m.recent[0], PathBuf::from("C:\\f5.pdf"));
        assert_eq!(m.recent.iter().filter(|p| p.ends_with("f5.pdf")).count(), 1);
    }
}
