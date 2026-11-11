//! The keys of Plume Viewer. A viewer proves that it knows the one-time code or the permanent password by signing
//! the WebRTC offer with a key derived from it; the PC signs its answer with the same key. Nothing secret crosses
//! the relay: it only sees the signatures. The same derivations run in the viewer page (viewer.js).

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// PBKDF2 rounds for the permanent password (the viewer page uses the same number).
pub const PBKDF2_ROUNDS: u32 = 100_000;

pub fn random(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).expect("random");
    b
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// A one-time code: 8 characters a person can read aloud (no 0/o, 1/l/i).
pub fn new_code() -> String {
    const SET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    random(8).iter().map(|b| SET[*b as usize % SET.len()] as char).collect()
}

/// A 9-digit ID that never starts with 0.
pub fn new_id() -> String {
    let b = random(8);
    let n = u64::from_le_bytes(b.try_into().unwrap()) % 900_000_000 + 100_000_000;
    n.to_string()
}

/// The code as the viewer types it: lowercase, without spaces or dashes.
pub fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| !c.is_whitespace() && *c != '-').flat_map(char::to_lowercase).collect()
}

pub fn code_key(id: &str, code: &str) -> [u8; 32] {
    Sha256::digest(format!("plume-viewer code\n{id}\n{}", normalize_code(code))).into()
}

pub fn password_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ROUNDS, &mut out);
    out
}

pub fn sign(key: &[u8], text: &str) -> String {
    let mut m = HmacSha256::new_from_slice(key).expect("hmac key");
    m.update(text.as_bytes());
    hex(&m.finalize().into_bytes())
}

/// Constant-time check of a signature given in hex.
pub fn verify(key: &[u8], text: &str, mac_hex: &str) -> bool {
    let Some(mac) = unhex(mac_hex) else { return false };
    let mut m = HmacSha256::new_from_slice(key).expect("hmac key");
    m.update(text.as_bytes());
    m.verify_slice(&mac).is_ok()
}

/// Windows keeps the permanent password's key for this user only (DPAPI): another account cannot read it.
pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    use windows::Win32::{Foundation::HLOCAL, Foundation::LocalFree, Security::Cryptography::*};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(&input, windows::core::w!("Plume Viewer"), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
        Some(v)
    }
}

pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    use windows::Win32::{Foundation::HLOCAL, Foundation::LocalFree, Security::Cryptography::*};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_as_webcrypto() {
        // values computed with the browser's WebCrypto (viewer.js), so both sides agree
        assert_eq!(hex(&code_key("123456789", "ab cd-ef23")), hex(&code_key("123456789", "abcdef23")));
        let k = code_key("123456789", "abcdef23");
        let mac = sign(&k, "offer\nnh\nnv\nsdp");
        assert!(verify(&k, "offer\nnh\nnv\nsdp", &mac));
        assert!(!verify(&k, "offer\nnh\nnv\nsdq", &mac));
        assert!(!verify(&k, "offer\nnh\nnv\nsdp", "zz"));
    }

    #[test]
    fn ids_and_codes() {
        for _ in 0..200 {
            let id = new_id();
            assert_eq!(id.len(), 9);
            assert!(!id.starts_with('0'));
            let c = new_code();
            assert_eq!(c.len(), 8);
            assert!(c.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit()));
        }
    }

    #[test]
    fn dpapi_round_trip() {
        let p = protect(b"secret key").unwrap();
        assert_ne!(p, b"secret key");
        assert_eq!(unprotect(&p).unwrap(), b"secret key");
    }
}
