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
    assert!(!has(&h, "touch:tool:basic"), "no tool strip before a tap");
    assert!(!has(&h, "touch:back"), "no top bar before a tap");
    assert_eq!(ask(&mut h, "ui.clickWidget", json!({"id": "touch:canvas"}))["ok"], true);
    assert!(h.app.ui.touch.chrome, "a tap shows the chrome");
    assert!(has(&h, "touch:tool:basic") && has(&h, "touch:back"));
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:canvas", "fx": 0.3, "fy": 0.4}));
    assert!(!h.app.ui.touch.chrome, "a second tap hides it");
}

#[test]
fn a_sideways_drag_adjusts_the_parameter_in_one_undo_step_and_cancel_restores() {
    let mut h = develop([1280.0, 800.0]);
    let before = value(&h, "light.exposure");
    assert_eq!(exec(&mut h, "touch.tool", json!({"tool": "basic", "control": "light.exposure"}))["ok"], true);
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
    exec(&mut h, "touch.tool", json!({"tool": "colour", "control": "effects.clarity"}));
    ask(&mut h, "ui.dragWidget", json!({"id": "touch:canvas", "fx": 0.5, "dx": -150.0, "steps": 10}));
    let v = value(&h, "effects.clarity");
    assert!(v < 0.0, "dragging left lowers clarity ({v})");
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:done"}));
    assert!(h.app.ui.touch.tool.is_none());
    assert_eq!(value(&h, "effects.clarity"), v);
}

/// Basic leads with white balance, Colour holds the Edit panel's Presence, Tone the parametric curve.
#[test]
fn the_tool_groups_hold_white_balance_presence_and_the_parametric_curve() {
    let mut h = develop([1280.0, 800.0]);
    let groups: [(&str, &[&str]); 3] = [
        ("basic", &["wb.temp", "wb.tint", "light.exposure", "light.blacks"]),
        ("colour", &["effects.texture", "effects.clarity", "effects.dehaze", "color.vibrance", "color.saturation"]),
        ("tone", &["curve.highlights", "curve.lights", "curve.darks", "curve.shadows", "curve.splitMid"]),
    ];
    for (tool, controls) in groups {
        for control in controls {
            let r = exec(&mut h, "touch.tool", json!({"tool": tool, "control": control}));
            assert_eq!(r["ok"], true, "{tool} offers {control}: {r}");
        }
    }
    // vignette and grain, once under Effects, are still a tool away
    for control in ["vignette.amount", "grain.amount"] {
        assert_eq!(exec(&mut h, "touch.tool", json!({"tool": "effects", "control": control}))["ok"], true, "{control}");
    }
    exec(&mut h, "touch.tool", json!({"tool": "basic"}));
    assert!(h.app.ui.touch.control.starts_with("wb.temp"), "Basic opens on Temp ({})", h.app.ui.touch.control);
}

/// Temp in adjust mode moves the photo's white balance on the photo's own scale, as the Edit
/// panel's slider does, and Cancel puts it back.
#[test]
fn a_drag_on_temp_warms_the_photo_and_cancel_restores_it() {
    let mut h = develop([1280.0, 800.0]);
    let wb = |h: &Headless| {
        let id = h.app.session.selection.active.unwrap();
        h.app.session.develop_of(id).unwrap_or_default().wb.temp
    };
    let before = wb(&h);
    assert_eq!(exec(&mut h, "touch.tool", json!({"tool": "basic", "control": "wb.temp"}))["ok"], true);
    let control = h.app.ui.touch.control.clone();
    let id = h.app.session.selection.active.unwrap();
    let raw = h.app.session.catalog.photo(id).is_some_and(|p| p.develops_raw() && !p.relative_wb());
    assert_eq!(control, if raw { "wb.temp" } else { "wb.tempRel" }, "Kelvin for raws, relative otherwise");
    assert!(has(&h, &format!("touch:param:{control}")));
    ask(&mut h, "ui.dragWidget", json!({"id": "touch:canvas", "fx": 0.3, "dx": 200.0, "steps": 12}));
    let after = wb(&h);
    assert!(after > before + 1.0, "dragging right warms the photo ({before} K → {after} K)");
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:cancel"}));
    assert!((wb(&h) - before).abs() < 1.0, "Cancel restores {before} K ({} K)", wb(&h));
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
    for params in [json!({}), json!({"tool": 3}), json!({"tool": "nope"}), json!({"tool": "basic", "control": "effects.clarity"})] {
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
    assert!(has(&h, "touch:import"), "Library bar");
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:import"}));
    assert!(has(&h, "touch:import:folder"), "Import offers a folder");
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:import"}));
    let cell = h.app.widgets.iter().find(|(w, _)| w.starts_with("thumb:")).map(|(w, _)| w.clone());
    let Some(cell) = cell else { panic!("no grid cell on screen: {:?}", h.app.widgets.iter().map(|w| &w.0).take(40).collect::<Vec<_>>()) };
    ask(&mut h, "ui.clickWidget", json!({"id": cell}));
    assert_eq!(h.app.ui.module, crate::module::ModuleId::Develop);
    assert_eq!(h.app.ui.view, crate::state::ViewMode::Detail);
}

#[test]
fn the_empty_library_offers_finger_sized_ways_in_without_desktop_hints() {
    // a folding phone's cover screen and its inner, near-square screen
    for size in [[412.0, 960.0], [1092.0, 984.0]] {
        let app = LccApp::new(lcc_engine::Session::new(), Services { png: None, ..Default::default() });
        let mut h = Headless::new(app, size, 1.0);
        assert_eq!(exec(&mut h, "view.touchLayout", json!({"on": true}))["ok"], true);
        h.settle(SETTLE);
        assert!(has(&h, "button:emptyImportPhotos") && has(&h, "button:emptyImportFolder"), "{size:?}");
        assert!(!has(&h, "button:emptyImportLightroom"), "no Lightroom catalog import on touch ({size:?})");
        let height = h.app.widgets.iter().find(|(w, _)| w == "button:emptyImportPhotos").map(|(_, r)| r.height()).unwrap();
        assert!(height >= 44.0, "finger-sized buttons ({size:?}: {height})");
    }
}
