//! V-8 (2026-05-21 video parity audit) — capsule pill animation engine.
//!
//! Tauri 1.2.4 reference frames show zone pills with discrete animation
//! channels layered on top of the static pill chrome:
//!
//! - `PillHover` keeps geometry fixed and drives the surface-tone reaction;
//!   hover-in runs over 160 ms and hover-out over 220 ms.
//! - `PillPress` keeps geometry fixed and supplies the press timing channel;
//!   pointer-down runs over 80 ms and release restores over 120 ms.
//! - `StatusDotPulse` is retained as a pure helper for a future status-dot
//!   paint consumer. The current renderer does not sample it, so it must not
//!   keep the shell frame pump alive on its own.
//!
//! Spec §10 (hot-path zero allocation): backing storage is a fixed-size
//! inline array (`[Option<Entry>; 64]`) — no `Vec`, no `String`, no `Box`.
//! The animator state lives on `AppState`, the per-frame pump samples it
//! after the existing `tick_zone_pill_animation` call, and the renderer
//! reads the eased 0..=1 value at paint time. The persisted pill geometry
//! tokens (`PILL_HEIGHT`, `RADIUS.capsule`) are NEVER mutated — transforms
//! apply at draw-time via `Rect`/alpha multipliers (per V-8 contract).
//!
//! No new threads / timers. The existing `GetTickCount` cadence in the
//! shell's render pump is the only time source.

use bento_nano_zone::ZoneId;

/// Maximum simultaneous animator entries. 64 is comfortably above the
/// 16-zone live seed scene (2 stateful channels × 16 zones = 32 after the
/// #2 step-6 PillExpand removal) and keeps the inline backing array small.
/// Spec §10 — zero alloc.
pub const ANIMATOR_CAPACITY: usize = 64;

/// V-8 — hover-in duration. Tauri reference: ~160 ms snap.
pub const HOVER_IN_DURATION_MS: u32 = 160;
/// V-8 — hover-out duration. Slightly longer so the pill settles gracefully.
pub const HOVER_OUT_DURATION_MS: u32 = 220;
/// V-8 — press-down duration. Quick haptic cue, sub-100 ms.
pub const PRESS_DOWN_DURATION_MS: u32 = 80;
/// V-8 — press-release duration.
pub const PRESS_UP_DURATION_MS: u32 = 120;
// #2 step 6 (2026-06-02) — `EXPAND_DURATION_MS` (=200) and the dead
// `AnimChannel::PillExpand` it timed were REMOVED. The pill↔panel expand is
// owned solely by the Wave G2 `zone_pill_anim_*` state machine on the M3
// easeOutBack@500ms curve; the V-8 PillExpand channel was never sampled
// (render samples only PillHover/PillPress), a dead parallel timeline.
/// V-8 — status-dot pulse period. 1600 ms full cycle = 0.6 → 1.0 → 0.6.
pub const PULSE_PERIOD_MS: u32 = 1600;
/// V-12 (2026-05-21) — pill hover scale **disabled** (0.0). The collapsed
/// pill's `scale_rect_centered` was scaling the outer surface rect but the
/// icon/label/badge sub-rects stayed at their original (un-scaled) positions
/// → user saw "胶囊显示不完全" because the chrome grew but the content did
/// not. Tauri 1.2.4 baseline does NOT scale the pill on hover either — it
/// only brightens the surface tone. We keep the hover animator running so
/// the surface-brighten reaction still feels alive but the geometry is now
/// pinned to the persisted token rect.
pub const HOVER_SCALE_DELTA: f32 = 0.0;
/// V-12 (2026-05-21) — pill press inverse-scale **disabled** (0.0). Same
/// reasoning as `HOVER_SCALE_DELTA` above; press still drives the surface
/// dim via a future channel but no longer perturbs the rect geometry.
pub const PRESS_SCALE_DELTA: f32 = 0.0;
/// V-8 — pulse alpha floor and span. Final alpha = floor + span * pulse_t.
pub const PULSE_ALPHA_FLOOR: f32 = 0.6;
pub const PULSE_ALPHA_SPAN: f32 = 0.4;

/// The live per-pill animation channels. Stored alongside the target `ZoneId`
/// so the same animator can drive an arbitrary number of pills.
///
/// #2 step 6 (2026-06-02) — the dead `PillExpand` channel was removed; the
/// structural expand is owned by the Wave G2 `zone_pill_anim_*` state machine,
/// not this animator (the renderer samples only `PillHover`/`PillPress`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimChannel {
    PillHover,
    PillPress,
    /// Status-dot pulse is global (every count>0 dot pulses in lockstep), so
    /// the sampler ignores the `ZoneId` for this channel.
    StatusDotPulse,
}

/// Easing curve selector. Pure functions, no allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    /// Linear — `t`. Used by `StatusDotPulse` for its triangle-wave phase
    /// before the sine remap.
    Linear,
    /// Ease-out cubic — `1 - (1-t)³`. Decelerating curve so the pill snaps
    /// quickly then settles. (The standalone `zone_pill_geometry`
    /// ease-out-cubic helper was removed in M3 once the live pill morph moved
    /// to the easeOutBack curve; this variant keeps its own inline impl below.)
    EaseOutCubic,
}
// #2 step 6 (2026-06-02) — the `EaseInOutQuad` selector variant was removed
// with the dead `PillExpand` channel (it had no other live producer). The
// `ease_in_out_quad` curve fn below is kept — `status_dot_pulse` calls it
// directly to smooth the breathing triangle wave.

#[inline]
pub fn ease_linear(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

#[inline]
pub fn ease_out_cubic(t: f32) -> f32 {
    let c = t.clamp(0.0, 1.0);
    let inv = 1.0 - c;
    1.0 - inv * inv * inv
}

#[inline]
pub fn ease_in_out_quad(t: f32) -> f32 {
    let c = t.clamp(0.0, 1.0);
    if c < 0.5 {
        2.0 * c * c
    } else {
        let inv = -2.0 * c + 2.0;
        1.0 - inv * inv * 0.5
    }
}

#[inline]
fn apply_easing(easing: Easing, t: f32) -> f32 {
    match easing {
        Easing::Linear => ease_linear(t),
        Easing::EaseOutCubic => ease_out_cubic(t),
    }
}

/// One in-flight animation entry. `Copy` so we can store it inline in a
/// fixed-size array with no allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Entry {
    zone: ZoneId,
    channel: AnimChannel,
    start_ms: u32,
    duration_ms: u32,
    from: f32,
    to: f32,
    easing: Easing,
}

/// Inline fixed-capacity animator. Stores up to `ANIMATOR_CAPACITY` entries
/// in a `[Option<Entry>; N]` so allocations stay off the hot path.
#[derive(Debug)]
pub struct Animator {
    entries: [Option<Entry>; ANIMATOR_CAPACITY],
}

impl Default for Animator {
    fn default() -> Self {
        Self::new()
    }
}

impl Animator {
    pub const fn new() -> Self {
        Self {
            entries: [None; ANIMATOR_CAPACITY],
        }
    }

    /// Start an animation on the given (zone, channel) tuple, replacing any
    /// in-flight entry so hover-in followed quickly by hover-out doesn't
    /// queue. The animator resumes from the **current sampled value** when
    /// the direction reverses — see `start_or_reverse`.
    // Geometric/easing-parameter animator entry point: the 8 primitives
    // (zone, channel, timing, from/to, easing) map 1:1 onto an `Entry` and
    // bundling them into a wrapper struct only adds indirection at the hot
    // call sites. Conventional render/animation-code shape — allow it.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        zone: ZoneId,
        channel: AnimChannel,
        now_ms: u32,
        duration_ms: u32,
        from: f32,
        to: f32,
        easing: Easing,
    ) {
        let entry = Entry {
            zone,
            channel,
            start_ms: now_ms,
            duration_ms: duration_ms.max(1),
            from,
            to,
            easing,
        };
        // Try to overwrite an existing slot first.
        for existing in self.entries.iter_mut().flatten() {
            if existing.zone == zone && existing.channel == channel {
                *existing = entry;
                return;
            }
        }
        // Otherwise occupy the first free slot.
        for slot in self.entries.iter_mut() {
            if slot.is_none() {
                *slot = Some(entry);
                return;
            }
        }
        // Saturated — drop the oldest by index. With 16 zones × 3 channels
        // this is unreachable in normal play.
        self.entries[0] = Some(entry);
    }

    /// Reverse-start: if an entry exists, sample its current value and
    /// restart from that value toward `to`. Eliminates the "snap to from"
    /// flicker when hover-in mid-animation flips to hover-out.
    pub fn start_or_reverse(
        &mut self,
        zone: ZoneId,
        channel: AnimChannel,
        now_ms: u32,
        duration_ms: u32,
        to: f32,
        easing: Easing,
    ) {
        let current = self.sample(zone, channel, now_ms);
        self.start(zone, channel, now_ms, duration_ms, current, to, easing);
    }

    /// Cancel an entry, freeing its slot. No-op if not present.
    pub fn cancel(&mut self, zone: ZoneId, channel: AnimChannel) {
        for slot in self.entries.iter_mut() {
            if let Some(existing) = slot {
                if existing.zone == zone && existing.channel == channel {
                    *slot = None;
                    return;
                }
            }
        }
    }

    /// Sample the eased current value for `(zone, channel)`. Returns 0.0 if
    /// no entry exists. For `StatusDotPulse` the zone is ignored — the
    /// pulse is a global breathing curve sourced from `now_ms`.
    pub fn sample(&self, zone: ZoneId, channel: AnimChannel, now_ms: u32) -> f32 {
        if matches!(channel, AnimChannel::StatusDotPulse) {
            return status_dot_pulse(now_ms);
        }
        for entry in self.entries.iter().flatten() {
            if entry.zone == zone && entry.channel == channel {
                let elapsed = now_ms.wrapping_sub(entry.start_ms);
                if elapsed >= entry.duration_ms {
                    return entry.to;
                }
                let raw = (elapsed as f32) / (entry.duration_ms as f32);
                let eased = apply_easing(entry.easing, raw);
                return entry.from + (entry.to - entry.from) * eased;
            }
        }
        0.0
    }

    /// Per-frame retire pass. Drops entries whose `elapsed >= duration` AND
    /// whose terminal value is 0.0 (i.e. the channel is fully idle). Keeps
    /// `to = 1.0` entries pinned so subsequent samples still report 1.0 for
    /// "held" hover/press states. Returns `true` if any entry was active.
    pub fn tick(&mut self, now_ms: u32) -> bool {
        let mut any_active = false;
        for slot in self.entries.iter_mut() {
            if let Some(entry) = slot {
                let elapsed = now_ms.wrapping_sub(entry.start_ms);
                if elapsed < entry.duration_ms {
                    any_active = true;
                    continue;
                }
                // Terminal — keep only if value is non-zero (held state).
                if entry.to.abs() < f32::EPSILON {
                    *slot = None;
                }
            }
        }
        any_active
    }

    /// True if any non-terminal entry exists. Used by the render pump to
    /// keep requesting redraws while animations are mid-flight.
    pub fn is_active(&self, now_ms: u32) -> bool {
        for entry in self.entries.iter().flatten() {
            let elapsed = now_ms.wrapping_sub(entry.start_ms);
            if elapsed < entry.duration_ms {
                return true;
            }
        }
        false
    }

    /// Count of occupied slots — used by unit tests and diagnostics.
    pub fn occupancy(&self) -> usize {
        self.entries.iter().filter(|s| s.is_some()).count()
    }
}

/// Global status-dot pulse — pure function of wall-clock time. Returns
/// 0.0 → 1.0 → 0.0 over `PULSE_PERIOD_MS`. Triangle phase is remapped
/// through `ease_in_out_quad` so the breathing reads as smooth (no kink at
/// the half-cycle apex).
#[inline]
pub fn status_dot_pulse(now_ms: u32) -> f32 {
    let phase = (now_ms % PULSE_PERIOD_MS) as f32 / (PULSE_PERIOD_MS as f32);
    // Triangle wave: 0 → 1 → 0 across the period.
    let triangle = if phase < 0.5 {
        phase * 2.0
    } else {
        (1.0 - phase) * 2.0
    };
    ease_in_out_quad(triangle)
}

/// Compose hover + press into the final pill scale multiplier. Hover
/// inflates by up to +4%; press deflates by up to -3% multiplied on top.
#[inline]
pub fn pill_scale_for(hover_t: f32, press_t: f32) -> f32 {
    let h = hover_t.clamp(0.0, 1.0);
    let p = press_t.clamp(0.0, 1.0);
    (1.0 + h * HOVER_SCALE_DELTA) * (1.0 - p * PRESS_SCALE_DELTA)
}

/// Apply a centered scale to a `Rect` without mutating the input. Used at
/// paint time so the persisted geometry tokens stay untouched (V-8 hard
/// constraint).
#[inline]
pub fn scale_rect_centered(rect: bento_nano_style::Rect, scale: f32) -> bento_nano_style::Rect {
    let cx = rect.x + rect.width * 0.5;
    let cy = rect.y + rect.height * 0.5;
    let nw = rect.width * scale;
    let nh = rect.height * scale;
    bento_nano_style::Rect {
        x: cx - nw * 0.5,
        y: cy - nh * 0.5,
        width: nw,
        height: nh,
    }
}

/// Final status-dot alpha = floor + span * pulse_t. Pure function so the
/// renderer can call it inline without touching animator state.
#[inline]
pub fn status_dot_alpha(now_ms: u32) -> f32 {
    PULSE_ALPHA_FLOOR + PULSE_ALPHA_SPAN * status_dot_pulse(now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(n: u64) -> ZoneId {
        ZoneId(n)
    }

    // ----- Easing function pin tests --------------------------------------

    #[test]
    fn ease_linear_endpoints_and_midpoint() {
        assert!(ease_linear(0.0).abs() < f32::EPSILON);
        assert!((ease_linear(0.25) - 0.25).abs() < 1e-6);
        assert!((ease_linear(0.5) - 0.5).abs() < 1e-6);
        assert!((ease_linear(0.75) - 0.75).abs() < 1e-6);
        assert!((ease_linear(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ease_out_cubic_pinned_samples() {
        // t=0 → 0; t=1 → 1; midpoint is past 0.5 (decelerating).
        assert!(ease_out_cubic(0.0).abs() < f32::EPSILON);
        // 1 - (1 - 0.25)^3 = 1 - 0.421875 = 0.578125
        assert!((ease_out_cubic(0.25) - 0.578_125).abs() < 1e-5);
        // 1 - 0.5^3 = 0.875
        assert!((ease_out_cubic(0.5) - 0.875).abs() < 1e-5);
        // 1 - 0.25^3 = 0.984375
        assert!((ease_out_cubic(0.75) - 0.984_375).abs() < 1e-5);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ease_in_out_quad_pinned_samples() {
        assert!(ease_in_out_quad(0.0).abs() < f32::EPSILON);
        // 2 * 0.25^2 = 0.125
        assert!((ease_in_out_quad(0.25) - 0.125).abs() < 1e-5);
        // midpoint exactly 0.5 (symmetric S)
        assert!((ease_in_out_quad(0.5) - 0.5).abs() < 1e-5);
        // 1 - (-2*0.75+2)^2 / 2 = 1 - 0.5^2 / 2 = 1 - 0.125 = 0.875
        assert!((ease_in_out_quad(0.75) - 0.875).abs() < 1e-5);
        assert!((ease_in_out_quad(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ease_curves_monotonic_across_range() {
        let mut prev_c = -1.0;
        let mut prev_q = -1.0;
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            let c = ease_out_cubic(t);
            let q = ease_in_out_quad(t);
            assert!(c >= prev_c);
            assert!(q >= prev_q);
            prev_c = c;
            prev_q = q;
        }
    }

    // ----- Animator state-machine tests -----------------------------------

    #[test]
    fn start_and_sample_at_zero_returns_from() {
        let mut a = Animator::new();
        a.start(
            z(1),
            AnimChannel::PillHover,
            1_000,
            160,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        let s = a.sample(z(1), AnimChannel::PillHover, 1_000);
        assert!(s.abs() < 1e-5);
    }

    #[test]
    fn sample_at_full_duration_returns_to() {
        let mut a = Animator::new();
        a.start(
            z(2),
            AnimChannel::PillPress,
            100,
            80,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        let s = a.sample(z(2), AnimChannel::PillPress, 100 + 80);
        assert!((s - 1.0).abs() < 1e-5);
    }

    #[test]
    fn sample_past_end_clamps_to_terminal() {
        let mut a = Animator::new();
        a.start(
            z(3),
            AnimChannel::PillPress,
            0,
            200,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        let s = a.sample(z(3), AnimChannel::PillPress, 10_000);
        assert!((s - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cancel_drops_entry() {
        let mut a = Animator::new();
        a.start(
            z(4),
            AnimChannel::PillHover,
            0,
            160,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        assert_eq!(a.occupancy(), 1);
        a.cancel(z(4), AnimChannel::PillHover);
        assert_eq!(a.occupancy(), 0);
        // sampling after cancel returns 0 (no entry).
        assert!(a.sample(z(4), AnimChannel::PillHover, 80).abs() < f32::EPSILON);
    }

    #[test]
    fn start_overwrites_existing_slot_no_growth() {
        let mut a = Animator::new();
        a.start(
            z(5),
            AnimChannel::PillHover,
            0,
            160,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        a.start(
            z(5),
            AnimChannel::PillHover,
            50,
            220,
            0.5,
            0.0,
            Easing::EaseOutCubic,
        );
        assert_eq!(a.occupancy(), 1);
    }

    #[test]
    fn start_or_reverse_picks_up_current_value() {
        let mut a = Animator::new();
        a.start(
            z(6),
            AnimChannel::PillHover,
            0,
            160,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        // Sample halfway.
        let mid = a.sample(z(6), AnimChannel::PillHover, 80);
        assert!(mid > 0.0 && mid < 1.0);
        // Reverse mid-flight back toward 0.0.
        a.start_or_reverse(
            z(6),
            AnimChannel::PillHover,
            80,
            220,
            0.0,
            Easing::EaseOutCubic,
        );
        let after = a.sample(z(6), AnimChannel::PillHover, 80);
        // Continuity — the sample at the reversal moment equals `mid`.
        assert!((after - mid).abs() < 1e-4);
    }

    #[test]
    fn multiple_zones_independent() {
        let mut a = Animator::new();
        a.start(
            z(10),
            AnimChannel::PillHover,
            0,
            160,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        a.start(
            z(11),
            AnimChannel::PillHover,
            0,
            160,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        a.cancel(z(10), AnimChannel::PillHover);
        // 11 still present.
        assert!(a.sample(z(11), AnimChannel::PillHover, 160) > 0.99);
        // 10 cleared.
        assert!(a.sample(z(10), AnimChannel::PillHover, 160).abs() < f32::EPSILON);
    }

    #[test]
    fn tick_retires_zero_terminal_entries() {
        let mut a = Animator::new();
        a.start(
            z(12),
            AnimChannel::PillHover,
            0,
            160,
            1.0,
            0.0,
            Easing::EaseOutCubic,
        );
        a.start(
            z(13),
            AnimChannel::PillHover,
            0,
            160,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        let _ = a.tick(160);
        // Zero-terminal retired, one-terminal held.
        assert!(a.sample(z(12), AnimChannel::PillHover, 200).abs() < f32::EPSILON);
        assert!((a.sample(z(13), AnimChannel::PillHover, 200) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn is_active_tracks_in_flight_entries() {
        let mut a = Animator::new();
        assert!(!a.is_active(0));
        a.start(
            z(14),
            AnimChannel::PillPress,
            0,
            200,
            0.0,
            1.0,
            Easing::EaseOutCubic,
        );
        assert!(a.is_active(100));
        assert!(!a.is_active(300));
    }

    // ----- Status-dot pulse -----------------------------------------------

    #[test]
    fn pulse_is_zero_at_period_boundary_and_peaks_midcycle() {
        // Period boundary → phase 0 → triangle 0 → eased 0.
        assert!(status_dot_pulse(0).abs() < 1e-5);
        assert!(status_dot_pulse(PULSE_PERIOD_MS).abs() < 1e-5);
        // Midcycle → phase 0.5 → triangle 1 → eased 1.
        let peak = status_dot_pulse(PULSE_PERIOD_MS / 2);
        assert!((peak - 1.0).abs() < 1e-5);
    }

    #[test]
    fn status_dot_alpha_within_token_range() {
        // Floor at trough, floor+span at peak.
        let trough = status_dot_alpha(0);
        let peak = status_dot_alpha(PULSE_PERIOD_MS / 2);
        assert!((trough - PULSE_ALPHA_FLOOR).abs() < 1e-5);
        assert!((peak - (PULSE_ALPHA_FLOOR + PULSE_ALPHA_SPAN)).abs() < 1e-5);
        // Sanity bounds.
        for t in [0_u32, 200, 400, 800, 1200, 1600, 3200] {
            let a = status_dot_alpha(t);
            assert!((0.0..=1.0).contains(&a));
        }
    }

    // ----- Geometry helpers -----------------------------------------------

    #[test]
    fn pill_scale_for_combines_hover_and_press() {
        // V-12 (2026-05-21) — HOVER_SCALE_DELTA + PRESS_SCALE_DELTA both
        // 0.0 so the pill rect is pinned to the persisted token geometry.
        // Hover/press channels still drive surface tone (brighten/dim) but
        // do NOT perturb the rect — fixes "胶囊显示不完全" where the surface
        // grew under hover but icon/label/badge stayed at their pre-scale
        // positions.
        assert!((pill_scale_for(0.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((pill_scale_for(1.0, 0.0) - 1.0).abs() < 1e-5);
        assert!((pill_scale_for(0.0, 1.0) - 1.0).abs() < 1e-5);
        assert!((pill_scale_for(1.0, 1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn scale_rect_centered_preserves_center() {
        let r = bento_nano_style::Rect {
            x: 100.0,
            y: 50.0,
            width: 200.0,
            height: 40.0,
        };
        let s = scale_rect_centered(r, 1.04);
        let cx = r.x + r.width * 0.5;
        let cy = r.y + r.height * 0.5;
        let scx = s.x + s.width * 0.5;
        let scy = s.y + s.height * 0.5;
        assert!((cx - scx).abs() < 1e-3);
        assert!((cy - scy).abs() < 1e-3);
        assert!((s.width - 208.0).abs() < 1e-3);
        assert!((s.height - 41.6).abs() < 1e-3);
    }

    #[test]
    fn scale_rect_centered_identity_when_scale_one() {
        let r = bento_nano_style::Rect {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 30.0,
        };
        let s = scale_rect_centered(r, 1.0);
        assert!((s.x - r.x).abs() < 1e-5);
        assert!((s.y - r.y).abs() < 1e-5);
        assert!((s.width - r.width).abs() < 1e-5);
        assert!((s.height - r.height).abs() < 1e-5);
    }

    // ----- Per-channel duration constants pinned --------------------------

    #[test]
    fn duration_constants_are_pinned() {
        // V-8 contract — these are user-visible feel knobs. Any silent
        // drift would change the pill cadence; pin them.
        assert_eq!(HOVER_IN_DURATION_MS, 160);
        assert_eq!(HOVER_OUT_DURATION_MS, 220);
        assert_eq!(PRESS_DOWN_DURATION_MS, 80);
        assert_eq!(PRESS_UP_DURATION_MS, 120);
        // #2 step 6 (2026-06-02) — EXPAND_DURATION_MS removed with the dead
        // PillExpand channel; the expand timeline is the Wave G2
        // ZONE_PILL_ANIM_DURATION_MS (500ms) pinned in zone_pill_geometry.
        assert_eq!(PULSE_PERIOD_MS, 1_600);
    }

    #[test]
    // Intentional compile-time-constant guard: asserts the const inline-capacity
    // covers the seeded scene demand.
    #[allow(clippy::assertions_on_constants)]
    fn capacity_above_seed_scene_demand() {
        // #2 step 6 (2026-06-02) — after removing the dead PillExpand channel
        // there are 2 stateful per-zone channels (hover/press; StatusDotPulse
        // is global and never stored). 16 seeded zones × 2 = 32.
        assert!(ANIMATOR_CAPACITY >= 32);
    }
}
