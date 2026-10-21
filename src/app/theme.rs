//! Raycast-like look (plan §12.7): neutral dark/light surfaces, a soft gray
//! selection instead of an accent fill, 8-point corner radii, a large frameless
//! search field, hairline dividers, and a footer action bar with key caps.
//!
//! Both egui themes are styled at startup and again when the text size,
//! reduced motion, or high contrast changes; egui keeps following the system
//! theme.

use std::time::{Duration, Instant};

use egui::{
    Color32, Context, CornerRadius, FontFamily, FontId, RichText, Stroke, TextStyle, Theme, Vec2,
};

use crate::core::config::{PaletteSize, SearchEngine, TextSize};
use crate::core::motion::{KEY_HELD_SECS, LEAN_SECS};

/// Horizontal inset of content shared by the search bar, list, and footer.
pub const GUTTER: i8 = 16;
/// Body padding. Row and cell fills start here; their content starts at
/// [`GUTTER`].
pub const BODY_MARGIN: i8 = 8;
/// Horizontal inset of row content inside its fill, so it meets the gutter.
pub const ROW_INSET: f32 = (GUTTER - BODY_MARGIN) as f32;
/// Vertical gap between body sections: a list and its buttons, form groups.
pub const SECTION_GAP: f32 = 8.0;
/// Result row height.
pub const ROW_HEIGHT: f32 = 38.0;
/// Vertical item spacing.
pub const SPACING: f32 = 4.0;
/// One list row plus the spacing after it.
pub const ROW_STRIDE: f32 = ROW_HEIGHT + SPACING;
/// Footer bar height, excluding its divider.
pub const FOOTER_HEIGHT: f32 = 40.0;
/// The medium window, in points.
const BASE_WINDOW: Vec2 = Vec2::new(640.0, 460.0);

/// List rows a palette size adds to (or removes from) each view's medium
/// row count.
pub fn extra_rows(size: PaletteSize) -> isize {
    match size {
        PaletteSize::Small => -1,
        PaletteSize::Medium => 0,
        PaletteSize::Large => 2,
    }
}

/// The base window for a palette size: wider, and taller by whole rows so
/// lists show more entries rather than more empty space.
pub fn window_size(size: PaletteSize) -> Vec2 {
    let width = match size {
        PaletteSize::Small => 560.0,
        PaletteSize::Medium => BASE_WINDOW.x,
        PaletteSize::Large => 760.0,
    };
    Vec2::new(width, BASE_WINDOW.y + extra_rows(size) as f32 * ROW_STRIDE)
}

/// A view's visible row count at `size`, from its medium count.
pub fn visible_rows(medium: usize, size: PaletteSize) -> usize {
    medium.saturating_add_signed(extra_rows(size)).max(1)
}

/// Font sizes for a text size: search, body, button, small, monospace.
fn font_sizes(size: TextSize) -> [f32; 5] {
    match size {
        TextSize::Small => [16.0, 13.0, 12.0, 11.0, 12.0],
        TextSize::Medium => [18.0, 14.0, 13.0, 12.0, 13.0],
        TextSize::Large => [20.0, 16.0, 14.0, 13.0, 14.0],
    }
}

const RADIUS: u8 = 8;
/// Raycast's brand red, used only for the text cursor, text selection, and links.
const ACCENT: Color32 = Color32::from_rgb(255, 99, 99);
/// Status colors, in small text only (DESIGN.md).
pub const WARNING: Color32 = Color32::from_rgb(230, 180, 60);
pub const ERROR: Color32 = Color32::from_rgb(230, 90, 90);

struct Palette {
    background: Color32,
    text: Color32,
    weak: Color32,
    /// Selected row, key caps, and pressed controls.
    selection: Color32,
    /// Hovered row and control backgrounds.
    hover: Color32,
    /// Resting control backgrounds (buttons, text fields in forms).
    control: Color32,
    divider: Color32,
}

const DARK: Palette = Palette {
    background: Color32::from_rgb(28, 28, 30),
    text: Color32::from_rgb(237, 237, 237),
    weak: Color32::from_rgb(142, 142, 147),
    selection: Color32::from_rgba_premultiplied(26, 26, 26, 26),
    hover: Color32::from_rgba_premultiplied(13, 13, 13, 13),
    control: Color32::from_rgba_premultiplied(18, 18, 18, 18),
    divider: Color32::from_rgba_premultiplied(26, 26, 26, 26),
};

const LIGHT: Palette = Palette {
    background: Color32::from_rgb(250, 250, 250),
    text: Color32::from_rgb(28, 28, 30),
    weak: Color32::from_rgb(110, 110, 115),
    selection: Color32::from_rgba_premultiplied(0, 0, 0, 20),
    hover: Color32::from_rgba_premultiplied(0, 0, 0, 10),
    control: Color32::from_rgba_premultiplied(0, 0, 0, 13),
    divider: Color32::from_rgba_premultiplied(0, 0, 0, 20),
};

fn palette(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

/// Under a Windows high-contrast theme the soft fills and gray text are too
/// faint: strengthen them, keeping the same look.
fn high_contrast(p: &Palette, dark: bool) -> Palette {
    let (overlay, weak) = if dark {
        (Color32::WHITE, Color32::from_rgb(200, 200, 205))
    } else {
        (Color32::BLACK, Color32::from_rgb(60, 60, 65))
    };
    let alpha = |opacity: f32| overlay.gamma_multiply(opacity);
    Palette {
        weak,
        selection: alpha(0.28),
        hover: alpha(0.14),
        control: alpha(0.16),
        divider: alpha(0.4),
        ..*p
    }
}

/// How the palette is styled beyond its text size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub text: TextSize,
    /// egui's own animations (menus, toggles) are turned off too.
    pub reduced_motion: bool,
    pub high_contrast: bool,
}

/// Style both egui themes. Called again when the look changes.
pub fn install(ctx: &Context, look: Look) {
    for theme in [Theme::Dark, Theme::Light] {
        let dark = theme == Theme::Dark;
        let base = palette(dark);
        let contrast = look.high_contrast.then(|| high_contrast(base, dark));
        let p = contrast.as_ref().unwrap_or(base);
        ctx.style_mut_of(theme, |style| {
            apply(style, p, look.text);
            // Set both ways: the style persists, so turning reduced motion
            // off again must restore egui's own animations.
            style.animation_time = if look.reduced_motion {
                0.0
            } else {
                egui::Style::default().animation_time
            };
        });
    }
}

fn apply(style: &mut egui::Style, p: &Palette, text: TextSize) {
    use FontFamily::{Monospace, Proportional};
    // The search text is the Heading style.
    let [search, body, button, small, mono] = font_sizes(text);
    style.text_styles = [
        (TextStyle::Small, FontId::new(small, Proportional)),
        (TextStyle::Body, FontId::new(body, Proportional)),
        (TextStyle::Button, FontId::new(button, Proportional)),
        (TextStyle::Heading, FontId::new(search, Proportional)),
        (TextStyle::Monospace, FontId::new(mono, Monospace)),
    ]
    .into();

    let spacing = &mut style.spacing;
    spacing.item_spacing = Vec2::new(8.0, SPACING);
    spacing.button_padding = Vec2::new(10.0, 4.0);
    spacing.interact_size.y = 24.0;

    let v = &mut style.visuals;
    let radius = CornerRadius::same(RADIUS);
    v.panel_fill = p.background;
    v.window_fill = p.background;
    v.extreme_bg_color = p.control;
    v.text_edit_bg_color = Some(p.control);
    v.faint_bg_color = p.hover;
    v.code_bg_color = p.control;
    v.weak_text_color = Some(p.weak);
    v.hyperlink_color = ACCENT;
    // Text selection needs contrast; rows and key caps use `selected_fill`.
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    // Only the color is used for selected text; egui also strokes a focused
    // text field with this, which drew a white outline. The red cursor
    // marks focus instead.
    v.selection.stroke = Stroke::new(0.0, p.text);
    v.text_cursor.stroke = Stroke::new(2.0, ACCENT);
    v.window_corner_radius = radius;
    v.menu_corner_radius = radius;
    v.window_stroke = Stroke::new(1.0, p.divider);

    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.divider);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    for (state, fill) in [
        (&mut w.inactive, p.control),
        (&mut w.hovered, p.hover.blend(p.control)),
        (&mut w.active, p.selection),
        (&mut w.open, p.selection),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::NONE;
        state.fg_stroke = Stroke::new(1.0, p.text);
        state.corner_radius = CornerRadius::same(6);
        state.expansion = 0.0;
    }
    // egui styles a keyboard-focused control as active, whose fill is barely
    // darker than at rest: a hairline ring makes Tab focus visible. Text
    // fields are exempt (their cursor shows focus).
    w.active.bg_stroke = Stroke::new(1.0, p.weak);
    w.noninteractive.corner_radius = radius;
}

/// Selected row and key cap fill: a soft gray, unlike text selection.
pub fn selected_fill(ui: &egui::Ui) -> Color32 {
    ui.visuals().widgets.active.weak_bg_fill
}

pub fn divider_color(ui: &egui::Ui) -> Color32 {
    ui.visuals().widgets.noninteractive.bg_stroke.color
}

/// A divider across the full width of `ui`'s clip rect, ignoring margins.
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    let clip = ui.clip_rect();
    ui.painter().hline(
        clip.x_range(),
        rect.center().y,
        Stroke::new(1.0, divider_color(ui)),
    );
}

/// Body content that is not a row or cell (labels, buttons, forms, notices),
/// inset so it lines up with the gutter like row content does.
pub fn inset<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(ROW_INSET as i8, 0))
        .show(ui, add_contents)
        .inner
}

/// A search engine's monogram badge: its letter on a circle in its color.
/// Painted, since engine logos are not bundled.
pub fn engine_badge(ui: &mut egui::Ui, engine: SearchEngine, edge: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(edge), egui::Sense::hover());
    let (letter, [r, g, b]) = engine.badge();
    let painter = ui.painter();
    painter.circle_filled(rect.center(), edge * 0.45, Color32::from_rgb(r, g, b));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        FontId::proportional((edge * 0.55).round()),
        Color32::WHITE,
    );
    response
}

/// A small gray section heading ("Suggestions", "General").
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

/// Height a list section header takes, including the spacing after it.
pub const SECTION_HEADER_HEIGHT: f32 = 2.0 + 24.0 + SPACING + 2.0;

/// A list section header ("Suggestions", "Calculator", "Results") aligned
/// with row content, with an optional busy status on the right.
pub fn section_header(ui: &mut egui::Ui, text: &str, status: Option<&str>) {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.set_height(24.0);
        ui.add_space(ROW_INSET);
        section_label(ui, text);
        if let Some(status) = status {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(ROW_INSET);
                action_busy(ui, status);
            });
        }
    });
    ui.add_space(2.0);
}

/// Hovered-row fill.
pub fn hover_fill(ui: &egui::Ui) -> Color32 {
    ui.visuals().faint_bg_color
}

/// Whether the key a cap names is held down, so the cap can light up.
fn key_held(ui: &egui::Ui, name: &str) -> bool {
    ui.input(|i| match name {
        "Ctrl" => i.modifiers.ctrl,
        "Alt" => i.modifiers.alt,
        "Shift" => i.modifiers.shift,
        _ => egui::Key::from_name(name).is_some_and(|key| i.key_down(key)),
    })
}

/// A rounded key cap such as `Enter` or `Ctrl`, as in Raycast's action bar.
/// Painted at its text size so centered layouts cannot stretch it. It lights
/// up while its key is held.
pub fn key_cap(ui: &mut egui::Ui, key: &str) {
    let galley = egui::WidgetText::from(RichText::new(key).small()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Small,
    );
    let size = galley.size() + Vec2::new(10.0, 4.0);
    let (rect, response) =
        ui.allocate_exact_size(size.max(Vec2::new(20.0, 0.0)), egui::Sense::hover());
    let down = key_held(ui, key);
    let held = ui
        .ctx()
        .animate_bool_with_time(response.id.with("held"), down, KEY_HELD_SECS);
    let visuals = ui.visuals();
    let fill = selected_fill(ui).lerp_to_gamma(visuals.weak_text_color(), 0.35 * held);
    let color = visuals
        .weak_text_color()
        .lerp_to_gamma(visuals.text_color(), held);
    ui.painter().rect_filled(rect, 4, fill);
    let text = rect.center() - galley.size() / 2.0;
    ui.painter().galley(text, galley, color);
}

/// Raycast's back control: a left arrow in a rounded square. Painted so it
/// does not depend on the font having an arrow glyph.
pub fn back_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter().rect_filled(rect, 6, visuals.bg_fill);
    // The focus ring other controls get from the active style.
    ui.painter()
        .rect_stroke(rect, 6, visuals.bg_stroke, egui::StrokeKind::Inside);
    // On hover the arrow leans the way it points.
    let lean = super::motion::movement(
        ui,
        response.id.with("lean"),
        response.hovered(),
        LEAN_SECS,
        egui::emath::easing::linear,
    );
    let c = rect.center() - Vec2::new(1.5 * lean, 0.0);
    let stroke = Stroke::new(1.5, visuals.fg_stroke.color);
    let painter = ui.painter();
    painter.line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0)], stroke);
    painter.line_segment(
        [c + Vec2::new(-5.0, 0.0), c + Vec2::new(-1.0, -4.0)],
        stroke,
    );
    painter.line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(-1.0, 4.0)], stroke);
    response
}

/// An action label followed by its key caps, e.g. "Open Application Enter", in
/// a right-to-left layout: the caps are added first, last key first.
pub fn action_hint(ui: &mut egui::Ui, label: &str, keys: &[&str]) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for key in keys.iter().rev() {
            key_cap(ui, key);
        }
    });
    ui.label(RichText::new(label).small());
}

/// Work shorter than this shows no progress indicator, so quick operations do
/// not flash one.
const BUSY_DELAY: Duration = Duration::from_millis(150);

/// Whether work started at `since` has run long enough to show as busy. Until
/// then it schedules the repaint that will show it; the palette is visible
/// whenever this runs.
pub fn busy_due(ui: &egui::Ui, since: Instant) -> bool {
    let elapsed = since.elapsed();
    if elapsed >= BUSY_DELAY {
        return true;
    }
    ui.ctx().request_repaint_after(BUSY_DELAY - elapsed);
    false
}

/// A spinner and a weak label for work in progress. The spinner repaints only
/// while it is drawn, so nothing animates while the palette is hidden.
pub fn busy(ui: &mut egui::Ui, label: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(14.0));
        ui.label(RichText::new(label).weak());
    });
}

/// [`busy`] at caption size.
pub fn busy_small(ui: &mut egui::Ui, label: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(11.0));
        ui.label(RichText::new(label).weak().small());
    });
}

/// The footer's action slot while that action runs, in a right-to-left
/// layout: the label, then a spinner to its left.
pub fn action_busy(ui: &mut egui::Ui, label: &str) {
    ui.label(RichText::new(label).small());
    ui.add(egui::Spinner::new().size(11.0));
}

/// Characters the palette's chrome and typical queries use.
const WARM_TEXT: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`\
abcdefghijklmnopqrstuvwxyz{|}~…·×";

/// Lay out common characters in every text style while the palette is hidden,
/// so the first summon does not parse fonts or rasterize glyphs.
pub fn warm_glyphs(ui: &egui::Ui) {
    let fonts: Vec<FontId> = ui.style().text_styles.values().cloned().collect();
    let color = ui.visuals().text_color();
    ui.ctx().fonts_mut(|f| {
        for font in fonts {
            let _ = f.layout_no_wrap(WARM_TEXT.to_owned(), font, color);
        }
    });
}
