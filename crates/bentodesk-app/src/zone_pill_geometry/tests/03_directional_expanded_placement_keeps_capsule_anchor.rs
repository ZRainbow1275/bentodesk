#[test]
fn directional_expanded_placement_covers_four_quadrants_and_center_ties() {
    let viewport = bentodesk_style::Size {
        width: 1000.0,
        height: 800.0,
    };
    let cases = [
        (Rect { x: 100.0, y: 100.0, width: 120.0, height: 48.0 }, false, false),
        (Rect { x: 800.0, y: 100.0, width: 120.0, height: 48.0 }, true, false),
        (Rect { x: 100.0, y: 700.0, width: 120.0, height: 48.0 }, false, true),
        (Rect { x: 800.0, y: 700.0, width: 120.0, height: 48.0 }, true, true),
        (Rect { x: 440.0, y: 376.0, width: 120.0, height: 48.0 }, false, false),
    ];

    for (capsule, anchor_right, anchor_bottom) in cases {
        let placement = expanded_zone_placement(capsule, 400.0, 300.0, viewport);
        assert_eq!(placement.anchor_right, anchor_right);
        assert_eq!(placement.anchor_bottom, anchor_bottom);
        if anchor_right {
            assert_eq!(placement.panel.right(), capsule.right());
        } else {
            assert_eq!(placement.panel.x, capsule.x);
        }
        if anchor_bottom {
            assert_eq!(placement.panel.bottom(), capsule.bottom());
        } else {
            assert_eq!(placement.panel.y, capsule.y);
        }
        assert!(placement.panel.x >= 0.0 && placement.panel.y >= 0.0);
        assert!(placement.panel.right() <= viewport.width);
        assert!(placement.panel.bottom() <= viewport.height);
    }
}

#[test]
fn directional_placement_shrinks_to_fractional_or_degenerate_viewport() {
    let fractional = bentodesk_style::Size {
        width: 1707.75,
        height: 912.8,
    };
    let placement = expanded_zone_placement(
        Rect { x: 1600.0, y: 850.0, width: 100.0, height: 52.0 },
        4000.0,
        4000.0,
        fractional,
    );
    assert!(placement.anchor_right && placement.anchor_bottom);
    assert_eq!(placement.panel, Rect { x: 0.0, y: 0.0, width: 1700.0, height: 902.0 });

    let degenerate = expanded_zone_placement(
        Rect { x: 0.0, y: 0.0, width: 120.0, height: 48.0 },
        400.0,
        300.0,
        bentodesk_style::Size {
            width: f32::NAN,
            height: -1.0,
        },
    );
    assert_eq!(degenerate.panel, Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 });
}

#[test]
fn production_morph_keeps_selected_anchor_edges_for_every_sample() {
    let viewport = bentodesk_style::Size {
        width: 1000.0,
        height: 800.0,
    };
    for capsule in [
        Rect { x: 100.0, y: 100.0, width: 120.0, height: 48.0 },
        Rect { x: 800.0, y: 100.0, width: 120.0, height: 48.0 },
        Rect { x: 100.0, y: 700.0, width: 120.0, height: 48.0 },
        Rect { x: 800.0, y: 700.0, width: 120.0, height: 48.0 },
    ] {
        let placement = expanded_zone_placement(capsule, 400.0, 300.0, viewport);
        for step in 0..=20 {
            let morph = current_morph_progress(0.0, step as f32 / 20.0, true);
            let rect = morph_pill_to_rect(capsule, placement.panel, morph);
            if placement.anchor_right {
                assert!((rect.right() - capsule.right()).abs() < 0.01);
            } else {
                assert!((rect.x - capsule.x).abs() < 0.01);
            }
            if placement.anchor_bottom {
                assert!((rect.bottom() - capsule.bottom()).abs() < 0.01);
            } else {
                assert!((rect.y - capsule.y).abs() < 0.01);
            }
            assert!(rect.x >= 0.0 && rect.y >= 0.0);
            assert!(rect.right() <= viewport.width + 0.01);
            assert!(rect.bottom() <= viewport.height + 0.01);
        }
    }
}

#[test]
fn capsule_origin_clamp_floors_logical_viewport_bounds() {
    let viewport = bentodesk_style::Size {
        width: 1707.9,
        height: 912.9,
    };
    assert_eq!(
        clamp_capsule_origin_to_viewport(5000, 5000, 220, 52, viewport),
        (1487, 860)
    );
    assert_eq!(
        clamp_capsule_origin_to_viewport(-20, -30, 120, 48, viewport),
        (0, 0)
    );
}
