#[test]
fn stack_bloom_timing_fits_fast_release_envelope() {
    assert_eq!(BLOOM_PETAL_ENTER_DURATION_MS, 300);
    assert_eq!(BLOOM_ENTRY_STAGGER_BUDGET_MS, 180);
    assert_eq!(BLOOM_PETAL_EXIT_DURATION_MS, 100);
    assert_eq!(BLOOM_EXIT_STAGGER_BUDGET_MS, 30);
    assert_eq!(BLOOM_EXIT_VISIBLE_DURATION_MS, 140);
    assert_eq!(BLOOM_PREVIEW_HOVER_INTENT_MS, 150);
    assert_eq!(BLOOM_LEAVE_GRACE_MS, 80);
    assert_eq!(stack_bloom_reveal_duration_ms(0), 300);
    assert_eq!(stack_bloom_reveal_duration_ms(1), 300);
    assert_eq!(stack_bloom_reveal_duration_ms(2), 390);
    assert_eq!(stack_bloom_exit_duration_ms(0), 100);
    assert_eq!(stack_bloom_exit_duration_ms(1), 100);
    assert_eq!(stack_bloom_exit_duration_ms(2), 115);
    assert_eq!(stack_bloom_exit_duration_ms(BLOOM_VISIBLE_PETAL_LIMIT), 128);
    assert_eq!(
        stack_bloom_reveal_duration_ms(BLOOM_VISIBLE_PETAL_LIMIT),
        472
    );
    assert_eq!(BLOOM_REVEAL_DURATION_MS, 472);
    const { assert!(BLOOM_REVEAL_DURATION_MS <= 480) };
    assert!(stack_bloom_exit_duration_ms(BLOOM_VISIBLE_PETAL_LIMIT) <= 140);
}

#[test]
fn stack_tray_state_distinguishes_management_from_bloom_preview() {
    let management = StackTrayState::new(ZoneId(1), ZoneId(2));
    let preview = StackTrayState::bloom_preview(ZoneId(1), ZoneId(2));

    assert!(management.is_management());
    assert!(!management.is_bloom_preview());
    assert!(preview.is_bloom_preview());
    assert!(!preview.is_management());
}

#[test]
fn focused_bloom_preview_stays_next_to_petal_and_inside_viewport() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(2), Cow::Borrowed("Preview"), 0, 0, 320, 360);
    let left_petal = Rect {
        x: 120.0,
        y: 180.0,
        width: 108.0,
        height: 96.0,
    };
    let right_petal = Rect {
        x: 1120.0,
        ..left_petal
    };

    let right_growing = focused_bloom_preview_rect(viewport, left_petal, &[left_petal], &zone);
    let left_growing = focused_bloom_preview_rect(viewport, right_petal, &[right_petal], &zone);

    assert_close(
        right_growing.x,
        left_petal.right() + FLOATING_PREVIEW_GAP_PX,
    );
    assert_close(
        left_growing.right(),
        right_petal.x - FLOATING_PREVIEW_GAP_PX,
    );
    for preview in [right_growing, left_growing] {
        assert!(preview.x >= FLOATING_PREVIEW_VIEWPORT_MARGIN_PX);
        assert!(preview.right() <= viewport.width - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX + 0.01);
        assert!(preview.y >= FLOATING_PREVIEW_VIEWPORT_MARGIN_PX);
        assert!(preview.bottom() <= viewport.height - FLOATING_PREVIEW_VIEWPORT_MARGIN_PX + 0.01);
    }
}

#[test]
fn focused_bloom_preview_avoids_the_complete_sibling_row() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(2), Cow::Borrowed("Preview"), 0, 0, 320, 360);
    let petals = [
        Rect {
            x: 120.0,
            y: 180.0,
            width: 108.0,
            height: 96.0,
        },
        Rect {
            x: 240.0,
            y: 180.0,
            width: 108.0,
            height: 96.0,
        },
        Rect {
            x: 360.0,
            y: 180.0,
            width: 108.0,
            height: 96.0,
        },
        Rect {
            x: 480.0,
            y: 180.0,
            width: 108.0,
            height: 96.0,
        },
    ];

    let preview = focused_bloom_preview_rect(viewport, petals[1], &petals, &zone);

    assert_close(preview.x, petals[3].right() + FLOATING_PREVIEW_GAP_PX);
    assert!(petals.iter().all(|petal| petal.right() <= preview.x));
    assert_close(preview.y, petals[1].y);
}

#[test]
fn focused_bloom_preview_hit_and_header_actions_share_painted_geometry() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(2), Cow::Borrowed("Preview"), 0, 0, 320, 360);
    let petal = Rect {
        x: 120.0,
        y: 180.0,
        width: 108.0,
        height: 96.0,
    };
    let petals = [petal];
    let preview = focused_bloom_preview_rect(viewport, petal, &petals, &zone);
    let search = focused_bloom_preview_search_rect(preview);
    let close = focused_bloom_preview_close_rect(preview);

    assert!(focused_bloom_preview_contains(
        viewport,
        petal,
        &petals,
        &zone,
        preview.x + 1.0,
        preview.y + 1.0
    ));
    assert!(!focused_bloom_preview_contains(
        viewport,
        petal,
        &petals,
        &zone,
        preview.x - 1.0,
        preview.y
    ));
    for button in [search, close] {
        assert!(button.x >= preview.x);
        assert!(button.right() <= preview.right());
        assert!(button.y >= preview.y);
        assert!(button.bottom() <= preview.bottom());
    }
    assert!(search.right() <= close.x);
}

#[test]
fn stack_tray_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
    // Wave D acceptance: tray panel chrome must map to
    // `bento_nano_style::tokens::PALETTE_DARK.surface_expanded` and the
    // Tauri `--shadow-expanded` outer geometry, not the legacy
    // `bento-nano-theme` palette.
    let chrome = StackTrayChrome::from_tauri_tokens(
        style_tokens::PALETTE_DARK,
        style_tokens::RADIUS,
        style_tokens::SHADOW,
    );
    assert_eq!(
        chrome.panel_background,
        style_tokens::PALETTE_DARK.surface_expanded
    );
    assert_eq!(
        chrome.preview_background,
        style_tokens::PALETTE_DARK.surface_expanded
    );
    assert_eq!(
        chrome.row_background,
        style_tokens::PALETTE_DARK.surface_hover
    );
    assert_eq!(
        chrome.selected_background,
        style_tokens::PALETTE_DARK.surface_active
    );
    assert_eq!(
        chrome.danger_background,
        style_tokens::PALETTE_DARK.accent_red
    );
    assert_eq!(chrome.text_primary, style_tokens::PALETTE_DARK.text_primary);
    assert_eq!(chrome.text_muted, style_tokens::PALETTE_DARK.text_muted);
    assert_eq!(chrome.text_accent, style_tokens::PALETTE_DARK.accent_blue);
    assert_eq!(
        chrome.panel_radius,
        BorderRadius::all(style_tokens::RADIUS.expanded)
    );
    assert_eq!(
        chrome.row_radius,
        BorderRadius::all(style_tokens::RADIUS.card)
    );
    assert_eq!(
        chrome.button_radius,
        BorderRadius::all(style_tokens::RADIUS.card)
    );
    // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
    assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
}

#[test]
fn stack_tray_typography_matches_tauri_compact_roles() {
    assert_eq!(TRAY_TITLE_FONT_PX, 13.0);
    assert_eq!(TRAY_TITLE_FONT_WEIGHT, 600);
    assert_eq!(TRAY_COUNT_FONT_PX, 11.0);
    assert_eq!(TRAY_TOOLBAR_FONT_PX, 11.0);
    assert_eq!(TRAY_TOOLBAR_FONT_WEIGHT, 400);
    assert_eq!(TRAY_MEMBER_NAME_FONT_PX, 13.0);
    assert_eq!(TRAY_MEMBER_NAME_FONT_WEIGHT, 600);
    assert_eq!(TRAY_MEMBER_META_FONT_PX, 11.0);
    assert_eq!(TRAY_ACTION_FONT_PX, 11.0);
    assert_eq!(TRAY_STATUS_FONT_PX, 11.0);
    const { assert!(TRAY_TEXT_LINE_HEIGHT <= 1.25) };

    assert_eq!(PREVIEW_EYEBROW_FONT_PX, 11.0);
    assert_eq!(PREVIEW_TITLE_FONT_PX, 13.0);
    assert_eq!(PREVIEW_META_FONT_PX, 11.0);
    assert_eq!(PREVIEW_ITEM_FONT_PX, 11.0);
    assert_eq!(PREVIEW_EMPTY_FONT_PX, 11.0);
    const { assert!(PREVIEW_TEXT_LINE_HEIGHT <= 1.25) };
}

fn anchor() -> Zone {
    Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 700, 120, 180, 130)
}

fn rect_center_x(rect: Rect) -> f32 {
    rect.x + rect.width / 2.0
}

#[test]
fn stack_tray_chrome_accepts_explicit_active_palette() {
    let mut palette = bento_nano_theme::current().palette;
    palette.scrim = Color::from_u8(0x00, 0x00, 0x00, 0x88);
    palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
    palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
    palette.selection = Color::from_u8(0x44, 0xAA, 0xEE, 0x66);
    palette.active_overlay = Color::from_u8(0x33, 0x44, 0x55, 0x99);
    palette.hover_overlay = Color::from_u8(0x10, 0x20, 0x30, 0x40);
    palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);
    palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
    palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);
    palette.accent = Color::from_u8(0x12, 0x34, 0x56, 0x78);

    let chrome = StackTrayChrome::from_palette(palette);

    assert_eq!(chrome.panel_shadow, shadow::DEFAULT.md);
    assert_eq!(chrome.panel_radius, radius::DEFAULT.xl);
    assert_eq!(chrome.row_radius, radius::DEFAULT.lg);
    assert_eq!(chrome.button_radius, radius::DEFAULT.md);
    assert_eq!(chrome.preview_item_radius, radius::DEFAULT.md);
    assert_eq!(
        chrome.panel_background,
        Color::from_u8(0x22, 0x33, 0x44, 0xDD)
    );
    assert_eq!(
        chrome.preview_background,
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
        chrome.dragged_background,
        Color::from_u8(0x33, 0x44, 0x55, 0x99)
    );
    assert_eq!(
        chrome.button_background,
        Color::from_u8(0x10, 0x20, 0x30, 0x40)
    );
    assert_eq!(
        chrome.danger_background,
        Color::from_u8(0xCC, 0x44, 0x44, 0xFF)
    );
    assert_eq!(chrome.text_primary, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.text_muted, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
    assert_eq!(chrome.text_accent, Color::from_u8(0x12, 0x34, 0x56, 0x78));
}

#[test]
fn stack_tray_chrome_accepts_explicit_radius_shadow_tokens() {
    let palette = bento_nano_theme::current().palette;
    let radius = RadiusTokens {
        sm: BorderRadius::all(3.0),
        md: BorderRadius::all(7.0),
        lg: BorderRadius::all(11.0),
        xl: BorderRadius::all(17.0),
        full: BorderRadius::all(999.0),
    };
    let shadow = ShadowTokens {
        sm: Shadow {
            offset_x: 1.0,
            offset_y: 2.0,
            blur: 3.0,
            spread: 0.0,
            color: Color::from_u8(0x01, 0x01, 0x01, 0x20),
        },
        md: Shadow {
            offset_x: 4.0,
            offset_y: 5.0,
            blur: 6.0,
            spread: 0.0,
            color: Color::from_u8(0x02, 0x02, 0x02, 0x40),
        },
        lg: Shadow {
            offset_x: 7.0,
            offset_y: 8.0,
            blur: 9.0,
            spread: 0.0,
            color: Color::from_u8(0x03, 0x03, 0x03, 0x60),
        },
    };

    let chrome = StackTrayChrome::from_tokens(palette, radius, shadow);

    assert_eq!(chrome.panel_shadow, shadow.md);
    assert_eq!(chrome.panel_radius, radius.xl);
    assert_eq!(chrome.row_radius, radius.lg);
    assert_eq!(chrome.button_radius, radius.md);
    assert_eq!(chrome.preview_item_radius, radius.md);
}

#[test]
fn panel_shadow_rect_uses_token_shadow_geometry() {
    let panel = Rect {
        x: 100.0,
        y: 40.0,
        width: 340.0,
        height: 168.0,
    };
    let shadow = Shadow {
        offset_x: 3.0,
        offset_y: 7.0,
        blur: 11.0,
        spread: 0.0,
        color: Color::BLACK,
    };

    assert_eq!(
        panel_shadow_rect(panel, shadow),
        Rect {
            x: 92.0,
            y: 36.0,
            width: 362.0,
            height: 190.0,
        }
    );
}

#[test]
fn stack_tray_flips_left_near_viewport_edge() {
    let viewport = Size {
        width: 900.0,
        height: 600.0,
    };

    let rect = stack_tray_rect(viewport, &anchor(), 3);

    assert!(rect.right() <= viewport.width - TRAY_VIEWPORT_MARGIN_PX);
    assert!(rect.x < 700.0);
}

#[test]
fn stack_tray_header_count_clears_action_buttons() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 100, 100, 180, 130);

    let title = stack_tray_header_title_rect(viewport, &zone, 3);
    let count = stack_tray_header_count_rect(viewport, &zone, 3);
    let many_count = stack_tray_header_count_rect(viewport, &zone, 1000);
    let dissolve = stack_tray_dissolve_rect(viewport, &zone, 3);
    let close = stack_tray_close_rect(viewport, &zone, 3);

    assert!(title.width > 0.0);
    assert!(count.width > 0.0);
    assert_eq!(stack_tray_header_count_label_len(3), 1);
    assert_eq!(stack_tray_header_count_label_len(10), 2);
    assert_eq!(stack_tray_header_count_label_len(999), 3);
    assert_eq!(stack_tray_header_count_label_len(1000), 4);
    assert!(
        count.width >= stack_tray_header_count_badge_width(3) - 0.01,
        "3-member badge keeps the full numeric label"
    );
    assert!(
        many_count.width >= stack_tray_header_count_badge_width(1000) - 0.01,
        "1000+ member badge keeps the capped 999+ label"
    );
    assert!(
        count.x >= title.right() + TRAY_GAP_PX - 0.01,
        "count starts after title"
    );
    assert!(
        count.right() <= dissolve.x - TRAY_GAP_PX + 0.01,
        "count must not overlap Dissolve"
    );
    assert!(
        many_count.right() <= dissolve.x - TRAY_GAP_PX + 0.01,
        "wide count badge must not overlap Dissolve"
    );
    assert!(dissolve.right() <= close.x - TRAY_GAP_PX + 0.01);
}

#[test]
fn stack_tray_hit_test_prefers_detach_over_row() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 100, 100, 180, 130);
    let detach = stack_tray_detach_rect(viewport, &zone, 4, 1);

    let hit = stack_tray_hit_test(viewport, &zone, 4, detach.x + 2.0, detach.y + 2.0);

    assert_eq!(hit, Some(StackTrayPointerHit::Detach(1)));
}

#[test]
fn stack_tray_member_meta_rects_reserve_detach_button_space() {
    let viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    let zone = Zone::new(ZoneId(1), Cow::Borrowed("Anchor"), 100, 100, 180, 130);
    let row = stack_tray_row_rect(viewport, &zone, 4, 0);
    let detach = stack_tray_detach_rect(viewport, &zone, 4, 0);

    let count = stack_tray_member_meta_count_rect(row);
    let suffix = stack_tray_member_meta_suffix_rect(row);

    assert_eq!(count.width, TRAY_MEMBER_META_COUNT_WIDTH_PX);
    assert!(count.x >= row.x + TRAY_MEMBER_TEXT_X_PX - 0.01);
    assert!(suffix.x >= count.right() + TRAY_MEMBER_META_GAP_PX - 0.01);
    assert!(suffix.right() <= detach.x - TRAY_MEMBER_META_GAP_PX + 0.01);
}

#[test]
fn stack_tray_status_segments_stay_inside_status_row() {
    let tray = Rect {
        x: 120.0,
        y: 80.0,
        width: TRAY_WIDTH_PX,
        height: TRAY_MIN_HEIGHT_PX,
    };

    let status = stack_tray_status_rect(tray);
    let prefix = stack_tray_status_prefix_rect(status);
    let count = stack_tray_status_count_rect(status);
    let suffix = stack_tray_status_suffix_rect(status);

    assert_eq!(status.height, TRAY_STATUS_HEIGHT_PX);
    assert!(prefix.x >= status.x);
    assert!(count.x >= prefix.right() - 0.01);
    assert!(suffix.x >= count.right() + TRAY_STATUS_GAP_PX - 0.01);
    assert!(suffix.right() <= status.right() + 0.01);
}

#[test]
fn focused_preview_meta_segments_leave_suffix_space() {
    let preview = Rect {
        x: 360.0,
        y: 120.0,
        width: PREVIEW_WIDTH_PX,
        height: PREVIEW_HEIGHT_PX,
    };

    let width_number = focused_preview_meta_number_rect(preview, 0);
    let width_mark = focused_preview_meta_mark_rect(preview, 0);
    let height_number = focused_preview_meta_number_rect(preview, 1);
    let height_mark = focused_preview_meta_mark_rect(preview, 1);
    let item_number = focused_preview_meta_number_rect(preview, 2);
    let suffix = focused_preview_meta_suffix_rect(preview);

    assert!(width_number.x >= preview.x + 16.0 - 0.01);
    assert!(width_mark.x >= width_number.right() - 0.01);
    assert!(height_number.x >= width_mark.right() + PREVIEW_META_GAP_PX - 0.01);
    assert!(height_mark.x >= height_number.right() - 0.01);
    assert!(item_number.x >= height_mark.right() + PREVIEW_META_GAP_PX - 0.01);
    assert!(suffix.x >= item_number.right() + PREVIEW_META_GAP_PX - 0.01);
    assert!(suffix.right() <= preview.right() - 16.0 + 0.01);
}

#[test]
fn focused_preview_stays_inside_viewport() {
    let viewport = Size {
        width: 720.0,
        height: 360.0,
    };
    let tray = Rect {
        x: 380.0,
        y: 260.0,
        width: TRAY_WIDTH_PX,
        height: 160.0,
    };

    let preview = focused_preview_rect(viewport, tray);

    assert!(preview.x >= TRAY_VIEWPORT_MARGIN_PX);
    assert!(preview.y >= TRAY_VIEWPORT_MARGIN_PX);
    assert!(preview.right() <= viewport.width - TRAY_VIEWPORT_MARGIN_PX);
    assert!(preview.bottom() <= viewport.height - TRAY_VIEWPORT_MARGIN_PX);
}

#[test]
fn focused_preview_uses_left_side_when_left_has_more_room() {
    let viewport = Size {
        width: 1707.0,
        height: 912.0,
    };
    let tray = Rect {
        x: 756.0,
        y: 332.0,
        width: TRAY_WIDTH_PX,
        height: TRAY_MIN_HEIGHT_PX,
    };

    let preview = focused_preview_rect(viewport, tray);

    assert!(preview.right() <= tray.x - PREVIEW_GAP_PX + 0.01);
    assert!(preview.x >= TRAY_VIEWPORT_MARGIN_PX);
    assert!(preview.right() <= viewport.width - TRAY_VIEWPORT_MARGIN_PX);
}

#[test]
fn focused_preview_keeps_right_side_when_right_has_more_room() {
    let viewport = Size {
        width: 2560.0,
        height: 1440.0,
    };
    let tray = Rect {
        x: 756.0,
        y: 332.0,
        width: TRAY_WIDTH_PX,
        height: TRAY_MIN_HEIGHT_PX,
    };

    let preview = focused_preview_rect(viewport, tray);

    assert!(preview.x >= tray.right() + PREVIEW_GAP_PX - 0.01);
    assert!(preview.right() <= viewport.width - TRAY_VIEWPORT_MARGIN_PX);
}

#[test]
fn stack_bloom_petal_size_matches_tauri_buckets() {
    assert_eq!(
        stack_bloom_petal_size(4),
        StackBloomPetalSize {
            width: 108.0,
            height: 96.0,
            icon_size: 36.0,
        }
    );
    assert_eq!(
        stack_bloom_petal_size(8),
        StackBloomPetalSize {
            width: 92.0,
            height: 84.0,
            icon_size: 32.0,
        }
    );
    assert_eq!(
        stack_bloom_petal_size(16),
        StackBloomPetalSize {
            width: 80.0,
            height: 72.0,
            icon_size: 28.0,
        }
    );
    assert_eq!(
        stack_bloom_petal_size(17),
        StackBloomPetalSize {
            width: 72.0,
            height: 64.0,
            icon_size: 24.0,
        }
    );
}

#[test]
fn stack_bloom_petal_content_layout_matches_tauri_column_box() {
    let petal = Rect {
        x: 24.0,
        y: 40.0,
        width: BLOOM_PETAL_WIDTH_PX,
        height: BLOOM_PETAL_HEIGHT_PX,
    };

    let layout = stack_bloom_petal_content_layout(petal, BLOOM_PETAL_ICON_PX, 1.0);

    assert_close(
        layout.icon_rect.x,
        petal.x + (petal.width - BLOOM_PETAL_ICON_PX) * 0.5,
    );
    assert_close(layout.icon_rect.y, petal.y + BLOOM_PETAL_PADDING_Y_PX);
    assert_close(layout.icon_rect.width, BLOOM_PETAL_ICON_PX);
    assert_close(layout.title_rect.x, petal.x + BLOOM_PETAL_PADDING_X_PX);
    assert_close(
        layout.title_rect.y,
        layout.icon_rect.bottom() + BLOOM_PETAL_CONTENT_GAP_PX,
    );
    assert_close(
        layout.title_rect.width,
        petal.width - BLOOM_PETAL_PADDING_X_PX * 2.0,
    );
    assert!(
        layout.title_rect.height
            >= BLOOM_PETAL_NAME_FONT_PX
                * BLOOM_PETAL_NAME_LINE_HEIGHT
                * BLOOM_PETAL_NAME_MAX_LINES as f32
    );
    assert!(layout.title_rect.bottom() <= petal.bottom() + 0.01);
}
