use super::*;
use bento_nano_backend::grouping::SuggestedGroup;
use bento_nano_backend::layout::{AutoGroupRule, GroupRuleType};
use bento_nano_layout::LayoutSource;

fn make_suggestion(name: &str, files: usize, confidence: f64) -> SuggestedGroup {
    SuggestedGroup {
        name: name.to_string(),
        icon: "folder".to_string(),
        rule: AutoGroupRule {
            rule_type: GroupRuleType::Extension,
            pattern: None,
            extensions: Some(vec!["pdf".to_string()]),
        },
        matching_files: (0..files)
            .map(|i| format!("C:/Desktop/file{i}.pdf"))
            .collect(),
        confidence,
    }
}

#[test]
fn confidence_tone_high_threshold() {
    assert_eq!(confidence_tone(0.85), ConfidenceTone::High);
    assert_eq!(confidence_tone(0.80), ConfidenceTone::High);
}

#[test]
fn confidence_tone_medium_threshold() {
    assert_eq!(confidence_tone(0.79), ConfidenceTone::Medium);
    assert_eq!(confidence_tone(0.50), ConfidenceTone::Medium);
}

#[test]
fn confidence_tone_low_threshold() {
    assert_eq!(confidence_tone(0.49), ConfidenceTone::Low);
    assert_eq!(confidence_tone(0.0), ConfidenceTone::Low);
}

#[test]
fn tone_colors_uses_palette_tokens() {
    let palette = theme::current().palette;
    let (high_bg, high_fg) = tone_colors(ConfidenceTone::High);
    assert_eq!(high_fg, palette.success);
    // Background is the same RGB but with the snap.md alpha applied.
    assert!((high_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);
    assert_eq!(high_bg.r, palette.success.r);

    let (med_bg, med_fg) = tone_colors(ConfidenceTone::Medium);
    assert_eq!(med_fg, palette.warning);
    assert!((med_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);

    let (low_bg, low_fg) = tone_colors(ConfidenceTone::Low);
    assert_eq!(low_fg, palette.text_muted);
    assert!((low_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);
}

#[test]
fn suggestor_chrome_accepts_explicit_active_palette() {
    let mut palette = theme::current().palette;
    palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
    palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
    palette.selection = Color::from_u8(0x44, 0xAA, 0xEE, 0x66);
    palette.accent = Color::from_u8(0x12, 0x34, 0x56, 0x78);
    palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);
    palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
    palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);

    let chrome = SmartGroupSuggestorChrome::from_palette(palette);

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
        Color::from_u8(0x12, 0x34, 0x56, 0x78)
    );
    assert_eq!(
        chrome.danger_background,
        Color::from_u8(0xCC, 0x44, 0x44, 0xFF)
    );
    assert_eq!(
        chrome.preview_background,
        Color::from_u8(0x11, 0x22, 0x33, 0xEE)
    );
    assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
}

#[test]
fn suggestor_chrome_accepts_explicit_radius_shadow_tokens() {
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

    let chrome = SmartGroupSuggestorChrome::from_tokens(palette, radius, shadow);

    assert_eq!(chrome.panel_shadow, shadow.md);
    assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
    assert_eq!(chrome.row_radius, BorderRadius::all(11.0));
    assert_eq!(chrome.badge_radius, BorderRadius::all(7.0));
    assert_eq!(chrome.action_radius, BorderRadius::all(11.0));
    assert_eq!(chrome.close_radius, BorderRadius::all(11.0));
    assert_eq!(chrome.preview_radius, BorderRadius::all(11.0));
    assert_eq!(chrome.preview_button_radius, BorderRadius::all(7.0));
}

#[test]
fn runtime_panel_owns_the_full_aux_client_without_a_translucent_host_frame() {
    let viewport = Size {
        width: 522.0,
        height: 574.0,
    };
    assert_eq!(
        suggestor_panel_rect(viewport),
        Rect {
            x: 0.0,
            y: 0.0,
            width: 522.0,
            height: 574.0,
        }
    );
    let last_row = suggestor_row_rect(viewport, MAX_VISIBLE_SUGGESTIONS - 1);
    let preview = suggestor_preview_rect(viewport);
    assert!(last_row.bottom() < preview.y);
    assert!(preview.bottom() <= viewport.height);
}

#[test]
fn suggestor_panel_shadow_rect_uses_token_shadow_geometry() {
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

    let rect = suggestor_panel_shadow_rect(panel, shadow);

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
fn tone_colors_accept_explicit_active_palette() {
    let mut palette = theme::current().palette;
    palette.success = Color::from_u8(0x11, 0xAA, 0x22, 0xFF);
    palette.warning = Color::from_u8(0xCC, 0x88, 0x11, 0xFF);
    palette.text_muted = Color::from_u8(0x77, 0x88, 0x99, 0xFF);

    let (high_bg, high_fg) = tone_colors_from_palette(ConfidenceTone::High, palette);
    assert_eq!(high_fg, Color::from_u8(0x11, 0xAA, 0x22, 0xFF));
    assert_eq!(high_bg.r, high_fg.r);
    assert!((high_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);

    let (_, med_fg) = tone_colors_from_palette(ConfidenceTone::Medium, palette);
    assert_eq!(med_fg, Color::from_u8(0xCC, 0x88, 0x11, 0xFF));

    let (_, low_fg) = tone_colors_from_palette(ConfidenceTone::Low, palette);
    assert_eq!(low_fg, Color::from_u8(0x77, 0x88, 0x99, 0xFF));
}

#[test]
fn snap_geometry_constants_pinned() {
    assert_eq!(PANEL_WIDTH_PX, 480.0);
    assert_eq!(PANEL_PADDING_PX, 24.0);
    assert_eq!(PANEL_CORNER_RADIUS_PX, 16.0);
    assert_eq!(ROW_GAP_PX, 8.0);
    assert_eq!(ROW_PADDING_X_PX, 16.0);
    assert_eq!(ROW_PADDING_Y_PX, 12.0);
    assert_eq!(ROW_ICON_SIZE_PX, 28.0);
    assert_eq!(MAX_VISIBLE_SUGGESTIONS, 5);
    assert_eq!(MAX_VISIBLE_PREVIEW_FILES, 2);
    assert!((CONFIDENCE_HIGH_THRESHOLD - 0.80).abs() < f64::EPSILON);
    assert!((CONFIDENCE_MEDIUM_THRESHOLD - 0.50).abs() < f64::EPSILON);
    assert!((BADGE_BG_ALPHA - 0.20).abs() < f32::EPSILON);
}

#[test]
fn runtime_hit_test_distinguishes_row_apply_dismiss_and_close() {
    let viewport = bento_nano_style::Size {
        width: 480.0,
        height: 360.0,
    };
    let row = suggestor_row_rect(viewport, 0);
    assert_eq!(
        suggestor_hit_test(viewport, 2, 2, row.x + 3.0, row.y + 3.0),
        Some(SuggestorPointerHit::Row(0))
    );
    let apply = suggestor_apply_rect(viewport, 1);
    assert_eq!(
        suggestor_hit_test(viewport, 2, 2, apply.x + 2.0, apply.y + 2.0),
        Some(SuggestorPointerHit::Apply(1))
    );
    let dismiss = suggestor_dismiss_rect(viewport, 1);
    assert_eq!(
        suggestor_hit_test(viewport, 2, 2, dismiss.x + 2.0, dismiss.y + 2.0),
        Some(SuggestorPointerHit::Dismiss(1))
    );
    let close = suggestor_close_rect(viewport);
    assert_eq!(
        suggestor_hit_test(viewport, 2, 2, close.x + 2.0, close.y + 2.0),
        Some(SuggestorPointerHit::Close)
    );
}

#[test]
fn runtime_hit_test_distinguishes_manual_preview_targets() {
    let viewport = bento_nano_style::Size {
        width: 640.0,
        height: 560.0,
    };
    let all = suggestor_select_all_rect(viewport);
    assert_eq!(
        suggestor_hit_test(viewport, 2, 2, all.x + 2.0, all.y + 2.0),
        Some(SuggestorPointerHit::SelectAllFiles)
    );
    let none = suggestor_select_none_rect(viewport);
    assert_eq!(
        suggestor_hit_test(viewport, 2, 2, none.x + 2.0, none.y + 2.0),
        Some(SuggestorPointerHit::SelectNoFiles)
    );
    let file = suggestor_preview_file_rect(viewport, 1);
    assert_eq!(
        suggestor_hit_test(viewport, 2, 2, file.x + 2.0, file.y + 2.0),
        Some(SuggestorPointerHit::TogglePreviewFile(1))
    );
}

#[test]
fn build_returns_panel_sized_container() {
    let node = build();
    let layout = node.layout();
    assert!(matches!(layout.width, Length::Px(w) if (w - PANEL_WIDTH_PX).abs() < 0.01));
    assert_eq!(layout.direction, Direction::Column);
    assert!((layout.padding.top - PANEL_PADDING_PX).abs() < 0.01);
    assert!((layout.padding.left - PANEL_PADDING_PX).abs() < 0.01);
}

#[test]
fn set_suggestions_truncates_to_visible_cap() {
    let mut state = SuggestorState::new();
    let many = (0..(MAX_VISIBLE_SUGGESTIONS + 3))
        .map(|i| make_suggestion(&format!("g{i}"), 4, 0.6))
        .collect::<Vec<_>>();
    state.set_suggestions(many);
    assert_eq!(state.entries().len(), MAX_VISIBLE_SUGGESTIONS);
}

#[test]
fn set_suggestions_resets_transient_state() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
    let id = state.entries()[0].id.clone();
    state.on_row_hover(id.clone());
    state.mark_applying(id);
    // Replacing the list invalidates old ids → both get cleared.
    state.set_suggestions(vec![make_suggestion("images", 4, 0.7)]);
    assert!(state.hovered_id().is_none());
    assert!(state.applying_id().is_none());
}

#[test]
fn keyboard_selection_and_remove_entry_clamp_cursor() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![
        make_suggestion("docs", 4, 0.6),
        make_suggestion("images", 4, 0.7),
    ]);
    state.select_next();
    assert_eq!(state.selected_index(), 1);
    let removed = state.entries()[1].id.clone();
    assert!(state.remove_entry(removed.as_str()));
    assert_eq!(state.selected_index(), 0);
    assert_eq!(state.visible_count(), 1);
}

#[test]
fn entry_id_is_stable_for_same_payload() {
    let s = make_suggestion("docs", 4, 0.6);
    let a = SuggestionEntry::from_suggestion(s.clone());
    let b = SuggestionEntry::from_suggestion(s);
    assert_eq!(a.id, b.id);
}

#[test]
fn on_row_hover_records_id_and_lookup_finds_entry() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![make_suggestion("docs", 3, 0.7)]);
    let id = state.entries()[0].id.clone();
    state.on_row_hover(id.clone());
    assert_eq!(state.hovered_id(), Some(&id));
    assert!(state.hovered_entry().is_some());
    state.on_row_leave();
    assert!(state.hovered_id().is_none());
    assert!(state.hovered_entry().is_none());
}

#[test]
fn apply_records_action_with_suggestion_payload() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
    let id = state.entries()[0].id.clone();
    assert!(state.apply(id.as_str()));
    assert!(state.has_pending_action());
    let action = state.take_action().expect("action recorded");
    match action {
        SuggestorAction::Apply {
            suggestion_id,
            suggestion,
        } => {
            assert_eq!(suggestion_id.as_str(), "docs:4");
            assert_eq!(suggestion.name, "docs");
            assert_eq!(suggestion.matching_files.len(), 4);
        }
        other => panic!("expected Apply, got {other:?}"),
    }
    // One-shot.
    assert!(state.take_action().is_none());
}

#[test]
fn apply_with_stale_id_records_nothing() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
    assert!(!state.apply("does:not:exist"));
    assert!(!state.has_pending_action());
}

#[test]
fn manual_selection_filters_apply_payload() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);

    assert!(state.toggle_preview_file(0));
    assert!(state.apply_selected());

    match state.take_action() {
        Some(SuggestorAction::Apply { suggestion, .. }) => {
            assert_eq!(suggestion.matching_files.len(), 3);
            assert!(
                !suggestion
                    .matching_files
                    .iter()
                    .any(|path| path.ends_with("file0.pdf"))
            );
        }
        other => panic!("expected filtered apply action, got {other:?}"),
    }
}

#[test]
fn manual_selection_blocks_empty_apply() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);

    assert!(state.select_none_for_selected());
    assert!(!state.apply_selected());
    assert!(!state.has_pending_action());
    assert!(state.select_all_for_selected());
    assert!(state.apply_selected());
}

#[test]
fn dismiss_records_action_with_suggestion_id() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
    let id = state.entries()[0].id.clone();
    assert!(state.dismiss(id.as_str()));
    assert_eq!(
        state.take_action(),
        Some(SuggestorAction::Dismiss { suggestion_id: id }),
    );
}

#[test]
fn selected_apply_and_dismiss_use_cursor_row() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![
        make_suggestion("docs", 4, 0.6),
        make_suggestion("images", 4, 0.7),
    ]);
    state.select_next();
    assert!(state.apply_selected());
    match state.take_action() {
        Some(SuggestorAction::Apply { suggestion, .. }) => {
            assert_eq!(suggestion.name, "images");
        }
        other => panic!("expected selected apply action, got {other:?}"),
    }
    assert!(state.dismiss_selected());
    assert_eq!(
        state.take_action(),
        Some(SuggestorAction::Dismiss {
            suggestion_id: SmolStr::new("images:4")
        })
    );
}

#[test]
fn close_records_close_action() {
    let mut state = SuggestorState::new();
    state.close();
    assert_eq!(state.take_action(), Some(SuggestorAction::Close));
}

#[test]
fn take_action_clears_pending_flag() {
    let mut state = SuggestorState::new();
    state.close();
    let _ = state.take_action();
    assert!(!state.has_pending_action());
}

#[test]
fn into_command_apply_maps_to_grouping_apply() {
    let s = make_suggestion("docs", 4, 0.6);
    let action = SuggestorAction::Apply {
        suggestion_id: SmolStr::new("docs:4"),
        suggestion: Box::new(s.clone()),
    };
    match action.into_command() {
        Some(Command::GroupingApply { suggestion }) => {
            assert_eq!(suggestion.name, s.name);
        }
        other => panic!("expected GroupingApply, got {other:?}"),
    }
}

#[test]
fn into_command_dismiss_maps_to_suggestor_dismiss() {
    let action = SuggestorAction::Dismiss {
        suggestion_id: SmolStr::new("docs:4"),
    };
    match action.into_command() {
        Some(Command::SuggestorDismiss { suggestion_id }) => {
            assert_eq!(suggestion_id.as_str(), "docs:4");
        }
        other => panic!("expected SuggestorDismiss, got {other:?}"),
    }
}

#[test]
fn into_command_close_yields_none() {
    assert!(SuggestorAction::Close.into_command().is_none());
}

#[test]
fn applying_marker_round_trip() {
    let mut state = SuggestorState::new();
    state.set_suggestions(vec![make_suggestion("docs", 4, 0.6)]);
    let id = state.entries()[0].id.clone();
    state.mark_applying(id.clone());
    assert_eq!(state.applying_id(), Some(&id));
    state.clear_applying();
    assert!(state.applying_id().is_none());
}

/// ΔB lock: the action enum round-trips through serde, mirroring
/// every other dispatcher payload.
#[test]
fn suggestor_action_serde_round_trip() {
    let action = SuggestorAction::Dismiss {
        suggestion_id: SmolStr::new("docs:4"),
    };
    let s = serde_json::to_string(&action).unwrap_or_default();
    let back: SuggestorAction = serde_json::from_str(&s).unwrap_or(SuggestorAction::Close);
    assert_eq!(back, action);
}

#[test]
fn suggestor_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
    use bento_nano_style::tokens as style_tokens;
    let chrome = SmartGroupSuggestorChrome::from_tauri_tokens(
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
    assert_eq!(
        chrome.danger_background,
        style_tokens::PALETTE_DARK.accent_red
    );
    assert_eq!(
        chrome.preview_background,
        style_tokens::PALETTE_DARK.surface_subtle
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
        chrome.action_radius,
        BorderRadius::all(style_tokens::RADIUS.card)
    );
    // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
    assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
}

#[test]
fn tone_colors_from_tauri_palette_maps_to_accent_green_orange_muted() {
    use bento_nano_style::tokens as style_tokens;
    let (high_bg, high_fg) =
        tone_colors_from_tauri_palette(ConfidenceTone::High, style_tokens::PALETTE_DARK);
    assert_eq!(high_fg, style_tokens::PALETTE_DARK.accent_green);
    assert!((high_bg.a - BADGE_BG_ALPHA).abs() < f32::EPSILON);

    let (_, med_fg) =
        tone_colors_from_tauri_palette(ConfidenceTone::Medium, style_tokens::PALETTE_DARK);
    assert_eq!(med_fg, style_tokens::PALETTE_DARK.accent_orange);

    let (_, low_fg) =
        tone_colors_from_tauri_palette(ConfidenceTone::Low, style_tokens::PALETTE_DARK);
    assert_eq!(low_fg, style_tokens::PALETTE_DARK.text_muted);
}
