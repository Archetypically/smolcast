//! List mechanics shared by the launcher, clipboard, and snippet views: wheel
//! input and row chrome. Selection and scroll math is in
//! `core::list_cursor`.

use egui::emath::GuiRounding as _;
use egui::{
    Align, Color32, Layout, Pos2, Rect, RichText, Sense, Shape, TextureHandle, TextureOptions,
    UiBuilder, Vec2,
};

use crate::core::list_cursor::ListCursor;
use crate::win::icon::IconRgba;

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

/// How a row stands in its list window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    Rest,
    Selected,
    /// The first or last row drawn while more rows wait beyond it: it dims,
    /// so the list reads as continuing (DESIGN.md, Result row).
    Edge,
}

impl RowState {
    /// The state of `row` in a window `shown` of `count` rows.
    pub fn of(row: usize, selected: usize, shown: &std::ops::Range<usize>, count: usize) -> Self {
        if row == selected {
            Self::Selected
        } else if (row == shown.start && shown.start > 0)
            || (row + 1 == shown.end && shown.end < count)
        {
            Self::Edge
        } else {
            Self::Rest
        }
    }
}

/// Opacity of a row dimmed at the edge of its window.
const EDGE_OPACITY: f32 = 0.45;

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

/// Upload a rasterized icon as a texture named `name`. `icon` draws it on whole
/// device pixels, so it is never resampled.
pub fn icon_texture(ctx: &egui::Context, name: &str, icon: &IconRgba) -> TextureHandle {
    let image = egui::ColorImage::from_rgba_premultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba_premultiplied,
    );
    ctx.load_texture(name, image, TextureOptions::LINEAR)
}

/// One clickable result row: `leading` (an icon or glyph), then a truncating
/// `name`, then `trailing` right-aligned. The trailing column is laid out first
/// so the name only takes the remaining width. Selection is a soft rounded
/// fill marked by the accent pip; both glide between the rows of one list,
/// and hover fills fade in. An edge row dims.
pub fn row(
    ui: &mut egui::Ui,
    state: RowState,
    name: &str,
    leading: impl FnOnce(&mut egui::Ui),
    trailing: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    ui.scope(|ui| {
        if state == RowState::Edge {
            ui.multiply_opacity(EDGE_OPACITY);
        }
        row_contents(ui, state == RowState::Selected, name, leading, trailing)
    })
    .inner
}

fn row_contents(
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
            ui.spacing_mut().item_spacing.x = theme::ICON_GAP;
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
        Shape::Vec(vec![
            Shape::rect_filled(at, 8, theme::selected_fill(ui)),
            theme::selection_pip(at),
        ])
    } else {
        Shape::rect_filled(rect, 8, theme::hover_fill(ui).gamma_multiply(hover))
    };
    ui.painter().set(fill, shape);
    response
}

/// Columns of a list beside a preview pane, in points.
const LIST_WIDTH: f32 = 300.0;

/// A list `height` points tall, beside a preview pane. The list has a fixed
/// size, so the action row below stays visible. The separator is capped too:
/// uncapped, it takes all remaining height and grows the window every frame.
pub fn split(
    ui: &mut egui::Ui,
    height: f32,
    list: impl FnOnce(&mut egui::Ui),
    preview: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal_top(|ui| {
        ui.set_max_height(height);
        let size = Vec2::new(LIST_WIDTH, height);
        ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
            ui.set_min_size(size);
            ui.set_width(LIST_WIDTH);
            list(ui);
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_max_height(height);
            preview(ui);
        });
    });
}

/// Characters of a text item that a preview shows.
const PREVIEW_CHARS: usize = 2000;

/// A text item's preview: its start in monospace, then its length in
/// characters, `chars`.
pub fn text_preview(ui: &mut egui::Ui, text: &str, chars: usize) {
    let shown: String = text.chars().take(PREVIEW_CHARS).collect();
    ui.add(egui::Label::new(RichText::new(shown).monospace().small()).wrap());
    ui.label(RichText::new(format!("{chars} characters")).weak().small());
}
