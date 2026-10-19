//! Tests verifying compact slider label and value layout in the Edit panel.

use super::{MAX_LABEL_COL, VALUE_W, slider, take_registry};
use crate::headless::HeadlessView;
use egui::vec2;
use lcc_develop::controls;

#[test]
fn slider_geometry_aligns_labels_flush_and_maximizes_track_width() {
    let mut view = HeadlessView::new();
    let panel_w = 270.0;
    let exposure_spec = controls::find("light.exposure").unwrap();
    let highlights_spec = controls::find("light.highlights").unwrap();
    let temp_spec = controls::find("wb.temp").unwrap();

    let mut time = 0.0;
    // Run 2 frames so label_column stabilizes across the frame
    for _ in 0..2 {
        time += 0.02;
        view.run(HeadlessView::raw_input(vec2(panel_w, 200.0), 1.0, time, vec![]), |ui| {
            slider(ui, exposure_spec, 0.0, true, None);
            slider(ui, highlights_spec, 0.0, true, None);
            slider(ui, temp_spec, 5500.0, true, None);
        });
        if time < 0.03 {
            take_registry(&view.ctx);
        }
    }

    let registry = take_registry(&view.ctx);
    let find = |name: &str| registry.iter().find(|(id, _)| id == name).map(|(_, r)| *r).unwrap();

    let exp_label = find("sliderLabel:light.exposure");
    let exp_track = find("slider:light.exposure");
    let exp_val = find("sliderValue:light.exposure");

    let hi_label = find("sliderLabel:light.highlights");
    let hi_track = find("slider:light.highlights");

    let temp_label = find("sliderLabel:wb.temp");
    let temp_track = find("slider:wb.temp");

    // 1. Labels are positioned flush with section headers at left offset 24.0
    assert_eq!(exp_label.left(), 24.0, "exposure label left flush with header margin");
    assert_eq!(hi_label.left(), 24.0, "highlights label left flush with header margin");
    assert_eq!(temp_label.left(), 24.0, "temp label left flush with header margin");

    // 2. Label column width is strictly capped to MAX_LABEL_COL (64.0)
    assert!(exp_label.width() <= MAX_LABEL_COL, "exposure label col <= {MAX_LABEL_COL}");
    assert!(hi_label.width() <= MAX_LABEL_COL, "highlights label col <= {MAX_LABEL_COL}");
    assert!(temp_label.width() <= MAX_LABEL_COL, "temp label col <= {MAX_LABEL_COL}");

    // 3. All slider tracks share the same left alignment in the section
    assert_eq!(exp_track.left(), hi_track.left(), "tracks align at same left edge");
    assert_eq!(exp_track.left(), temp_track.left(), "tracks align at same left edge");

    // 4. Value column width is tightened to VALUE_W (34.0)
    assert_eq!(exp_val.width(), VALUE_W, "value width is tightened to {VALUE_W}");
    assert_eq!(exp_val.right(), panel_w - 16.0, "value column ends with 16.0 pad_r");

    // 5. Track width occupies majority of available interior width
    assert!(exp_track.width() > 115.0, "track occupies major width ({}) in 270px panel", exp_track.width());
}
