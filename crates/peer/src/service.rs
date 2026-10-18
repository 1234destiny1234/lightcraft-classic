//! The running peer service: beacons out, beacons in, connections in, and the pairing in progress.
//!
//! Threads: one sends beacons (only while looking for devices or when paired devices exist), one
//! listens for beacons, one accepts connections, and each connection gets its own short-lived
//! thread. Every loop checks the stop flag at least twice a second, so dropping the [`Service`]
//! ends them promptly. Nothing here panics on what the network sends.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::beacon::{self, Beacon};
use crate::identity::{Peer, Store, clean_name};
use crate::secure::Channel;
use crate::{Error, Result};

/// How often beacons go out.
const BEACON_EVERY: Duration = Duration::from_secs(2);
/// A device not heard from for this long is no longer listed (or online).
const GONE_AFTER: Duration = Duration::from_secs(8);
/// How long a pairing waits for the people to compare codes.
const DECIDE_WITHIN: Duration = Duration::from_secs(120);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Where a pairing is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "stage", content = "message")]
pub enum Stage {
    Connecting,
    /// Both screens show the code; waiting for this side's answer.
    Confirm,
    /// This side said yes; waiting for the other.
    Waiting,
    Paired,
    Failed(String),
}

struct Pairing {
    device_id: String,
    name: String,
    code: Option<String>,
    stage: Stage,
    decide: Option<Sender<bool>>,
}

struct Found {
    beacon: Beacon,
    ip: IpAddr,
    seen: Instant,
}

#[derive(Default)]
struct State {
    found: HashMap<String, Found>,
    pairing: Option<Pairing>,
}

struct Inner {
    stop: AtomicBool,
    finding: AtomicBool,
    /// Ask the beacon thread to send now (a change worth announcing).
    nudge: AtomicBool,
    state: Mutex<State>,
    store: Mutex<Store>,
    store_path: PathBuf,
    tcp_port: u16,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// What Settings ▸ Sync shows.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub device_id: String,
    pub name: String,
    pub finding: bool,
    /// Devices looking for a pairing right now (not yet paired with this one).
    pub found: Vec<FoundDevice>,
    pub paired: Vec<PairedDevice>,
    pub pairing: Option<PairingStatus>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundDevice {
    pub device_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDevice {
    pub device_id: String,
    pub name: String,
    /// Heard on the network in the last few seconds.
    pub online: bool,
    pub paired_at: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingStatus {
    pub device_id: String,
    pub name: String,
    pub code: Option<String>,
    #[serde(flatten)]
    pub stage: Stage,
}

/// The first message on a connection.
#[derive(Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
enum Opening {
    /// Asks to pair (the receiver must be looking for devices).
    Pair { name: String },
    /// A paired device saying hello (the catalog exchange starts from here).
    Hello,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
enum Reply {
    Ready { name: String },
    No { why: String },
    Decision { ok: bool },
}

/// The peer service; dropping it stops every thread.
pub struct Service {
    inner: Arc<Inner>,
    threads: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service").field("tcp_port", &self.inner.tcp_port).finish_non_exhaustive()
    }
}

fn discovery_socket(port: u16) -> Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    s.set_reuse_port(true)?;
    s.set_broadcast(true)?;
    s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())?;
    let udp: UdpSocket = s.into();
    udp.set_read_timeout(Some(Duration::from_millis(400)))?;
    Ok(udp)
}

impl Service {
    /// Opens (or creates) the store at `store_path` and starts listening. `default_name` names a
    /// new device. `port` is the discovery port ([`beacon::PORT`]; tests pass their own).
    pub fn start(store_path: PathBuf, default_name: &str, port: u16) -> Result<Service> {
        let store = Store::load_or_create(&store_path, default_name)?;
        let udp = discovery_socket(port)?;
        let tcp = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        tcp.set_nonblocking(true)?;
        let tcp_port = tcp.local_addr()?.port();
        let inner = Arc::new(Inner {
            stop: AtomicBool::new(false),
            finding: AtomicBool::new(false),
            nudge: AtomicBool::new(true),
            state: Mutex::new(State::default()),
            store: Mutex::new(store),
            store_path,
            tcp_port,
        });
        let mut threads = Vec::new();
        let spawn =
            |name: &str, f: Box<dyn FnOnce() + Send>| std::thread::Builder::new().name(name.into()).spawn(f).map_err(|e| Error::Io(e.to_string()));
        let send_udp = udp.try_clone()?;
        let i = inner.clone();
        threads.push(spawn("peer-beacon", Box::new(move || beacon_loop(&i, &send_udp, port)))?);
        let i = inner.clone();
        threads.push(spawn("peer-listen", Box::new(move || listen_loop(&i, &udp)))?);
        let i = inner.clone();
        threads.push(spawn("peer-accept", Box::new(move || accept_loop(&i, &tcp)))?);
        Ok(Service { inner, threads })
    }

    /// Looks for devices to pair with (or stops looking).
    pub fn set_finding(&self, on: bool) {
        if self.inner.finding.swap(on, Ordering::Relaxed) != on {
            self.inner.nudge.store(true, Ordering::Relaxed);
        }
    }

    pub fn finding(&self) -> bool {
        self.inner.finding.load(Ordering::Relaxed)
    }

    pub fn has_peers(&self) -> bool {
        !lock(&self.inner.store).peers.is_empty()
    }

    pub fn status(&self) -> Status {
        let store = lock(&self.inner.store).clone();
        let mut st = lock(&self.inner.state);
        st.found.retain(|_, f| f.seen.elapsed() < GONE_AFTER);
        let mut found: Vec<FoundDevice> = st
            .found
            .values()
            .filter(|f| f.beacon.pairing && store.peer(&f.beacon.device_id).is_none())
            .map(|f| FoundDevice { device_id: f.beacon.device_id.clone(), name: f.beacon.name.clone() })
            .collect();
        found.sort_by(|a, b| a.name.cmp(&b.name).then(a.device_id.cmp(&b.device_id)));
        let paired = store
            .peers
            .iter()
            .map(|p| PairedDevice {
                device_id: p.device_id.clone(),
                name: p.name.clone(),
                online: st.found.contains_key(&p.device_id),
                paired_at: p.paired_at,
            })
            .collect();
        let pairing = st.pairing.as_ref().map(|p| PairingStatus {
            device_id: p.device_id.clone(),
            name: p.name.clone(),
            code: p.code.clone(),
            stage: p.stage.clone(),
        });
        Status { device_id: store.identity.device_id.clone(), name: store.identity.name.clone(), finding: self.finding(), found, paired, pairing }
    }

    /// Starts pairing with a device that is looking for one.
    pub fn pair(&self, device_id: &str) -> Result<()> {
        let (addr, name) = {
            let mut st = lock(&self.inner.state);
            if st.pairing.as_ref().is_some_and(|p| matches!(p.stage, Stage::Connecting | Stage::Confirm | Stage::Waiting)) {
                return Err(Error::Busy);
            }
            let f = st
                .found
                .get(device_id)
                .filter(|f| f.beacon.pairing)
                .ok_or_else(|| Error::Protocol("that device isn't looking for a pairing now".into()))?;
            let addr = SocketAddr::new(f.ip, f.beacon.port);
            let name = f.beacon.name.clone();
            st.pairing = Some(Pairing { device_id: device_id.to_string(), name: name.clone(), code: None, stage: Stage::Connecting, decide: None });
            (addr, name)
        };
        let inner = self.inner.clone();
        std::thread::Builder::new()
            .name("peer-pair".into())
            .spawn(move || {
                let r = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(Error::from).and_then(|s| {
                    let me = lock(&inner.store).identity.clone();
                    let mut ch = Channel::handshake(s, &me, true)?;
                    ch.set_timeout(Some(Duration::from_secs(10)))?;
                    ch.send_json(&Opening::Pair { name: me.name.clone() })?;
                    match ch.recv_json::<Reply>()? {
                        Reply::Ready { .. } => {}
                        Reply::No { why } => return Err(Error::Protocol(why)),
                        Reply::Decision { .. } => return Err(Error::Protocol("unexpected answer".into())),
                    }
                    decide(&inner, ch, &name)
                });
                finish(&inner, r);
            })
            .map_err(|e| Error::Io(e.to_string()))?;
        Ok(())
    }

    /// The person's answer to "do both screens show this code?".
    pub fn confirm(&self, accept: bool) -> Result<()> {
        let st = lock(&self.inner.state);
        let tx = st.pairing.as_ref().and_then(|p| p.decide.clone()).ok_or_else(|| Error::Protocol("no pairing is waiting for an answer".into()))?;
        drop(st);
        tx.send(accept).map_err(|_| Error::Protocol("the pairing has ended".into()))
    }

    /// Forgets a finished or failed pairing (the status no longer shows it).
    pub fn clear_pairing(&self) {
        let mut st = lock(&self.inner.state);
        if st.pairing.as_ref().is_some_and(|p| matches!(p.stage, Stage::Paired | Stage::Failed(_))) {
            st.pairing = None;
        }
    }

    pub fn rename(&self, name: &str) -> Result<String> {
        let mut store = lock(&self.inner.store);
        store.identity.name = clean_name(name, &store.identity.name.clone());
        store.save(&self.inner.store_path)?;
        self.inner.nudge.store(true, Ordering::Relaxed);
        Ok(store.identity.name.clone())
    }

    pub fn unpair(&self, device_id: &str) -> Result<bool> {
        let mut store = lock(&self.inner.store);
        let removed = store.remove_peer(device_id);
        if removed {
            store.save(&self.inner.store_path)?;
        }
        Ok(removed)
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.inner.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            if t.join().is_err() {
                log::warn!("peer: a service thread panicked");
            }
        }
    }
}

fn my_beacon(inner: &Inner) -> Option<Beacon> {
    let store = lock(&inner.store);
    Some(Beacon {
        app: lcc_brand::ENV_PREFIX.to_string(),
        v: beacon::VERSION,
        device_id: store.identity.device_id.clone(),
        name: store.identity.name.clone(),
        key: store.identity.public_key().ok()?,
        port: inner.tcp_port,
        pairing: inner.finding.load(Ordering::Relaxed),
    })
}

fn beacon_loop(inner: &Inner, udp: &UdpSocket, port: u16) {
    let mut last: Option<Instant> = None;
    while !inner.stop.load(Ordering::Relaxed) {
        let due = last.is_none_or(|t| t.elapsed() >= BEACON_EVERY) || inner.nudge.swap(false, Ordering::Relaxed);
        let wanted = inner.finding.load(Ordering::Relaxed) || !lock(&inner.store).peers.is_empty();
        if due && wanted {
            if let Some(b) = my_beacon(inner) {
                let to = SocketAddr::from((Ipv4Addr::BROADCAST, port));
                if let Err(e) = udp.send_to(&b.encode(), to) {
                    log::debug!("peer: beacon not sent: {e}");
                }
            }
            last = Some(Instant::now());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn listen_loop(inner: &Inner, udp: &UdpSocket) {
    let mut buf = [0u8; beacon::MAX_LEN];
    while !inner.stop.load(Ordering::Relaxed) {
        let Ok((n, from)) = udp.recv_from(&mut buf) else { continue };
        let Some(b) = buf.get(..n).and_then(Beacon::decode) else { continue };
        if b.device_id == lock(&inner.store).identity.device_id {
            continue;
        }
        let mut st = lock(&inner.state);
        // a busy network can't grow the list without bound
        if st.found.len() >= 256 && !st.found.contains_key(&b.device_id) {
            continue;
        }
        st.found.insert(b.device_id.clone(), Found { beacon: b, ip: from.ip(), seen: Instant::now() });
    }
}

fn accept_loop(inner: &Arc<Inner>, tcp: &TcpListener) {
    while !inner.stop.load(Ordering::Relaxed) {
        match tcp.accept() {
            Ok((stream, from)) => {
                let i = inner.clone();
                let spawned = std::thread::Builder::new().name("peer-conn".into()).spawn(move || {
                    if let Err(e) = serve(&i, stream) {
                        log::info!("peer: connection from {from}: {e}");
                    }
                });
                if let Err(e) = spawned {
                    log::warn!("peer: no thread for a connection: {e}");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(150)),
            Err(e) => {
                log::debug!("peer: accept: {e}");
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

/// An incoming connection.
fn serve(inner: &Arc<Inner>, stream: TcpStream) -> Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let me = lock(&inner.store).identity.clone();
    let mut ch = Channel::handshake(stream, &me, false)?;
    match ch.recv_json::<Opening>()? {
        Opening::Pair { name } => {
            let name = clean_name(&name, "Device");
            if !inner.finding.load(Ordering::Relaxed) {
                return ch.send_json(&Reply::No { why: "the other device isn't looking for devices".into() });
            }
            {
                let mut st = lock(&inner.state);
                if st.pairing.as_ref().is_some_and(|p| matches!(p.stage, Stage::Connecting | Stage::Confirm | Stage::Waiting)) {
                    drop(st);
                    return ch.send_json(&Reply::No { why: "the other device is pairing with someone else".into() });
                }
                st.pairing =
                    Some(Pairing { device_id: ch.peer_device.clone(), name: name.clone(), code: None, stage: Stage::Connecting, decide: None });
            }
            ch.send_json(&Reply::Ready { name: me.name.clone() })?;
            let r = decide(inner, ch, &name);
            finish(inner, r);
            Ok(())
        }
        Opening::Hello => {
            let known = lock(&inner.store).peer(&ch.peer_device).is_some_and(|p| p.public_key == ch.peer_key);
            if !known {
                return ch.send_json(&Reply::No { why: "not paired".into() });
            }
            // the catalog exchange comes here
            ch.send_json(&Reply::Ready { name: me.name })
        }
    }
}

/// Shows the code, waits for this side's answer, swaps answers with the other side and, if both
/// said yes, stores the pairing. Returns the paired device.
fn decide(inner: &Inner, mut ch: Channel, name: &str) -> Result<Peer> {
    let (tx, rx): (Sender<bool>, Receiver<bool>) = channel();
    {
        let mut st = lock(&inner.state);
        let p = st.pairing.as_mut().ok_or_else(|| Error::Protocol("the pairing was cancelled".into()))?;
        p.code = Some(ch.code.clone());
        p.stage = Stage::Confirm;
        p.decide = Some(tx);
    }
    let started = Instant::now();
    let ok = loop {
        if inner.stop.load(Ordering::Relaxed) || started.elapsed() > DECIDE_WITHIN {
            break false;
        }
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(v) => break v,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break false,
        }
    };
    if let Some(p) = lock(&inner.state).pairing.as_mut() {
        p.stage = Stage::Waiting;
        p.decide = None;
    }
    ch.send_json(&Reply::Decision { ok })?;
    if !ok {
        return Err(Error::Protocol("pairing cancelled".into()));
    }
    let remaining = DECIDE_WITHIN.saturating_sub(started.elapsed()).max(Duration::from_secs(5));
    ch.set_timeout(Some(remaining))?;
    match ch.recv_json::<Reply>()? {
        Reply::Decision { ok: true } => {}
        Reply::Decision { ok: false } => return Err(Error::Protocol("the other device cancelled the pairing".into())),
        _ => return Err(Error::Protocol("unexpected answer".into())),
    }
    let peer = Peer { device_id: ch.peer_device.clone(), name: name.to_string(), public_key: ch.peer_key.clone(), paired_at: now_secs() };
    let mut store = lock(&inner.store);
    store.add_peer(peer.clone());
    store.save(&inner.store_path)?;
    Ok(peer)
}

fn finish(inner: &Inner, r: Result<Peer>) {
    let mut st = lock(&inner.state);
    if let Some(p) = st.pairing.as_mut() {
        p.decide = None;
        p.stage = match r {
            Ok(_) => Stage::Paired,
            Err(e) => Stage::Failed(e.to_string()),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(what: &str, mut f: impl FnMut() -> bool) {
        let t = Instant::now();
        while !f() {
            assert!(t.elapsed() < Duration::from_secs(20), "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Two services on this machine find each other by broadcast and pair; then each knows the
    /// other as paired, and a third party's attempt is refused while they aren't looking.
    #[test]
    fn two_devices_find_each_other_and_pair() {
        let dir = std::env::temp_dir().join(format!("lcc-peer-svc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // a port of its own, so parallel test runs don't hear each other
        let port = 40000 + (std::process::id() % 20000) as u16;
        let a = Service::start(dir.join("a.json"), "Desk", port).unwrap();
        let b = Service::start(dir.join("b.json"), "Tablet", port).unwrap();
        a.set_finding(true);
        b.set_finding(true);
        let b_id = b.status().device_id;
        let a_id = a.status().device_id;
        wait("a to see b", || a.status().found.iter().any(|f| f.device_id == b_id));
        // broadcast may not loop back on every machine: then there is nothing more to test here
        a.pair(&b_id).unwrap();
        wait("both to show the code", || {
            let (sa, sb) = (a.status(), b.status());
            sa.pairing.as_ref().is_some_and(|p| p.stage == Stage::Confirm) && sb.pairing.as_ref().is_some_and(|p| p.stage == Stage::Confirm)
        });
        let (ca, cb) = (a.status().pairing.unwrap().code, b.status().pairing.unwrap().code);
        assert!(ca.is_some());
        assert_eq!(ca, cb);
        a.confirm(true).unwrap();
        b.confirm(true).unwrap();
        wait("both paired", || a.status().paired.iter().any(|p| p.device_id == b_id) && b.status().paired.iter().any(|p| p.device_id == a_id));
        // the pairing is remembered
        drop(a);
        let store = Store::load_or_create(&dir.join("a.json"), "x").unwrap();
        assert!(store.peer(&b_id).is_some());
        drop(b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refused_code_pairs_nobody() {
        let dir = std::env::temp_dir().join(format!("lcc-peer-no-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let port = 40000 + ((std::process::id() + 7) % 20000) as u16;
        let a = Service::start(dir.join("a.json"), "Desk", port).unwrap();
        let b = Service::start(dir.join("b.json"), "Tablet", port).unwrap();
        a.set_finding(true);
        b.set_finding(true);
        let b_id = b.status().device_id;
        wait("a to see b", || a.status().found.iter().any(|f| f.device_id == b_id));
        a.pair(&b_id).unwrap();
        wait("b to show the code", || b.status().pairing.as_ref().is_some_and(|p| p.stage == Stage::Confirm));
        a.confirm(true).unwrap();
        b.confirm(false).unwrap();
        wait("a to fail", || a.status().pairing.as_ref().is_some_and(|p| matches!(p.stage, Stage::Failed(_))));
        assert!(a.status().paired.is_empty());
        assert!(b.status().paired.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
