//! Animation timing (DESIGN.md, Motion): short, interruptible tweens. The app
//! paints with their values; input and layout never wait for them.

use std::time::{Duration, Instant};

/// The palette fading and settling in after a summon.
pub const APPEAR: Duration = Duration::from_millis(160);
/// The palette fading out after a dismiss. Shorter than [`APPEAR`]: leaving
/// should never feel like waiting.
pub const DISMISS: Duration = Duration::from_millis(90);
/// The body sliding in after moving between views.
pub const NAVIGATE: Duration = Duration::from_millis(150);

/// A value moving from `from` to `to` over `duration`. Retargeting starts
/// from wherever the value is now, so a reversed animation never jumps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Tween {
    /// A tween resting at `value`.
    pub fn at(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            to: value,
            start: now,
            duration: Duration::ZERO,
        }
    }

    /// Linear progress from 0 to 1.
    fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.start);
        (elapsed.as_secs_f32() / self.duration.as_secs_f32()).min(1.0)
    }

    /// The current value, linearly interpolated; callers apply easing.
    pub fn value(&self, now: Instant) -> f32 {
        let t = self.progress(now);
        self.from + (self.to - self.from) * t
    }

    pub fn done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    /// Head for `to`. The duration is scaled by the distance left, so
    /// reversing halfway takes half as long as a full run.
    pub fn retarget(&mut self, to: f32, full: Duration, now: Instant) {
        let from = self.value(now);
        let distance = (to - from).abs().min(1.0);
        *self = Self {
            from,
            to,
            start: now,
            duration: full.mul_f32(distance),
        };
    }
}

/// Ease out (cubic): fast start, gentle landing.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Scale and opacity of the palette's content at presence `level` (0 is
/// gone, 1 is fully shown). Easing the level out serves both directions:
/// appearing it starts fast and lands gently; dismissing, the level falls,
/// so the fade starts gently and finishes fast (an ease in over time).
/// Reduced motion keeps the fade and drops the scaling.
pub fn presence_look(level: f32, reduced: bool) -> (f32, f32) {
    let eased = ease_out(level);
    let scale = if reduced { 1.0 } else { 0.97 + 0.03 * eased };
    (scale, eased)
}

/// Horizontal offset (in points) and opacity of a view sliding in at linear
/// progress `t`; `forward` slides in from the right, back from the left.
pub fn navigate_look(t: f32, forward: bool, reduced: bool) -> (f32, f32) {
    const DISTANCE: f32 = 12.0;
    let eased = ease_out(t);
    let offset = if reduced {
        0.0
    } else {
        let sign = if forward { 1.0 } else { -1.0 };
        sign * DISTANCE * (1.0 - eased)
    };
    (offset, eased)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn tween_runs_and_settles() {
        let t0 = Instant::now();
        let mut tween = Tween::at(0.0, t0);
        assert!(tween.done(t0));
        tween.retarget(1.0, ms(100), t0);
        assert_eq!(tween.value(t0), 0.0);
        assert!((tween.value(t0 + ms(50)) - 0.5).abs() < 1e-3);
        assert!(!tween.done(t0 + ms(50)));
        assert_eq!(tween.value(t0 + ms(500)), 1.0);
        assert!(tween.done(t0 + ms(100)));
    }

    #[test]
    fn reversal_starts_where_it_is_and_takes_the_remaining_share() {
        let t0 = Instant::now();
        let mut tween = Tween::at(1.0, t0);
        tween.retarget(0.0, ms(100), t0);
        let mid = t0 + ms(40);
        let level = tween.value(mid);
        tween.retarget(1.0, ms(200), mid);
        assert!((tween.value(mid) - level).abs() < 1e-6);
        // 0.4 of the way left, at 200 ms per full run.
        assert!(!tween.done(mid + ms(79)));
        assert!(tween.done(mid + ms(81)));
    }

    #[test]
    fn retarget_to_the_same_value_is_instant() {
        let t0 = Instant::now();
        let mut tween = Tween::at(1.0, t0);
        tween.retarget(1.0, ms(100), t0);
        assert!(tween.done(t0));
    }

    #[test]
    fn easing_hits_the_ends() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert_eq!(ease_out(-1.0), 0.0);
        assert_eq!(ease_out(2.0), 1.0);
        assert!(ease_out(0.5) > 0.5);
    }

    #[test]
    fn presence_ends_fully_shown_and_reduced_never_scales() {
        assert_eq!(presence_look(1.0, false), (1.0, 1.0));
        let (scale, opacity) = presence_look(0.0, false);
        assert!(scale < 1.0 && opacity == 0.0);
        assert_eq!(presence_look(0.3, true).0, 1.0);
    }

    #[test]
    fn navigation_slides_by_direction_and_reduced_only_fades() {
        assert!(navigate_look(0.0, true, false).0 > 0.0);
        assert!(navigate_look(0.0, false, false).0 < 0.0);
        assert_eq!(navigate_look(1.0, true, false), (0.0, 1.0));
        assert_eq!(navigate_look(0.0, true, true), (0.0, 0.0));
    }
}
