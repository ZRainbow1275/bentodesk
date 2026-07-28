#[test]
fn bulk_manager_chrome_accepts_explicit_active_palette() {
    let mut palette = theme::current().palette;
    palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
    palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
    palette.hover_overlay = Color::from_u8(0x33, 0x44, 0x55, 0x99);
    palette.selection = Color::from_u8(0x44, 0xAA, 0xEE, 0x66);
    palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
    palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);

    let chrome = BulkManagerChrome::from_palette(palette);

    assert_eq!(
        chrome.panel_background,
        Color::from_u8(0x22, 0x33, 0x44, 0xDD)
    );
    assert_eq!(
        chrome.row_background,
        Color::from_u8(0x11, 0x22, 0x33, 0xEE)
    );
    assert_eq!(
        chrome.cursor_background,
        Color::from_u8(0x33, 0x44, 0x55, 0x99)
    );
    assert_eq!(
        chrome.selected_background,
        Color::from_u8(0x44, 0xAA, 0xEE, 0x66)
    );
    assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
}

#[test]
fn bulk_manager_chrome_accepts_explicit_radius_shadow_tokens() {
    let palette = theme::current().palette;
    let radius = theme::RadiusTokens {
        sm: BorderRadius::all(3.0),
        md: BorderRadius::all(7.0),
        lg: BorderRadius::all(11.0),
        xl: BorderRadius::all(17.0),
        full: BorderRadius::all(999.0),
    };
    let mut shadow = theme::shadow::DEFAULT;
    shadow.md = Shadow {
        offset_x: 2.0,
        offset_y: 5.0,
        blur: 13.0,
        spread: 0.0,
        color: Color::from_u8(0x10, 0x20, 0x30, 0x99),
    };

    let chrome = BulkManagerChrome::from_tokens(palette, radius, shadow);

    assert_eq!(chrome.panel_shadow, shadow.md);
    assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
    assert_eq!(chrome.search_radius, BorderRadius::all(11.0));
    assert_eq!(chrome.button_radius, BorderRadius::all(7.0));
    assert_eq!(chrome.sort_radius, BorderRadius::all(7.0));
    assert_eq!(chrome.row_radius, BorderRadius::all(11.0));
    assert_eq!(chrome.edit_radius, BorderRadius::all(7.0));
}

#[test]
fn bulk_manager_panel_shadow_rect_uses_token_shadow_geometry() {
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

    let rect = bulk_manager_panel_shadow_rect(panel, shadow);

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

/// ΔB lock: `SortKey` round-trips through serde (`lowercase` rename
/// matches the wire format).
#[test]
fn sort_key_serde_round_trip() {
    for v in SortKey::ALL {
        let s = serde_json::to_string(v).unwrap_or_default();
        let back: SortKey = serde_json::from_str(&s).unwrap_or_default();
        assert_eq!(*v, back);
    }
    assert_eq!(
        serde_json::to_string(&SortKey::Accent).unwrap_or_default(),
        "\"accent\""
    );
}

/// ΔB lock: `BulkManagerAction` round-trips through serde so any
/// future scripting surface (Phase 5+) can hand actions back to the
/// panel.
#[test]
fn bulk_manager_action_serde_round_trip() {
    let action = BulkManagerAction::Move {
        ids: vec![ZoneId(1), ZoneId(2)],
        delta: Point::new(5, -7),
    };
    let s = serde_json::to_string(&action).unwrap_or_default();
    let back: BulkManagerAction = serde_json::from_str(&s).unwrap_or(BulkManagerAction::Close);
    assert_eq!(back, action);
}

/// ΔB lock: `ZoneRow` round-trips through serde so the row list can
/// be hydrated from a backend JSON payload in Phase 5+.
#[test]
fn zone_row_serde_round_trip() {
    let r = sample_row(42, "Sample", 7, "#abcdef", 33, 44);
    let s = serde_json::to_string(&r).unwrap_or_default();
    let back: ZoneRow = serde_json::from_str(&s).unwrap_or_else(|_| r.clone());
    assert_eq!(back, r);
}

#[test]
fn bulk_manager_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
    use bentodesk_style::BorderRadius;
    use bentodesk_style::tokens as style_tokens;
    let chrome = BulkManagerChrome::from_tauri_tokens(
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
        style_tokens::PALETTE_DARK.surface_hover
    );
    assert_eq!(
        chrome.cursor_background,
        style_tokens::PALETTE_DARK.surface_active
    );
    assert_eq!(
        chrome.selected_background,
        style_tokens::PALETTE_DARK.surface_active
    );
    assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
    assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
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
