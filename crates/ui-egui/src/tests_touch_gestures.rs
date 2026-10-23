//! Headless tests of the touch layout's gestures and edges: a swipe scrolls the grid (a tap still
//! opens, a long press picks), Back closes things in order and hands the root to the system, the
//! UI keeps clear of the screen's insets, the adjusted parameter stays in view, and Develop's
//! histogram pins or floats.

use std::time::Duration;

use serde_json::{Value, json};

use crate::headless::Headless;
use crate::module::ModuleId;
use crate::touch::HistogramPlace;
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

fn rect(h: &Headless, id: &str) -> Option<egui::Rect> {
    h.app.widgets.iter().rev().find(|(w, _)| w == id).map(|(_, r)| *r)
}

/// The demo library's grid in the touch layout.
fn library(size: [f32; 2]) -> Headless {
    let app = LccApp::new(lcc_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, size, 1.0);
    assert_eq!(exec(&mut h, "view.touchLayout", json!({"on": true}))["ok"], true);
    exec(&mut h, "view.squareGrid", json!({}));
    h
}

/// The demo library in Develop on its first photo, touch layout on.
fn develop(size: [f32; 2]) -> Headless {
    let mut h = library(size);
    let id = h.app.session.visible_cloned()[0].0;
    exec(&mut h, "library.select", json!({"ids": [id]}));
    exec(&mut h, "module.develop", json!({}));
    exec(&mut h, "view.detail", json!({}));
    exec(&mut h, "touch.chrome", json!({"on": true}));
    h
}

fn first_thumb(h: &Headless) -> (String, u64) {
    let grid = rect(h, "grid:scroll").expect("the grid");
    let visible = |r: &egui::Rect| r.intersect(grid).area();
    let (w, _) = h
        .app
        .widgets
        .iter()
        .filter(|(w, _)| w.starts_with("thumb:"))
        .max_by(|a, b| visible(&a.1).total_cmp(&visible(&b.1)))
        .expect("a grid cell on screen");
    (w.clone(), w["thumb:".len()..].parse().unwrap())
}

#[test]
fn a_swipe_scrolls_the_grid_without_picking_or_dragging_a_photo() {
    // narrow and short: the demo library is taller than the screen
    let mut h = library([360.0, 420.0]);
    let selected = h.app.session.selection.ids.clone();
    let top = h.app.grid_scroll.unwrap_or(0.0);
    let (cell, _) = first_thumb(&h);
    let r = ask(&mut h, "ui.dragWidget", json!({"id": cell, "dy": -160.0, "steps": 8}));
    assert_eq!(r["ok"], true, "{r}");
    let after = h.app.grid_scroll.unwrap_or(0.0);
    assert!(after > top + 40.0, "swiping up scrolls the grid down ({top} → {after})");
    assert_eq!(h.app.ui.module, ModuleId::Library, "a swipe opens nothing");
    assert_eq!(h.app.session.selection.ids, selected, "a swipe picks nothing");
    assert!(h.app.ui.dragging_photos.is_none(), "a swipe drags no photos");
    // a tap still opens the photo under it
    let (cell, id) = first_thumb(&h);
    ask(&mut h, "ui.clickWidget", json!({"id": cell}));
    assert_eq!(h.app.ui.module, ModuleId::Develop);
    assert_eq!(h.app.session.active().map(|p| p.0), Some(id));
}

#[test]
fn a_long_press_picks_photos_and_taps_then_add_until_back() {
    let mut h = library([600.0, 900.0]);
    let cells: Vec<(String, egui::Rect)> = h.app.widgets.iter().filter(|(w, _)| w.starts_with("thumb:")).take(3).cloned().collect();
    assert!(cells.len() >= 3);
    let at = cells[0].1.center();
    h.app.synthetic.push(egui::Event::PointerMoved(at));
    h.app.synthetic.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    for _ in 0..60 {
        h.step();
    }
    h.app.synthetic.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.settle(SETTLE);
    assert!(h.app.ui.touch.selecting, "a long press starts picking");
    assert_eq!(h.app.ui.module, ModuleId::Library, "and opens nothing");
    assert_eq!(h.app.session.selection.ids.len(), 1);
    assert!(rect(&h, "touch:endSelect").is_some(), "the bar counts the picked photos");
    for (w, _) in &cells[1..] {
        ask(&mut h, "ui.clickWidget", json!({"id": w}));
    }
    assert_eq!(h.app.session.selection.ids.len(), 3, "taps add photos while picking");
    assert_eq!(h.app.ui.module, ModuleId::Library);
    let r = exec(&mut h, "touch.back", json!({}));
    assert_eq!(r["result"]["closed"], "selection", "{r}");
    assert!(!h.app.ui.touch.selecting);
    assert!(h.app.session.selection.ids.len() <= 1);
}

#[test]
fn back_closes_dialog_tool_panel_and_develop_then_leaves_the_root_to_the_system() {
    let mut h = develop([800.0, 1280.0]);
    let back = |h: &mut Headless| {
        let r = exec(h, "touch.back", json!({}));
        assert_eq!(r["ok"], true, "{r}");
        r["result"].clone()
    };
    exec(&mut h, "app.settings", json!({}));
    assert!(h.app.ui.dialog.is_some());
    assert_eq!(back(&mut h)["closed"], "dialog");
    assert!(h.app.ui.dialog.is_none(), "Back closes the dialog as Esc does");

    exec(&mut h, "touch.tool", json!({"tool": "light", "control": "light.exposure"}));
    ask(&mut h, "ui.dragWidget", json!({"id": "touch:canvas", "fx": 0.4, "dx": 150.0, "steps": 8}));
    let id = h.app.session.active().unwrap();
    let changed = lcc_develop::controls::get(&h.app.session.develop_of(id).unwrap_or_default(), "light.exposure").unwrap();
    assert!(changed > 0.0);
    assert_eq!(back(&mut h)["closed"], "tool");
    assert!(h.app.ui.touch.tool.is_none());
    let kept = lcc_develop::controls::get(&h.app.session.develop_of(id).unwrap_or_default(), "light.exposure").unwrap();
    assert_eq!(kept, changed, "Back leaves the tool as Done does: the change stays");

    exec(&mut h, "touch.panel", json!({"on": true}));
    assert_eq!(back(&mut h)["closed"], "panel");
    assert!(!h.app.ui.touch.panel);

    assert_eq!(back(&mut h)["closed"], "develop");
    assert_eq!(h.app.ui.module, ModuleId::Library);

    assert!(!crate::touch::take_system_back(&mut h.app));
    let root = back(&mut h);
    assert_eq!(root["handled"], false, "{root}");
    assert!(crate::touch::take_system_back(&mut h.app), "the root's Back goes to the system");
}

#[test]
fn the_back_key_runs_the_same_steps() {
    let mut h = develop([800.0, 1280.0]);
    exec(&mut h, "touch.panel", json!({"on": true}));
    ask(&mut h, "ui.key", json!({"key": "BrowserBack"}));
    assert!(!h.app.ui.touch.panel, "the Back key closed the sheet");
    assert_eq!(h.app.ui.module, ModuleId::Develop, "one press, one step");
}

#[test]
fn the_ui_keeps_clear_of_the_cutout_and_the_system_bars() {
    // a folding phone's cover screen with the camera cutout at the top, and a gesture bar
    let (top, bottom) = (38.0, 18.0);
    let mut h = library([412.0, 960.0]);
    assert_eq!(exec(&mut h, "touch.insets", json!({"top": top, "bottom": bottom}))["ok"], true);
    let import = rect(&h, "touch:import").expect("library bar");
    assert!(import.top() >= top, "the Library bar is below the cutout ({import:?})");
    let mut h = develop([412.0, 960.0]);
    exec(&mut h, "touch.insets", json!({"top": top, "bottom": bottom}));
    let b = rect(&h, "touch:back").expect("top bar");
    assert!(b.top() >= top, "Develop's top bar is below the cutout ({b:?})");
    let strip = rect(&h, "touch:strip").expect("tool strip");
    assert!(strip.bottom() <= 960.0 - bottom, "the tool strip is above the gesture bar ({strip:?})");
    exec(&mut h, "touch.panel", json!({"on": true}));
    let grab = rect(&h, "touch:sheetGrab").expect("sheet");
    assert!(grab.bottom() <= 960.0 - bottom);
    // nonsense is refused or cleaned, never a panic
    assert_eq!(exec(&mut h, "touch.insets", json!({"top": "lots"}))["ok"], false);
    let r = exec(&mut h, "touch.insets", json!({"top": -5.0, "left": 1e30}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(h.app.ui.touch.insets.top, 0.0);
    assert!(h.app.ui.touch.insets.left <= 400.0);
}

#[test]
fn the_parameter_being_adjusted_stays_in_view() {
    let mut h = develop([412.0, 960.0]);
    exec(&mut h, "touch.tool", json!({"tool": "detail", "control": "detail.sharpenAmount"}));
    let chips = rect(&h, "touch:chips").expect("the parameter row");
    // step down through every parameter with up/down drags on the photo
    for _ in 0..5 {
        ask(&mut h, "ui.dragWidget", json!({"id": "touch:canvas", "fx": 0.5, "fy": 0.3, "dy": 70.0, "steps": 8}));
        let id = format!("touch:param:{}", h.app.ui.touch.control);
        let chip = rect(&h, &id).expect("the chosen chip");
        assert!(chip.left() >= chips.left() - 1.0 && chip.right() <= chips.right() + 1.0, "{id} is inside the row ({chip:?} in {chips:?})");
    }
    assert_eq!(h.app.ui.touch.control, "detail.nrColor", "stepped to the last");
    // one in the middle sits in the middle
    exec(&mut h, "touch.tool", json!({"tool": "detail", "control": "detail.sharpenMasking"}));
    let chip = rect(&h, "touch:param:detail.sharpenMasking").unwrap();
    assert!((chip.center().x - chips.center().x).abs() < 40.0, "centred ({chip:?} in {chips:?})");
}

#[test]
fn the_histogram_pins_under_the_top_bar_or_floats_where_it_is_dragged() {
    let mut h = develop([412.0, 960.0]);
    assert_eq!(h.app.ui.touch.histogram, HistogramPlace::Off, "off by default");
    assert!(rect(&h, "touch:histogram").is_none());
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:histogramButton"}));
    assert_eq!(h.app.ui.touch.histogram, HistogramPlace::Pinned, "portrait pins it");
    let hist = rect(&h, "touch:histogram").unwrap();
    let bar = rect(&h, "touch:back").unwrap();
    assert!(hist.top() >= bar.bottom(), "under the top bar");

    let mut h = develop([1280.0, 800.0]);
    ask(&mut h, "ui.clickWidget", json!({"id": "touch:histogramButton"}));
    assert_eq!(h.app.ui.touch.histogram, HistogramPlace::Floating, "landscape floats it");
    let before = rect(&h, "touch:histogram").unwrap();
    ask(&mut h, "ui.dragWidget", json!({"id": "touch:histogram", "dx": -300.0, "dy": 200.0, "steps": 10}));
    let after = rect(&h, "touch:histogram").unwrap();
    assert!(after.left() < before.left() - 100.0 && after.top() > before.top() + 100.0, "{before:?} → {after:?}");
    let at = h.app.ui.touch.histogram_at;
    assert!(at.iter().all(|v| (0.0..=1.0).contains(v)), "kept as a fraction ({at:?})");
    // saved with the UI state
    let saved: crate::touch::TouchUi = serde_json::from_value(serde_json::to_value(&h.app.ui.touch).unwrap()).unwrap();
    assert_eq!((saved.histogram, saved.histogram_at), (HistogramPlace::Floating, at));
    for bad in [json!({"mode": "sideways"}), json!({"mode": 3}), json!({"mode": "off", "at": [0.5]}), json!({"at": "x"})] {
        assert_eq!(exec(&mut h, "touch.histogram", bad.clone())["ok"], false, "{bad}");
    }
    assert_eq!(exec(&mut h, "touch.histogram", json!({"mode": "off"}))["ok"], true);
    assert!(rect(&h, "touch:histogram").is_none());
}
