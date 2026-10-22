//! Headless tests of the touch layout (`crate::touch`): the photo alone until a tap, a sideways
//! drag on the photo adjusts the chosen parameter in one undo step, Cancel puts the tool's values
//! back, the side panel follows the screen's shape, and bad commands are errors.

use std::time::Duration;

use serde_json::{Value, json};

use crate::headless::Headless;
use crate::touch::Form;
use crate::{LccApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

fn ask(h: &mut Headless, method: &str, params: Value) -> Value {
    let r = h.request(method, params.clone(), T);
    h.settle(SETTLE);
    r
}

fn exec(h: &mut Headless, command: &str, params: Value) -> Value {
    ask(h, "engine.execute", json!({"command": command, "params": params}))
}

/// The demo library in Develop on its first photo, touch layout on.
fn develop(size: [f32; 2]) -> Headless {
    let app = LccApp::new(lcc_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, size, 1.0);
    let id = h.app.session.visible_cloned()[0].0;
    assert_eq!(exec(&mut h, "view.touchLayout", json!({"on": true}))["ok"], true);
    assert_eq!(exec(&mut h, "library.select", json!({"ids": [id]}))["ok"], true);
    assert_eq!(exec(&mut h, "module.develop", json!({}))["ok"], true);
    assert_eq!(exec(&mut h, "view.detail", json!({}))["ok"], true);
    exec(&mut h, "touch.chrome", json!({"on": false}));
    h
}

fn has(h: &Headless, id: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w == id)
}

fn value(h: &Headless, control: &str) -> f64 {
    let id = h.app.session.selection.active.unwrap();
    lcc_develop::controls::get(&h.app.session.develop_of(id).unwrap_or_default(), control).unwrap()
}

#[test]
fn the_photo_is_alone_until_a_tap_shows_the_tools() {
    let mut h = develop([1280.0, 800.0]);
    assert!(has(&h, "touch:canvas"));
    assert!(!has(&h, "touch:tool:light"), "no tool strip before a tap");
    assert!(!has(&h, "touch:back"), "no top bar before a tap");
    assert_eq!(ask(&mut h, "ui.clickWidget", json!({"id": "touch:canvas"}))["ok"], true);
    assert!(h.app.ui.touch.chrome, "a tap shows the chrome");
    assert!(has(&h, "touch:tool:light") && has(&h, "touch:back"));
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:canvas", "fx": 0.3, "fy": 0.4}));
    assert!(!h.app.ui.touch.chrome, "a second tap hides it");
}

#[test]
fn a_sideways_drag_adjusts_the_parameter_in_one_undo_step_and_cancel_restores() {
    let mut h = develop([1280.0, 800.0]);
    let before = value(&h, "light.exposure");
    assert_eq!(exec(&mut h, "touch.tool", json!({"tool": "light", "control": "light.exposure"}))["ok"], true);
    assert!(has(&h, "touch:param:light.exposure"), "the tool's parameters show");
    let undo = h.app.session.undo.len();
    let r = ask(&mut h, "ui.dragWidget", json!({"id": "touch:canvas", "fx": 0.3, "dx": 200.0, "steps": 12}));
    assert_eq!(r["ok"], true, "{r}");
    let after = value(&h, "light.exposure");
    assert!(after > before, "dragging right raises exposure ({before} → {after})");
    assert_eq!(h.app.session.undo.len(), undo + 1, "one drag, one undo step");
    // an up/down drag picks another parameter and leaves the values alone
    ask(&mut h, "ui.dragWidget", json!({"id": "touch:canvas", "fx": 0.5, "fy": 0.3, "dy": 70.0, "steps": 8}));
    assert_eq!(h.app.ui.touch.control, "light.contrast");
    assert_eq!(value(&h, "light.exposure"), after);
    // Cancel: back to the values the tool opened with
    assert_eq!(ask(&mut h, "ui.clickWidget", json!({"id": "touch:cancel"}))["ok"], true);
    assert!(h.app.ui.touch.tool.is_none());
    assert_eq!(value(&h, "light.exposure"), before);
}

#[test]
fn done_keeps_the_values() {
    let mut h = develop([1280.0, 800.0]);
    exec(&mut h, "touch.tool", json!({"tool": "effects", "control": "effects.clarity"}));
    ask(&mut h, "ui.dragWidget", json!({"id": "touch:canvas", "fx": 0.5, "dx": -150.0, "steps": 10}));
    let v = value(&h, "effects.clarity");
    assert!(v < 0.0, "dragging left lowers clarity ({v})");
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:done"}));
    assert!(h.app.ui.touch.tool.is_none());
    assert_eq!(value(&h, "effects.clarity"), v);
}

#[test]
fn the_panel_is_beside_the_photo_in_landscape_and_under_it_in_portrait() {
    assert!(Form::of(egui::vec2(1280.0, 800.0)).side_panel());
    assert!(!Form::of(egui::vec2(800.0, 1280.0)).side_panel());
    assert!(!Form::of(egui::vec2(400.0, 860.0)).side_panel());
    let mut h = develop([800.0, 1280.0]);
    exec(&mut h, "touch.panel", json!({"on": true}));
    assert!(has(&h, "touch:sheetGrab"), "portrait: a bottom sheet");
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:sheetGrab"}));
    assert!(!h.app.ui.touch.panel, "a tap on the grabber closes it");
}

#[test]
fn bad_touch_commands_are_errors_not_panics() {
    let mut h = develop([400.0, 860.0]);
    for params in [json!({}), json!({"tool": 3}), json!({"tool": "nope"}), json!({"tool": "light", "control": "effects.clarity"})] {
        let r = exec(&mut h, "touch.tool", params.clone());
        assert_eq!(r["ok"], false, "touch.tool {params}: {r}");
    }
    assert_eq!(exec(&mut h, "touch.done", json!({"cancel": "yes"}))["ok"], true, "no tool open: nothing to do");
    // the layout turned off leaves the desktop's edges
    exec(&mut h, "view.touchLayout", json!({"on": false}));
    assert!(!has(&h, "touch:canvas"));
}

#[test]
fn a_tap_in_the_library_grid_opens_the_photo() {
    let app = LccApp::new(lcc_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1280.0, 800.0], 1.0);
    exec(&mut h, "view.touchLayout", json!({"on": true}));
    exec(&mut h, "view.photoGrid", json!({}));
    assert!(has(&h, "touch:import"), "Library's bar");
    let cell = h.app.widgets.iter().find(|(w, _)| w.starts_with("thumb:")).map(|(w, _)| w.clone());
    let Some(cell) = cell else { panic!("no grid cell on screen: {:?}", h.app.widgets.iter().map(|w| &w.0).take(40).collect::<Vec<_>>()) };
    ask(&mut h, "ui.clickWidget", json!({"id": cell}));
    assert_eq!(h.app.ui.module, crate::module::ModuleId::Develop);
    assert_eq!(h.app.ui.view, crate::state::ViewMode::Detail);
}
