//! Visual design system.
//!
//! One palette, one type scale, one spacing grid, and a handful of small
//! widgets (`section`, `field`, `pill`, `card`, `empty_state`, …) so that every
//! panel is built from the same vocabulary instead of ad-hoc labels and
//! hand-picked sizes.
//!
//! Rules this module enforces:
//!
//! * **Grey carries the hierarchy.** The canvas is the darkest surface, panels
//!   step up one shade, cards one more. Colour is *accent only*: a 3 px stripe,
//!   an icon, a port dot, a status pill — never a large filled block.
//! * **An 8 px grid.** Panel padding, control height, card padding and list
//!   gaps are multiples of four, and the common step is eight, so rows line up
//!   across panels without per-widget tweaks.
//! * **A short type ladder.** 10 / 11 / 12 / 13 / 14 / 17 px. Nothing else.
//! * **One icon family.** [`Icon`] is drawn as line art on a 16×16 grid with a
//!   1.5 px stroke, so no emoji and no tofu boxes can slip in.

use std::cell::Cell;

use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Layout, Margin, Painter, Pos2, Rect, Response,
    Rounding, Stroke, Vec2,
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
    /// Inputs, secondary buttons.
    pub surface: Color32,
    /// Hover state for cards and buttons.
    pub surface_hi: Color32,
    /// Node card body.
    pub card: Color32,
    /// Node card title bar (one shade above the body).
    pub card_top: Color32,
    /// Code / command preview well.
    pub code_bg: Color32,
    pub stroke: Color32,
    /// Hairline around elevated surfaces (cards, windows): 1 px, translucent.
    pub hairline: Color32,
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
    /// Command-preview syntax colours. All are ≥ 4.5:1 on `code_bg`.
    pub code_flag: Color32,
    pub code_value: Color32,
    pub code_path: Color32,
    pub code_filter: Color32,
    pub code_output: Color32,
}

impl Palette {
    /// Dark: layered greys, one blue accent. Card surfaces sit around
    /// `#1E1E22` so a node reads as a raised panel, not a hole in the canvas.
    pub const DARK: Self = Self {
        bg: Color32::from_rgb(0x08, 0x08, 0x0A),
        panel: Color32::from_rgb(0x10, 0x10, 0x13),
        surface: Color32::from_rgb(0x19, 0x19, 0x1D),
        surface_hi: Color32::from_rgb(0x25, 0x25, 0x2B),
        card: Color32::from_rgb(0x1E, 0x1E, 0x22),
        card_top: Color32::from_rgb(0x26, 0x26, 0x2C),
        code_bg: Color32::from_rgb(0x0D, 0x0D, 0x11),
        stroke: Color32::from_rgb(0x2B, 0x2B, 0x31),
        hairline: Color32::from_rgba_premultiplied(0x1F, 0x1F, 0x22, 0x1F),
        text: Color32::from_rgb(0xF4, 0xF4, 0xF7),
        dim: Color32::from_rgb(0xA6, 0xA6, 0xB0),
        faint: Color32::from_rgb(0x7C, 0x7C, 0x88),
        accent: Color32::from_rgb(0x4C, 0x8D, 0xFF),
        accent_soft: Color32::from_rgb(0x1A, 0x28, 0x44),
        ok: Color32::from_rgb(0x4A, 0xDE, 0x80),
        warn: Color32::from_rgb(0xF5, 0xC2, 0x42),
        err: Color32::from_rgb(0xFF, 0x6B, 0x6B),
        video: Color32::from_rgb(0xFB, 0x92, 0x3C),
        audio: Color32::from_rgb(0x45, 0xD4, 0x9A),
        file: Color32::from_rgb(0x6C, 0xAE, 0xFF),
        code_flag: Color32::from_rgb(0x8A, 0xB4, 0xFF),
        code_value: Color32::from_rgb(0x7E, 0xE7, 0xD8),
        code_path: Color32::from_rgb(0xB8, 0xE9, 0x86),
        code_filter: Color32::from_rgb(0xD0, 0xA8, 0xFF),
        code_output: Color32::from_rgb(0x9C, 0xE8, 0x7A),
    };

    /// Light: white panels, warm-grey canvas.
    pub const LIGHT: Self = Self {
        bg: Color32::from_rgb(0xEF, 0xEF, 0xF2),
        panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        surface: Color32::from_rgb(0xF4, 0xF4, 0xF6),
        surface_hi: Color32::from_rgb(0xE7, 0xE7, 0xEB),
        card: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        card_top: Color32::from_rgb(0xF7, 0xF7, 0xF9),
        code_bg: Color32::from_rgb(0xF7, 0xF7, 0xFA),
        stroke: Color32::from_rgb(0xE2, 0xE2, 0xE8),
        hairline: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x14),
        text: Color32::from_rgb(0x14, 0x14, 0x18),
        dim: Color32::from_rgb(0x52, 0x52, 0x5C),
        faint: Color32::from_rgb(0x86, 0x86, 0x91),
        accent: Color32::from_rgb(0x25, 0x63, 0xEB),
        accent_soft: Color32::from_rgb(0xDE, 0xEA, 0xFE),
        ok: Color32::from_rgb(0x15, 0x8A, 0x3E),
        warn: Color32::from_rgb(0xA1, 0x62, 0x00),
        err: Color32::from_rgb(0xC8, 0x1F, 0x1F),
        video: Color32::from_rgb(0xC2, 0x4C, 0x08),
        audio: Color32::from_rgb(0x10, 0x80, 0x52),
        file: Color32::from_rgb(0x1D, 0x4E, 0xD8),
        code_flag: Color32::from_rgb(0x1D, 0x4E, 0xD8),
        code_value: Color32::from_rgb(0x0B, 0x6A, 0x66),
        code_path: Color32::from_rgb(0x2F, 0x6F, 0x1A),
        code_filter: Color32::from_rgb(0x6B, 0x2D, 0xB6),
        code_output: Color32::from_rgb(0x24, 0x6B, 0x16),
    };
}

thread_local! {
    static CURRENT: Cell<Palette> = const { Cell::new(Palette::DARK) };
}

/// The palette currently in effect.
pub fn p() -> Palette {
    CURRENT.with(|c| c.get())
}

pub fn is_dark() -> bool {
    p().bg.r() < 0x80
}

fn set_palette(dark: bool) {
    CURRENT.with(|c| c.set(if dark { Palette::DARK } else { Palette::LIGHT }));
}

/// Blend two colours, keeping the result opaque.
///
/// Deliberately not `Color32::gamma_multiply`: that one scales **alpha** too,
/// so a "darkened" colour comes out translucent and a titlebar over a pale
/// outline washes out into pastel.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// Same colour at a given alpha, useful for hairlines and soft fills.
pub fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

// ---------------------------------------------------------------------------
// Semantic colours
// ---------------------------------------------------------------------------

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

/// Colour that identifies a node by the *media type* it works on. Used for the
/// icon, the card's titlebar tint and the port dots — never for a filled
/// background or a border stripe.
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
// Metrics
// ---------------------------------------------------------------------------

/// The spacing unit everything else is a multiple of.
pub const GAP: f32 = 8.0;
/// Corner radius for controls (buttons, inputs, chips).
pub const ROUND: f32 = 6.0;
/// Corner radius for cards and panels.
pub const ROUND_CARD: f32 = 8.0;
/// Height shared by every toolbar/panel button.
pub const BUTTON_H: f32 = 28.0;
/// Connection stroke width, in graph px (the canvas zoom scales it).
///
/// 2.0 was the first pass and read a touch thin next to the 8 px cards; 2.5
/// keeps the line visible at 50% zoom without turning into a cable.
pub const WIRE: f32 = 2.5;
/// Gutter between a form's label and its control.
pub const LABEL_W: f32 = 92.0;

/// Scale factor of the `Ui` currently being laid out.
///
/// `egui_node_graph2` zooms by swapping in a `Style::scaled(zoom)`; widgets
/// that paint by hand (node buttons, for example) run in screen pixels and
/// must read the factor back out of the style instead of assuming 1.0.
pub fn ui_zoom(ui: &egui::Ui) -> f32 {
    let body = egui::TextStyle::Body.resolve(ui.style()).size;
    (body / BODY).clamp(0.1, 8.0)
}

// ---------------------------------------------------------------------------
// Type scale
// ---------------------------------------------------------------------------
//
// A fixed ladder, no exceptions: 10 · 11 · 12 · 13 · 14 · 17.

/// Status bullets and dot glyphs.
pub const DOT: f32 = 9.0;
/// Uppercase section headers, table heads, badges.
pub const CAPTION: f32 = 10.0;
/// Hints, meta rows, secondary detail.
pub const SMALL: f32 = 11.0;
/// Code, argv tokens, error output.
pub const MONO: f32 = 12.0;
/// Default text.
pub const BODY: f32 = 13.0;
/// Buttons and inputs (same size as body: weight, not size, separates them).
pub const BUTTON: f32 = 13.0;
/// Panel titles and node headers.
pub const TITLE: f32 = 14.0;
/// Reserved for the help window title.
pub const HEADING: f32 = 17.0;

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

    // 8 px grid.
    style.spacing.item_spacing = Vec2::new(GAP, GAP);
    style.spacing.button_padding = Vec2::new(12.0, 5.0);
    style.spacing.interact_size.y = 24.0;
    style.spacing.window_margin = Margin::same(12.0);
    style.spacing.menu_margin = Margin::same(6.0);
    style.spacing.indent = 16.0;
    style.spacing.combo_width = 132.0;
    style.spacing.text_edit_width = 152.0;
    style.spacing.slider_width = 112.0;
    style.spacing.tooltip_width = 320.0;
    style.animation_time = 0.10;

    // Scroll bars: a thin floating pill, no trough. egui's default paints the
    // trough with `extreme_bg_color`, the darkest colour in the app, which read
    // as a border down the edge of every panel.
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
    v.extreme_bg_color = p.code_bg;
    v.faint_bg_color = p.card;
    v.code_bg_color = p.code_bg;
    // `window_stroke` is also the hairline `egui_node_graph2` puts around a node
    // card (see the vendor patch), so a card and a dialog share one border.
    v.window_stroke = Stroke::new(1.0, p.hairline);
    v.window_rounding = Rounding::same(ROUND_CARD);
    v.menu_rounding = Rounding::same(ROUND_CARD);
    v.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 8.0),
        blur: 24.0,
        spread: 0.0,
        color: Color32::from_black_alpha(140),
    };
    v.popup_shadow = v.window_shadow;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.err;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.30);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.striped = false;
    v.slider_trailing_fill = true;
    v.clip_rect_margin = 2.0;
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);

    // egui makes every `Label` sense `click_and_drag` so text can be selected.
    // In a node graph that is fatal: the node title is a `Label` sitting on the
    // node's drag handle, so grabbing a titlebar started a text selection
    // instead of moving the node. The dependency owns that label, so the only
    // lever is global.
    style.interaction.selectable_labels = false;

    let rounding = Rounding::same(ROUND);
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
        bg_stroke: Stroke::new(1.0, p.stroke),
        rounding,
        fg_stroke: Stroke::new(1.0, p.text),
        expansion: 0.0,
    };
    v.widgets.active = egui::style::WidgetVisuals {
        bg_fill: p.accent_soft,
        weak_bg_fill: p.accent_soft,
        bg_stroke: Stroke::new(1.0, p.accent.gamma_multiply(0.8)),
        rounding,
        fg_stroke: Stroke::new(1.0, p.text),
        expansion: 0.0,
    };
    v.widgets.open = v.widgets.hovered;

    style.visuals = v;
    ctx.set_style(style);
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

/// Line-art icons, drawn on a 16×16 grid with a 1.5 px stroke.
///
/// A hand-drawn set rather than an icon font: the bundled fonts have no such
/// glyphs (anything outside Ubuntu-Light + NotoEmoji renders as tofu), and a
/// consistent stroke width is the whole point of the exercise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Folder,
    Split,
    Resize,
    Frame,
    Crop,
    Rotate,
    Clock,
    Sliders,
    Layers,
    Sparkle,
    Cloud,
    Pin,
    Speaker,
    Wave,
    Funnel,
    Gauge,
    Headphones,
    Steps,
    Code,
    Branch,
    Camera,
    Music,
    Package,
    Gear,
    Play,
    Files,
    Search,
    Pointer,
}

/// Paint `icon` centred in `rect`.
pub fn icon(painter: &Painter, rect: Rect, which: Icon, color: Color32) {
    // One stroke width for the whole set, in screen pixels.
    let stroke = Stroke::new((rect.width() / 16.0 * 1.5).max(1.0), color);
    icon_on(painter, rect, which, stroke);
}

fn icon_on(painter: &Painter, rect: Rect, which: Icon, stroke: Stroke) {
    let size = rect.width().min(rect.height());
    let s = size / 16.0;
    // Design-space (0..16) → screen, centred in a non-square rect.
    let origin = rect.min
        + Vec2::new((rect.width() - size) * 0.5, (rect.height() - size) * 0.5);
    let m = |x: f32, y: f32| -> Pos2 { origin + Vec2::new(x * s, y * s) };
    let line = |a: (f32, f32), b: (f32, f32)| {
        painter.line_segment([m(a.0, a.1), m(b.0, b.1)], stroke);
    };
    let path = |pts: &[(f32, f32)], closed: bool| {
        let mut v: Vec<Pos2> = pts.iter().map(|&(x, y)| m(x, y)).collect();
        if closed {
            if let Some(&first) = v.first() {
                v.push(first);
            }
        }
        painter.add(egui::Shape::line(v, stroke));
    };
    let circle = |c: (f32, f32), r: f32| {
        painter.circle_stroke(m(c.0, c.1), r * s, stroke);
    };
    let dot = |c: (f32, f32)| {
        painter.circle_filled(m(c.0, c.1), 1.1 * s, stroke.color);
    };
    let rect_at = |x: f32, y: f32, w: f32, h: f32, r: f32| {
        painter.rect_stroke(
            Rect::from_min_size(m(x, y), Vec2::new(w * s, h * s)),
            Rounding::same(r * s),
            stroke,
        );
    };
    let radius = 2.0;

    match which {
        Icon::Folder => {
            path(
                &[
                    (1.5, 4.0),
                    (6.0, 4.0),
                    (7.5, 6.0),
                    (14.5, 6.0),
                    (14.5, 13.0),
                    (1.5, 13.0),
                ],
                true,
            );
        }
        Icon::Split => {
            line((1.5, 8.0), (8.0, 8.0));
            path(&[(8.0, 8.0), (12.0, 4.0), (14.0, 4.0)], false);
            path(&[(8.0, 8.0), (12.0, 12.0), (14.0, 12.0)], false);
            dot((1.5, 8.0));
        }
        Icon::Resize => {
            rect_at(2.0, 3.0, 12.0, 10.0, radius);
            line((5.5, 10.5), (10.5, 5.5));
            path(&[(7.0, 5.5), (10.5, 5.5), (10.5, 9.0)], false);
        }
        Icon::Frame => {
            path(&[(2.0, 6.0), (2.0, 2.0), (6.0, 2.0)], false);
            path(&[(10.0, 2.0), (14.0, 2.0), (14.0, 6.0)], false);
            path(&[(14.0, 10.0), (14.0, 14.0), (10.0, 14.0)], false);
            path(&[(6.0, 14.0), (2.0, 14.0), (2.0, 10.0)], false);
        }
        Icon::Crop => {
            path(&[(5.0, 1.5), (5.0, 11.0), (14.5, 11.0)], false);
            path(&[(1.5, 5.0), (11.0, 5.0), (11.0, 14.5)], false);
        }
        Icon::Rotate => {
            path(
                &[
                    (13.0, 8.0),
                    (12.2, 5.2),
                    (10.2, 3.4),
                    (7.4, 3.0),
                    (4.8, 4.2),
                    (3.2, 6.6),
                    (3.2, 9.4),
                    (4.8, 11.8),
                    (7.4, 13.0),
                    (10.2, 12.8),
                    (12.4, 11.2),
                ],
                false,
            );
            path(&[(13.0, 8.0), (11.0, 6.2), (14.6, 5.4)], false);
        }
        Icon::Clock => {
            circle((8.0, 8.0), 6.0);
            path(&[(8.0, 4.5), (8.0, 8.0), (10.6, 9.6)], false);
        }
        Icon::Sliders => {
            line((1.5, 4.5), (14.5, 4.5));
            line((1.5, 11.5), (14.5, 11.5));
            painter.rect_filled(
                Rect::from_center_size(m(5.5, 4.5), Vec2::splat(3.0 * s)),
                Rounding::same(1.0 * s),
                stroke.color,
            );
            painter.rect_filled(
                Rect::from_center_size(m(10.5, 11.5), Vec2::splat(3.0 * s)),
                Rounding::same(1.0 * s),
                stroke.color,
            );
        }
        Icon::Layers => {
            path(&[(8.0, 2.0), (14.0, 5.5), (8.0, 9.0), (2.0, 5.5)], true);
            path(&[(2.0, 9.5), (8.0, 13.0), (14.0, 9.5)], false);
        }
        Icon::Sparkle => {
            path(&[(8.0, 2.0), (8.0, 14.0)], false);
            path(&[(2.0, 8.0), (14.0, 8.0)], false);
            path(&[(4.2, 4.2), (11.8, 11.8)], false);
            path(&[(11.8, 4.2), (4.2, 11.8)], false);
        }
        Icon::Cloud => {
            path(
                &[
                    (4.0, 12.5),
                    (2.6, 10.6),
                    (3.2, 8.4),
                    (5.2, 7.4),
                    (6.2, 5.4),
                    (8.4, 4.6),
                    (10.6, 5.4),
                    (11.6, 7.2),
                    (13.4, 8.0),
                    (13.8, 10.2),
                    (12.4, 12.0),
                    (10.0, 12.5),
                ],
                true,
            );
        }
        Icon::Pin => {
            circle((8.0, 6.5), 3.5);
            line((8.0, 10.0), (8.0, 14.5));
        }
        Icon::Speaker => {
            path(
                &[
                    (2.0, 6.0),
                    (5.0, 6.0),
                    (8.5, 3.0),
                    (8.5, 13.0),
                    (5.0, 10.0),
                    (2.0, 10.0),
                ],
                true,
            );
            path(&[(11.0, 6.0), (12.6, 8.0), (11.0, 10.0)], false);
            path(&[(13.0, 4.0), (15.0, 8.0), (13.0, 12.0)], false);
        }
        Icon::Wave => {
            path(
                &[
                    (1.5, 8.0),
                    (3.5, 8.0),
                    (5.0, 3.5),
                    (7.5, 12.5),
                    (9.5, 5.5),
                    (11.5, 10.5),
                    (13.0, 8.0),
                    (14.5, 8.0),
                ],
                false,
            );
        }
        Icon::Funnel => {
            path(
                &[
                    (2.0, 3.0),
                    (14.0, 3.0),
                    (9.5, 8.5),
                    (9.5, 13.5),
                    (6.5, 12.0),
                    (6.5, 8.5),
                ],
                true,
            );
        }
        Icon::Gauge => {
            path(
                &[
                    (2.2, 11.5),
                    (3.0, 8.6),
                    (4.8, 6.0),
                    (7.6, 4.6),
                    (10.4, 5.0),
                    (12.8, 6.8),
                    (14.0, 9.4),
                ],
                false,
            );
            line((8.0, 11.5), (11.0, 7.0));
            dot((8.0, 11.5));
        }
        Icon::Headphones => {
            path(
                &[
                    (2.5, 11.0),
                    (2.5, 8.0),
                    (4.2, 5.0),
                    (6.8, 3.4),
                    (9.2, 3.4),
                    (11.8, 5.0),
                    (13.5, 8.0),
                    (13.5, 11.0),
                ],
                false,
            );
            line((4.2, 10.0), (4.2, 13.0));
            line((11.8, 10.0), (11.8, 13.0));
        }
        Icon::Steps => {
            path(
                &[
                    (2.0, 13.0),
                    (6.0, 13.0),
                    (6.0, 9.0),
                    (10.0, 9.0),
                    (10.0, 5.0),
                    (14.0, 5.0),
                ],
                false,
            );
            line((2.0, 13.0), (2.0, 8.0));
        }
        Icon::Code => {
            path(&[(5.5, 4.5), (2.0, 8.0), (5.5, 11.5)], false);
            path(&[(10.5, 4.5), (14.0, 8.0), (10.5, 11.5)], false);
            line((9.0, 3.0), (7.0, 13.0));
        }
        Icon::Branch => {
            line((2.0, 13.0), (2.0, 6.0));
            line((2.0, 8.5), (7.0, 8.5));
            path(&[(7.0, 8.5), (10.5, 5.0), (13.0, 5.0)], false);
            path(&[(7.0, 8.5), (10.5, 12.0), (13.0, 12.0)], false);
            dot((2.0, 13.0));
        }
        Icon::Camera => {
            rect_at(1.5, 4.5, 9.5, 7.0, 1.5);
            path(&[(11.0, 8.0), (14.5, 5.5), (14.5, 10.5), (11.0, 8.0)], true);
        }
        Icon::Music => {
            line((6.0, 12.0), (6.0, 4.0));
            line((6.0, 4.5), (13.0, 3.0));
            line((13.0, 3.0), (13.0, 10.0));
            circle((4.6, 12.0), 1.6);
            circle((11.6, 10.0), 1.6);
        }
        Icon::Package => {
            path(
                &[
                    (8.0, 2.0),
                    (14.0, 5.0),
                    (14.0, 11.0),
                    (8.0, 14.0),
                    (2.0, 11.0),
                    (2.0, 5.0),
                ],
                true,
            );
            line((2.0, 5.0), (8.0, 8.0));
            line((14.0, 5.0), (8.0, 8.0));
            line((8.0, 8.0), (8.0, 14.0));
        }
        Icon::Gear => {
            circle((8.0, 8.0), 3.4);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let (sin, cos) = a.sin_cos();
                line(
                    (8.0 + cos * 5.2, 8.0 + sin * 5.2),
                    (8.0 + cos * 6.6, 8.0 + sin * 6.6),
                );
            }
        }
        Icon::Play => {
            path(&[(5.0, 3.0), (13.0, 8.0), (5.0, 13.0)], true);
        }
        Icon::Files => {
            rect_at(2.0, 4.5, 9.0, 10.0, 1.5);
            path(
                &[(5.0, 4.0), (5.0, 2.0), (13.5, 2.0), (13.5, 10.5), (11.5, 10.5)],
                false,
            );
        }
        Icon::Search => {
            circle((7.0, 7.0), 4.4);
            line((10.2, 10.2), (14.0, 14.0));
        }
        Icon::Pointer => {
            path(
                &[
                    (4.0, 2.5),
                    (4.0, 12.5),
                    (7.0, 9.8),
                    (9.3, 14.0),
                    (11.0, 13.0),
                    (8.8, 9.0),
                    (12.6, 8.4),
                ],
                true,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Small widgets
// ---------------------------------------------------------------------------

/// Small uppercase caption that introduces a group of controls.
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(GAP);
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
            FontId::proportional(SMALL),
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
    let font = FontId::proportional(CAPTION);
    let galley = ui.painter().layout_no_wrap(text, font, color);
    let pad = Vec2::new(GAP, 3.0);
    let size = galley.size() + pad * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(
            rect,
            Rounding::same(size.y * 0.5),
            color.gamma_multiply(0.14),
        );
        ui.painter().galley(rect.min + pad, galley, color);
    }
    resp
}

/// Filled primary action (Run).
pub fn primary_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let p = p();
    let text = text.into();
    let enabled = ui.is_enabled();
    let fg = if enabled { Color32::WHITE } else { p.faint };
    let galley = ui
        .painter()
        .layout_no_wrap(text, FontId::proportional(BUTTON), fg);
    let size = Vec2::new(galley.size().x + 32.0, BUTTON_H);
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
            mix(p.accent, Color32::BLACK, 0.18)
        } else if resp.hovered() {
            mix(p.accent, Color32::WHITE, 0.10)
        } else {
            p.accent
        };
        ui.painter().rect_filled(rect, Rounding::same(ROUND), fill);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, fg);
    }
    resp
}

/// Secondary button: same height and radius as the primary one, hairline
/// border, surface fill. One visual family for the whole toolbar.
pub fn ghost_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let p = p();
    let text = text.into();
    let enabled = ui.is_enabled();
    let base = if enabled { p.dim } else { p.faint };
    let galley = ui
        .painter()
        .layout_no_wrap(text.clone(), FontId::proportional(BUTTON), base);
    let size = Vec2::new(galley.size().x + 24.0, BUTTON_H);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let (fill, stroke, fg) = if !enabled {
            (Color32::TRANSPARENT, Stroke::NONE, p.faint)
        } else if resp.is_pointer_button_down_on() || resp.hovered() {
            (p.surface_hi, Stroke::new(1.0, p.stroke), p.text)
        } else {
            (p.surface, Stroke::new(1.0, p.stroke), p.dim)
        };
        ui.painter().rect(rect, Rounding::same(ROUND), fill, stroke);
        let galley = ui
            .painter()
            .layout_no_wrap(text, FontId::proportional(BUTTON), fg);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, fg);
    }
    resp
}

/// Compact button for use *inside* a node card, where the 28 px toolbar height
/// would dominate the card. Same radius and border as `ghost_button`.
pub fn mini_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let p = p();
    let text = text.into();
    // This runs inside a zoomed node card, so every metric has to follow the
    // style scale. With a fixed 18 px height the button kept its full size at
    // 40% zoom and dominated the card, forcing the card to reflow around it.
    let zoom = ui_zoom(ui);
    let galley = ui.painter().layout_no_wrap(
        text,
        FontId::proportional((CAPTION * zoom).max(1.0)),
        p.dim,
    );
    let size = Vec2::new(galley.size().x + 12.0 * zoom, 18.0 * zoom);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let stroke_w = zoom.max(0.5);
        let (fill, stroke) = if resp.hovered() {
            (p.surface_hi, Stroke::new(stroke_w, p.stroke))
        } else {
            (p.surface, Stroke::new(stroke_w, p.hairline))
        };
        ui.painter()
            .rect(rect, Rounding::same((4.0 * zoom).max(1.0)), fill, stroke);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, p.dim);
    }
    resp
}

/// Danger-tinted button (cancel / delete).
pub fn danger_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let p = p();
    let text = text.into();
    let enabled = ui.is_enabled();
    let fg = if enabled { p.err } else { p.faint };
    let galley = ui
        .painter()
        .layout_no_wrap(text, FontId::proportional(BUTTON), fg);
    let size = Vec2::new(galley.size().x + 24.0, BUTTON_H);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let (bg, stroke) = if !enabled {
            (Color32::TRANSPARENT, Stroke::NONE)
        } else if resp.hovered() {
            (
                p.err.gamma_multiply(0.20),
                Stroke::new(1.0, p.err.gamma_multiply(0.55)),
            )
        } else {
            (
                p.err.gamma_multiply(0.10),
                Stroke::new(1.0, p.err.gamma_multiply(0.35)),
            )
        };
        ui.painter().rect(rect, Rounding::same(ROUND), bg, stroke);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, fg);
    }
    resp
}

/// Thin horizontal rule.
pub fn rule(ui: &mut egui::Ui) {
    ui.add_space(GAP);
    let rect = ui.available_rect_before_wrap();
    let y = rect.top();
    ui.painter()
        .hline(rect.left()..=rect.right(), y, Stroke::new(1.0, p().stroke));
    ui.add_space(GAP);
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
    let height = 16.0;
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
        ui.painter().rect_filled(fill, radius, color);
    }
    // Dark text on the filled part, dim on the empty track.
    let on_fill = Color32::from_rgb(0x0B, 0x10, 0x16);
    let text_color = if !is_dark() || frac >= 0.55 {
        on_fill
    } else {
        p.dim
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.into(), FontId::proportional(CAPTION), text_color);
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, text_color);
}

/// A list/clickable card.
///
/// `id` must be unique for the whole frame: the cards are addressed by name
/// elsewhere (tests, and the drag ghost), so it is built from the caller's salt
/// rather than from the parent `Ui`'s id.
pub fn card(
    ui: &mut egui::Ui,
    selected: bool,
    id: egui::Id,
    sense: egui::Sense,
    add: impl FnOnce(&mut egui::Ui),
) -> Response {
    let p = p();
    let bg = ui.painter().add(egui::Shape::Noop);
    let frame = egui::Frame::none().inner_margin(Margin::symmetric(10.0, 6.0));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    let rect = inner.response.rect;
    let resp = ui.interact(rect, id, sense);
    let (fill, stroke) = if selected {
        (p.accent_soft, Stroke::new(1.0, p.accent))
    } else if resp.hovered() {
        (p.surface_hi, Stroke::new(1.0, p.stroke))
    } else {
        (p.surface, Stroke::new(1.0, p.hairline))
    };
    ui.painter().set(
        bg,
        egui::epaint::RectShape::new(rect, Rounding::same(ROUND_CARD), fill, stroke),
    );
    resp
}

/// Right-aligned helper for the toolbar: everything added inside is laid out
/// from the right edge inwards.
pub fn right_aligned<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(Layout::right_to_left(Align::Center), add)
        .inner
}

/// A keyboard chip: monospace glyphs in a hairline box.
pub fn key_chip(ui: &mut egui::Ui, key: &str) {
    let galley = ui
        .painter()
        .layout_no_wrap(key.to_owned(), FontId::monospace(CAPTION), p().text);
    let size = galley.size() + Vec2::new(10.0, 4.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            Rounding::same(4.0),
            p().surface,
            Stroke::new(1.0, p().stroke),
        );
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, p().text);
    }
}

/// Empty-state block: an icon, a title, a hint and a compact shortcut guide.
///
/// A blank panel with one line of text reads as "broken"; the key list gives it
/// a purpose and teaches the canvas at the same time.
pub fn empty_state(
    ui: &mut egui::Ui,
    which: Icon,
    title: &str,
    hint: &str,
    keys: &[(&str, &str)],
) {
    let p = p();
    // Short panels (the task list once the window is squeezed) drop the icon
    // and the guide instead of clipping the hint.
    let compact = ui.available_height() < 96.0;
    ui.vertical_centered(|ui| {
        if !compact {
            ui.add_space(8.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
            ui.painter().rect(
                rect,
                Rounding::same(ROUND_CARD),
                p.surface,
                Stroke::new(1.0, p.hairline),
            );
            icon(ui.painter(), rect.shrink(9.0), which, p.faint);
            ui.add_space(8.0);
        }
        ui.label(egui::RichText::new(title).size(BODY).color(p.dim));
        ui.add_space(2.0);
        ui.label(egui::RichText::new(hint).size(SMALL).color(p.faint));
        if !compact && !keys.is_empty() {
            ui.add_space(10.0);
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for (k, what) in keys {
                    ui.horizontal(|ui| {
                        // Centre the chip + caption pair: widths vary, so
                        // measure them first instead of relying on the layout
                        // direction.
                        let key_w = (k.chars().count() as f32 * 6.5 + 10.0).max(26.0);
                        let text_w = what.chars().count() as f32 * 5.6;
                        let pad = ((ui.available_width() - key_w - GAP - text_w) * 0.5).max(0.0);
                        ui.add_space(pad);
                        key_chip(ui, k);
                        ui.label(egui::RichText::new(*what).size(CAPTION).color(p.faint));
                    });
                }
            });
            ui.add_space(4.0);
        }
    });
}

/// Fade the bottom `height` of a scrolling list into the panel colour, so the
/// last row reads as "more below" instead of being sliced.
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
