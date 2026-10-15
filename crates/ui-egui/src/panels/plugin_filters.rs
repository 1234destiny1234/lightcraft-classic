//! The Edit panel's Plug-in Filters section: the active photo's pixel-filter plug-in stack, one row
//! per entry (enable, name, stage, reorder, remove) with controls generated from the plug-in's
//! manifest parameters. Every edit goes through the `develop.pluginFilter.*` commands; a slider drag
//! is wrapped in `develop.beginInteraction` / `endInteraction`, so it is one History step.

use egui::{Color32, RichText};
use lcc_develop::{DevelopSettings, FilterStage, PLUGIN_FILTERS_SECTION, PluginFilter};
use serde_json::{Map, Value, json};

use crate::LccApp;
use crate::i18n::tr;
use crate::widgets::register;

/// An installed filter plug-in as `plugin.list` describes it (missing keys: no controls).
#[derive(Clone, Debug, Default)]
pub(crate) struct FilterPlugin {
    pub id: String,
    pub name: String,
    pub module_hash: String,
    /// The manifest's parameter schema (`{key: {type, …}}`), or `None` when not given.
    pub params: Option<Map<String, Value>>,
    pub analyze: bool,
}

/// Installed filter plug-ins from a `plugin.list` reply; entries without an id or module hash are skipped.
pub(crate) fn filter_plugins(list: &Value) -> Vec<FilterPlugin> {
    let Some(all) = list.get("plugins").and_then(Value::as_array) else { return Vec::new() };
    all.iter()
        .filter(|p| p.get("kind").and_then(Value::as_str) == Some("filter"))
        .filter_map(|p| {
            let id = p.get("id").and_then(Value::as_str)?.to_string();
            let module_hash = p.get("moduleHash").and_then(Value::as_str)?.to_string();
            Some(FilterPlugin {
                name: p.get("name").and_then(Value::as_str).unwrap_or(&id).to_string(),
                id,
                module_hash,
                params: p.get("params").and_then(Value::as_object).cloned(),
                analyze: p.get("analyze").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

/// Defaults from a manifest parameter schema (what a new entry starts with).
fn defaults(schema: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, s) in schema {
        let v = match s.get("type").and_then(Value::as_str) {
            Some("number") | Some("int") | Some("bool") => s.get("default").cloned(),
            Some("choice") => s
                .get("default")
                .filter(|d| d.is_string())
                .cloned()
                .or_else(|| s.get("options").and_then(Value::as_array).and_then(|o| o.first()).cloned()),
            _ => None,
        };
        if let Some(v) = v.filter(|v| !v.is_null()) {
            out.insert(k.clone(), v);
        }
    }
    out
}

/// What a row asks for this frame.
enum Action {
    Run(&'static str, Value),
    Begin(String),
    End,
}

pub(crate) fn show(app: &mut LccApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    super::edit::section(app, ui, d, PLUGIN_FILTERS_SECTION, "Plug-in Filters", body);
}

fn body(app: &mut LccApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    // `plugin.list` is only asked while the section is open
    let list = app.session.execute("plugin.list", &json!({})).unwrap_or(Value::Null);
    let installed = filter_plugins(&list);
    let can_measure = lcc_engine::find_command("develop.pluginFilter.analyze").is_some();
    let mut actions: Vec<Action> = Vec::new();
    let n = d.plugin_filters.len();
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 4, bottom: 4 }).show(ui, |ui| {
        if n == 0 {
            ui.label(RichText::new(tr("No plug-in filters on this photo")).weak());
        }
        for (i, f) in d.plugin_filters.iter().enumerate() {
            let plugin = installed.iter().find(|p| p.id == f.plugin);
            ui.push_id(("pluginFilter", f.uid), |ui| row(ui, f, i, n, plugin, can_measure, &mut actions));
            ui.separator();
        }
        let r = ui.menu_button(tr("Add Filter…"), |ui| {
            ui.set_min_width(180.0);
            if installed.is_empty() {
                ui.label(RichText::new(tr("No filter plug-ins installed")).weak());
            }
            for p in &installed {
                let r = ui.button(&p.name);
                register(ui.ctx(), format!("pluginFilterAdd:{}", p.id), r.rect);
                if r.clicked() {
                    let params = p.params.as_ref().map(defaults).unwrap_or_default();
                    actions.push(Action::Run(
                        "develop.pluginFilter.add",
                        json!({"plugin": p.id, "module_hash": p.module_hash, "name": p.name, "params": params}),
                    ));
                    ui.close();
                }
            }
        });
        register(ui.ctx(), "pluginFilterAddMenu", r.response.rect);
    });
    for a in actions {
        // errors surface through the app's command error path; nothing here can corrupt the stack
        let _ = match a {
            Action::Run(id, p) => app.run(id, p),
            Action::Begin(label) => app.run("develop.beginInteraction", json!({"label": label})),
            Action::End => app.run("develop.endInteraction", json!({})),
        };
    }
}

fn row(ui: &mut egui::Ui, f: &PluginFilter, i: usize, n: usize, plugin: Option<&FilterPlugin>, can_measure: bool, out: &mut Vec<Action>) {
    let uid = f.uid;
    ui.horizontal(|ui| {
        let mut on = f.enabled;
        let r = ui.checkbox(&mut on, "");
        crate::access::label(&r, "Enabled");
        if r.changed() {
            out.push(Action::Run("develop.pluginFilter.set", json!({"uid": uid, "enabled": on})));
        }
        ui.label(RichText::new(&f.name).strong());
        if plugin.is_none() {
            ui.label(RichText::new(format!("⚠ {}", tr("not installed"))).color(Color32::from_rgb(0xe0, 0xa0, 0x30)))
                .on_hover_text(tr("This filter's plug-in is not installed: it is skipped and its settings are kept."));
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("Stage"));
        let name = |s: FilterStage| match s {
            FilterStage::Source => tr("Source"),
            FilterStage::Display => tr("Display"),
        };
        egui::ComboBox::from_id_salt("stage").width(76.0).selected_text(name(f.stage)).show_ui(ui, |ui| {
            for (s, key) in [(FilterStage::Source, "source"), (FilterStage::Display, "display")] {
                if ui.selectable_label(f.stage == s, name(s)).clicked() && f.stage != s {
                    out.push(Action::Run("develop.pluginFilter.set", json!({"uid": uid, "stage": key})));
                }
            }
        });
        if can_measure && plugin.is_some_and(|p| p.analyze) {
            let r = ui.button(tr("Measure"));
            register(ui.ctx(), format!("pluginFilterMeasure:{uid}"), r.rect);
            if r.clicked() {
                out.push(Action::Run("develop.pluginFilter.analyze", json!({"uid": uid})));
            }
        }
        if ui.add_enabled(i > 0, egui::Button::new("▲").small()).on_hover_text(tr("Move Up")).clicked() {
            out.push(Action::Run("develop.pluginFilter.move", json!({"uid": uid, "index": i.saturating_sub(1)})));
        }
        if ui.add_enabled(i + 1 < n, egui::Button::new("▼").small()).on_hover_text(tr("Move Down")).clicked() {
            out.push(Action::Run("develop.pluginFilter.move", json!({"uid": uid, "index": i + 1})));
        }
        if ui.small_button("×").on_hover_text(tr("Remove")).clicked() {
            out.push(Action::Run("develop.pluginFilter.remove", json!({"uid": uid})));
        }
    });
    let Some(schema) = plugin.and_then(|p| p.params.as_ref()) else {
        // no plug-in or no schema: values shown read-only
        for (k, v) in &f.params {
            ui.label(RichText::new(format!("{k}: {v}")).weak());
        }
        return;
    };
    for (k, s) in schema {
        param(ui, uid, &f.name, k, s, f.params.get(k), out);
    }
}

fn param(ui: &mut egui::Ui, uid: u32, name: &str, key: &str, s: &Value, cur: Option<&Value>, out: &mut Vec<Action>) {
    let label = s.get("label").and_then(Value::as_str).unwrap_or(key);
    let set = |v: Value| Action::Run("develop.pluginFilter.set", json!({"uid": uid, "params": {key: v}}));
    match s.get("type").and_then(Value::as_str) {
        Some(t @ ("number" | "int")) => {
            let int = t == "int";
            let (Some(mut lo), Some(mut hi)) = (s.get("min").and_then(Value::as_f64), s.get("max").and_then(Value::as_f64)) else {
                if !int {
                    return;
                }
                // ints default to a wide range, as the manifest does
                return int_param(ui, label, s, cur, set, out, uid, name);
            };
            if !(lo.is_finite() && hi.is_finite()) {
                return;
            }
            if lo > hi {
                std::mem::swap(&mut lo, &mut hi);
            }
            let def = s.get("default").and_then(Value::as_f64).unwrap_or(lo);
            let mut v = cur.and_then(Value::as_f64).unwrap_or(def).clamp(lo, hi);
            let before = v;
            ui.label(label);
            let mut w = egui::Slider::new(&mut v, lo..=hi);
            if int {
                w = w.integer();
            }
            let r = ui.add(w);
            crate::access::label(&r, label);
            slider_actions(&r, before, v, int, set, out, uid, name, key);
            if r.double_clicked() {
                out.push(Action::Run("develop.pluginFilter.set", json!({"uid": uid, "params": {key: def}})));
            }
        }
        Some("bool") => {
            let mut b = cur.and_then(Value::as_bool).or_else(|| s.get("default").and_then(Value::as_bool)).unwrap_or(false);
            let r = ui.checkbox(&mut b, label);
            if r.changed() {
                out.push(set(json!(b)));
            }
        }
        Some("choice") => {
            let Some(opts) = s.get("options").and_then(Value::as_array) else { return };
            let now = cur.and_then(Value::as_str).or_else(|| s.get("default").and_then(Value::as_str)).unwrap_or("");
            ui.horizontal(|ui| {
                ui.label(label);
                egui::ComboBox::from_id_salt(("choice", key)).selected_text(now).show_ui(ui, |ui| {
                    for o in opts.iter().filter_map(Value::as_str) {
                        if ui.selectable_label(o == now, o).clicked() && o != now {
                            out.push(set(json!(o)));
                        }
                    }
                });
            });
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn int_param(
    ui: &mut egui::Ui,
    label: &str,
    s: &Value,
    cur: Option<&Value>,
    set: impl Fn(Value) -> Action,
    out: &mut Vec<Action>,
    uid: u32,
    name: &str,
) {
    let lo = s.get("min").and_then(Value::as_i64).unwrap_or(-1_000_000);
    let hi = s.get("max").and_then(Value::as_i64).unwrap_or(1_000_000).max(lo);
    let mut v = cur.and_then(Value::as_i64).or_else(|| s.get("default").and_then(Value::as_i64)).unwrap_or(0).clamp(lo, hi);
    let before = v;
    ui.horizontal(|ui| {
        ui.label(label);
        let r = ui.add(egui::DragValue::new(&mut v).range(lo..=hi));
        crate::access::label(&r, label);
        slider_actions(&r, before as f64, v as f64, true, set, out, uid, name, label);
    });
}

/// A drag is one History step: begin on the first frame, live `set`s while it moves, end on release.
#[allow(clippy::too_many_arguments)]
fn slider_actions(
    r: &egui::Response,
    before: f64,
    v: f64,
    int: bool,
    set: impl Fn(Value) -> Action,
    out: &mut Vec<Action>,
    _uid: u32,
    name: &str,
    key: &str,
) {
    if r.drag_started() {
        out.push(Action::Begin(format!("Plug-in: {name} › {key}")));
    }
    if r.changed() && v != before && v.is_finite() {
        out.push(set(if int { json!(v.round() as i64) } else { json!(v) }));
    }
    if r.drag_stopped() {
        out.push(Action::End);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_list_parsing_is_defensive() {
        let list = json!({"plugins": [
            {"id": "a.neg", "name": "Negative", "kind": "filter", "moduleHash": "h1", "analyze": true,
             "params": {"gamma": {"type": "number", "min": 1.0, "max": 3.0, "default": 2.2},
                        "mode": {"type": "choice", "options": ["c41", "bw"]},
                        "flip": {"type": "bool"}}},
            {"id": "b.cmd", "name": "Command", "kind": "command"},
            {"id": "c.nohash", "kind": "filter"},
            {"kind": "filter", "moduleHash": "x"},
            "junk",
        ]});
        let f = filter_plugins(&list);
        assert_eq!(f.len(), 1);
        assert!(f[0].analyze);
        let d = defaults(f[0].params.as_ref().unwrap());
        assert_eq!(d.get("gamma"), Some(&json!(2.2)));
        assert_eq!(d.get("mode"), Some(&json!("c41")));
        assert!(d.get("flip").is_none(), "no default given: the plug-in's own applies");
        assert!(filter_plugins(&Value::Null).is_empty());
        assert!(filter_plugins(&json!({"plugins": 3})).is_empty());
    }
}
