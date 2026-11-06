// Windows Hello (#24). Une clé protégée par Hello (KeyCredentialManager) signe un défi aléatoire ;
// l'empreinte SHA-256 de la signature (RSA PKCS#1 v1.5, déterministe) chiffre la clé du coffre.
// Sans Hello (PIN, empreinte, visage), impossible de retrouver la clé ; le mot de passe maître reste valable.
// Fichier hello.bin : défi (32) | nonce (12) | clé du coffre chiffrée (AES-256-GCM).
// Les appels bloquent (fenêtre système de Windows Hello) : à faire hors du fil de l'interface.
use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use sha2::{Digest, Sha256};
use std::path::Path;
use windows::{
    Security::{
        Credentials::{KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus, UI::UserConsentVerificationResult},
        Cryptography::CryptographicBuffer,
    },
    Win32::{Foundation::HWND, System::WinRT::IUserConsentVerifierInterop},
    core::{HSTRING, factory},
};
use windows_future::IAsyncOperation;

const NAME: &str = "Plume";

pub fn supported() -> bool {
    KeyCredentialManager::IsSupportedAsync().and_then(|a| a.get()).unwrap_or(false)
}

/// Signe le défi avec la clé Hello de Plume (`create` : en crée une nouvelle, remplace l'ancienne).
fn sign(create: bool, challenge: &[u8]) -> Result<Vec<u8>, String> {
    let name = HSTRING::from(NAME);
    let r = if create {
        KeyCredentialManager::RequestCreateAsync(&name, KeyCredentialCreationOption::ReplaceExisting)
    } else {
        KeyCredentialManager::OpenAsync(&name)
    }
    .and_then(|a| a.get())
    .map_err(|e| e.message())?;
    let st = r.Status().map_err(|e| e.message())?;
    if st != KeyCredentialStatus::Success {
        return Err(if st == KeyCredentialStatus::UserCanceled { "canceled".into() } else { format!("hello status {}", st.0) });
    }
    let buf = CryptographicBuffer::CreateFromByteArray(challenge).map_err(|e| e.message())?;
    let res = r.Credential().and_then(|c| c.RequestSignAsync(&buf)).and_then(|a| a.get()).map_err(|e| e.message())?;
    if res.Status().map_err(|e| e.message())? != KeyCredentialStatus::Success {
        return Err("canceled".into());
    }
    let mut out = windows::core::Array::<u8>::new();
    CryptographicBuffer::CopyToByteArray(&res.Result().map_err(|e| e.message())?, &mut out).map_err(|e| e.message())?;
    Ok(out.to_vec())
}

fn kek(sig: &[u8]) -> [u8; 32] {
    Sha256::digest(sig).into()
}

/// Active Hello : protège une copie de la clé du coffre (déjà déverrouillé).
pub fn enable(path: &Path, vault_key: [u8; 32]) -> Result<(), String> {
    let challenge = crate::vault::Vault::random(32);
    let k = kek(&sign(true, &challenge)?);
    let nonce = crate::vault::Vault::random(12);
    let ct = Aes256Gcm::new(&k.into()).encrypt(nonce.as_slice().into(), vault_key.as_slice()).map_err(|e| e.to_string())?;
    std::fs::write(path, [challenge.as_slice(), &nonce, &ct].concat()).map_err(|e| e.to_string())
}

/// Déverrouillage par Hello : rend la clé du coffre.
pub fn unlock(path: &Path) -> Result<[u8; 32], String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 32 + 12 + 16 {
        return Err("corrupt".into());
    }
    let k = kek(&sign(false, &data[..32])?);
    let key = Aes256Gcm::new(&k.into()).decrypt(data[32..44].into(), &data[44..]).map_err(|_| "wrong".to_string())?;
    key.try_into().map_err(|_| "corrupt".into())
}

/// Simple vérification de présence (avant un remplissage automatique), rattachée à la fenêtre de Plume.
pub fn verify(owner: HWND, message: &str) -> bool {
    unsafe {
        let Ok(interop) = factory::<windows::Security::Credentials::UI::UserConsentVerifier, IUserConsentVerifierInterop>() else { return false };
        let op: windows::core::Result<IAsyncOperation<UserConsentVerificationResult>> = interop.RequestVerificationForWindowAsync(owner, &HSTRING::from(message));
        op.and_then(|a| a.get()).is_ok_and(|r| r == UserConsentVerificationResult::Verified)
    }
}
