//! The UDP broadcast each running copy of the app sends every couple of seconds while it looks
//! for devices (Settings ▸ Sync open) or has paired devices: who it is and where to connect.

use serde::{Deserialize, Serialize};

/// The UDP port beacons go to and are heard on.
pub const PORT: u16 = 47821;
/// The largest beacon read or sent.
pub const MAX_LEN: usize = 1024;
/// The beacon format; others are ignored.
pub const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Beacon {
    /// Which app sent it (`env_prefix` from brand.toml): other apps' beacons are ignored.
    pub app: String,
    pub v: u32,
    pub device_id: String,
    pub name: String,
    /// Ed25519 public key, base64.
    pub key: String,
    /// The TCP port it accepts connections on.
    pub port: u16,
    /// It is looking for devices to pair with (its Settings ▸ Sync is open).
    pub pairing: bool,
}

impl Beacon {
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// A beacon of this app and version, from a datagram; anything else is `None`.
    pub fn decode(bytes: &[u8]) -> Option<Beacon> {
        if bytes.len() > MAX_LEN {
            return None;
        }
        let b: Beacon = serde_json::from_slice(bytes).ok()?;
        let sane = b.app == lcc_brand::ENV_PREFIX
            && b.v == VERSION
            && b.port != 0
            && (1..=64).contains(&b.device_id.len())
            && b.device_id.chars().all(|c| c.is_ascii_hexdigit())
            && b.name.chars().count() <= crate::identity::MAX_NAME
            && b.key.len() <= 64;
        sane.then_some(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Beacon {
        Beacon {
            app: lcc_brand::ENV_PREFIX.into(),
            v: VERSION,
            device_id: "ab12".into(),
            name: "Tablet".into(),
            key: "k".into(),
            port: 4000,
            pairing: true,
        }
    }

    #[test]
    fn round_trips() {
        assert_eq!(Beacon::decode(&sample().encode()), Some(sample()));
    }

    #[test]
    fn hostile_datagrams_are_ignored() {
        let mut other = sample();
        other.app = "SOMEONE_ELSE".into();
        let mut bad_id = sample();
        bad_id.device_id = "../../etc".into();
        let mut no_port = sample();
        no_port.port = 0;
        for b in [other, bad_id, no_port] {
            assert_eq!(Beacon::decode(&b.encode()), None);
        }
        for raw in [&b""[..], b"{", b"null", b"\xff\x00", &[b'a'; MAX_LEN + 1]] {
            assert_eq!(Beacon::decode(raw), None);
        }
    }
}
