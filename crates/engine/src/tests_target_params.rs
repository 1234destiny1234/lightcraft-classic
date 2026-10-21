//! Malformed index / component / mask-id targets are refused before anything changes; they are never truncated
//! (`0.9` -> 0) or treated as an omitted target (`-1` -> the active object).

use serde_json::{Value, json};

use crate::Session;

fn state(s: &Session) -> Value {
    let id = s.active().unwrap();
    json!({"develop": serde_json::to_value(s.develop_of(id).unwrap()).unwrap(), "mask": s.active_mask, "spot": s.active_spot,
           "versions": s.catalog.photo(id).map(|p| p.versions.len())})
}

fn refused(s: &mut Session, cmd: &str, p: Value) {
    let before = state(s);
    let r = s.execute(cmd, &p);
    assert!(matches!(r, Err(crate::EngineError::BadParams { .. })), "{cmd} {p} must be BadParams, got {r:?}");
    assert_eq!(before, state(s), "{cmd} {p} changed state");
}

#[test]
fn fractional_and_malformed_indices_are_refused() {
    let mut s = Session::with_demo();
    s.execute("version.create", &json!({"name": "A"})).unwrap();
    for bad in [json!(0.9), json!(-1), json!(1e30), json!("0"), json!(0.0000001)] {
        refused(&mut s, "version.delete", json!({"index": bad}));
        refused(&mut s, "history.restore", json!({"index": bad}));
    }
    s.execute("version.delete", &json!({"index": 0.0})).unwrap();
}

#[test]
fn malformed_mask_and_spot_targets_are_refused() {
    let mut s = Session::with_demo();
    s.execute("mask.add", &json!({"kind": "radial"})).unwrap();
    s.execute("spot.add", &json!({"points": [[0.5, 0.5]]})).unwrap();
    refused(&mut s, "mask.update", json!({"component": -1, "shape": {"kind": "brush", "strokes": []}}));
    refused(&mut s, "mask.update", json!({"component": 0.5, "shape": {"kind": "brush", "strokes": []}}));
    refused(&mut s, "mask.component", json!({"component": -1, "action": "delete"}));
    refused(&mut s, "spot.delete", json!({"index": -1}));
    refused(&mut s, "spot.select", json!({"index": -1}));
    refused(&mut s, "spot.select", json!({"index": 0.5}));
    refused(&mut s, "mask.select", json!({"id": -1}));
    refused(&mut s, "mask.applyEffect", json!({"name": "x", "ids": [0, -1]}));
    // valid defaults still work: omitted / null target = the active mask, null selection clears
    s.execute("mask.select", &json!({"id": null})).unwrap();
    assert_eq!(s.active_mask, None);
}
