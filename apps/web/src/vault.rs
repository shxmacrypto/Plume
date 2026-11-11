//! Coffre à mots de passe chiffré : clé dérivée du mot de passe maître (Argon2id),
//! contenu chiffré en AES-256-GCM. Rien n'est jamais écrit en clair sur le disque.
//! Fichier : "PLV1" | sel (16) | nonce (12) | données chiffrées (JSON).
use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, AeadCore, OsRng, rand_core::RngCore},
};
use argon2::{Algorithm, Argon2, Params, Version};
use serde_json::Value;
use std::path::PathBuf;

const MAGIC: &[u8; 4] = b"PLV1";

pub struct Vault {
    path: PathBuf,
    key: Option<[u8; 32]>,
    salt: [u8; 16],
    pub items: Vec<Value>,
}

fn derive(pw: &str, salt: &[u8; 16]) -> Result<[u8; 32], String> {
    // Paramètres OWASP (19 Mio, 2 passes) : sûr, et léger en RAM
    let params = Params::new(19 * 1024, 2, 1, Some(32)).map_err(|e| e.to_string())?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(pw.as_bytes(), salt, &mut key).map_err(|e| e.to_string())?;
    Ok(key)
}

impl Vault {
    pub fn new(path: PathBuf) -> Self {
        Vault { path, key: None, salt: [0; 16], items: vec![] }
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn unlocked(&self) -> bool {
        self.key.is_some()
    }

    /// Crée un coffre vide (ou ré-chiffre le contenu actuel) avec un nouveau mot de passe maître.
    pub fn create(&mut self, pw: &str) -> Result<(), String> {
        if pw.chars().count() < 8 {
            return Err("short".into());
        }
        OsRng.fill_bytes(&mut self.salt);
        self.key = Some(derive(pw, &self.salt)?);
        self.save()
    }

    pub fn unlock(&mut self, pw: &str) -> Result<(), String> {
        let data = std::fs::read(&self.path).map_err(|e| e.to_string())?;
        if data.len() < 32 || &data[..4] != MAGIC {
            return Err("corrupt".into());
        }
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&data[4..20]);
        let key = derive(pw, &salt)?;
        let plain = Aes256Gcm::new(&key.into()).decrypt(data[20..32].into(), &data[32..]).map_err(|_| "wrong".to_string())?;
        self.items = serde_json::from_slice(&plain).unwrap_or_default();
        self.salt = salt;
        self.key = Some(key);
        Ok(())
    }

    /// Chiffre des données annexes (notes d'onglet…) avec la clé du coffre : nonce (12) | données chiffrées.
    pub fn seal(&self, plain: &[u8]) -> Option<Vec<u8>> {
        let key = self.key?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct = Aes256Gcm::new(&key.into()).encrypt(&nonce, plain).ok()?;
        Some([nonce.as_slice(), &ct].concat())
    }

    pub fn open(&self, data: &[u8]) -> Option<Vec<u8>> {
        let key = self.key?;
        if data.len() < 13 {
            return None;
        }
        Aes256Gcm::new(&key.into()).decrypt(data[..12].into(), &data[12..]).ok()
    }

    /// Clé du coffre protégée par Windows Hello (#24), à côté du coffre.
    pub fn hello_path(&self) -> PathBuf {
        self.path.with_file_name("hello.bin")
    }

    /// Clé du coffre ouvert (activation de Windows Hello).
    pub fn key(&self) -> Option<[u8; 32]> {
        self.key
    }

    /// Déverrouillage par une clé déjà connue (Windows Hello) au lieu du mot de passe maître.
    pub fn unlock_with_key(&mut self, key: [u8; 32]) -> Result<(), String> {
        let data = std::fs::read(&self.path).map_err(|e| e.to_string())?;
        if data.len() < 32 || &data[..4] != MAGIC {
            return Err("corrupt".into());
        }
        let plain = Aes256Gcm::new(&key.into()).decrypt(data[20..32].into(), &data[32..]).map_err(|_| "wrong".to_string())?;
        self.items = serde_json::from_slice(&plain).unwrap_or_default();
        self.salt.copy_from_slice(&data[4..20]);
        self.key = Some(key);
        Ok(())
    }

    /// Octets aléatoires (générateur de mots de passe).
    pub fn random(n: usize) -> Vec<u8> {
        let mut b = vec![0u8; n];
        OsRng.fill_bytes(&mut b);
        b
    }

    /// Oublie la clé et les mots de passe déchiffrés.
    pub fn lock(&mut self) {
        if let Some(k) = self.key.as_mut() {
            k.fill(0);
        }
        self.key = None;
        self.items.clear();
    }

    pub fn save(&self) -> Result<(), String> {
        let key = self.key.ok_or("locked")?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let plain = serde_json::to_vec(&self.items).map_err(|e| e.to_string())?;
        let ct = Aes256Gcm::new(&key.into()).encrypt(&nonce, plain.as_slice()).map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(32 + ct.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, out).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Identifiants enregistrés pour un site (même domaine ou sous-domaine).
    pub fn matches(&self, host: &str) -> Vec<&Value> {
        self.items
            .iter()
            .filter(|i| {
                let o = i["o"].as_str().unwrap_or("");
                !o.is_empty() && (host == o || host.ends_with(&format!(".{o}")) || o.ends_with(&format!(".{host}")))
            })
            .collect()
    }

    /// Ajoute ou met à jour (même site + même identifiant). Renvoie true si quelque chose a changé.
    pub fn upsert(&mut self, item: Value) -> bool {
        let same = |i: &Value| i["o"] == item["o"] && i["u"] == item["u"];
        match self.items.iter_mut().find(|i| same(i)) {
            Some(i) if i["p"] == item["p"] => false,
            Some(i) => {
                i["p"] = item["p"].clone();
                i["t"] = item["t"].clone();
                true
            }
            None => {
                self.items.push(item);
                true
            }
        }
    }

    pub fn delete(&mut self) -> Result<(), String> {
        self.lock();
        std::fs::remove_file(&self.path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn create_unlock_roundtrip() {
        let p = std::env::temp_dir().join(format!("plume-vault-test-{}.bin", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let mut v = Vault::new(p.clone());
        assert!(v.create("court").is_err(), "mot de passe trop court refusé");
        v.create("une phrase secrète").unwrap();
        assert!(v.upsert(json!({"o": "github.com", "u": "sam", "p": "hunter2", "t": 1})));
        assert!(!v.upsert(json!({"o": "github.com", "u": "sam", "p": "hunter2", "t": 2})), "doublon ignoré");
        assert!(v.upsert(json!({"o": "github.com", "u": "sam", "p": "nouveau", "t": 3})), "mise à jour");
        v.save().unwrap();
        let raw = std::fs::read(&p).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("nouveau"), "rien en clair sur le disque");
        v.lock();
        assert!(!v.unlocked() && v.items.is_empty());
        assert_eq!(v.unlock("mauvais mot de passe").unwrap_err(), "wrong");
        v.unlock("une phrase secrète").unwrap();
        assert_eq!(v.items.len(), 1);
        assert_eq!(v.matches("www.github.com").len(), 1);
        assert_eq!(v.matches("gist.github.com").len(), 1);
        assert_eq!(v.matches("notgithub.com").len(), 0, "pas de faux positif");
        assert_eq!(v.items[0]["p"], "nouveau");
        v.delete().unwrap();
        assert!(!p.exists());
    }
}
