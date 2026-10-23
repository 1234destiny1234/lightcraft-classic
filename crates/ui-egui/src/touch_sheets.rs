//! The touch layout's full screens (fork-owned): Settings and Export.
//!
//! **Settings** shows the desktop Settings' own categories and rows ([`crate::panels::settings`]),
//! so both layouts read and write the same settings; only the presentation differs. On a phone it
//! is a full-screen page: the list of categories, and a tap opens one as its own page (Back
//! returns). On a tablet (or a folding phone's inner screen) it is a large sheet with the list
//! beside the open category. Rows are finger-sized, a label above its controls on a narrow screen,
//! and the page scrolls by dragging.
//!
//! **Export** is a sheet with a few presets (full-size JPEG, a 2048-pixel JPEG for sharing, 16-bit
//! TIFF, DNG for raw photos), size and quality as segmented choices, and the way the files leave:
//! on Android saved to Pictures/<app> (the gallery) or shared through the system share sheet,
//! elsewhere written to the export folder. It exports through `app.export` (the engine's one
//! encoder, `lcc_engine::export`) in the background; the activity stack shows the export, then the
//! save to Pictures; failures are toasts.
//!
//! Own design (patterns of touch settings and share sheets; no assets taken from any).

use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{Align2, Id, Rect, Sense, Stroke, Vec2, pos2, vec2};
use lcc_engine::activity::{Cancel, TaskGuard};
use serde_json::{Value, json};

use crate::LccApp;
use crate::icons::Icon;
use crate::module::ModuleId;
use crate::state::Dialog;
use crate::theme::Tokens;
use crate::touch::round_button;
use crate::widgets::register;

/// The header of a full screen or sheet.
const HEADER_H: f32 = 56.0;
/// A row of the category list.
const ROW_H: f32 = 56.0;
/// The category list's width beside the open category (tablet).
const LIST_W: f32 = 240.0;
/// Short side, in points, from which Settings is a sheet rather than a full screen.
const TABLET: f32 = 600.0;

/// What the touch screens remember between frames (`LccApp::touch_sheets`).
#[derive(Default)]
pub struct TouchSheets {
    /// Settings was showing last frame (so a fresh opening starts on the category list).
    settings_seen: bool,
    /// Phone: a category's page is open (rather than the list).
    pub settings_detail: bool,
    /// Settings was last drawn as a phone screen (the list and a category's page one at a time).
    settings_phone: bool,
    /// The Export sheet, while it is open.
    pub export: Option<ExportSheet>,
    /// The sheet's choices last time (the next opening starts from them).
    last: Option<ExportChoice>,
    /// A background export the sheet started, and where its files go when it ends.
    handoff: Option<Then>,
    /// Files being saved to Pictures (and shared) by the host.
    gallery: Option<Gallery>,
}

struct Gallery {
    rx: Receiver<Result<Vec<String>, String>>,
    /// The save's row in the activity stack (it can't be stopped once the host has the files).
    _guard: TaskGuard,
    share: bool,
}

/// Where the exported files go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    /// Pictures/<app> (Android's gallery).
    Save,
    /// Pictures/<app>, then the share sheet.
    Share,
    /// The export folder only (no gallery on this platform).
    Folder,
}

impl Then {
    fn parse(s: &str) -> Option<Then> {
        match s.to_ascii_lowercase().as_str() {
            "save" => Some(Then::Save),
            "share" => Some(Then::Share),
            "folder" => Some(Then::Folder),
            _ => None,
        }
    }
}

/// The file format the sheet writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Tiff,
    Dng,
}

impl Format {
    fn parse(s: &str) -> Option<Format> {
        match s.to_ascii_lowercase().as_str() {
            "jpeg" | "jpg" => Some(Format::Jpeg),
            "tiff" | "tif" => Some(Format::Tiff),
            "dng" => Some(Format::Dng),
            _ => None,
        }
    }
}

/// The sheet's choices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportChoice {
    pub format: Format,
    /// Long edge in pixels; 0 = full size.
    pub long_edge: u32,
    /// JPEG quality (1–100).
    pub quality: u8,
}

impl Default for ExportChoice {
    fn default() -> Self {
        ExportChoice { format: Format::Jpeg, long_edge: 0, quality: 90 }
    }
}

/// The open Export sheet: what it exports and how.
#[derive(Clone, Debug)]
pub struct ExportSheet {
    pub ids: Vec<u64>,
    pub choice: ExportChoice,
    /// Some of the photos are raw (DNG is offered).
    raw: bool,
}

/// The presets: (id, label, choice).
const PRESETS: [(&str, &str, ExportChoice); 4] = [
    ("full", "JPEG · Full Size", ExportChoice { format: Format::Jpeg, long_edge: 0, quality: 90 }),
    ("share", "JPEG · 2048 px, for Sharing", ExportChoice { format: Format::Jpeg, long_edge: 2048, quality: 85 }),
    ("tiff", "TIFF · 16-bit, Full Size", ExportChoice { format: Format::Tiff, long_edge: 0, quality: 90 }),
    ("dng", "DNG · Raw Data with Edits", ExportChoice { format: Format::Dng, long_edge: 0, quality: 90 }),
];
const SIZES: [(u32, &str); 4] = [(0, "Full"), (4096, "4096"), (2048, "2048"), (1080, "1080")];
const QUALITIES: [(u8, &str); 4] = [(60, "60"), (80, "80"), (90, "90"), (100, "100")];

/// Per frame, before the dialogs: the hand-off of finished exports, the Export sheet, and (touch
/// layout) Settings as a touch screen. True when Settings was drawn here (the desktop dialog is
/// then skipped).
pub(crate) fn show(app: &mut LccApp, ctx: &egui::Context) -> bool {
    poll(app, ctx);
    if !crate::touch::on(app) {
        app.touch_sheets.settings_seen = false;
        app.touch_sheets.export = None;
        return false;
    }
    if app.ui.dialog.is_none() && app.touch_sheets.export.is_some() {
        export_sheet(app, ctx);
    }
    if matches!(app.ui.dialog, Some(Dialog::Settings { .. })) {
        settings(app, ctx);
        return true;
    }
    app.touch_sheets.settings_seen = false;
    false
}

/// Back (`touch.back`) on these screens, before the dialogs: the Export sheet closes; on a
/// phone, Settings goes from a category's page to the category list (the next Back closes it, as
/// the dialog it is). `None`: neither was open, or Settings is on its list (Back closes it).
pub(crate) fn back(app: &mut LccApp) -> Option<&'static str> {
    // (a dialog over the sheet closes first, as Esc would)
    if app.ui.dialog.is_none() && app.touch_sheets.export.take().is_some() {
        return Some("export");
    }
    let settings = matches!(app.ui.dialog, Some(Dialog::Settings { .. }));
    if settings && app.touch_sheets.settings_seen && app.touch_sheets.settings_phone && app.touch_sheets.settings_detail {
        app.touch_sheets.settings_detail = false;
        return Some("settingsPage");
    }
    None
}

fn dim(ctx: &egui::Context, screen: Rect) {
    egui::Area::new(Id::new("touch-sheet-dim")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
        // the backdrop takes the taps under the sheet
        let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(140));
    });
}

// ------------------------------------------------------------------------ Settings

fn settings(app: &mut LccApp, ctx: &egui::Context) {
    let Some(Dialog::Settings { tab }) = app.ui.dialog.clone() else { return };
    let mut tab = tab;
    let screen = ctx.content_rect();
    let phone = screen.width().min(screen.height()) < TABLET;
    app.touch_sheets.settings_phone = phone;
    if !app.touch_sheets.settings_seen {
        app.touch_sheets.settings_seen = true;
        // a category asked for by name opens straight away; Settings on its own, the list
        app.touch_sheets.settings_detail = tab != "general" || app.ui.settings_section.is_some();
    }
    let mut close = false;
    // Back (Esc): from a category to the list on a phone, else closes
    if !egui::Popup::is_any_open(ctx) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        if phone && app.touch_sheets.settings_detail {
            app.touch_sheets.settings_detail = false;
        } else {
            close = true;
        }
    }
    let rect = if phone {
        screen
    } else {
        dim(ctx, screen);
        Rect::from_center_size(screen.center(), vec2((screen.width() - 48.0).min(1000.0), (screen.height() - 48.0).min(860.0)))
    };
    let t = Tokens::get(ctx);
    egui::Area::new(Id::new("touch-settings")).order(egui::Order::Foreground).fixed_pos(rect.min).show(ctx, |ui| {
        let (bg, _) = ui.allocate_exact_size(rect.size(), Sense::click());
        ui.painter().rect_filled(bg, if phone { 0.0 } else { 16.0 }, t.chrome);
        if !phone {
            ui.painter().rect_stroke(bg, 16.0, Stroke::new(1.0, t.divider), egui::StrokeKind::Inside);
        }
        let header = Rect::from_min_size(bg.min, vec2(bg.width(), HEADER_H));
        let detail_open = phone && app.touch_sheets.settings_detail;
        let label =
            |id: &str| crate::panels::settings::TABS.iter().find(|(i, _)| *i == id).map_or(id.to_string(), |(_, l)| crate::i18n::tr(l).to_string());
        let title = if detail_open { label(&tab) } else { crate::i18n::tr("Settings").to_string() };
        let mut x = header.left() + 16.0;
        if detail_open {
            let back = Rect::from_min_size(pos2(header.left() + 6.0, header.center().y - 22.0), Vec2::splat(44.0));
            if round_button(ui, back, "touch:settings:back", Icon::ChevronLeft, "Back", false, true).clicked() {
                app.touch_sheets.settings_detail = false;
            }
            x = back.right() + 6.0;
        }
        ui.painter().text(pos2(x, header.center().y), Align2::LEFT_CENTER, title, t.semibold(19.0), t.text);
        let shut = Rect::from_min_size(pos2(header.right() - 50.0, header.center().y - 22.0), Vec2::splat(44.0));
        if round_button(ui, shut, "touch:settings:close", Icon::Close, "Close", false, true).clicked() {
            close = true;
        }
        ui.painter().hline(bg.x_range(), header.bottom(), Stroke::new(1.0, t.divider));
        let body = Rect::from_min_max(pos2(bg.left(), header.bottom() + 1.0), bg.max);
        if phone {
            if app.touch_sheets.settings_detail {
                detail(app, ui, body, &tab, true);
            } else if let Some(picked) = category_list(ui, body, &tab, false) {
                tab = picked;
                app.touch_sheets.settings_detail = true;
            }
        } else {
            let list = Rect::from_min_size(body.min, vec2(LIST_W.min(body.width() * 0.4), body.height()));
            if let Some(picked) = category_list(ui, list, &tab, true) {
                tab = picked;
            }
            ui.painter().vline(list.right(), body.y_range(), Stroke::new(1.0, t.divider));
            let right = Rect::from_min_max(pos2(list.right() + 1.0, body.top()), body.max);
            detail(app, ui, right, &tab, right.width() < 520.0);
        }
    });
    if close {
        app.ui.dialog = None;
        app.ui.settings_section = None;
        app.touch_sheets.settings_seen = false;
        return;
    }
    // the rows may have closed Settings or opened another dialog (e.g. a folder picker's)
    if let Some(Dialog::Settings { tab: now }) = &mut app.ui.dialog {
        *now = tab;
    }
}

/// The categories, one big row each; returns the one tapped. `selected` marks the open one
/// (tablet, where it shows beside the list).
fn category_list(ui: &mut egui::Ui, area: Rect, open: &str, selected: bool) -> Option<String> {
    let t = Tokens::get(ui.ctx());
    let mut picked = None;
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(area));
    egui::ScrollArea::vertical().id_salt("touch-settings-list").auto_shrink([false, false]).show(&mut inner, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (id, label) in crate::panels::settings::TABS {
            #[cfg(target_arch = "wasm32")]
            if matches!(*id, "connections" | "sync") {
                continue;
            }
            // the shortcut table needs a wide screen (and a keyboard): not on a phone's
            if !selected && *id == "shortcuts" {
                continue;
            }
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
            let name = crate::i18n::tr(label);
            let on = selected && *id == open;
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, name));
            register(ui.ctx(), format!("touch:settings:{id}"), r);
            let p = ui.painter();
            if on {
                p.rect_filled(r.shrink2(vec2(8.0, 4.0)), 10.0, t.tool_active);
            } else if resp.is_pointer_button_down_on() {
                p.rect_filled(r.shrink2(vec2(8.0, 4.0)), 10.0, t.pressed);
            }
            p.text(pos2(r.left() + 20.0, r.center().y), Align2::LEFT_CENTER, name, t.font(16.0), if on { t.pick } else { t.text });
            if !selected {
                crate::icons::paint(
                    p,
                    Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), Vec2::splat(18.0)),
                    Icon::ChevronRight,
                    t.text_dim,
                );
                p.hline(r.left() + 20.0..=r.right(), r.bottom() - 0.5, Stroke::new(1.0, t.divider));
            }
            if resp.clicked() {
                picked = Some((*id).to_string());
            }
        }
    });
    picked
}

/// A category's rows (the desktop Settings' own), finger-sized; `stacked` puts labels above
/// their controls.
fn detail(app: &mut LccApp, ui: &mut egui::Ui, area: Rect, tab: &str, stacked: bool) {
    let t = Tokens::get(ui.ctx());
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink2(vec2(16.0, 0.0))));
    let was = crate::dialog_frame::set_finger_sized(ui.ctx(), true);
    egui::ScrollArea::vertical().id_salt(("touch-settings-detail", tab)).auto_shrink([false, false]).show(&mut inner, |ui| {
        let s = ui.spacing_mut();
        s.item_spacing = vec2(10.0, 10.0);
        s.interact_size = vec2(48.0, 40.0);
        s.button_padding = vec2(14.0, 8.0);
        s.icon_width = 24.0;
        s.icon_width_inner = 14.0;
        s.icon_spacing = 10.0;
        s.combo_height = 480.0;
        for (style, size) in [(egui::TextStyle::Body, 15.0), (egui::TextStyle::Button, 15.0)] {
            if let Some(f) = ui.style_mut().text_styles.get_mut(&style) {
                f.size = size;
            }
        }
        ui.add_space(12.0);
        // room on both sides for the group panels
        ui.set_width((ui.available_width() - 12.0).max(120.0));
        let body = |ui: &mut egui::Ui| {
            crate::dialog_frame::group_begin(ui, None);
            if stacked && tab == "shortcuts" {
                // (asked for by name on a phone; the list doesn't offer it there)
                ui.label(egui::RichText::new(crate::i18n::tr("Keyboard shortcuts are edited on a wider screen.")).color(t.text_label));
                return;
            }
            crate::panels::settings::tab_body(app, ui, &t, tab);
        };
        if stacked {
            crate::dialog_frame::stacked_form(ui, 120.0, body);
        } else {
            crate::dialog_frame::form(ui, 180.0, body);
        }
        ui.add_space(48.0);
    });
    crate::dialog_frame::set_finger_sized(ui.ctx(), was);
}

// ------------------------------------------------------------------------ Export

/// The photos an Export would take: Library's selection (in grid order), else the active photo;
/// in Develop the photo being edited.
fn targets(app: &mut LccApp) -> Vec<u64> {
    if app.ui.module == ModuleId::Library && !app.session.selection.ids.is_empty() {
        let sel: std::collections::HashSet<_> = app.session.selection.ids.iter().copied().collect();
        let mut v: Vec<_> = app.session.visible().iter().copied().filter(|id| sel.contains(id)).collect();
        let shown: std::collections::HashSet<_> = v.iter().copied().collect();
        v.extend(app.session.selection.ids.iter().filter(|id| !shown.contains(id)));
        return v.into_iter().map(|id| id.0).collect();
    }
    app.session.active().into_iter().map(|id| id.0).collect()
}

/// Opens the Export sheet for [`targets`]; a toast says so when there is nothing to export.
pub(crate) fn open_export(app: &mut LccApp, ctx: &egui::Context) {
    if let Err(e) = open_sheet(app) {
        app.toast_error(ctx, crate::i18n::tr(&e));
    }
}

fn open_sheet(app: &mut LccApp) -> Result<Value, String> {
    let ids = targets(app);
    if ids.is_empty() {
        return Err("No photo selected".into());
    }
    let raw = ids.iter().any(|id| app.session.catalog.photo(lcc_catalog::PhotoId(*id)).is_some_and(|p| p.develops_raw()));
    let mut choice = app.touch_sheets.last.unwrap_or_default();
    if choice.format == Format::Dng && !raw {
        choice = ExportChoice::default();
    }
    let n = ids.len();
    app.touch_sheets.export = Some(ExportSheet { ids, choice, raw });
    Ok(json!({"photos": n}))
}

/// Where the sheet's files are written: the app's own storage on Android (they are then copied
/// to Pictures), else the usual export folder.
pub fn export_dir() -> String {
    if cfg!(target_os = "android")
        && let Some(d) = lcc_brand::platform_dirs()
    {
        return d.pictures.join("Exports").to_string_lossy().into_owned();
    }
    crate::control::default_export_dir()
}

fn can_hand_off(app: &LccApp) -> bool {
    app.services.save_to_gallery.is_some()
}

/// Exports `ids` with `choice`, then does `then` with the files. Params of `touch.export`.
fn start_export(app: &mut LccApp, ids: Vec<u64>, choice: ExportChoice, then: Then, dir: Option<String>) -> Result<Value, String> {
    if ids.is_empty() {
        return Err("no photo selected".into());
    }
    if then != Then::Folder && !can_hand_off(app) {
        return Err("saving to Pictures and sharing need the Android app here; use `then: folder`".into());
    }
    if app.export.is_some() {
        return Err("an export is already running".into());
    }
    let dir = dir.unwrap_or_else(export_dir);
    if dir.trim().is_empty() {
        return Err(crate::control::NO_EXPORT_FOLDER.into());
    }
    let mut p = json!({
        "ids": ids,
        "dir": dir,
        "destinationMode": "specificFolder",
        "conflict": "unique",
        "naming": "{name}",
        "background": true,
    });
    match choice.format {
        Format::Jpeg => {
            p["format"] = json!("jpeg");
            p["quality"] = json!(choice.quality.clamp(1, 100));
        }
        Format::Tiff => {
            p["format"] = json!("tiff");
            p["bitDepth"] = json!(16);
        }
        Format::Dng => p["format"] = json!("dng"),
    }
    if choice.long_edge > 0 && choice.format != Format::Dng {
        p["longEdge"] = json!(choice.long_edge);
        p["dontEnlarge"] = json!(true);
    }
    app.touch_sheets.last = Some(choice);
    app.last_export_result = None;
    let r = app.run("app.export", p)?;
    if app.export.is_some() {
        app.touch_sheets.handoff = Some(then);
    } else {
        // no background writer (a window-less run): written already
        let files = paths(&r);
        hand_off(app, &files, then)?;
    }
    app.touch_sheets.export = None;
    Ok(r)
}

/// The files an export result lists as written.
fn paths(result: &Value) -> Vec<String> {
    result
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| f.get("path").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// Gives the files to the host (Pictures, the share sheet); nothing to do for `Folder`.
fn hand_off(app: &mut LccApp, files: &[String], then: Then) -> Result<(), String> {
    if then == Then::Folder || files.is_empty() {
        return Ok(());
    }
    let share = then == Then::Share;
    let save = app.services.save_to_gallery.as_mut().ok_or("no gallery on this platform")?;
    let rx = save(files, share)?;
    let label = if share { "Sharing" } else { "Saving to Pictures" };
    let guard = app.session.activity.start("export", crate::i18n::tr(label), Cancel::No);
    app.touch_sheets.gallery = Some(Gallery { rx, _guard: guard, share });
    Ok(())
}

/// A finished background export goes on to Pictures / the share sheet; the host's answer is told.
fn poll(app: &mut LccApp, ctx: &egui::Context) {
    if app.touch_sheets.handoff.is_some() && app.export.is_none() {
        let then = app.touch_sheets.handoff.take().unwrap_or(Then::Folder);
        // (an export that failed outright left no result; its toast said why)
        let files = match &app.last_export_result {
            Some(r) if r.get("cancelled").and_then(Value::as_bool) != Some(true) => paths(r),
            _ => Vec::new(),
        };
        if let Err(e) = hand_off(app, &files, then) {
            app.toast_error(ctx, format!("{}: {e}", crate::i18n::tr("Not saved to Pictures")));
        }
    }
    let Some(g) = &app.touch_sheets.gallery else { return };
    match g.rx.try_recv() {
        Ok(Ok(uris)) => {
            let share = g.share;
            app.touch_sheets.gallery = None;
            let n = uris.len();
            let msg = if share {
                format!("{} · {n}", crate::i18n::tr("Saved to Pictures and shared"))
            } else {
                format!("{} · {n}", crate::i18n::tr("Saved to Pictures"))
            };
            app.toast(ctx, msg);
        }
        Ok(Err(e)) => {
            app.touch_sheets.gallery = None;
            app.toast_error(ctx, format!("{}: {e}", crate::i18n::tr("Not saved to Pictures")));
        }
        Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(250)),
        Err(TryRecvError::Disconnected) => {
            app.touch_sheets.gallery = None;
            app.toast_error(ctx, crate::i18n::tr("Saving to Pictures stopped unexpectedly"));
        }
    }
}

fn export_sheet(app: &mut LccApp, ctx: &egui::Context) {
    let Some(sheet) = app.touch_sheets.export.clone() else { return };
    let screen = ctx.content_rect();
    let phone = screen.width().min(screen.height()) < TABLET;
    let t = Tokens::get(ctx);
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.touch_sheets.export = None;
        return;
    }
    dim(ctx, screen);
    let w = if phone { screen.width() } else { 560.0f32.min(screen.width() - 32.0) };
    let h_id = Id::new("touch-export-height");
    let h = ctx.data(|d| d.get_temp::<f32>(h_id)).unwrap_or(520.0).min(screen.height() - 24.0);
    let min = if phone { pos2(screen.left(), screen.bottom() - h) } else { pos2(screen.center().x - w / 2.0, screen.center().y - h / 2.0) };
    let mut choice = sheet.choice;
    let mut go: Option<Then> = None;
    let mut close = false;
    egui::Area::new(Id::new("touch-export")).order(egui::Order::Foreground).fixed_pos(min).show(ctx, |ui| {
        let bg = Rect::from_min_size(min, vec2(w, h));
        let radius = if phone { egui::CornerRadius { nw: 16, ne: 16, sw: 0, se: 0 } } else { egui::CornerRadius::same(16) };
        ui.painter().rect_filled(bg, radius, t.chrome);
        ui.set_width(w);
        let was = crate::dialog_frame::set_finger_sized(ctx, true);
        let content = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.spacing_mut().item_spacing = vec2(8.0, 10.0);
            // header: title, count, close
            let (head, _) = ui.allocate_exact_size(vec2(w - 32.0, 44.0), Sense::hover());
            ui.painter().text(pos2(head.left(), head.center().y - 8.0), Align2::LEFT_CENTER, crate::i18n::tr("Export"), t.semibold(19.0), t.text);
            let count = format!("{} · {}", crate::i18n::tr("Photos"), sheet.ids.len());
            ui.painter().text(pos2(head.left(), head.center().y + 12.0), Align2::LEFT_CENTER, count, t.font(13.0), t.text_dim);
            let shut = Rect::from_min_size(pos2(head.right() - 44.0, head.top()), Vec2::splat(44.0));
            if round_button(ui, shut, "touch:export:close", Icon::Close, "Close", false, true).clicked() {
                close = true;
            }
            // presets
            for (id, label, preset) in PRESETS {
                if preset.format == Format::Dng && !sheet.raw {
                    continue;
                }
                let on = choice == preset;
                let (r, resp) = ui.allocate_exact_size(vec2(w - 32.0, 48.0), Sense::click());
                let name = crate::i18n::tr(label);
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, name));
                register(ctx, format!("touch:export:preset:{id}"), r);
                let p = ui.painter();
                p.rect(
                    r,
                    10.0,
                    if on { t.tool_active } else { t.button },
                    Stroke::new(1.0, if on { t.accent } else { t.button_border }),
                    egui::StrokeKind::Inside,
                );
                let dot = pos2(r.left() + 22.0, r.center().y);
                p.circle_stroke(dot, 8.0, Stroke::new(1.5, if on { t.accent } else { t.text_dim }));
                if on {
                    p.circle_filled(dot, 4.5, t.accent);
                }
                p.text(pos2(r.left() + 42.0, r.center().y), Align2::LEFT_CENTER, name, t.font(15.0), t.text);
                if resp.clicked() {
                    choice = preset;
                }
            }
            if choice.format != Format::Dng {
                ui.label(egui::RichText::new(crate::i18n::tr("Long Edge (pixels)")).color(t.text_label));
                let items: Vec<(&str, &str)> = SIZES.iter().map(|(_, l)| (*l, *l)).collect();
                let active = SIZES.iter().position(|(v, _)| *v == choice.long_edge);
                if let Some(i) = crate::widgets::segmented(ui, "touchExportSize", &items, active, SIZES.len())
                    && let Some((v, _)) = SIZES.get(i)
                {
                    choice.long_edge = *v;
                }
            }
            if choice.format == Format::Jpeg {
                ui.label(egui::RichText::new(crate::i18n::tr("Quality")).color(t.text_label));
                let items: Vec<(&str, &str)> = QUALITIES.iter().map(|(_, l)| (*l, *l)).collect();
                let active = QUALITIES.iter().position(|(v, _)| *v == choice.quality);
                if let Some(i) = crate::widgets::segmented(ui, "touchExportQuality", &items, active, QUALITIES.len())
                    && let Some((v, _)) = QUALITIES.get(i)
                {
                    choice.quality = *v;
                }
            }
            ui.add_space(4.0);
            let busy = app.export.is_some();
            let gap = 10.0;
            if can_hand_off(app) {
                let bw = ((w - 32.0 - gap) / 2.0).max(60.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    if action_button(ui, "touch:export:share", Some(Icon::Share), "Share", false, !busy, bw).clicked() {
                        go = Some(Then::Share);
                    }
                    if action_button(ui, "touch:export:save", None, "Save to Pictures", true, !busy, bw).clicked() {
                        go = Some(Then::Save);
                    }
                });
                ui.label(egui::RichText::new(format!("{} ▸ {}", crate::i18n::tr("Pictures"), lcc_brand::DISPLAY_NAME)).size(12.0).color(t.text_dim));
            } else {
                if action_button(ui, "touch:export:go", None, "Export", true, !busy, w - 32.0).clicked() {
                    go = Some(Then::Folder);
                }
                ui.label(egui::RichText::new(export_dir()).size(12.0).color(t.text_dim));
            }
            if busy {
                ui.label(egui::RichText::new(crate::i18n::tr("An export is running; see the activity list.")).size(12.0).color(t.text_dim));
            }
            ui.add_space(8.0);
        });
        crate::dialog_frame::set_finger_sized(ctx, was);
        let measured = content.response.rect.height().ceil();
        if (measured - h).abs() > 0.5 {
            ctx.data_mut(|d| d.insert_temp(h_id, measured));
            ctx.request_repaint();
        }
    });
    if close {
        app.touch_sheets.export = None;
        return;
    }
    if let Some(s) = app.touch_sheets.export.as_mut() {
        s.choice = choice;
    }
    if let Some(then) = go {
        match start_export(app, sheet.ids.clone(), choice, then, None) {
            Ok(_) => app.touch_sheets.export = None,
            Err(e) => app.toast_error(ctx, e),
        }
    }
}

/// A wide button at the bottom of a sheet; `primary` is filled with the accent.
fn action_button(ui: &mut egui::Ui, id: &str, icon: Option<Icon>, label: &str, primary: bool, enabled: bool, width: f32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let name = crate::i18n::tr(label);
    let (r, resp) = ui.allocate_exact_size(vec2(width, 50.0), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    register(ui.ctx(), id, r);
    let p = ui.painter();
    let fill = if !enabled {
        t.button
    } else if primary {
        t.accent
    } else if resp.is_pointer_button_down_on() {
        t.pressed
    } else {
        t.button
    };
    p.rect(r, 12.0, fill, Stroke::new(1.0, if primary { fill } else { t.button_border }), egui::StrokeKind::Inside);
    let c = if !enabled {
        t.text_disabled
    } else if primary {
        egui::Color32::WHITE
    } else {
        t.text
    };
    let galley = p.layout_no_wrap(name.to_string(), t.semibold(15.0), c);
    let icon_w = if icon.is_some() { 28.0 } else { 0.0 };
    let start = r.center().x - (galley.size().x + icon_w) / 2.0;
    if let Some(icon) = icon {
        crate::icons::paint(p, Rect::from_center_size(pos2(start + 10.0, r.center().y), Vec2::splat(20.0)), icon, c);
    }
    p.galley(pos2(start + icon_w, r.center().y - galley.size().y / 2.0), galley, c);
    resp
}

// ------------------------------------------------------------------------ commands

/// The touch screens' commands (`menus_fork` lists them).
pub const COMMANDS: &[crate::menus::UiCommand] =
    &[("touch.exportSheet", "Export (Touch Sheet)…", None, ""), ("touch.export", "Export from the Touch Sheet", None, "")];

/// `touch.exportSheet {on?}` opens (or closes) the Export sheet; `touch.export {format?, longEdge?,
/// quality?, then?, dir?, ids?}` exports as the sheet's buttons do (`then`: save | share | folder).
pub(crate) fn run(app: &mut LccApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    Some(match id {
        "touch.exportSheet" => {
            if p.get("on").and_then(Value::as_bool) == Some(false) {
                app.touch_sheets.export = None;
                Ok(json!({"on": false}))
            } else {
                open_sheet(app)
            }
        }
        "touch.export" => export_command(app, p),
        _ => return None,
    })
}

fn export_command(app: &mut LccApp, p: &Value) -> Result<Value, String> {
    let base = app.touch_sheets.export.as_ref().map_or_else(|| app.touch_sheets.last.unwrap_or_default(), |s| s.choice);
    let format = match p.get("format") {
        None | Some(Value::Null) => base.format,
        Some(v) => v.as_str().and_then(Format::parse).ok_or("touch.export: `format` is jpeg, tiff or dng")?,
    };
    let long_edge = match p.get("longEdge") {
        None | Some(Value::Null) => base.long_edge,
        Some(v) => {
            v.as_u64().filter(|n| *n <= 65_535).ok_or("touch.export: `longEdge` is a whole number of pixels up to 65535 (0 = full size)")? as u32
        }
    };
    let quality = match p.get("quality") {
        None | Some(Value::Null) => base.quality,
        Some(v) => v.as_u64().filter(|q| (1..=100).contains(q)).ok_or("touch.export: `quality` is 1 to 100")? as u8,
    };
    let default_then = if can_hand_off(app) { Then::Save } else { Then::Folder };
    let then = match p.get("then") {
        None | Some(Value::Null) => default_then,
        Some(v) => v.as_str().and_then(Then::parse).ok_or("touch.export: `then` is save, share or folder")?,
    };
    let dir = match p.get("dir") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_str().ok_or("touch.export: `dir` is a folder path")?.to_string()),
    };
    let ids: Vec<u64> = match p.get("ids") {
        None | Some(Value::Null) => match app.touch_sheets.export.as_ref().map(|s| s.ids.clone()) {
            Some(ids) => ids,
            None => targets(app),
        },
        Some(v) => v
            .as_array()
            .ok_or("touch.export: `ids` is a list of photo ids")?
            .iter()
            .map(|x| x.as_u64().ok_or("touch.export: `ids` is a list of photo ids"))
            .collect::<Result<_, _>>()?,
    };
    start_export(app, ids, ExportChoice { format, long_edge, quality }, then, dir)
}
