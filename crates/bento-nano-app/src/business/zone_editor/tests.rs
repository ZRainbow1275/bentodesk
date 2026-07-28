use super::*;

fn sample_zone() -> BentoZone {
    use bento_nano_backend::layout::persistence::{RelativePosition, RelativeSize};
    BentoZone {
        id: SmolStr::new_static("zone-42"),
        name: "Inbox".to_string(),
        icon: SmolStr::new_static("folder"),
        position: RelativePosition {
            x_percent: 0.0,
            y_percent: 0.0,
        },
        expanded_size: RelativeSize {
            w_percent: 30.0,
            h_percent: 40.0,
        },
        items: Vec::new(),
        accent_color: Some(SmolStr::new_static("#3b82f6")),
        sort_order: 0,
        auto_group: None,
        grid_columns: 4,
        created_at: SmolStr::new_static("2026-05-03T00:00:00Z"),
        updated_at: SmolStr::new_static("2026-05-03T00:00:00Z"),
        capsule_size: SmolStr::new_static("medium"),
        capsule_shape: SmolStr::new_static("pill"),
        locked: false,
        visible: true,
        stack_id: None,
        stack_order: 0,
        alias: None,
        display_mode: None,
        live_folder_path: None,
    }
}

fn seeded_state() -> ZoneEditorState {
    let mut s = ZoneEditorState::new();
    let zone = sample_zone();
    s.load_zone(&zone);
    s
}

#[test]
fn editor_default_chrome_uses_palette_surface() {
    let e = ZoneEditor::new();
    let palette = theme::current().palette;
    assert_eq!(e.background, palette.surface);
    assert_eq!(e.border, palette.border);
    assert_eq!(e.title_color, palette.text);
    assert_eq!(e.border_radius.top_left, PANEL_CORNER_RADIUS);
    assert_eq!(e.width, Length::Px(PANEL_WIDTH));
}

#[test]
fn snap_constants_match_spec() {
    assert_eq!(PANEL_WIDTH, 400.0);
    assert!((PANEL_MAX_HEIGHT_FRACTION - 0.80).abs() < f32::EPSILON);
    assert_eq!(HEADER_HEIGHT, 52.0);
    assert_eq!(FOOTER_HEIGHT, 64.0);
    assert_eq!(BODY_PADDING_VERTICAL, 16.0);
    assert_eq!(BODY_PADDING_HORIZONTAL, 20.0);
    assert_eq!(FIELD_GAP, 16.0);
    assert_eq!(PANEL_OPEN_DURATION_MS, 200);
    assert_eq!(PANEL_CORNER_RADIUS, 16.0);
    assert_eq!(ICON_CELL_SIZE, 36.0);
    assert_eq!(SWATCH_SIZE, 28.0);
    assert_eq!(ICON_GRID_COLUMNS, 6);
    assert_eq!(NAME_MAX_LEN, 32);
    assert_eq!(GRID_COLUMNS_MIN, 2);
    assert_eq!(GRID_COLUMNS_MAX, 6);
    assert_eq!(ACCENT_PALETTE.len(), 10);
}

#[test]
fn capsule_shape_wire_round_trip_and_unknown_falls_back() {
    for v in CapsuleShapeChoice::ALL {
        assert_eq!(CapsuleShapeChoice::parse(v.wire()), *v);
    }
    assert_eq!(
        CapsuleShapeChoice::parse("hexagon"),
        CapsuleShapeChoice::default()
    );
}

#[test]
fn capsule_size_wire_round_trip_and_unknown_falls_back() {
    for v in CapsuleSizeChoice::ALL {
        assert_eq!(CapsuleSizeChoice::parse(v.wire()), *v);
    }
    assert_eq!(
        CapsuleSizeChoice::parse("colossal"),
        CapsuleSizeChoice::default()
    );
}

#[test]
fn capsule_shape_serde_round_trip() {
    for v in [
        CapsuleShapeChoice::Pill,
        CapsuleShapeChoice::Rounded,
        CapsuleShapeChoice::Circle,
        CapsuleShapeChoice::Minimal,
        CapsuleShapeChoice::Square,
    ] {
        let s = serde_json::to_string(&v).unwrap_or_default();
        let back: CapsuleShapeChoice = serde_json::from_str(&s).unwrap_or_default();
        assert_eq!(v, back);
    }
    assert_eq!(
        serde_json::to_string(&CapsuleShapeChoice::Pill).unwrap_or_default(),
        "\"pill\""
    );
}

#[test]
fn fresh_state_is_not_dirty() {
    let s = ZoneEditorState::new();
    assert!(!s.is_dirty());
    assert!(!s.can_save());
    assert_eq!(s.zone_id(), "");
}

#[test]
fn load_zone_seeds_fields_and_clears_dirty() {
    let s = seeded_state();
    assert_eq!(s.zone_id(), "zone-42");
    assert_eq!(s.name(), "Inbox");
    assert_eq!(s.icon(), "folder");
    assert_eq!(s.accent_color(), Some("#3b82f6"));
    assert_eq!(s.grid_columns(), 4);
    assert_eq!(s.capsule_shape(), CapsuleShapeChoice::Pill);
    assert_eq!(s.capsule_size(), CapsuleSizeChoice::Medium);
    assert!(!s.is_dirty());
}

#[test]
fn set_name_marks_dirty_and_truncates_to_32_chars() {
    let mut s = seeded_state();
    let long = "a".repeat(64);
    s.set_name(long);
    assert_eq!(s.name().chars().count(), NAME_MAX_LEN);
    assert!(s.is_dirty());
}

#[test]
fn set_name_truncates_at_codepoint_boundary_for_emoji() {
    let mut s = seeded_state();
    // 40 codepoints, all multi-byte → byte length > 32 but char count = 40.
    let emoji_name = "🦀".repeat(40);
    s.set_name(emoji_name);
    assert_eq!(s.name().chars().count(), NAME_MAX_LEN);
}

#[test]
fn set_grid_columns_clamps_to_range() {
    let mut s = seeded_state();
    s.set_grid_columns(99);
    assert_eq!(s.grid_columns(), GRID_COLUMNS_MAX);
    s.set_grid_columns(0);
    assert_eq!(s.grid_columns(), GRID_COLUMNS_MIN);
}

#[test]
fn can_save_requires_dirty_and_non_blank_name() {
    let mut s = seeded_state();
    assert!(!s.can_save(), "fresh load is not dirty");

    s.set_grid_columns(5);
    assert!(s.can_save(), "dirty + non-blank name → can save");

    s.set_name("   ");
    assert!(!s.can_save(), "blank name disables save");
}

#[test]
fn click_save_with_clean_state_is_noop() {
    let mut s = seeded_state();
    s.click_save();
    assert!(s.take_action().is_none());
}

#[test]
fn click_save_records_payload_with_only_touched_fields() {
    let mut s = seeded_state();
    s.set_icon("star");
    s.set_capsule_size(CapsuleSizeChoice::Large);
    s.click_save();

    let action = s.take_action().expect("save action queued");
    let ZoneEditorAction::Save { zone_id, update } = action else {
        panic!("expected Save");
    };
    assert_eq!(zone_id, SmolStr::new_static("zone-42"));
    // Touched: icon + capsule_size.
    assert_eq!(update.icon, Some(SmolStr::new_static("star")));
    assert_eq!(update.capsule_size, Some(SmolStr::new_static("large")));
    // Untouched: everything else.
    assert!(update.name.is_none());
    assert!(update.accent_color.is_none());
    assert!(update.grid_columns.is_none());
    assert!(update.capsule_shape.is_none());
    assert!(update.alias.is_none());
    assert!(update.display_mode.is_none());
}

#[test]
fn click_save_emits_trimmed_name_when_dirty() {
    let mut s = seeded_state();
    s.set_name("  Renamed  ");
    s.click_save();
    let action = s.take_action().expect("save action");
    let ZoneEditorAction::Save { update, .. } = action else {
        panic!("expected Save");
    };
    assert_eq!(update.name.as_deref(), Some("Renamed"));
}

#[test]
fn click_save_with_no_accent_emits_none_clear() {
    let mut s = seeded_state();
    s.set_accent_color(None);
    s.click_save();
    let action = s.take_action().expect("save action");
    let ZoneEditorAction::Save { update, .. } = action else {
        panic!("expected Save");
    };
    // Accent dirty → field present in update; user picked None.
    assert!(update.accent_color.is_none());
}

#[test]
fn click_cancel_records_cancel_action() {
    let mut s = seeded_state();
    s.set_icon("ignored");
    s.click_cancel();
    assert!(matches!(s.take_action(), Some(ZoneEditorAction::Cancel)));
}

#[test]
fn take_action_is_one_shot() {
    let mut s = seeded_state();
    s.click_cancel();
    assert!(s.take_action().is_some());
    assert!(s.take_action().is_none());
}

#[test]
fn build_returns_panel_sized_container() {
    let node = build();
    let layout = node.layout();
    assert!(matches!(layout.width, Length::Px(w) if (w - PANEL_WIDTH).abs() < 0.01));
    assert_eq!(layout.direction, Direction::Column);
}

#[test]
fn panel_max_height_resolves_to_80_percent_of_viewport() {
    let viewport_height = 1000.0_f32;
    let resolved = viewport_height * PANEL_MAX_HEIGHT_FRACTION;
    assert!((resolved - 800.0).abs() < 0.01);
}
