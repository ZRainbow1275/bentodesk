#[test]
fn current_morph_rect_matches_shared_geometry_timeline_formula() {
    // #2 step 8 (2026-06-02) — `current_morph_rect` is the single SSoT for the
    // content-raw→geometry-raw→monotonic-ease-out→morph_pill_to_rect math used
    // by the paint path and BOTH hit-geometry sites. Pin it bit-identical to
    // that shared formula at several `t`, in both directions, so paint == hit
    // can never drift.
    let pill = Rect {
        x: 10.0,
        y: 20.0,
        width: 96.0,
        height: 36.0,
    };
    let expanded = Rect {
        x: 10.0,
        y: 20.0,
        width: 320.0,
        height: 240.0,
    };
    for &raw in &[0.05_f32, 0.25, 0.5, 0.75, 0.95] {
        for &expanding in &[true, false] {
            let eased = ease_out_progress(pill_geometry_progress(raw));
            let from_morph = if expanding { 0.0 } else { 1.0 };
            let target = if expanding { 1.0 } else { 0.0 };
            let morph_expected = from_morph + (target - from_morph) * eased;
            let rect_expected = morph_pill_to_rect(pill, expanded, morph_expected);
            let (morph_actual, rect_actual) =
                current_morph_rect(pill, expanded, from_morph, raw, expanding);
            assert_eq!(
                morph_actual, morph_expected,
                "morph drift @raw={raw} exp={expanding}"
            );
            assert_eq!(rect_actual.x, rect_expected.x);
            assert_eq!(rect_actual.y, rect_expected.y);
            assert_eq!(rect_actual.width, rect_expected.width);
            assert_eq!(rect_actual.height, rect_expected.height);
        }
    }
}

#[test]
fn pill_geometry_and_content_share_fast_release_envelope() {
    assert_eq!(ZONE_PILL_ANIM_DURATION_MS, 240);
    assert_eq!(ZONE_PILL_GEOMETRY_DURATION_MS, 240);
    assert_eq!(pill_geometry_progress(0.0), 0.0);

    let half_geometry = 120.0 / ZONE_PILL_ANIM_DURATION_MS as f32;
    assert!((pill_geometry_progress(half_geometry) - 0.5).abs() < f32::EPSILON);

    let settled_geometry =
        ZONE_PILL_GEOMETRY_DURATION_MS as f32 / ZONE_PILL_ANIM_DURATION_MS as f32;
    assert_eq!(pill_geometry_progress(settled_geometry), 1.0);
    assert_eq!(pill_geometry_progress(1.0), 1.0);
}

#[test]
fn morph_pill_radius_interpolates_between_endpoints() {
    assert_eq!(morph_pill_radius(24.0, 12.0, 0.0), 24.0);
    assert_eq!(morph_pill_radius(24.0, 12.0, 1.0), 12.0);
    let mid = morph_pill_radius(24.0, 12.0, 0.5);
    assert!((mid - 18.0).abs() < 0.001);
}

#[test]
fn pill_anim_duration_keeps_fast_fourteen_frame_envelope_at_sixty_hz() {
    assert_eq!(ZONE_PILL_ANIM_DURATION_MS, 240);
    assert_eq!(ZONE_PILL_GEOMETRY_DURATION_MS, 240);
}

#[test]
fn interrupted_segment_duration_scales_without_snapping() {
    assert_eq!(pill_segment_duration_ms(0.0, 1.0), 240);
    assert_eq!(pill_segment_duration_ms(0.5, 1.0), 120);
    assert_eq!(pill_segment_duration_ms(0.95, 1.0), 50);
}

#[test]
fn interrupted_reverse_starts_at_the_current_visual_morph() {
    let current = current_morph_progress(0.0, 0.25, true).clamp(0.0, 1.0);
    assert_eq!(current_morph_progress(current, 0.0, false), current);
    assert_eq!(current_morph_progress(current, 0.0, true), current);
}

#[test]
fn zone_morph_is_monotonic_and_can_drive_all_visual_channels() {
    let mut previous = 0.0;
    for step in 0..=100 {
        let current = current_morph_progress(0.0, step as f32 / 100.0, true);
        assert!((0.0..=1.0).contains(&current));
        assert!(current >= previous, "morph regressed at step {step}");
        previous = current;
    }
    assert_eq!(previous, 1.0);
}

#[test]
fn zen_content_reflows_inside_the_live_morph_rect() {
    let base = pill_layout_for_zone(&fixture(64, 332), 10);
    let expanded = Rect {
        x: 64.0,
        y: 332.0,
        width: 320.0,
        height: 220.0,
    };
    let live = pill_content_layout_in_rect(base, expanded);

    assert_eq!(pill_content_layout_in_rect(base, base.rect), base);
    assert!((live.icon.x - base.icon.x).abs() < f32::EPSILON);
    assert!(live.icon.y > base.icon.y);
    assert!(live.badge.x > base.badge.x);
    assert!(live.badge.y > base.badge.y);
    assert!(live.label.width > base.label.width);
}

// --- M3 easeOutBack cubic-bezier solver --------------------------------

#[test]
fn ease_out_back_endpoints_are_exact() {
    // Must land on EXACTLY 0.0 and 1.0 — no Newton residual at the
    // boundaries (the settle has to hit the token target precisely).
    assert_eq!(ease_out_back_progress(0.0), 0.0);
    assert_eq!(ease_out_back_progress(1.0), 1.0);
    // Out-of-range clamps to the endpoints.
    assert_eq!(ease_out_back_progress(-0.5), 0.0);
    assert_eq!(ease_out_back_progress(2.0), 1.0);
}

#[test]
fn ease_out_back_overshoots_past_one_midflight() {
    // cubic-bezier(0.34,1.56,0.64,1) bulges ~10% past 1.0 before settling.
    // The peak sits around the input region 0.6..0.85.
    let mut peak = 0.0_f32;
    let mut i = 0;
    while i <= 100 {
        let v = ease_out_back_progress(i as f32 / 100.0);
        if v > peak {
            peak = v;
        }
        i += 1;
    }
    // Overshoot present and in the ~5-15% band (10% nominal).
    assert!(peak > 1.05, "expected overshoot > 1.05, got {peak}");
    assert!(peak < 1.20, "overshoot unexpectedly large: {peak}");
}

#[test]
fn ease_out_back_pinned_samples() {
    // Sampled progress at the report checkpoints. The curve front-loads
    // hard, peaks ~1.098 around t≈0.573, then settles EXACTLY to 1.0.
    // Exact values verified against the reference solver:
    //   t=0.00 → 0.000000   t=0.25 → 0.816289   t=0.50 → 1.087401
    //   t=0.70 → 1.075776   t=1.00 → 1.000000
    assert_eq!(ease_out_back_progress(0.0), 0.0);
    assert_eq!(ease_out_back_progress(1.0), 1.0);
    assert!((ease_out_back_progress(0.25) - 0.816_289).abs() < 1e-3);
    assert!((ease_out_back_progress(0.5) - 1.087_401).abs() < 1e-3);
    assert!((ease_out_back_progress(0.7) - 1.075_776).abs() < 1e-3);
    // The 0.5 and 0.7 samples sit ABOVE 1.0 — the overshoot zone.
    assert!(ease_out_back_progress(0.5) > 1.0);
    assert!(ease_out_back_progress(0.7) > 1.0);
}

#[test]
fn ease_out_back_x_inversion_round_trips() {
    // bezier_solve_x must invert bezier_axis on the x-axis to f32 epsilon.
    let mut i = 0;
    while i <= 20 {
        let x = i as f32 / 20.0;
        let u = bezier_solve_x(x, BEZIER_P1X, BEZIER_P2X);
        let back = bezier_axis(u, BEZIER_P1X, BEZIER_P2X);
        assert!((back - x).abs() < 1e-4, "x={x} round-tripped to {back}");
        i += 1;
    }
}

#[test]
fn morph_pill_to_rect_with_back_curve_overshoots_then_settles() {
    // The overshoot WILL grow the rect past the expanded target mid-flight
    // (correct — Tauri does it) but settle EXACTLY on target at t=1.
    let pill = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 40.0,
    };
    let expanded = Rect {
        x: 0.0,
        y: 0.0,
        width: 300.0,
        height: 200.0,
    };
    // Mid-flight overshoot: find the largest interpolated width.
    let mut max_w = 0.0_f32;
    let mut i = 0;
    while i <= 100 {
        let p = ease_out_back_progress(i as f32 / 100.0);
        let r = morph_pill_to_rect(pill, expanded, p);
        if r.width > max_w {
            max_w = r.width;
        }
        i += 1;
    }
    assert!(
        max_w > expanded.width,
        "expected width overshoot, got {max_w}"
    );
    // Settle is exact at t=1.
    let settled = morph_pill_to_rect(pill, expanded, ease_out_back_progress(1.0));
    assert_eq!(settled, expanded);
}

// --- A3 HoverScheduler grace state machine -----------------------------

const EXPAND_DELAY: u32 = 60;
const COLLAPSE_DELAY: u32 = 150;

#[test]
fn expand_lock_tracks_the_fast_outer_shell_timeline() {
    assert_eq!(EXPAND_LOCK_MS, 260);
    assert_eq!(EXPAND_LOCK_MS, ZONE_PILL_GEOMETRY_DURATION_MS + 20);
    const { assert!(EXPAND_LOCK_MS > ZONE_PILL_ANIM_DURATION_MS) };
}

fn zid(n: u64) -> ZoneId {
    ZoneId(n)
}

#[test]
fn enter_arms_expand_intent_and_fires_after_delay() {
    let mut s = HoverScheduler::new();
    s.on_enter(zid(1), 1_000, EXPAND_DELAY);
    assert!(s.is_pending());
    // Before the delay elapses — nothing fires, zone not yet expanded.
    assert_eq!(s.poll(1_000 + EXPAND_DELAY - 1), HoverAction::None);
    assert_eq!(s.expanded_zone(), None);
    // At the deadline — expand fires exactly once.
    assert_eq!(s.poll(1_000 + EXPAND_DELAY), HoverAction::Expand(zid(1)));
    assert_eq!(s.expanded_zone(), Some(zid(1)));
    // Subsequent polls don't re-fire.
    assert_eq!(s.poll(1_000 + EXPAND_DELAY + 50), HoverAction::None);
}

#[test]
fn leave_before_expand_clears_intent() {
    let mut s = HoverScheduler::new();
    s.on_enter(zid(1), 1_000, EXPAND_DELAY);
    // Cursor leaves before the intent elapses.
    s.on_leave(1_000 + EXPAND_DELAY / 2, COLLAPSE_DELAY, true);
    // No expand should ever fire — there was nothing expanded to collapse.
    assert_eq!(s.poll(1_000 + EXPAND_DELAY), HoverAction::None);
    assert_eq!(s.poll(10_000), HoverAction::None);
    assert_eq!(s.expanded_zone(), None);
    assert!(!s.is_pending());
}

#[test]
fn expand_sets_lock_window() {
    let mut s = HoverScheduler::new();
    s.on_enter(zid(1), 1_000, EXPAND_DELAY);
    assert_eq!(s.poll(1_000 + EXPAND_DELAY), HoverAction::Expand(zid(1)));
    // Leave immediately after expand — collapse must defer to the lock,
    // NOT fire at now + collapse_delay (which is earlier than the lock).
    let leave = 1_000 + EXPAND_DELAY; // == expand tick
    s.on_leave(leave, COLLAPSE_DELAY, true);
    // collapse_delay is shorter than EXPAND_LOCK_MS, so the lock wins.
    let lock_until = leave + EXPAND_LOCK_MS;
    // At now + collapse_delay the lock has NOT elapsed — no collapse yet.
    assert_eq!(s.poll(leave + COLLAPSE_DELAY), HoverAction::None);
    // At the lock deadline the collapse fires.
    assert_eq!(s.poll(lock_until), HoverAction::Collapse(zid(1)));
    assert_eq!(s.expanded_zone(), None);
}

#[test]
fn leave_during_lock_defers_collapse_to_lock_until() {
    let mut s = HoverScheduler::new();
    // Force-expand at t=2000 so the lock window starts at that timestamp.
    s.mark_expanded(zid(7), 2_000);
    assert_eq!(s.expanded_zone(), Some(zid(7)));
    // Leave at t=2100 (inside the lock). base = 2100+150 is still before the
    // selected-stack lock deadline.
    s.on_leave(2_100, COLLAPSE_DELAY, true);
    // One millisecond before the lock deadline — must NOT collapse.
    assert_eq!(s.poll(2_000 + EXPAND_LOCK_MS - 1), HoverAction::None);
    // lock_until — collapse fires.
    assert_eq!(
        s.poll(2_000 + EXPAND_LOCK_MS),
        HoverAction::Collapse(zid(7))
    );
}

#[test]
fn leave_after_lock_sets_collapse_at_now_plus_delay() {
    let mut s = HoverScheduler::new();
    s.mark_expanded(zid(3), 1_000);
    // Leave well after the lock has expired (t=5000). base = 5150.
    s.on_leave(5_000, COLLAPSE_DELAY, true);
    // Before now+delay — nothing.
    assert_eq!(s.poll(5_000 + COLLAPSE_DELAY - 1), HoverAction::None);
    // At now+delay — collapse fires (lock long gone, so base wins).
    assert_eq!(
        s.poll(5_000 + COLLAPSE_DELAY),
        HoverAction::Collapse(zid(3))
    );
}

#[test]
fn reenter_before_collapse_cancels_grace() {
    let mut s = HoverScheduler::new();
    s.mark_expanded(zid(2), 5_000);
    s.on_leave(6_000, COLLAPSE_DELAY, true); // collapse pending at 6150
    assert!(s.is_pending());
    // Cursor re-enters the same zone before the grace elapses.
    s.on_enter(zid(2), 6_100, EXPAND_DELAY);
    // Collapse must be cancelled — no Collapse ever fires.
    assert_eq!(s.poll(6_400), HoverAction::None);
    assert_eq!(s.poll(10_000), HoverAction::None);
    // Zone stays expanded.
    assert_eq!(s.expanded_zone(), Some(zid(2)));
}

#[test]
fn always_mode_leave_does_not_collapse() {
    let mut s = HoverScheduler::new();
    s.mark_expanded(zid(4), 1_000);
    // auto_collapse = false (ALWAYS display mode): leave is a no-op for
    // the collapse path (mirrors Tauri BentoZone.tsx:589).
    s.on_leave(5_000, COLLAPSE_DELAY, false);
    assert_eq!(s.poll(5_000 + COLLAPSE_DELAY), HoverAction::None);
    assert_eq!(s.poll(100_000), HoverAction::None);
    assert_eq!(s.expanded_zone(), Some(zid(4)));
}

#[test]
fn reset_clears_all_pending_and_expanded() {
    let mut s = HoverScheduler::new();
    s.on_enter(zid(1), 0, EXPAND_DELAY);
    s.mark_expanded(zid(1), 0);
    s.reset();
    assert!(!s.is_pending());
    assert_eq!(s.expanded_zone(), None);
    assert_eq!(s.poll(100_000), HoverAction::None);
}

#[test]
fn reached_handles_tick_wraparound() {
    // Deadline just before the u32 wrap; "now" just after — reached.
    let deadline = u32::MAX - 10;
    let now = 5_u32; // wrapped past the deadline by 15ms
    assert!(reached(now, deadline));
    // Now still before the deadline — not reached.
    assert!(!reached(deadline - 100, deadline));
}
