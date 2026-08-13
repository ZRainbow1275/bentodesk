use super::{
    PANEL_ACCENT_EDGE_THICKNESS_PX, expanded_panel_accent_clip_rect, zone_identity_layout_at,
};
use crate::{expanded_zone_grid, zone_pill_geometry};
use bentodesk_style::Rect;
use bentodesk_zone::{Zone, ZoneId};

#[test]
fn expanded_panel_accent_clip_stays_on_panel_top_edge() {
    let panel = Rect {
        x: 64.0,
        y: 332.0,
        width: 320.0,
        height: 220.0,
    };
    let clip = expanded_panel_accent_clip_rect(panel);
    assert_eq!(clip.x, panel.x);
    assert_eq!(clip.y, panel.y);
    assert_eq!(clip.width, panel.width);
    assert_eq!(clip.height, PANEL_ACCENT_EDGE_THICKNESS_PX);
}

#[test]
fn expanded_panel_accent_clip_does_not_overflow_short_panel() {
    let panel = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 1.0,
    };
    let clip = expanded_panel_accent_clip_rect(panel);
    assert_eq!(clip.height, panel.height);
}

#[test]
fn morph_identity_row_has_exact_collapsed_and_expanded_endpoints() {
    let mut zone = Zone::new(ZoneId(1), "Benchmark Zone", 20, 30, 320, 240);
    for index in 0..10 {
        zone.add_item(format!("C:/item-{index}.txt"), "builtin:file")
            .expect("fixture item");
    }
    let zen = zone_pill_geometry::pill_layout_for_zone(&zone, zone.items.len());
    let panel = expanded_zone_grid::expanded_zone_layout_for_rect(
        Rect {
            x: 20.0,
            y: 30.0,
            width: 320.0,
            height: 240.0,
        },
        10,
    );

    let collapsed = zone_identity_layout_at(&zone, panel.panel, 0.0);
    assert_eq!(collapsed.icon, zen.icon);
    assert_eq!(collapsed.label, zen.label);
    assert_eq!(collapsed.badge, zen.badge);

    let expanded = zone_identity_layout_at(&zone, panel.panel, 1.0);
    assert_eq!(expanded.icon, panel.header_icon);
    assert_eq!(expanded.badge, panel.header_badge);
    assert_eq!(expanded.rect, panel.header_band);
}

#[test]
fn morph_identity_row_moves_monotonically_in_every_direction() {
    for panel_rect in [
        Rect {
            x: 20.0,
            y: 30.0,
            width: 320.0,
            height: 240.0,
        },
        Rect {
            x: -140.0,
            y: 30.0,
            width: 320.0,
            height: 240.0,
        },
        Rect {
            x: 20.0,
            y: -160.0,
            width: 320.0,
            height: 240.0,
        },
        Rect {
            x: -140.0,
            y: -160.0,
            width: 320.0,
            height: 240.0,
        },
    ] {
        let zone = Zone::new(ZoneId(1), "Benchmark Zone", 20, 30, 320, 240);
        let pill = zone_pill_geometry::pill_layout_for_zone(&zone, 10);
        let panel = expanded_zone_grid::expanded_zone_layout_for_rect(panel_rect, 10);
        let end = zone_identity_layout_at(&zone, panel.panel, 1.0);
        let mut previous = pill;

        for step in 1..=20 {
            let current = zone_identity_layout_at(&zone, panel.panel, step as f32 / 20.0);
            for (start, before, now, target) in [
                (pill.icon.x, previous.icon.x, current.icon.x, end.icon.x),
                (pill.icon.y, previous.icon.y, current.icon.y, end.icon.y),
                (pill.label.x, previous.label.x, current.label.x, end.label.x),
                (pill.label.y, previous.label.y, current.label.y, end.label.y),
                (pill.badge.x, previous.badge.x, current.badge.x, end.badge.x),
                (pill.badge.y, previous.badge.y, current.badge.y, end.badge.y),
            ] {
                let low = start.min(target) - f32::EPSILON;
                let high = start.max(target) + f32::EPSILON;
                assert!((low..=high).contains(&now));
                assert!((target - now).abs() <= (target - before).abs() + f32::EPSILON);
            }
            previous = current;
        }

        assert_eq!(previous, end);
    }
}
