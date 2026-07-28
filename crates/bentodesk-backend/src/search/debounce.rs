//! Monotonic debouncer used by the SearchBar UI to avoid querying the
//! inverted index on every keystroke.
//!
//! The debouncer is wall-clock-driven: callers feed it `now_ms` (a
//! monotonically-increasing millisecond stamp owned by the UI tick loop)
//! and it gates `tap()` until at least `delay_ms` has elapsed since the
//! previous accepted tap. The first tap after construction is always
//! accepted.
//!
//! The component is intentionally `now_ms`-injected rather than reading
//! `std::time::Instant` internally so the SearchBar state machine — which
//! already owns its own monotonic accumulator at
//! `bentodesk-app::business::search_bar::SearchBarState::now_ms` — can
//! drive it deterministically from tests.

use serde::{Deserialize, Serialize};

/// Wall-clock debouncer.
///
/// `delay_ms` is the minimum gap between two accepted taps. `last_tap_ms`
/// is `None` until the first accepted tap, after which it tracks the
/// timestamp of that acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Debouncer {
    delay_ms: u32,
    last_tap_ms: Option<u32>,
}

impl Debouncer {
    /// Construct a fresh debouncer with the given gap. A `delay_ms` of 0
    /// degenerates into "always accept", which is the documented escape
    /// hatch for tests / disabled-debounce code paths.
    pub fn new(delay_ms: u32) -> Self {
        Self {
            delay_ms,
            last_tap_ms: None,
        }
    }

    /// Configured gap.
    pub fn delay_ms(&self) -> u32 {
        self.delay_ms
    }

    /// Try to accept a tap at `now_ms`. Returns `true` iff the tap is
    /// accepted (first tap, or ≥ `delay_ms` since the last accepted tap).
    /// On acceptance the internal cursor advances to `now_ms`.
    pub fn tap(&mut self, now_ms: u32) -> bool {
        let accept = match self.last_tap_ms {
            None => true,
            Some(prev) => now_ms.saturating_sub(prev) >= self.delay_ms,
        };
        if accept {
            self.last_tap_ms = Some(now_ms);
        }
        accept
    }

    /// Forget the previous tap. The next `tap()` is guaranteed to be
    /// accepted regardless of `now_ms`. Used when the SearchBar window
    /// closes so a re-open starts with a clean slate.
    pub fn reset(&mut self) {
        self.last_tap_ms = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_tap_is_always_accepted() {
        let mut d = Debouncer::new(120);
        assert!(d.tap(0));
    }

    #[test]
    fn rapid_taps_within_delay_are_rejected() {
        let mut d = Debouncer::new(120);
        assert!(d.tap(0));
        assert!(!d.tap(50));
        assert!(!d.tap(119));
    }

    #[test]
    fn taps_at_or_after_delay_are_accepted() {
        let mut d = Debouncer::new(120);
        assert!(d.tap(0));
        assert!(d.tap(120));
        assert!(d.tap(300));
    }

    #[test]
    fn accepted_tap_resets_the_window() {
        let mut d = Debouncer::new(100);
        assert!(d.tap(0));
        // 80 ms in — too soon.
        assert!(!d.tap(80));
        // 100 ms after t=0 — accepted; window now anchored at 100.
        assert!(d.tap(100));
        // 80 ms after t=100 — too soon again.
        assert!(!d.tap(180));
        // 100 ms after t=100 — accepted.
        assert!(d.tap(200));
    }

    #[test]
    fn reset_re_arms_the_first_tap_path() {
        let mut d = Debouncer::new(120);
        assert!(d.tap(0));
        assert!(!d.tap(50));
        d.reset();
        // After reset the next tap is accepted unconditionally.
        assert!(d.tap(50));
    }

    #[test]
    fn zero_delay_accepts_every_tap() {
        let mut d = Debouncer::new(0);
        assert!(d.tap(0));
        assert!(d.tap(0));
        assert!(d.tap(0));
    }

    #[test]
    fn now_ms_going_backwards_does_not_panic() {
        // Defensive: monotonic clock should never regress, but if the
        // caller hands us a smaller stamp we must not underflow.
        let mut d = Debouncer::new(100);
        assert!(d.tap(500));
        assert!(!d.tap(400)); // saturating_sub clamps to 0 < 100
    }
}
