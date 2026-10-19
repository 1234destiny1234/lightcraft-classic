//! The Library's Grid as Lightroom Classic's (fork-owned): one grid without date sections, the Library Filter bar
//! above it, and \ toggling that bar in the Grid (and Show Original elsewhere, as in Lightroom Classic).

use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::state::BeforeAfter;
use crate::{LccApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

fn library() -> Headless {
    let app = LccApp::new(lcc_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    let mut h = Headless::new(app, [1600.0, 1000.0], 1.0);
    let r = h.request("ui.set", json!({"module": "library", "view": "photoGrid", "leftPanel": true, "right": "none"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    h
}

fn has(h: &Headless, prefix: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w.starts_with(prefix))
}

#[test]
fn the_grid_has_no_date_sections_and_shows_the_filter_bar() {
    let h = library();
    assert!(!has(&h, "group:"), "no date headers");
    assert!(has(&h, "filterbar"), "the Library Filter bar is on top");
}

#[test]
fn backslash_toggles_the_filter_bar_in_the_grid_and_show_original_elsewhere() {
    let mut h = library();
    let r = h.request("ui.key", json!({"key": "\\"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    assert!(!h.app.ui.filter_bar, "\\ hides the filter bar in the Grid");
    assert_eq!(h.app.ui.before_after, BeforeAfter::Off, "and doesn't show the original");
    let r = h.request("ui.set", json!({"module": "develop", "view": "detail"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    let r = h.request("ui.key", json!({"key": "\\"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    assert_eq!(h.app.ui.before_after, BeforeAfter::Original, "in Develop \\ shows the original");
    assert!(!h.app.ui.filter_bar, "the filter bar stays as it was");
}

/// The Library's right column starts with the Histogram, as Lightroom Classic's: the active photo's, above Quick
/// Develop.
#[test]
fn the_library_right_column_starts_with_the_histogram() {
    let h = library();
    let top = |id: &str| h.app.widgets.iter().find(|(w, _)| w == id).map(|(_, r)| r.top());
    let (histogram, quick) = (top("classicPanel:histogram"), top("classicPanel:quickDevelop"));
    assert!(histogram.is_some() && quick.is_some(), "{histogram:?} {quick:?}");
    assert!(histogram < quick, "Histogram first");
    assert!(top("histogram").is_some(), "its plot is drawn");
}
