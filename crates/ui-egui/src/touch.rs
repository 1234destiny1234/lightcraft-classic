//! The touch layout (fork-owned): on Android by default, elsewhere through `view.touchLayout`.
//!
//! Everything is hidden until it is wanted. The photo fills the screen; a tap on it shows or hides
//! the chrome (a slim top bar and a bottom tool strip, both floating over the photo). Picking a
//! slider tool (Light, Color, Effects, Detail, Optics) enters single-tool adjust mode: the photo
//! stays whole, a row of the tool's parameters runs along the bottom, and a sideways drag anywhere
//! on the photo changes the chosen one (an up/down drag picks the next or previous). Crop, Masks,
//! Remove and Presets open their panels. The full Edit panel is one button away: on the right side
//! in landscape, as a bottom sheet in portrait. Pressing and holding the photo (or the Before
//! button) shows the original. Library is the grid under a slim bar; a tap opens a photo.
//!
//! Own design (interaction patterns of touch photo editors; no assets taken from any). The desktop
//! layout is untouched: `LccApp::ui` calls in here only while [`TouchUi::enabled`] is set.

use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use lcc_develop::{ControlSpec, controls};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::LccApp;
use crate::icons::{Icon, paint};
use crate::module::{self, ModuleId};
use crate::state::{BeforeAfter, RightPanel, ViewMode};
use crate::theme::Tokens;
use crate::widgets::register;

/// The touch layout's state, saved with the UI state (`touch` in `ui.json` and `ui.set`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TouchUi {
    /// The touch layout is on (Android's default).
    pub enabled: bool,
    /// Develop's top bar and tool strip are shown (a tap on the photo toggles them).
    pub chrome: bool,
    /// The side panel (landscape) or bottom sheet (portrait) is open.
    pub panel: bool,
    /// The filmstrip shows under the photo while the chrome does.
    pub filmstrip: bool,
    /// Library's filter bar shows over the grid.
    pub filters: bool,
    /// The slider tool being adjusted (single-tool mode), if any.
    pub tool: Option<Tool>,
    /// The tool's parameter a drag changes (a control id).
    pub control: String,
    /// The tool's values when it was opened, which Cancel puts back.
    #[serde(skip)]
    pub snapshot: Vec<(String, f64)>,
    #[serde(skip)]
    drag: Option<Drag>,
    /// The original is showing because the photo (or Before) is held.
    #[serde(skip)]
    holding: bool,
}

// (derivable only where `cfg!(target_os = "android")` is false)
#[allow(clippy::derivable_impls)]
impl Default for TouchUi {
    fn default() -> Self {
        TouchUi {
            enabled: cfg!(target_os = "android"),
            chrome: false,
            panel: false,
            filmstrip: false,
            filters: false,
            tool: None,
            control: String::new(),
            snapshot: Vec::new(),
            drag: None,
            holding: false,
        }
    }
}

/// A drag on the photo in adjust mode.
#[derive(Clone, Debug, PartialEq)]
struct Drag {
    /// The value under the finger when the drag decided it was sideways, as a track position (0…1).
    start: f64,
    /// Sideways (adjusting) or up/down (choosing a parameter); undecided until it has moved.
    axis: Option<Axis>,
    /// Parameters stepped through so far by an up/down drag.
    stepped: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Axis {
    Across,
    Along,
}

/// The single-slider tools, each a short list of controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    Light,
    Color,
    Effects,
    Detail,
    Optics,
}

impl Tool {
    pub const ALL: [Tool; 5] = [Tool::Light, Tool::Color, Tool::Effects, Tool::Detail, Tool::Optics];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Light => "Light",
            Tool::Color => "Color",
            Tool::Effects => "Effects",
            Tool::Detail => "Detail",
            Tool::Optics => "Optics",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Tool::Light => Icon::Sun,
            Tool::Color => Icon::Palette,
            Tool::Effects => Icon::Sliders,
            Tool::Detail => Icon::Target,
            Tool::Optics => Icon::Circle,
        }
    }

    pub fn parse(s: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.label().eq_ignore_ascii_case(s))
    }

    /// The controls, in the order the parameter row shows them.
    fn control_ids(self) -> &'static [&'static str] {
        match self {
            Tool::Light => &["light.exposure", "light.contrast", "light.highlights", "light.shadows", "light.whites", "light.blacks"],
            Tool::Color => &["wb.temp", "wb.tint", "color.vibrance", "color.saturation"],
            Tool::Effects => &["effects.texture", "effects.clarity", "effects.dehaze", "vignette.amount", "grain.amount"],
            Tool::Detail => &[
                "detail.sharpenAmount",
                "detail.sharpenRadius",
                "detail.sharpenDetail",
                "detail.sharpenMasking",
                "detail.nrLuminance",
                "detail.nrColor",
            ],
            Tool::Optics => &["optics.distortion", "optics.vignetting", "optics.caRed", "optics.caBlue"],
        }
    }
}

/// The tool strip's buttons, in order.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Strip {
    Auto,
    Presets,
    Slider(Tool),
    Crop,
    Masks,
    Remove,
}

const STRIP: [Strip; 10] = [
    Strip::Auto,
    Strip::Presets,
    Strip::Slider(Tool::Light),
    Strip::Slider(Tool::Color),
    Strip::Slider(Tool::Effects),
    Strip::Slider(Tool::Detail),
    Strip::Slider(Tool::Optics),
    Strip::Crop,
    Strip::Masks,
    Strip::Remove,
];

impl Strip {
    fn label(self) -> &'static str {
        match self {
            Strip::Auto => "Auto",
            Strip::Presets => "Presets",
            Strip::Slider(t) => t.label(),
            Strip::Crop => "Crop",
            Strip::Masks => "Masks",
            Strip::Remove => "Remove",
        }
    }
    /// `None`: Auto, drawn as a lettered badge.
    fn icon(self) -> Option<Icon> {
        Some(match self {
            Strip::Auto => return None,
            Strip::Presets => Icon::Presets,
            Strip::Slider(t) => t.icon(),
            Strip::Crop => Icon::Crop,
            Strip::Masks => Icon::Mask,
            Strip::Remove => Icon::Eraser,
        })
    }
    fn id(self) -> String {
        format!("touch:tool:{}", self.label().to_ascii_lowercase())
    }
}

/// The shape of the screen, which decides where panels go and how big the chrome is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    TabletLandscape,
    TabletPortrait,
    PhoneLandscape,
    PhonePortrait,
}

impl Form {
    /// From the window's size in points: a short side under 600 points is a phone.
    pub fn of(size: Vec2) -> Form {
        let phone = size.x.min(size.y) < 600.0;
        match (phone, size.x >= size.y) {
            (false, true) => Form::TabletLandscape,
            (false, false) => Form::TabletPortrait,
            (true, true) => Form::PhoneLandscape,
            (true, false) => Form::PhonePortrait,
        }
    }
    /// Panels open beside the photo (landscape) rather than under it.
    pub fn side_panel(self) -> bool {
        matches!(self, Form::TabletLandscape | Form::PhoneLandscape)
    }
    fn phone(self) -> bool {
        matches!(self, Form::PhoneLandscape | Form::PhonePortrait)
    }
}

/// Bars and buttons: finger-sized.
const BAR_H: f32 = 52.0;
const BUTTON: f32 = 44.0;
const STRIP_H: f32 = 68.0;
/// How long a press on the photo waits before showing the original (s).
const HOLD_SECS: f64 = 0.45;
/// How far a finger may move and still be a tap or a hold (points).
const SLOP: f32 = 10.0;
/// An up/down drag of this many points steps to the next parameter.
const STEP_PTS: f32 = 56.0;

/// The touch layout is on.
pub(crate) fn on(app: &LccApp) -> bool {
    app.ui.touch.enabled
}

/// The touch layout's edges (replacing `shell::edges`): Library's bar, or Develop's side panel or
/// bottom sheet when it is open. Returns the module, which draws the centre.
pub(crate) fn edges(app: &mut LccApp, ui: &mut egui::Ui, ctx: &egui::Context) -> &'static dyn module::Module {
    module::sync(app);
    let m = module::get(app.ui.module);
    let form = Form::of(ctx.content_rect().size());
    if app.ui.module == ModuleId::Library {
        library_bar(app, ui, form);
    } else if app.ui.touch.panel && app.ui.touch.tool.is_none() {
        panel(app, ui, form);
    }
    m
}

/// The centre: the module's view, then (Develop) the photo's gestures and the floating chrome.
pub(crate) fn center(app: &mut LccApp, ui: &mut egui::Ui, m: &'static dyn module::Module) {
    let area = ui.max_rect();
    let form = Form::of(ui.ctx().content_rect().size());
    if app.ui.module == ModuleId::Library {
        let before = app.session.active();
        // the grid alone: its filmstrip stays away, its filter bar shows when asked for
        let (film, filters) = (app.ui.filmstrip, app.ui.filter_bar);
        app.ui.filmstrip = false;
        app.ui.filter_bar = app.ui.touch.filters;
        m.center(ui, app);
        app.ui.filmstrip = film;
        if app.ui.filter_bar == app.ui.touch.filters {
            app.ui.filter_bar = filters;
        }
        open_tapped(app, ui, area, before);
        return;
    }
    // the filmstrip shows only with the chrome (and when asked for); the saved setting is kept
    let film = app.ui.filmstrip;
    app.ui.filmstrip = app.ui.touch.filmstrip && app.ui.touch.chrome && app.ui.touch.tool.is_none();
    m.center(ui, app);
    app.ui.filmstrip = film;
    let canvas = app.canvas_rect.unwrap_or(area).intersect(area);
    if app.session.active().is_none() {
        // nothing to edit: the empty view keeps its own buttons; only the way back shows
        top_bar(app, ui, area, form);
        return;
    }
    gestures(app, ui, canvas);
    if app.ui.touch.tool.is_some() {
        adjust_bar(app, ui, area, form);
    } else if app.ui.touch.chrome {
        top_bar(app, ui, area, form);
        tool_strip(app, ui, area, form);
    } else {
        hint(ui, area);
    }
}

// ------------------------------------------------------------------------ Library

fn library_bar(app: &mut LccApp, ui: &mut egui::Ui, form: Form) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("touch-library-bar").exact_size(BAR_H).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        // (the grid's own header under this bar names the source and counts its photos)
        ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, crate::i18n::tr("Library"), t.semibold(18.0), t.text);
        let mut x = r.right() - 8.0;
        let mut button = |app: &mut LccApp, icon: Icon, id: &str, label: &str, enabled: bool| -> bool {
            x -= BUTTON;
            let rect = Rect::from_min_size(pos2(x, r.center().y - BUTTON / 2.0), Vec2::splat(BUTTON));
            let _ = app;
            round_button(ui, rect, id, icon, label, false, enabled).clicked()
        };
        if button(app, Icon::Gear, "touch:settings", "Settings", true) {
            let _ = app.run("app.settings", json!({}));
        }
        let square = app.ui.view == ViewMode::SquareGrid;
        if button(app, if square { Icon::GridPhoto } else { Icon::GridSquare }, "touch:gridStyle", "Grid Style", true) {
            let _ = app.run(if square { "view.photoGrid" } else { "view.squareGrid" }, json!({}));
        }
        let can_edit = app.session.active().is_some();
        if button(app, Icon::Sliders, "touch:edit", "Edit", can_edit) {
            open_develop(app);
        }
        if button(app, Icon::Filter, "touch:filters", "Filter", true) {
            app.ui.touch.filters = !app.ui.touch.filters;
        }
        let menu_id = Id::new("touch-import-menu");
        if button(app, Icon::Plus, "touch:import", "Import", true) {
            let open = ui.ctx().data(|d| d.get_temp::<bool>(menu_id).unwrap_or(false));
            ui.ctx().data_mut(|d| d.insert_temp(menu_id, !open));
        } else if ui.ctx().data(|d| d.get_temp::<bool>(menu_id).unwrap_or(false)) {
            import_menu(app, ui, pos2(r.right() - 8.0, r.bottom() + 4.0), menu_id);
        }
        let _ = form;
    });
}

/// Import's choices: photos, a folder, and the camera roll where the host has one.
fn import_menu(app: &mut LccApp, ui: &egui::Ui, right_top: Pos2, menu_id: Id) {
    let t = Tokens::get(ui.ctx());
    let mut items: Vec<(&str, &str, &str)> =
        vec![("touch:import:photos", "Import Photos…", "file.addPhotos"), ("touch:import:folder", "Import Folder…", "file.addFolder")];
    if app.services.camera_roll.is_some() {
        items.push(("touch:import:cameraRoll", "Camera Roll", "file.addCameraRoll"));
    }
    let size = vec2(220.0, BUTTON * items.len() as f32 + 8.0);
    let min = pos2(right_top.x - size.x, right_top.y);
    let mut chosen = None;
    egui::Area::new(Id::new("touch-import-area")).fixed_pos(min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (bg, _) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().rect_filled(bg, 10.0, t.chrome);
        ui.painter().rect_stroke(bg, 10.0, Stroke::new(1.0, t.divider), egui::StrokeKind::Inside);
        for (i, (id, label, command)) in items.iter().enumerate() {
            let row = Rect::from_min_size(pos2(bg.left(), bg.top() + 4.0 + BUTTON * i as f32), vec2(size.x, BUTTON));
            let resp = ui.interact(row, Id::new(*id), Sense::click());
            register(ui.ctx(), *id, row);
            if resp.is_pointer_button_down_on() {
                ui.painter().rect_filled(row.shrink(2.0), 6.0, t.pressed);
            }
            ui.painter().text(pos2(row.left() + 16.0, row.center().y), Align2::LEFT_CENTER, crate::i18n::tr(label), t.font(14.0), t.text);
            if resp.clicked() {
                chosen = Some(*command);
            }
        }
    });
    let tapped_away = ui.input(|i| i.pointer.any_click())
        && chosen.is_none()
        && !ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| Rect::from_min_size(min, size).contains(p));
    if let Some(command) = chosen {
        ui.ctx().data_mut(|d| d.insert_temp(menu_id, false));
        if let Err(e) = app.run(command, json!({})) {
            app.toast_error(ui.ctx(), e);
        }
    } else if tapped_away {
        ui.ctx().data_mut(|d| d.insert_temp(menu_id, false));
    }
}

/// A tap on a photo in the grid opens it in Develop (the grid's own click picks it first).
fn open_tapped(app: &mut LccApp, ui: &egui::Ui, area: Rect, before: Option<lcc_catalog::PhotoId>) {
    if !matches!(app.ui.view, ViewMode::PhotoGrid | ViewMode::SquareGrid) || ui.ctx().any_popup_open() {
        return;
    }
    let tapped = ui.input(|i| {
        i.pointer.primary_released()
            && i.pointer.interact_pos().is_some_and(|p| area.contains(p))
            && !i.pointer.is_decidedly_dragging()
            && i.pointer.button_clicked(egui::PointerButton::Primary)
            && !i.modifiers.any()
    });
    // a single photo picked by this tap (a tap on empty grid clears the selection instead);
    // `before` is kept for a future "tap the picked photo again" rule
    let _ = before;
    if tapped && app.session.active().is_some() && app.session.selection.ids.len() <= 1 {
        open_develop(app);
    }
}

fn open_develop(app: &mut LccApp) {
    let _ = app.run("module.develop", json!({}));
    if app.ui.view != ViewMode::Detail {
        app.ui.view = ViewMode::Detail;
    }
    app.ui.touch.chrome = true;
}

fn back_to_library(app: &mut LccApp) {
    close_tool(app, true);
    app.ui.touch.panel = false;
    let _ = app.run("module.library", json!({}));
    if !matches!(app.ui.view, ViewMode::PhotoGrid | ViewMode::SquareGrid) {
        app.ui.view = ViewMode::PhotoGrid;
    }
}

// ------------------------------------------------------------------------ Develop: panels

fn panel(app: &mut LccApp, ui: &mut egui::Ui, form: Form) {
    let t = Tokens::get(ui.ctx());
    let presets = app.ui.presets;
    let body = |app: &mut LccApp, ui: &mut egui::Ui| {
        if presets {
            crate::panels::presets::show(app, ui, module::DEVELOP_LEFT, false);
        } else {
            if app.ui.right == RightPanel::None {
                app.ui.right = RightPanel::Edit;
            }
            crate::panels::right::show(app, ui);
        }
    };
    let frame = egui::Frame::NONE.fill(t.chrome).stroke(Stroke::new(1.0, t.divider));
    let screen = ui.ctx().content_rect().size();
    if form.side_panel() {
        let w = if form.phone() { (screen.x * 0.45).clamp(260.0, 360.0) } else { 360.0f32.min(screen.x * 0.4) };
        egui::Panel::right("touch-panel").exact_size(w).resizable(false).frame(frame).show(ui, |ui| body(app, ui));
    } else {
        let h = (screen.y * 0.42).clamp(220.0, 560.0);
        egui::Panel::bottom("touch-sheet").exact_size(h).resizable(false).frame(frame).show(ui, |ui| {
            // the sheet's grabber: a tap on it closes the sheet
            let grab = Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), 18.0));
            let resp = ui.interact(grab, Id::new("touch-sheet-grab"), Sense::click());
            register(ui.ctx(), "touch:sheetGrab", grab);
            ui.painter().rect_filled(Rect::from_center_size(grab.center(), vec2(40.0, 4.0)), 2.0, t.text_dim);
            if resp.clicked() {
                app.ui.touch.panel = false;
            }
            ui.add_space(18.0);
            body(app, ui);
        });
    }
}

// ------------------------------------------------------------------------ Develop: the photo

/// Tap: show or hide the chrome. Press and hold: the original while held. In adjust mode a
/// sideways drag changes the parameter and an up/down drag picks another.
fn gestures(app: &mut LccApp, ui: &mut egui::Ui, canvas: Rect) {
    let adjusting = app.ui.touch.tool.is_some();
    let sense = if adjusting { Sense::click_and_drag() } else { Sense::click() };
    let resp = ui.interact(canvas, Id::new("touch-canvas"), sense);
    register(ui.ctx(), "touch:canvas", canvas);
    // hold to see the original
    let (held_for, moved) = ui.input(|i| {
        let held = i.pointer.press_start_time().map(|s| i.time - s).unwrap_or(0.0);
        let moved = match (i.pointer.press_origin(), i.pointer.interact_pos()) {
            (Some(a), Some(b)) => a.distance(b),
            _ => 0.0,
        };
        (held, moved)
    });
    let down = resp.is_pointer_button_down_on();
    if down && !app.ui.touch.holding && held_for >= HOLD_SECS && moved < SLOP && app.ui.touch.drag.is_none() {
        app.ui.touch.holding = true;
        if app.ui.before_after == BeforeAfter::Off {
            app.ui.before_after = BeforeAfter::Original;
        }
    }
    if down && !app.ui.touch.holding {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    let was_holding = app.ui.touch.holding;
    if !down && was_holding {
        stop_holding(app);
    }
    if adjusting {
        adjust_drag(app, ui, &resp, canvas);
    }
    if resp.clicked() && !was_holding && !adjusting {
        app.ui.touch.chrome = !app.ui.touch.chrome;
    }
}

fn stop_holding(app: &mut LccApp) {
    app.ui.touch.holding = false;
    if app.ui.before_after == BeforeAfter::Original {
        app.ui.before_after = BeforeAfter::Off;
    }
}

fn adjust_drag(app: &mut LccApp, ui: &egui::Ui, resp: &egui::Response, canvas: Rect) {
    let Some(tool) = app.ui.touch.tool else { return };
    if resp.drag_started() {
        app.ui.touch.drag = Some(Drag { start: 0.0, axis: None, stepped: 0 });
    }
    if resp.dragged()
        && let Some(drag) = app.ui.touch.drag.clone()
    {
        let total = ui.input(|i| match (i.pointer.press_origin(), i.pointer.interact_pos()) {
            (Some(a), Some(b)) => b - a,
            _ => Vec2::ZERO,
        });
        let mut drag = drag;
        if drag.axis.is_none() && total.length() >= SLOP {
            let across = total.x.abs() >= total.y.abs();
            drag.axis = Some(if across { Axis::Across } else { Axis::Along });
            if across && let Some((spec, v)) = current(app, tool) {
                drag.start = spec.position(v);
                let _ = app.run("develop.beginInteraction", json!({"label": spec.label}));
                app.ui.dragging_control = Some(spec.id.to_string());
            }
        }
        match drag.axis {
            Some(Axis::Across) => {
                if let Some((spec, _)) = current(app, tool) {
                    // the photo's width (at least 300 points) spans the whole track
                    let p = drag.start + f64::from(total.x / canvas.width().max(300.0));
                    let v = snap(spec, spec.value_at(p));
                    let _ = app.run("develop.set", json!({"control": spec.id, "value": v}));
                }
            }
            Some(Axis::Along) => {
                let steps = (total.y / STEP_PTS).trunc() as i32;
                if steps != drag.stepped {
                    step_control(app, tool, steps - drag.stepped);
                    drag.stepped = steps;
                }
            }
            None => {}
        }
        app.ui.touch.drag = Some(drag);
    }
    if resp.drag_stopped() {
        if app.ui.touch.drag.as_ref().is_some_and(|d| d.axis == Some(Axis::Across)) {
            let _ = app.run("develop.endInteraction", json!({}));
            app.ui.dragging_control = None;
        }
        app.ui.touch.drag = None;
    }
}

fn snap(spec: &ControlSpec, v: f64) -> f64 {
    let v = if spec.step > 0.0 && spec.id != "wb.temp" { (v / spec.step).round() * spec.step } else { v.round() };
    spec.clamp(v)
}

/// The tool's controls this photo has (white balance in Kelvin only for raw files).
fn tool_specs(app: &LccApp, tool: Tool) -> Vec<&'static ControlSpec> {
    let kelvin = app.session.active().and_then(|id| app.session.catalog.photo(id)).is_some_and(|p| p.develops_raw() && !p.relative_wb());
    tool.control_ids().iter().filter(|id| kelvin || !id.starts_with("wb.")).filter_map(|id| controls::find(id)).collect()
}

/// A control's value on the active photo (the white balance in effect, as the Edit panel shows it).
fn value_of(app: &LccApp, spec: &ControlSpec) -> Option<f64> {
    let id = app.session.active()?;
    let d = app.session.develop_of(id).unwrap_or_default();
    if spec.id.starts_with("wb.") {
        let info = app.session.source_info(id);
        let (temp, tint) = lcc_engine::pipeline::local::effective_wb(&info, &d);
        return Some(if spec.id == "wb.temp" { temp } else { tint });
    }
    Some(controls::get(&d, spec.id).unwrap_or(spec.default))
}

/// The parameter a drag changes, and its value.
fn current(app: &LccApp, tool: Tool) -> Option<(&'static ControlSpec, f64)> {
    let specs = tool_specs(app, tool);
    let spec = specs.iter().find(|s| s.id == app.ui.touch.control).or(specs.first()).copied()?;
    Some((spec, value_of(app, spec)?))
}

fn step_control(app: &mut LccApp, tool: Tool, by: i32) {
    let specs = tool_specs(app, tool);
    if specs.is_empty() {
        return;
    }
    let at = specs.iter().position(|s| s.id == app.ui.touch.control).unwrap_or(0) as i32;
    let n = specs.len() as i32;
    let next = (at + by).rem_euclid(n) as usize;
    if let Some(s) = specs.get(next) {
        app.ui.touch.control = s.id.to_string();
    }
}

/// Opens a slider tool: remembers its values for Cancel and picks its first parameter.
pub(crate) fn open_tool(app: &mut LccApp, tool: Tool) {
    let specs = tool_specs(app, tool);
    app.ui.touch.snapshot = specs.iter().filter_map(|s| Some((s.id.to_string(), value_of(app, s)?))).collect();
    app.ui.touch.control = specs.first().map(|s| s.id.to_string()).unwrap_or_default();
    app.ui.touch.tool = Some(tool);
    app.ui.touch.panel = false;
    app.ui.touch.drag = None;
}

/// Leaves adjust mode; `cancel` puts the values back as they were when the tool opened.
pub(crate) fn close_tool(app: &mut LccApp, keep: bool) {
    if app.ui.touch.tool.take().is_none() {
        return;
    }
    if app.ui.touch.drag.take().is_some_and(|d| d.axis == Some(Axis::Across)) {
        let _ = app.run("develop.endInteraction", json!({}));
        app.ui.dragging_control = None;
    }
    let snapshot = std::mem::take(&mut app.ui.touch.snapshot);
    if !keep && !snapshot.is_empty() {
        let changed: serde_json::Map<String, Value> = snapshot
            .into_iter()
            .filter(|(id, v)| controls::find(id).and_then(|s| value_of(app, s)).is_some_and(|now| (now - v).abs() > 1e-9))
            .map(|(id, v)| (id, json!(v)))
            .collect();
        if !changed.is_empty() {
            let _ = app.run("develop.set", json!({"values": changed}));
        }
    }
}

// ------------------------------------------------------------------------ Develop: chrome

/// The first look at Develop: a faint pill saying that a tap brings the tools.
fn hint(ui: &mut egui::Ui, area: Rect) {
    let t = Tokens::get(ui.ctx());
    let r = Rect::from_center_size(pos2(area.center().x, area.bottom() - 22.0), vec2(36.0, 5.0));
    ui.painter().rect_filled(r, 2.5, t.text_dim.gamma_multiply(0.6));
}

fn top_bar(app: &mut LccApp, ui: &mut egui::Ui, area: Rect, form: Form) {
    let t = Tokens::get(ui.ctx());
    let bar = Rect::from_min_size(area.min, vec2(area.width(), BAR_H));
    egui::Area::new(Id::new("touch-top")).fixed_pos(bar.min).order(egui::Order::Middle).show(ui.ctx(), |ui| {
        let (r, _) = ui.allocate_exact_size(bar.size(), Sense::hover());
        ui.painter().rect_filled(r, 0.0, t.chrome.gamma_multiply(0.88));
        let y = r.center().y - BUTTON / 2.0;
        let at = |x: f32| Rect::from_min_size(pos2(x, y), Vec2::splat(BUTTON));
        if round_button(ui, at(r.left() + 6.0), "touch:back", Icon::ChevronLeft, "Library", false, true).clicked() {
            back_to_library(app);
            return;
        }
        if !form.phone()
            && let Some(name) = app.session.active().and_then(|id| app.session.catalog.photo(id)).map(|p| p.file_name.clone())
        {
            ui.painter().text(pos2(r.left() + 6.0 + BUTTON + 6.0, r.center().y), Align2::LEFT_CENTER, name, t.font(13.0), t.text_label);
        }
        let mut x = r.right() - 6.0;
        let mut next = || {
            x -= BUTTON + 2.0;
            at(x)
        };
        if round_button(ui, next(), "touch:share", Icon::Share, "Export…", false, true).clicked() {
            let _ = app.run("dialog.export", json!({}));
        }
        let panel_open = app.ui.touch.panel;
        if round_button(ui, next(), "touch:panel", Icon::SidebarRight, "All Edits", panel_open, true).clicked() {
            app.ui.touch.panel = !panel_open;
            if app.ui.touch.panel {
                app.ui.presets = false;
                if !app.ui.right.is_edit_tool() {
                    app.ui.right = RightPanel::Edit;
                }
            }
        }
        if round_button(ui, next(), "touch:filmstrip", Icon::Filmstrip, "Filmstrip", app.ui.touch.filmstrip, true).clicked() {
            app.ui.touch.filmstrip = !app.ui.touch.filmstrip;
        }
        // press and hold: the original
        let before = round_button(ui, next(), "touch:before", Icon::BeforeAfter, "Hold to See the Original", app.ui.touch.holding, true);
        if before.is_pointer_button_down_on() {
            if !app.ui.touch.holding {
                app.ui.touch.holding = true;
                if app.ui.before_after == BeforeAfter::Off {
                    app.ui.before_after = BeforeAfter::Original;
                }
            }
        } else if app.ui.touch.holding && !ui.input(|i| i.pointer.any_down()) {
            stop_holding(app);
        }
        let can_redo = !app.session.redo.is_empty();
        if round_button(ui, next(), "touch:redo", Icon::RotateRight, "Redo", false, can_redo).clicked() {
            let _ = app.run("edit.redo", json!({}));
        }
        let can_undo = !app.session.undo.is_empty();
        if round_button(ui, next(), "touch:undo", Icon::RotateLeft, "Undo", false, can_undo).clicked() {
            let _ = app.run("edit.undo", json!({}));
        }
    });
}

fn tool_strip(app: &mut LccApp, ui: &mut egui::Ui, area: Rect, form: Form) {
    let t = Tokens::get(ui.ctx());
    let film = if app.ui.touch.filmstrip { t.film_h } else { 0.0 };
    let cell = if form.phone() { 64.0 } else { 76.0 };
    let full = cell * STRIP.len() as f32 + 16.0;
    let w = full.min(area.width());
    let strip = Rect::from_min_size(pos2(area.center().x - w / 2.0, area.bottom() - film - STRIP_H - 10.0), vec2(w, STRIP_H));
    egui::Area::new(Id::new("touch-strip")).fixed_pos(strip.min).order(egui::Order::Middle).show(ui.ctx(), |ui| {
        ui.set_width(w);
        let (bg, _) = ui.allocate_exact_size(strip.size(), Sense::hover());
        ui.painter().rect_filled(bg, 14.0, t.chrome.gamma_multiply(0.9));
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(bg.shrink2(vec2(8.0, 0.0))));
        egui::ScrollArea::horizontal()
            .id_salt("touch-strip-scroll")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut inner, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for s in STRIP {
                        let active = match s {
                            Strip::Presets => app.ui.touch.panel && app.ui.presets,
                            Strip::Crop => app.ui.touch.panel && app.ui.right == RightPanel::Crop,
                            Strip::Masks => app.ui.touch.panel && app.ui.right == RightPanel::Masking,
                            Strip::Remove => app.ui.touch.panel && app.ui.right == RightPanel::Remove,
                            _ => false,
                        };
                        if labelled_button(ui, vec2(cell, STRIP_H), &s.id(), s.icon(), s.label(), active).clicked() {
                            strip_tool(app, s, active);
                        }
                    }
                });
            });
    });
}

fn strip_tool(app: &mut LccApp, s: Strip, active: bool) {
    let open = |app: &mut LccApp, p: RightPanel| {
        app.ui.presets = false;
        app.ui.right = p;
        app.ui.touch.panel = true;
    };
    match s {
        Strip::Auto => {
            let _ = app.run("develop.auto", json!({}));
        }
        Strip::Slider(tool) => open_tool(app, tool),
        _ if active => app.ui.touch.panel = false,
        Strip::Presets => {
            app.ui.presets = true;
            app.ui.touch.panel = true;
        }
        Strip::Crop => open(app, RightPanel::Crop),
        Strip::Masks => open(app, RightPanel::Masking),
        Strip::Remove => {
            open(app, RightPanel::Remove);
            if app.ui.tool.is_empty() {
                app.ui.tool = "remove".into();
            }
        }
    }
}

/// Adjust mode: the value at the top, the tool's parameters and Cancel / Done along the bottom.
fn adjust_bar(app: &mut LccApp, ui: &mut egui::Ui, area: Rect, form: Form) {
    let Some(tool) = app.ui.touch.tool else { return };
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    // the value, over the top of the photo
    if let Some((spec, v)) = current(app, tool) {
        let text = format!("{}  {}", crate::i18n::tr(spec.label), spec.format(v));
        let galley = ctx.layer_painter(egui::LayerId::background()).layout_no_wrap(text, t.semibold(15.0), Color32::WHITE);
        let pill = Rect::from_center_size(pos2(area.center().x, area.top() + 34.0), galley.size() + vec2(32.0, 22.0));
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, Id::new("touch-value")));
        p.rect_filled(pill, pill.height() / 2.0, Color32::from_black_alpha(170));
        p.galley(pill.center() - galley.size() / 2.0 - vec2(0.0, 3.0), galley, Color32::WHITE);
        // where the value sits on its track
        let track = Rect::from_min_size(pos2(pill.left() + 16.0, pill.bottom() - 7.0), vec2(pill.width() - 32.0, 2.0));
        p.rect_filled(track, 1.0, Color32::from_white_alpha(60));
        let pos = spec.position(v) as f32;
        let x = track.left() + track.width() * pos;
        if spec.min < 0.0 {
            let mid = track.center().x;
            p.rect_filled(Rect::from_x_y_ranges(mid.min(x)..=mid.max(x), track.y_range()), 1.0, t.accent);
        } else {
            p.rect_filled(Rect::from_x_y_ranges(track.left()..=x, track.y_range()), 1.0, t.accent);
        }
        register(&ctx, "touch:value", pill);
    }
    let specs = tool_specs(app, tool);
    let h = STRIP_H + 8.0;
    let w = if form.phone() { area.width() } else { area.width().min(760.0) };
    let bar = Rect::from_min_size(pos2(area.center().x - w / 2.0, area.bottom() - h - if form.phone() { 0.0 } else { 10.0 }), vec2(w, h));
    egui::Area::new(Id::new("touch-adjust")).fixed_pos(bar.min).order(egui::Order::Middle).show(&ctx, |ui| {
        let (bg, _) = ui.allocate_exact_size(bar.size(), Sense::hover());
        ui.painter().rect_filled(bg, if form.phone() { 0.0 } else { 14.0 }, t.chrome.gamma_multiply(0.92));
        let y = bg.center().y - BUTTON / 2.0;
        if round_button(ui, Rect::from_min_size(pos2(bg.left() + 8.0, y), Vec2::splat(BUTTON)), "touch:cancel", Icon::Close, "Cancel", false, true)
            .clicked()
        {
            close_tool(app, false);
            return;
        }
        if round_button(
            ui,
            Rect::from_min_size(pos2(bg.right() - 8.0 - BUTTON, y), Vec2::splat(BUTTON)),
            "touch:done",
            Icon::Check,
            "Done",
            false,
            true,
        )
        .clicked()
        {
            close_tool(app, true);
            return;
        }
        let chips = Rect::from_min_max(pos2(bg.left() + 16.0 + BUTTON, bg.top()), pos2(bg.right() - 16.0 - BUTTON, bg.bottom()));
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(chips));
        egui::ScrollArea::horizontal().id_salt("touch-chips").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(
            &mut inner,
            |ui| {
                ui.set_min_height(chips.height());
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for spec in &specs {
                        let selected =
                            spec.id == app.ui.touch.control || (app.ui.touch.control.is_empty() && specs.first().is_some_and(|f| f.id == spec.id));
                        let changed = value_of(app, spec).is_some_and(|v| (v - spec.default).abs() > 1e-9 && !spec.id.starts_with("wb."));
                        let resp = chip(ui, spec, selected, changed);
                        if resp.double_clicked() {
                            // double tap: back to the default
                            if !spec.id.starts_with("wb.") {
                                let _ = app.run("develop.set", json!({"control": spec.id, "value": spec.default}));
                            }
                        } else if resp.clicked() {
                            app.ui.touch.control = spec.id.to_string();
                        }
                    }
                });
            },
        );
    });
}

// ------------------------------------------------------------------------ widgets

/// A round icon button (44 points), registered as `id` for the control channel.
fn round_button(ui: &mut egui::Ui, rect: Rect, id: &str, icon: Icon, label: &str, active: bool, enabled: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let name = crate::i18n::tr(label);
    let resp = ui.interact(rect, Id::new(id), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, active, name));
    register(ui.ctx(), id, rect);
    let p = ui.painter();
    if active {
        p.circle_filled(rect.center(), rect.width() / 2.0 - 2.0, t.tool_active);
    } else if resp.is_pointer_button_down_on() && enabled {
        p.circle_filled(rect.center(), rect.width() / 2.0 - 2.0, t.pressed);
    }
    let c = if !enabled {
        t.text_disabled
    } else if active {
        t.pick
    } else {
        t.text
    };
    paint(p, Rect::from_center_size(rect.center(), Vec2::splat(22.0)), icon, c);
    resp
}

/// A tool strip button: icon over its name.
fn labelled_button(ui: &mut egui::Ui, size: Vec2, id: &str, icon: Option<Icon>, label: &str, active: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let name = crate::i18n::tr(label);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, name));
    register(ui.ctx(), id, r);
    let p = ui.painter();
    if active || resp.is_pointer_button_down_on() {
        p.rect_filled(r.shrink(4.0), 10.0, if active { t.tool_active } else { t.pressed });
    }
    let c = if active { t.pick } else { t.text };
    let glyph = Rect::from_center_size(pos2(r.center().x, r.top() + 24.0), Vec2::splat(24.0));
    match icon {
        Some(icon) => paint(p, glyph, icon, c),
        None => {
            let badge = Rect::from_center_size(glyph.center(), vec2(30.0, 18.0));
            p.rect_stroke(badge, 4.0, Stroke::new(1.3, c), egui::StrokeKind::Inside);
            p.text(badge.center(), Align2::CENTER_CENTER, "A", t.semibold(12.0), c);
        }
    }
    p.text(pos2(r.center().x, r.bottom() - 15.0), Align2::CENTER_CENTER, name, t.font(11.5), if active { t.pick } else { t.text_label });
    resp
}

/// A parameter in adjust mode's row; a dot marks one changed from its default.
fn chip(ui: &mut egui::Ui, spec: &ControlSpec, selected: bool, changed: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let name = crate::i18n::tr(spec.label);
    let galley = ui.painter().layout_no_wrap(name.to_string(), t.font(13.0), t.text);
    let size = vec2(galley.size().x + 28.0, BUTTON);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, name));
    register(ui.ctx(), format!("touch:param:{}", spec.id), r);
    let p = ui.painter();
    let fill = if selected { t.accent } else { t.button };
    p.rect_filled(r, r.height() / 2.0, fill);
    let text = if selected { Color32::WHITE } else { t.text };
    p.galley_with_override_text_color(r.center() - galley.size() / 2.0, galley, text);
    if changed {
        p.circle_filled(Pos2::new(r.right() - 9.0, r.top() + 9.0), 3.0, if selected { Color32::WHITE } else { t.accent });
    }
    resp
}

/// The fork's touch commands (`menus_fork` lists them).
pub const COMMANDS: &[crate::menus::UiCommand] = &[
    ("view.touchLayout", "Touch Layout", None, "Window>Panels"),
    ("touch.chrome", "Show Touch Tools", None, ""),
    ("touch.panel", "Show All Edits", None, ""),
    ("touch.tool", "Adjust with One Tool", None, ""),
    ("touch.done", "Finish Adjusting", None, ""),
];

/// Runs a touch command; `None`: not one. `touch.tool {tool, control?}` opens a slider tool,
/// `touch.done {cancel?}` leaves it.
pub(crate) fn run(app: &mut LccApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let flag = |p: &Value, now: bool| p.get("on").and_then(Value::as_bool).unwrap_or(!now);
    Some(match id {
        "view.touchLayout" => {
            app.ui.touch.enabled = flag(p, app.ui.touch.enabled);
            if !app.ui.touch.enabled {
                close_tool(app, true);
                stop_holding(app);
            }
            Ok(json!({"on": app.ui.touch.enabled}))
        }
        "touch.chrome" => {
            app.ui.touch.chrome = flag(p, app.ui.touch.chrome);
            Ok(json!({"on": app.ui.touch.chrome}))
        }
        "touch.panel" => {
            app.ui.touch.panel = flag(p, app.ui.touch.panel);
            if app.ui.touch.panel {
                close_tool(app, true);
            }
            Ok(json!({"on": app.ui.touch.panel}))
        }
        "touch.tool" => {
            let Some(name) = p.get("tool").and_then(Value::as_str) else {
                return Some(Err("touch.tool: give `tool` (light, color, effects, detail or optics)".into()));
            };
            let Some(tool) = Tool::parse(name) else {
                return Some(Err(format!("touch.tool: unknown tool `{name}` (light, color, effects, detail or optics)")));
            };
            if app.session.active().is_none() {
                return Some(Err("touch.tool: no photo is active".into()));
            }
            close_tool(app, true);
            open_tool(app, tool);
            if let Some(c) = p.get("control").and_then(Value::as_str) {
                if !tool_specs(app, tool).iter().any(|s| s.id == c) {
                    return Some(Err(format!("touch.tool: `{c}` is not one of {}'s controls", tool.label())));
                }
                app.ui.touch.control = c.to_string();
            }
            Ok(json!({"tool": tool, "control": app.ui.touch.control}))
        }
        "touch.done" => {
            let cancel = p.get("cancel").and_then(Value::as_bool).unwrap_or(false);
            close_tool(app, !cancel);
            Ok(Value::Null)
        }
        _ => return None,
    })
}
