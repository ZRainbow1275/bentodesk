    use super::*;
    use crate::ZoneResizeHandle;
    use bentodesk_style::Rect;
    use bentodesk_zone::{Zone, ZoneId, ZoneList};

    fn resize_session(anchor_right: bool, anchor_bottom: bool) -> ZoneResizeSession {
        let handle = match (anchor_right, anchor_bottom) {
            (false, false) => ZoneResizeHandle::BottomRight,
            (true, false) => ZoneResizeHandle::BottomLeft,
            (false, true) => ZoneResizeHandle::TopRight,
            (true, true) => ZoneResizeHandle::TopLeft,
        };
        ZoneResizeSession {
            id: ZoneId(9),
            start_pointer_x: 100.0,
            start_pointer_y: 100.0,
            handle,
            start_panel: Rect {
                x: 40.0,
                y: 40.0,
                width: 240.0,
                height: 180.0,
            },
            start_persisted_width: 240,
            start_persisted_height: 180,
            start_home_x: if anchor_right { 232 } else { 40 },
            start_home_y: if anchor_bottom { 172 } else { 40 },
            start_capsule: Rect {
                x: if anchor_right { 232.0 } else { 40.0 },
                y: if anchor_bottom { 172.0 } else { 40.0 },
                width: 48.0,
                height: 48.0,
            },
            anchor_right,
            anchor_bottom,
        }
    }

    #[test]
    fn directional_resize_has_no_first_move_jump_and_tracks_all_four_corners() {
        for anchor_right in [false, true] {
            for anchor_bottom in [false, true] {
                let session = resize_session(anchor_right, anchor_bottom);
                let at_start = directional_resize_geometry(
                    session,
                    100.0,
                    100.0,
                    Size {
                        width: 600.0,
                        height: 500.0,
                    },
                    80.0,
                    60.0,
                );
                assert_eq!(at_start.width, Some(240));
                assert_eq!(at_start.height, Some(180));
                assert_eq!(at_start.home_x, None);
                assert_eq!(at_start.home_y, None);
                let pointer_x = if anchor_right { 80.0 } else { 120.0 };
                let pointer_y = if anchor_bottom { 70.0 } else { 130.0 };
                let result = directional_resize_geometry(
                    session,
                    pointer_x,
                    pointer_y,
                    Size {
                        width: 600.0,
                        height: 500.0,
                    },
                    80.0,
                    60.0,
                );
                assert_eq!(result.width, Some(260));
                assert_eq!(result.height, Some(210));
            }
        }
    }

    #[test]
    fn directional_resize_away_then_back_restores_snapshot_axes_and_anchor_home() {
        let mut session = resize_session(false, false);
        session.handle = ZoneResizeHandle::BottomLeft;
        let viewport = Size {
            width: 600.0,
            height: 500.0,
        };

        let away = directional_resize_geometry(session, 60.0, 140.0, viewport, 80.0, 60.0);
        assert_eq!(away.width, Some(280));
        assert_eq!(away.height, Some(220));
        assert_eq!(away.home_x, Some(0));
        assert_eq!(away.home_y, None, "inactive home axis must remain untouched");

        let back = directional_resize_geometry(session, 100.0, 100.0, viewport, 80.0, 60.0);
        assert_eq!(back.width, Some(session.start_persisted_width));
        assert_eq!(back.height, Some(session.start_persisted_height));
        assert_eq!(back.home_x, Some(session.start_home_x));
        assert_eq!(back.home_y, None, "inactive home axis must remain untouched");
    }

    #[test]
    fn all_eight_handles_write_only_their_active_axes() {
        use ZoneResizeHandle::*;

        for handle in [
            Left,
            Right,
            Top,
            Bottom,
            TopLeft,
            TopRight,
            BottomLeft,
            BottomRight,
        ] {
            let session = ZoneResizeSession {
                id: ZoneId(10),
                start_pointer_x: 200.0,
                start_pointer_y: 200.0,
                handle,
                start_panel: Rect {
                    x: 100.0,
                    y: 100.0,
                    width: 240.0,
                    height: 180.0,
                },
                start_persisted_width: 240,
                start_persisted_height: 180,
                start_home_x: 100,
                start_home_y: 100,
                start_capsule: Rect {
                    x: 100.0,
                    y: 100.0,
                    width: 48.0,
                    height: 48.0,
                },
                anchor_right: false,
                anchor_bottom: false,
            };
            let pointer_x = if handle.drags_left() { 190.0 } else { 210.0 };
            let pointer_y = if handle.drags_top() { 190.0 } else { 210.0 };
            let result = directional_resize_geometry(
                session,
                pointer_x,
                pointer_y,
                Size {
                    width: 800.0,
                    height: 600.0,
                },
                80.0,
                60.0,
            );

            assert_eq!(result.width.is_some(), handle.resizes_horizontally());
            assert_eq!(result.height.is_some(), handle.resizes_vertically());
            assert_eq!(result.home_x.is_some(), handle.drags_left());
            assert_eq!(result.home_y.is_some(), handle.drags_top());
        }
    }

    #[test]
    fn sub_dip_motion_does_not_commit_until_selected_edge_rounds() {
        let mut session = resize_session(false, false);
        session.handle = ZoneResizeHandle::Right;
        assert_eq!(
            directional_resize_geometry(
                session,
                100.49,
                100.0,
                Size {
                    width: 600.0,
                    height: 500.0,
                },
                80.0,
                60.0,
            )
            .width,
            Some(240)
        );
        assert_eq!(
            directional_resize_geometry(
                session,
                100.51,
                100.0,
                Size {
                    width: 600.0,
                    height: 500.0,
                },
                80.0,
                60.0,
            )
            .width,
            Some(241)
        );
    }

    #[test]
    fn anchored_side_home_rounding_keeps_release_recompute_in_original_quadrant() {
        let viewport = Size {
            width: 600.0,
            height: 500.0,
        };
        for mut session in [
            ZoneResizeSession {
                id: ZoneId(11),
                start_pointer_x: 0.0,
                start_pointer_y: 0.0,
                handle: ZoneResizeHandle::TopLeft,
                start_panel: Rect {
                    x: 40.0,
                    y: 40.0,
                    width: 540.0,
                    height: 440.0,
                },
                start_persisted_width: 540,
                start_persisted_height: 440,
                start_home_x: 40,
                start_home_y: 40,
                start_capsule: Rect {
                    x: 40.0,
                    y: 40.0,
                    width: 48.0,
                    height: 48.0,
                },
                anchor_right: false,
                anchor_bottom: false,
            },
            ZoneResizeSession {
                id: ZoneId(12),
                start_pointer_x: 0.0,
                start_pointer_y: 0.0,
                handle: ZoneResizeHandle::BottomRight,
                start_panel: Rect {
                    x: 88.0,
                    y: 48.0,
                    width: 360.0,
                    height: 300.0,
                },
                start_persisted_width: 360,
                start_persisted_height: 300,
                start_home_x: 400,
                start_home_y: 300,
                start_capsule: Rect {
                    x: 400.0,
                    y: 300.0,
                    width: 48.0,
                    height: 48.0,
                },
                anchor_right: true,
                anchor_bottom: true,
            },
        ] {
            let result = directional_resize_geometry(
                session,
                if session.anchor_right { -500.0 } else { 500.0 },
                if session.anchor_bottom { -500.0 } else { 500.0 },
                viewport,
                80.0,
                60.0,
            );
            let home_x = result.home_x.expect("anchored horizontal side moves home");
            let home_y = result.home_y.expect("anchored vertical side moves home");
            session.start_capsule.x = home_x as f32;
            session.start_capsule.y = home_y as f32;
            let width = result.width.expect("width changed") as f32;
            let height = result.height.expect("height changed") as f32;
            let recomputed =
                expanded_zone_placement(session.start_capsule, width, height, viewport);

            assert_eq!(recomputed.anchor_right, session.anchor_right);
            assert_eq!(recomputed.anchor_bottom, session.anchor_bottom);
            let fixed_x = if session.handle.drags_left() {
                recomputed.panel.right()
            } else {
                recomputed.panel.x
            };
            let start_fixed_x = if session.handle.drags_left() {
                session.start_panel.right()
            } else {
                session.start_panel.x
            };
            let fixed_y = if session.handle.drags_top() {
                recomputed.panel.bottom()
            } else {
                recomputed.panel.y
            };
            let start_fixed_y = if session.handle.drags_top() {
                session.start_panel.bottom()
            } else {
                session.start_panel.y
            };
            assert!((fixed_x - start_fixed_x).abs() <= 1.0);
            assert!((fixed_y - start_fixed_y).abs() <= 1.0);
        }
    }

    #[test]
    fn directional_resize_respects_directional_max_even_below_normal_minimum() {
        let mut session = resize_session(false, false);
        session.start_panel.width = 50.0;
        session.start_panel.height = 40.0;
        session.start_persisted_width = 50;
        session.start_persisted_height = 40;
        let result = directional_resize_geometry(
            session,
            500.0,
            500.0,
            Size {
                width: 90.0,
                height: 80.0,
            },
            80.0,
            60.0,
        );
        assert_eq!(result.width, Some(50));
        assert_eq!(result.height, Some(40));
    }

    #[test]
    fn edge_resize_never_writes_the_inactive_axis_or_clipped_persisted_size() {
        let mut session = resize_session(false, false);
        session.handle = ZoneResizeHandle::Right;
        let changed = directional_resize_geometry(
            session,
            120.0,
            400.0,
            Size {
                width: 600.0,
                height: 500.0,
            },
            80.0,
            60.0,
        );
        assert_eq!(changed.width, Some(260));
        assert_eq!(changed.height, None);
        assert_eq!(changed.home_y, None);

        session.start_persisted_width = 400;
        let clipped = directional_resize_geometry(
            session,
            140.0,
            100.0,
            Size {
                width: session.start_panel.right(),
                height: 500.0,
            },
            80.0,
            60.0,
        );
        assert_eq!(clipped.width, Some(400));
        assert_eq!(clipped.height, None);
    }

    #[test]
    fn legacy_below_minimum_can_grow_but_not_shrink_on_first_frame() {
        let mut session = resize_session(false, false);
        session.handle = ZoneResizeHandle::Right;
        session.start_panel.width = 70.0;
        session.start_persisted_width = 70;
        assert_eq!(
            directional_resize_geometry(
                session,
                90.0,
                100.0,
                Size {
                    width: 600.0,
                    height: 500.0
                },
                80.0,
                60.0,
            )
            .width,
            Some(70)
        );
        assert_eq!(
            directional_resize_geometry(
                session,
                110.0,
                100.0,
                Size {
                    width: 600.0,
                    height: 500.0
                },
                80.0,
                60.0,
            )
            .width,
            Some(80)
        );
    }

    // ── exceeds_drag_threshold ────────────────────────────────────────────

    #[test]
    fn threshold_below_4_dip_is_still_a_click() {
        assert!(!exceeds_drag_threshold(0, 0));
        assert!(!exceeds_drag_threshold(3, 0));
        assert!(!exceeds_drag_threshold(0, 3));
        // (2,2) ⇒ 8 < 16 ⇒ still a click.
        assert!(!exceeds_drag_threshold(2, 2));
        // (-3, 0) symmetric with (3, 0).
        assert!(!exceeds_drag_threshold(-3, 0));
    }

    #[test]
    fn threshold_at_or_past_4_dip_is_a_drag() {
        // (4,0) ⇒ 16 >= 16 ⇒ drag (the first qualifying frame).
        assert!(exceeds_drag_threshold(4, 0));
        assert!(exceeds_drag_threshold(0, 4));
        // (3,3) ⇒ 18 >= 16 ⇒ drag.
        assert!(exceeds_drag_threshold(3, 3));
        assert!(exceeds_drag_threshold(-3, 3));
        assert!(exceeds_drag_threshold(100, -100));
    }

    #[test]
    fn threshold_does_not_overflow_on_large_deltas() {
        // 50_000² = 2.5e9 > i32::MAX (~2.1e9), so a same-width i32 multiply
        // would overflow; the i64 widen keeps it safe for any realistic
        // multi-monitor desktop delta (Tauri parity range is a few thousand).
        assert!(exceeds_drag_threshold(50_000, 50_000));
        assert!(exceeds_drag_threshold(-50_000, 50_000));
    }

    // ── score_stack_candidate (pure rect math) ────────────────────────────

    #[test]
    fn score_far_apart_does_not_fire() {
        // Two 200x200 zones, centres ~1000 apart, no overlap, far beyond
        // proximity radius (0.8 * (100 + 100) = 160).
        assert!(score_stack_candidate((0, 0, 200, 200), (1000, 0, 200, 200)).is_none());
    }

    #[test]
    fn score_full_overlap_fires_with_high_score() {
        // Identical rects ⇒ overlapRatio ≈ 1.0 ⇒ score ≈ 2.0.
        let s = score_stack_candidate((0, 0, 200, 200), (0, 0, 200, 200)).unwrap();
        assert!((s - 2.0).abs() < 1e-3, "score was {s}");
    }

    #[test]
    fn score_25_percent_overlap_below_threshold_with_far_centres() {
        // Construct ~25% overlap of the smaller area but keep centres outside
        // the proximity radius so trigger B does NOT rescue it.
        // self 200x200 at (0,0): area 40000. other 200x200 shifted so the
        // intersection area is ~25% of 40000 = 10000 ⇒ overlap rect 100x100.
        // overlap 100x100 needs other to start at (100,100): inter = [100,200)
        // both axes ⇒ 100x100 = 10000 ⇒ ratio 0.25 < 0.30.
        // BUT centres: self (100,100), other (200,200) ⇒ dist ~141.4, radius
        // 160 ⇒ proximity WOULD fire. So push other further to (160,160):
        // inter_w = 200-160 = 40, area 1600 ⇒ ratio 0.04; centres (100,100)
        // vs (260,260) ⇒ dist ~226 > 160 ⇒ neither fires.
        assert!(score_stack_candidate((0, 0, 200, 200), (160, 160, 200, 200)).is_none());
    }

    #[test]
    fn score_35_percent_overlap_fires() {
        // self 200x200 at (0,0); other shifted to give >30% overlap of the
        // smaller area. other at (74,0): inter_w = 200-74 = 126, inter_h 200
        // ⇒ 25200 / 40000 = 0.63 > 0.30 ⇒ fires.
        let s = score_stack_candidate((0, 0, 200, 200), (74, 0, 200, 200)).unwrap();
        // overlap > 0 ⇒ score = ratio + 1 > 1.
        assert!(s > 1.0, "overlapping score must exceed 1.0, was {s}");
    }

    #[test]
    fn score_proximity_only_fires_below_one() {
        // No overlap but centres within 0.8*(rSelf+rOther). Two 100x100 zones
        // (r = 50 each), radius = 0.8 * 100 = 80. Place them edge-touching so
        // there is zero overlap (gap of 0) but centres 100 apart along x:
        // self (0,0,100,100) centre (50,50); other (100,0,100,100) centre
        // (150,50) ⇒ dist 100 > 80 ⇒ no fire. Move closer: other at (60,0):
        // inter_w = 100-60 = 40 ⇒ overlap fires. To get PURE proximity with
        // zero overlap we need a gap on one axis but proximity on centres —
        // use a vertical gap: self (0,0,100,100), other (0,120,100,100):
        // no overlap (gap 20 on y), centres (50,50) vs (50,170) ⇒ dist 120 >
        // 80 ⇒ no fire. Tighten gap: other (0,100,100,100): touching, dist
        // 100 > 80 still. The proximity trigger only beats overlap when boxes
        // are SMALL relative to radius; use 100x100 with a diagonal near-touch
        // that has zero AABB overlap is geometrically impossible here, so test
        // proximity via a thin gap that yields ratio 0 yet dist <= radius:
        // self (0,0,200,40) centre (100,20) r=(200+40)/4=60; other
        // (0,50,200,40) centre (100,70) r=60; radius=0.8*120=96; dist=50<=96;
        // AABB overlap: y [0,40) vs [50,90) ⇒ none ⇒ pure proximity.
        let s = score_stack_candidate((0, 0, 200, 40), (0, 50, 200, 40)).unwrap();
        // Pure proximity ⇒ score in (0, 1].
        assert!(
            s > 0.0 && s <= 1.0,
            "proximity score must be in (0,1], was {s}"
        );
    }

    // ── stack_target_for_drop (ZoneList integration, no window) ───────────

    fn zone_at(id: u64, x: i32, y: i32, w: i32, h: i32) -> Zone {
        Zone::new(ZoneId(id), "z", x, y, w, h)
    }

    #[test]
    fn drop_far_from_all_returns_none() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 0, 0, 200, 200));
        zones.add(zone_at(2, 5000, 5000, 200, 200));
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn expanded_panel_size_does_not_create_an_invisible_merge_halo() {
        let mut zones = ZoneList::new();
        // The persisted expanded panels overlap by 300×600 DIP, but the
        // painted medium capsules are only 214×48 and sit 286 DIP apart.
        // Tauri scores the capsules, so this drop must remain independent.
        zones.add(zone_at(1, 0, 100, 800, 600));
        zones.add(zone_at(2, 500, 100, 800, 600));

        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn painted_capsule_overlap_forms_a_stack_regardless_of_panel_size() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 100, 100, 80, 60));
        zones.add(zone_at(2, 180, 100, 900, 700));

        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), Some(ZoneId(2)));
    }

    #[test]
    fn drop_onto_overlapping_zone_returns_that_anchor() {
        let mut zones = ZoneList::new();
        // Dragged (id 1) fully inside id 2 → high overlap.
        zones.add(zone_at(1, 50, 50, 100, 100));
        zones.add(zone_at(2, 0, 0, 300, 300));
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), Some(ZoneId(2)));
    }

    #[test]
    fn self_is_never_returned() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 0, 0, 200, 200));
        // Only the dragged zone exists → no target.
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn highest_scoring_candidate_wins() {
        let mut zones = ZoneList::new();
        // Dragged at (100,100,100,100), centre (150,150).
        zones.add(zone_at(1, 100, 100, 100, 100));
        // Candidate 2: small overlap.
        zones.add(zone_at(2, 180, 100, 100, 100)); // inter_w=20 ⇒ ratio 0.2 (<0.30) but proximity may fire
        // Candidate 3: near-full overlap ⇒ much higher score.
        zones.add(zone_at(3, 110, 110, 100, 100));
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), Some(ZoneId(3)));
    }

    #[test]
    fn locked_candidate_is_skipped() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 50, 50, 100, 100));
        let mut locked = zone_at(2, 0, 0, 300, 300);
        locked.locked = true;
        zones.add(locked);
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn invisible_candidate_is_skipped() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 50, 50, 100, 100));
        let mut hidden = zone_at(2, 0, 0, 300, 300);
        hidden.visible = false;
        zones.add(hidden);
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn stacked_child_candidate_is_skipped() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 50, 50, 100, 100));
        let mut child = zone_at(2, 0, 0, 300, 300);
        child.stack_parent = Some(ZoneId(9)); // is_stacked_child() == true
        zones.add(child);
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn same_stack_candidate_is_skipped() {
        let mut zones = ZoneList::new();
        // Anchor (id 2) overlaps dragged (id 1); make id 1 already a member of
        // id 2's stack so they share the same anchor ⇒ skip the restack.
        zones.add(zone_at(1, 50, 50, 100, 100));
        zones.add(zone_at(2, 0, 0, 300, 300));
        assert!(zones.stack(ZoneId(2), ZoneId(1)));
        // Now dragged (1) shares anchor (2) with the only candidate (2, the
        // anchor). is_stacked_child(1)==true is irrelevant for the dragged;
        // the candidate 2 shares the anchor with 1 ⇒ skip ⇒ None.
        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }

    #[test]
    fn existing_stack_anchor_does_not_enter_the_free_zone_auto_merge_path() {
        let mut zones = ZoneList::new();
        zones.add(zone_at(1, 100, 100, 400, 300));
        zones.add(zone_at(2, 100, 100, 400, 300));
        zones.add(zone_at(3, 100, 100, 400, 300));
        assert!(zones.stack(ZoneId(1), ZoneId(2)));

        assert_eq!(stack_target_for_drop(&zones, ZoneId(1)), None);
    }
