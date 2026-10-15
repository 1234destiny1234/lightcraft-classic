//! The active photo's pixel-filter plug-in stack (`develop.pluginFilter.*`): add, remove, change,
//! reorder and list entries. Each edit is one History step; Auto Sync carries it to the selection
//! like any develop edit, and Copy / Paste / Sync Settings and presets carry the stack as the
//! "Plug-in Filters" group. Rendering the stack is the pipeline's business, not this module's.

use lcc_develop::{DevelopSettings, FilterStage, MAX_PLUGIN_FILTERS, PluginFilter};
use serde_json::{Map, Value, json};

use super::{CommandSpec, bad, cmd, has_active};
use crate::catalog::PhotoId;
use crate::{Result, Session};

/// Longest plug-in id, module hash, display name or parameter key accepted.
const MAX_TEXT: usize = 256;
/// Most parameters one entry holds.
const MAX_PARAMS: usize = 64;

fn text(p: &Value, key: &str, c: &str) -> Result<String> {
    let t = p.get(key).and_then(Value::as_str).ok_or_else(|| bad(c, format!("missing string `{key}`")))?;
    if t.is_empty() || t.len() > MAX_TEXT {
        return Err(bad(c, format!("`{key}` must be 1..{MAX_TEXT} bytes")));
    }
    Ok(t.to_string())
}

fn uid(p: &Value, c: &str) -> Result<u32> {
    let v = p.get("uid").and_then(Value::as_u64).ok_or_else(|| bad(c, "missing integer `uid`"))?;
    u32::try_from(v).map_err(|_| bad(c, "no such plug-in filter"))
}

fn stage(p: &Value, c: &str) -> Result<Option<FilterStage>> {
    match p.get("stage") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|_| bad(c, "`stage` must be \"source\" or \"display\"")),
    }
}

/// `params` as a checked object (numbers finite, values scalar), or `None` when absent.
fn params(p: &Value, c: &str) -> Result<Option<Map<String, Value>>> {
    let m = match p.get("params") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Object(m)) => m,
        Some(_) => return Err(bad(c, "`params` must be an object")),
    };
    if m.len() > MAX_PARAMS {
        return Err(bad(c, format!("at most {MAX_PARAMS} parameters")));
    }
    for (k, v) in m {
        if k.is_empty() || k.len() > MAX_TEXT {
            return Err(bad(c, "parameter names must be 1..256 bytes"));
        }
        let ok = match v {
            Value::Number(n) => n.as_f64().is_some_and(f64::is_finite),
            Value::Bool(_) => true,
            Value::String(s) => s.len() <= MAX_TEXT,
            _ => false,
        };
        if !ok {
            return Err(bad(c, format!("parameter `{k}` must be a number, boolean or short string")));
        }
    }
    Ok(Some(m.clone()))
}

fn active(s: &Session, c: &str) -> Result<(PhotoId, DevelopSettings)> {
    let id = s.active().ok_or_else(|| bad(c, "no active photo"))?;
    let d = s.develop_of(id).ok_or_else(|| bad(c, "no active photo"))?;
    Ok((id, (*d).clone()))
}

fn position(d: &DevelopSettings, uid: u32, c: &str) -> Result<usize> {
    d.plugin_filters.iter().position(|f| f.uid == uid).ok_or_else(|| bad(c, "no such plug-in filter"))
}

fn label(name: &str) -> String {
    format!("Plug-in: {name}")
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let c = "develop.pluginFilter.add";
    let (plugin, module_hash, name) = (text(p, "plugin", c)?, text(p, "module_hash", c)?, text(p, "name", c)?);
    let (params, stage) = (params(p, c)?.unwrap_or_default(), stage(p, c)?.unwrap_or_default());
    let (id, mut d) = active(s, c)?;
    if d.plugin_filters.len() >= MAX_PLUGIN_FILTERS {
        return Err(bad(c, format!("at most {MAX_PLUGIN_FILTERS} plug-in filters")));
    }
    let uid = match d.plugin_filters.iter().map(|f| f.uid).max() {
        Some(m) => m.checked_add(1).ok_or_else(|| bad(c, "no free uid"))?,
        None => 1,
    };
    let l = label(&name);
    d.plugin_filters.push(PluginFilter { uid, plugin, module_hash, name, params, enabled: true, stage, analysis: None });
    s.set_develop(id, d, &l)?;
    Ok(json!({ "uid": uid }))
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let c = "develop.pluginFilter.remove";
    let u = uid(p, c)?;
    let (id, mut d) = active(s, c)?;
    let i = position(&d, u, c)?;
    let f = d.plugin_filters.remove(i);
    s.set_develop(id, d, &format!("Remove Plug-in: {}", f.name))?;
    Ok(Value::Null)
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let c = "develop.pluginFilter.set";
    let u = uid(p, c)?;
    let patch = params(p, c)?;
    let st = stage(p, c)?;
    let enabled = match p.get("enabled") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| bad(c, "`enabled` must be a boolean"))?),
    };
    let (id, mut d) = active(s, c)?;
    let i = position(&d, u, c)?;
    let f = d.plugin_filters.get_mut(i).ok_or_else(|| bad(c, "no such plug-in filter"))?;
    let mut l = label(&f.name);
    if let Some(m) = patch {
        if let [(k, _)] = m.iter().collect::<Vec<_>>().as_slice() {
            l = format!("{l} › {k}");
        }
        f.params.extend(m);
        if f.params.len() > MAX_PARAMS {
            return Err(bad(c, format!("at most {MAX_PARAMS} parameters")));
        }
    }
    if let Some(e) = enabled {
        f.enabled = e;
    }
    if let Some(st) = st {
        f.stage = st;
    }
    s.set_develop(id, d, &l)?;
    Ok(Value::Null)
}

fn move_to(s: &mut Session, p: &Value) -> Result<Value> {
    let c = "develop.pluginFilter.move";
    let u = uid(p, c)?;
    let to = p.get("index").and_then(Value::as_u64).ok_or_else(|| bad(c, "missing integer `index`"))?;
    let (id, mut d) = active(s, c)?;
    let i = position(&d, u, c)?;
    let to = usize::try_from(to).ok().filter(|t| *t < d.plugin_filters.len()).ok_or_else(|| bad(c, "`index` out of range"))?;
    let f = d.plugin_filters.remove(i);
    let l = format!("Move Plug-in: {}", f.name);
    d.plugin_filters.insert(to, f);
    s.set_develop(id, d, &l)?;
    Ok(Value::Null)
}

/// Largest analysis JSON accepted (serialized bytes).
const MAX_ANALYSIS: usize = 64 * 1024;

fn set_analysis(s: &mut Session, p: &Value) -> Result<Value> {
    let c = "develop.pluginFilter.setAnalysis";
    let u = uid(p, c)?;
    let a = match p.get("analysis") {
        None => return Err(bad(c, "missing `analysis`")),
        Some(Value::Null) => None,
        Some(v) => {
            if serde_json::to_vec(v).map_or(true, |b| b.len() > MAX_ANALYSIS) {
                return Err(bad(c, format!("`analysis` must be at most {MAX_ANALYSIS} bytes of JSON")));
            }
            Some(v.clone())
        }
    };
    let (id, mut d) = active(s, c)?;
    let i = position(&d, u, c)?;
    let f = d.plugin_filters.get_mut(i).ok_or_else(|| bad(c, "no such plug-in filter"))?;
    f.analysis = a;
    let l = format!("{} › Measure", label(&f.name));
    s.set_develop(id, d, &l)?;
    Ok(Value::Null)
}

fn list(s: &mut Session, _p: &Value) -> Result<Value> {
    let (_, d) = active(s, "develop.pluginFilter.list")?;
    Ok(json!({
        "enabled": d.section_enabled(lcc_develop::PLUGIN_FILTERS_SECTION),
        "filters": serde_json::to_value(&d.plugin_filters).unwrap_or(Value::Null),
    }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "develop.pluginFilter.add",
            "Add Plug-in Filter",
            [],
            None,
            "{plugin, module_hash, name, params?: {key: number|bool|string}, stage?: source|display} — appends a pixel-filter plug-in to the active photo; returns {uid}",
            has_active,
            add
        ),
        cmd!("develop.pluginFilter.remove", "Remove Plug-in Filter", [], None, "{uid}", has_active, remove),
        cmd!(
            "develop.pluginFilter.set",
            "Change Plug-in Filter",
            [],
            None,
            "{uid, params?: {key: value} (merged), enabled?: bool, stage?: source|display}",
            has_active,
            set
        ),
        cmd!("develop.pluginFilter.move", "Move Plug-in Filter", [], None, "{uid, index} — new position in the stack", has_active, move_to),
        cmd!(
            "develop.pluginFilter.setAnalysis",
            "Measure Plug-in Filter",
            [],
            None,
            "{uid, analysis: JSON (null clears)} — stores the filter's whole-image measurement",
            has_active,
            set_analysis
        ),
        cmd!(query "develop.pluginFilter.list", "List Plug-in Filters", [], None, "{} — returns {enabled, filters: [...]}", has_active, list),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(s: &Session) -> Vec<PluginFilter> {
        s.develop_of(s.active().unwrap()).unwrap().plugin_filters.clone()
    }

    fn add_one(s: &mut Session, name: &str) -> u64 {
        let r =
            s.execute("develop.pluginFilter.add", &json!({"plugin": "t.f", "module_hash": "ab", "name": name, "params": {"amount": 1.0}})).unwrap();
        r["uid"].as_u64().unwrap()
    }

    #[test]
    fn edits_are_history_steps_and_undo_restores() {
        let mut s = Session::with_demo();
        let u0 = s.undo.len();
        let a = add_one(&mut s, "Neg");
        let b = add_one(&mut s, "Glow");
        assert_ne!(a, b);
        assert_eq!(s.undo.len(), u0 + 2);
        s.execute("develop.pluginFilter.set", &json!({"uid": a, "params": {"gamma": 2.2}, "stage": "source"})).unwrap();
        let f = &stack(&s)[0];
        assert_eq!((f.params.len(), f.stage), (2, FilterStage::Source));
        s.execute("develop.pluginFilter.setAnalysis", &json!({"uid": a, "analysis": {"base": [0.8, 0.5, 0.3]}})).unwrap();
        s.execute("develop.pluginFilter.set", &json!({"uid": a, "enabled": false})).unwrap();
        assert!(stack(&s)[0].analysis.is_some(), "set keeps the analysis");
        s.execute("develop.pluginFilter.move", &json!({"uid": a, "index": 1})).unwrap();
        assert_eq!(stack(&s)[1].uid as u64, a);
        s.execute("develop.pluginFilter.remove", &json!({"uid": b})).unwrap();
        assert_eq!(stack(&s).len(), 1);
        assert_eq!(s.undo.len(), u0 + 7);
        let listed = s.execute("develop.pluginFilter.list", &json!({})).unwrap();
        assert_eq!(listed["filters"].as_array().unwrap().len(), 1);
        for _ in 0..7 {
            s.execute("edit.undo", &json!({})).unwrap();
        }
        assert!(stack(&s).is_empty());
    }

    #[test]
    fn hostile_args_error_without_panic() {
        let mut s = Session::with_demo();
        let a = add_one(&mut s, "Neg");
        let bad_calls = [
            ("develop.pluginFilter.add", json!({})),
            ("develop.pluginFilter.add", json!({"plugin": 1, "module_hash": "x", "name": "n"})),
            ("develop.pluginFilter.add", json!({"plugin": "p", "module_hash": "x", "name": "n", "params": [1]})),
            ("develop.pluginFilter.add", json!({"plugin": "p", "module_hash": "x", "name": "n", "params": {"a": {"b": 1}}})),
            ("develop.pluginFilter.add", json!({"plugin": "p", "module_hash": "x", "name": "n", "stage": "middle"})),
            ("develop.pluginFilter.add", json!({"plugin": "x".repeat(10_000), "module_hash": "x", "name": "n"})),
            ("develop.pluginFilter.remove", json!({"uid": 999})),
            ("develop.pluginFilter.remove", json!({"uid": u64::MAX})),
            ("develop.pluginFilter.remove", json!({"uid": "1"})),
            ("develop.pluginFilter.set", json!({"uid": a, "enabled": "yes"})),
            ("develop.pluginFilter.set", json!({"uid": 12345, "enabled": false})),
            ("develop.pluginFilter.move", json!({"uid": a, "index": u64::MAX})),
            ("develop.pluginFilter.move", json!({"uid": a, "index": -1})),
            ("develop.pluginFilter.move", json!({"uid": a, "index": 1})),
            ("develop.pluginFilter.setAnalysis", json!({"uid": a})),
            ("develop.pluginFilter.setAnalysis", json!({"uid": 77, "analysis": 1})),
            ("develop.pluginFilter.setAnalysis", json!({"uid": a, "analysis": "x".repeat(100_000)})),
        ];
        for (id, p) in bad_calls {
            assert!(s.execute(id, &p).is_err(), "{id} {p}");
        }
        assert_eq!(stack(&s).len(), 1);
        for _ in 0..MAX_PLUGIN_FILTERS - 1 {
            add_one(&mut s, "More");
        }
        assert!(s.execute("develop.pluginFilter.add", &json!({"plugin": "p", "module_hash": "x", "name": "n"})).is_err());
    }
}
