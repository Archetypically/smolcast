//! Paint-side motion (DESIGN.md, Motion). Timing lives in `core::motion`;
//! this module applies it to egui: the reduced-motion flag, gliding
//! highlights, hover fades, and the small movements reduced motion drops.
//! Everything here changes paint only, so input and layout always see final
//! positions.

use egui::{Context, Id, Rect, Ui};

use crate::core::motion::GLIDE_SECS;

fn reduced_id() -> Id {
    Id::new("smolcast-motion-reduced")
}

fn epoch_id() -> Id {
    Id::new("smolcast-motion-epoch")
}

/// Set by the host from the `reduce_motion` setting and Windows' animation
/// switch.
pub fn set_reduced(ctx: &Context, reduced: bool) {
    ctx.data_mut(|d| d.insert_temp(reduced_id(), reduced));
}

pub fn reduced(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp(reduced_id())).unwrap_or(false)
}

/// Start a new motion epoch: highlights snap to their next position instead
/// of gliding there from a stale one (a fresh summon, another view).
pub fn new_epoch(ctx: &Context) {
    ctx.data_mut(|d| {
        let epoch = d.get_temp_mut_or_default::<u64>(epoch_id());
        *epoch = epoch.wrapping_add(1);
    });
}

fn epoch(ctx: &Context) -> u64 {
    ctx.data(|d| d.get_temp(epoch_id())).unwrap_or(0)
}

/// Where to paint a selection highlight headed for `target`: it glides from
/// its previous position, one per `id` (a list or grid).
pub fn glide(ui: &Ui, id: Id, target: Rect) -> Rect {
    let ctx = ui.ctx();
    if reduced(ctx) {
        return target;
    }
    let id = id.with(epoch(ctx));
    let x = ctx.animate_value_with_time(id.with("x"), target.min.x, GLIDE_SECS);
    let y = ctx.animate_value_with_time(id.with("y"), target.min.y, GLIDE_SECS);
    Rect::from_min_size(egui::pos2(x, y), target.size())
}

/// How far a hover fill has faded in, 0 to 1. A fade, so reduced motion
/// keeps it.
pub fn hover(ui: &Ui, id: Id, hovered: bool) -> f32 {
    ui.ctx()
        .animate_bool_with_time(id.with("hover"), hovered, GLIDE_SECS)
}

/// How far a movement toward `on` has got, 0 to 1 (a lean or a pop). Unlike
/// a fade, reduced motion holds it at rest.
pub fn movement(ui: &Ui, id: Id, on: bool, secs: f32, easing: fn(f32) -> f32) -> f32 {
    if reduced(ui.ctx()) {
        return 0.0;
    }
    ui.ctx()
        .animate_bool_with_time_and_easing(id, on, secs, easing)
}
