//! Paint-side motion (DESIGN.md, Motion). Timing lives in `core::motion`;
//! this module holds the reduced-motion flag the host sets and the views
//! read.

use egui::{Context, Id};

fn reduced_id() -> Id {
    Id::new("smolcast-motion-reduced")
}

/// Set by the host from the `reduce_motion` setting and Windows' animation
/// switch.
pub fn set_reduced(ctx: &Context, reduced: bool) {
    ctx.data_mut(|d| d.insert_temp(reduced_id(), reduced));
}

pub fn reduced(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp(reduced_id())).unwrap_or(false)
}
