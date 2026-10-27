//! smolcast's look (plan §12.7, decision 028): Raycast's structure (a large
//! frameless search field, a soft gray selection, a footer action bar with key
//! caps) with Tinycast's dissolving edges, Spotlight's search glyph, and a
//! Windows 11 selection pip in the theme's accent (the brand red by default).
//!
//! Both egui themes are styled, in the color theme's light and dark variants,
//! at startup and again when the theme, text size, reduced motion, or high
//! contrast changes. Which one shows follows the `appearance` setting: the
//! system theme, or a forced light or dark.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use egui::emath::GuiRounding as _;
use egui::{
    Color32, Context, CornerRadius, FontFamily, FontId, RichText, Stroke, TextStyle, Theme,
    ThemePreference, Vec2,
};

use crate::core::config::{Appearance, ColorTheme, MAX_MICA_OPACITY, PaletteSize, TextSize};
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
/// Row icon slot. The search glyph and back button sit on the same column.
pub const ICON_SLOT: f32 = 22.0;
/// Gap between the icon slot and a row's name, or the search text.
pub const ICON_GAP: f32 = 10.0;
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
/// The brand red ("s." in the app icon): the default theme's accent.
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
    /// The text cursor, text selection, links, and the selection pip.
    accent: Color32,
}

const DARK: Palette = Palette {
    background: Color32::from_rgb(28, 28, 30),
    text: Color32::from_rgb(237, 237, 237),
    weak: Color32::from_rgb(142, 142, 147),
    selection: Color32::from_rgba_premultiplied(26, 26, 26, 26),
    hover: Color32::from_rgba_premultiplied(13, 13, 13, 13),
    control: Color32::from_rgba_premultiplied(18, 18, 18, 18),
    divider: Color32::from_rgba_premultiplied(26, 26, 26, 26),
    accent: ACCENT,
};

const LIGHT: Palette = Palette {
    background: Color32::from_rgb(250, 250, 250),
    text: Color32::from_rgb(28, 28, 30),
    weak: Color32::from_rgb(110, 110, 115),
    selection: Color32::from_rgba_premultiplied(0, 0, 0, 20),
    hover: Color32::from_rgba_premultiplied(0, 0, 0, 10),
    control: Color32::from_rgba_premultiplied(0, 0, 0, 13),
    divider: Color32::from_rgba_premultiplied(0, 0, 0, 20),
    accent: ACCENT,
};

/// A tinted theme's palette: its fills are its own text color at the
/// default theme's strengths, so they carry the theme's hue.
const fn tinted(background: Color32, text: Color32, weak: Color32, accent: Color32) -> Palette {
    let [r, g, b, _] = text.to_array();
    let dark = r as u16 + g as u16 + b as u16 > 3 * 128;
    let [selection, hover, control, divider] = if dark {
        [26, 13, 18, 26]
    } else {
        [20, 10, 13, 20]
    };
    Palette {
        background,
        text,
        weak,
        selection: Color32::from_rgba_unmultiplied_const(r, g, b, selection),
        hover: Color32::from_rgba_unmultiplied_const(r, g, b, hover),
        control: Color32::from_rgba_unmultiplied_const(r, g, b, control),
        divider: Color32::from_rgba_unmultiplied_const(r, g, b, divider),
        accent,
    }
}

/// A color theme's light and dark variants.
struct Scheme {
    light: Palette,
    dark: Palette,
    /// Percent of the background painted over Mica, so a tinted theme keeps
    /// its hue while the backdrop still shows through. Neutral themes leave
    /// Mica clear.
    mica_opacity: u8,
}

const SMOLCAST: Scheme = Scheme {
    light: LIGHT,
    dark: DARK,
    mica_opacity: 0,
};

/// Claude's ivory and charcoal, with its clay as the accent.
const CLAUDE: Scheme = Scheme {
    light: tinted(
        Color32::from_rgb(250, 249, 245),
        Color32::from_rgb(31, 30, 29),
        Color32::from_rgb(112, 110, 103),
        Color32::from_rgb(198, 97, 63),
    ),
    dark: tinted(
        Color32::from_rgb(38, 38, 36),
        Color32::from_rgb(238, 237, 231),
        Color32::from_rgb(160, 158, 150),
        Color32::from_rgb(217, 119, 87),
    ),
    mica_opacity: 60,
};

const SLATE: Scheme = Scheme {
    light: tinted(
        Color32::from_rgb(247, 248, 250),
        Color32::from_rgb(26, 30, 37),
        Color32::from_rgb(95, 103, 116),
        Color32::from_rgb(47, 107, 216),
    ),
    dark: tinted(
        Color32::from_rgb(27, 30, 36),
        Color32::from_rgb(228, 232, 239),
        Color32::from_rgb(139, 147, 161),
        Color32::from_rgb(111, 163, 255),
    ),
    mica_opacity: 45,
};

const SAGE: Scheme = Scheme {
    light: tinted(
        Color32::from_rgb(246, 247, 243),
        Color32::from_rgb(28, 33, 29),
        Color32::from_rgb(100, 108, 101),
        Color32::from_rgb(58, 132, 89),
    ),
    dark: tinted(
        Color32::from_rgb(27, 31, 28),
        Color32::from_rgb(227, 232, 226),
        Color32::from_rgb(140, 149, 141),
        Color32::from_rgb(116, 191, 142),
    ),
    mica_opacity: 50,
};

/// Paper and ink: no hue at all, so the accent is the text color.
const MONO: Scheme = Scheme {
    light: tinted(
        Color32::from_rgb(255, 255, 255),
        Color32::from_rgb(17, 17, 17),
        Color32::from_rgb(107, 107, 107),
        Color32::from_rgb(17, 17, 17),
    ),
    dark: tinted(
        Color32::from_rgb(14, 14, 14),
        Color32::from_rgb(242, 242, 242),
        Color32::from_rgb(140, 140, 140),
        Color32::from_rgb(242, 242, 242),
    ),
    mica_opacity: 0,
};

fn scheme(theme: ColorTheme) -> &'static Scheme {
    match theme {
        ColorTheme::Default => &SMOLCAST,
        ColorTheme::Claude => &CLAUDE,
        ColorTheme::Slate => &SLATE,
        ColorTheme::Sage => &SAGE,
        ColorTheme::Mono => &MONO,
    }
}

fn palette(theme: ColorTheme, dark: bool) -> &'static Palette {
    let scheme = scheme(theme);
    if dark { &scheme.dark } else { &scheme.light }
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
    pub appearance: Appearance,
    pub theme: ColorTheme,
}

/// Style both egui themes in the color theme's light and dark variants, and
/// pick which one shows. Called again when the look changes.
pub fn install(ctx: &Context, look: Look) {
    for egui_theme in [Theme::Dark, Theme::Light] {
        let dark = egui_theme == Theme::Dark;
        let base = palette(look.theme, dark);
        let contrast = look.high_contrast.then(|| high_contrast(base, dark));
        let p = contrast.as_ref().unwrap_or(base);
        ctx.style_mut_of(egui_theme, |style| {
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
    ctx.set_theme(match look.appearance {
        Appearance::System => ThemePreference::System,
        Appearance::Light => ThemePreference::Light,
        Appearance::Dark => ThemePreference::Dark,
    });
}

/// A theme's default opacity over Mica, in percent.
pub fn mica_opacity(theme: ColorTheme) -> u8 {
    scheme(theme).mica_opacity
}

/// The panel fill over a Mica backdrop: the background at `opacity` percent,
/// the theme's default unless set. Neutral themes leave Mica clear; tinted ones
/// keep their hue.
pub fn mica_fill(ui: &egui::Ui, theme: ColorTheme, opacity: Option<u8>) -> Color32 {
    let opacity = opacity.unwrap_or_else(|| mica_opacity(theme));
    ui.visuals()
        .panel_fill
        .gamma_multiply(f32::from(opacity.min(MAX_MICA_OPACITY)) / 100.0)
}

/// Edge of a color theme's swatch.
pub const SWATCH: f32 = 14.0;

/// A color theme's swatch in `rect`: a small rounded square of its
/// background, in the variant for the mode showing now, holding a selection
/// pip in its accent.
pub fn swatch(ui: &egui::Ui, theme: ColorTheme, rect: egui::Rect) {
    let p = palette(theme, ui.visuals().dark_mode);
    let painter = ui.painter();
    painter.rect_filled(rect, 3, p.background);
    painter.rect_stroke(
        rect,
        3,
        Stroke::new(1.0, p.divider),
        egui::StrokeKind::Inside,
    );
    let pip = egui::Rect::from_center_size(rect.center(), Vec2::new(3.0, 8.0));
    painter.rect_filled(pip, 1, p.accent);
}

/// The theme's accent.
pub fn accent(ui: &egui::Ui) -> Color32 {
    ui.visuals().hyperlink_color
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
    v.hyperlink_color = p.accent;
    // Text selection needs contrast; rows and key caps use `selected_fill`.
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    // Only the color is used for selected text; egui also strokes a focused
    // text field with this, which drew a white outline. The red cursor
    // marks focus instead.
    v.selection.stroke = Stroke::new(0.0, p.text);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
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

/// Share of a divider's length over which each end dissolves.
const DIVIDER_FADE: f32 = 0.18;

/// A divider across the full width of `ui`'s clip rect, ignoring margins. Its
/// ends dissolve into the window rather than stopping at its edges.
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    let x = ui.clip_rect().x_range();
    let color = divider_color(ui);
    let fade = x.span() * DIVIDER_FADE;
    let ppp = ui.ctx().pixels_per_point();
    // One whole device pixel row at least, so it stays crisp at any scale.
    let top = (rect.center().y - 0.5).round_to_pixels(ppp);
    let bottom = top + (1.0f32).round_to_pixels(ppp).max(1.0 / ppp);
    let stops = [
        (x.min, Color32::TRANSPARENT),
        (x.min + fade, color),
        (x.max - fade, color),
        (x.max, Color32::TRANSPARENT),
    ];
    let mut mesh = egui::Mesh::default();
    for (i, (at, color)) in stops.into_iter().enumerate() {
        mesh.colored_vertex(egui::pos2(at, top), color);
        mesh.colored_vertex(egui::pos2(at, bottom), color);
        if i > 0 {
            let n = 2 * i as u32;
            mesh.add_triangle(n - 2, n - 1, n);
            mesh.add_triangle(n - 1, n, n + 1);
        }
    }
    ui.painter().add(mesh);
}

/// The selected row's pip: a short accent bar at the fill's left edge, as
/// Windows 11 marks the current item, in the theme's accent. Painted with
/// the fill so it glides with it.
pub fn selection_pip(ui: &egui::Ui, fill: egui::Rect) -> egui::Shape {
    let pip = egui::Rect::from_center_size(
        egui::pos2(fill.left() + 3.5, fill.center().y),
        Vec2::new(3.0, 14.0),
    );
    egui::Shape::rect_filled(pip, 2, accent(ui))
}

/// Spotlight's magnifier, painted in the icon slot at the head of the search
/// field so the query starts on the row names' column.
pub fn search_glyph(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(ICON_SLOT), egui::Sense::hover());
    let stroke = Stroke::new(1.5, ui.visuals().weak_text_color());
    let c = rect.center() - Vec2::splat(1.5);
    let radius = 5.5;
    let painter = ui.painter();
    painter.circle_stroke(c, radius, stroke);
    let handle = Vec2::splat(std::f32::consts::FRAC_1_SQRT_2);
    painter.line_segment([c + handle * radius, c + handle * (radius + 5.0)], stroke);
}

/// Body content that is not a row or cell (labels, buttons, forms, notices),
/// inset so it lines up with the gutter like row content does.
pub fn inset<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(ROW_INSET as i8, 0))
        .show(ui, add_contents)
        .inner
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
    // A darker lower edge, like a physical key's; it flattens while held.
    let lip = rect
        .with_min_y(rect.max.y - 1.0)
        .shrink2(Vec2::new(3.0, 0.0));
    let shade = Color32::from_black_alpha(70).gamma_multiply(1.0 - held);
    ui.painter().rect_filled(lip, 0, shade);
    let text = rect.center() - galley.size() / 2.0;
    ui.painter().galley(text, galley, color);
}

/// Raycast's back control: a left arrow in a rounded square. Painted so it
/// does not depend on the font having an arrow glyph. It takes the icon
/// slot, centered on the row icons' column, and its square reaches 3 points
/// past the slot on each side.
pub fn back_button(ui: &mut egui::Ui) -> egui::Response {
    let (slot, _) = ui.allocate_exact_size(Vec2::new(ICON_SLOT, 28.0), egui::Sense::hover());
    let rect = egui::Rect::from_center_size(slot.center(), Vec2::splat(28.0));
    let response = ui.interact(rect, ui.id().with("back"), egui::Sense::click());
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

/// The font family holding Windows' icon font (`appearance::icon_font`).
pub const ICON_FAMILY: &str = "windows-icons";
/// Whether `ICON_FAMILY` was installed; egui panics on an unknown family.
static ICON_FONT: AtomicBool = AtomicBool::new(false);

/// Record that the host installed Windows' icon font as `ICON_FAMILY`.
pub fn set_icon_font_installed() {
    ICON_FONT.store(true, Ordering::Release);
}

/// An icon from Windows' icon font, as the Start menu and Settings draw it,
/// with an emoji to show where the font or the glyph is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    /// A Segoe Fluent Icons / Segoe MDL2 Assets codepoint.
    pub windows: char,
    pub emoji: &'static str,
}

impl Glyph {
    pub const fn new(windows: char, emoji: &'static str) -> Self {
        Self { windows, emoji }
    }
}

/// Draw `glyph` centered in an `edge`-point square, in the text color.
pub fn glyph(ui: &mut egui::Ui, glyph: Glyph, edge: f32) -> egui::Response {
    let size = Vec2::splat(edge);
    if ICON_FONT.load(Ordering::Acquire) {
        // The icon font's glyphs fill their em box; 3/4 of the slot matches
        // the visual weight of the program icons beside them.
        let font = FontId::new((edge * 0.75).round(), FontFamily::Name(ICON_FAMILY.into()));
        if ui.ctx().fonts_mut(|f| f.has_glyph(&font, glyph.windows)) {
            let text = RichText::new(glyph.windows.to_string())
                .font(font)
                .color(ui.visuals().text_color());
            return ui.add_sized(size, egui::Label::new(text).selectable(false));
        }
    }
    ui.add_sized(size, egui::Label::new(glyph.emoji).selectable(false))
}

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
