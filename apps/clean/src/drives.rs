//! The drives of the PC with their used and free space.

use std::path::PathBuf;
use windows::{
    Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW},
    core::PCWSTR,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Drive {
    /// "C:\", or a folder (the sandbox when testing).
    pub root: PathBuf,
    pub label: String,
    pub total: u64,
    pub free: u64,
    pub removable: bool,
}

impl Drive {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.free)
    }
    /// "Local Disk (C:)", "USB (E:)".
    pub fn title(&self) -> String {
        let letter = self.root.to_string_lossy().trim_end_matches('\\').to_string();
        if letter.len() == 2 {
            let name =
                if self.label.is_empty() { crate::i18n::tr(if self.removable { "USB drive" } else { "Local Disk" }).to_string() } else { self.label.clone() };
            format!("{name} ({letter})")
        } else {
            self.label.clone()
        }
    }
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Total and free bytes of the volume holding `path`.
pub fn space(path: &std::path::Path) -> (u64, u64) {
    let p = w(&path.to_string_lossy());
    let (mut free, mut total) = (0u64, 0u64);
    unsafe {
        let _ = GetDiskFreeSpaceExW(PCWSTR(p.as_ptr()), Some(&mut free), Some(&mut total), None);
    }
    (total, free)
}

pub fn list() -> Vec<Drive> {
    let mut buf = [0u16; 512];
    let n = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) } as usize;
    let mut out = Vec::new();
    for root in String::from_utf16_lossy(&buf[..n.min(buf.len())]).split('\0').filter(|s| !s.is_empty()) {
        let rw = w(root);
        let kind = unsafe { GetDriveTypeW(PCWSTR(rw.as_ptr())) };
        // 2 removable, 3 fixed (network, optical and RAM drives are left out)
        if kind != 2 && kind != 3 {
            continue;
        }
        let (total, free) = space(std::path::Path::new(root));
        if total == 0 {
            continue;
        }
        let mut label = [0u16; 261];
        unsafe {
            let _ = GetVolumeInformationW(PCWSTR(rw.as_ptr()), Some(&mut label), None, None, None, None);
        }
        let len = label.iter().position(|&c| c == 0).unwrap_or(0);
        out.push(Drive { root: PathBuf::from(root), label: String::from_utf16_lossy(&label[..len]), total, free, removable: kind == 2 });
    }
    if let Some(sb) = std::env::var_os("PLUME_CLEAN_SANDBOX").map(PathBuf::from) {
        let (total, free) = space(&sb);
        out.push(Drive { root: sb, label: crate::i18n::tr("Test folder").to_string(), total, free, removable: false });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn has_the_system_drive() {
        let d = super::list();
        assert!(d.iter().any(|d| d.root.to_string_lossy().eq_ignore_ascii_case("C:\\") && d.total >= d.free && d.total > 0));
    }
}
