//! The filter bar's camera / lens / keyword dropdowns: a search box that takes the typing as soon
//! as the dropdown opens, narrows the rows, keeps the "Any …" row, and starts empty next time.

use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::{LccApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

fn ok(h: &mut Headless, method: &str, params: serde_json::Value) {
    let r = h.request(method, params, T);
    assert_eq!(r["ok"], true, "{method}: {r}");
    h.settle(SETTLE);
}

fn rows(h: &Headless, picker: &str) -> Vec<String> {
    let prefix = format!("filterItem:{picker}:");
    h.app.widgets.iter().filter(|(w, _)| w.starts_with(&prefix)).map(|(w, _)| w.clone()).collect()
}

#[test]
fn typing_narrows_the_keyword_rows_and_keeps_any() {
    let app = LccApp::new(lcc_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1600.0, 1000.0], 1.0);
    // the filter bar shows over the grid
    h.app.ui.view = crate::state::ViewMode::PhotoGrid;
    h.app.ui.filter_bar = true;
    h.settle(SETTLE);
    let keywords = h.app.caches.filter_values(&h.app.session.catalog).keywords.clone();
    assert!(keywords.len() >= 2, "the demo library has keywords: {keywords:?}");
    let target = keywords[0].clone();
    ok(&mut h, "ui.clickWidget", json!({"id": "dropdown:filter-keyword"}));
    let all = rows(&h, "keyword");
    assert_eq!(all.len(), keywords.len() + 1, "every keyword and Any");
    // no click into the box first: it has the focus when the dropdown opens
    ok(&mut h, "ui.text", json!({"text": target.to_uppercase()}));
    let narrowed = rows(&h, "keyword");
    let matching = keywords.iter().filter(|k| k.to_lowercase().contains(&target.to_lowercase())).count();
    assert_eq!(narrowed.len(), matching + 1, "matches (any case) and Any: {narrowed:?}");
    assert!(narrowed.contains(&"filterItem:keyword:0".to_string()), "the Any row stays");
    assert!(narrowed.len() < all.len());
    // closed and opened again: the search starts empty, and other pickers never saw it
    ok(&mut h, "ui.key", json!({"key": "Escape"}));
    ok(&mut h, "ui.key", json!({"key": "Escape"}));
    ok(&mut h, "ui.clickWidget", json!({"id": "dropdown:filter-camera"}));
    let cameras = h.app.caches.filter_values(&h.app.session.catalog).cameras.len();
    assert_eq!(rows(&h, "camera").len(), cameras + 1, "the camera picker isn't filtered by the keyword search");
}

/// Issue #213: the keyword picker takes several keywords. It stays open while they are clicked, shows any of them,
/// and "Any keyword" clears them all.
#[test]
fn the_keyword_picker_takes_several_keywords() {
    let app = LccApp::new(lcc_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1600.0, 1000.0], 1.0);
    h.app.ui.view = crate::state::ViewMode::PhotoGrid;
    h.app.ui.filter_bar = true;
    h.settle(SETTLE);
    let keywords = h.app.caches.filter_values(&h.app.session.catalog).keywords.clone();
    assert!(keywords.len() >= 2, "the demo library has keywords: {keywords:?}");
    let all = h.app.session.visible_cloned().len();
    ok(&mut h, "ui.clickWidget", json!({"id": "dropdown:filter-keyword"}));
    ok(&mut h, "ui.clickWidget", json!({"id": "filterItem:keyword:1"}));
    assert_eq!(h.app.session.filter.keywords, vec![keywords[0].clone()]);
    let one = h.app.session.visible_cloned().len();
    assert!(rows(&h, "keyword").len() > 1, "the list stays open for another keyword");
    ok(&mut h, "ui.clickWidget", json!({"id": "filterItem:keyword:2"}));
    assert_eq!(h.app.session.filter.keywords, vec![keywords[0].clone(), keywords[1].clone()]);
    let two = h.app.session.visible_cloned().len();
    assert!(one <= two && two <= all, "any of them: {one} ≤ {two} ≤ {all}");
    // a second click takes one off again
    ok(&mut h, "ui.clickWidget", json!({"id": "filterItem:keyword:1"}));
    assert_eq!(h.app.session.filter.keywords, vec![keywords[1].clone()]);
    ok(&mut h, "ui.clickWidget", json!({"id": "filterItem:keyword:0"}));
    assert!(h.app.session.filter.keywords.is_empty() && h.app.session.filter.keyword.is_none());
    assert_eq!(h.app.session.visible_cloned().len(), all);
    assert!(rows(&h, "keyword").is_empty(), "Any keyword closes the list");
}

/// Issue #218: the filter bar's Metadata columns list each field's values with counts; a click narrows the grid,
/// the next column counts only what is left, and "All" clears the column again.
#[test]
fn metadata_columns_narrow_the_grid_and_cascade() {
    let app = LccApp::new(lcc_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1600.0, 1000.0], 1.0);
    h.app.ui.view = crate::state::ViewMode::PhotoGrid;
    h.app.ui.filter_bar = true;
    h.settle(SETTLE);
    let all = h.app.session.visible_cloned().len();
    ok(&mut h, "ui.clickWidget", json!({"id": "button:filterColumns"}));
    assert!(!h.app.ui.filter_columns.is_empty(), "the columns show");
    // column 1 lists cameras, column 2 lenses
    ok(&mut h, "engine.execute", json!({"command": "view.filterColumns", "params": {"column": 1, "field": "camera"}}));
    ok(&mut h, "engine.execute", json!({"command": "view.filterColumns", "params": {"column": 2, "field": "keyword"}}));
    let column_rows = |h: &Headless, c: usize| {
        let prefix = format!("filterValue:{c}:");
        h.app.widgets.iter().filter(|(w, _)| w.starts_with(&prefix)).count()
    };
    let keywords_before = column_rows(&h, 2);
    assert!(column_rows(&h, 1) > 2, "All and at least two cameras");
    // the first camera: the grid narrows to its photos
    ok(&mut h, "ui.clickWidget", json!({"id": "filterValue:1:1"}));
    let cameras = h.app.session.filter.cameras.clone();
    assert_eq!(cameras.len(), 1, "{:?}", h.app.session.filter);
    let narrowed = h.app.session.visible_cloned();
    assert!(!narrowed.is_empty() && narrowed.len() < all, "{} of {all}", narrowed.len());
    assert!(narrowed.iter().all(|id| h.app.session.catalog.photo(*id).unwrap().meta.camera.eq_ignore_ascii_case(&cameras[0])));
    // the keyword column counts only those photos' keywords
    assert!(column_rows(&h, 2) <= keywords_before, "the keyword column cascades");
    // the camera column still offers the other cameras
    assert!(column_rows(&h, 1) > 2);
    // All clears the camera column
    ok(&mut h, "ui.clickWidget", json!({"id": "filterValue:1:0"}));
    assert!(h.app.session.filter.cameras.is_empty());
    assert_eq!(h.app.session.visible_cloned().len(), all);
    // the field picker in a column header
    ok(&mut h, "ui.clickWidget", json!({"id": "dropdown:filterColumn0"}));
    ok(&mut h, "ui.clickWidget", json!({"id": "filterColumnField:0:flag"}));
    assert_eq!(h.app.ui.filter_columns[0], "flag");
    // bad arguments are errors
    for p in [
        json!({"column": 9, "field": "camera"}),
        json!({"column": 0, "field": "shoe"}),
        json!({"column": 0}),
        json!({"count": 0}),
        json!({"show": "yes"}),
    ] {
        let r = h.request("engine.execute", json!({"command": "view.filterColumns", "params": p}), T);
        assert_eq!(r["ok"], false, "{p}: {r}");
    }
    // hidden again
    ok(&mut h, "ui.clickWidget", json!({"id": "button:filterColumns"}));
    assert!(h.app.ui.filter_columns.is_empty());
    assert!(!h.app.widgets.iter().any(|(w, _)| w == "filterColumns"));
}
