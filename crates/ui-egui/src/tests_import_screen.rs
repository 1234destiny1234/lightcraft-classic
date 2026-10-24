//! The Import screen (fork-owned, `crate::import_screen`): its filters show what they say, Add in
//! place hides the copy-only rows, its button imports the checked photos, and Back cancels.

use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::import::ImportDialog;
use crate::state::Dialog;
use crate::{LccApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

fn has(h: &Headless, id: &str) -> bool {
    h.app.widgets.iter().any(|(w, _)| w == id)
}

fn click(h: &mut Headless, id: &str) {
    let r = h.request("ui.clickWidget", json!({"id": id}), T);
    assert_eq!(r["ok"], true, "{id}: {r}");
    h.settle(SETTLE);
}

fn review(dir: &std::path::Path, n: usize) -> Headless {
    std::fs::create_dir_all(dir).unwrap();
    // (a folder picker: Copy and Move are offered)
    let services = Services { png: None, pick_folder: Some(Box::new(|| None)), ..Default::default() };
    let app = LccApp::new(lcc_engine::Session::with_demo().with_fs(), services);
    let mut h = Headless::new(app, [1300.0, 900.0], 1.0);
    let mut paths = Vec::new();
    for i in 0..n {
        let img = lcc_raster::Rgba8 { width: 8, height: 8, data: vec![[(i * 30) as u8, 90, 9, 255]; 64] };
        let png = lcc_codecs::encode_png(&lcc_codecs::EncodeImage::rgba8(&img), &lcc_codecs::EncodeMeta::default()).unwrap();
        let p = dir.join(format!("new{i}.png"));
        std::fs::write(&p, png).unwrap();
        paths.push(p.to_string_lossy().to_string());
    }
    let r = h.request("engine.execute", json!({"command": "file.addPhotos", "params": {"paths": paths}}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    assert!(matches!(h.app.ui.dialog, Some(Dialog::Import { .. })), "the review is open");
    h
}

fn opts(h: &Headless) -> &ImportDialog {
    let Some(Dialog::Import { opts }) = &h.app.ui.dialog else { panic!("the review is open") };
    opts
}

fn cells(h: &Headless) -> usize {
    h.app.widgets.iter().filter(|(w, _)| w.strip_prefix("import:").is_some_and(|n| n.parse::<usize>().is_ok())).count()
}

#[test]
fn the_filters_show_new_photos_and_duplicates() {
    let dir = std::env::temp_dir().join(format!("lcc-import-screen-filter-{}", std::process::id()));
    let mut h = review(&dir, 3);
    // one of them already in the library
    if let Some(Dialog::Import { opts }) = &mut h.app.ui.dialog {
        opts.candidates[1].duplicate = Some("content".into());
        opts.candidates[1].existing = Some(1);
    }
    h.settle(SETTLE);
    assert_eq!(cells(&h), 3, "All");
    click(&mut h, "import:filter:new");
    assert_eq!(cells(&h), 2, "New");
    click(&mut h, "import:filter:dups");
    assert_eq!(cells(&h), 1, "Duplicates");
    click(&mut h, "import:filter:all");
    assert_eq!(cells(&h), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_in_place_hides_the_copy_rows_and_copy_shows_them() {
    let dir = std::env::temp_dir().join(format!("lcc-import-screen-how-{}", std::process::id()));
    let mut h = review(&dir, 2);
    click(&mut h, "button:importCopy");
    assert!(opts(&h).copy && !opts(&h).move_files);
    assert!(has(&h, "combo:importOrganize") && has(&h, "field:importRename") && has(&h, "check:importDng"), "the copy rows");
    click(&mut h, "button:importMove");
    assert!(opts(&h).move_files);
    assert!(!has(&h, "check:importDng"), "no DNG when moving");
    click(&mut h, "button:importAdd");
    assert!(!opts(&h).copy);
    assert!(!has(&h, "combo:importOrganize") && !has(&h, "field:importRename"), "hidden, not greyed");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_button_imports_the_checked_photos_and_back_cancels() {
    let dir = std::env::temp_dir().join(format!("lcc-import-screen-go-{}", std::process::id()));
    let mut h = review(&dir, 2);
    click(&mut h, "button:importAdd");
    let before = h.app.session.catalog.photos().count();
    click(&mut h, "import:go");
    assert!(h.app.ui.dialog.is_none(), "the review closes");
    let start = std::time::Instant::now();
    while h.app.session.catalog.photos().count() < before + 2 {
        assert!(start.elapsed() < T, "imported");
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    // a second review: Back closes it without importing
    let mut h = review(&dir.join("again"), 1);
    let before = h.app.session.catalog.photos().count();
    click(&mut h, "import:back");
    assert!(h.app.ui.dialog.is_none());
    for _ in 0..10 {
        h.step();
    }
    assert_eq!(h.app.session.catalog.photos().count(), before, "nothing imported");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nothing_checked_leaves_the_button_doing_nothing() {
    let dir = std::env::temp_dir().join(format!("lcc-import-screen-none-{}", std::process::id()));
    let mut h = review(&dir, 2);
    click(&mut h, "button:importNone");
    assert!(opts(&h).selected_paths().is_empty());
    let r = h.request("ui.clickWidget", json!({"id": "import:go"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    assert!(matches!(h.app.ui.dialog, Some(Dialog::Import { .. })), "still open: nothing to import");
    let _ = std::fs::remove_dir_all(&dir);
}
