//! What Plume Viewer keeps between launches: %LOCALAPPDATA%\Plume\Viewer\viewer.json
//! (PLUME_VIEWER_DIR for tests). The permanent password is never stored: only the key derived from it, sealed by
//! Windows for this user (DPAPI).

use crate::crypto;
use serde_json::{Value, json};
use std::path::PathBuf;

/// The relay that puts devices in touch (set at build time by PLUME_VIEWER_RELAY_URL); "relay" in viewer.json replaces it.
pub const RELAY: &str = match option_env!("PLUME_VIEWER_RELAY_URL") {
    Some(u) => u,
    None => "https://plume-viewer.sxhmacrypto.workers.dev",
};

pub struct Store {
    /// The 9-digit ID of this PC.
    pub id: String,
    /// Proves to the relay that this PC owns its ID (never shown).
    pub token: String,
    /// Salt of the permanent password, and its key sealed by DPAPI (hex). Empty: no permanent password.
    pub pw_salt: String,
    pub pw_sealed: String,
    /// Starts with Windows, in the background (needed to reach the PC with its password).
    pub autostart: bool,
    pub relay: String,
}

pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PLUME_VIEWER_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Plume").join("Viewer")
}

fn file() -> PathBuf {
    dir().join("viewer.json")
}

impl Store {
    pub fn load() -> Store {
        let v: Value = std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let mut st = Store {
            id: s("id"),
            token: s("token"),
            pw_salt: s("pw_salt"),
            pw_sealed: s("pw_sealed"),
            autostart: v["autostart"].as_bool().unwrap_or(false),
            relay: s("relay"),
        };
        let fresh = st.id.len() != 9 || !st.id.bytes().all(|b| b.is_ascii_digit()) || st.token.len() < 32;
        if fresh {
            st.id = crypto::new_id();
            st.token = crypto::hex(&crypto::random(24));
            st.save();
        }
        st
    }

    pub fn save(&self) {
        let v = json!({
            "id": self.id,
            "token": self.token,
            "pw_salt": self.pw_salt,
            "pw_sealed": self.pw_sealed,
            "autostart": self.autostart,
            "relay": self.relay,
        });
        let _ = std::fs::create_dir_all(dir());
        let tmp = file().with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_string_pretty(&v).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, file());
        }
    }

    pub fn relay_url(&self) -> String {
        // tests point to a local relay; a published build only listens to viewer.json (a relay of one's own)
        let test = std::env::var("PLUME_VIEWER_RELAY").ok().filter(|u| cfg!(feature = "test-hook") && !u.is_empty());
        let u = test.unwrap_or_else(|| if self.relay.is_empty() { RELAY.to_string() } else { self.relay.clone() });
        u.trim_end_matches('/').to_string()
    }

    pub fn has_password(&self) -> bool {
        !self.pw_sealed.is_empty() && !self.pw_salt.is_empty()
    }

    /// The key of the permanent password (None: no password, or Windows cannot unseal it).
    pub fn password_key(&self) -> Option<Vec<u8>> {
        if !self.has_password() {
            return None;
        }
        crypto::unprotect(&crypto::unhex(&self.pw_sealed)?)
    }

    pub fn set_password(&mut self, password: Option<&str>) -> bool {
        match password {
            None => {
                self.pw_salt.clear();
                self.pw_sealed.clear();
            }
            Some(p) => {
                let salt = crypto::random(16);
                let key = crypto::password_key(p, &salt);
                let Some(sealed) = crypto::protect(&key) else { return false };
                self.pw_salt = crypto::hex(&salt);
                self.pw_sealed = crypto::hex(&sealed);
            }
        }
        self.save();
        true
    }
}
