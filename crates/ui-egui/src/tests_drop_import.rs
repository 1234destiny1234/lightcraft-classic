//! A folder dragged from the file manager onto the window imports the way File ▸ Import from
//! Folder… does: the folder is scanned (its subfolders too) and the import review opens; its
//! activity row finishes. Dropped photo files are added straight away, as before.

use std::path::PathBuf;
use std::time::Duration;

use crate::headless::Headless;
use crate::{LccApp, Services};

const T: Duration = Duration::from_secs(20);

/// A fresh temporary folder `tag` with a photo in it and one in a subfolder.
fn folder(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("lc-drop-import-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let sub = root.join("day 2");
    std::fs::create_dir_all(&sub).unwrap();
    // different pictures: the same bytes twice would import once (a duplicate)
    for (file, shade) in [(root.join("a.png"), 90), (sub.join("b.png"), 200)] {
        let img = lcc_raster::Rgba8 { width: 8, height: 8, data: vec![[shade, 120, 160, 255]; 64] };
        let png = lcc_codecs::encode_png(&lcc_codecs::EncodeImage::rgba8(&img), &lcc_codecs::EncodeMeta::default()).unwrap();
        std::fs::write(file, png).unwrap();
    }
    root
}

fn app() -> Headless {
    let session = lcc_engine::Session::new().with_fs();
    let mut h = Headless::new(LccApp::new(session, Services { png: None, ..Default::default() }), [1200.0, 900.0], 1.0);
    h.settle(T);
    h
}

/// Run frames (without other input, as a window left alone) until `done` or the timeout.
fn wait(h: &mut Headless, mut done: impl FnMut(&Headless) -> bool) -> bool {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < T {
        h.step();
        if done(h) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn a_dropped_folder_opens_the_import_review_like_add_folder() {
    let dir = folder("review");
    let mut h = app();
    h.dropped.push(dir.clone());
    let opened = wait(&mut h, |h| matches!(h.app.ui.dialog, Some(crate::state::Dialog::Import { .. })));
    assert!(opened, "no import review after the drop: {:?}", h.app.ui.dialog);
    let Some(crate::state::Dialog::Import { opts }) = &h.app.ui.dialog else { panic!("no import review") };
    assert_eq!(opts.candidates.len(), 2, "both files, the subfolder's too");
    assert_eq!(opts.sources, vec![dir.to_string_lossy().to_string()]);
    assert!(h.app.session.activity.list().is_empty(), "the scan's activity row finished: {:?}", h.app.session.activity.list());
    // confirming the review adds the photos and the import's row finishes too
    let r = h.request("ui.dialog.confirm", serde_json::json!({}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(wait(&mut h, |h| h.app.import.is_none() && h.app.session.catalog.photos().count() == 2), "the import finished");
    assert!(h.app.session.activity.list().is_empty(), "no row left behind: {:?}", h.app.session.activity.list());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropped_photo_files_are_added_and_the_row_finishes() {
    let dir = folder("files");
    let mut h = app();
    h.dropped.push(dir.join("a.png"));
    assert!(wait(&mut h, |h| h.app.import.is_none() && h.app.session.catalog.photos().count() == 1), "the dropped photo was added");
    assert!(h.app.session.activity.list().is_empty(), "no row left behind: {:?}", h.app.session.activity.list());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dropped_folder_that_vanished_says_so_and_leaves_nothing_running() {
    let dir = folder("gone").join("missing");
    let mut h = app();
    h.dropped.push(dir);
    for _ in 0..30 {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(wait(&mut h, |h| h.app.scan.is_none() && h.app.import.is_none()), "nothing stays running");
    assert!(h.app.session.activity.list().is_empty(), "no row left behind: {:?}", h.app.session.activity.list());
    assert!(h.app.ui.toast.is_some(), "the failure is shown");
}
