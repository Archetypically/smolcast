//! About smolcast: the version, who made it, and what it is built with.
//! A quiet page with one grace note: the heart beats once when it opens.

use std::time::Instant;

use egui::{Align, Color32, FontId, Layout, RichText, Sense, Vec2};

use crate::core::emoji_data::{CLDR_VERSION, UNICODE_EMOJI_VERSION};
use crate::core::motion::GLIDE_SECS;

use super::{motion, theme};

const AUTHOR: &str = "Evan Lee";
/// How long the heart's single beat takes, in seconds.
const BEAT_SECS: f32 = 0.7;
const HEART_EDGE: f32 = 14.0;

pub struct AboutView {
    opened_at: Instant,
}

impl Default for AboutView {
    fn default() -> Self {
        Self {
            opened_at: Instant::now(),
        }
    }
}

impl AboutView {
    pub fn open(&mut self) {
        self.opened_at = Instant::now();
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.add_space(theme::SECTION_GAP);
            let heading = egui::TextStyle::Heading.resolve(ui.style()).size;
            ui.label(
                RichText::new("smolcast")
                    .font(FontId::proportional((heading * 1.6).round()))
                    .strong(),
            );
            ui.label(
                RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .weak(),
            );
            ui.add_space(theme::SECTION_GAP);
            ui.label("A small, fully native command palette for Windows.");

            ui.add_space(2.0 * theme::SECTION_GAP);
            ui.horizontal(|ui| {
                // A horizontal line starts at the left: measure it, then pad
                // it to the center.
                let gap = 4.0;
                ui.spacing_mut().item_spacing.x = gap;
                let width = ["Made with", "by", AUTHOR]
                    .map(|text| text_width(ui, text))
                    .iter()
                    .sum::<f32>()
                    + HEART_EDGE
                    + 3.0 * gap;
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                ui.label("Made with");
                self.heart(ui);
                ui.label("by");
                ui.label(RichText::new(AUTHOR).strong());
            });

            ui.add_space(2.0 * theme::SECTION_GAP);
            theme::section_label(ui, "Built with");
            for line in [
                "egui and eframe · windows-rs · global-hotkey · tray-icon",
                "fuzzy-matcher · serde · toml · image · unicode-segmentation",
            ] {
                ui.label(RichText::new(line).small());
            }
            ui.label(
                RichText::new(format!(
                    "Emoji data from Unicode {UNICODE_EMOJI_VERSION} and CLDR {CLDR_VERSION}, \
                     © Unicode, Inc."
                ))
                .small(),
            );
            ui.add_space(theme::SECTION_GAP);
            ui.label(
                RichText::new("Inspired by Raycast and Tinycast.")
                    .small()
                    .weak(),
            );
            ui.label(
                RichText::new(format!("MIT License · © 2026 {AUTHOR}"))
                    .small()
                    .weak(),
            );
        });
    }

    /// A painted heart (no font has to have it) that beats once on open.
    fn heart(&self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(HEART_EDGE), Sense::hover());
        let elapsed = self.opened_at.elapsed().as_secs_f32();
        let beat = if motion::reduced(ui.ctx()) || elapsed >= BEAT_SECS {
            0.0
        } else {
            ui.ctx().request_repaint();
            // Two quick pulses, like a heartbeat: lub-dub.
            let t = elapsed / BEAT_SECS;
            let pulse = |center: f32, width: f32| {
                let x = (t - center) / width;
                (1.0 - x * x).max(0.0)
            };
            pulse(0.2, 0.15).max(0.7 * pulse(0.5, 0.15))
        };
        // A second beat on hover, for anyone who goes looking. A movement, so
        // reduced motion holds it at rest.
        let hover = motion::movement(
            ui,
            response.id.with("hover"),
            response.hovered(),
            GLIDE_SECS,
            egui::emath::easing::linear,
        );
        let scale = 1.0 + 0.25 * beat.max(hover * 0.5);
        // In the theme's accent: the one decorative color (DESIGN.md).
        paint_heart(ui, rect.center(), 6.0 * scale, theme::accent(ui));
        response.on_hover_text("Thank you for using smolcast");
    }
}

fn text_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(text.to_owned(), font, Color32::WHITE)
            .size()
            .x
    })
}

/// Two circles over a downward triangle, `r` the circles' radius.
fn paint_heart(ui: &egui::Ui, center: egui::Pos2, r: f32, color: Color32) {
    let painter = ui.painter();
    let lobe = r * 0.55;
    let top = center.y - r * 0.25;
    painter.circle_filled(egui::pos2(center.x - lobe, top), lobe, color);
    painter.circle_filled(egui::pos2(center.x + lobe, top), lobe, color);
    // The triangle's sides are tangent-ish to the lobes' outer edges.
    let points = vec![
        egui::pos2(center.x - 2.0 * lobe + 0.3, top + lobe * 0.35),
        egui::pos2(center.x + 2.0 * lobe - 0.3, top + lobe * 0.35),
        egui::pos2(center.x, center.y + r),
    ];
    painter.add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
}
