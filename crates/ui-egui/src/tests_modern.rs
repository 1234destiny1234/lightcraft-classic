//! Headless tests of the Modern layout (`crate::modern`): the floating chrome, the dock's groups and
//! their cards of pill sliders, the tools, the dock's two places, the menu bar, the Library /
//! Develop switch, and bad commands as errors.

use std::time::Duration;

use serde_json::{Value, json};

use crate::headless::Headless;
use crate::modern::Form;
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

/// The demo library in Develop on its first photo, Modern layout on.
fn develop(size: [f32; 2]) -> Headless {
    let app = LccApp::new(lcc_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, size, 1.0);
    let id = h.app.session.visible_cloned()[0].0;
    assert_eq!(exec(&mut h, "view.modernLayout", json!({"on": true}))["ok"], true);
    assert_eq!(exec(&mut h, "library.select", json!({"ids": [id]}))["ok"], true);
    assert_eq!(exec(&mut h, "module.develop", json!({}))["ok"], true);
    assert_eq!(exec(&mut h, "view.detail", json!({}))["ok"], true);
    exec(&mut h, "modern.chrome", json!({"on": false}));
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
fn the_chrome_floats_over_the_photo_and_a_tap_hides_and_shows_it() {
    let mut h = develop([1280.0, 800.0]);
    exec(&mut h, "modern.chrome", json!({"on": true}));
    for id in ["modern:canvas", "modern:dock", "modern:group:light", "modern:group:crop", "modern:menu", "modern:mode:library", "modern:histogram"] {
        assert!(has(&h, id), "{id}");
    }
    assert_eq!(ask(&mut h, "ui.clickWidget", json!({"id": "modern:canvas", "fx": 0.4, "fy": 0.4}))["ok"], true);
    assert!(!h.app.ui.modern.chrome, "a tap hides the chrome");
    assert!(!has(&h, "modern:dock"));
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:canvas", "fx": 0.4, "fy": 0.4}));
    assert!(h.app.ui.modern.chrome && has(&h, "modern:dock"), "a second tap shows it");
}

#[test]
fn a_group_opens_its_card_of_pills_and_a_drag_is_one_undo_step() {
    let mut h = develop([1280.0, 800.0]);
    exec(&mut h, "modern.chrome", json!({"on": true}));
    let before = value(&h, "light.exposure");
    assert_eq!(ask(&mut h, "ui.clickWidget", json!({"id": "modern:group:light"}))["ok"], true);
    assert_eq!(h.app.ui.modern.group, Some(crate::modern_dock::Group::Light));
    assert!(has(&h, "modern:card") && has(&h, "slider:light.exposure"), "the card's pills");
    let undo = h.app.session.undo.len();
    let r = ask(&mut h, "ui.dragWidget", json!({"id": "slider:light.exposure", "fx": 0.2, "dx": 120.0, "steps": 12}));
    assert_eq!(r["ok"], true, "{r}");
    let after = value(&h, "light.exposure");
    assert!(after > before, "dragging right raises exposure ({before} → {after})");
    assert_eq!(h.app.session.undo.len(), undo + 1, "one drag, one undo step");
    // a second tap on the open group puts it away; a tap on the photo does too
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:group:light"}));
    assert!(h.app.ui.modern.group.is_none() && !has(&h, "modern:card"));
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:group:effects"}));
    assert!(has(&h, "slider:effects.clarity"));
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:canvas", "fx": 0.5, "fy": 0.2}));
    assert!(h.app.ui.modern.group.is_none(), "a tap on the photo puts the card away");
    assert!(h.app.ui.modern.chrome, "and leaves the chrome");
}

/// Light holds the tone sliders, Colour white balance and saturation, Tone the parametric curve,
/// Effects presence, vignette and grain.
#[test]
fn the_groups_hold_their_sliders() {
    let mut h = develop([1280.0, 800.0]);
    exec(&mut h, "modern.chrome", json!({"on": true}));
    let groups: [(&str, &[&str]); 4] = [
        ("light", &["light.exposure", "light.blacks"]),
        ("colour", &["color.vibrance", "color.saturation"]),
        ("tone", &["curve.highlights", "curve.splitMid"]),
        ("effects", &["effects.texture", "effects.dehaze", "vignette.amount", "grain.amount"]),
    ];
    for (group, controls) in groups {
        assert_eq!(exec(&mut h, "modern.group", json!({"group": group}))["ok"], true, "{group}");
        for control in controls {
            assert!(has(&h, &format!("slider:{control}")), "{group} offers {control}");
        }
    }
    // white balance on the photo's own scale: Kelvin for raws, relative otherwise
    exec(&mut h, "modern.group", json!({"group": "colour"}));
    let id = h.app.session.selection.active.unwrap();
    let raw = h.app.session.catalog.photo(id).is_some_and(|p| p.develops_raw() && !p.relative_wb());
    assert!(has(&h, if raw { "slider:wb.temp" } else { "slider:wb.tempRel" }));
}

#[test]
fn the_tools_open_their_panels_and_dodge_starts_a_lightening_brush() {
    let mut h = develop([1280.0, 800.0]);
    exec(&mut h, "modern.chrome", json!({"on": true}));
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:group:crop"}));
    assert_eq!(h.app.ui.right, crate::state::RightPanel::Crop);
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:group:crop"}));
    assert_eq!(h.app.ui.right, crate::state::RightPanel::Edit, "closing a tool leaves its overlay");
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:group:dodge"}));
    assert_eq!(h.app.ui.right, crate::state::RightPanel::Masking);
    let id = h.app.session.selection.active.unwrap();
    let masks = h.app.session.develop_of(id).unwrap_or_default().masks.clone();
    let last = masks.last().expect("a new mask");
    assert!(last.adjust.exposure > 0.0, "Dodge lightens ({})", last.adjust.exposure);
}

#[test]
fn the_dock_moves_to_the_side_and_back() {
    let mut h = develop([1280.0, 800.0]);
    exec(&mut h, "modern.chrome", json!({"on": true}));
    let dock = |h: &Headless| h.app.widgets.iter().rev().find(|(w, _)| w == "modern:dock").map(|(_, r)| *r).unwrap();
    assert!(dock(&h).width() > dock(&h).height(), "along the bottom");
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:dockSwitch"}));
    assert!(h.app.ui.modern.dock_side);
    assert!(dock(&h).height() > dock(&h).width() && dock(&h).left() < 40.0, "down the side");
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:dockSwitch"}));
    assert!(!h.app.ui.modern.dock_side);
}

#[test]
fn the_menu_opens_on_shortcuts_and_reaches_every_menu() {
    let mut h = develop([1280.0, 800.0]);
    exec(&mut h, "modern.chrome", json!({"on": true}));
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:menu"}));
    assert_eq!(h.app.ui.modern.menu.as_deref(), Some("Shortcuts"));
    assert!(has(&h, "modern:shortcut:import") && has(&h, "modern:shortcut:export"));
    for title in ["File", "Edit", "Photo", "View", "Help"] {
        assert!(has(&h, &format!("modern:menu:{title}")), "{title}");
    }
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:menu:File"}));
    assert_eq!(h.app.ui.modern.menu.as_deref(), Some("File"));
    ask(&mut h, "ui.key", json!({"key": "Escape"}));
    assert!(h.app.ui.modern.menu.is_none(), "Esc closes it");
    assert_eq!(h.app.ui.module, crate::module::ModuleId::Develop, "and nothing more");
}

#[test]
fn the_switch_goes_between_library_and_develop() {
    let mut h = develop([1280.0, 800.0]);
    exec(&mut h, "modern.chrome", json!({"on": true}));
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:mode:library"}));
    assert_eq!(h.app.ui.module, crate::module::ModuleId::Library);
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:mode:develop"}));
    assert_eq!(h.app.ui.module, crate::module::ModuleId::Develop);
}

#[test]
fn the_phone_and_tablet_forms() {
    assert!(Form::of(egui::vec2(1280.0, 800.0)).side_panel());
    assert!(!Form::of(egui::vec2(800.0, 1280.0)).side_panel());
    assert!(!Form::of(egui::vec2(400.0, 860.0)).side_panel());
}

#[test]
fn bad_modern_commands_are_errors_not_panics() {
    let mut h = develop([400.0, 860.0]);
    for params in [json!({}), json!({"group": 3}), json!({"group": "nope"})] {
        let r = exec(&mut h, "modern.group", params.clone());
        assert_eq!(r["ok"], false, "modern.group {params}: {r}");
    }
    assert_eq!(exec(&mut h, "modern.group", json!({"group": null}))["ok"], true, "nothing open: nothing to close");
    assert_eq!(exec(&mut h, "modern.menu", json!({"menu": "Nope"}))["ok"], false);
    assert_eq!(exec(&mut h, "modern.menu", json!({"menu": 7}))["ok"], true, "not a name: the first menu");
    exec(&mut h, "modern.menu", json!({"on": false}));
    // the old touch command names are gone
    assert_eq!(exec(&mut h, "modern.tool", json!({"tool": "basic"}))["ok"], false);
    // the layout turned off leaves Classic's edges
    exec(&mut h, "view.modernLayout", json!({"on": false}));
    assert!(!has(&h, "modern:canvas"));
}

#[test]
fn a_tap_in_the_library_grid_opens_the_photo() {
    let app = LccApp::new(lcc_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1280.0, 800.0], 1.0);
    exec(&mut h, "view.modernLayout", json!({"on": true}));
    exec(&mut h, "view.photoGrid", json!({}));
    assert!(has(&h, "modern:import"), "Library bar");
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:import"}));
    assert!(has(&h, "modern:import:folder"), "Import offers a folder");
    ask(&mut h, "ui.clickWidget", json!({"id": "modern:import"}));
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
        assert_eq!(exec(&mut h, "view.modernLayout", json!({"on": true}))["ok"], true);
        h.settle(SETTLE);
        assert!(has(&h, "button:emptyImportPhotos") && has(&h, "button:emptyImportFolder"), "{size:?}");
        assert!(!has(&h, "button:emptyImportLightroom"), "no Lightroom catalog import on touch ({size:?})");
        let height = h.app.widgets.iter().find(|(w, _)| w == "button:emptyImportPhotos").map(|(_, r)| r.height()).unwrap();
        assert!(height >= 44.0, "finger-sized buttons ({size:?}: {height})");
    }
}
