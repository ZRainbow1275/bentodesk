use super::*;
use bento_nano_layout::LayoutSource;

#[test]
fn variant_is_wide() {
    assert!(!CardVariant::Standard.is_wide());
    assert!(CardVariant::Wide.is_wide());
}

#[test]
fn variant_column_span_matches_grid_helper() {
    assert_eq!(CardVariant::Standard.column_span(), 1);
    assert_eq!(CardVariant::Wide.column_span(), 2);
}

#[test]
fn variant_height_matches_grid_row() {
    assert!((CardVariant::Standard.height_px() - ITEM_GRID_ROW_HEIGHT_PX).abs() < 0.01);
    assert!((CardVariant::Wide.height_px() - ITEM_GRID_ROW_HEIGHT_PX).abs() < 0.01);
}

#[test]
fn variant_direction_per_snap_md() {
    assert_eq!(CardVariant::Standard.direction(), Direction::Column);
    assert_eq!(CardVariant::Wide.direction(), Direction::Row);
}

#[test]
fn card_scale_idle_is_identity() {
    assert!((card_scale_for(0.0, 0.0) - 1.0).abs() < 1e-6);
}

#[test]
fn card_scale_hover_inflates_to_tauri_1_02() {
    // Full hover, no press → scale(1.02).
    assert!((card_scale_for(1.0, 0.0) - CARD_HOVER_SCALE).abs() < 1e-5);
    // Half hover sits between 1.0 and 1.02.
    let half = card_scale_for(0.5, 0.0);
    assert!(half > 1.0 && half < CARD_HOVER_SCALE);
}

#[test]
fn card_scale_press_deflates_to_tauri_0_97() {
    // Full press, no hover → scale(0.97).
    assert!((card_scale_for(0.0, 1.0) - CARD_PRESS_SCALE).abs() < 1e-5);
}

#[test]
fn card_scale_press_overrides_hover_to_a_net_shrink() {
    // Pressing while hovered must read as a relative shrink (< the hover
    // peak) — Tauri `:active` overrides `:hover`.
    let pressed_while_hovered = card_scale_for(1.0, 1.0);
    assert!(pressed_while_hovered < CARD_HOVER_SCALE);
    // 1.02 * 0.97 = 0.9894 — below 1.0, a visible shrink.
    assert!((pressed_while_hovered - (CARD_HOVER_SCALE * CARD_PRESS_SCALE)).abs() < 1e-5);
    assert!(pressed_while_hovered < 1.0);
}

#[test]
fn card_press_duration_matches_tauri_80ms() {
    assert_eq!(CARD_PRESS_DURATION_MS, 80);
}

#[test]
fn card_hover_duration_matches_tauri_transition_fast_150ms() {
    // Tauri `.item-card { transition: all var(--transition-fast) }`,
    // `--transition-fast: 150ms ease-out`.
    assert_eq!(CARD_HOVER_DURATION_MS, 150);
}

#[test]
fn card_enter_constants_fit_fast_native_morph_envelope() {
    assert_eq!(CARD_ENTER_DURATION_MS, 190);
    assert_eq!(CARD_ENTER_STAGGER_MS, 10);
    assert_eq!(CARD_ENTER_START_DELAY_MS, 0);
    assert_eq!(
        CARD_ENTER_MORPH_ENVELOPE_MS,
        crate::zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS
    );
    assert_eq!(CARD_ENTER_MAX_STAGGER_INDEX, 5);
    assert_eq!(
        CARD_ENTER_DURATION_MS + CARD_ENTER_STAGGER_MS * CARD_ENTER_MAX_STAGGER_INDEX as u32,
        CARD_ENTER_MORPH_ENVELOPE_MS
    );
    assert!((CARD_ENTER_OFFSET_Y - 6.0).abs() < f32::EPSILON);
}

#[test]
fn card_enter_progress_staggers_inside_morph_envelope() {
    assert!(card_enter_progress_for_morph(0.0, 0).abs() < 1e-6);

    let first = card_enter_progress_for_morph(0.20, 0);
    let second = card_enter_progress_for_morph(0.20, 1);
    let later = card_enter_progress_for_morph(0.20, 4);
    assert!(first > second);
    assert!(second > later);

    for index in [0, CARD_ENTER_MAX_STAGGER_INDEX, 42] {
        assert!((card_enter_progress_for_morph(1.0, index) - 1.0).abs() < 1e-6);
    }
}

#[test]
fn card_enter_has_no_base_delay_beyond_native_stagger() {
    let first = card_enter_progress_for_morph(0.15, 0);
    let delayed = card_enter_progress_for_morph(0.15, CARD_ENTER_MAX_STAGGER_INDEX);

    assert!(first > 0.0);
    assert!(delayed.abs() < 1e-6);
}

#[test]
fn card_enter_translate_y_follows_tauri_keyframe_endpoints() {
    assert!((card_enter_translate_y(0.0) - 6.0).abs() < 1e-6);
    assert!(card_enter_translate_y(0.5) > 0.0);
    assert!(card_enter_translate_y(0.5) < 6.0);
    assert!(card_enter_translate_y(1.0).abs() < 1e-6);
    assert!(card_enter_translate_y(2.0).abs() < 1e-6);
}

#[test]
fn card_ramp_endpoints_and_easeout_shape() {
    // Rising: 0 at start, 1 at/after the full duration, decelerating
    // (past linear at the midpoint).
    assert!(card_ramp_t(0, 150, true).abs() < 1e-5);
    assert!((card_ramp_t(150, 150, true) - 1.0).abs() < 1e-5);
    assert!((card_ramp_t(300, 150, true) - 1.0).abs() < 1e-5); // clamps
    let mid = card_ramp_t(75, 150, true);
    assert!(mid > 0.5 && mid < 1.0); // ease-out is ahead of linear 0.5
    // Falling is the mirror: 1 at start, 0 at the end.
    assert!((card_ramp_t(0, 150, false) - 1.0).abs() < 1e-5);
    assert!(card_ramp_t(150, 150, false).abs() < 1e-5);
    // Rising + falling at the same elapsed sum to 1 (continuous reversal).
    assert!((card_ramp_t(75, 150, true) + card_ramp_t(75, 150, false) - 1.0).abs() < 1e-5);
}

#[test]
fn card_ramp_uses_single_ease_out_cubic_ssot() {
    // #2 step 9 (2026-06-02) — `card_ramp_t` (rising) must equal the shared
    // `animator::ease_out_cubic` SSoT at sample points (the inlined cubic
    // copy was replaced by a call to it). Pin a few `t` so the item-card and
    // pill easing can never silently diverge again.
    for &(elapsed, raw) in &[
        (0_u32, 0.0_f32),
        (30, 0.2),
        (75, 0.5),
        (120, 0.8),
        (150, 1.0),
    ] {
        let ramp = card_ramp_t(elapsed, 150, true);
        let direct = crate::animator::ease_out_cubic(raw);
        assert!(
            (ramp - direct).abs() < 1e-6,
            "card_ramp_t({elapsed}) = {ramp} != ease_out_cubic({raw}) = {direct}"
        );
    }
}

#[test]
fn card_ramp_zero_duration_does_not_div_by_zero() {
    // `duration_ms.max(1)` guards the divide. With a 0ms duration the
    // 1ms floor means any elapsed >= 1 reads as fully complete (rising →
    // 1.0, falling → 0.0); the start sample at elapsed 0 is still the
    // ramp origin. The key invariant is "no panic / no NaN".
    assert!(card_ramp_t(0, 0, true).abs() < 1e-5);
    assert!((card_ramp_t(1, 0, true) - 1.0).abs() < 1e-5);
    assert!((card_ramp_t(1, 0, false)).abs() < 1e-5);
    assert!(card_ramp_t(10, 0, true).is_finite());
}

fn card(z: u64, i: u64) -> (ZoneId, ZoneItemId) {
    (ZoneId(z), ZoneItemId(i))
}

#[test]
fn item_hover_state_idle_samples_identity() {
    let st = ItemHoverState::new();
    let (h, p) = st.sample(card(1, 1), 1_000);
    assert!(h.abs() < 1e-6 && p.abs() < 1e-6);
    assert!((card_scale_for(h, p) - 1.0).abs() < 1e-6);
    assert!(!st.is_active(1_000));
}

#[test]
fn item_hover_enter_ramps_up_and_changes_target() {
    let mut st = ItemHoverState::new();
    assert!(st.on_hover(Some(card(1, 7)), 1_000));
    // Same target again is a no-op (no spurious redraw).
    assert!(!st.on_hover(Some(card(1, 7)), 1_050));
    // Mid-ramp the hovered card is between 0 and 1.
    let (h, _) = st.sample(card(1, 7), 1_000 + 75);
    assert!(h > 0.0 && h < 1.0);
    // Fully ramped after the 150ms window.
    let (h_full, _) = st.sample(card(1, 7), 1_000 + 150);
    assert!((h_full - 1.0).abs() < 1e-5);
    assert!(st.is_active(1_000 + 75));
}

#[test]
fn item_hover_handoff_ramps_prev_down_and_next_up() {
    let mut st = ItemHoverState::new();
    st.on_hover(Some(card(1, 1)), 0); // settle card A up
    let _ = st.sample(card(1, 1), 200);
    st.on_hover(Some(card(1, 2)), 200); // hand off to card B
    // Card A (leaving) ramps down from 1.0; card B (entering) ramps up.
    let (a_h, _) = st.sample(card(1, 1), 200 + 75);
    let (b_h, _) = st.sample(card(1, 2), 200 + 75);
    assert!(a_h > 0.0 && a_h < 1.0);
    assert!(b_h > 0.0 && b_h < 1.0);
    // After the leave window the prior card retires to identity.
    let _ = st.tick(200 + CARD_HOVER_DURATION_MS);
    let (a_done, _) = st.sample(card(1, 1), 200 + CARD_HOVER_DURATION_MS);
    assert!(a_done.abs() < 1e-5);
}

#[test]
fn item_press_ramps_to_tauri_shrink_then_releases() {
    let mut st = ItemHoverState::new();
    st.on_hover(Some(card(2, 5)), 0);
    st.on_press(card(2, 5), 0);
    // Full press → press_t 1.0 → composed scale is the 1.02*0.97 shrink.
    let (h, p) = st.sample(
        card(2, 5),
        CARD_HOVER_DURATION_MS.max(CARD_PRESS_DURATION_MS),
    );
    assert!((p - 1.0).abs() < 1e-5);
    let scale = card_scale_for(h, p);
    assert!(scale < 1.0);
    assert!((scale - CARD_HOVER_SCALE * CARD_PRESS_SCALE).abs() < 1e-4);
    // Release ramps press back toward 0; tick retires it after 80ms.
    assert!(st.on_release(200));
    assert!(st.is_active(200 + 10));
    let _ = st.tick(200 + CARD_PRESS_DURATION_MS);
    let (_h2, p2) = st.sample(card(2, 5), 200 + CARD_PRESS_DURATION_MS);
    assert!(p2.abs() < 1e-5);
}

#[test]
fn item_press_only_on_pressed_card() {
    let mut st = ItemHoverState::new();
    st.on_press(card(3, 1), 0);
    // A different card sees no press.
    let (_h, p) = st.sample(card(3, 2), CARD_PRESS_DURATION_MS);
    assert!(p.abs() < 1e-5);
}

#[test]
fn display_name_strips_lnk_and_url_case_insensitive() {
    assert_eq!(display_name("Notes.lnk"), "Notes");
    assert_eq!(display_name("Notes.LNK"), "Notes");
    assert_eq!(display_name("Bookmark.URL"), "Bookmark");
    assert_eq!(display_name("Bookmark.url"), "Bookmark");
}

#[test]
fn display_name_preserves_other_extensions() {
    assert_eq!(display_name("photo.png"), "photo.png");
    assert_eq!(display_name("readme.md"), "readme.md");
}

#[test]
fn display_name_handles_short_and_empty() {
    assert_eq!(display_name(""), "");
    assert_eq!(display_name("a"), "a");
    assert_eq!(display_name(".md"), ".md"); // 3 chars, untouched
}

#[test]
fn build_standard_is_column_oriented_and_row_height() {
    let node = build();
    let layout = node.layout();
    assert_eq!(layout.direction, Direction::Column);
    assert!(matches!(layout.height, Length::Px(h) if (h - ITEM_GRID_ROW_HEIGHT_PX).abs() < 0.01));
}

#[test]
fn build_wide_is_row_oriented() {
    let node = build_with(CardVariant::Wide);
    let layout = node.layout();
    assert_eq!(layout.direction, Direction::Row);
}

#[test]
fn card_variant_serde_round_trip() {
    for v in [CardVariant::Standard, CardVariant::Wide] {
        let s = serde_json::to_string(&v).unwrap_or_default();
        let back: CardVariant = serde_json::from_str(&s).unwrap_or_default();
        assert_eq!(v, back);
    }
    assert_eq!(
        serde_json::to_string(&CardVariant::Wide).unwrap_or_default(),
        "\"wide\""
    );
}

#[test]
fn item_card_chrome_accepts_explicit_active_palette() {
    let palette = PaletteTokens {
        bg: Color::from_u8(0x01, 0x02, 0x03, 0xFF),
        surface: Color::from_u8(0x11, 0x12, 0x13, 0xFF),
        surface_alt: Color::from_u8(0x21, 0x22, 0x23, 0xFF),
        border: Color::from_u8(0x31, 0x32, 0x33, 0xFF),
        text: Color::from_u8(0x41, 0x42, 0x43, 0xFF),
        text_muted: Color::from_u8(0x51, 0x52, 0x53, 0xFF),
        accent: Color::from_u8(0x61, 0x62, 0x63, 0xFF),
        accent_hover: Color::from_u8(0x71, 0x72, 0x73, 0xFF),
        danger: Color::from_u8(0x81, 0x82, 0x83, 0xFF),
        success: Color::from_u8(0x91, 0x92, 0x93, 0xFF),
        warning: Color::from_u8(0xA1, 0xA2, 0xA3, 0xFF),
        info: Color::from_u8(0xB1, 0xB2, 0xB3, 0xFF),
        scrim: Color::from_u8(0xC1, 0xC2, 0xC3, 0xFF),
        hover_overlay: Color::from_u8(0xD1, 0xD2, 0xD3, 0xFF),
        active_overlay: Color::from_u8(0xE1, 0xE2, 0xE3, 0xFF),
        selection: Color::from_u8(0xF1, 0xF2, 0xF3, 0xFF),
    };

    let chrome = ItemCardChrome::from_palette(palette);

    // M2 E-03 — card radius is the Tauri `--radius-card` (10), NOT the
    // live `radius.md` (6).
    assert_eq!(
        chrome.card_radius,
        BorderRadius::all(bento_nano_style::tokens::RADIUS.card)
    );
    // Normal bg is the Tauri `--surface-subtle` (white @ 0.03), not the
    // warm `surface_alt @ 0.46`.
    assert_eq!(
        chrome.normal_background,
        bento_nano_style::tokens::PALETTE_DARK.surface_subtle
    );
    assert_eq!(
        chrome.drag_source_background,
        with_alpha(palette.surface_alt, 0.18)
    );
    assert_eq!(chrome.ghost_background, with_alpha(palette.surface, 0.86));
    assert_eq!(chrome.ghost_shadow, with_alpha(palette.scrim, 0.24));
    // Missing fill softened toward Tauri `rgba(239,68,68,0.08)`.
    assert_eq!(chrome.missing_background, with_alpha(palette.danger, 0.10));
    // V21-C3 — name text is the Tauri `--text-secondary` (#c0c0cc).
    assert_eq!(
        chrome.text,
        bento_nano_style::tokens::PALETTE_DARK.text_secondary
    );
    assert_eq!(
        chrome.icon_text,
        bento_nano_style::tokens::PALETTE_DARK.text_primary
    );
}

#[test]
fn item_card_chrome_uses_tauri_card_radius_token() {
    // E-03 — `card_radius` is pinned to the static Tauri `--radius-card`
    // (10) regardless of the passed live `radius.md`, so the card corner
    // matches the reference exactly.
    let palette = bento_nano_theme::current().palette;
    let radius = RadiusTokens {
        sm: BorderRadius::all(3.0),
        md: BorderRadius::all(7.0),
        lg: BorderRadius::all(11.0),
        xl: BorderRadius::all(17.0),
        full: BorderRadius::all(999.0),
    };

    let chrome = ItemCardChrome::from_tokens(
        palette,
        radius,
        bento_nano_style::tokens::PALETTE_DARK.surface_subtle,
        bento_nano_style::tokens::PALETTE_DARK.text_secondary,
        bento_nano_style::tokens::PALETTE_DARK.text_primary,
        bento_nano_style::tokens::PALETTE_DARK.surface_hover,
        bento_nano_style::tokens::PALETTE_DARK.border_hover,
    );

    assert_eq!(
        chrome.card_radius,
        BorderRadius::all(bento_nano_style::tokens::RADIUS.card)
    );
}

#[test]
fn item_card_chrome_icon_text_uses_live_tauri_token() {
    let palette = bento_nano_theme::current().palette;
    let dark = ItemCardChrome::from_tokens(
        palette,
        radius::DEFAULT,
        bento_nano_style::tokens::PALETTE_DARK.surface_subtle,
        bento_nano_style::tokens::PALETTE_DARK.text_secondary,
        bento_nano_style::tokens::PALETTE_DARK.text_primary,
        bento_nano_style::tokens::PALETTE_DARK.surface_hover,
        bento_nano_style::tokens::PALETTE_DARK.border_hover,
    );
    let light = ItemCardChrome::from_tokens(
        palette,
        radius::DEFAULT,
        bento_nano_style::tokens::PALETTE_LIGHT.surface_subtle,
        bento_nano_style::tokens::PALETTE_LIGHT.text_secondary,
        bento_nano_style::tokens::PALETTE_LIGHT.text_primary,
        bento_nano_style::tokens::PALETTE_LIGHT.surface_hover,
        bento_nano_style::tokens::PALETTE_LIGHT.border_hover,
    );

    assert_eq!(
        dark.icon_text,
        bento_nano_style::tokens::PALETTE_DARK.text_primary
    );
    assert_eq!(
        light.icon_text,
        bento_nano_style::tokens::PALETTE_LIGHT.text_primary
    );
    assert_ne!(dark.icon_text, light.icon_text);
}

#[test]
fn card_hover_lift_dy_zero_at_idle_minus_one_at_full_hover() {
    // FIX 1 — the renderer offsets `card_rect.y` by `CARD_HOVER_LIFT_DY *
    // hover_t`, mirroring Tauri `:hover { transform: translateY(-1px) }`.
    // At idle (hover_t 0) the lift is 0; at full hover (hover_t 1) it is
    // exactly -1 px; mid-hover lerps linearly. The renderer applies the
    // const directly so this pure check pins the contract the const must
    // satisfy.
    let lift_at = |hover_t: f32| CARD_HOVER_LIFT_DY * hover_t;
    assert!(lift_at(0.0).abs() < 1e-6, "idle card must not lift");
    assert!(
        (lift_at(1.0) - (-1.0)).abs() < 1e-6,
        "full hover must lift exactly -1px"
    );
    // Monotone upward (more negative) as hover ramps in.
    assert!(lift_at(0.5) < 0.0 && lift_at(0.5) > lift_at(1.0));
    assert!((lift_at(0.5) - (-0.5)).abs() < 1e-6);
}

#[test]
fn item_card_chrome_exposes_tauri_hover_chrome_tokens() {
    // FIX 2 — hover background/border come from the live palette; the
    // two-layer `--shadow-item-hover` is theme-independent black at the
    // Tauri alphas (0.12 outer / 0.08 inner).
    let palette = bento_nano_theme::current().palette;
    let chrome = ItemCardChrome::from_tokens(
        palette,
        radius::DEFAULT,
        bento_nano_style::tokens::PALETTE_DARK.surface_subtle,
        bento_nano_style::tokens::PALETTE_DARK.text_secondary,
        bento_nano_style::tokens::PALETTE_DARK.text_primary,
        bento_nano_style::tokens::PALETTE_DARK.surface_hover,
        bento_nano_style::tokens::PALETTE_DARK.border_hover,
    );
    assert_eq!(
        chrome.hover_background,
        bento_nano_style::tokens::PALETTE_DARK.surface_hover
    );
    assert_eq!(
        chrome.hover_border,
        bento_nano_style::tokens::PALETTE_DARK.border_hover
    );
    // 0.12 * 255 ≈ 31 (0x1F); 0.08 * 255 ≈ 20 (0x14).
    assert_eq!(chrome.hover_shadow_outer, Color::from_u8(0, 0, 0, 0x1F));
    assert_eq!(chrome.hover_shadow_inner, Color::from_u8(0, 0, 0, 0x14));
}
