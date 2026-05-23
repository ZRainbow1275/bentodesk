//! Animation primitives — `Easing` curves, `AnimatedValue<T>`.
//!
//! Per the team-lead's §13 ruling, the `Lerp` trait + `impl Lerp for f32`
//! moved into `bento-nano-style::lerp` so the orphan rule is satisfied for
//! third-crate impls (Color/Rect/Size/Length all live in style). Tree
//! re-exports the trait so existing `bento_nano_tree::Lerp` call sites keep
//! compiling unchanged.
//!
//! Spec §C4: animation state lives in `bento-nano-tree` (not a separate crate)
//! so the widget tree can co-locate state without an extra dependency edge.
//! Spec §10: every primitive is `Copy` + branch-light; no allocations in
//! `tick`.

use core::marker::PhantomData;

pub use bento_nano_style::Lerp;

/// Easing curves — three built-in shapes cover the design tokens. New curves
/// belong here so the `match` stays exhaustive at every call site.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseInOut,
    EaseOut,
}

impl Easing {
    /// Map raw progress `t ∈ [0,1]` through the curve. Out-of-range inputs are
    /// clamped — animations driven by accumulated time can drift slightly past
    /// the endpoint between frames and we don't want that to feed back into
    /// over/undershoot.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            // Standard `t*t*(3 - 2t)` smoothstep — symmetric around 0.5,
            // cheap, and indistinguishable from cubic-bezier(0.4,0,0.6,1) at
            // the resolutions we render at.
            Easing::EaseInOut => t * t * (3.0 - 2.0 * t),
            // `1 - (1 - t)^3` — matches cubic-bezier(0.0,0.0,0.2,1) closely
            // enough for UI use; cheaper than a real bezier solve.
            Easing::EaseOut => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
        }
    }
}

/// Per-value tween. Holds `from`/`to`, total duration, accumulated elapsed
/// seconds, and the curve. Driven by [`AnimatedValue::tick`] from the frame
/// loop — never allocates, never blocks.
#[derive(Debug, Clone, Copy)]
pub struct AnimatedValue<T: Lerp> {
    from: T,
    to: T,
    elapsed: f32,
    duration: f32,
    easing: Easing,
    _t: PhantomData<T>,
}

impl<T: Lerp> AnimatedValue<T> {
    /// Construct a static value (already at the target). `tick` is a no-op
    /// until [`AnimatedValue::animate_to`] introduces a new target.
    pub fn new(initial: T) -> Self {
        Self {
            from: initial,
            to: initial,
            elapsed: 0.0,
            duration: 0.0,
            easing: Easing::Linear,
            _t: PhantomData,
        }
    }

    /// Snapshot the current frozen target — useful when the caller needs a
    /// stable read without driving time forward.
    pub fn target(&self) -> T {
        self.to
    }

    /// Sample the current interpolated value. Stable while the tween hasn't
    /// been ticked.
    pub fn current(&self) -> T {
        if self.duration <= 0.0 {
            return self.to;
        }
        let raw = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let t = self.easing.apply(raw);
        self.from.lerp(self.to, t)
    }

    /// Begin a new tween. The current sampled value becomes the new `from`,
    /// preventing visible jumps when re-targeting mid-flight.
    pub fn animate_to(&mut self, target: T, duration_secs: f32, easing: Easing) {
        self.from = self.current();
        self.to = target;
        self.elapsed = 0.0;
        self.duration = duration_secs.max(0.0);
        self.easing = easing;
    }

    /// Advance the tween by `dt` seconds. Returns `true` while the tween is
    /// still in flight — the renderer uses this to decide whether to schedule
    /// another frame.
    pub fn tick(&mut self, dt: f32) -> bool {
        if self.duration <= 0.0 {
            return false;
        }
        self.elapsed += dt.max(0.0);
        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.from = self.to;
            self.duration = 0.0;
            return false;
        }
        true
    }

    /// True while the tween has not reached its target.
    pub fn is_active(&self) -> bool {
        self.duration > 0.0 && self.elapsed < self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_endpoints_are_exact() {
        for e in [Easing::Linear, Easing::EaseInOut, Easing::EaseOut] {
            assert_eq!(e.apply(0.0), 0.0, "{e:?} should pin t=0");
            assert_eq!(e.apply(1.0), 1.0, "{e:?} should pin t=1");
        }
    }

    #[test]
    fn easing_clamps_out_of_range_input() {
        // Drift past the endpoint must not over/undershoot.
        assert_eq!(Easing::Linear.apply(-0.5), 0.0);
        assert_eq!(Easing::EaseOut.apply(1.5), 1.0);
    }

    #[test]
    fn animated_value_reaches_target_after_full_duration() {
        let mut a: AnimatedValue<f32> = AnimatedValue::new(0.0);
        a.animate_to(10.0, 1.0, Easing::Linear);
        assert!(a.is_active());
        // One big tick covers the full duration.
        let still_running = a.tick(1.0);
        assert!(!still_running);
        assert!((a.current() - 10.0).abs() < 1e-6);
        assert!(!a.is_active());
    }

    #[test]
    fn animated_value_retargeting_uses_current_as_new_from() {
        let mut a: AnimatedValue<f32> = AnimatedValue::new(0.0);
        a.animate_to(10.0, 1.0, Easing::Linear);
        // Halfway through.
        let _ = a.tick(0.5);
        let mid = a.current();
        assert!((mid - 5.0).abs() < 1e-3);
        // Retarget — must not snap back to 0.0.
        a.animate_to(20.0, 1.0, Easing::Linear);
        assert!((a.current() - mid).abs() < 1e-3);
    }
}
