use super::{
    collapsed_pill_display_count, format_small_count, tauri_badge_fill, tauri_zone_accent_color,
};
use crate::{AppState, zone_pill_geometry};
use bento_nano_style::Color;
use bento_nano_zone::{Zone, ZoneId};

fn zone_with_items(id: u64, item_count: usize) -> Zone {
    let mut zone = Zone::new(ZoneId(id), format!("Zone {id}"), 40, 40, 220, 140);
    for index in 0..item_count {
        let _ = zone.add_item(format!("C:/proof/zone-{id}/item-{index}.txt"), "");
    }
    zone
}

#[test]
fn badge_fill_matches_tauri_zone_accent_fallback_contract() {
    let fallback = Color::from_u8(0xFF, 0xFF, 0xFF, 0x1F);

    assert_eq!(tauri_badge_fill(None, fallback), fallback);
    assert_eq!(tauri_badge_fill(Some(""), fallback), fallback);
    assert_eq!(tauri_badge_fill(Some("#zzzzzz"), fallback), fallback);
    assert_eq!(
        tauri_badge_fill(Some("#3B82F6"), fallback),
        Color::from_u8(0x3B, 0x82, 0xF6, 0xE0)
    );
    assert_eq!(tauri_zone_accent_color(None), None);
}

#[test]
fn normal_collapsed_pill_uses_item_count() {
    let mut app = AppState::new();
    app.zones.add(zone_with_items(1, 3));
    let zone = app.zones.get(ZoneId(1)).expect("zone");

    assert_eq!(collapsed_pill_display_count(&app, zone), 3);
    assert_eq!(
        format_small_count(collapsed_pill_display_count(&app, zone)),
        "3"
    );
}

#[test]
fn stack_anchor_collapsed_capsule_uses_stack_member_count_for_layout_and_text() {
    let mut app = AppState::new();
    app.zones.add(zone_with_items(1, 10));
    app.zones.add(zone_with_items(2, 1));
    assert!(app.zones.stack(ZoneId(1), ZoneId(2)));

    let anchor = app.zones.get(ZoneId(1)).expect("anchor");
    let display_count = collapsed_pill_display_count(&app, anchor);
    let layout = zone_pill_geometry::stack_capsule_layout_for_zone(anchor, display_count);
    let count_text = format_small_count(display_count);

    assert_eq!(display_count, 2);
    assert_eq!(count_text, "2");
    assert_ne!(format_small_count(anchor.items.len()), count_text);
    assert_eq!(layout.peek_visible_count, 2);
    assert!(layout.badge.width >= zone_pill_geometry::STACK_CAPSULE_BADGE_MIN_WIDTH_PX);
    assert!(layout.rect.width > 160.0);
}
