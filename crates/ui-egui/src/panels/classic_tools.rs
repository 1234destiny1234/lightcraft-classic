//! Develop's tool strip as Lightroom Classic has it (fork-owned): a row of the editing tools under
//! the histogram (Crop, Remove, Red Eye, Masking, in the user's order), each opening its drawer
//! in the Develop panels' place: while a tool is in use the panel holds only its own controls, so
//! the photo's global sliders don't crowd it (#618). A click on the tool in use closes its drawer,
//! and the panels come back, as Return and the drawer's Close do. A child module of `right`, so it
//! uses that file's helpers. The row starts with an Edit button that shows the Develop panels
//! (closing any drawer), and a dot under a tool tells that it has been applied to the photo (a crop,
//! spots, masks, red-eye fixes; #617).

use egui::vec2;
use serde_json::json;

use crate::LccApp;
use crate::icons::Icon;
use crate::module::{ModuleId, PanelId};
use crate::state::RightPanel;
use crate::theme::Tokens;
use crate::widgets::{divider, icon_button_named};

/// Height of the row.
const ROW_H: f32 = 34.0;
/// Width of one tool's button.
const BUTTON_W: f32 = 40.0;

/// Whether `right` shows Develop's histogram, tool row and panels (the Edit panel, a tool's drawer
/// over it, or the profile browser in its place).
pub(super) fn develop_tools(app: &LccApp) -> bool {
    app.ui.module == ModuleId::Develop && app.ui.right.is_edit_tool()
}

/// The row of tools under the histogram, centred, with a rule under it.
pub(super) fn row(app: &mut LccApp, ui: &mut egui::Ui) {
    let has_photo = app.session.active().is_some();
    let tools: Vec<PanelId> = crate::panels::strip::strip_panels(app).into_iter().filter(|p| *p != PanelId::Edit).collect();
    let develop = app.session.active().and_then(|id| app.session.develop_of(id));
    let width = ui.available_width();
    ui.allocate_ui_with_layout(vec2(width, ROW_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_space(((width - (tools.len() + 1) as f32 * (BUTTON_W + 4.0)) / 2.0).max(0.0));
        // Edit: the Develop panels, with no tool's drawer over them
        let edit_on = app.ui.right == RightPanel::Edit;
        let sc = crate::shortcuts::shortcut_of(&app.ui.settings.keymap, "panel.edit");
        let tip = crate::widgets::command_tooltip(ui, "Edit", sc);
        let resp = icon_button_named(ui, "toolsEdit", Icon::Sliders, vec2(BUTTON_W, ROW_H - 4.0), edit_on, has_photo, crate::i18n::tr("Edit"), &tip);
        if resp.clicked() && !edit_on {
            let _ = app.run("panel.edit", json!({}));
        }
        for p in &tools {
            let Some((id, icon, panel, tip, needs_photo)) = crate::panels::strip::entry(*p) else { continue };
            let on = app.ui.right == panel;
            let video = crate::video_ui::blocked(app, &format!("panel.{id}"));
            let sc = crate::shortcuts::shortcut_of(&app.ui.settings.keymap, &format!("panel.{id}"));
            let tip = if video { crate::i18n::tr(crate::video_ui::NOT_FOR_VIDEO).to_string() } else { crate::widgets::command_tooltip(ui, tip, sc) };
            let name = crate::i18n::tr(entry_name(*p));
            let resp = icon_button_named(ui, id, icon, vec2(BUTTON_W, ROW_H - 4.0), on, (has_photo || !needs_photo) && !video, name, &tip);
            if develop.as_ref().is_some_and(|d| applied(d, panel)) {
                // the tool has been applied to this photo: a dot under its icon
                let t = Tokens::get(ui.ctx());
                ui.painter().circle_filled(egui::pos2(resp.rect.center().x, resp.rect.bottom() - 1.0), 2.0, t.pick);
            }
            if resp.clicked() {
                // the tool in use: its drawer closes, back to the panels
                let cmd = if on { "panel.edit".to_string() } else { format!("panel.{id}") };
                let _ = app.run(&cmd, json!({}));
            }
            // the strip's header menu: show/hide, solo, hiding mode
            crate::panels::classic::side_menu(app, &resp, crate::panels::strip::strip_declared(ModuleId::Develop));
        }
    });
    divider(ui);
}

/// Whether a tool has left something on the photo's settings.
fn applied(d: &lcc_develop::DevelopSettings, tool: RightPanel) -> bool {
    match tool {
        RightPanel::Crop => d.crop != lcc_develop::Crop::default(),
        RightPanel::Remove => !d.spots.is_empty(),
        RightPanel::Masking => !d.masks.is_empty(),
        RightPanel::RedEye => !d.red_eye.is_empty(),
        _ => false,
    }
}

/// A tool's name for screen readers (its tooltip carries the shortcut).
fn entry_name(p: PanelId) -> &'static str {
    crate::panels::strip::entry(p).map_or("", |e| e.3)
}
