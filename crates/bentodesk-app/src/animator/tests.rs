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
fn previous_presented_frame_keeps_a_just_started_animation_at_from() {
    let mut a = Animator::new();
    a.start(
        z(20),
        AnimChannel::PillMorph,
        1_001,
        240,
        0.25,
        1.0,
        Easing::PillMorph,
    );

    assert_eq!(elapsed_ms_at_or_after(1_000, 1_001), 0);
    assert_eq!(a.sample(z(20), AnimChannel::PillMorph, 1_000), 0.25);
    assert!(a.is_active_entry(z(20), AnimChannel::PillMorph, 1_000));
    assert!(a.is_active(1_000));
    assert!(a.needs_tick(1_000));
    assert!(a.tick(1_000));
    assert!(a.contains(z(20), AnimChannel::PillMorph));
}

#[test]
fn elapsed_time_distinguishes_real_tick_wrap_from_a_pre_start_sample() {
    assert_eq!(elapsed_ms_at_or_after(10, u32::MAX - 5), 16);
    assert_eq!(elapsed_ms_at_or_after(u32::MAX - 5, 10), 0);
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
fn pill_morph_entries_advance_and_retire_independently() {
    let mut a = Animator::new();
    for (zone, from, to) in [(z(20), 0.0, 1.0), (z(21), 1.0, 0.0)] {
        a.start(
            zone,
            AnimChannel::PillMorph,
            0,
            240,
            from,
            to,
            Easing::PillMorph,
        );
    }
    let first = a.sample(z(20), AnimChannel::PillMorph, 120);
    let second = a.sample(z(21), AnimChannel::PillMorph, 120);
    assert!(first > 0.0 && first < 1.0);
    assert!(second > 0.0 && second < 1.0);
    assert!((first + second - 1.0).abs() < 1e-5);
    assert!(a.tick(239));
    assert!(a.needs_tick(240));
    assert!(!a.tick(240));
    assert!(!a.needs_tick(240));
    assert_eq!(a.occupancy(), 0);
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
fn stack_emerge_remaining_fraction_retires_at_tauri_duration() {
    let mut a = Animator::new();
    a.start(
        z(14),
        AnimChannel::StackEmerge,
        1_000,
        STACK_EMERGE_DURATION_MS,
        1.0,
        0.0,
        Easing::Linear,
    );

    assert_eq!(a.sample(z(14), AnimChannel::StackEmerge, 1_000), 1.0);
    assert!((a.sample(z(14), AnimChannel::StackEmerge, 1_120) - 0.5).abs() < 1e-5);
    assert!(a.tick(1_239));
    assert!(!a.tick(1_240));
    assert_eq!(a.occupancy(), 0);
    assert_eq!(a.sample(z(14), AnimChannel::StackEmerge, 1_240), 0.0);
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
    let r = bentodesk_style::Rect {
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
    let r = bentodesk_style::Rect {
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
    assert_eq!(HOVER_IN_DURATION_MS, 120);
    assert_eq!(HOVER_OUT_DURATION_MS, 160);
    assert_eq!(PRESS_DOWN_DURATION_MS, 80);
    assert_eq!(PRESS_UP_DURATION_MS, 120);
    // #2 step 6 (2026-06-02) — EXPAND_DURATION_MS removed with the dead
    // PillExpand channel; the expand timeline is the Wave G2
    // ZONE_PILL_ANIM_DURATION_MS pinned in zone_pill_geometry.
    assert_eq!(PULSE_PERIOD_MS, 1_600);
}

#[test]
// Intentional compile-time-constant guard: asserts the const inline-capacity
// covers the seeded scene demand.
#[allow(clippy::assertions_on_constants)]
fn capacity_above_seed_scene_demand() {
    // Structural morph, hover and press can overlap across all 16 seeded
    // Zones. Inline search and stack-emerge are sparse one-shot channels.
    assert!(ANIMATOR_CAPACITY >= 16 * 3);
}
