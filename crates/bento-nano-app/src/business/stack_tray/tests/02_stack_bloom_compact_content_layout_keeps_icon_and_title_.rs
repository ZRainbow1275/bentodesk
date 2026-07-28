#[test]
fn stack_bloom_compact_content_layout_keeps_icon_and_title_separated() {
    let petal = Rect {
        x: 8.0,
        y: 8.0,
        width: BLOOM_PETAL_WIDTH_COMPACT_PX,
        height: BLOOM_PETAL_HEIGHT_COMPACT_PX,
    };

    let layout = stack_bloom_petal_content_layout(petal, BLOOM_PETAL_ICON_COMPACT_PX, 1.0);

    assert!(layout.icon_rect.height > 0.0);
    assert!(layout.title_rect.height > 0.0);
    assert!(layout.icon_rect.bottom() + BLOOM_PETAL_CONTENT_GAP_PX <= layout.title_rect.y + 0.01);
    assert!(layout.title_rect.bottom() <= petal.bottom() + 0.01);
}

#[test]
fn stack_bloom_petals_follow_member_count_and_stay_stable() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
    let capsule = zone_pill_geometry::stack_capsule_layout_for_zone(&zone, 8).rect;

    let rects = stack_bloom_petal_rects(viewport, &zone, 8);

    assert_eq!(rects.len(), 8);
    assert!(rects.windows(2).all(|pair| {
        (pair[0].y - pair[1].y).abs() < 0.01
            && pair[0].right() + BLOOM_PETAL_GAP_PX <= pair[1].x + 0.01
    }));
    assert!(
        rects
            .iter()
            .all(|rect| rect.y >= capsule.bottom() + BLOOM_PETAL_GAP_BELOW_CAPSULE_PX - 0.01)
    );
}

#[test]
fn stack_bloom_row_clamps_near_viewport_edge() {
    let viewport = Size {
        width: 500.0,
        height: 360.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 340, 120, 140, 100);

    let rects = stack_bloom_petal_rects(viewport, &zone, 3);

    assert_eq!(rects.len(), 3);
    assert!(rects.iter().all(|rect| rect.x >= BLOOM_VIEWPORT_INSET_PX
        && rect.right() <= viewport.width - BLOOM_VIEWPORT_INSET_PX + 0.01));
}

#[test]
fn stack_bloom_hit_test_returns_petal_index() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
    let rects = stack_bloom_petal_rects(viewport, &zone, 4);
    let target = rects[2];

    let hit = stack_bloom_hit_test(viewport, &zone, 4, target.x + 4.0, target.y + 4.0);

    assert_eq!(hit, Some(2));
}

#[test]
fn stack_bloom_hit_test_uses_tauri_twelve_pixel_petal_halo() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
    let first = stack_bloom_petal_rects(viewport, &zone, 4)[0];
    let y = first.y + first.height * 0.5;

    assert_eq!(
        stack_bloom_hit_test(viewport, &zone, 4, first.x - 8.0, y),
        Some(0)
    );
    assert_eq!(
        stack_bloom_hit_test(viewport, &zone, 4, first.x - 13.0, y),
        None
    );
}

#[test]
fn stack_bloom_caps_slots_and_reserves_the_last_for_overflow() {
    let viewport = Size {
        width: 1920.0,
        height: 1080.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

    assert_eq!(
        stack_bloom_petal_rects(viewport, &zone, 30).len(),
        BLOOM_VISIBLE_PETAL_LIMIT
    );
    assert_eq!(stack_bloom_overflow_count(24), 0);
    assert_eq!(stack_bloom_overflow_count(25), 2);
    assert_eq!(stack_bloom_overflow_count(30), 7);
    assert_eq!(stack_bloom_member_index_for_petal(25, 22), Some(22));
    assert_eq!(stack_bloom_member_index_for_petal(25, 23), None);
    assert_eq!(stack_bloom_member_index_for_petal(24, 23), Some(23));
    assert_eq!(stack_bloom_member_index_for_petal(24, 24), None);
}

#[test]
fn stack_bloom_frames_apply_staggered_motion_without_losing_hit_targets() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

    let frames = stack_bloom_frames(viewport, &zone, 5);
    let partial = stack_bloom_frames_at(viewport, &zone, 5, 0.45);

    assert_eq!(frames.len(), 5);
    assert_eq!(partial.len(), 5);
    assert!(
        partial
            .windows(2)
            .all(|pair| pair[0].progress >= pair[1].progress)
    );
    assert!(partial[0].progress > partial[partial.len() - 1].progress);
    assert!(frames.iter().all(|frame| {
        (frame.progress - 1.0).abs() < 0.01
            && (frame.scale - 1.0).abs() < 0.01
            && (frame.alpha - 1.0).abs() < 0.01
            && frame.connector.width >= 0.0
            && frame.rect.x >= BLOOM_VIEWPORT_INSET_PX
            && frame.rect.right() <= viewport.width - BLOOM_VIEWPORT_INSET_PX
    }));
    assert_eq!(
        stack_bloom_hit_test(
            viewport,
            &zone,
            5,
            frames[3].rect.x + 4.0,
            frames[3].rect.y + 4.0
        ),
        Some(3)
    );
}

#[test]
fn stack_bloom_frames_progress_from_anchor_to_settled_geometry() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

    let start = stack_bloom_frames_at(viewport, &zone, 4, 0.0);
    let midway = stack_bloom_frames_at(viewport, &zone, 4, 0.45);
    let settled = stack_bloom_frames_at(viewport, &zone, 4, 1.0);

    assert_eq!(start.len(), 4);
    assert_eq!(midway.len(), 4);
    assert_eq!(settled.len(), 4);
    assert!((start[0].scale - BLOOM_MOTION_MIN_SCALE).abs() < 0.01);
    assert!((start[0].alpha - BLOOM_MOTION_MIN_ALPHA).abs() < 0.01);
    assert!((start[0].scale - 0.4).abs() < 0.01);
    assert!(start[0].alpha.abs() < 0.01);
    assert!(
        start
            .iter()
            .all(|frame| frame.progress.abs() < f32::EPSILON)
    );
    assert!(midway[0].progress >= midway[1].progress);
    assert!(midway[0].progress > midway[3].progress);
    assert!(midway[0].progress > start[0].progress);
    assert!(settled[0].progress >= midway[0].progress);
    assert!(settled[3].progress > midway[3].progress);
    assert!(start[0].rect.x > settled[0].rect.x);
    assert!(start[3].rect.x < settled[3].rect.x);
    assert!(start[0].rect.y < settled[0].rect.y);
    assert!(start[0].alpha <= midway[0].alpha && midway[0].alpha <= settled[0].alpha);
    assert!(start[0].scale <= midway[0].scale);
    assert!((settled[0].scale - 1.0).abs() < 0.01);
    let capsule = zone_pill_geometry::stack_capsule_layout_for_zone(&zone, 4).rect;
    assert!((rect_center_x(start[0].rect) - rect_center_x(capsule)).abs() < 0.01);
}

#[test]
fn stack_bloom_two_petal_entry_preserves_a_short_stagger_window() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
    let duration = stack_bloom_reveal_duration_ms(2);
    let stagger_window_progress = 80.0 / duration as f32;

    let stagger_window = stack_bloom_frames_at(viewport, &zone, 2, stagger_window_progress);
    let settled = stack_bloom_frames_at(viewport, &zone, 2, 1.0);

    assert_eq!(duration, 390);
    assert!(stagger_window[0].progress > 0.0);
    assert!(stagger_window[1].progress.abs() < f32::EPSILON);
    assert!(stagger_window[1].alpha < settled[1].alpha);
    assert!(stagger_window[1].scale < settled[1].scale);
    assert!((stagger_window[1].rect.x - settled[1].rect.x).abs() > 1.0);
}

#[test]
fn stack_bloom_exit_keeps_petals_visible_during_fast_reverse_stagger() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);
    let duration = stack_bloom_exit_duration_ms(2);
    let exit_80ms_progress = 80.0 / duration as f32;

    let stable = stack_bloom_frames_at(viewport, &zone, 2, 1.0);
    let leaving = stack_bloom_exit_frames_at(viewport, &zone, 2, exit_80ms_progress);

    assert_eq!(duration, 115);
    assert!(duration <= BLOOM_EXIT_VISIBLE_DURATION_MS);
    assert_eq!(leaving.len(), 2);
    assert!(leaving[0].alpha > 0.0);
    assert!(leaving[1].alpha > 0.0);
    assert!(leaving[0].alpha < stable[0].alpha);
    assert!(leaving[1].alpha < stable[1].alpha);
    assert!(leaving[0].rect.width < stable[0].rect.width);
    assert!(leaving[1].rect.width < stable[1].rect.width);
    assert!((leaving[0].rect.x - stable[0].rect.x).abs() > 1.0);
    assert!((leaving[1].rect.x - stable[1].rect.x).abs() > 1.0);
    assert!(
        leaving[0].alpha > leaving[1].alpha,
        "reverse stagger should keep the first-in petal visible longer"
    );
}

#[test]
fn stack_bloom_settled_frames_never_overlap_in_row_or_grid() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 120, 240, 180, 130);

    let frames = stack_bloom_frames_at(viewport, &zone, 5, 1.0);

    assert_eq!(frames.len(), 5);
    assert!(
        frames.windows(2).all(|pair| {
            pair[0].rect.right() <= pair[1].rect.x || pair[0].rect.bottom() <= pair[1].rect.y
        }),
        "settled bloom frames must stay separated"
    );
}

#[test]
fn stack_wrapper_halo_scales_with_visible_member_count() {
    let zone = anchor();

    let one = stack_wrapper_halo_rect(&zone, 1);
    let many = stack_wrapper_halo_rect(&zone, 8);

    assert!(many.x < one.x);
    assert!(many.y < one.y);
    assert!(many.width > one.width);
    assert!(many.height > one.height);
    let expected_width =
        zone.w as f32 + (BLOOM_WRAPPER_BASE_PAD_PX + 8.0 * BLOOM_WRAPPER_MEMBER_PAD_PX) * 2.0;
    assert!((many.width - expected_width).abs() < f32::EPSILON);
}
