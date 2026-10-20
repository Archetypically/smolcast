//! Raycast-like look (plan §12.7): neutral dark/light surfaces, a soft gray
//! selection instead of an accent fill, 8-point corner radii, a large frameless
//! search field, hairline dividers, and a footer action bar with key caps.
//!
//! Both egui themes are styled once at startup; egui keeps following the
//! system theme.

use egui::{
    Color32, Context, CornerRadius, FontFamily, FontId, RichText, Stroke, TextStyle, Theme, Vec2,
};

/// Search field text size; Raycast's query line is larger than the body.
pub const SEARCH_SIZE: f32 = 18.0;
/// Horizontal inset shared by the search bar, list, and footer.
pub const GUTTER: i8 = 16;
/// Result row height.
pub const ROW_HEIGHT: f32 = 38.0;
/// Vertical item spacing.
pub const SPACING: f32 = 4.0;
/// One list row plus the spacing after it.
pub const ROW_STRIDE: f32 = ROW_HEIGHT + SPACING;
/// Footer bar height, excluding its divider.
pub const FOOTER_HEIGHT: f32 = 40.0;

const RADIUS: u8 = 8;
/// Raycast's brand red, used only for the text cursor, text selection, and links.
const ACCENT: Color32 = Color32::from_rgb(255, 99, 99);

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

/// Style both egui themes.
pub fn install(ctx: &Context) {
    for theme in [Theme::Dark, Theme::Light] {
        ctx.style_mut_of(theme, |style| apply(style, palette(theme == Theme::Dark)));
    }
}

fn apply(style: &mut egui::Style, p: &Palette) {
    use FontFamily::{Monospace, Proportional};
    style.text_styles = [
        (TextStyle::Small, FontId::new(12.0, Proportional)),
        (TextStyle::Body, FontId::new(14.0, Proportional)),
        (TextStyle::Button, FontId::new(13.0, Proportional)),
        (TextStyle::Heading, FontId::new(SEARCH_SIZE, Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, Monospace)),
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
    v.selection.stroke = Stroke::new(1.0, p.text);
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

/// Hovered-row fill.
pub fn hover_fill(ui: &egui::Ui) -> Color32 {
    ui.visuals().faint_bg_color
}

/// A rounded key cap such as `Enter` or `Ctrl`, as in Raycast's action bar.
/// Painted at its text size so centered layouts cannot stretch it.
pub fn key_cap(ui: &mut egui::Ui, key: &str) {
    let galley = egui::WidgetText::from(RichText::new(key).small()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Small,
    );
    let size = galley.size() + Vec2::new(10.0, 4.0);
    let (rect, _) = ui.allocate_exact_size(size.max(Vec2::new(20.0, 0.0)), egui::Sense::hover());
    let visuals = ui.visuals();
    ui.painter().rect_filled(rect, 4, selected_fill(ui));
    let text = rect.center() - galley.size() / 2.0;
    ui.painter().galley(text, galley, visuals.weak_text_color());
}

/// Raycast's back control: a left arrow in a rounded square. Painted so it
/// does not depend on the font having an arrow glyph.
pub fn back_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter().rect_filled(rect, 6, visuals.bg_fill);
    let c = rect.center();
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
