//! List mechanics shared by the launcher, clipboard, and snippet views:
//! wrapping arrow-key selection, the visible-row window, and row chrome.

use egui::{Color32, RichText};

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

/// One clickable result row: `leading` (an icon or glyph), then a truncating
/// `name`, then `trailing` right-aligned. The trailing column is laid out first
/// so the name only takes the remaining width.
pub fn row(
    ui: &mut egui::Ui,
    selected: bool,
    name: &str,
    leading: impl FnOnce(&mut egui::Ui),
    trailing: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    egui::Frame::new()
        .fill(if selected {
            ui.visuals().selection.bg_fill
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                leading(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    trailing(ui);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let mut label = RichText::new(name);
                        if selected {
                            label = label.color(ui.visuals().selection.stroke.color);
                        }
                        ui.add(egui::Label::new(label).truncate());
                    });
                });
            });
        })
        .response
        .interact(egui::Sense::click())
}
