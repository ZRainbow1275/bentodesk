use super::*;

const INNER_W: f32 = 440.0;

#[test]
fn seventeen_presets_with_4_swatch_colors_each() {
    assert_eq!(BUILTIN_THEMES.len(), PRESET_COUNT);
    for (i, preset) in BUILTIN_THEMES.iter().enumerate() {
        assert_eq!(
            preset.id as usize, i,
            "preset id must equal its array index"
        );
        assert_eq!(preset.swatch_colors.len(), 4);
        for (q, c) in preset.swatch_colors.iter().enumerate() {
            assert!(c.a > 0.0, "preset {i} quadrant {q} alpha must be > 0");
        }
    }
}

#[test]
fn preset_name_ids_are_distinct() {
    for (i, theme) in BUILTIN_THEMES.iter().enumerate() {
        for (j, other) in BUILTIN_THEMES.iter().enumerate().skip(i + 1) {
            assert_ne!(
                theme.name_id, other.name_id,
                "presets {i} and {j} must have distinct name ids",
            );
        }
    }
}

#[test]
fn preset_theme_ids_are_distinct_and_match_builtin_set() {
    // Every theme_id resolves through the M6a 17-id palette lookup.
    for (i, theme) in BUILTIN_THEMES.iter().enumerate() {
        for (j, other) in BUILTIN_THEMES.iter().enumerate().skip(i + 1) {
            assert_ne!(
                theme.theme_id, other.theme_id,
                "presets {i} and {j} must have distinct theme ids",
            );
        }
        assert!(
            bentodesk_style::tokens::palette_tauri_for_theme(theme.theme_id).is_some(),
            "preset {i} theme_id {} must be a known builtin",
            theme.theme_id,
        );
    }
}

#[test]
fn group_counts_match_tauri_families() {
    let mut rounded = 0;
    let mut solid = 0;
    let mut angular = 0;
    let mut personality = 0;
    for p in &BUILTIN_THEMES {
        match p.group {
            ThemeGroup::Rounded => rounded += 1,
            ThemeGroup::Solid => solid += 1,
            ThemeGroup::Angular => angular += 1,
            ThemeGroup::Personality => personality += 1,
        }
    }
    assert_eq!((rounded, solid, angular, personality), (9, 1, 4, 3));
}

#[test]
fn vertical_density_matches_tauri_theme_group_css() {
    assert!((GROUP_HEADING_HEIGHT - 12.0).abs() < f32::EPSILON);
    assert!((CARD_LABEL_HEIGHT - 12.0).abs() < f32::EPSILON);
    assert!((GROUP_TO_GROUP_GAP - 12.0).abs() < f32::EPSILON);
    assert!((THEME_CARD_HEIGHT - 80.0).abs() < f32::EPSILON);

    let layout = appearance_layout(Point::ZERO, INNER_W);
    let rounded_grid_h = 3.0 * THEME_CARD_HEIGHT + 2.0 * THEME_GRID_GAP;
    let single_row_grid_h = THEME_CARD_HEIGHT;
    let rounded_group_h = GROUP_HEADING_HEIGHT + GROUP_HEADING_TO_GRID_GAP + rounded_grid_h;
    let single_row_group_h = GROUP_HEADING_HEIGHT + GROUP_HEADING_TO_GRID_GAP + single_row_grid_h;
    let theme_groups_bottom = rounded_group_h
        + GROUP_TO_GROUP_GAP
        + single_row_group_h
        + GROUP_TO_GROUP_GAP
        + single_row_group_h
        + GROUP_TO_GROUP_GAP
        + single_row_group_h;

    assert!((theme_groups_bottom - 608.0).abs() < 0.01);
    assert!((layout.accent_row.y - (theme_groups_bottom + ACCENT_ROW_TOP_GAP)).abs() < 0.01);
    assert!((layout.total_height - (layout.accent_row.bottom())).abs() < 0.01);
}

#[test]
fn frosted_has_two_translucent_quadrants() {
    let frosted = BUILTIN_THEMES
        .iter()
        .find(|p| p.theme_id == "frosted")
        .unwrap();
    // TL ≈ 0.15, BR ≈ 0.25 — translucent; TR/BL opaque.
    assert!(frosted.swatch_colors[0].a < 0.99 && frosted.swatch_colors[0].a > 0.0);
    assert!(frosted.swatch_colors[3].a < 0.99 && frosted.swatch_colors[3].a > 0.0);
    assert!((frosted.swatch_colors[1].a - 1.0).abs() < f32::EPSILON);
    assert!((frosted.swatch_colors[2].a - 1.0).abs() < f32::EPSILON);
}

#[test]
fn layout_cards_are_in_four_columns_per_group() {
    let layout = appearance_layout(Point::new(20.0, 60.0), INNER_W);
    // Rounded group has 9 cards (ids 0..9): rows 0,1 full (4 each), row 2 = 1.
    // Card 0 (col 0) and card 3 (col 3) share a row; card 4 starts row 1.
    let c0 = layout.cards[0];
    let c3 = layout.cards[3];
    let c4 = layout.cards[4];
    assert!((c0.y - c3.y).abs() < 0.01, "first 4 cards share a row");
    assert!(c4.y > c0.y, "5th card wraps to the next row");
    assert!(c3.x > c0.x, "columns increase left→right");
    // All card widths equal.
    for i in 1..PRESET_COUNT {
        assert!((layout.cards[i].width - c0.width).abs() < 0.01);
    }
}

#[test]
fn layout_groups_stack_in_render_order() {
    let layout = appearance_layout(Point::new(20.0, 60.0), INNER_W);
    // Heading 0 (Rounded) above heading 1 (Solid) above 2 (Angular) above
    // 3 (Personality).
    assert!(layout.group_headings[0].y < layout.group_headings[1].y);
    assert!(layout.group_headings[1].y < layout.group_headings[2].y);
    assert!(layout.group_headings[2].y < layout.group_headings[3].y);
    // Solid heading sits below the last Rounded card (id 8).
    assert!(layout.group_headings[1].y > layout.cards[8].bottom());
}

#[test]
fn layout_swatch_blocks_centred_in_cards() {
    let layout = appearance_layout(Point::ZERO, INNER_W);
    for i in 0..PRESET_COUNT {
        let card = layout.cards[i];
        let block = layout.swatch_blocks[i];
        assert_eq!(block.width, SWATCH_BLOCK_SIZE);
        assert_eq!(block.height, SWATCH_BLOCK_SIZE);
        // Horizontally centred.
        let lead = block.x - card.x;
        let trail = card.right() - block.right();
        assert!((lead - trail).abs() < 0.01, "block {i} not centred");
        // Pad-top from card top.
        assert!((block.y - card.y - THEME_CARD_PAD_TOP).abs() < 0.01);
    }
}

#[test]
fn layout_accent_row_matches_tauri_compact_color_control() {
    let layout = appearance_layout(Point::ZERO, INNER_W);
    // Accent row sits below every theme card.
    for i in 0..PRESET_COUNT {
        assert!(layout.accent_row.y >= layout.cards[i].bottom() - 0.01);
    }
    assert_eq!(layout.accent_row.height, 42.0);
    assert_eq!(layout.accent_picker.width, 36.0);
    assert_eq!(layout.accent_picker.height, 28.0);
    assert!((layout.accent_picker.right() - layout.accent_row.right()).abs() < 0.01);
    assert!(
        (layout.accent_picker.y
            - layout.accent_row.y
            - (layout.accent_row.height - layout.accent_picker.height) * 0.5)
            .abs()
            < 0.01
    );
    assert!(layout.accent_picker.y >= layout.accent_row.y);
    assert!(layout.accent_picker.bottom() <= layout.accent_row.bottom() + 0.01);
    assert_eq!(layout.accent_input, Rect::ZERO);
    assert_eq!(layout.accent_clear, Rect::ZERO);
    assert!(
        layout
            .accent_swatches
            .iter()
            .all(|rect| *rect == Rect::ZERO)
    );
}

#[test]
fn total_height_spans_grid_plus_accent_row() {
    let layout = appearance_layout(Point::new(0.0, 100.0), INNER_W);
    assert!((layout.total_height - (layout.accent_row.bottom() - 100.0)).abs() < 0.01);
    assert!(layout.total_height > 0.0);
    // appearance_content_height agrees (anchor-independent).
    assert!((appearance_content_height(INNER_W) - layout.total_height).abs() < 0.01);
}

#[test]
fn quadrants_tile_without_overlap_inside_block() {
    let block = Rect {
        x: 100.0,
        y: 200.0,
        width: SWATCH_BLOCK_SIZE,
        height: SWATCH_BLOCK_SIZE,
    };
    let quads = thumbnail_swatch_quadrants(block, SWATCH_INNER_GAP);
    for q in &quads {
        assert!(q.width > 0.0 && q.height > 0.0);
        assert!(q.x >= block.x);
        assert!(q.y >= block.y);
        assert!(q.right() <= block.right() + 0.01);
        assert!(q.bottom() <= block.bottom() + 0.01);
    }
    assert_eq!(quads[0].y, quads[1].y);
    assert_eq!(quads[2].y, quads[3].y);
    assert_eq!(quads[0].x, quads[2].x);
    assert_eq!(quads[1].x, quads[3].x);
    // 3-DIP gutter divides them.
    assert!((quads[1].x - quads[0].right() - SWATCH_INNER_GAP).abs() < 0.01);
    assert!((quads[2].y - quads[0].bottom() - SWATCH_INNER_GAP).abs() < 0.01);
}

#[test]
fn hit_test_each_card_centre() {
    let layout = appearance_layout(Point::new(20.0, 60.0), INNER_W);
    for i in 0..PRESET_COUNT {
        let c = layout.cards[i];
        let cx = c.x + c.width * 0.5;
        let cy = c.y + c.height * 0.5;
        assert_eq!(
            appearance_hit_test(&layout, cx, cy),
            Some(AppearanceHit::Card(i as u8)),
            "centre of card {i} must hit-test to Card({i})",
        );
    }
}

#[test]
fn hit_test_accent_picker() {
    let layout = appearance_layout(Point::new(20.0, 60.0), INNER_W);
    let picker = layout.accent_picker;
    assert_eq!(
        appearance_hit_test(
            &layout,
            picker.x + picker.width * 0.5,
            picker.y + picker.height * 0.5,
        ),
        Some(AppearanceHit::AccentPicker)
    );
}

#[test]
fn hit_test_none_outside_all_regions() {
    let layout = appearance_layout(Point::new(20.0, 60.0), INNER_W);
    assert_eq!(appearance_hit_test(&layout, -100.0, -100.0), None);
}

#[test]
fn accent_swatch_hex_matches_swatch_count_and_clamps() {
    for s in 0..ACCENT_SWATCH_COUNT {
        assert!(accent_swatch_hex(s).unwrap().starts_with('#'));
        assert_eq!(accent_swatch_hex(s).unwrap().len(), 7);
    }
    assert_eq!(accent_swatch_hex(ACCENT_SWATCH_COUNT), None);
}

#[test]
fn point_zero_constructs() {
    assert_eq!(Point::ZERO, Point::new(0.0, 0.0));
}
