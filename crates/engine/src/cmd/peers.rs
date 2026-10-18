//! `sync.*`: pairing with the app on other devices and the paired devices (Settings ▸ Sync;
//! [`crate::peers`]).

use serde_json::Value;

use super::{CommandSpec, always, bad, cmd, str_param};
use crate::peers;

fn bool_req(p: &Value, key: &str, cmd: &str) -> crate::Result<bool> {
    p.get(key).and_then(Value::as_bool).ok_or_else(|| bad(cmd, format!("missing boolean `{key}`")))
}

fn device(p: &Value, cmd: &str) -> crate::Result<String> {
    str_param(p, "device").filter(|d| !d.is_empty()).map(str::to_string).ok_or_else(|| bad(cmd, "missing `device` (a device id from sync.status)"))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "sync.status", "Sync Status", [], None, "{} → {running, available, deviceId, name, finding, found: [{deviceId, name}], paired: [{deviceId, name, online, pairedAt}], pairing: {deviceId, name, code, stage, message?} | null}", always, |s, _p| peers::status(s)),
        cmd!(query "sync.find", "Find Devices", [], None, "{on: bool}: look for devices to pair with (while Settings ▸ Sync is open) → sync.status", always, |s, p| {
            let on = bool_req(p, "on", "sync.find")?;
            peers::find(s, on)
        }),
        cmd!(query "sync.start", "Start Sync", [], None, "{}: start listening for paired devices, if there are any → {running}", always, |s, _p| peers::start(s)),
        cmd!(query "sync.pair", "Pair Device", [], None, "{device}: pair with a device that is looking for one (both screens then show a code) → sync.status", always, |s, p| {
            let d = device(p, "sync.pair")?;
            peers::pair(s, &d)
        }),
        cmd!(query "sync.confirm", "Confirm Pairing", [], None, "{accept: bool}: whether both screens show the same code → sync.status", always, |s, p| {
            let accept = bool_req(p, "accept", "sync.confirm")?;
            peers::confirm(s, accept)
        }),
        cmd!(query "sync.clearPairing", "Dismiss Pairing", [], None, "{}: forget a finished or failed pairing → sync.status", always, |s, _p| peers::clear_pairing(s)),
        cmd!(query "sync.unpair", "Remove Paired Device", [], None, "{device} → sync.status", always, |s, p| {
            let d = device(p, "sync.unpair")?;
            peers::unpair(s, &d)
        }),
        cmd!(query "sync.rename", "Rename This Device", [], None, "{name}: what other devices call this one → sync.status", always, |s, p| {
            let name = str_param(p, "name").filter(|n| !n.trim().is_empty()).ok_or_else(|| bad("sync.rename", "missing `name`"))?;
            peers::rename(s, name)
        }),
    ]
}
