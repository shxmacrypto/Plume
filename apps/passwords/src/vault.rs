//! The vault: one encrypted file, nothing ever written in clear.
//!
//! A random 256-bit data key seals the entries (AES-256-GCM). The master password, stretched with Argon2id, seals
//! the data key. Changing the master password makes a new data key. File layout (little endian):
//!
//!   "PPW1" | memory KiB u32 | passes u32 | lanes u32 | salt 16 | key nonce 12 | sealed data key 48 | data nonce 12 | sealed JSON
//!
//! The first 92 bytes (everything before the data nonce) are bound to both seals as associated data: changing a
//! parameter or the salt makes the file unreadable rather than weaker.

use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, OsRng, Payload, rand_core::RngCore},
};
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

const MAGIC: &[u8; 4] = b"PPW1";
const HEAD: usize = 4 + 12 + 16 + 12 + 48;

/// Argon2id: 64 MiB, 3 passes, 1 lane (about a third of a second on a usual PC). Tests use a light setting.
#[cfg(not(test))]
pub const KDF: (u32, u32, u32) = (64 * 1024, 3, 1);
#[cfg(test)]
pub const KDF: (u32, u32, u32) = (1024, 1, 1);

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The master password does not open this file.
    Wrong,
    /// Not a vault, or damaged.
    Corrupt,
    Io(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Wrong => write!(f, "wrong password"),
            Error::Corrupt => write!(f, "not a vault, or damaged"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Login,
    Totp,
    Note,
    Card,
}

/// One item of the vault. One flat shape for every kind: the fields a kind does not use stay empty.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub folder: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    /// 2FA: a base32 secret or an otpauth:// address.
    pub totp: String,
    pub card_holder: String,
    pub card_number: String,
    pub card_expiry: String,
    pub card_cvv: String,
    /// Unix seconds.
    pub created: u64,
    pub modified: u64,
}

impl Entry {
    pub fn new(kind: Kind) -> Entry {
        let now = now();
        Entry { id: new_id(), kind, created: now, modified: now, ..Default::default() }
    }

    /// Everything a search can match (never the secrets).
    pub fn haystack(&self) -> String {
        format!("{} {} {} {} {} {}", self.title, self.username, self.url, self.folder, self.tags.join(" "), self.card_holder).to_lowercase()
    }

    pub fn wipe(&mut self) {
        for s in [
            &mut self.title,
            &mut self.folder,
            &mut self.username,
            &mut self.password,
            &mut self.url,
            &mut self.notes,
            &mut self.totp,
            &mut self.card_holder,
            &mut self.card_number,
            &mut self.card_expiry,
            &mut self.card_cvv,
        ] {
            s.zeroize();
        }
        for t in &mut self.tags {
            t.zeroize();
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub entries: Vec<Entry>,
    /// Folders, also the empty ones.
    pub folders: Vec<String>,
}

impl Data {
    pub fn wipe(&mut self) {
        for e in &mut self.entries {
            e.wipe();
        }
        self.entries.clear();
        self.folders.clear();
    }

    /// Adds the entries of `other` this vault does not have; for the same item, the newer one wins.
    /// Returns how many were added or updated.
    pub fn merge(&mut self, other: Data) -> usize {
        let mut n = 0;
        for e in other.entries {
            match self.entries.iter_mut().find(|x| x.id == e.id) {
                Some(x) if e.modified > x.modified => {
                    *x = e;
                    n += 1;
                }
                Some(_) => {}
                None => {
                    self.entries.push(e);
                    n += 1;
                }
            }
        }
        for f in other.folders {
            if !self.folders.contains(&f) {
                self.folders.push(f);
            }
        }
        n
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn random(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    OsRng.fill_bytes(&mut b);
    b
}

/// A uniform random number below `n` (no modulo bias).
pub fn random_below(n: u32) -> u32 {
    assert!(n > 0);
    let zone = u32::MAX - (u32::MAX % n);
    loop {
        let v = OsRng.next_u32();
        if v < zone {
            return v % n;
        }
    }
}

fn new_id() -> String {
    random(8).iter().map(|b| format!("{b:02x}")).collect()
}

fn derive(pw: &str, salt: &[u8], kdf: (u32, u32, u32)) -> Result<Zeroizing<[u8; 32]>, Error> {
    let params = Params::new(kdf.0, kdf.1, kdf.2, Some(32)).map_err(|_| Error::Corrupt)?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(pw.as_bytes(), salt, key.as_mut()).map_err(|_| Error::Corrupt)?;
    Ok(key)
}

fn seal(key: &[u8; 32], nonce: &[u8], aad: &[u8], plain: &[u8]) -> Vec<u8> {
    Aes256Gcm::new(key.into()).encrypt(nonce.into(), Payload { msg: plain, aad }).expect("AES-GCM")
}

fn open(key: &[u8; 32], nonce: &[u8], aad: &[u8], sealed: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    Aes256Gcm::new(key.into()).decrypt(nonce.into(), Payload { msg: sealed, aad }).ok().map(Zeroizing::new)
}

/// What the file says before the sealed data.
struct Header {
    bytes: [u8; HEAD],
}

impl Header {
    fn new(pw: &str, data_key: &[u8; 32]) -> Result<Header, Error> {
        let mut bytes = [0u8; HEAD];
        bytes[..4].copy_from_slice(MAGIC);
        bytes[4..8].copy_from_slice(&KDF.0.to_le_bytes());
        bytes[8..12].copy_from_slice(&KDF.1.to_le_bytes());
        bytes[12..16].copy_from_slice(&KDF.2.to_le_bytes());
        bytes[16..32].copy_from_slice(&random(16));
        bytes[32..44].copy_from_slice(&random(12));
        let kek = derive(pw, &bytes[16..32], KDF)?;
        let wrapped = seal(&kek, &bytes[32..44], &bytes[..32], data_key);
        bytes[44..92].copy_from_slice(&wrapped);
        Ok(Header { bytes })
    }

    fn parse(file: &[u8]) -> Result<Header, Error> {
        if file.len() < HEAD + 12 + 16 || &file[..4] != MAGIC {
            return Err(Error::Corrupt);
        }
        let mut bytes = [0u8; HEAD];
        bytes.copy_from_slice(&file[..HEAD]);
        Ok(Header { bytes })
    }

    fn kdf(&self) -> (u32, u32, u32) {
        let u = |i: usize| u32::from_le_bytes(self.bytes[i..i + 4].try_into().unwrap());
        (u(4), u(8), u(12))
    }

    fn data_key(&self, pw: &str) -> Result<Zeroizing<[u8; 32]>, Error> {
        let kdf = self.kdf();
        // a damaged or hostile file must not make us allocate gigabytes
        if !(8..=1024 * 1024).contains(&kdf.0) || !(1..=64).contains(&kdf.1) || !(1..=16).contains(&kdf.2) {
            return Err(Error::Corrupt);
        }
        let kek = derive(pw, &self.bytes[16..32], kdf)?;
        let dk = open(&kek, &self.bytes[32..44], &self.bytes[..32], &self.bytes[44..92]).ok_or(Error::Wrong)?;
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&dk);
        Ok(key)
    }
}

/// The vault file and, once unlocked, its key and its entries.
pub struct Vault {
    pub path: PathBuf,
    key: Option<Zeroizing<[u8; 32]>>,
    header: Option<Header>,
    pub data: Data,
}

impl Vault {
    pub fn new(path: PathBuf) -> Vault {
        Vault { path, key: None, header: None, data: Data::default() }
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    pub fn unlocked(&self) -> bool {
        self.key.is_some()
    }

    /// A new, empty vault sealed with this master password (replaces any file there).
    pub fn create(&mut self, pw: &str) -> Result<(), Error> {
        let key = Zeroizing::new(random(32).try_into().unwrap());
        self.header = Some(Header::new(pw, &key)?);
        self.key = Some(key);
        self.data = Data::default();
        self.save()
    }

    pub fn unlock(&mut self, pw: &str) -> Result<(), Error> {
        let file = std::fs::read(&self.path).map_err(|e| Error::Io(e.to_string()))?;
        let header = Header::parse(&file)?;
        let key = header.data_key(pw)?;
        self.data = decrypt(&file, &key)?;
        self.key = Some(key);
        self.header = Some(header);
        Ok(())
    }

    /// Unlock with the data key itself (Windows Hello keeps a copy of it).
    pub fn unlock_with_key(&mut self, key: &[u8; 32]) -> Result<(), Error> {
        let file = std::fs::read(&self.path).map_err(|e| Error::Io(e.to_string()))?;
        let header = Header::parse(&file)?;
        let key = Zeroizing::new(*key);
        self.data = decrypt(&file, &key)?;
        self.key = Some(key);
        self.header = Some(header);
        Ok(())
    }

    /// The data key, to hand to Windows Hello's copy.
    pub fn key(&self) -> Option<Zeroizing<[u8; 32]>> {
        self.key.clone()
    }

    /// Checks a password against the file, without unlocking anything.
    pub fn check(&self, pw: &str) -> bool {
        std::fs::read(&self.path).ok().and_then(|f| Header::parse(&f).ok()).is_some_and(|h| h.data_key(pw).is_ok())
    }

    pub fn lock(&mut self) {
        self.key = None;
        self.header = None;
        self.data.wipe();
    }

    /// The sealed file, as it would be written now.
    pub fn sealed(&self) -> Result<Vec<u8>, Error> {
        let (Some(key), Some(header)) = (&self.key, &self.header) else { return Err(Error::Wrong) };
        let plain = Zeroizing::new(serde_json::to_vec(&self.data).map_err(|e| Error::Io(e.to_string()))?);
        let nonce = random(12);
        let ct = seal(key, &nonce, &header.bytes, &plain);
        Ok([header.bytes.as_slice(), &nonce, &ct].concat())
    }

    /// Writes the vault: a temporary file first, then it replaces the old one in one step.
    pub fn save(&self) -> Result<(), Error> {
        let bytes = self.sealed()?;
        write_atomic(&self.path, &bytes)
    }

    /// A new master password: a new salt and a new data key (the old file cannot open the new one).
    pub fn change_password(&mut self, pw: &str) -> Result<(), Error> {
        if !self.unlocked() {
            return Err(Error::Wrong);
        }
        let key = Zeroizing::new(random(32).try_into().unwrap());
        self.header = Some(Header::new(pw, &key)?);
        self.key = Some(key);
        self.save()
    }

    /// Reads another vault file (a backup) with its password.
    pub fn read_other(path: &Path, pw: &str) -> Result<Data, Error> {
        let mut v = Vault::new(path.to_path_buf());
        v.unlock(pw)?;
        let data = std::mem::take(&mut v.data);
        v.lock();
        Ok(data)
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        self.data.wipe();
    }
}

fn decrypt(file: &[u8], key: &[u8; 32]) -> Result<Data, Error> {
    let plain = open(key, &file[HEAD..HEAD + 12], &file[..HEAD], &file[HEAD + 12..]).ok_or(Error::Wrong)?;
    serde_json::from_slice(&plain).map_err(|_| Error::Corrupt)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::Io(e.to_string()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| Error::Io(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| Error::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("plume-passwords-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("vault.ppw")
    }

    #[test]
    fn round_trip_and_wrong_password() {
        let path = temp("rt");
        let mut v = Vault::new(path.clone());
        v.create("test master one").unwrap();
        let mut e = Entry::new(Kind::Login);
        e.title = "Example".into();
        e.username = "tester".into();
        e.password = "test-secret-123".into();
        v.data.entries.push(e.clone());
        v.save().unwrap();
        // nothing readable on disk
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(15).any(|w| w == b"test-secret-123"));
        assert!(!raw.windows(6).any(|w| w == b"tester"));
        v.lock();
        assert!(v.data.entries.is_empty());

        let mut w = Vault::new(path.clone());
        assert_eq!(w.unlock("not the password"), Err(Error::Wrong));
        assert!(!w.unlocked());
        assert!(w.check("test master one"));
        assert!(!w.check("test master two"));
        w.unlock("test master one").unwrap();
        assert_eq!(w.data.entries, vec![e.clone()]);

        // the data key alone opens it too (Windows Hello's way)
        let key = w.key().unwrap();
        let mut h = Vault::new(path.clone());
        h.unlock_with_key(&key).unwrap();
        assert_eq!(h.data.entries.len(), 1);

        // a new master password: the old one no longer works, nor the old data key
        w.change_password("test master two").unwrap();
        let mut x = Vault::new(path.clone());
        assert_eq!(x.unlock("test master one"), Err(Error::Wrong));
        assert_eq!(x.unlock_with_key(&key), Err(Error::Wrong));
        x.unlock("test master two").unwrap();
        assert_eq!(x.data.entries, vec![e]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn tampering_is_refused() {
        let path = temp("tamper");
        let mut v = Vault::new(path.clone());
        v.create("test master").unwrap();
        v.lock();
        let good = std::fs::read(&path).unwrap();
        // a weaker parameter in the header, a flipped byte of the data, a cut file
        for (i, cut) in [(8usize, false), (good.len() - 3, false), (0, true)] {
            let mut bad = good.clone();
            if cut {
                bad.truncate(50);
            } else {
                bad[i] ^= 1;
            }
            std::fs::write(&path, &bad).unwrap();
            assert!(Vault::new(path.clone()).unlock("test master").is_err());
        }
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(Vault::new(path.clone()).unlock("test master"), Err(Error::Corrupt));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn merge_keeps_newer() {
        let mut a = Data::default();
        let mut e = Entry::new(Kind::Note);
        e.title = "one".into();
        a.entries.push(e.clone());
        let mut b = Data::default();
        let mut e2 = e.clone();
        e2.title = "one, newer".into();
        e2.modified += 10;
        b.entries.push(e2);
        b.entries.push(Entry::new(Kind::Card));
        b.folders.push("Work".into());
        assert_eq!(a.merge(b), 2);
        assert_eq!(a.entries.len(), 2);
        assert_eq!(a.entries[0].title, "one, newer");
        assert_eq!(a.folders, vec!["Work".to_string()]);
    }

    #[test]
    fn random_below_is_in_range() {
        for n in [1u32, 2, 7, 62, 1000] {
            for _ in 0..200 {
                assert!(random_below(n) < n);
            }
        }
    }
}
