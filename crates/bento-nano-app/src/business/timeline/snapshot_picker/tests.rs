use super::*;
use bento_nano_backend::layout::Resolution;

fn make_snapshot(zones: usize) -> DesktopSnapshot {
    DesktopSnapshot {
        id: SmolStr::new_static("s"),
        name: "snap-1".to_string(),
        resolution: Resolution {
            width: 1920,
            height: 1080,
        },
        dpi: 1.0,
        zones: (0..zones)
            .map(|i| bento_nano_backend::layout::BentoZone {
                id: SmolStr::from(format!("z{i}")),
                name: format!("Z{i}"),
                icon: SmolStr::new_static("folder"),
                position: bento_nano_backend::layout::RelativePosition {
                    x_percent: 0.0,
                    y_percent: 0.0,
                },
                expanded_size: bento_nano_backend::layout::RelativeSize {
                    w_percent: 30.0,
                    h_percent: 30.0,
                },
                items: Vec::new(),
                accent_color: None,
                sort_order: 0,
                auto_group: None,
                grid_columns: 4,
                created_at: SmolStr::new_static(""),
                updated_at: SmolStr::new_static(""),
                capsule_size: SmolStr::new_static("medium"),
                capsule_shape: SmolStr::new_static("pill"),
                locked: false,
                visible: true,
                stack_id: None,
                stack_order: 0,
                alias: None,
                display_mode: None,
                live_folder_path: None,
            })
            .collect(),
        captured_at: SmolStr::new_static("2026-01-01T00:00:00Z"),
    }
}

#[test]
fn build_returns_zero_padded_outer_container() {
    use bento_nano_layout::LayoutSource;
    let node = build();
    let layout = node.layout();
    // Outer chrome stays at zero so the inner header/body padding wins.
    assert!(layout.padding.left.abs() < 0.01);
    assert!(layout.padding.top.abs() < 0.01);
}

#[test]
fn snapshot_picker_chrome_accepts_explicit_active_palette() {
    let mut palette = bento_nano_theme::current().palette;
    palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
    palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
    palette.selection = Color::from_u8(0x44, 0xAA, 0xEE, 0x66);
    palette.active_overlay = Color::from_u8(0x33, 0x44, 0x55, 0x99);
    palette.border = Color::from_u8(0x66, 0x77, 0x88, 0xAA);
    palette.hover_overlay = Color::from_u8(0x10, 0x20, 0x30, 0x40);
    palette.accent = Color::from_u8(0x12, 0x34, 0x56, 0x78);
    palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
    palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);
    palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);

    let chrome = SnapshotPickerChrome::from_palette(palette);

    assert_eq!(
        chrome.panel_background,
        Color::from_u8(0x22, 0x33, 0x44, 0xDD)
    );
    assert_eq!(
        chrome.row_background,
        Color::from_u8(0x11, 0x22, 0x33, 0xEE)
    );
    assert_eq!(
        chrome.selected_background,
        Color::from_u8(0x44, 0xAA, 0xEE, 0x66)
    );
    assert_eq!(
        chrome.action_background,
        Color::from_u8(0x33, 0x44, 0x55, 0x99)
    );
    assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
    assert_eq!(chrome.error_color, Color::from_u8(0xCC, 0x44, 0x44, 0xFF));
    assert_eq!(
        chrome.thumbnail_chrome.border_color,
        Color::from_u8(0x66, 0x77, 0x88, 0xAA)
    );
    assert_eq!(
        chrome.thumbnail_chrome.background_color,
        Color::from_u8(0x10, 0x20, 0x30, 0x40)
    );
    assert_eq!(
        chrome.thumbnail_chrome.fallback_zone_color,
        Color::from_u8(0x12, 0x34, 0x56, 0x78)
    );
    assert_eq!(
        chrome.thumbnail_chrome.empty_text_color,
        Color::from_u8(0x88, 0x99, 0xAA, 0xFF)
    );
}

#[test]
fn snapshot_picker_chrome_accepts_explicit_radius_shadow_tokens() {
    let palette = bento_nano_theme::current().palette;
    let radius = bento_nano_theme::RadiusTokens {
        sm: BorderRadius::all(3.0),
        md: BorderRadius::all(7.0),
        lg: BorderRadius::all(11.0),
        xl: BorderRadius::all(17.0),
        full: BorderRadius::all(999.0),
    };
    let mut shadow = bento_nano_theme::shadow::DEFAULT;
    shadow.md = Shadow {
        offset_x: 2.0,
        offset_y: 5.0,
        blur: 13.0,
        spread: 0.0,
        color: Color::from_u8(0x10, 0x20, 0x30, 0x99),
    };

    let chrome = SnapshotPickerChrome::from_tokens(palette, radius, shadow);

    assert_eq!(chrome.panel_shadow, shadow.md);
    assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
    assert_eq!(chrome.button_radius, BorderRadius::all(7.0));
    assert_eq!(chrome.row_radius, BorderRadius::all(11.0));
    assert_eq!(
        chrome.thumbnail_chrome.border_radius,
        BorderRadius::all(7.0)
    );
    assert_eq!(
        chrome.thumbnail_chrome.content_radius,
        BorderRadius::all(7.0)
    );
    assert_eq!(chrome.thumbnail_chrome.zone_radius, BorderRadius::all(3.0));
}

#[test]
fn snapshot_picker_panel_shadow_rect_uses_token_shadow_geometry() {
    let panel = Rect {
        x: 24.0,
        y: 30.0,
        width: 320.0,
        height: 180.0,
    };
    let shadow = Shadow {
        offset_x: 3.0,
        offset_y: 5.0,
        blur: 11.0,
        spread: 0.0,
        color: Color::from_u8(0x10, 0x20, 0x30, 0x40),
    };

    let rect = snapshot_picker_panel_shadow_rect(panel, shadow);

    assert_eq!(
        rect,
        Rect {
            x: 16.0,
            y: 24.0,
            width: 342.0,
            height: 202.0,
        }
    );
}

#[test]
fn snap_constants_match_baseline_css() {
    assert_eq!(PANEL_WIDTH, 440.0);
    assert!((PANEL_MAX_HEIGHT_FRACTION - 0.70).abs() < f32::EPSILON);
    assert_eq!(HEADER_HEIGHT, 52.0);
    assert_eq!(HEADER_PADDING_X, 20.0);
    assert_eq!(BODY_PADDING_X, 20.0);
    assert_eq!(BODY_PADDING_Y, 16.0);
    assert_eq!(PANEL_OPEN_DURATION_MS, 200);
    assert_eq!(RUNTIME_PANEL_INSET_PX, 18.0);
    assert_eq!(RUNTIME_CLOSE_BUTTON_SIZE_PX, 32.0);
    assert_eq!(RUNTIME_ACTION_BUTTON_HEIGHT_PX, 28.0);
    assert_eq!(RUNTIME_ACTION_BUTTON_TOP_PX, 108.0);
    assert_eq!(RUNTIME_ROW_TOP_PX, 148.0);
    assert_eq!(RUNTIME_ROW_HEIGHT_PX, 44.0);
    assert_eq!(RUNTIME_ROW_STRIDE_PX, 52.0);
    assert_eq!(RUNTIME_VISIBLE_ROW_LIMIT, 8);
}

#[test]
fn runtime_hit_test_maps_buttons_and_visible_rows() {
    let viewport = Size {
        width: 640.0,
        height: 520.0,
    };
    for spec in SNAPSHOT_PICKER_ACTION_BUTTONS {
        let rect = snapshot_picker_button_rect(viewport, *spec);
        assert_eq!(
            snapshot_picker_hit_test(viewport, 3, rect.x + 1.0, rect.y + 1.0),
            Some(spec.hit)
        );
    }

    let close = snapshot_picker_close_rect(viewport);
    assert_eq!(
        snapshot_picker_hit_test(viewport, 3, close.x + 1.0, close.y + 1.0),
        Some(SnapshotPickerPointerHit::Close)
    );

    let second_row = snapshot_picker_row_rect(viewport, 1);
    assert_eq!(
        snapshot_picker_hit_test(viewport, 3, second_row.x + 1.0, second_row.y + 1.0),
        Some(SnapshotPickerPointerHit::Row(1))
    );
    assert_eq!(
        snapshot_picker_hit_test(viewport, 1, second_row.x + 1.0, second_row.y + 1.0),
        None
    );
}

#[test]
fn row_action_default_is_not_awaiting_anything() {
    assert!(!RowAction::Default.is_awaiting_for("anything"));
}

#[test]
fn row_action_awaiting_matches_only_its_owner() {
    let action = RowAction::begin_confirm("snap-7");
    assert!(action.is_awaiting_for("snap-7"));
    assert!(!action.is_awaiting_for("snap-8"));
}

#[test]
fn meta_line_uses_bullet_separator_and_resolution() {
    let snap = make_snapshot(3);
    let line = meta_line(&snap, "2026-01-01 00:00", "Zones");
    // U+2022 BULLET — matches the 1.x display character.
    assert_eq!(
        line.as_str(),
        "3 Zones \u{2022} 1920x1080 \u{2022} 2026-01-01 00:00"
    );
}

#[test]
fn meta_line_handles_zero_zones() {
    let snap = make_snapshot(0);
    let line = meta_line(&snap, "now", "Zones");
    assert!(line.starts_with("0 Zones"));
}

#[test]
fn snapshot_picker_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
    use bento_nano_style::tokens as style_tokens;
    let chrome = SnapshotPickerChrome::from_tauri_tokens(
        style_tokens::PALETTE_DARK,
        style_tokens::RADIUS,
        style_tokens::SHADOW,
    );
    assert_eq!(
        chrome.panel_background,
        style_tokens::PALETTE_DARK.surface_expanded
    );
    assert_eq!(
        chrome.row_background,
        style_tokens::PALETTE_DARK.surface_subtle
    );
    assert_eq!(
        chrome.selected_background,
        style_tokens::PALETTE_DARK.surface_active
    );
    assert_eq!(
        chrome.action_background,
        style_tokens::PALETTE_DARK.accent_blue
    );
    assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
    assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
    assert_eq!(chrome.error_color, style_tokens::PALETTE_DARK.accent_red);
    assert_eq!(
        chrome.panel_radius,
        BorderRadius::all(style_tokens::RADIUS.expanded)
    );
    assert_eq!(
        chrome.button_radius,
        BorderRadius::all(style_tokens::RADIUS.card)
    );
    assert_eq!(
        chrome.row_radius,
        BorderRadius::all(style_tokens::RADIUS.card)
    );
    // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
    assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
    // Thumbnail chrome is composed from the same Tauri palette.
    assert_eq!(
        chrome.thumbnail_chrome.fallback_zone_color,
        style_tokens::PALETTE_DARK.accent_blue
    );
    assert_eq!(
        chrome.thumbnail_chrome.border_color,
        style_tokens::PALETTE_DARK.border_expanded
    );
}
