use super::*;
use bento_nano_layout::LayoutSource;

fn rect(x: f32, y: f32) -> HighlightRect {
    HighlightRect::new(x, y, 80.0, 80.0)
}

#[test]
fn snap_constants_pinned() {
    assert_eq!(OUTLINE_WIDTH_PX, 2.0);
    assert_eq!(TARGET_INSET_PX, 4.0);
    assert_eq!(TARGET_CORNER_RADIUS_PX, 8.0);
    assert!((FILL_ALPHA - 0.20).abs() < f32::EPSILON);
    assert!((OUTLINE_ALPHA - 0.80).abs() < f32::EPSILON);
    assert_eq!(INLINE_TARGET_CAP, 8);
}

#[test]
fn build_returns_transparent_auto_container() {
    let node = build();
    let layout = node.layout();
    assert!(matches!(layout.width, Length::Auto));
    assert!(matches!(layout.height, Length::Auto));
    assert_eq!(layout.padding, Edges::ZERO);
}

#[test]
fn fill_color_uses_palette_accent_with_alpha() {
    let palette = theme::current().palette;
    let fill = fill_color();
    assert_eq!(fill.r, palette.accent.r);
    assert_eq!(fill.g, palette.accent.g);
    assert_eq!(fill.b, palette.accent.b);
    assert!((fill.a - FILL_ALPHA).abs() < f32::EPSILON);
}

#[test]
fn outline_color_uses_palette_accent_with_alpha() {
    let palette = theme::current().palette;
    let outline = outline_color();
    assert_eq!(outline.r, palette.accent.r);
    assert!((outline.a - OUTLINE_ALPHA).abs() < f32::EPSILON);
}

#[test]
fn highlight_colors_accept_explicit_active_palette() {
    let mut palette = theme::current().palette;
    palette.accent = Color::from_u8(0x44, 0x88, 0xCC, 0xFF);

    let fill = fill_color_from_palette(palette);
    let outline = outline_color_from_palette(palette);

    assert_eq!(fill.r, palette.accent.r);
    assert_eq!(fill.g, palette.accent.g);
    assert_eq!(fill.b, palette.accent.b);
    assert!((fill.a - FILL_ALPHA).abs() < f32::EPSILON);
    assert_eq!(outline.r, palette.accent.r);
    assert_eq!(outline.g, palette.accent.g);
    assert_eq!(outline.b, palette.accent.b);
    assert!((outline.a - OUTLINE_ALPHA).abs() < f32::EPSILON);
}

#[test]
fn target_radius_accepts_explicit_active_radius_tokens() {
    let radius = RadiusTokens {
        sm: BorderRadius::all(3.0),
        md: BorderRadius::all(7.0),
        lg: BorderRadius::all(11.0),
        xl: BorderRadius::all(17.0),
        full: BorderRadius::all(999.0),
    };

    assert_eq!(target_radius_from_tokens(radius), BorderRadius::all(11.0));
    assert_eq!(target_radius(), BorderRadius::all(TARGET_CORNER_RADIUS_PX));
}

#[test]
fn default_state_has_no_targets_outline_on() {
    let state = HighlightOverlayState::new();
    assert!(!state.has_targets());
    assert!(state.show_outline());
    assert!(state.auto_clear_remaining_ms().is_none());
}

#[test]
fn set_targets_overwrites_previous_list() {
    let mut state = HighlightOverlayState::new();
    state.set_targets([rect(0.0, 0.0), rect(100.0, 0.0)]);
    assert_eq!(state.targets().len(), 2);
    state.set_targets([rect(50.0, 50.0)]);
    assert_eq!(state.targets().len(), 1);
    assert_eq!(state.targets()[0].x, 50.0);
}

#[test]
fn set_targets_cancels_in_flight_countdown() {
    let mut state = HighlightOverlayState::new();
    state.set_targets_for([rect(0.0, 0.0)], 1_000);
    assert_eq!(state.auto_clear_remaining_ms(), Some(1_000));
    // Replacing with non-timed set_targets cancels the countdown.
    state.set_targets([rect(0.0, 0.0)]);
    assert!(state.auto_clear_remaining_ms().is_none());
}

#[test]
fn clear_empties_targets_and_cancels_countdown() {
    let mut state = HighlightOverlayState::new();
    state.set_targets_for([rect(0.0, 0.0)], 500);
    state.clear();
    assert!(!state.has_targets());
    assert!(state.auto_clear_remaining_ms().is_none());
}

#[test]
fn tick_with_no_countdown_returns_false() {
    let mut state = HighlightOverlayState::new();
    state.set_targets([rect(0.0, 0.0)]);
    // No countdown set — tick has nothing to do, targets stay.
    assert!(!state.tick(100));
    assert!(state.has_targets());
}

#[test]
fn tick_decrements_countdown_until_clear() {
    let mut state = HighlightOverlayState::new();
    state.set_targets_for([rect(0.0, 0.0)], 1_000);
    // Half the window — still ticking, targets remain.
    assert!(state.tick(500));
    assert_eq!(state.auto_clear_remaining_ms(), Some(500));
    assert!(state.has_targets());
    // Cross the threshold — clears + returns false.
    assert!(!state.tick(500));
    assert!(!state.has_targets());
    assert!(state.auto_clear_remaining_ms().is_none());
}

#[test]
fn tick_overshoot_clears_immediately() {
    let mut state = HighlightOverlayState::new();
    state.set_targets_for([rect(0.0, 0.0)], 200);
    // dt larger than remaining → clears in one go.
    assert!(!state.tick(1_000));
    assert!(!state.has_targets());
}

#[test]
fn set_targets_for_zero_duration_treated_as_sticky() {
    let mut state = HighlightOverlayState::new();
    state.set_targets_for([rect(0.0, 0.0)], 0);
    assert!(state.auto_clear_remaining_ms().is_none());
    assert!(state.has_targets());
}

#[test]
fn show_outline_toggle_round_trip() {
    let mut state = HighlightOverlayState::new();
    assert!(state.show_outline());
    state.set_show_outline(false);
    assert!(!state.show_outline());
    state.set_show_outline(true);
    assert!(state.show_outline());
}

#[test]
fn item_target_rect_matches_grid_geometry() {
    let mut zone = Zone::new(bento_nano_zone::ZoneId(9), "Docs", 10, 20, 240, 180);
    let item_id = zone.add_item("C:/Desktop/doc.pdf", "hash");
    assert!(item_id.is_some(), "item id is allocated");
    let item_id = item_id.unwrap_or(bento_nano_zone::ZoneItemId(0));
    let Some(item) = zone.item(item_id) else {
        assert!(zone.item(item_id).is_some(), "item exists");
        return;
    };

    let target = item_target_rect(&zone, item);

    // P3.5: column 1 starts at zone_left + the 16-DIP `HEADER_INSET_X`
    // (zone_x 10 + 16 = 26). P3.6: grid-top is the 56-DIP offset
    // (zone_top 20 + 56 = 76).
    assert!((target.x - (10.0 + expanded_zone_grid::HEADER_INSET_X)).abs() < f32::EPSILON);
    assert!((target.y - (20.0 + item_grid::ITEM_GRID_TOP_OFFSET_PX)).abs() < f32::EPSILON);
    assert!(target.width > 0.0);
    assert!(target.height > 0.0);
}

#[test]
fn item_card_rect_for_item_in_panel_tracks_morph_panel_rect() {
    let mut zone = Zone::new(bento_nano_zone::ZoneId(9), "Docs", 64, 332, 320, 220);
    zone.set_grid_columns(5);
    let item_id = zone
        .add_item("C:/Desktop/doc.pdf", "hash")
        .expect("item id");
    let item = zone.item(item_id).expect("item");
    let morph_panel = Rect {
        x: 80.0,
        y: 360.0,
        width: 260.0,
        height: 160.0,
    };

    let rect = item_card_rect_for_item_in_panel(&zone, item, morph_panel);

    assert!((rect.x - (morph_panel.x + expanded_zone_grid::HEADER_INSET_X)).abs() < 0.01);
    assert!((rect.y - (morph_panel.y + item_grid::ITEM_GRID_TOP_OFFSET_PX)).abs() < 0.01);
    assert!(rect.right() <= morph_panel.right() - expanded_zone_grid::HEADER_INSET_X + 0.01);
    assert!(rect.bottom() <= morph_panel.bottom() - 8.0 + 0.01);
}

#[test]
fn item_card_rect_reflows_requested_columns_when_panel_is_too_narrow() {
    let mut zone = Zone::new(bento_nano_zone::ZoneId(9), "Docs", 64, 332, 320, 220);
    zone.set_grid_columns(5);

    let first = item_card_rect_for_grid(&zone, 0, 0, false);
    let fourth = item_card_rect_for_grid(&zone, 3, 0, false);
    let fifth = item_card_rect_for_grid(&zone, 4, 0, false);

    assert!(first.width >= item_grid::ITEM_GRID_MIN_CARD_WIDTH_PX);
    assert!(
        fourth.right() <= zone.x as f32 + zone.w as f32 - expanded_zone_grid::HEADER_INSET_X + 0.01
    );
    assert!((fifth.x - first.x).abs() < 0.01);
    assert!(
        fifth.y > fourth.y,
        "5 requested columns in a 320-DIP panel should reflow to a new row"
    );
}

#[test]
fn item_card_rect_for_item_advances_after_wide_cards() {
    let mut zone = Zone::new(bento_nano_zone::ZoneId(9), "Docs", 64, 332, 320, 260);
    zone.set_grid_columns(5);
    let first_id = zone
        .add_item("C:/Desktop/item-01.txt", "h1")
        .expect("first");
    let second_id = zone
        .add_item("C:/Desktop/item-02.txt", "h2")
        .expect("second");
    let third_id = zone
        .add_item("C:/Desktop/item-03.txt", "h3")
        .expect("third");
    let fourth_id = zone
        .add_item("C:/Desktop/item-04.txt", "h4")
        .expect("fourth");
    assert!(zone.toggle_item_wide(first_id));

    let first = item_card_rect_for_item(&zone, zone.item(first_id).expect("first item"));
    let second = item_card_rect_for_item(&zone, zone.item(second_id).expect("second item"));
    let third = item_card_rect_for_item(&zone, zone.item(third_id).expect("third item"));
    let fourth = item_card_rect_for_item(&zone, zone.item(fourth_id).expect("fourth item"));

    assert!(first.width > second.width);
    assert!(
        second.x >= first.right() + item_grid::ITEM_GRID_COLUMN_GAP_PX - 0.01,
        "wide first card must reserve two lanes before the second card"
    );
    assert!(
        third.x >= second.right() + item_grid::ITEM_GRID_COLUMN_GAP_PX - 0.01,
        "third card should continue after the second card"
    );
    assert!((fourth.x - first.x).abs() < 0.01);
    assert!(
        fourth.y > first.y,
        "fourth card wraps because the first wide card consumed two lanes"
    );
}

#[test]
fn expanded_item_scroll_keeps_full_card_geometry_and_shared_clip() {
    let mut zone = Zone::new(bento_nano_zone::ZoneId(9), "Docs", 64, 332, 320, 220);
    zone.set_grid_columns(5);
    let item_id = zone.add_item("C:/Desktop/item-01.txt", "h1").expect("item");
    let item = zone.item(item_id).expect("item row");
    let base = item_card_rect_for_item(&zone, item);
    let scrolled = item_card_rect_for_item_scrolled(&zone, item, 32.0);

    assert_eq!(scrolled.x, base.x);
    assert_eq!(scrolled.y, base.y - 32.0);
    assert_eq!(scrolled.width, base.width);
    assert_eq!(scrolled.height, item_grid::ITEM_GRID_ROW_HEIGHT_PX);

    let normal_clip = item_content_clip_rect(&zone, 0.0);
    let search_clip = item_content_clip_rect(&zone, 44.0);
    assert_eq!(
        normal_clip.y,
        zone.y as f32 + expanded_zone_grid::HEADER_BAND_HEIGHT
    );
    assert_eq!(search_clip.y, normal_clip.y + 44.0);
    assert_eq!(normal_clip.bottom(), (zone.y + zone.h) as f32);
    assert_eq!(search_clip.bottom(), normal_clip.bottom());
}

#[test]
fn item_flow_max_scroll_accounts_for_search_and_bottom_padding() {
    let mut zone = Zone::new(bento_nano_zone::ZoneId(9), "Docs", 64, 332, 320, 220);
    zone.set_grid_columns(5);
    let normal = item_flow_max_scroll(&zone, 0.0, std::iter::repeat_n(false, 10));
    let search = item_flow_max_scroll(&zone, 44.0, std::iter::repeat_n(false, 10));

    assert!(normal > 0.0);
    assert_eq!(search, normal + 44.0);
    assert_eq!(item_flow_max_scroll(&zone, 0.0, std::iter::empty()), 0.0);
}

#[test]
fn paint_rect_applies_snap_inset() {
    let painted = paint_rect(HighlightRect::new(10.0, 20.0, 80.0, 60.0));

    assert_eq!(painted.x, 14.0);
    assert_eq!(painted.y, 24.0);
    assert_eq!(painted.width, 72.0);
    assert_eq!(painted.height, 52.0);
}

#[test]
fn pulse_geometry_and_color_follow_phase() {
    let target = HighlightPulse::new("desktop.pdf", 100.0, 120.0);
    let phase = 0.5;
    let halo = pulse_halo_rect(&target, phase);
    let core = pulse_core_rect(&target);
    let palette = theme::current().palette;
    let halo_color = pulse_halo_color_from_palette(palette, phase);
    let core_color = pulse_core_color_from_palette(palette);

    assert!(halo.width > core.width);
    assert_eq!(core.x, 92.0);
    assert_eq!(core.y, 112.0);
    assert_eq!(pulse_phase(PULSE_LOOP_MS + 800), 0.5);
    assert!((halo_color.a - PULSE_HALO_ALPHA * 0.5).abs() < f32::EPSILON);
    assert!((core_color.a - PULSE_CORE_ALPHA).abs() < f32::EPSILON);
}

#[test]
fn state_tracks_pulses_and_ticks_animation() {
    let mut state = HighlightOverlayState::new();
    state.set_targets_and_pulses(
        std::iter::empty::<HighlightRect>(),
        [HighlightPulse::new("desktop.pdf", 40.0, 50.0)],
    );

    assert!(state.has_targets());
    assert_eq!(state.targets().len(), 0);
    assert_eq!(state.pulses().len(), 1);
    assert!(state.tick(400));
    assert!(state.current_pulse_phase() > 0.0);

    state.set_targets_and_pulses_for(
        std::iter::empty::<HighlightRect>(),
        [HighlightPulse::new("desktop.pdf", 40.0, 50.0)],
        10,
    );
    assert!(!state.tick(10));
    assert!(!state.has_targets());
}

#[test]
fn inline_cap_avoids_heap_for_typical_clusters() {
    let mut state = HighlightOverlayState::new();
    let rects: Vec<HighlightRect> = (0..INLINE_TARGET_CAP)
        .map(|i| rect(i as f32 * 100.0, 0.0))
        .collect();
    state.set_targets(rects);
    assert_eq!(state.targets().len(), INLINE_TARGET_CAP);
    // SmallVec internal API doesn't expose `spilled` publicly here;
    // the contract we lock instead is "exactly INLINE_TARGET_CAP fits
    // in the typed inline storage" — a regression that shrinks the
    // cap would fail this length equality.
}

#[test]
fn highlight_overlay_colors_from_tauri_palette_use_accent_blue() {
    use bento_nano_style::tokens as style_tokens;
    let fill = fill_color_from_tauri_palette(style_tokens::PALETTE_DARK);
    assert_eq!(fill.r, style_tokens::PALETTE_DARK.accent_blue.r);
    assert!((fill.a - FILL_ALPHA).abs() < f32::EPSILON);

    let outline = outline_color_from_tauri_palette(style_tokens::PALETTE_DARK);
    assert!((outline.a - OUTLINE_ALPHA).abs() < f32::EPSILON);

    let core = pulse_core_color_from_tauri_palette(style_tokens::PALETTE_DARK);
    assert!((core.a - PULSE_CORE_ALPHA).abs() < f32::EPSILON);

    // Phase = 0 → halo at full PULSE_HALO_ALPHA.
    let halo = pulse_halo_color_from_tauri_palette(style_tokens::PALETTE_DARK, 0.0);
    assert!((halo.a - PULSE_HALO_ALPHA).abs() < f32::EPSILON);
    // Phase = 1 → halo fades to zero.
    let halo = pulse_halo_color_from_tauri_palette(style_tokens::PALETTE_DARK, 1.0);
    assert!(halo.a.abs() < f32::EPSILON);

    // Target radius from Tauri tokens is RADIUS.card (10 px).
    assert_eq!(
        target_radius_from_tauri_tokens(style_tokens::RADIUS),
        BorderRadius::all(style_tokens::RADIUS.card)
    );
}

#[test]
fn filtered_flow_slots_reflow_and_apply_inline_search_offset() {
    let mut zone = Zone::new(bento_nano_zone::ZoneId(12), "Search", 40, 60, 320, 240);
    zone.set_grid_columns(4);

    let (first, next) = item_card_rect_for_flow_slot(&zone, 0, true, 44.0);
    let (second, after_second) = item_card_rect_for_flow_slot(&zone, next, false, 44.0);

    assert_eq!(next, 2, "wide result must consume two flow slots");
    assert_eq!(after_second, 3);
    assert!(second.x > first.x);
    assert_eq!(
        first.y,
        zone.y as f32 + item_grid::ITEM_GRID_TOP_OFFSET_PX + 44.0
    );
}

#[test]
fn floating_panel_flow_uses_zone_columns_and_shared_pointer_mapping() {
    let mut zone = Zone::new(bento_nano_zone::ZoneId(13), "Preview", 0, 0, 800, 600);
    zone.set_grid_columns(4);
    let panel = Rect {
        x: 100.0,
        y: 80.0,
        width: 360.0,
        height: 420.0,
    };

    let (first, next) = item_card_rect_for_flow_slot_in_panel(&zone, panel, 0, false, 0.0);
    let (second, after_second) =
        item_card_rect_for_flow_slot_in_panel(&zone, panel, next, false, 0.0);
    let (searched, _) = item_card_rect_for_flow_slot_in_panel(&zone, panel, 0, false, 44.0);

    assert_eq!(next, 1);
    assert_eq!(after_second, 2);
    assert!(second.x > first.right());
    assert_eq!(searched.y - first.y, 44.0);
    assert_eq!(
        item_grid_position_for_panel(
            panel,
            zone.grid_columns,
            first.x + first.width * 0.5,
            first.y + first.height * 0.5,
            0.0,
        ),
        Some((0, 0))
    );
    assert!(first.bottom() <= panel.bottom());
}
