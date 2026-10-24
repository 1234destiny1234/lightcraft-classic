//! The Modern layout (fork-owned): on Android by default, elsewhere through `view.modernLayout`.
//!
//! Everything is hidden until it is wanted. The photo fills the screen; a tap on it shows or hides
//! the chrome (a slim top bar and a bottom tool strip, both floating over the photo). Picking a
//! slider tool (Basic, Colour, Tone, Detail, Optics, Effects) enters single-tool adjust mode: the photo
//! stays whole, a row of the tool's parameters runs along the bottom, and a sideways drag anywhere
//! on the photo changes the chosen one (an up/down drag picks the next or previous). Crop, Masks,
//! Remove and Presets open their panels. The full Edit panel is one button away: on the right side
//! in landscape, as a bottom sheet in portrait. Pressing and holding the photo (or the Before
//! button) shows the original. Library is the grid under a slim bar; a tap opens a photo.
//!
//! Own design (interaction patterns of touch photo editors; no assets taken from any). The desktop
//! layout is untouched: `LccApp::ui` calls in here only while [`ModernUi::enabled`] is set.

use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use lcc_develop::{ControlSpec, controls};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::LccApp;
use crate::icons::{Icon, paint};
use crate::module::{self, ModuleId};
use crate::panels::edit;
use crate::state::{BeforeAfter, RightPanel, ViewMode};
use crate::theme::Tokens;
use crate::widgets::register;

/// The Modern layout's state, saved with the UI state (`modern` in `ui.json`, formerly `touch` and `ui.set`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModernUi {
    /// The Modern layout is on (Android's default).
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
    /// The histogram over the photo in Develop: off, pinned under the top bar, or floating.
    pub histogram: HistogramPlace,
    /// Where the floating histogram sits: its top-left corner as a fraction of the room it can
    /// move in (0…1 each way), so it stays put across rotations and screen sizes.
    pub histogram_at: [f32; 2],
    /// The screen's edges hidden by the display cutout and the system bars (points), from the host
    /// (Android) or `modern.insets`; the whole UI keeps clear of them.
    #[serde(skip)]
    pub insets: Insets,
    /// Library: a long press started picking several photos; taps add or remove.
    #[serde(skip)]
    pub selecting: bool,
    /// Library: Import's menu is open.
    #[serde(skip)]
    import_menu: bool,
    /// A popup (menu, dropdown) was open last frame.
    #[serde(skip)]
    popup_open: bool,
    /// Back had nothing to close: the host hands it to the system (Android puts the app away).
    #[serde(skip)]
    system_back: bool,
    /// When Back was last taken (s, input time): the key and the system's back callback can both
    /// report one press.
    #[serde(skip)]
    last_back: Option<f64>,
    /// The parameter chip and tool button last brought to the middle of their rows.
    #[serde(skip)]
    centred: (String, String),
    #[serde(skip)]
    scroller: crate::modern_input::Scroller,
    /// A long press on photos this frame, where.
    #[serde(skip)]
    long_press: Option<Pos2>,
}

/// Where Develop's histogram shows in the Modern layout (`modern.histogram {mode}`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistogramPlace {
    #[default]
    Off,
    /// Across the top, under the top bar (portrait's choice).
    Pinned,
    /// A small window the user drags anywhere (landscape's choice).
    Floating,
}

impl HistogramPlace {
    fn parse(s: &str) -> Option<HistogramPlace> {
        Some(match s.to_ascii_lowercase().as_str() {
            "off" => HistogramPlace::Off,
            "pinned" => HistogramPlace::Pinned,
            "floating" => HistogramPlace::Floating,
            _ => return None,
        })
    }
}

/// Screen edges to keep clear of (points).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    /// The most an edge may take (points): a bad report can't hide the whole UI.
    const MAX: f32 = 400.0;

    /// From the host's pixels (left, top, right, bottom) at `pixels_per_point`.
    pub fn from_px(px: [i32; 4], pixels_per_point: f32) -> Insets {
        let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 { pixels_per_point } else { 1.0 };
        let pt = |v: i32| (v.max(0) as f32 / ppp).min(Insets::MAX);
        Insets { left: pt(px[0]), top: pt(px[1]), right: pt(px[2]), bottom: pt(px[3]) }
    }

    fn clean(v: Option<f64>) -> f32 {
        v.filter(|v| v.is_finite()).map_or(0.0, |v| (v as f32).clamp(0.0, Insets::MAX))
    }
}

// (derivable only where `cfg!(target_os = "android")` is false)
#[allow(clippy::derivable_impls)]
impl Default for ModernUi {
    fn default() -> Self {
        ModernUi {
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
            histogram: HistogramPlace::Off,
            histogram_at: [1.0, 0.0],
            insets: Insets::default(),
            selecting: false,
            import_menu: false,
            popup_open: false,
            system_back: false,
            last_back: None,
            centred: Default::default(),
            scroller: Default::default(),
            long_press: None,
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
    /// White balance, then the Basic panel's tone sliders.
    Basic,
    /// The Basic panel's Presence: texture, clarity, dehaze, vibrance, saturation.
    Colour,
    /// The Tone Curve panel's parametric curve: its four regions and three split points.
    Tone,
    Detail,
    Optics,
    /// Post-crop vignette and grain.
    Effects,
}

impl Tool {
    pub const ALL: [Tool; 6] = [Tool::Basic, Tool::Colour, Tool::Tone, Tool::Detail, Tool::Optics, Tool::Effects];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Basic => "Basic",
            Tool::Colour => "Colour",
            Tool::Tone => "Tone",
            Tool::Detail => "Detail",
            Tool::Optics => "Optics",
            Tool::Effects => "Effects",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Tool::Basic => Icon::Sun,
            Tool::Colour => Icon::Palette,
            Tool::Tone => Icon::Sliders,
            Tool::Detail => Icon::Target,
            Tool::Optics => Icon::Circle,
            Tool::Effects => Icon::Sparkle,
        }
    }

    /// A tool by its label, any case; `light` and `color` (the groups' earlier names) still work.
    pub fn parse(s: &str) -> Option<Tool> {
        match s.to_ascii_lowercase().as_str() {
            "light" => Some(Tool::Basic),
            "color" => Some(Tool::Colour),
            _ => Tool::ALL.into_iter().find(|t| t.label().eq_ignore_ascii_case(s)),
        }
    }

    /// The controls, in the order the parameter row shows them.
    fn control_ids(self) -> &'static [&'static str] {
        match self {
            Tool::Basic => {
                &["wb.temp", "wb.tint", "light.exposure", "light.contrast", "light.highlights", "light.shadows", "light.whites", "light.blacks"]
            }
            Tool::Colour => &["effects.texture", "effects.clarity", "effects.dehaze", "color.vibrance", "color.saturation"],
            Tool::Tone => {
                &["curve.highlights", "curve.lights", "curve.darks", "curve.shadows", "curve.splitShadows", "curve.splitMid", "curve.splitHighlights"]
            }
            Tool::Detail => &[
                "detail.sharpenAmount",
                "detail.sharpenRadius",
                "detail.sharpenDetail",
                "detail.sharpenMasking",
                "detail.nrLuminance",
                "detail.nrColor",
            ],
            Tool::Optics => &["optics.distortion", "optics.vignetting", "optics.caRed", "optics.caBlue"],
            Tool::Effects => &["vignette.amount", "grain.amount"],
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

const STRIP: [Strip; 11] = [
    Strip::Auto,
    Strip::Presets,
    Strip::Slider(Tool::Basic),
    Strip::Slider(Tool::Colour),
    Strip::Slider(Tool::Tone),
    Strip::Slider(Tool::Detail),
    Strip::Slider(Tool::Optics),
    Strip::Slider(Tool::Effects),
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
        format!("modern:tool:{}", self.label().to_ascii_lowercase())
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

/// The Modern layout is on.
pub(crate) fn on(app: &LccApp) -> bool {
    app.ui.modern.enabled
}

// ------------------------------------------------------------------------ input: Back, insets, scrolling

/// Before the frame's input reaches egui: the screen's insets become egui's safe area (so every
/// panel, dialog and the touch chrome keep clear of the camera cutout and the system bars), and a
/// Back press (the Android key or gesture, as egui's `BrowserBack`) runs [`back`].
pub(crate) fn before_input(app: &mut LccApp, raw: &mut egui::RawInput) {
    let i = app.ui.modern.insets;
    raw.safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 { left: i.left, right: i.right, top: i.top, bottom: i.bottom }));
}

/// Back presses among the frame's input (the host's and the control channel's): [`back`].
fn back_keys(app: &mut LccApp, raw: &mut egui::RawInput) {
    let mut backs = 0;
    raw.events.retain(|e| match e {
        egui::Event::Key { key: egui::Key::BrowserBack, pressed, .. } => {
            backs += usize::from(*pressed);
            false
        }
        _ => true,
    });
    if backs == 0 {
        return;
    }
    // one press can arrive twice (the key, and the system's back callback): a second within
    // this long is the same press
    let now = raw.time.unwrap_or(0.0);
    if app.ui.modern.last_back.is_some_and(|t| (now - t).abs() < 0.25) {
        return;
    }
    app.ui.modern.last_back = Some(now);
    if back(app).is_none() {
        app.ui.modern.system_back = true;
    }
}

/// After the synthetic input is in: in the Modern layout, swipes scroll (see [`crate::modern_input`]).
pub(crate) fn after_input(app: &mut LccApp, raw: &mut egui::RawInput) {
    back_keys(app, raw);
    if !app.ui.modern.enabled {
        app.ui.modern.scroller = Default::default();
        return;
    }
    let widgets = std::mem::take(&mut app.widgets);
    let rect_of = |id: &str| widgets.iter().rev().find(|(w, _)| w == id).map(|(_, r)| *r);
    let modal = app.ui.dialog.is_some() || app.ui.modern.popup_open;
    let develop = app.ui.module != ModuleId::Library;
    let canvas = if develop { rect_of("modern:canvas").or(app.canvas_rect) } else { None };
    let photos = [rect_of("grid:scroll"), rect_of("filmstrip")];
    let strips = [rect_of("modern:strip"), rect_of("modern:chips")];
    let region = |p: Pos2| {
        use crate::modern_input::Region;
        if modal {
            return Some(Region::Free);
        }
        // the photo in Develop has gestures of its own (tap, hold, adjust, crop, brush)
        if canvas.is_some_and(|c| c.contains(p)) && !strips.iter().flatten().any(|r| r.contains(p)) && !photos[1].is_some_and(|r| r.contains(p)) {
            return None;
        }
        if strips.iter().flatten().any(|r| r.contains(p)) {
            Some(Region::Strip)
        } else if photos.iter().flatten().any(|r| r.contains(p)) {
            Some(Region::Photos)
        } else {
            Some(Region::Free)
        }
    };
    let toggle = app.ui.modern.selecting;
    let outcome = app.ui.modern.scroller.process(raw, region, toggle);
    app.widgets = widgets;
    if outcome.long_press.is_some() {
        app.ui.modern.long_press = outcome.long_press;
    }
    if outcome.busy
        && let Some(ctx) = app.tasks.repaint.as_ref()
    {
        ctx.request_repaint();
    }
}

/// Back: closes the innermost thing that is open and says what (`None`: nothing was, so the
/// system's Back applies: Android puts the app away). In order: a dialog, menu or popup (as Esc
/// would); picking several photos; the adjust tool (Done: the changes stay, as a touch editor's
/// Back keeps them; during a drag Back waits); the side panel or sheet; Develop (back to the
/// Library); the Library's filter bar.
pub(crate) fn back(app: &mut LccApp) -> Option<&'static str> {
    let esc = |app: &mut LccApp| {
        for pressed in [true, false] {
            app.synthetic.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
    };
    // the first-start camera roll question: Back is Not Now
    if crate::camera_roll::prompt_due(app) {
        app.ui.camera_roll_asked = true;
        return Some("prompt");
    }
    if crate::modern::on(app)
        && let Some(what) = crate::modern_sheets::back(app)
    {
        return Some(what);
    }
    if app.ui.dialog.is_some() {
        esc(app);
        return Some("dialog");
    }
    if app.ui.modern.import_menu {
        app.ui.modern.import_menu = false;
        return Some("menu");
    }
    if app.ui.modern.popup_open {
        esc(app);
        app.ui.modern.popup_open = false;
        return Some("menu");
    }
    if !app.ui.modern.enabled {
        return None;
    }
    if app.ui.modern.selecting && app.ui.module == ModuleId::Library {
        app.ui.modern.selecting = false;
        let keep: Vec<u64> = app.session.active().map(|id| id.0).into_iter().collect();
        let _ = app.run("library.select", json!({"ids": keep}));
        return Some("selection");
    }
    if app.ui.modern.tool.is_some() {
        if app.ui.modern.drag.as_ref().is_some_and(|d| d.axis.is_some()) {
            return Some("busy");
        }
        close_tool(app, true);
        return Some("tool");
    }
    if app.ui.modern.panel {
        app.ui.modern.panel = false;
        return Some("panel");
    }
    if app.ui.module != ModuleId::Library {
        back_to_library(app);
        return Some("develop");
    }
    if app.ui.modern.filters {
        app.ui.modern.filters = false;
        return Some("filters");
    }
    None
}

/// Whether Back had nothing left to close since the last call: the host then does the system's
/// Back (Android: the app goes to the background).
pub fn take_system_back(app: &mut LccApp) -> bool {
    std::mem::take(&mut app.ui.modern.system_back)
}

/// The Modern layout's edges (replacing `shell::edges`): Library's bar, or Develop's side panel or
/// bottom sheet when it is open. Returns the module, which draws the centre.
/// The top-left corner for floating cards (the activity stack) in the Modern layout: inside the
/// screen's insets and below the top bar, so they never cover its buttons (Back, Import, …).
pub(crate) fn overlay_origin(app: &LccApp) -> Pos2 {
    let i = app.ui.modern.insets;
    pos2(i.left + 8.0, i.top + BAR_H + 8.0)
}

pub(crate) fn edges(app: &mut LccApp, ui: &mut egui::Ui, ctx: &egui::Context) -> &'static dyn module::Module {
    module::sync(app);
    let m = module::get(app.ui.module);
    let form = Form::of(ctx.content_rect().size());
    app.ui.modern.popup_open = egui::Popup::is_any_open(ctx);
    // a tap's release (and a hold's timing, a glide) comes on a later frame: ask for it here, as
    // the host draws only on input otherwise (Android)
    if app.ui.modern.scroller.busy() {
        ctx.request_repaint();
    }
    // the cutout and the system bars: every panel below keeps clear of them
    let i = app.ui.modern.insets;
    let fill = egui::Frame::NONE.fill(Tokens::get(ctx).chrome);
    if i.top > 0.5 {
        egui::Panel::top("modern-inset-top").exact_size(i.top).resizable(false).frame(fill).show(ui, |_| {});
    }
    if i.bottom > 0.5 {
        egui::Panel::bottom("modern-inset-bottom").exact_size(i.bottom).resizable(false).frame(fill).show(ui, |_| {});
    }
    if i.left > 0.5 {
        egui::Panel::left("modern-inset-left").exact_size(i.left).resizable(false).frame(fill).show(ui, |_| {});
    }
    if i.right > 0.5 {
        egui::Panel::right("modern-inset-right").exact_size(i.right).resizable(false).frame(fill).show(ui, |_| {});
    }
    if app.ui.modern.insets != Insets::default() {
        // behind the cutout and the system bars: the chrome's colour, not whatever was there
        let t = Tokens::get(ctx);
        ctx.layer_painter(egui::LayerId::background()).rect_filled(ctx.viewport_rect(), 0.0, t.chrome);
    }
    if app.ui.module == ModuleId::Library {
        library_bar(app, ui, form);
    } else if app.ui.modern.panel && app.ui.modern.tool.is_none() {
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
        app.ui.filter_bar = app.ui.modern.filters;
        m.center(ui, app);
        app.ui.filmstrip = film;
        if app.ui.filter_bar == app.ui.modern.filters {
            app.ui.filter_bar = filters;
        }
        long_press(app);
        if app.ui.modern.selecting && app.session.selection.ids.is_empty() {
            app.ui.modern.selecting = false;
        }
        if !app.ui.modern.selecting {
            open_tapped(app, ui, area, before);
        }
        return;
    }
    // the filmstrip shows only with the chrome (and when asked for); the saved setting is kept
    let film = app.ui.filmstrip;
    app.ui.filmstrip = app.ui.modern.filmstrip && app.ui.modern.chrome && app.ui.modern.tool.is_none();
    m.center(ui, app);
    app.ui.filmstrip = film;
    let canvas = app.canvas_rect.unwrap_or(area).intersect(area);
    if app.session.active().is_none() {
        // nothing to edit: the empty view keeps its own buttons; only the way back shows
        top_bar(app, ui, area, form);
        return;
    }
    gestures(app, ui, canvas);
    let pinned = histogram(app, ui, area, form);
    if app.ui.modern.tool.is_some() {
        adjust_bar(app, ui, area, form, pinned);
    } else if app.ui.modern.chrome {
        top_bar(app, ui, area, form);
        tool_strip(app, ui, area, form);
    } else {
        hint(ui, area);
    }
}

// ------------------------------------------------------------------------ Library

fn library_bar(app: &mut LccApp, ui: &mut egui::Ui, form: Form) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("modern-library-bar").exact_size(BAR_H).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        // (the grid's own header under this bar names the source and counts its photos)
        if app.ui.modern.selecting {
            // picking several: how many, and the way out (Back does the same)
            let close = Rect::from_min_size(pos2(r.left() + 6.0, r.center().y - BUTTON / 2.0), Vec2::splat(BUTTON));
            if round_button(ui, close, "modern:endSelect", Icon::Close, "Done Selecting", false, true).clicked() {
                back(app);
            }
            let n = app.session.selection.ids.len();
            ui.painter().text(
                pos2(close.right() + 8.0, r.center().y),
                Align2::LEFT_CENTER,
                format!("{n} {}", crate::i18n::tr("Selected")),
                t.semibold(17.0),
                t.text,
            );
        } else {
            ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, crate::i18n::tr("Library"), t.semibold(18.0), t.text);
        }
        let ctx = ui.ctx().clone();
        let mut x = r.right() - 8.0;
        let mut button = |app: &mut LccApp, icon: Icon, id: &str, label: &str, enabled: bool| -> bool {
            x -= BUTTON;
            let rect = Rect::from_min_size(pos2(x, r.center().y - BUTTON / 2.0), Vec2::splat(BUTTON));
            let _ = app;
            round_button(ui, rect, id, icon, label, false, enabled).clicked()
        };
        if button(app, Icon::Gear, "modern:settings", "Settings", true) {
            let _ = app.run("app.settings", json!({}));
        }
        let square = app.ui.view == ViewMode::SquareGrid;
        if button(app, if square { Icon::GridPhoto } else { Icon::GridSquare }, "modern:gridStyle", "Grid Style", true) {
            let _ = app.run(if square { "view.photoGrid" } else { "view.squareGrid" }, json!({}));
        }
        let can_edit = app.session.active().is_some();
        if button(app, Icon::Sliders, "modern:edit", "Edit", can_edit) {
            open_develop(app);
        }
        let can_export = !app.session.selection.ids.is_empty() || can_edit;
        if button(app, Icon::Share, "modern:export", "Export…", can_export) {
            crate::modern_sheets::open_export(app, &ctx);
        }
        if button(app, Icon::Filter, "modern:filters", "Filter", true) {
            app.ui.modern.filters = !app.ui.modern.filters;
        }
        if button(app, Icon::Plus, "modern:import", "Import", true) {
            app.ui.modern.import_menu = !app.ui.modern.import_menu;
        } else if app.ui.modern.import_menu {
            import_menu(app, ui, pos2(r.right() - 8.0, r.bottom() + 4.0));
        }
        let _ = form;
    });
}

/// Import's choices: photos, a folder, and the camera roll where the host has one.
fn import_menu(app: &mut LccApp, ui: &egui::Ui, right_top: Pos2) {
    let t = Tokens::get(ui.ctx());
    let mut items: Vec<(&str, &str, &str)> =
        vec![("modern:import:photos", "Import Photos…", "file.addPhotos"), ("modern:import:folder", "Import Folder…", "file.addFolder")];
    if app.services.camera_roll.is_some() {
        items.push(("modern:import:cameraRoll", "Camera Roll", "file.addCameraRoll"));
    }
    let size = vec2(220.0, BUTTON * items.len() as f32 + 8.0);
    let min = pos2(right_top.x - size.x, right_top.y);
    let mut chosen = None;
    egui::Area::new(Id::new("modern-import-area")).fixed_pos(min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
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
        app.ui.modern.import_menu = false;
        if let Err(e) = app.run(command, json!({})) {
            app.toast_error(ui.ctx(), e);
        }
    } else if tapped_away {
        app.ui.modern.import_menu = false;
    }
}

/// A long press on a grid photo starts picking several, with that photo; later ones add or remove.
fn long_press(app: &mut LccApp) {
    let Some(at) = app.ui.modern.long_press.take() else { return };
    let Some(id) =
        app.widgets.iter().rev().find(|(w, r)| w.starts_with("thumb:") && r.contains(at)).and_then(|(w, _)| w["thumb:".len()..].parse::<u64>().ok())
    else {
        return;
    };
    let mode = if app.ui.modern.selecting { "toggle" } else { "replace" };
    if app.run("library.select", json!({"ids": [id], "mode": mode})).is_ok() {
        app.ui.modern.selecting = true;
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
    app.ui.modern.chrome = true;
}

fn back_to_library(app: &mut LccApp) {
    close_tool(app, true);
    app.ui.modern.panel = false;
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
        egui::Panel::right("modern-panel").exact_size(w).resizable(false).frame(frame).show(ui, |ui| body(app, ui));
    } else {
        let h = (screen.y * 0.42).clamp(220.0, 560.0);
        egui::Panel::bottom("modern-sheet").exact_size(h).resizable(false).frame(frame).show(ui, |ui| {
            // the sheet's grabber: a tap on it closes the sheet
            let grab = Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), 18.0));
            let resp = ui.interact(grab, Id::new("modern-sheet-grab"), Sense::click());
            register(ui.ctx(), "modern:sheetGrab", grab);
            ui.painter().rect_filled(Rect::from_center_size(grab.center(), vec2(40.0, 4.0)), 2.0, t.text_dim);
            if resp.clicked() {
                app.ui.modern.panel = false;
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
    let adjusting = app.ui.modern.tool.is_some();
    let sense = if adjusting { Sense::click_and_drag() } else { Sense::click() };
    let resp = ui.interact(canvas, Id::new("modern-canvas"), sense);
    register(ui.ctx(), "modern:canvas", canvas);
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
    if down && !app.ui.modern.holding && held_for >= HOLD_SECS && moved < SLOP && app.ui.modern.drag.is_none() {
        app.ui.modern.holding = true;
        if app.ui.before_after == BeforeAfter::Off {
            app.ui.before_after = BeforeAfter::Original;
        }
    }
    if down && !app.ui.modern.holding {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    let was_holding = app.ui.modern.holding;
    if !down && was_holding {
        stop_holding(app);
    }
    if adjusting {
        adjust_drag(app, ui, &resp, canvas);
    }
    if resp.clicked() && !was_holding && !adjusting {
        app.ui.modern.chrome = !app.ui.modern.chrome;
    }
}

fn stop_holding(app: &mut LccApp) {
    app.ui.modern.holding = false;
    if app.ui.before_after == BeforeAfter::Original {
        app.ui.before_after = BeforeAfter::Off;
    }
}

fn adjust_drag(app: &mut LccApp, ui: &egui::Ui, resp: &egui::Response, canvas: Rect) {
    let Some(tool) = app.ui.modern.tool else { return };
    if resp.drag_started() {
        app.ui.modern.drag = Some(Drag { start: 0.0, axis: None, stepped: 0 });
    }
    if resp.dragged()
        && let Some(drag) = app.ui.modern.drag.clone()
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
                    set_value(app, spec, v);
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
        app.ui.modern.drag = Some(drag);
    }
    if resp.drag_stopped() {
        if app.ui.modern.drag.as_ref().is_some_and(|d| d.axis == Some(Axis::Across)) {
            let _ = app.run("develop.endInteraction", json!({}));
            app.ui.dragging_control = None;
        }
        app.ui.modern.drag = None;
    }
}

fn snap(spec: &ControlSpec, v: f64) -> f64 {
    let v = if spec.step > 0.0 && spec.id != "wb.temp" { (v / spec.step).round() * spec.step } else { v.round() };
    spec.clamp(v)
}

/// The tool's controls this photo has. White balance is on the photo's own scale, as the Edit
/// panel shows it: Kelvin for raw files, relative (−100..100) for rendered ones.
fn tool_specs(app: &LccApp, tool: Tool) -> Vec<&'static ControlSpec> {
    let kelvin = app.session.active().and_then(|id| app.session.catalog.photo(id)).is_some_and(|p| p.develops_raw() && !p.relative_wb());
    tool.control_ids()
        .iter()
        .filter_map(|id| match *id {
            "wb.temp" if !kelvin => Some(&edit::REL_TEMP),
            "wb.tint" if !kelvin => Some(&edit::REL_TINT),
            id => controls::find(id),
        })
        .collect()
}

/// A control spec by id, the relative white balance of rendered files included.
fn find_spec(id: &str) -> Option<&'static ControlSpec> {
    match id {
        "wb.tempRel" => Some(&edit::REL_TEMP),
        "wb.tintRel" => Some(&edit::REL_TINT),
        id => controls::find(id),
    }
}

/// The develop control and value a parameter's value sets (relative white balance → Kelvin and tint).
fn develop_value(spec: &ControlSpec, v: f64) -> (&'static str, f64) {
    match spec.id {
        "wb.tempRel" => ("wb.temp", edit::rel_to_k(v)),
        "wb.tintRel" => ("wb.tint", v),
        id => (controls::find(id).map_or("", |s| s.id), v),
    }
}

/// Sets a parameter on the active photo, as its Edit panel slider does.
fn set_value(app: &mut LccApp, spec: &ControlSpec, v: f64) {
    let (control, value) = develop_value(spec, v);
    if !control.is_empty() {
        let _ = app.run("develop.set", json!({"control": control, "value": value}));
    }
}

/// A control's value on the active photo (the white balance in effect, as the Edit panel shows it).
fn value_of(app: &LccApp, spec: &ControlSpec) -> Option<f64> {
    let id = app.session.active()?;
    let d = app.session.develop_of(id).unwrap_or_default();
    match spec.id {
        "wb.tempRel" => return Some(edit::k_to_rel(d.wb.temp)),
        "wb.tintRel" => return Some(d.wb.tint.clamp(-100.0, 100.0)),
        _ => {}
    }
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
    let spec = specs.iter().find(|s| s.id == app.ui.modern.control).or(specs.first()).copied()?;
    Some((spec, value_of(app, spec)?))
}

fn step_control(app: &mut LccApp, tool: Tool, by: i32) {
    let specs = tool_specs(app, tool);
    if specs.is_empty() {
        return;
    }
    let at = specs.iter().position(|s| s.id == app.ui.modern.control).unwrap_or(0) as i32;
    let n = specs.len() as i32;
    let next = (at + by).rem_euclid(n) as usize;
    if let Some(s) = specs.get(next) {
        app.ui.modern.control = s.id.to_string();
    }
}

/// Opens a slider tool: remembers its values for Cancel and picks its first parameter.
pub(crate) fn open_tool(app: &mut LccApp, tool: Tool) {
    let specs = tool_specs(app, tool);
    app.ui.modern.snapshot = specs.iter().filter_map(|s| Some((s.id.to_string(), value_of(app, s)?))).collect();
    app.ui.modern.control = specs.first().map(|s| s.id.to_string()).unwrap_or_default();
    app.ui.modern.tool = Some(tool);
    app.ui.modern.centred.0.clear();
    app.ui.modern.panel = false;
    app.ui.modern.drag = None;
}

/// Leaves adjust mode; `cancel` puts the values back as they were when the tool opened.
pub(crate) fn close_tool(app: &mut LccApp, keep: bool) {
    if app.ui.modern.tool.take().is_none() {
        return;
    }
    if app.ui.modern.drag.take().is_some_and(|d| d.axis == Some(Axis::Across)) {
        let _ = app.run("develop.endInteraction", json!({}));
        app.ui.dragging_control = None;
    }
    let snapshot = std::mem::take(&mut app.ui.modern.snapshot);
    if !keep && !snapshot.is_empty() {
        let changed: serde_json::Map<String, Value> = snapshot
            .into_iter()
            .filter_map(|(id, v)| {
                let spec = find_spec(&id)?;
                value_of(app, spec).is_some_and(|now| (now - v).abs() > 1e-9).then_some((spec, v))
            })
            .map(|(spec, v)| develop_value(spec, v))
            .filter(|(id, _)| !id.is_empty())
            .map(|(id, v)| (id.to_string(), json!(v)))
            .collect();
        if !changed.is_empty() {
            let _ = app.run("develop.set", json!({"values": changed}));
        }
    }
}

// ------------------------------------------------------------------------ Develop: chrome

/// The histogram over the photo (`modern.histogram`): pinned across the top under the top bar, or
/// a small window the user drags anywhere (its place kept as a fraction of the room it has).
/// Returns the height the pinned one takes at the top.
fn histogram(app: &mut LccApp, ui: &egui::Ui, area: Rect, form: Form) -> f32 {
    let place = app.ui.modern.histogram;
    if place == HistogramPlace::Off {
        return 0.0;
    }
    let Some(id) = app.session.active() else { return 0.0 };
    let hist = crate::panels::edit::preview_histogram(app, id);
    let scale = crate::panels::histogram::scale_of(app);
    let mode = lcc_raster::HistogramMode::default();
    let bar = if app.ui.modern.chrome && app.ui.modern.tool.is_none() { BAR_H } else { 0.0 };
    let ctx = ui.ctx().clone();
    let paint_in = |ui: &mut egui::Ui, r: Rect| {
        let p = ui.painter();
        p.rect_filled(r, 8.0, Color32::from_black_alpha(170));
        let plot = r.shrink(6.0);
        p.rect_filled(plot, 4.0, crate::panels::histogram::SCOPE_BG);
        if let Some(h) = &hist {
            crate::panels::histogram::paint_plot(p, plot, h, mode, scale);
        }
        register(ui.ctx(), "modern:histogram", r);
    };
    match place {
        HistogramPlace::Off => 0.0,
        HistogramPlace::Pinned => {
            let h = if form.phone() { 72.0 } else { 96.0 };
            let r = Rect::from_min_size(pos2(area.left() + 8.0, area.top() + bar + 6.0), vec2((area.width() - 16.0).max(40.0), h));
            // taps go through to the photo below
            egui::Area::new(Id::new("modern-histogram")).fixed_pos(r.min).order(egui::Order::Middle).interactable(false).show(&ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(r.size(), Sense::hover());
                paint_in(ui, r);
            });
            h + 12.0
        }
        HistogramPlace::Floating => {
            let size = if form.phone() { vec2(168.0, 92.0) } else { vec2(220.0, 120.0) };
            let room = Rect::from_min_max(pos2(area.left() + 8.0, area.top() + bar + 8.0), pos2(area.right() - 8.0, area.bottom() - 8.0));
            let span = (room.size() - size).max(Vec2::ZERO);
            let fraction = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
            let at = app.ui.modern.histogram_at.map(fraction);
            let min = room.min + vec2(span.x * at[0], span.y * at[1]);
            let mut moved = None;
            egui::Area::new(Id::new("modern-histogram")).fixed_pos(min).order(egui::Order::Middle).movable(false).show(&ctx, |ui| {
                let (r, resp) = ui.allocate_exact_size(size, Sense::drag());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, crate::i18n::tr("Histogram")));
                paint_in(ui, r);
                if resp.dragged() {
                    moved = Some(resp.drag_delta());
                }
            });
            if let Some(d) = moved {
                let to = min + d - room.min;
                let f = |v: f32, s: f32| if s > 0.0 { fraction(v / s) } else { 0.0 };
                app.ui.modern.histogram_at = [f(to.x, span.x), f(to.y, span.y)];
            }
            0.0
        }
    }
}

/// The first look at Develop: a faint pill saying that a tap brings the tools.
fn hint(ui: &mut egui::Ui, area: Rect) {
    let t = Tokens::get(ui.ctx());
    let r = Rect::from_center_size(pos2(area.center().x, area.bottom() - 22.0), vec2(36.0, 5.0));
    ui.painter().rect_filled(r, 2.5, t.text_dim.gamma_multiply(0.6));
}

fn top_bar(app: &mut LccApp, ui: &mut egui::Ui, area: Rect, form: Form) {
    let t = Tokens::get(ui.ctx());
    let bar = Rect::from_min_size(area.min, vec2(area.width(), BAR_H));
    egui::Area::new(Id::new("modern-top")).fixed_pos(bar.min).order(egui::Order::Middle).show(ui.ctx(), |ui| {
        let (r, _) = ui.allocate_exact_size(bar.size(), Sense::hover());
        ui.painter().rect_filled(r, 0.0, t.chrome.gamma_multiply(0.88));
        let y = r.center().y - BUTTON / 2.0;
        let at = |x: f32| Rect::from_min_size(pos2(x, y), Vec2::splat(BUTTON));
        if round_button(ui, at(r.left() + 6.0), "modern:back", Icon::ChevronLeft, "Library", false, true).clicked() {
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
        if round_button(ui, next(), "modern:share", Icon::Share, "Export…", false, true).clicked() {
            crate::modern_sheets::open_export(app, ui.ctx());
        }
        let panel_open = app.ui.modern.panel;
        if round_button(ui, next(), "modern:panel", Icon::SidebarRight, "All Edits", panel_open, true).clicked() {
            app.ui.modern.panel = !panel_open;
            if app.ui.modern.panel {
                app.ui.presets = false;
                if !app.ui.right.is_edit_tool() {
                    app.ui.right = RightPanel::Edit;
                }
            }
        }
        let hist_on = app.ui.modern.histogram != HistogramPlace::Off;
        if round_button(ui, next(), "modern:histogramButton", Icon::Activity, "Histogram", hist_on, true).clicked() {
            app.ui.modern.histogram = if hist_on {
                HistogramPlace::Off
            } else if form.side_panel() {
                HistogramPlace::Floating
            } else {
                HistogramPlace::Pinned
            };
        }
        if round_button(ui, next(), "modern:filmstrip", Icon::Filmstrip, "Filmstrip", app.ui.modern.filmstrip, true).clicked() {
            app.ui.modern.filmstrip = !app.ui.modern.filmstrip;
        }
        // press and hold: the original
        let before = round_button(ui, next(), "modern:before", Icon::BeforeAfter, "Hold to See the Original", app.ui.modern.holding, true);
        if before.is_pointer_button_down_on() {
            if !app.ui.modern.holding {
                app.ui.modern.holding = true;
                if app.ui.before_after == BeforeAfter::Off {
                    app.ui.before_after = BeforeAfter::Original;
                }
            }
        } else if app.ui.modern.holding && !ui.input(|i| i.pointer.any_down()) {
            stop_holding(app);
        }
        let can_redo = !app.session.redo.is_empty();
        if round_button(ui, next(), "modern:redo", Icon::RotateRight, "Redo", false, can_redo).clicked() {
            let _ = app.run("edit.redo", json!({}));
        }
        let can_undo = !app.session.undo.is_empty();
        if round_button(ui, next(), "modern:undo", Icon::RotateLeft, "Undo", false, can_undo).clicked() {
            let _ = app.run("edit.undo", json!({}));
        }
    });
}

fn tool_strip(app: &mut LccApp, ui: &mut egui::Ui, area: Rect, form: Form) {
    let t = Tokens::get(ui.ctx());
    let film = if app.ui.modern.filmstrip { t.film_h } else { 0.0 };
    let cell = if form.phone() { 64.0 } else { 76.0 };
    let full = cell * STRIP.len() as f32 + 16.0;
    let w = full.min(area.width());
    let strip = Rect::from_min_size(pos2(area.center().x - w / 2.0, area.bottom() - film - STRIP_H - 10.0), vec2(w, STRIP_H));
    egui::Area::new(Id::new("modern-strip")).fixed_pos(strip.min).order(egui::Order::Middle).show(ui.ctx(), |ui| {
        ui.set_width(w);
        let (bg, _) = ui.allocate_exact_size(strip.size(), Sense::hover());
        register(ui.ctx(), "modern:strip", bg);
        ui.painter().rect_filled(bg, 14.0, t.chrome.gamma_multiply(0.9));
        let mut centred = None;
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(bg.shrink2(vec2(8.0, 0.0))));
        egui::ScrollArea::horizontal()
            .id_salt("modern-strip-scroll")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut inner, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for s in STRIP {
                        let active = match s {
                            Strip::Presets => app.ui.modern.panel && app.ui.presets,
                            Strip::Crop => app.ui.modern.panel && app.ui.right == RightPanel::Crop,
                            Strip::Masks => app.ui.modern.panel && app.ui.right == RightPanel::Masking,
                            Strip::Remove => app.ui.modern.panel && app.ui.right == RightPanel::Remove,
                            _ => false,
                        };
                        let resp = labelled_button(ui, vec2(cell, STRIP_H), &s.id(), s.icon(), s.label(), active);
                        if active {
                            // the open tool sits in the middle of the strip
                            if app.ui.modern.centred.1 != s.id() {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                            centred = Some(s.id());
                        }
                        if resp.clicked() {
                            strip_tool(app, s, active);
                        }
                    }
                });
            });
        app.ui.modern.centred.1 = centred.unwrap_or_default();
    });
}

fn strip_tool(app: &mut LccApp, s: Strip, active: bool) {
    let open = |app: &mut LccApp, p: RightPanel| {
        app.ui.presets = false;
        app.ui.right = p;
        app.ui.modern.panel = true;
    };
    match s {
        Strip::Auto => {
            let _ = app.run("develop.auto", json!({}));
        }
        Strip::Slider(tool) => open_tool(app, tool),
        _ if active => app.ui.modern.panel = false,
        Strip::Presets => {
            app.ui.presets = true;
            app.ui.modern.panel = true;
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
fn adjust_bar(app: &mut LccApp, ui: &mut egui::Ui, area: Rect, form: Form, top: f32) {
    let Some(tool) = app.ui.modern.tool else { return };
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    // the value, over the top of the photo
    if let Some((spec, v)) = current(app, tool) {
        let text = format!("{}  {}", crate::i18n::tr(spec.label), spec.format(v));
        let galley = ctx.layer_painter(egui::LayerId::background()).layout_no_wrap(text, t.semibold(15.0), Color32::WHITE);
        let pill = Rect::from_center_size(pos2(area.center().x, area.top() + top + 34.0), galley.size() + vec2(32.0, 22.0));
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, Id::new("modern-value")));
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
        register(&ctx, "modern:value", pill);
    }
    let specs = tool_specs(app, tool);
    let h = STRIP_H + 8.0;
    let w = if form.phone() { area.width() } else { area.width().min(760.0) };
    let bar = Rect::from_min_size(pos2(area.center().x - w / 2.0, area.bottom() - h - if form.phone() { 0.0 } else { 10.0 }), vec2(w, h));
    egui::Area::new(Id::new("modern-adjust")).fixed_pos(bar.min).order(egui::Order::Middle).show(&ctx, |ui| {
        let (bg, _) = ui.allocate_exact_size(bar.size(), Sense::hover());
        ui.painter().rect_filled(bg, if form.phone() { 0.0 } else { 14.0 }, t.chrome.gamma_multiply(0.92));
        let y = bg.center().y - BUTTON / 2.0;
        if round_button(ui, Rect::from_min_size(pos2(bg.left() + 8.0, y), Vec2::splat(BUTTON)), "modern:cancel", Icon::Close, "Cancel", false, true)
            .clicked()
        {
            close_tool(app, false);
            return;
        }
        if round_button(
            ui,
            Rect::from_min_size(pos2(bg.right() - 8.0 - BUTTON, y), Vec2::splat(BUTTON)),
            "modern:done",
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
        register(ui.ctx(), "modern:chips", chips);
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(chips));
        egui::ScrollArea::horizontal().id_salt("modern-chips").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(
            &mut inner,
            |ui| {
                ui.set_min_height(chips.height());
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for spec in &specs {
                        let selected =
                            spec.id == app.ui.modern.control || (app.ui.modern.control.is_empty() && specs.first().is_some_and(|f| f.id == spec.id));
                        let changed = value_of(app, spec).is_some_and(|v| (v - spec.default).abs() > 1e-9 && !spec.id.starts_with("wb."));
                        let resp = chip(ui, spec, selected, changed);
                        if selected && app.ui.modern.centred.0 != spec.id {
                            // the parameter a drag changes sits in the middle of the row (an
                            // up/down drag on the photo steps through them)
                            resp.scroll_to_me(Some(egui::Align::Center));
                            app.ui.modern.centred.0 = spec.id.to_string();
                        }
                        if resp.double_clicked() {
                            // double tap: back to the default
                            if !spec.id.starts_with("wb.") {
                                let _ = app.run("develop.set", json!({"control": spec.id, "value": spec.default}));
                            }
                        } else if resp.clicked() {
                            app.ui.modern.control = spec.id.to_string();
                        }
                    }
                });
            },
        );
    });
}

// ------------------------------------------------------------------------ widgets

/// A round icon button (44 points), registered as `id` for the control channel.
pub(crate) fn round_button(ui: &mut egui::Ui, rect: Rect, id: &str, icon: Icon, label: &str, active: bool, enabled: bool) -> egui::Response {
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
    register(ui.ctx(), format!("modern:param:{}", spec.id), r);
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
    ("view.modernLayout", "Modern Layout", None, "Window>Panels"),
    ("modern.chrome", "Show Touch Tools", None, ""),
    ("modern.panel", "Show All Edits", None, ""),
    ("modern.tool", "Adjust with One Tool", None, ""),
    ("modern.done", "Finish Adjusting", None, ""),
    ("modern.back", "Back", None, ""),
    ("modern.insets", "Screen Edges to Keep Clear", None, ""),
    ("modern.histogram", "Modern Histogram", None, ""),
];

/// Runs a touch command; `None`: not one. `modern.tool {tool, control?}` opens a slider tool,
/// `modern.done {cancel?}` leaves it.
pub(crate) fn run(app: &mut LccApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let flag = |p: &Value, now: bool| p.get("on").and_then(Value::as_bool).unwrap_or(!now);
    Some(match id {
        "view.modernLayout" => {
            app.ui.modern.enabled = flag(p, app.ui.modern.enabled);
            if !app.ui.modern.enabled {
                close_tool(app, true);
                stop_holding(app);
            }
            Ok(json!({"on": app.ui.modern.enabled}))
        }
        "modern.chrome" => {
            app.ui.modern.chrome = flag(p, app.ui.modern.chrome);
            Ok(json!({"on": app.ui.modern.chrome}))
        }
        "modern.panel" => {
            app.ui.modern.panel = flag(p, app.ui.modern.panel);
            if app.ui.modern.panel {
                close_tool(app, true);
            }
            Ok(json!({"on": app.ui.modern.panel}))
        }
        "modern.tool" => {
            let Some(name) = p.get("tool").and_then(Value::as_str) else {
                return Some(Err("modern.tool: give `tool` (basic, colour, tone, detail, optics or effects)".into()));
            };
            let Some(tool) = Tool::parse(name) else {
                return Some(Err(format!("modern.tool: unknown tool `{name}` (basic, colour, tone, detail, optics or effects)")));
            };
            if app.session.active().is_none() {
                return Some(Err("modern.tool: no photo is active".into()));
            }
            close_tool(app, true);
            open_tool(app, tool);
            if let Some(c) = p.get("control").and_then(Value::as_str) {
                // `wb.temp` / `wb.tint` also name the relative white balance of a rendered file
                let Some(spec) = tool_specs(app, tool).into_iter().find(|s| s.id == c || develop_value(s, 0.0).0 == c) else {
                    return Some(Err(format!("modern.tool: `{c}` is not one of {}'s controls", tool.label())));
                };
                app.ui.modern.control = spec.id.to_string();
            }
            Ok(json!({"tool": tool, "control": app.ui.modern.control}))
        }
        "modern.done" => {
            let cancel = p.get("cancel").and_then(Value::as_bool).unwrap_or(false);
            close_tool(app, !cancel);
            Ok(Value::Null)
        }
        "modern.back" => {
            let closed = back(app);
            if closed.is_none() {
                app.ui.modern.system_back = true;
            }
            Ok(json!({"handled": closed.is_some(), "closed": closed}))
        }
        "modern.insets" => {
            let mut v = [0.0f32; 4];
            for (slot, key) in v.iter_mut().zip(["left", "top", "right", "bottom"]) {
                match p.get(key) {
                    None | Some(Value::Null) => {}
                    Some(Value::Number(n)) => *slot = Insets::clean(n.as_f64()),
                    Some(other) => return Some(Err(format!("modern.insets: `{key}` must be a number of points, not {other}"))),
                }
            }
            app.ui.modern.insets = Insets { left: v[0], top: v[1], right: v[2], bottom: v[3] };
            Ok(serde_json::to_value(app.ui.modern.insets).unwrap_or(Value::Null))
        }
        "modern.histogram" => {
            let place = match p.get("mode") {
                None | Some(Value::Null) => {
                    if app.ui.modern.histogram == HistogramPlace::Off {
                        HistogramPlace::Pinned
                    } else {
                        HistogramPlace::Off
                    }
                }
                Some(Value::String(m)) => match HistogramPlace::parse(m) {
                    Some(h) => h,
                    None => return Some(Err(format!("modern.histogram: unknown mode `{m}` (off, pinned or floating)"))),
                },
                Some(other) => return Some(Err(format!("modern.histogram: `mode` must be off, pinned or floating, not {other}"))),
            };
            if let Some(at) = p.get("at") {
                let xy: Option<Vec<f32>> = at
                    .as_array()
                    .filter(|a| a.len() == 2)
                    .map(|a| a.iter().filter_map(Value::as_f64).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0) as f32).collect());
                match xy.as_deref() {
                    Some([x, y]) => app.ui.modern.histogram_at = [*x, *y],
                    _ => return Some(Err("modern.histogram: `at` must be [x, y], each 0…1".into())),
                }
            }
            app.ui.modern.histogram = place;
            Ok(json!({"mode": place, "at": app.ui.modern.histogram_at}))
        }
        _ => return None,
    })
}
