use super::*;

// --- A3 (2026-05-29) auto-return grace state machine ----------------------
//
// Tauri (`BentoZone.tsx`) does NOT expand/collapse the instant the cursor
// crosses a zone edge — it runs hover-intent + grace timers so transient
// pointer twitches and the morph overshoot can't race the open/close:
//
//   * HOVER-INTENT: entering a collapsed zone schedules an expand `now +
//     expand_delay_ms` (Native release default 90). Leaving before it fires cancels.
//   * EXPAND-LOCK: when an expand fires it sets `expand_lock_until = now +
//     EXPAND_LOCK_MS`, derived from the same morph clock, so a transient leave
//     cannot race the settling endpoint.
//   * GRACE COLLAPSE: leaving an expanded zone schedules a collapse at
//     `max(now + collapse_delay_ms (Native release default 200), expand_lock_until)`.
//     Re-entering before it fires cancels.
//
// This struct is the PURE, allocation-free, unit-testable core (spec §10/§11)
// driven by frame-tick `GetTickCount` timestamps from the shell — NO
// `WM_TIMER`, no thread, no clock access inside the struct. The shell feeds
// it `on_enter` / `on_leave` events and polls `poll(now)` once per frame.

/// Expand-lock window applied when an expand fires.
///
/// One short compositor guard follows the shared morph endpoint. Deriving the
/// lock from the real animation duration prevents a stale long lock from making
/// the already-settled Zone feel unresponsive.
pub const EXPAND_LOCK_MS: u32 = ZONE_PILL_GEOMETRY_DURATION_MS + 20;

/// Action the [`HoverScheduler`] asks the shell to perform on a `poll`. The
/// shell maps `Expand`/`Collapse` onto the per-Zone morph animator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverAction {
    /// Nothing due this frame.
    None,
    /// The hover-intent delay elapsed while the cursor stayed inside — expand
    /// the carried zone.
    Expand(ZoneId),
    /// The grace delay (and any expand-lock) elapsed while the cursor stayed
    /// outside — collapse the carried zone back to its pill.
    Collapse(ZoneId),
}

/// Pure hover/grace scheduler. One instance per process tracks the single
/// zone the pointer is currently interacting with (native only expands one
/// zone at a time). All timestamps are raw `GetTickCount` ms supplied by the
/// caller; the struct never reads a clock itself, which makes every
/// transition deterministically testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoverScheduler {
    /// Zone with an armed expand-intent timer (cursor inside a collapsed
    /// zone), and the tick at which the expand should fire.
    expand_zone: Option<ZoneId>,
    expand_pending_at_ms: u32,
    /// Zone currently expanded (or expanding) under this scheduler.
    expanded_zone: Option<ZoneId>,
    /// Tick before which a collapse must not fire (set when an expand fires).
    expand_lock_until_ms: u32,
    /// Zone with an armed collapse-grace timer (cursor left an expanded
    /// zone), and the tick at which the collapse should fire.
    collapse_zone: Option<ZoneId>,
    collapse_pending_at_ms: u32,
}

impl Default for HoverScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl HoverScheduler {
    pub const fn new() -> Self {
        Self {
            expand_zone: None,
            expand_pending_at_ms: 0,
            expanded_zone: None,
            expand_lock_until_ms: 0,
            collapse_zone: None,
            collapse_pending_at_ms: 0,
        }
    }

    /// True while any expand-intent or collapse-grace timer is armed — the
    /// shell uses this to keep the frame pump alive until the timer resolves.
    #[inline]
    pub fn is_pending(&self) -> bool {
        self.expand_zone.is_some() || self.collapse_zone.is_some()
    }

    /// The zone the scheduler currently considers expanded (if any).
    #[inline]
    pub fn expanded_zone(&self) -> Option<ZoneId> {
        self.expanded_zone
    }

    /// Cursor entered collapsed `zone`. Arms the hover-intent expand at
    /// `now + expand_delay_ms`. Cancels any pending collapse for that zone
    /// (re-enter aborts the grace). No-op if the zone is already expanded.
    pub fn on_enter(&mut self, zone: ZoneId, now_ms: u32, expand_delay_ms: u32) {
        // Re-entering the zone whose collapse is pending cancels the collapse.
        if self.collapse_zone == Some(zone) {
            self.collapse_zone = None;
        }
        // Already expanded — nothing to schedule.
        if self.expanded_zone == Some(zone) {
            self.expand_zone = None;
            return;
        }
        self.expand_zone = Some(zone);
        self.expand_pending_at_ms = now_ms.wrapping_add(expand_delay_ms);
    }

    /// Cursor left whatever it was over. Clears a pending expand-intent and,
    /// if the carried zone is expanded, arms a collapse at
    /// `max(now + collapse_delay_ms, expand_lock_until)`. `auto_collapse`
    /// gates display-mode: Hover and Click return to a capsule; Always is pinned.
    pub fn on_leave(&mut self, now_ms: u32, collapse_delay_ms: u32, auto_collapse: bool) {
        // A leave always cancels a not-yet-fired expand intent.
        self.expand_zone = None;
        let Some(expanded) = self.expanded_zone else {
            return;
        };
        if !auto_collapse {
            // ALWAYS / pinned mode never auto-collapses on leave.
            return;
        }
        let base = now_ms.wrapping_add(collapse_delay_ms);
        // Defer past the expand-lock window so the overshoot can't be raced:
        // pending = max(base, expand_lock_until). `!reached(base, lock)` means
        // `base` has not yet caught up to the lock deadline (lock is later).
        let pending = if !reached(base, self.expand_lock_until_ms) {
            self.expand_lock_until_ms
        } else {
            base
        };
        self.collapse_zone = Some(expanded);
        self.collapse_pending_at_ms = pending;
    }

    /// Force the scheduler back to a fully idle state (e.g. the pointer left
    /// the whole overlay and the fallback path collapsed everything). Drops
    /// all pending timers and the expanded marker.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Advance one frame at `now_ms`. Returns the action that just became due
    /// (at most one per call) and updates internal state so the action is not
    /// re-emitted. Expand wins over collapse if both somehow resolve on the
    /// same tick (they target different lifecycle states so this is defensive).
    pub fn poll(&mut self, now_ms: u32) -> HoverAction {
        if let Some(zone) = self.expand_zone {
            if reached(now_ms, self.expand_pending_at_ms) {
                self.expand_zone = None;
                self.mark_expanded(zone, now_ms);
                return HoverAction::Expand(zone);
            }
        }
        if let Some(zone) = self.collapse_zone {
            if reached(now_ms, self.collapse_pending_at_ms) {
                self.collapse_zone = None;
                if self.expanded_zone == Some(zone) {
                    self.expanded_zone = None;
                }
                return HoverAction::Collapse(zone);
            }
        }
        HoverAction::None
    }

    /// Record that `zone` is now expanded and arm the expand-lock window so a
    /// transient leave during the overshoot defers the collapse. Called from
    /// `poll` when a hover-intent fires; also exposed for the shell when an
    /// expand is forced through a path other than the intent timer (e.g. a
    /// click or a direct zone-to-zone hand-off).
    pub fn mark_expanded(&mut self, zone: ZoneId, now_ms: u32) {
        self.expanded_zone = Some(zone);
        self.expand_lock_until_ms = now_ms.wrapping_add(EXPAND_LOCK_MS);
        self.expand_zone = None;
        // A fresh expand cancels any stale collapse for the same zone.
        if self.collapse_zone == Some(zone) {
            self.collapse_zone = None;
        }
    }
}

/// Monotone-ish "now has reached deadline" test tolerant of `GetTickCount`
/// wraparound (every ~49.7 days). Treats the unsigned wrap distance: if
/// `now - deadline` is small-positive (< half the u32 range) the deadline
/// has passed.
#[inline]
pub(super) fn reached(now_ms: u32, deadline_ms: u32) -> bool {
    now_ms.wrapping_sub(deadline_ms) < (u32::MAX / 2)
}

// Unit + state-machine tests live in the sibling `tests.rs` to keep this
// production module within the §15 800-line budget.
