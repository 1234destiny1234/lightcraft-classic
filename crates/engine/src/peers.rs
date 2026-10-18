//! Peer sync between this app's copies on the local network (Settings ▸ Sync): the
//! [`lcc_peer::Service`] a session runs, started on demand.
//!
//! The service starts when Settings ▸ Sync looks for devices (`sync.find`), or at launch when
//! paired devices exist (`sync.start`), never by itself: a session that never asks opens no
//! sockets. Looking for devices shows in the activity stack; cancelling that row stops looking.
//! The paired devices and this device's identity live in `<settings>/sync/peers.json`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};

use crate::activity::{Activity, Cancel, TaskGuard};
use crate::peer_exchange::{self as px, Action, Base, CatalogInfo, LibrarySync, Record};
use crate::{EngineError, Result, Session};

/// How often the device with the lower id starts an exchange with each paired device online.
const EXCHANGE_EVERY: Duration = Duration::from_secs(15);
/// How often the shared records are refreshed for incoming exchanges.
const PUBLISH_EVERY: Duration = Duration::from_secs(3);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// This library as the exchange threads see it (refreshed by `sync.tick`).
#[derive(Clone)]
struct Published {
    library: PathBuf,
    name: String,
    sync: LibrarySync,
    records: Arc<px::Records>,
}

/// How the last exchange with a device went.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Exchange {
    running: bool,
    /// When it last finished (Unix seconds).
    last: u64,
    error: Option<String>,
    /// Photos that took the other device's edit, and copies made for edits on both sides.
    taken: usize,
    copies: usize,
}

/// Shared between the session and the exchange threads.
struct Hub {
    activity: Activity,
    published: Mutex<Option<Published>>,
    outcomes: Mutex<Vec<(lcc_peer::Peer, PathBuf, std::result::Result<px::Outcome, String>)>>,
    running: Mutex<HashSet<String>>,
    tried: Mutex<HashMap<String, Instant>>,
    offers: Mutex<Vec<(lcc_peer::Peer, CatalogInfo)>>,
    exchanges: Mutex<HashMap<String, Exchange>>,
}

impl Hub {
    /// Runs one exchange with `peer` on this thread and queues what came back.
    fn run(&self, peer: &lcc_peer::Peer, ch: &mut lcc_peer::Channel, initiator: bool) {
        let published = lock(&self.published).clone();
        let library = published.as_ref().map(|p| p.library.clone()).unwrap_or_default();
        let result = {
            let catalog = published
                .as_ref()
                .filter(|p| p.sync.shared_with(&peer.device_id))
                .and_then(|p| p.sync.sync_id.clone().map(|id| CatalogInfo { sync_id: id, name: p.name.clone() }));
            let base_remote = match (&published, &catalog) {
                (Some(p), Some(c)) => Base::load(&p.library, &peer.device_id, &c.sync_id).remote,
                _ => HashMap::new(),
            };
            let records = published.as_ref().map(|p| p.records.clone()).unwrap_or_default();
            let local = px::Local { catalog, records, base_remote };
            // the activity row only while records actually travel
            let mut task: Option<TaskGuard> = None;
            let activity = self.activity.clone();
            px::exchange(ch, initiator, &local, &mut |n| {
                if n > 0 && task.is_none() {
                    task = Some(activity.start("sync", "Syncing library", Cancel::No));
                }
            })
        };
        lock(&self.running).remove(&peer.device_id);
        lock(&self.outcomes).push((peer.clone(), library, result));
    }
}

/// The session's peer-sync state.
#[derive(Default)]
pub struct State {
    /// `<settings>/sync/peers.json`; `None` keeps the feature off (tests, `--memory`).
    pub store_path: Option<PathBuf>,
    /// The discovery port; `None` = [`lcc_peer::beacon::PORT`] (tests use their own).
    pub port: Option<u16>,
    service: Option<Arc<lcc_peer::Service>>,
    /// The activity row while looking for devices, and its cancel flag.
    finding: Option<(TaskGuard, Arc<AtomicBool>)>,
    hub: Option<Arc<Hub>>,
    published_at: Option<Instant>,
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
    /// The service, started if needed (with the exchange hub answering paired devices).
    fn ensure(&mut self, activity: &Activity) -> Result<&Arc<lcc_peer::Service>> {
        if self.service.is_none() {
            let path = self.store_path.clone().ok_or_else(|| EngineError::Other("sync needs a settings folder".into()))?;
            let svc = Arc::new(lcc_peer::Service::start(path, &default_device_name(), self.port.unwrap_or(lcc_peer::beacon::PORT)).map_err(other)?);
            let hub = Arc::new(Hub {
                activity: activity.clone(),
                published: Mutex::new(None),
                outcomes: Mutex::new(Vec::new()),
                running: Mutex::new(HashSet::new()),
                tried: Mutex::new(HashMap::new()),
                offers: Mutex::new(Vec::new()),
                exchanges: Mutex::new(HashMap::new()),
            });
            let h = hub.clone();
            svc.set_hello_handler(Arc::new(move |peer, mut ch| {
                lock(&h.running).insert(peer.device_id.clone());
                h.run(&peer, &mut ch, false);
            }));
            self.hub = Some(hub);
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
    let mut v = base_status(s)?;
    if let Some(o) = v.as_object_mut() {
        o.insert("library".into(), library_status(s));
        let (offers, exchanges) = match &s.peers.hub {
            Some(h) => (
                lock(&h.offers).iter().map(|(p, c)| json!({"deviceId": p.device_id, "name": p.name, "library": c.name})).collect::<Vec<_>>(),
                serde_json::to_value(&*lock(&h.exchanges)).unwrap_or(Value::Null),
            ),
            None => (Vec::new(), json!({})),
        };
        o.insert("offers".into(), json!(offers));
        o.insert("exchanges".into(), exchanges);
    }
    Ok(v)
}

fn base_status(s: &mut Session) -> Result<Value> {
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
        st.ensure(&activity)?.set_finding(true);
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
    let activity = s.activity.clone();
    let st = &mut s.peers;
    if st.service.is_none() && st.stored().is_some_and(|store| !store.peers.is_empty()) {
        st.ensure(&activity)?;
    }
    Ok(json!({"running": st.service.is_some()}))
}

pub(crate) fn pair(s: &mut Session, device: &str) -> Result<Value> {
    let a = s.activity.clone();
    s.peers.ensure(&a)?.pair(device).map_err(other)?;
    status(s)
}

pub(crate) fn confirm(s: &mut Session, accept: bool) -> Result<Value> {
    let a = s.activity.clone();
    s.peers.ensure(&a)?.confirm(accept).map_err(other)?;
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
    let a = s.activity.clone();
    s.peers.ensure(&a)?.rename(name).map_err(other)?;
    status(s)
}

impl Session {
    /// Keep the peer-sync identity and paired devices in `<settings>/sync/peers.json`.
    pub fn with_default_peers(mut self) -> Self {
        self.peers.store_path = crate::config::config_dir().map(|d| d.join("sync").join("peers.json"));
        self
    }
}

/// The open library, if it is a folder on disk.
fn library_dir(s: &Session) -> Option<PathBuf> {
    s.library.as_ref().filter(|l| l.on_disk).map(|l| l.dir.clone())
}

fn library_name(dir: &std::path::Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Library".into())
}

fn library_status(s: &Session) -> Value {
    let Some(dir) = library_dir(s) else { return Value::Null };
    let sync = LibrarySync::load(&dir);
    json!({"name": library_name(&dir), "syncId": sync.sync_id, "sharedWith": sync.shared_with})
}

/// Shares (or stops sharing) the open library with a paired device.
pub(crate) fn share(s: &mut Session, device: &str, on: bool) -> Result<Value> {
    let dir = library_dir(s).ok_or_else(|| EngineError::Other("only a library on disk can be shared".into()))?;
    if on && !s.peers.stored().is_some_and(|st| st.peer(device).is_some()) {
        return Err(EngineError::Other(format!("not paired with `{device}`")));
    }
    let mut sync = LibrarySync::load(&dir);
    if on {
        if sync.sync_id.is_none() {
            sync.sync_id = Some(lcc_peer::identity::random_id().map_err(other)?);
        }
        if !sync.shared_with.iter().any(|d| d == device) {
            sync.shared_with.push(device.to_string());
        }
    } else {
        sync.shared_with.retain(|d| d != device);
    }
    sync.save(&dir).map_err(EngineError::Other)?;
    s.peers.published_at = None;
    status(s)
}

/// Takes up (or turns down) a library another device offered: this library becomes the same catalog.
pub(crate) fn answer_offer(s: &mut Session, device: &str, accept: bool) -> Result<Value> {
    let dir = library_dir(s).ok_or_else(|| EngineError::Other("only a library on disk can be shared".into()))?;
    let hub = s.peers.hub.clone().ok_or_else(|| EngineError::Other("no library was offered".into()))?;
    let offer = {
        let mut offers = lock(&hub.offers);
        let i = offers.iter().position(|(p, _)| p.device_id == device).ok_or_else(|| EngineError::Other("that device offered no library".into()))?;
        offers.remove(i).1
    };
    let mut sync = LibrarySync::load(&dir);
    if accept {
        sync.sync_id = Some(offer.sync_id);
        if !sync.shared_with.iter().any(|d| d == device) {
            sync.shared_with.push(device.to_string());
        }
    } else {
        sync.declined.push((device.to_string(), offer.sync_id));
    }
    sync.save(&dir).map_err(EngineError::Other)?;
    s.peers.published_at = None;
    status(s)
}

/// Writes one record onto a photo (only what differs), as one undoable step.
fn apply_record(s: &mut Session, id: lcc_catalog::PhotoId, r: &Record, label: &str) -> Result<()> {
    let Some(p) = s.catalog.photo(id).cloned() else { return Ok(()) };
    if p.develop != r.develop {
        s.set_develop(id, (*r.develop).clone(), label)?;
    }
    if p.rating != r.rating {
        s.commit(label, lcc_catalog::Op::SetRating { id, rating: r.rating.min(5) })?;
    }
    if p.flag != r.flag {
        s.commit(label, lcc_catalog::Op::SetFlag { id, flag: r.flag })?;
    }
    if p.label != r.label {
        s.commit(label, lcc_catalog::Op::SetLabel { id, label: r.label })?;
    }
    Ok(())
}

/// Merges what an exchange brought back into the open library (it must still be the same one).
fn merge(s: &mut Session, peer: &lcc_peer::Peer, library: &std::path::Path, o: px::Outcome) -> Result<(usize, usize)> {
    let Some(sync_id) = o.sync_id else { return Ok((0, 0)) };
    if library_dir(s).as_deref() != Some(library) || LibrarySync::load(library).sync_id.as_deref() != Some(&sync_id) {
        return Ok((0, 0));
    }
    let mut base = Base::load(library, &peer.device_id, &sync_id);
    let current = px::records(&s.catalog);
    let label = format!("Sync from {}", peer.name);
    let (mut taken, mut copies) = (0, 0);
    for (key, remote_digest) in &o.remote {
        let Some((id, local)) = current.get(key) else { continue };
        match px::decide(local, remote_digest, o.records.get(key), &base, key) {
            Action::Take(r) => {
                apply_record(s, *id, &r, &label)?;
                taken += 1;
            }
            Action::Copy(r) => {
                let name = format!("{} edit", peer.name);
                if let Some(copy) = crate::cmd::organize::virtual_copy(s, *id, Some(&name))? {
                    apply_record(s, copy, &r, &label)?;
                    copies += 1;
                }
            }
            Action::Keep => {}
        }
        if let Some(p) = s.catalog.photo(*id) {
            base.local.insert(key.clone(), Record::of(p).digest());
        }
        base.remote.insert(key.clone(), remote_digest.clone());
    }
    base.last = now_secs();
    base.save(library, &peer.device_id).map_err(EngineError::Other)?;
    Ok((taken, copies))
}

/// `sync.tick`, every second or so while sync runs: refreshes what incoming exchanges see,
/// starts exchanges with paired devices online (the device with the lower id starts), and
/// merges what finished exchanges brought back.
pub(crate) fn tick(s: &mut Session) -> Result<Value> {
    let (Some(svc), Some(hub)) = (s.peers.service.clone(), s.peers.hub.clone()) else { return Ok(json!({"running": false})) };
    if s.peers.published_at.is_none_or(|t| t.elapsed() >= PUBLISH_EVERY) {
        s.peers.published_at = Some(Instant::now());
        let published = library_dir(s).map(|dir| {
            let sync = LibrarySync::load(&dir);
            let records = if sync.sync_id.is_some() { px::records(&s.catalog) } else { px::Records::new() };
            Published { name: library_name(&dir), library: dir, sync, records: Arc::new(records) }
        });
        *lock(&hub.published) = published;
    }
    let me = svc.device_id();
    for peer in svc.online_peers() {
        if me >= peer.device_id || lock(&hub.running).contains(&peer.device_id) {
            continue;
        }
        let due = lock(&hub.tried).get(&peer.device_id).is_none_or(|t| t.elapsed() >= EXCHANGE_EVERY);
        if !due {
            continue;
        }
        lock(&hub.tried).insert(peer.device_id.clone(), Instant::now());
        lock(&hub.running).insert(peer.device_id.clone());
        lock(&hub.exchanges).entry(peer.device_id.clone()).or_default().running = true;
        let (svc, hub) = (svc.clone(), hub.clone());
        let spawned = std::thread::Builder::new().name("peer-exchange".into()).spawn(move || match svc.connect(&peer.device_id) {
            Ok(mut ch) => hub.run(&peer, &mut ch, true),
            Err(e) => {
                lock(&hub.running).remove(&peer.device_id);
                lock(&hub.outcomes).push((peer, PathBuf::new(), Err(e.to_string())));
            }
        });
        if let Err(e) = spawned {
            log::warn!("sync: no thread for an exchange: {e}");
        }
    }
    let outcomes = std::mem::take(&mut *lock(&hub.outcomes));
    let (mut taken, mut copies) = (0, 0);
    for (peer, library, result) in outcomes {
        let mut record = Exchange { last: now_secs(), ..Default::default() };
        match result {
            Err(e) => record.error = Some(e),
            Ok(mut o) => {
                if let Some(offer) = o.offer.take() {
                    let declined = library_dir(s)
                        .is_some_and(|d| LibrarySync::load(&d).declined.iter().any(|(dev, id)| *dev == peer.device_id && *id == offer.sync_id));
                    let mut offers = lock(&hub.offers);
                    if !declined && !offers.iter().any(|(p, _)| p.device_id == peer.device_id) {
                        offers.push((peer.clone(), offer));
                    }
                }
                match merge(s, &peer, &library, o) {
                    Ok((t, c)) => {
                        record.taken = t;
                        record.copies = c;
                        taken += t;
                        copies += c;
                    }
                    Err(e) => record.error = Some(e.to_string()),
                }
            }
        }
        record.running = lock(&hub.running).contains(&peer.device_id);
        lock(&hub.exchanges).insert(peer.device_id.clone(), record);
    }
    Ok(json!({"running": true, "taken": taken, "copies": copies}))
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
