//! Peer sync between this app's copies on the local network (Settings ▸ Sync): the
//! [`lcc_peer::Service`] a session runs, started on demand.
//!
//! The service starts when Settings ▸ Sync looks for devices (`sync.find`), or at launch when
//! paired devices exist (`sync.start`), never by itself: a session that never asks opens no
//! sockets. Looking for devices shows in the activity stack; cancelling that row stops looking.
//! The paired devices and this device's identity live in `<settings>/sync/peers.json`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use crate::activity::{Cancel, TaskGuard};
use crate::{EngineError, Result, Session};

/// The session's peer-sync state.
#[derive(Default)]
pub struct State {
    /// `<settings>/sync/peers.json`; `None` keeps the feature off (tests, `--memory`).
    pub store_path: Option<PathBuf>,
    /// The discovery port; `None` = [`lcc_peer::beacon::PORT`] (tests use their own).
    pub port: Option<u16>,
    service: Option<lcc_peer::Service>,
    /// The activity row while looking for devices, and its cancel flag.
    finding: Option<(TaskGuard, Arc<AtomicBool>)>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State").field("store_path", &self.store_path).field("running", &self.service.is_some()).finish_non_exhaustive()
    }
}

/// What a new device is called: the computer's name where the system says it, else the platform.
pub fn default_device_name() -> String {
    let host = std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty() && h != "localhost");
    host.unwrap_or_else(|| {
        if cfg!(target_os = "android") {
            "Android tablet".into()
        } else if cfg!(target_os = "macos") {
            "Mac".into()
        } else if cfg!(windows) {
            "Windows PC".into()
        } else {
            "Computer".into()
        }
    })
}

fn other(e: lcc_peer::Error) -> EngineError {
    EngineError::Other(e.to_string())
}

impl State {
    /// The service, started if needed.
    fn ensure(&mut self) -> Result<&lcc_peer::Service> {
        if self.service.is_none() {
            let path = self.store_path.clone().ok_or_else(|| EngineError::Other("sync needs a settings folder".into()))?;
            let svc = lcc_peer::Service::start(path, &default_device_name(), self.port.unwrap_or(lcc_peer::beacon::PORT)).map_err(other)?;
            self.service = Some(svc);
        }
        self.service.as_ref().ok_or_else(|| EngineError::Other("sync isn't running".into()))
    }

    /// Stops looking for devices if the activity row was cancelled.
    fn reconcile(&mut self) {
        let cancelled = self.finding.as_ref().is_some_and(|(_, flag)| flag.load(Ordering::Relaxed));
        if cancelled {
            self.finding = None;
            if let Some(s) = &self.service {
                s.set_finding(false);
            }
        }
    }

    /// The stored identity and paired devices without starting anything.
    fn stored(&self) -> Option<lcc_peer::Store> {
        let path = self.store_path.as_ref()?;
        path.is_file().then(|| lcc_peer::Store::load_or_create(path, "").ok()).flatten()
    }
}

pub(crate) fn status(s: &mut Session) -> Result<Value> {
    let st = &mut s.peers;
    st.reconcile();
    if let Some(svc) = &st.service {
        let mut v = serde_json::to_value(svc.status()).map_err(|e| EngineError::Other(e.to_string()))?;
        if let Some(o) = v.as_object_mut() {
            o.insert("running".into(), json!(true));
            o.insert("available".into(), json!(true));
        }
        return Ok(v);
    }
    let stored = st.stored();
    Ok(json!({
        "running": false,
        "available": st.store_path.is_some(),
        "deviceId": stored.as_ref().map(|s| s.identity.device_id.clone()),
        "name": stored.as_ref().map_or_else(default_device_name, |s| s.identity.name.clone()),
        "finding": false,
        "found": [],
        "paired": stored.map(|s| s.peers.iter().map(|p| json!({"deviceId": p.device_id, "name": p.name, "online": false, "pairedAt": p.paired_at})).collect::<Vec<_>>()).unwrap_or_default(),
        "pairing": null,
    }))
}

pub(crate) fn find(s: &mut Session, on: bool) -> Result<Value> {
    let activity = s.activity.clone();
    let st = &mut s.peers;
    if on {
        st.ensure()?.set_finding(true);
        if st.finding.is_none() {
            let flag = Arc::new(AtomicBool::new(false));
            let guard = activity.start("sync", "Looking for devices", Cancel::Flag(flag.clone()));
            st.finding = Some((guard, flag));
        }
    } else {
        st.finding = None;
        if let Some(svc) = &st.service {
            svc.set_finding(false);
        }
    }
    status(s)
}

/// Starts the service when paired devices exist (at launch), so they can find each other.
pub(crate) fn start(s: &mut Session) -> Result<Value> {
    let st = &mut s.peers;
    if st.service.is_none() && st.stored().is_some_and(|store| !store.peers.is_empty()) {
        st.ensure()?;
    }
    Ok(json!({"running": st.service.is_some()}))
}

pub(crate) fn pair(s: &mut Session, device: &str) -> Result<Value> {
    s.peers.ensure()?.pair(device).map_err(other)?;
    status(s)
}

pub(crate) fn confirm(s: &mut Session, accept: bool) -> Result<Value> {
    s.peers.ensure()?.confirm(accept).map_err(other)?;
    status(s)
}

pub(crate) fn clear_pairing(s: &mut Session) -> Result<Value> {
    if let Some(svc) = &s.peers.service {
        svc.clear_pairing();
    }
    status(s)
}

pub(crate) fn unpair(s: &mut Session, device: &str) -> Result<Value> {
    let removed = match &s.peers.service {
        Some(svc) => svc.unpair(device).map_err(other)?,
        None => {
            let path = s.peers.store_path.clone().ok_or_else(|| EngineError::Other("sync needs a settings folder".into()))?;
            let mut store = s.peers.stored().ok_or_else(|| EngineError::Other("no device is paired".into()))?;
            let removed = store.remove_peer(device);
            if removed {
                store.save(&path).map_err(other)?;
            }
            removed
        }
    };
    if !removed {
        return Err(EngineError::Other(format!("no paired device `{device}`")));
    }
    status(s)
}

pub(crate) fn rename(s: &mut Session, name: &str) -> Result<Value> {
    s.peers.ensure()?.rename(name).map_err(other)?;
    status(s)
}

impl Session {
    /// Keep the peer-sync identity and paired devices in `<settings>/sync/peers.json`.
    pub fn with_default_peers(mut self) -> Self {
        self.peers.store_path = crate::config::config_dir().map(|d| d.join("sync").join("peers.json"));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn session(dir: &std::path::Path, name: &str, port: u16) -> Session {
        let mut s = Session::new();
        s.peers.store_path = Some(dir.join(format!("{name}.json")));
        s.peers.port = Some(port);
        s
    }

    fn until(s: &mut Session, what: &str, f: impl Fn(&Value) -> bool) -> Value {
        let t = Instant::now();
        loop {
            let v = s.execute("sync.status", &json!({})).unwrap();
            if f(&v) {
                return v;
            }
            assert!(t.elapsed() < Duration::from_secs(20), "timed out waiting for {what}: {v}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Settings ▸ Sync's whole flow through commands: both look, one pairs, both confirm the same
    /// code, both list the other as paired; looking shows in the activity stack until stopped.
    #[test]
    fn two_sessions_pair_through_the_commands() {
        let dir = std::env::temp_dir().join(format!("lcc-engine-peers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let port = 30000 + (std::process::id() % 9000) as u16;
        let (mut a, mut b) = (session(&dir, "a", port), session(&dir, "b", port));
        // nothing runs before it is asked for
        assert_eq!(a.execute("sync.status", &json!({})).unwrap()["running"], json!(false));
        assert_eq!(a.execute("sync.start", &json!({})).unwrap()["running"], json!(false));
        a.execute("sync.find", &json!({"on": true})).unwrap();
        b.execute("sync.find", &json!({"on": true})).unwrap();
        assert!(a.activity.list().iter().any(|r| r.kind == "sync"));
        let b_id = b.execute("sync.status", &json!({})).unwrap()["deviceId"].as_str().unwrap().to_string();
        until(&mut a, "a to find b", |v| v["found"].as_array().is_some_and(|f| f.iter().any(|d| d["deviceId"] == b_id.as_str())));
        a.execute("sync.pair", &json!({"device": b_id})).unwrap();
        let sa = until(&mut a, "a's code", |v| v["pairing"]["stage"] == "confirm");
        let sb = until(&mut b, "b's code", |v| v["pairing"]["stage"] == "confirm");
        assert_eq!(sa["pairing"]["code"], sb["pairing"]["code"]);
        a.execute("sync.confirm", &json!({"accept": true})).unwrap();
        b.execute("sync.confirm", &json!({"accept": true})).unwrap();
        until(&mut a, "a paired", |v| v["paired"].as_array().is_some_and(|p| p.len() == 1));
        until(&mut b, "b paired", |v| v["paired"].as_array().is_some_and(|p| p.len() == 1));
        a.execute("sync.find", &json!({"on": false})).unwrap();
        assert!(!a.activity.list().iter().any(|r| r.kind == "sync"));
        // bad parameters are errors, not panics
        for (cmd, p) in [
            ("sync.find", json!({})),
            ("sync.pair", json!({"device": ""})),
            ("sync.confirm", json!({"accept": "yes"})),
            ("sync.unpair", json!({"device": "nobody"})),
        ] {
            assert!(a.execute(cmd, &p).is_err(), "{cmd}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
