use super::{
    PANEL_ACCENT_EDGE_THICKNESS_PX, expanded_panel_accent_clip_rect, morph_zen_content_to_header,
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
    let zone = Zone::new(ZoneId(1), "Benchmark Zone", 20, 30, 320, 240);
    let zen = zone_pill_geometry::pill_layout_for_zone(&zone, 10);
    let panel = expanded_zone_grid::expanded_zone_layout_for_rect(
        Rect {
            x: 20.0,
            y: 30.0,
            width: 320.0,
            height: 240.0,
        },
        10,
    );

    let collapsed = morph_zen_content_to_header(zen, &panel, 0.0);
    assert_eq!(collapsed.icon, zen.icon);
    assert_eq!(collapsed.label, zen.label);
    assert_eq!(collapsed.badge, zen.badge);

    let expanded = morph_zen_content_to_header(zen, &panel, 1.0);
    assert_eq!(expanded.icon, panel.header_icon);
    assert_eq!(expanded.badge, panel.header_badge);
    assert_eq!(expanded.rect, panel.header_band);
}
