//! Visual design system.
//!
//! One palette, one type scale, one set of spacing rules, and a handful of small
//! widgets (`section`, `field`, `pill`, …) so that every panel is built from the
//! same vocabulary instead of ad-hoc `ui.label` + fixed-width widget pairs.
//!
//! egui's stock theme is deliberately neutral; this module gives the app a
//! layered dark surface (canvas → panel → card → control) with a single accent.

use std::cell::Cell;

use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Layout, Margin, Response, Rounding, Stroke, Vec2,
};

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Palette {
    /// Deepest surface: the node canvas.
    pub bg: Color32,
    /// Side panels / toolbars.
    pub panel: Color32,
    /// Cards, buttons, input fields.
    pub surface: Color32,
    /// Hover state for cards and buttons.
    pub surface_hi: Color32,
    pub stroke: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub err: Color32,
    pub video: Color32,
    pub audio: Color32,
    pub file: Color32,
}

impl Palette {
    /// shadcn "zinc" dark. Surfaces step up in tiny increments and the border
    /// is only a hair lighter than the fill, which is what keeps the UI from
    /// looking like a table of boxes.
    pub const DARK: Self = Self {
        bg: Color32::from_rgb(0x09, 0x09, 0x0B),
        panel: Color32::from_rgb(0x0C, 0x0C, 0x0F),
        surface: Color32::from_rgb(0x18, 0x18, 0x1B),
        surface_hi: Color32::from_rgb(0x23, 0x23, 0x27),
        stroke: Color32::from_rgb(0x2A, 0x2A, 0x2E),
        text: Color32::from_rgb(0xFA, 0xFA, 0xFA),
        dim: Color32::from_rgb(0xA1, 0xA1, 0xAA),
        faint: Color32::from_rgb(0x71, 0x71, 0x7A),
        accent: Color32::from_rgb(0x3B, 0x82, 0xF6),
        accent_soft: Color32::from_rgb(0x1B, 0x2A, 0x44),
        ok: Color32::from_rgb(0x22, 0xC5, 0x5E),
        warn: Color32::from_rgb(0xEA, 0xB3, 0x08),
        err: Color32::from_rgb(0xEF, 0x44, 0x44),
        video: Color32::from_rgb(0xF9, 0x73, 0x16),
        audio: Color32::from_rgb(0x22, 0xC5, 0x5E),
        file: Color32::from_rgb(0x60, 0xA5, 0xFA),
    };

    /// shadcn "zinc" light.
    pub const LIGHT: Self = Self {
        bg: Color32::from_rgb(0xF4, 0xF4, 0xF5),
        panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        surface: Color32::from_rgb(0xF4, 0xF4, 0xF5),
        surface_hi: Color32::from_rgb(0xE9, 0xE9, 0xEB),
        stroke: Color32::from_rgb(0xE4, 0xE4, 0xE7),
        text: Color32::from_rgb(0x09, 0x09, 0x0B),
        dim: Color32::from_rgb(0x52, 0x52, 0x5B),
        faint: Color32::from_rgb(0x8B, 0x8B, 0x93),
        accent: Color32::from_rgb(0x25, 0x63, 0xEB),
        accent_soft: Color32::from_rgb(0xDB, 0xEA, 0xFE),
        ok: Color32::from_rgb(0x16, 0xA3, 0x4A),
        warn: Color32::from_rgb(0xCA, 0x8A, 0x04),
        err: Color32::from_rgb(0xDC, 0x26, 0x26),
        video: Color32::from_rgb(0xEA, 0x58, 0x0C),
        audio: Color32::from_rgb(0x16, 0xA3, 0x4A),
        file: Color32::from_rgb(0x25, 0x63, 0xEB),
    };
}

thread_local! {
    static CURRENT: Cell<Palette> = const { Cell::new(Palette::DARK) };
}

/// The palette currently in effect.
pub fn p() -> Palette {
    CURRENT.with(|c| c.get())
}

fn set_palette(dark: bool) {
    CURRENT.with(|c| c.set(if dark { Palette::DARK } else { Palette::LIGHT }));
}

// ---------------------------------------------------------------------------
// Semantic colours
// ---------------------------------------------------------------------------

/// Blend two colours in gamma space (`t = 0` → `a`, `t = 1` → `b`), keeping the
/// result fully opaque.
///
/// Deliberately not `Color32::gamma_multiply`: that one scales **alpha** as well,
/// so a "darkened" colour comes out translucent. Node titlebars are drawn over
/// the near-white outline `egui_node_graph2` puts behind every node, so a
/// translucent titlebar gets washed out into a pale pastel in both themes.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

pub fn port_color(t: crate::graph::StreamType) -> Color32 {
    use crate::graph::StreamType::*;
    let p = p();
    match t {
        File => p.file,
        Video => p.video,
        Audio => p.audio,
        Any => p.warn,
    }
}

/// Accent colour for a node-library category.
pub fn category_color(category: &str) -> Color32 {
    let p = p();
    match category {
        "Input" => p.file,
        "Filters" => Color32::from_rgb(0xB4, 0x82, 0xF2),
        "Encoders" => p.video,
        "Output" => p.ok,
        _ => p.dim,
    }
}

/// Colour that identifies a node by the *media type* it works on, so the video
/// branch (orange) and the audio branch (green) are distinguishable at a glance
/// on the canvas, in the library and in the inspector.
pub fn node_color(kind: crate::graph::NodeKind) -> Color32 {
    use crate::graph::nodes::Family;
    let p = p();
    match kind.family() {
        Family::File => p.file,
        Family::Video => p.video,
        Family::Audio => p.audio,
        Family::Logic => p.warn,
        Family::Output => p.ok,
    }
}

// ---------------------------------------------------------------------------
// Theme application
// ---------------------------------------------------------------------------

pub const LABEL_W: f32 = 92.0;
pub const ROUND: f32 = 6.0;

// ---------------------------------------------------------------------------
// Type scale
// ---------------------------------------------------------------------------
//
// Every text size in the UI comes from here. Panels used to mix nine different
// ad-hoc sizes (9, 9.5, 10, 10.5, 11, 11.5, 12, 13, 15), which is what made the
// layout feel uneven from one row to the next.

/// Status bullet glyphs (`•`).
pub const DOT: f32 = 9.0;
/// Uppercase section headers and table column heads.
pub const CAPTION: f32 = 10.0;
/// Meta text, hints, secondary rows.
pub const SMALL: f32 = 11.0;
/// Monospace: command preview, error output.
pub const MONO: f32 = 12.0;
/// Compact body text: table cells, paths, notes.
pub const BODY: f32 = 12.5;
/// Default control text (buttons, inputs).
pub const BUTTON: f32 = 13.0;
/// Panel and node titles.
pub const TITLE: f32 = 15.0;
/// Large `Heading` style, used sparingly.
pub const HEADING: f32 = 17.0;
/// Empty-state icons.
pub const ICON: f32 = 18.0;

pub fn apply(ctx: &egui::Context, dark: bool) {
    set_palette(dark);
    let p = p();

    let mut style = (*ctx.style()).clone();

    style.text_styles = [
        (egui::TextStyle::Heading, FontId::new(HEADING, FontFamily::Proportional)),
        (egui::TextStyle::Body, FontId::new(BODY, FontFamily::Proportional)),
        (egui::TextStyle::Button, FontId::new(BUTTON, FontFamily::Proportional)),
        (egui::TextStyle::Small, FontId::new(SMALL, FontFamily::Proportional)),
        (egui::TextStyle::Monospace, FontId::new(MONO, FontFamily::Monospace)),
    ]
    .into();

    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 4.0);
    style.spacing.interact_size.y = 22.0;
    style.spacing.window_margin = Margin::same(10.0);
    style.spacing.menu_margin = Margin::same(6.0);
    style.spacing.indent = 16.0;
    style.spacing.combo_width = 130.0;
    style.spacing.text_edit_width = 150.0;
    style.spacing.slider_width = 110.0;
    style.spacing.tooltip_width = 320.0;
    style.animation_time = 0.10;

    // Scroll bars: a thin pill with no trough. egui's default paints the
    // trough with `extreme_bg_color`, which in this palette is the darkest
    // colour in the app — a 12 px near-black band down the edge of every panel
    // that reads as a border, not a scrollbar. Zeroing the background opacities
    // keeps only the handle.
    style.spacing.scroll = egui::style::ScrollStyle {
        floating: true,
        bar_width: 7.0,
        floating_width: 4.0,
        floating_allocated_width: 0.0,
        bar_inner_margin: 2.0,
        bar_outer_margin: 2.0,
        handle_min_length: 20.0,
        foreground_color: true,
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.0,
        interact_background_opacity: 0.0,
        dormant_handle_opacity: 0.22,
        active_handle_opacity: 0.40,
        interact_handle_opacity: 0.65,
    };

    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.dark_mode = dark;
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = if dark {
        Color32::from_rgb(0x0E, 0x0E, 0x11)
    } else {
        Color32::from_rgb(0xFA, 0xFA, 0xFA)
    };
    v.faint_bg_color = p.surface;
    v.code_bg_color = if dark {
        Color32::from_rgb(0x0E, 0x0E, 0x11)
    } else {
        Color32::from_rgb(0xF4, 0xF4, 0xF5)
    };
    v.window_stroke = Stroke::new(1.0, p.stroke);
    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);
    v.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 6.0),
        blur: 18.0,
        spread: 0.0,
        color: Color32::from_black_alpha(120),
    };
    v.popup_shadow = v.window_shadow;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.err;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.striped = false;
    v.slider_trailing_fill = true;
    v.clip_rect_margin = 2.0;
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);

    // egui makes every `Label` sense `click_and_drag` by default so text can be
    // selected. In a node graph that is fatal: the node title is a `Label`, and
    // it sits on top of the node's own drag handle, so grabbing a node by its
    // titlebar would start a text selection instead of moving the node. Turning
    // this off is the only lever we have, because the title label belongs to
    // `egui_node_graph2`.
    style.interaction.selectable_labels = false;

    let rounding = Rounding::same(ROUND);
    // Borders stay hairline: the same colour is used for every control, and it
    // is only a shade off the fill, so a row of buttons reads as one surface
    // rather than a stack of outlined boxes.
    let idle_stroke = Stroke::new(1.0, p.stroke);
    v.widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: p.panel,
        weak_bg_fill: p.panel,
        bg_stroke: Stroke::NONE,
        rounding,
        fg_stroke: Stroke::new(1.0, p.text),
        expansion: 0.0,
    };
    v.widgets.inactive = egui::style::WidgetVisuals {
        bg_fill: p.surface,
        weak_bg_fill: p.surface,
        bg_stroke: idle_stroke,
        rounding,
        fg_stroke: Stroke::new(1.0, p.text),
        expansion: 0.0,
    };
    v.widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: p.surface_hi,
        weak_bg_fill: p.surface_hi,
        bg_stroke: Stroke::new(1.0, p.surface_hi),
        rounding,
        fg_stroke: Stroke::new(1.0, p.text),
        expansion: 0.0,
    };
    v.widgets.active = egui::style::WidgetVisuals {
        bg_fill: p.accent_soft,
        weak_bg_fill: p.accent_soft,
        bg_stroke: Stroke::new(1.0, p.accent.gamma_multiply(0.7)),
        rounding,
        fg_stroke: Stroke::new(1.0, p.text),
        expansion: 0.0,
    };
    v.widgets.open = v.widgets.hovered;

    style.visuals = v;
    ctx.set_style(style);
}

// ---------------------------------------------------------------------------
// Small widgets
// ---------------------------------------------------------------------------

/// Small uppercase caption that introduces a group of controls.
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(CAPTION)
            .strong()
            .color(p().faint),
    );
}

/// Two-column form row: fixed label gutter on the left, control on the right.
///
/// The label is painted rather than laid out so rows line up regardless of text
/// length, which is what made the old forms look ragged.
pub fn field(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let row_h = ui.spacing().interact_size.y.max(20.0);
    // Measure before the row starts: inside a horizontal `Ui` egui reports the
    // *remaining* width of the parent, which would make wrapped controls break
    // far too early.
    let control_w = (ui.available_width() - LABEL_W - ui.spacing().item_spacing.x).max(40.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(LABEL_W, row_h), egui::Sense::hover());
        ui.painter().text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.0),
            p().dim,
        );
        ui.allocate_ui_with_layout(
            Vec2::new(control_w, row_h),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_width(control_w);
                add(ui);
            },
        );
    });
}

/// Rounded label chip, used for statuses and counts.
pub fn pill(ui: &mut egui::Ui, text: impl Into<String>, color: Color32) -> Response {
    let text = text.into();
    let font = FontId::proportional(11.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text, font, color);
    let pad = Vec2::new(7.0, 2.5);
    let size = galley.size() + pad * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, Rounding::same(size.y * 0.5), color.gamma_multiply(0.16));
        ui.painter()
            .galley(rect.min + pad, galley, color);
    }
    resp
}

/// Height shared by every button, so a toolbar row never looks ragged.
pub const BUTTON_H: f32 = 26.0;

/// The one filled button per view (Run).
pub fn primary_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let p = p();
    let text = text.into();
    let enabled = ui.is_enabled();
    let fg = if enabled { Color32::WHITE } else { p.faint };
    let galley = ui
        .painter()
        .layout_no_wrap(text, FontId::proportional(13.0), fg);
    let pad = Vec2::new(14.0, 6.0);
    let size = Vec2::new(galley.size().x + pad.x * 2.0, BUTTON_H);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let fill = if !enabled {
            p.surface
        } else if resp.is_pointer_button_down_on() {
            p.accent.gamma_multiply(0.75)
        } else if resp.hovered() {
            p.accent.gamma_multiply(1.12)
        } else {
            p.accent
        };
        ui.painter().rect_filled(rect, Rounding::same(ROUND), fill);
        ui.painter().galley(rect.center() - galley.size() * 0.5, galley, fg);
    }
    resp
}

/// Muted button for secondary actions.
///
/// Painted by hand rather than built from `egui::Button`: the stock widget's
/// height follows the text metrics and the style padding, so it sat a few
/// pixels shorter than `primary_button` and the toolbar row came out uneven.
pub fn ghost_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let p = p();
    let text = text.into();
    let fg = if ui.is_enabled() { p.text } else { p.faint };
    let galley = ui
        .painter()
        .layout_no_wrap(text, FontId::proportional(BUTTON), fg);
    let size = Vec2::new(galley.size().x + 20.0, BUTTON_H);
    let sense = if ui.is_enabled() {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let fill = if resp.is_pointer_button_down_on() {
            p.surface_hi
        } else if resp.hovered() {
            p.surface_hi
        } else {
            p.surface
        };
        ui.painter().rect_filled(rect, Rounding::same(ROUND), fill);
        ui.painter().galley(rect.center() - galley.size() * 0.5, galley, fg);
    }
    resp
}

/// Danger-tinted button (cancel).
pub fn danger_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let p = p();
    let text = text.into();
    let enabled = ui.is_enabled();
    let galley = ui.painter().layout_no_wrap(
        text,
        FontId::proportional(13.0),
        if enabled { p.err } else { p.faint },
    );
    let pad = Vec2::new(12.0, 6.0);
    let size = Vec2::new(galley.size().x + pad.x * 2.0, BUTTON_H);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let bg = if !enabled {
            p.surface
        } else if resp.hovered() {
            p.err.gamma_multiply(0.28)
        } else {
            p.err.gamma_multiply(0.14)
        };
        ui.painter().rect_filled(rect, Rounding::same(ROUND), bg);
        ui.painter().galley(rect.center() - galley.size() * 0.5, galley, p.err);
    }
    resp
}

/// Thin horizontal rule.
pub fn rule(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let rect = ui.available_rect_before_wrap();
    let y = rect.top();
    ui.painter().hline(
        rect.left()..=rect.right(),
        y,
        Stroke::new(1.0, p().stroke),
    );
    ui.add_space(5.0);
}

/// Progress bar with the label centred by hand.
///
/// egui's `ProgressBar` aligns its text with the *current* layout, which puts it
/// hard against the left edge inside a `horizontal` row. This one always centres.
pub fn progress(
    ui: &mut egui::Ui,
    width: f32,
    pct: f32,
    color: Color32,
    label: impl Into<String>,
) {
    let p = p();
    let height = 15.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let radius = Rounding::same(height * 0.5);
    ui.painter().rect_filled(rect, radius, p.bg);
    let frac = (pct / 100.0).clamp(0.0, 1.0);
    if frac > 0.0 {
        let mut fill = rect;
        fill.set_width((rect.width() * frac).max(height));
        ui.painter().rect_filled(fill, radius, color.gamma_multiply(0.85));
    }
    // The label sits in the middle: dark on the filled part, light on the empty
    // track. The light theme always uses dark text, because its track is pale.
    let on_fill = Color32::from_rgb(0x0E, 0x12, 0x18);
    let on_track = Color32::from_rgb(0xC6, 0xD0, 0xDE);
    let text_color = if p.bg.r() > 0x80 {
        on_fill
    } else if frac >= 0.5 {
        on_fill
    } else {
        on_track
    };
    let galley =
        ui.painter()
            .layout_no_wrap(label.into(), FontId::proportional(10.0), text_color);
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, p.text);
}

/// Card container used by the node library and the file list.
///
/// Returns the response of the *whole* card, so callers can make the entire
/// surface clickable/draggable instead of just the text inside it.
/// A clickable card. `id` must be unique for the whole frame: the cards are
/// addressed by name elsewhere (tests, and the drag ghost), so it is built from
/// the caller's salt rather than from the parent `Ui`'s id.
pub fn card(
    ui: &mut egui::Ui,
    selected: bool,
    id: egui::Id,
    sense: egui::Sense,
    add: impl FnOnce(&mut egui::Ui),
) -> Response {
    let p = p();
    // The background is reserved before the content is laid out so hover can be
    // painted *under* it; the card itself has no outline unless it is selected.
    let bg = ui.painter().add(egui::Shape::Noop);
    let frame = egui::Frame::none().inner_margin(Margin::symmetric(8.0, 4.0));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    let rect = inner.response.rect;
    let resp = ui.interact(rect, id, sense);
    let fill = if selected {
        p.accent_soft
    } else if resp.hovered() {
        p.surface_hi
    } else {
        p.surface
    };
    let stroke = if selected {
        Stroke::new(1.0, p.accent)
    } else {
        Stroke::NONE
    };
    ui.painter().set(
        bg,
        egui::epaint::RectShape::new(rect, Rounding::same(ROUND), fill, stroke),
    );
    resp
}

/// A left colour bar, used to tie a card to its category.
pub fn accent_bar(ui: &mut egui::Ui, color: Color32, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, Rounding::same(width * 0.5), color);
    }
}

/// Right-aligned helper for the toolbar: everything added inside is laid out
/// from the right edge inwards.
pub fn right_aligned<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(Layout::right_to_left(Align::Center), add)
        .inner
}

/// Empty-state block: a title plus a hint, vertically centred in what is left.
pub fn empty_state(ui: &mut egui::Ui, icon: &str, title: &str, hint: &str) {
    let p = p();
    // Short panels (the task list once the window is squeezed) drop the icon
    // and the padding instead of clipping the hint line.
    let compact = ui.available_height() < 84.0;
    ui.vertical_centered(|ui| {
        if !compact {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(icon).size(ICON).color(p.faint));
            ui.add_space(3.0);
        }
        ui.label(egui::RichText::new(title).size(BODY).color(p.dim));
        ui.add_space(1.0);
        ui.label(egui::RichText::new(hint).size(SMALL).color(p.faint));
        ui.add_space(8.0);
    });
}

/// Fade the bottom `height` of a scrolling list into the panel colour.
///
/// A scroll area clips at its edge, so the last row is always sliced in half
/// and the panel border reads as a cut. A short gradient makes it read as
/// "there is more below" instead, the way shadcn scroll shadows do.
pub fn fade_bottom(ui: &egui::Ui, rect: egui::Rect, height: f32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let p = p();
    let h = height.min(rect.height());
    let fade = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - h),
        rect.right_bottom(),
    );
    let clear = Color32::TRANSPARENT;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(fade.left_top(), clear);
    mesh.colored_vertex(fade.right_top(), clear);
    mesh.colored_vertex(fade.right_bottom(), p.panel);
    mesh.colored_vertex(fade.left_bottom(), p.panel);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}
