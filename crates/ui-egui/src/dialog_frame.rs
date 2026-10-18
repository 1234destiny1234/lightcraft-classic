//! One layout for every modal dialog: a fixed **header** (the title bar, plus tabs where the dialog
//! has them), a **body** that scrolls vertically, and a fixed **footer** with the action buttons.
//!
//! The whole window is capped to the viewport height minus [`SCREEN_MARGIN`] at the top and the
//! bottom, so the footer buttons stay on screen however tall the content is (a 1366×768 laptop, a
//! small window). When the content fits, the body takes just its own height: no scrollbar and no
//! empty space. The body keeps the full dialog width, so a scrollbar appearing doesn't reflow it.
//!
//! The parts are drawn in order inside the window's content closure, so each can borrow what it
//! needs mutably:
//!
//! ```ignore
//! let shown = egui::Window::new(title).id(id).show(ctx, |ui| {
//!     let mut layout = ModalLayout::begin(ui, id);
//!     tabs(ui);                               // header (optional)
//!     layout.body(ui, |ui| content(ui));     // scrolls when needed
//!     layout.footer(ui, |ui| buttons(ui));    // always visible
//! });
//! ModalLayout::finish(ctx, id, shown.as_ref().map(|w| w.response.rect));
//! ```
//!
//! The heights of the footer and of the window chrome (title bar, frame margins) are measured each
//! frame and used on the next, so the cap settles after the first frame.

use egui::{Id, Rect, Ui};

/// Room kept free above and below a dialog, in points.
pub const SCREEN_MARGIN: f32 = 32.0;
/// The body never gets shorter than this (a viewport smaller than any dialog still shows some).
const MIN_BODY: f32 = 60.0;
/// First-frame guesses, before anything was measured.
const GUESS_CHROME: f32 = 64.0;
const GUESS_FOOTER: f32 = 44.0;

/// The tallest a dialog window may be in this viewport.
pub fn max_window_height(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() - 2.0 * SCREEN_MARGIN).max(MIN_BODY * 2.0)
}

/// The header / scrolling body / footer layout of a modal dialog; see the module docs.
pub struct ModalLayout {
    id: Id,
    /// the most the content (header + body + footer) may take
    max_content: f32,
    top: f32,
    body_bottom: Option<f32>,
}

impl ModalLayout {
    /// Starts the layout at the top of a dialog window's content. `id` is the window's id.
    pub fn begin(ui: &Ui, id: Id) -> Self {
        Self::begin_with_max(ui, id, max_window_height(ui.ctx()))
    }

    /// [`begin`](Self::begin) for a window anchored `top` points below the viewport's top edge
    /// rather than centred: it may reach down to [`SCREEN_MARGIN`] above the bottom.
    pub fn begin_below(ui: &Ui, id: Id, top: f32) -> Self {
        let room = (ui.ctx().content_rect().height() - top.max(0.0) - SCREEN_MARGIN).max(MIN_BODY * 2.0);
        Self::begin_with_max(ui, id, room)
    }

    fn begin_with_max(ui: &Ui, id: Id, max_window: f32) -> Self {
        let chrome = ui.ctx().data(|d| d.get_temp::<f32>(id.with("dialog-chrome"))).unwrap_or(GUESS_CHROME);
        let max_content = max_window - chrome.clamp(0.0, 400.0);
        Self { id, max_content, top: ui.cursor().top(), body_bottom: None }
    }

    /// The height left for the body now that the header is drawn.
    pub fn body_max_height(&self, ui: &Ui) -> f32 {
        let footer = ui.ctx().data(|d| d.get_temp::<f32>(self.id.with("dialog-footer"))).unwrap_or(GUESS_FOOTER);
        let header = (ui.cursor().top() - self.top).max(0.0);
        (self.max_content - header - footer.clamp(0.0, 400.0)).max(MIN_BODY)
    }

    /// The middle of the dialog: scrolls vertically once it is taller than the room left.
    ///
    /// While the content fits (as measured on the previous frame) it is laid out directly, so the
    /// dialog has its exact height from the first frame (a scroll area sizes itself from the
    /// previous frame's content, which would make a just-opened dialog jump). Once it doesn't fit,
    /// a scroll area exactly the room left holds it.
    pub fn body<R>(&mut self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
        let max = self.body_max_height(ui);
        let key = self.id.with("dialog-body-height");
        let last = ui.ctx().data(|d| d.get_temp::<f32>(key));
        let (out, height) = if last.is_some_and(|h| h > max) {
            let out = egui::ScrollArea::vertical()
                .id_salt(self.id.with("dialog-body"))
                .max_height(max)
                .min_scrolled_height(max)
                // full width: no reflow when the scrollbar appears
                .auto_shrink([false, false])
                .show(ui, add);
            (out.inner, out.content_size.y)
        } else {
            let top = ui.cursor().top();
            let out = ui.scope(add).inner;
            (out, ui.min_rect().bottom() - top)
        };
        self.body_bottom = Some(ui.cursor().top());
        remember(ui.ctx(), key, height);
        out
    }

    /// The action buttons at the bottom; always on screen.
    pub fn footer<R>(&mut self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
        let start = self.body_bottom.unwrap_or_else(|| ui.cursor().top());
        let out = add(ui);
        let end = ui.min_rect().bottom();
        let (id, content) = (self.id, end - self.top);
        remember(ui.ctx(), id.with("dialog-footer"), (end - start).max(0.0));
        ui.ctx().data_mut(|d| d.insert_temp(id.with("dialog-content"), content.max(0.0)));
        out
    }

    /// Records the window chrome (title bar, margins) from the shown window, for the next frame.
    pub fn finish(ctx: &egui::Context, id: Id, window: Option<Rect>) {
        let Some(rect) = window else { return };
        let Some(content) = ctx.data(|d| d.get_temp::<f32>(id.with("dialog-content"))) else { return };
        let chrome = (rect.height() - content).max(0.0);
        if chrome.is_finite() {
            remember(ctx, id.with("dialog-chrome"), chrome);
        }
    }
}

/// Stores a measured height; a change lays the frame out again (no window jumping a frame later).
fn remember(ctx: &egui::Context, key: Id, value: f32) {
    let value = (value * 2.0).round() / 2.0;
    if ctx.data(|d| d.get_temp::<f32>(key)) != Some(value) {
        ctx.data_mut(|d| d.insert_temp(key, value));
        ctx.request_discard("dialog layout measured");
    }
}

// ------------------------------------------------------------------------------- Form building
//
// The look every dialog shares (Settings is the reference): titled **groups** drawn as subtle inset
// panels, a two-column **form** inside them (labels right-aligned in a fixed column, controls to
// their right), checkboxes and **help** text aligned to the control column, and, for dialogs with
// categories, a **sidebar** list on the left.

use egui::{Align, Layout, RichText, Shape, StrokeKind, layers::ShapeIdx, vec2};

use crate::theme::Tokens;

/// The label column of a form, in points (each form may set its own with [`form`]).
pub const LABEL_W: f32 = 150.0;
/// The gap between the label column and the control column.
pub const LABEL_GAP: f32 = 10.0;
/// How far a group's panel reaches past its content on each side.
pub const GROUP_PAD: f32 = 10.0;
/// The height of one form row.
const ROW_H: f32 = 24.0;

/// The label column width the form in `ui` uses (set by [`form`], else [`LABEL_W`]).
pub fn label_width(ui: &Ui) -> f32 {
    ui.ctx().data(|d| d.get_temp::<f32>(Id::new("dialog-form-label-w"))).unwrap_or(LABEL_W)
}

fn set_label_width(ui: &Ui, width: f32) {
    let width = if width.is_finite() { width.clamp(40.0, 400.0) } else { LABEL_W };
    ui.ctx().data_mut(|d| d.insert_temp(Id::new("dialog-form-label-w"), width));
}

/// The x offset of the control column from the form's left edge.
pub fn control_indent(ui: &Ui) -> f32 {
    label_width(ui) + LABEL_GAP
}

/// One form row: `label` right-aligned in the label column, then the control(s) `add` draws.
pub fn form_row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let w = label_width(ui);
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        let gap = ui.spacing().item_spacing.x;
        ui.allocate_ui_with_layout(vec2(w, ROW_H), Layout::right_to_left(Align::Center), |ui| {
            ui.set_min_width(w);
            if !label.is_empty() {
                ui.add(egui::Label::new(RichText::new(crate::i18n::tr(label)).color(t.text_label)).truncate());
            }
        });
        ui.add_space((LABEL_GAP - gap).max(0.0));
        // a row of many choices wraps inside the control column instead of widening the dialog
        let room = vec2(ui.available_width().max(40.0), ROW_H);
        ui.allocate_ui_with_layout(room, Layout::left_to_right(Align::Center).with_main_wrap(true), add).inner
    })
    .inner
}

/// Draws `add` in the control column (indented past the labels) when `ui` lays out top-down; inside
/// a row (left to right) it draws in place.
pub fn in_control_column<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !in_form(ui) || !ui.layout().main_dir().is_vertical() {
        return add(ui);
    }
    let indent = control_indent(ui);
    ui.horizontal_top(|ui| {
        ui.add_space(indent);
        ui.vertical(|ui| {
            // help text wraps at the panel's edge, not past it
            ui.set_max_width(ui.available_width());
            add(ui)
        })
        .inner
    })
    .inner
}

/// Help / description text: small and muted, in the control column.
pub fn help(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    let text = RichText::new(crate::i18n::tr(text)).size(11.0).color(t.text_dim);
    in_control_column(ui, |ui| {
        ui.add(egui::Label::new(text).wrap());
    });
}

/// A checkbox in the control column, addressable as `check:{id}`; true when toggled.
pub fn form_check(ui: &mut Ui, id: &str, value: &mut bool, label: &str) -> bool {
    let label = crate::i18n::tr(label).to_string();
    in_control_column(ui, |ui| {
        let r = ui.checkbox(value, label);
        crate::widgets::register(ui.ctx(), format!("check:{id}"), r.rect);
        r.changed()
    })
}

/// `ui.checkbox`, in the control column in a form.
pub fn checkbox(ui: &mut Ui, value: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    in_control_column(ui, |ui| ui.checkbox(value, text))
}

/// A note (`ui.label`), in the control column in a form.
pub fn note(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    in_control_column(ui, |ui| ui.label(text))
}

/// The open group of a form: the reserved panel shape, filled in when the group closes.
#[derive(Clone, Copy)]
struct OpenGroup {
    shape: ShapeIdx,
    top: f32,
    left: f32,
    right: f32,
}

fn group_key(ui: &Ui) -> Id {
    Id::new("dialog-open-group").with(ui.layer_id())
}

/// Starts a titled group: a heading, then an inset panel behind everything drawn until the next
/// [`group_begin`] or [`group_end`]. Lets a long, linear form be grouped without nesting closures
/// (Settings' tabs); use [`group`] where a closure is handy.
pub fn group_begin(ui: &mut Ui, title: Option<&str>) {
    group_end(ui);
    let t = Tokens::get(ui.ctx());
    if !in_form(ui) {
        if let Some(title) = title.filter(|s| !s.is_empty()) {
            ui.label(RichText::new(crate::i18n::tr(title)).font(t.semibold(12.0)).color(t.text));
        }
        return;
    }
    ui.add_space(4.0);
    if let Some(title) = title.filter(|s| !s.is_empty()) {
        ui.label(RichText::new(crate::i18n::tr(title)).font(t.semibold(12.0)).color(t.text));
    }
    let shape = ui.painter().add(Shape::Noop);
    let rect = ui.max_rect();
    let open = OpenGroup { shape, top: ui.cursor().top(), left: rect.left() - GROUP_PAD, right: rect.right() + GROUP_PAD };
    ui.ctx().data_mut(|d| d.insert_temp(group_key(ui), open));
    ui.add_space(4.0);
}

/// Closes the open group (if any), painting its panel.
pub fn group_end(ui: &mut Ui) {
    let key = group_key(ui);
    let Some(open) = ui.ctx().data(|d| d.get_temp::<OpenGroup>(key)) else { return };
    ui.ctx().data_mut(|d| d.remove::<OpenGroup>(key));
    // nothing drawn in it (a category that starts with a titled group): no empty panel
    if ui.cursor().top() <= open.top + 4.0 + ui.spacing().item_spacing.y + 0.5 {
        return;
    }
    ui.add_space(4.0);
    let t = Tokens::get(ui.ctx());
    let bottom = ui.cursor().top() - ui.spacing().item_spacing.y + 2.0;
    let rect = Rect::from_min_max(egui::pos2(open.left, open.top), egui::pos2(open.right, bottom.max(open.top)));
    let panel = egui::epaint::RectShape::new(rect, 4.0, t.inset, egui::Stroke::new(1.0, t.divider), StrokeKind::Inside);
    ui.painter().set(open.shape, panel);
    ui.add_space(6.0);
}

/// A titled group around `add` (see [`group_begin`]).
pub fn group<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    group_begin(ui, Some(title));
    let out = add(ui);
    group_end(ui);
    out
}

/// A form body: room on both sides for the group panels, and the label column width.
pub fn form<R>(ui: &mut Ui, label_w: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    set_label_width(ui, label_w);
    let active = Id::new("dialog-form-active");
    let outer = in_form(ui);
    ui.ctx().data_mut(|d| d.insert_temp(active, true));
    let pad = GROUP_PAD as i8 + 1;
    egui::Frame::NONE
        .inner_margin(egui::Margin { left: pad, right: pad, top: 0, bottom: 0 })
        .show(ui, |ui| {
            let out = add(ui);
            group_end(ui);
            ui.ctx().data_mut(|d| d.insert_temp(active, outer));
            out
        })
        .inner
}

/// Whether `ui` is drawing inside a [`form`] (its helpers then lay out as a form; elsewhere, e.g.
/// in a side panel, they keep to the panel's own flow).
pub fn in_form(ui: &Ui) -> bool {
    ui.ctx().data(|d| d.get_temp::<bool>(Id::new("dialog-form-active"))).unwrap_or(false)
}

/// The category list on the left of a dialog with sections (Settings): one full-width row per
/// `(id, label)`, the selected one highlighted, each addressable as `button:{prefix}-{id}`. True
/// when the selection changed.
pub fn sidebar(ui: &mut Ui, prefix: &str, items: &[(&str, &str)], selected: &mut String, width: f32) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut changed = false;
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 1.0;
        for (id, label) in items {
            let label = crate::i18n::tr(label);
            let on = selected == id;
            let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), egui::Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, label));
            crate::widgets::register(ui.ctx(), format!("button:{prefix}-{id}"), rect);
            let fill = if on {
                t.pressed
            } else if resp.hovered() {
                t.hover
            } else {
                egui::Color32::TRANSPARENT
            };
            let p = ui.painter();
            p.rect_filled(rect, 4.0, fill);
            if on {
                let bar = Rect::from_min_size(rect.left_top() + vec2(0.0, 5.0), vec2(3.0, rect.height() - 10.0));
                p.rect_filled(bar, 1.5, t.accent);
            }
            let font = if on { t.semibold(12.5) } else { egui::FontId::proportional(12.5) };
            let galley = p.layout_no_wrap(label.to_string(), font, if on { t.text } else { t.text_label });
            let pos = egui::pos2(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0);
            p.with_clip_rect(rect.intersect(p.clip_rect())).galley(pos, galley, t.text);
            if resp.clicked() && !on {
                *selected = id.to_string();
                changed = true;
            }
        }
    });
    changed
}

/// A thin rule across the dialog above its footer buttons.
pub fn footer_rule(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width().max(ui.min_rect().width());
    let (rect, _) = ui.allocate_exact_size(vec2(w, 1.0), egui::Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, t.divider));
}
