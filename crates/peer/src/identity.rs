//! This device's identity (a random id, a name people see, an Ed25519 key) and the devices it is
//! paired with, kept in one JSON file (`<settings>/sync/peers.json`).
//!
//! The private key sits in that file, readable only by the user on Unix. It only proves which
//! device is talking; it guards no photo data at rest.

use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// The longest device name kept (characters).
pub const MAX_NAME: usize = 64;

/// This device.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    /// Random, 32 hex digits; never changes.
    pub device_id: String,
    /// What other devices show.
    pub name: String,
    /// The Ed25519 secret key, base64.
    secret: String,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity").field("device_id", &self.device_id).field("name", &self.name).finish_non_exhaustive()
    }
}

pub(crate) fn random<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| Error::Io(format!("no random source: {e}")))?;
    Ok(b)
}

/// A random id: 32 hex digits.
pub fn random_id() -> Result<String> {
    Ok(hex(&random::<16>()?))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A device name cut to [`MAX_NAME`] characters, without control characters; `fallback` if empty.
pub fn clean_name(name: &str, fallback: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
    let s = s.trim();
    if s.is_empty() { fallback.to_string() } else { s.to_string() }
}

impl Identity {
    pub fn generate(name: &str) -> Result<Identity> {
        let id: [u8; 16] = random()?;
        let secret: [u8; 32] = random()?;
        Ok(Identity { device_id: hex(&id), name: clean_name(name, "Device"), secret: B64.encode(secret) })
    }

    fn signing_key(&self) -> Result<SigningKey> {
        let bytes = B64.decode(&self.secret).map_err(|_| Error::Corrupt("the device key".into()))?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| Error::Corrupt("the device key".into()))?;
        Ok(SigningKey::from_bytes(&bytes))
    }

    /// The public key, base64 (what beacons and paired devices carry).
    pub fn public_key(&self) -> Result<String> {
        Ok(B64.encode(self.signing_key()?.verifying_key().to_bytes()))
    }

    pub(crate) fn sign(&self, msg: &[u8]) -> Result<[u8; 64]> {
        Ok(self.signing_key()?.sign(msg).to_bytes())
    }
}

/// Checks `sig` over `msg` by the device whose public key (base64) is `key`.
pub(crate) fn verify(key: &str, msg: &[u8], sig: &[u8]) -> bool {
    let Ok(bytes) = B64.decode(key) else { return false };
    let Ok(bytes) = <[u8; 32]>::try_from(bytes) else { return false };
    let Ok(vk) = VerifyingKey::from_bytes(&bytes) else { return false };
    let Ok(sig) = Signature::from_slice(sig) else { return false };
    vk.verify(msg, &sig).is_ok()
}

/// A device this one is paired with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub device_id: String,
    pub name: String,
    /// Its Ed25519 public key, base64: a connection claiming this device must prove it.
    pub public_key: String,
    /// When it was paired (Unix seconds).
    pub paired_at: u64,
}

/// What `peers.json` holds.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub identity: Identity,
    #[serde(default)]
    pub peers: Vec<Peer>,
}

/// The largest `peers.json` read (a damaged or hostile file is refused, not loaded).
const MAX_FILE: u64 = 1 << 20;

impl Store {
    /// The store at `path`; a new identity (named `name`) when there is no file yet.
    pub fn load_or_create(path: &Path, name: &str) -> Result<Store> {
        match std::fs::metadata(path) {
            Ok(m) if m.len() > MAX_FILE => return Err(Error::Corrupt(format!("{} is too large", path.display()))),
            Ok(_) => {
                let bytes = std::fs::read(path).map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
                let store: Store = serde_json::from_slice(&bytes).map_err(|e| Error::Corrupt(format!("{}: {e}", path.display())))?;
                store.identity.signing_key()?;
                return Ok(store);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::Io(format!("{}: {e}", path.display()))),
        }
        let store = Store { identity: Identity::generate(name)?, peers: Vec::new() };
        store.save(path)?;
        Ok(store)
    }

    /// Writes the store atomically (temporary file, then rename), private to the user on Unix.
    pub fn save(&self, path: &Path) -> Result<()> {
        let io = |e: std::io::Error| Error::Io(format!("{}: {e}", path.display()));
        let json = serde_json::to_vec_pretty(self).map_err(|e| Error::Io(e.to_string()))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = path.with_extension("json.tmp");
        {
            use std::io::Write as _;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                opts.mode(0o600);
            }
            let mut f = opts.open(&tmp).map_err(io)?;
            f.write_all(&json).map_err(io)?;
            f.sync_all().map_err(io)?;
        }
        std::fs::rename(&tmp, path).map_err(io)
    }

    pub fn peer(&self, device_id: &str) -> Option<&Peer> {
        self.peers.iter().find(|p| p.device_id == device_id)
    }

    /// Adds or refreshes a paired device.
    pub fn add_peer(&mut self, peer: Peer) {
        self.peers.retain(|p| p.device_id != peer.device_id);
        self.peers.push(peer);
    }

    pub fn remove_peer(&mut self, device_id: &str) -> bool {
        let before = self.peers.len();
        self.peers.retain(|p| p.device_id != device_id);
        self.peers.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_check_out_and_forgeries_dont() {
        let a = Identity::generate("A").unwrap();
        let b = Identity::generate("B").unwrap();
        let sig = a.sign(b"hello").unwrap();
        assert!(verify(&a.public_key().unwrap(), b"hello", &sig));
        assert!(!verify(&b.public_key().unwrap(), b"hello", &sig));
        assert!(!verify(&a.public_key().unwrap(), b"hellO", &sig));
        assert!(!verify("not base64!", b"hello", &sig));
        assert!(!verify(&a.public_key().unwrap(), b"hello", &sig[..10]));
    }

    #[test]
    fn the_store_round_trips_and_keeps_the_identity() {
        let dir = std::env::temp_dir().join(format!("lcc-peer-store-{}", std::process::id()));
        let path = dir.join("peers.json");
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Store::load_or_create(&path, "Tablet").unwrap();
        s.add_peer(Peer { device_id: "x".into(), name: "Desk".into(), public_key: s.identity.public_key().unwrap(), paired_at: 1 });
        s.save(&path).unwrap();
        let again = Store::load_or_create(&path, "Other").unwrap();
        assert_eq!(again.identity.device_id, s.identity.device_id);
        assert_eq!(again.peers, s.peers);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_store_is_an_error_not_a_panic() {
        let dir = std::env::temp_dir().join(format!("lcc-peer-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (i, bytes) in
            [&b"{"[..], b"[]", b"{\"identity\":{\"deviceId\":\"a\",\"name\":\"b\",\"secret\":\"AAAA\"}}", b"\xff\xfe"].iter().enumerate()
        {
            let path = dir.join(format!("p{i}.json"));
            std::fs::write(&path, bytes).unwrap();
            assert!(Store::load_or_create(&path, "x").is_err(), "{i}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  \u{7}Tab\nlet ", "D"), "Tablet");
        assert_eq!(clean_name("", "D"), "D");
        assert_eq!(clean_name(&"x".repeat(200), "D").chars().count(), MAX_NAME);
    }
}
