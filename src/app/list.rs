//! List mechanics shared by the launcher, clipboard, and snippet views:
//! wrapping arrow-key selection, the visible-row window, and row chrome.

use egui::{Align, Color32, Layout, RichText, Sense, Shape, UiBuilder, Vec2};

use super::theme;

/// Move `selected` one row down or up, wrapping at both ends.
pub fn step_selection(selected: &mut usize, count: usize, down: bool, up: bool) {
    if count == 0 {
        return;
    }
    if down {
        *selected = (*selected + 1) % count;
    }
    if up {
        *selected = (*selected + count - 1) % count;
    }
}

/// Scroll the `visible_rows` window starting at `first_row` so it shows
/// `selected`.
pub fn keep_visible(selected: usize, first_row: &mut usize, visible_rows: usize) {
    if selected < *first_row {
        *first_row = selected;
    } else if selected >= *first_row + visible_rows {
        *first_row = selected + 1 - visible_rows;
    }
}

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

/// Move a `visible`-row window over `count` rows by `step` rows, then keep
/// `selected` inside it, so the selection follows the wheel instead of
/// pulling the window back.
pub fn scroll_window(
    first_row: &mut usize,
    selected: &mut usize,
    count: usize,
    visible: usize,
    step: isize,
) {
    if step == 0 || count == 0 {
        return;
    }
    let max_first = count.saturating_sub(visible);
    *first_row = first_row.saturating_add_signed(step).min(max_first);
    let last = (*first_row + visible).min(count) - 1;
    *selected = (*selected).clamp(*first_row, last);
}

/// One clickable result row: `leading` (an icon or glyph), then a truncating
/// `name`, then `trailing` right-aligned. The trailing column is laid out first
/// so the name only takes the remaining width. Selection is a soft rounded
/// fill, as in Raycast, rather than an accent color.
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
    let color = if selected {
        theme::selected_fill(ui)
    } else if response.hovered() {
        theme::hover_fill(ui)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().set(fill, Shape::rect_filled(rect, 8, color));
    response
}
