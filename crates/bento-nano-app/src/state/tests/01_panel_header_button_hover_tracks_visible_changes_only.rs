#[test]
fn panel_header_button_hover_tracks_visible_changes_only() {
    let app = AppState::new();
    let search = PanelHeaderButtonHover::new(ZoneId(7), PanelHeaderButtonKind::Search);
    let close = PanelHeaderButtonHover::new(ZoneId(7), PanelHeaderButtonKind::Close);

    assert_eq!(app.panel_header_button_hover.get(), None);
    assert!(!app.is_panel_header_button_hovered(ZoneId(7), PanelHeaderButtonKind::Search));

    assert!(app.set_panel_header_button_hover(Some(search)));
    assert!(app.is_panel_header_button_hovered(ZoneId(7), PanelHeaderButtonKind::Search));
    assert!(!app.set_panel_header_button_hover(Some(search)));

    assert!(app.set_panel_header_button_hover(Some(close)));
    assert!(app.is_panel_header_button_hovered(ZoneId(7), PanelHeaderButtonKind::Close));

    assert!(app.set_panel_header_button_hover(None));
    assert_eq!(app.panel_header_button_hover.get(), None);
    assert!(!app.set_panel_header_button_hover(None));
}

#[test]
fn settings_encryption_mode_hover_tracks_visible_changes_only() {
    let app = AppState::new();

    assert_eq!(app.settings_encryption_mode_hover.get(), None);
    assert!(!app.is_settings_encryption_mode_hovered(SettingsEncryptionMode::Dpapi));

    assert!(app.set_settings_encryption_mode_hover(Some(SettingsEncryptionMode::Dpapi)));
    assert!(app.is_settings_encryption_mode_hovered(SettingsEncryptionMode::Dpapi));
    assert!(!app.set_settings_encryption_mode_hover(Some(SettingsEncryptionMode::Dpapi)));

    assert!(app.set_settings_encryption_mode_hover(Some(SettingsEncryptionMode::Passphrase)));
    assert!(app.is_settings_encryption_mode_hovered(SettingsEncryptionMode::Passphrase));

    assert!(app.set_settings_encryption_mode_hover(None));
    assert_eq!(app.settings_encryption_mode_hover.get(), None);
    assert!(!app.set_settings_encryption_mode_hover(None));
}

#[test]
fn settings_appearance_hover_tracks_visible_changes_only() {
    let app = AppState::new();

    assert_eq!(app.settings_appearance_hover.get(), None);
    assert!(!app.is_settings_appearance_card_hovered(5));

    assert!(app.set_settings_appearance_hover(Some(crate::theme_picker::AppearanceHit::Card(5))));
    assert!(app.is_settings_appearance_card_hovered(5));
    assert!(!app.is_settings_appearance_accent_hovered(5));
    assert!(!app.set_settings_appearance_hover(Some(crate::theme_picker::AppearanceHit::Card(5))));

    assert!(app.set_settings_appearance_hover(Some(crate::theme_picker::AppearanceHit::Accent(3))));
    assert!(app.is_settings_appearance_accent_hovered(3));
    assert!(!app.is_settings_appearance_card_hovered(5));

    assert!(app.set_settings_appearance_hover(None));
    assert_eq!(app.settings_appearance_hover.get(), None);
    assert!(!app.set_settings_appearance_hover(None));
}

#[test]
fn settings_close_hover_tracks_visible_changes_only() {
    let app = AppState::new();

    assert!(!app.settings_close_hover.get());
    assert!(app.set_settings_close_hover(true));
    assert!(app.settings_close_hover.get());
    assert!(!app.set_settings_close_hover(true));
    assert!(app.set_settings_close_hover(false));
    assert!(!app.settings_close_hover.get());
    assert!(!app.set_settings_close_hover(false));
}

#[test]
fn settings_focused_field_default_is_none() {
    let app = AppState::new();
    assert_eq!(app.settings_focused_field.get(), SettingsTextField::None);
    // None/Passphrase fields are no-ops for the non-passphrase edit ops.
    assert!(!app.settings_focused_push_char('a'));
    app.settings_focused_field
        .set(SettingsTextField::Passphrase);
    assert!(!app.settings_focused_push_char('a'));
    assert!(!app.settings_focused_backspace());
    assert_eq!(app.settings_focused_caret(), 0);
}

#[test]
fn settings_focused_push_char_appends_and_caps() {
    let app = AppState::new();
    // DesktopPath — clear the seeded default, then append. Cap = 260.
    app.settings_focused_field
        .set(SettingsTextField::DesktopPath);
    *app.desktop_path_draft.borrow_mut() = SmolStr::default();
    assert!(app.settings_focused_push_char('C'));
    assert!(app.settings_focused_push_char(':'));
    assert_eq!(app.desktop_path_draft.borrow().as_str(), "C:");
    // Control chars rejected on DesktopPath (incl. newline — single-line).
    assert!(!app.settings_focused_push_char('\n'));
    assert!(!app.settings_focused_push_char('\t'));
    assert_eq!(app.desktop_path_draft.borrow().as_str(), "C:");
    // Cap: fill to the limit, then the next push is rejected.
    *app.desktop_path_draft.borrow_mut() =
        SmolStr::new("x".repeat(SETTINGS_DESKTOP_PATH_DRAFT_LIMIT));
    assert!(!app.settings_focused_push_char('y'));
    assert_eq!(
        app.desktop_path_draft.borrow().chars().count(),
        SETTINGS_DESKTOP_PATH_DRAFT_LIMIT
    );

    // WatchValues — newline IS allowed (one path per line); other controls
    // rejected. Non-ASCII (Chinese path) accepted.
    app.settings_focused_field
        .set(SettingsTextField::WatchValues);
    *app.watch_paths_draft.borrow_mut() = SmolStr::default();
    assert!(app.settings_focused_push_char('D'));
    assert!(app.settings_focused_push_char('\n'));
    assert!(app.settings_focused_push_char('桌'));
    assert!(app.settings_focused_push_char('面'));
    assert_eq!(app.watch_paths_draft.borrow().as_str(), "D\n桌面");
    assert!(!app.settings_focused_push_char('\r'));
    assert_eq!(app.watch_paths_draft.borrow().as_str(), "D\n桌面");
}

#[test]
fn settings_focused_backspace_pops_last_scalar() {
    let app = AppState::new();
    app.settings_focused_field
        .set(SettingsTextField::WatchValues);
    // Mix ASCII + a multi-byte CJK scalar; backspace must pop the scalar,
    // not a partial byte.
    *app.watch_paths_draft.borrow_mut() = SmolStr::new("a桌");
    assert!(app.settings_focused_backspace());
    assert_eq!(app.watch_paths_draft.borrow().as_str(), "a");
    assert!(app.settings_focused_backspace());
    assert_eq!(app.watch_paths_draft.borrow().as_str(), "");
    // Empty draft → no-op.
    assert!(!app.settings_focused_backspace());
}

#[test]
fn settings_focused_caret_equals_char_count() {
    let app = AppState::new();
    app.settings_focused_field
        .set(SettingsTextField::DesktopPath);
    *app.desktop_path_draft.borrow_mut() = SmolStr::new("C:\\桌面");
    // 5 scalar values: C : \ 桌 面 (CJK counts as ONE each).
    assert_eq!(app.settings_focused_caret(), 5);
}

#[test]
fn settings_accent_editor_seeds_from_persisted_or_default() {
    let app = AppState::new();
    assert_eq!(app.settings_accent_editor_value().as_str(), "#3b82f6");
    *app.theme_base_accent.borrow_mut() = Some(SmolStr::new_static("#f97316"));
    assert_eq!(app.settings_accent_editor_value().as_str(), "#f97316");

    app.focus_settings_accent_color();
    assert_eq!(
        app.settings_focused_field.get(),
        SettingsTextField::AccentColor
    );
    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#f97316")
    );
}

#[test]
fn settings_accent_clear_request_falls_back_to_default_and_refocuses_as_draft() {
    let app = AppState::new();
    *app.theme_base_accent.borrow_mut() = Some(SmolStr::new_static("#f97316"));
    *app.settings_draft_accent_color.borrow_mut() = Some(SmolStr::new_static("#abcdef"));
    app.settings_focused_field
        .set(SettingsTextField::AccentColor);
    app.settings_dirty.set(false);

    app.request_settings_accent_clear();

    assert!(app.settings_accent_clear_requested.get());
    assert!(app.settings_draft_accent_color.borrow().is_none());
    assert_eq!(app.settings_focused_field.get(), SettingsTextField::None);
    assert!(app.settings_dirty.get());
    assert_eq!(app.settings_accent_editor_value().as_str(), "#3b82f6");
    assert_eq!(app.settings_valid_accent_draft(), None);

    app.focus_settings_accent_color();
    assert!(!app.settings_accent_clear_requested.get());
    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#3b82f6")
    );
    assert_eq!(
        app.settings_focused_field.get(),
        SettingsTextField::AccentColor
    );
}

#[test]
fn settings_accent_picker_result_is_save_gated_draft() {
    let app = AppState::new();
    app.settings_accent_clear_requested.set(true);
    app.settings_focused_field
        .set(SettingsTextField::AccentColor);
    app.settings_dirty.set(false);

    app.set_settings_accent_color_from_picker(SmolStr::new_static("#14b8a6"));

    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#14b8a6")
    );
    assert!(!app.settings_accent_clear_requested.get());
    assert_eq!(app.settings_focused_field.get(), SettingsTextField::None);
    assert!(app.settings_dirty.get());
    assert_eq!(
        app.settings_valid_accent_draft().as_deref(),
        Some("#14b8a6")
    );
}

#[test]
fn settings_accent_editor_accepts_only_partial_hex_draft() {
    let app = AppState::new();
    app.settings_focused_field
        .set(SettingsTextField::AccentColor);
    *app.settings_draft_accent_color.borrow_mut() = None;

    assert!(app.settings_focused_push_char('A'));
    assert!(app.settings_focused_push_char('b'));
    assert!(app.settings_focused_push_char('C'));
    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#abc")
    );
    assert!(!app.settings_focused_push_char('g'));
    assert!(!app.settings_focused_push_char('#'));
    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#abc")
    );
    assert!(app.settings_focused_push_char('d'));
    assert!(app.settings_focused_push_char('E'));
    assert!(app.settings_focused_push_char('f'));
    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#abcdef")
    );
    assert!(!app.settings_focused_push_char('0'));
    assert_eq!(
        app.settings_valid_accent_draft().as_deref(),
        Some("#abcdef")
    );
}

#[test]
fn settings_accent_editor_backspace_caret_and_invalid_save_filter() {
    let app = AppState::new();
    app.settings_focused_field
        .set(SettingsTextField::AccentColor);
    *app.settings_draft_accent_color.borrow_mut() = Some(SmolStr::new_static("#ab"));

    assert_eq!(app.settings_focused_caret(), 3);
    assert_eq!(app.settings_valid_accent_draft(), None);
    assert!(app.settings_focused_backspace());
    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#a")
    );
    assert!(app.settings_focused_backspace());
    assert_eq!(
        app.settings_draft_accent_color.borrow().as_deref(),
        Some("#")
    );
    assert_eq!(app.settings_valid_accent_draft(), None);
}

#[test]
fn active_theme_exposes_non_palette_tokens_for_renderer() {
    let app = AppState::new();
    let mut tokens = ThemeTokens {
        palette: palette::DARK,
        spacing: spacing::DEFAULT,
        radius: radius::DEFAULT,
        shadow: shadow::DEFAULT,
        typo: typo::TypoTokens {
            font_family: SmolStr::new_static("Segoe UI"),
            sizes: typo::FontSizes {
                xs: 10.0,
                sm: 12.0,
                md: 14.0,
                lg: 18.0,
                xl: 22.0,
                xxl: 28.0,
            },
            weights: typo::FontWeights {
                normal: 400,
                medium: 500,
                bold: 700,
            },
            line_heights: typo::LineHeights {
                tight: 1.1,
                normal: 1.4,
                loose: 1.7,
            },
        },
    };
    tokens.radius.md = BorderRadius::all(9.0);
    tokens.radius.xl = BorderRadius::all(18.0);
    tokens.spacing.md = 11.0;
    tokens.shadow.md.offset_y = 5.0;
    tokens.shadow.md.blur = 14.0;
    tokens.shadow.md.color = Color::from_u8(0x10, 0x11, 0x12, 0x80);
    tokens.typo.font_family = SmolStr::new_static("Segoe UI Variable");
    tokens.typo.sizes.md = 15.0;

    assert!(app.apply_active_theme(
        SmolStr::new_static("test-token-theme"),
        SmolStr::new_static("Test Token Theme"),
        tokens,
    ));

    assert_eq!(app.active_theme_radius().md, BorderRadius::all(9.0));
    assert_eq!(app.active_theme_radius().xl, BorderRadius::all(18.0));
    assert_eq!(app.active_theme_spacing().md, 11.0);
    assert_eq!(app.active_theme_shadow().md.offset_y, 5.0);
    assert_eq!(app.active_theme_shadow().md.blur, 14.0);
    assert_eq!(
        app.active_theme_shadow().md.color,
        Color::from_u8(0x10, 0x11, 0x12, 0x80)
    );
    assert_eq!(
        app.active_theme_typography().font_family.as_str(),
        "Segoe UI Variable"
    );
    assert_eq!(app.active_theme_typography().sizes.md, 15.0);
}

#[test]
fn fresh_appstate_active_theme_tauri_is_dark_default() {
    // Boot default must be byte-identical to PALETTE_DARK.
    let app = AppState::new();
    assert_eq!(
        app.active_theme_tauri(),
        bento_nano_style::tokens::PALETTE_DARK,
    );
}

#[test]
fn apply_dark_by_id_yields_exact_palette_dark() {
    let app = AppState::new();
    // Move off dark first so the apply is observable as a change.
    assert_eq!(app.apply_active_theme_by_id("ocean-blue"), Some(true));
    assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
    assert_eq!(
        app.active_theme_tauri(),
        bento_nano_style::tokens::PALETTE_DARK,
    );
    assert_eq!(app.active_theme_id.borrow().as_str(), "dark");
}

#[test]
fn apply_ocean_blue_by_id_yields_exact_palette_ocean_blue() {
    let app = AppState::new();
    assert_eq!(app.apply_active_theme_by_id("ocean-blue"), Some(true));
    assert_eq!(
        app.active_theme_tauri(),
        bento_nano_style::tokens::PALETTE_OCEAN_BLUE,
    );
    assert_eq!(app.active_theme_id.borrow().as_str(), "ocean-blue");
    // ocean-blue has no authored ThemeTokens — falls back to the dark
    // default by polarity (documented partial; widgets only).
    assert_eq!(
        app.active_theme_palette().bg,
        bento_nano_theme::DARK_DEFAULT.palette.bg,
    );
}

#[test]
fn apply_light_by_id_yields_exact_palette_light_and_polarity() {
    let app = AppState::new();
    assert_eq!(app.apply_active_theme_by_id("light"), Some(true));
    let pal = app.active_theme_tauri();
    assert_eq!(pal, bento_nano_style::tokens::PALETTE_LIGHT);
    assert!(!pal.is_dark);
    // light HAS an authored ThemeTokens (registry) — uses LIGHT_DEFAULT.
    assert_eq!(
        app.active_theme_palette().bg,
        bento_nano_theme::LIGHT_DEFAULT.palette.bg,
    );
}

#[test]
fn theme_transition_progress_and_ease_match_v21_n2_contract() {
    assert_eq!(THEME_TRANSITION_MS, 150);
    assert!((theme_transition_progress(1_000, 1_000) - 0.0).abs() < f32::EPSILON);
    assert!((theme_transition_progress(1_000, 1_075) - 0.5).abs() < f32::EPSILON);
    assert!((theme_transition_progress(1_000, 1_150) - 1.0).abs() < f32::EPSILON);
    assert!((theme_transition_ease(0.25) - 0.378_138).abs() < 0.001);
    assert!((theme_transition_ease(0.5) - 0.684_643).abs() < 0.001);
    assert!((theme_transition_ease(0.75) - 0.906_535).abs() < 0.001);
    assert!(theme_transition_ease(0.5) < 0.875);
}

#[test]
fn settings_open_animation_matches_fast_auxiliary_scale_in_contract() {
    assert_eq!(SETTINGS_OPEN_ANIMATION_MS, 160);
    assert!((SETTINGS_OPEN_SCALE_FROM - 0.96).abs() < f32::EPSILON);
    assert!((settings_open_animation_progress(2_000, 2_000) - 0.0).abs() < f32::EPSILON);
    assert!((settings_open_animation_progress(2_000, 2_080) - 0.5).abs() < f32::EPSILON);
    assert!((settings_open_animation_progress(2_000, 2_160) - 1.0).abs() < f32::EPSILON);

    let mid_ease = settings_open_animation_ease(0.5);
    assert!((mid_ease - 0.684_643).abs() < 0.001);
    assert!((settings_open_animation_scale(0.0) - SETTINGS_OPEN_SCALE_FROM).abs() < f32::EPSILON);
    assert!((settings_open_animation_scale(mid_ease) - 0.987_386).abs() < 0.001);
    assert!((settings_open_animation_scale(1.0) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn settings_open_animation_pump_only_while_open_and_unsettled() {
    let app = AppState::new();
    let started = 6_000;

    app.start_settings_open_animation(started);
    assert!(!app.settings_open_animation_pending_at(started));

    app.settings_open.set(true);
    assert!(app.settings_open_animation_pending_at(started));
    assert!(app.settings_open_animation_pending_at(started + SETTINGS_OPEN_ANIMATION_MS - 1));
    assert!(!app.settings_open_animation_pending_at(started + SETTINGS_OPEN_ANIMATION_MS));
}

#[test]
fn live_theme_transition_switches_palette_immediately_and_animates_cards() {
    let app = AppState::new();
    let started = 4_000;
    app.settings_open.set(true);
    let from_card = app.active_theme_card_id();
    assert_eq!(from_card, Some(0));

    assert_eq!(app.apply_active_theme_by_id("light"), Some(true));
    assert_eq!(app.active_theme_card_id(), Some(1));
    assert_eq!(
        app.active_theme_tauri(),
        bento_nano_style::tokens::PALETTE_LIGHT
    );
    assert_eq!(app.active_theme_palette(), LIGHT_DEFAULT.palette);
    assert!(app.start_theme_transition_from(from_card, started));
    assert!(app.theme_transition_pending_at(started));

    assert_eq!(app.theme_card_selection_progress_at(0, false, started), 1.0);
    assert_eq!(app.theme_card_selection_progress_at(1, true, started), 0.0);

    let mid_ms = started + THEME_TRANSITION_MS / 2;
    let old_mid = app.theme_card_selection_progress_at(0, false, mid_ms);
    let new_mid = app.theme_card_selection_progress_at(1, true, mid_ms);
    assert!(old_mid > 0.0 && old_mid < 1.0);
    assert!(new_mid > 0.0 && new_mid < 1.0);
    assert!((old_mid + new_mid - 1.0).abs() < 0.001);

    let settled_ms = started + THEME_TRANSITION_MS;
    assert_eq!(
        app.theme_card_selection_progress_at(0, false, settled_ms),
        0.0
    );
    assert_eq!(
        app.theme_card_selection_progress_at(1, true, settled_ms),
        1.0
    );
    assert!(!app.theme_transition_pending_at(settled_ms));
    assert_eq!(app.theme_transition_from_card.get(), None);

    app.settings_open.set(false);
    assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
    assert!(!app.start_theme_transition_from(Some(1), settled_ms + 1));
}

#[test]
fn apply_all_17_builtin_ids_resolves_exact_const() {
    let app = AppState::new();
    for id in [
        "dark",
        "light",
        "midnight",
        "forest",
        "sunset",
        "frosted",
        "ocean-blue",
        "rose-gold",
        "forest-green",
        "solid",
        "order",
        "flat",
        "brutalism",
        "editorial",
        "neo",
        "terminal",
        "cyberpunk",
    ] {
        assert!(app.apply_active_theme_by_id(id).is_some(), "{id} applied");
        assert_eq!(
            Some(app.active_theme_tauri()),
            bento_nano_style::tokens::palette_tauri_for_theme(id),
            "{id} active_theme_tauri must equal its authored const",
        );
    }
}

#[test]
fn m6b_apply_repopulates_per_theme_radius_shadow_typography() {
    // M6b — the choke-point repopulate fills the three new RefCells, and
    // the accessors return the per-theme const for all 17 builtins.
    let app = AppState::new();
    for id in [
        "dark",
        "light",
        "midnight",
        "forest",
        "sunset",
        "frosted",
        "ocean-blue",
        "rose-gold",
        "forest-green",
        "solid",
        "order",
        "flat",
        "brutalism",
        "editorial",
        "neo",
        "terminal",
        "cyberpunk",
    ] {
        assert!(app.apply_active_theme_by_id(id).is_some(), "{id} applied");
        assert_eq!(
            Some(app.active_theme_radius_tauri()),
            bento_nano_style::tokens::radius_tauri_for_theme(id),
            "{id} active_theme_radius_tauri must equal its authored const",
        );
        assert_eq!(
            Some(app.active_theme_shadow_tauri()),
            bento_nano_style::tokens::shadow_tauri_for_theme(id),
            "{id} active_theme_shadow_tauri must equal its authored const",
        );
        assert_eq!(
            Some(app.active_theme_typography_tauri()),
            bento_nano_style::tokens::typography_tauri_for_theme(id),
            "{id} active_theme_typography_tauri must equal its authored const",
        );
    }
}

#[test]
fn m6c_apply_repopulates_per_theme_effect() {
    // M6c — the choke-point repopulate fills the new effect RefCell, and
    // the accessor returns the per-theme const for all 17 builtins (3 set
    // an effect; 14 resolve to `None`).
    use bento_nano_style::tokens::EffectTauri;
    let app = AppState::new();
    for id in [
        "dark",
        "light",
        "midnight",
        "forest",
        "sunset",
        "frosted",
        "ocean-blue",
        "rose-gold",
        "forest-green",
        "solid",
        "order",
        "flat",
        "brutalism",
        "editorial",
        "neo",
        "terminal",
        "cyberpunk",
    ] {
        assert!(app.apply_active_theme_by_id(id).is_some(), "{id} applied");
        assert_eq!(
            Some(app.active_theme_effect_tauri()),
            bento_nano_style::tokens::effect_tauri_for_theme(id),
            "{id} active_theme_effect_tauri must equal its authored const",
        );
    }
    // The 3 effect themes resolve to their distinct variants.
    assert_eq!(app.apply_active_theme_by_id("terminal"), Some(true));
    assert!(matches!(
        app.active_theme_effect_tauri(),
        EffectTauri::Scanlines(_)
    ));
    assert_eq!(app.apply_active_theme_by_id("cyberpunk"), Some(true));
    assert!(matches!(
        app.active_theme_effect_tauri(),
        EffectTauri::Neon(_)
    ));
    assert_eq!(app.apply_active_theme_by_id("editorial"), Some(true));
    assert!(matches!(
        app.active_theme_effect_tauri(),
        EffectTauri::Chromatic(_)
    ));
    // A non-effect theme clears it back to `None`.
    assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
    assert_eq!(app.active_theme_effect_tauri(), EffectTauri::None);
}
