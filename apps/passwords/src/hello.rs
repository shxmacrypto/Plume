//! Unlocking with Windows Hello (face, fingerprint or PIN). When it is turned on, a copy of the vault's data key is
//! kept sealed by Windows for this user account only (DPAPI) in hello.bin; Windows Hello must say "verified" before
//! the app unseals it. The master password always keeps working.

use std::path::Path;
use windows::{
    Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability},
    Win32::{
        Foundation::{HLOCAL, HWND, LocalFree},
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData},
        System::WinRT::IUserConsentVerifierInterop,
    },
    core::{HSTRING, factory},
};
use windows_future::IAsyncOperation;
use zeroize::Zeroizing;

/// Mixed into the seal: another program's DPAPI data cannot be passed off as ours, nor ours read without it.
const ENTROPY: &[u8] = b"Plume Passwords hello key";

pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let ent = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(&input, &HSTRING::from("Plume Passwords"), Some(&ent), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
        Some(v)
    }
}

pub fn unprotect(data: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let ent = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&input, None, Some(&ent), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
        let v = Zeroizing::new(std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec());
        // wipe Windows' copy before giving it back
        std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
        Some(v)
    }
}

/// Is Windows Hello set up on this PC?
pub fn available() -> bool {
    #[cfg(feature = "test-hook")]
    if std::env::var_os("PLUME_PASSWORDS_FAKE_HELLO").is_some() {
        return true;
    }
    UserConsentVerifier::CheckAvailabilityAsync().and_then(|a| a.get()).is_ok_and(|a| a == UserConsentVerifierAvailability::Available)
}

/// Asks Windows Hello, over our window. Blocks until the person answers: call it off the interface thread.
pub fn verify(owner: HWND, message: &str) -> bool {
    #[cfg(feature = "test-hook")]
    if let Some(v) = std::env::var_os("PLUME_PASSWORDS_FAKE_HELLO") {
        return v == "yes";
    }
    unsafe {
        let Ok(interop) = factory::<UserConsentVerifier, IUserConsentVerifierInterop>() else { return false };
        let op: windows::core::Result<IAsyncOperation<UserConsentVerificationResult>> =
            interop.RequestVerificationForWindowAsync(owner, &HSTRING::from(message));
        op.and_then(|a| a.get()).is_ok_and(|r| r == UserConsentVerificationResult::Verified)
    }
}

/// Keeps a sealed copy of the data key.
pub fn enable(path: &Path, key: &[u8; 32]) -> bool {
    protect(key).is_some_and(|sealed| crate::vault::write_atomic(path, &sealed).is_ok())
}

/// The data key from the sealed copy (after `verify`).
pub fn key(path: &Path) -> Option<Zeroizing<[u8; 32]>> {
    let k = unprotect(&std::fs::read(path).ok()?)?;
    let mut out = Zeroizing::new([0u8; 32]);
    if k.len() != 32 {
        return None;
    }
    out.copy_from_slice(&k);
    Some(out)
}

pub fn disable(path: &Path) {
    if let Ok(len) = std::fs::metadata(path).map(|m| m.len()) {
        // overwrite before removing
        let _ = std::fs::write(path, vec![0u8; len as usize]);
    }
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_round_trip() {
        let key = [7u8; 32];
        let sealed = protect(&key).unwrap();
        assert!(!sealed.windows(32).any(|w| w == key));
        assert_eq!(unprotect(&sealed).unwrap().as_slice(), &key);
        let mut bad = sealed.clone();
        let n = bad.len();
        bad[n - 5] ^= 1;
        assert!(unprotect(&bad).is_none());
        let dir = std::env::temp_dir().join(format!("plume-passwords-hello-{}", std::process::id()));
        let f = dir.join("hello.bin");
        assert!(enable(&f, &key));
        assert_eq!(*key_of(&f), key);
        disable(&f);
        assert!(!f.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn key_of(f: &Path) -> Zeroizing<[u8; 32]> {
        key(f).unwrap()
    }
}
