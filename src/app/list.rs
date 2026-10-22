//! List mechanics shared by the launcher, clipboard, and snippet views: wheel
//! input and row chrome. Selection and scroll math is in
//! `core::list_cursor`.

use egui::emath::GuiRounding as _;
use egui::{
    Align, Color32, Layout, Pos2, Rect, RichText, Sense, Shape, TextureHandle, UiBuilder, Vec2,
};

use crate::core::list_cursor::ListCursor;

use super::{motion, theme};

/// Mouse-wheel scrolling for the windowed lists: whole rows to move the
/// window by while the pointer is over `ui` (positive scrolls down). The
/// sub-row remainder carries over in `carry`, so slow trackpad scrolling
/// still moves.
pub fn wheel_rows(ui: &egui::Ui, row_height: f32, carry: &mut f32) -> isize {
    if !ui.ui_contains_pointer() {
        *carry = 0.0;
        return 0;
    }
    // egui's delta is positive when content should move down (scroll up).
    *carry -= ui.input(|i| i.smooth_scroll_delta.y);
    let rows = (*carry / row_height).trunc();
    *carry -= rows * row_height;
    rows as isize
}

/// Wheel-scroll a plain list of `count` rows, `visible` at a time, and keep
/// the selection inside the window. Returns the rows to draw.
pub fn scroll(
    ui: &egui::Ui,
    cursor: &mut ListCursor,
    count: usize,
    visible: usize,
) -> std::ops::Range<usize> {
    let step = wheel_rows(ui, theme::ROW_STRIDE, &mut cursor.wheel);
    cursor.scroll(count, visible, step)
}

/// The pixel edge to extract an icon at for a row drawn `edge` pixels wide.
pub fn icon_request_edge(edge: u32) -> u32 {
    edge.clamp(16, 256)
}

/// A prescaled icon texture in an `edge`-point square, drawn texel-for-pixel
/// on whole device pixels: any resample here would blur it.
pub fn icon(ui: &mut egui::Ui, texture: &TextureHandle, edge: f32) {
    let (slot, _) = ui.allocate_exact_size(Vec2::splat(edge), Sense::hover());
    let ppp = ui.ctx().pixels_per_point();
    let [w, h] = texture.size();
    let mut drawn = Vec2::new(w as f32, h as f32) / ppp;
    // One extracted before a DPI change: fit the slot.
    if drawn.max_elem() > edge {
        drawn *= edge / drawn.max_elem();
    }
    let min = (slot.center() - drawn / 2.0).round_to_pixels(ppp);
    ui.painter().image(
        texture.id(),
        Rect::from_min_size(min, drawn),
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// One clickable result row: `leading` (an icon or glyph), then a truncating
/// `name`, then `trailing` right-aligned. The trailing column is laid out first
/// so the name only takes the remaining width. Selection is a soft rounded
/// fill, as in Raycast, rather than an accent color; it glides between the
/// rows of one list, and hover fills fade in.
pub fn row(
    ui: &mut egui::Ui,
    selected: bool,
    name: &str,
    leading: impl FnOnce(&mut egui::Ui),
    trailing: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let rect = ui
        .allocate_exact_size(
            Vec2::new(ui.available_width(), theme::ROW_HEIGHT),
            Sense::hover(),
        )
        .0;
    // The fill depends on hover, known only once the click area exists, and
    // the click area must be added after the labels so they cannot take it.
    let fill = ui.painter().add(Shape::Noop);
    let inner = rect.shrink2(Vec2::new(theme::ROW_INSET, 0.0));
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            leading(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                trailing(ui);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(name).strong()).truncate());
                });
            });
        },
    );
    let response = ui.interact(
        rect,
        ui.id().with(("row", rect.min.y as i32)),
        Sense::click(),
    );
    let hover = motion::hover(ui, response.id, response.hovered() && !selected);
    let shape = if selected {
        let at = motion::glide(ui, ui.id().with("selection"), rect);
        Shape::rect_filled(at, 8, theme::selected_fill(ui))
    } else {
        Shape::rect_filled(rect, 8, theme::hover_fill(ui).gamma_multiply(hover))
    };
    ui.painter().set(fill, shape);
    response
}
