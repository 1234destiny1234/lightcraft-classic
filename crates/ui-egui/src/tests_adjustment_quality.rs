//! Adjustment quality is independent of interaction scheduling and the preview size limit.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use lcc_catalog::{Op, Photo, PhotoId, Source};
use lcc_engine::media::{RenderJob, RenderResult};
use lcc_pipeline::Quality;
use serde_json::json;

use crate::headless::Headless;
use crate::render::{PANE_AFTER, PANE_BEFORE, RenderOffload, Slot};
use crate::state::{AppSettings, BeforeAfter, ViewMode, Zoom};
use crate::{LccApp, Services};

const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Default)]
struct Work {
    asked: Vec<(Slot, RenderJob)>,
    running: Vec<(Slot, RenderJob)>,
}

struct Record(Rc<RefCell<Work>>);

impl RenderOffload for Record {
    fn try_start(&mut self, slot: Slot, job: RenderJob) -> Option<RenderJob> {
        let mut work = self.0.borrow_mut();
        work.asked.push((slot, job.clone()));
        work.running.push((slot, job));
        None
    }

    fn finished(&mut self) -> Vec<(Slot, RenderResult, f64)> {
        std::mem::take(&mut self.0.borrow_mut().running).into_iter().map(|(slot, job)| (slot, job.run(), 0.0)).collect()
    }
}

fn detail(native: [u32; 2], ppp: f32) -> (Headless, Rc<RefCell<Work>>) {
    let mut session = lcc_engine::Session::new();
    session.media.file_loader = Some(Arc::new(move |_, _, _| {
        let (w, h) = (native[0] as usize, native[1] as usize);
        let mut image = lcc_raster::Rgb32f::new(w, h);
        // Fine stripes, edges, colour and noise expose resampling and draft NR shortcuts.
        for (i, pixel) in image.data.iter_mut().enumerate() {
            let (x, y) = (i % w, i / w);
            let stripe = if (x / 2 + y / 3) % 2 == 0 { 0.12 } else { 0.72 };
            *pixel = [stripe, stripe * 0.7 + (x % 7) as f32 * 0.01, stripe * 0.5 + (y % 11) as f32 * 0.01];
        }
        Ok((image, Default::default()))
    }));
    let photo = Photo::new(
        PhotoId(1),
        Source::File { path: "adjustment-quality.jpg".into() },
        "adjustment-quality.jpg",
        "JPEG",
        native[0],
        native[1],
        "2026-10-22",
    );
    session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    session.selection = lcc_engine::Selection::single(PhotoId(1));
    session.execute("develop.set", &json!({"values": {"light.highlights": -35.0, "detail.nrColor": 35.0}})).unwrap();
    let mut app = LccApp::new(session, Services { png: None, ..Default::default() });
    app.ui.view = ViewMode::Detail;
    app.ui.right = crate::state::RightPanel::None;
    app.ui.filmstrip = false;
    app.ui.settings.high_quality_adjustment_previews = true;
    let work = Rc::new(RefCell::new(Work::default()));
    app.renderer.set_offload(Box::new(Record(work.clone())));
    let mut h = Headless::new(app, [740.3, 570.5], ppp);
    assert!(h.settle(TIMEOUT));
    (h, work)
}

fn latest(work: &Rc<RefCell<Work>>, slot: Slot) -> RenderJob {
    work.borrow().asked.iter().rev().find(|(s, _)| *s == slot).expect("requested view").1.clone()
}

fn pixels(h: &Headless, slot: Slot) -> Arc<egui::ColorImage> {
    h.app.renderer.textures[&slot].pixels.clone().expect("headless texture pixels")
}

#[test]
fn high_quality_adjustment_preference_round_trips_and_defaults_off() {
    let old: AppSettings = serde_json::from_value(json!({"previewLimit": 1600})).unwrap();
    assert!(!old.high_quality_adjustment_previews);
    let chosen = AppSettings { high_quality_adjustment_previews: true, ..old };
    let saved = serde_json::to_value(&chosen).unwrap();
    assert_eq!(saved["highQualityAdjustmentPreviews"], true);
    let back: AppSettings = serde_json::from_value(saved).unwrap();
    assert!(back.high_quality_adjustment_previews);
    assert_eq!(back.preview_limit, 1600);
}

#[test]
fn high_quality_adjustment_setting_is_available_in_rendering_preferences() {
    let (mut h, _) = detail([640, 480], 1.0);
    h.app.ui.settings.high_quality_adjustment_previews = false;
    h.app.ui.dialog = Some(crate::state::Dialog::Settings { tab: "performance".into() });
    h.step();
    let reply = h.request("ui.clickWidget", json!({"id": "check:settings.highQualityAdjustmentPreviews"}), TIMEOUT);
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(h.app.ui.settings.high_quality_adjustment_previews);
}

#[test]
fn high_quality_adjustments_match_settled_pixels_at_every_display_scale() {
    let _ = lcc_engine::gpu::available();
    for native in [[1024, 768], [1200, 800]] {
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            let (mut h, work) = detail(native, ppp);
            // Also exercise an explicitly capped preview and a CPU-only proofed view.
            h.app.ui.settings.preview_limit = 1600;
            h.app.ui.soft_proof = ppp == 1.25;
            assert!(h.settle(TIMEOUT));
            let still = latest(&work, Slot::Main);
            let still_pixels = pixels(&h, Slot::Main);
            h.app.session.begin_interaction("Exposure").unwrap();
            assert!(h.settle(TIMEOUT));
            assert_eq!(pixels(&h, Slot::Main).pixels, still_pixels.pixels, "unchanged drag: {native:?}, {ppp}");
            for value in [0.25, -0.25] {
                h.app.session.execute("develop.set", &json!({"values": {"light.exposure": value}})).unwrap();
                assert!(h.settle(TIMEOUT));
                let live = latest(&work, Slot::Main);
                assert_eq!(live.request.quality, Quality::Full);
                assert_eq!((live.request.max_w, live.request.max_h), (still.request.max_w, still.request.max_h));
                assert_eq!(live.display_fit, still.display_fit);
                let live_pixels = pixels(&h, Slot::Main);
                h.app.session.end_interaction().unwrap();
                assert!(h.settle(TIMEOUT));
                assert_eq!(pixels(&h, Slot::Main).size, live_pixels.size);
                assert_eq!(pixels(&h, Slot::Main).pixels, live_pixels.pixels, "release: {native:?}, {ppp}");
                h.app.session.begin_interaction("Exposure").unwrap();
            }
            h.app.session.end_interaction().unwrap();
        }
    }
}

#[test]
fn high_quality_adjustments_respect_preview_and_texture_limits() {
    let (mut h, work) = detail([2000, 1500], 1.0);
    h.app.ui.settings.preview_limit = 1600;
    assert!(h.settle(TIMEOUT));
    let still = latest(&work, Slot::Main);
    assert_eq!(still.request.max_w, 1600);
    h.app.session.begin_interaction("Exposure").unwrap();
    h.app.session.execute("develop.set", &json!({"values": {"light.exposure": 0.5}})).unwrap();
    assert!(h.settle(TIMEOUT));
    assert_eq!(latest(&work, Slot::Main).request.max_w, 1600);
    h.max_texture_side = 1024;
    assert!(h.settle(TIMEOUT));
    let limited = latest(&work, Slot::Main);
    assert_eq!(limited.request.max_w, 1024);
    assert_eq!(limited.request.quality, Quality::Full);
}

#[test]
fn high_quality_adjustment_hdr_jobs_keep_full_quality_and_extended_values() {
    struct Hdr;
    impl crate::HdrOutput for Hdr {
        fn headroom(&mut self) -> Option<f32> {
            Some(4.0)
        }
        fn upload(&mut self, view: u8, _: usize, _: usize, _: &[f32]) -> Result<egui::TextureId, String> {
            Ok(egui::TextureId::User(700 + u64::from(view)))
        }
    }
    let (mut h, work) = detail([640, 480], 1.0);
    h.app.services.hdr_output = Some(Box::new(Hdr));
    h.app.hdr_headroom = Some(4.0);
    h.app.session.execute("develop.hdr", &json!({"enabled": true})).unwrap();
    h.app.session.execute("develop.set", &json!({"values": {"light.exposure": 3.0}})).unwrap();
    h.app.session.begin_interaction("Exposure").unwrap();
    let job = h.app.session.render_job(PhotoId(1), 320, 240, false, true).unwrap();
    assert!(crate::panels::detail::request_hdr(&mut h.app, crate::render::HDR_LOUPE, job, 98).is_some());
    let live = latest(&work, Slot::HdrView(crate::render::HDR_LOUPE));
    assert_eq!(live.request.quality, Quality::Full);
    assert_eq!(live.request.depth, lcc_pipeline::OutputDepth::F32Hdr);
    let deep = live.run().rendered.unwrap().deep.expect("HDR float pixels");
    let lcc_pipeline::DeepSamples::F32(rgb) = deep.samples else { panic!("linear HDR float output") };
    assert!(rgb.iter().copied().any(|v| v > 1.0));
}

#[test]
fn high_quality_adjustments_can_switch_during_a_drag_and_ignore_slow_verdicts() {
    let (mut h, work) = detail([1024, 768], 1.0);
    h.app.drag_scale.remember_slow(PhotoId(1), "Exposure");
    h.app.session.begin_interaction("Exposure").unwrap();
    for high in [false, true, false, true] {
        let reply = h.request("ui.set", json!({"settings": {"highQualityAdjustmentPreviews": high}}), TIMEOUT);
        assert_eq!(reply["ok"], true, "{reply}");
        assert!(h.settle(TIMEOUT));
        let job = latest(&work, Slot::Main);
        assert_eq!(job.request.quality, if high { Quality::Full } else { Quality::Draft });
        assert_eq!(job.display_fit.is_some(), high);
        assert!(h.app.session.interaction.is_some(), "changing quality must not end the drag");
    }
}

#[test]
fn high_quality_adjustments_keep_both_before_after_views_at_settled_quality() {
    let (mut h, work) = detail([1024, 768], 1.25);
    for mode in [BeforeAfter::SideBySide, BeforeAfter::TopBottom, BeforeAfter::Split, BeforeAfter::SplitTopBottom] {
        h.app.ui.before_after = mode;
        assert!(h.settle(TIMEOUT));
        let before = pixels(&h, Slot::Before);
        let after = pixels(&h, Slot::Main);
        h.app.session.begin_interaction("Exposure").unwrap();
        assert!(h.settle(TIMEOUT));
        for slot in [Slot::Main, Slot::Before] {
            let job = latest(&work, slot);
            assert_eq!(job.request.quality, Quality::Full);
            assert!(job.display_fit.is_some());
        }
        assert_eq!(pixels(&h, Slot::Before).pixels, before.pixels);
        assert_eq!(pixels(&h, Slot::Main).pixels, after.pixels);
        h.app.session.end_interaction().unwrap();
    }
}

#[test]
fn high_quality_adjustments_use_one_native_window_per_pane_instead_of_tiles() {
    let (mut h, work) = detail([1600, 1200], 1.0);
    h.app.ui.zoom = Zoom::Percent(100.0);
    h.app.ui.before_after = BeforeAfter::SideBySide;
    assert!(h.settle(TIMEOUT));
    work.borrow_mut().asked.clear();
    h.app.session.begin_interaction("Exposure").unwrap();
    h.app.session.execute("develop.set", &json!({"values": {"light.exposure": 0.5}})).unwrap();
    assert!(h.settle(TIMEOUT));
    assert_eq!(h.app.renderer.tiles_pending(), 0);
    assert!(work.borrow().asked.iter().all(|(s, _)| !matches!(s, Slot::Tile { .. })));
    for pane in [PANE_AFTER, PANE_BEFORE] {
        let job = latest(&work, Slot::Window(pane));
        assert_eq!(job.request.quality, Quality::Full);
        let window = job.request.window.expect("visible window with filter context");
        assert_eq!(job.request.max_w.max(job.request.max_h), 1600);
        assert!(window.w * window.h < 1600 * 1200, "not the entire original");
        assert!(job.stages.is_some(), "interactive window reuses its stage cache");
    }
}
