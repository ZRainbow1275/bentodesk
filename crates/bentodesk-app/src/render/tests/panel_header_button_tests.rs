use super::{
    AuxiliaryActionEmphasis, auxiliary_action_chrome, expanded_panel_aux_chrome, lerp_color,
    panel_header_button_chrome, settings_encryption_mode_button_fill_color,
    settings_theme_card_chrome, with_alpha,
};
use crate::PanelHeaderButtonKind;
use bentodesk_style::tokens::{PALETTE_DARK, PALETTE_LIGHT};

#[test]
fn panel_header_button_chrome_matches_tauri_hover_tokens() {
    let idle = panel_header_button_chrome(PALETTE_DARK, PanelHeaderButtonKind::Search, false);
    assert_eq!(idle.background, None);
    assert_eq!(idle.glyph, PALETTE_DARK.text_muted);

    let search = panel_header_button_chrome(PALETTE_DARK, PanelHeaderButtonKind::Search, true);
    assert_eq!(search.background, Some(PALETTE_DARK.surface_hover));
    assert_eq!(search.glyph, PALETTE_DARK.text_primary);

    let close = panel_header_button_chrome(PALETTE_DARK, PanelHeaderButtonKind::Close, true);
    assert_eq!(
        close.background,
        Some(with_alpha(PALETTE_DARK.accent_red, 0.20))
    );
    assert_eq!(close.glyph, PALETTE_DARK.accent_red);
}

#[test]
fn auxiliary_action_chrome_has_distinct_primary_danger_and_disabled_hierarchy() {
    let primary = auxiliary_action_chrome(PALETTE_DARK, AuxiliaryActionEmphasis::Primary);
    let secondary = auxiliary_action_chrome(PALETTE_DARK, AuxiliaryActionEmphasis::Secondary);
    let danger = auxiliary_action_chrome(PALETTE_DARK, AuxiliaryActionEmphasis::Danger);
    let disabled = auxiliary_action_chrome(PALETTE_DARK, AuxiliaryActionEmphasis::Disabled);

    assert_eq!(primary.fill, with_alpha(PALETTE_DARK.accent_blue, 0.88));
    assert_eq!(primary.border, PALETTE_DARK.accent_blue);
    assert_eq!(primary.text, PALETTE_DARK.control_palette().on_accent);
    assert_eq!(secondary.text, PALETTE_DARK.text_primary);
    assert_eq!(danger.text, PALETTE_DARK.accent_red);
    assert_eq!(disabled.text, PALETTE_DARK.control_palette().disabled_text);
    assert_ne!(primary.fill, secondary.fill);
    assert_ne!(danger.fill, secondary.fill);

    let light_primary = auxiliary_action_chrome(PALETTE_LIGHT, AuxiliaryActionEmphasis::Primary);
    let light_disabled = auxiliary_action_chrome(PALETTE_LIGHT, AuxiliaryActionEmphasis::Disabled);
    assert_eq!(
        light_primary.text,
        PALETTE_LIGHT.control_palette().on_accent
    );
    assert_ne!(light_primary.fill, light_disabled.fill);
}

#[test]
fn expanded_panel_aux_chrome_uses_live_folder_theme_tokens() {
    let dark = expanded_panel_aux_chrome(PALETTE_DARK);
    assert_eq!(
        dark.live_folder_fill,
        with_alpha(PALETTE_DARK.text_primary, 0.08)
    );
    assert_eq!(dark.live_folder_text, PALETTE_DARK.text_muted);

    let light = expanded_panel_aux_chrome(PALETTE_LIGHT);
    assert_eq!(
        light.live_folder_fill,
        with_alpha(PALETTE_LIGHT.text_primary, 0.08)
    );
    assert_eq!(light.live_folder_text, PALETTE_LIGHT.text_muted);
    assert_ne!(dark.live_folder_text, light.live_folder_text);
}

#[test]
fn settings_theme_card_chrome_matches_tauri_hover_tokens() {
    let idle = settings_theme_card_chrome(PALETTE_DARK, 0.0, false);
    assert_eq!(idle.fill, PALETTE_DARK.control_palette().fill);
    assert_eq!(idle.border, None);

    let hover = settings_theme_card_chrome(PALETTE_DARK, 0.0, true);
    assert_eq!(hover.fill, PALETTE_DARK.control_palette().hover_fill);
    assert_eq!(hover.border, Some(PALETTE_DARK.control_palette().border));

    let active = settings_theme_card_chrome(PALETTE_DARK, 1.0, false);
    assert_eq!(active.fill, with_alpha(PALETTE_DARK.accent_blue, 0.10));
    assert_eq!(active.border, Some(PALETTE_DARK.accent_blue));

    let mid = settings_theme_card_chrome(PALETTE_DARK, 0.5, false);
    assert_eq!(mid.fill, lerp_color(idle.fill, active.fill, 0.5));
    assert_eq!(mid.border, Some(with_alpha(PALETTE_DARK.accent_blue, 0.5)));

    let active_hover = settings_theme_card_chrome(PALETTE_DARK, 1.0, true);
    assert_eq!(
        active_hover.fill,
        with_alpha(PALETTE_DARK.accent_blue, 0.14)
    );
    assert_eq!(active_hover.border, Some(PALETTE_DARK.accent_blue));

    let light_idle = settings_theme_card_chrome(PALETTE_LIGHT, 0.0, false);
    assert_eq!(light_idle.fill, PALETTE_LIGHT.control_palette().fill);
    assert_ne!(light_idle.fill, idle.fill);
}

#[test]
fn settings_encryption_mode_button_fill_matches_tauri_hover_priority() {
    let base = with_alpha(bentodesk_style::Color::WHITE, 0.04);
    let accent = bentodesk_style::Color::from_u8(0x60, 0xA5, 0xFA, 0xFF);
    let hover = with_alpha(accent, 0.12);
    let active = with_alpha(accent, 0.18);

    assert_eq!(
        settings_encryption_mode_button_fill_color(false, false, base, hover, active),
        base
    );
    assert_eq!(
        settings_encryption_mode_button_fill_color(false, true, base, hover, active),
        hover
    );
    assert_eq!(
        settings_encryption_mode_button_fill_color(true, false, base, hover, active),
        active
    );
    assert_eq!(
        settings_encryption_mode_button_fill_color(true, true, base, hover, active),
        active
    );
}
