//! Chaînes UTF-16 pour les API Windows, et chemins longs.

use std::path::Path;

/// Chaîne UTF-16 terminée par zéro.
pub fn wz(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Chemin pour les API fichiers : préfixe `\\?\` (plus de limite à 260 caractères), terminé par zéro.
/// Les chemins déjà préfixés et les chemins relatifs sont laissés tels quels.
pub fn long_path(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let w: Vec<u16> = p.as_os_str().encode_wide().collect();
    let is = |i: usize, c: char| w.get(i).is_some_and(|&x| x == c as u16);
    let sep = |i: usize| is(i, '\\') || is(i, '/');
    let mut out: Vec<u16>;
    if sep(0) && sep(1) && (is(2, '?') || is(2, '.')) {
        out = w; // déjà \\?\ ou \\.\
    } else if sep(0) && sep(1) {
        // \\serveur\partage → \\?\UNC\serveur\partage
        out = r"\\?\UNC\".encode_utf16().collect();
        out.extend_from_slice(&w[2..]);
    } else if w.len() >= 3 && is(1, ':') && sep(2) {
        out = r"\\?\".encode_utf16().collect();
        out.extend_from_slice(&w);
    } else {
        out = w;
    }
    // \\?\ n'accepte que des barres obliques inverses
    for c in out.iter_mut() {
        if *c == '/' as u16 {
            *c = '\\' as u16;
        }
    }
    out.push(0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(w: Vec<u16>) -> String {
        String::from_utf16_lossy(&w[..w.len() - 1])
    }

    #[test]
    fn long_paths() {
        assert_eq!(s(long_path(Path::new(r"C:\Users\a"))), r"\\?\C:\Users\a");
        assert_eq!(s(long_path(Path::new("C:/Users/a"))), r"\\?\C:\Users\a");
        assert_eq!(s(long_path(Path::new(r"\\srv\share\x"))), r"\\?\UNC\srv\share\x");
        assert_eq!(s(long_path(Path::new(r"\\?\C:\x"))), r"\\?\C:\x");
        assert_eq!(s(long_path(Path::new(r"rel\x"))), r"rel\x");
    }
}
