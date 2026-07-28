use super::*;

fn vp() -> Size {
    Size {
        width: 800.0,
        height: 600.0,
    }
}

#[test]
fn panel_rect_centres_x_and_top_anchors_y() {
    let v = vp();
    let r = settings_panel_rect(v);
    assert_eq!(r.width, SETTINGS_PANEL_WIDTH);
    assert_eq!(r.height, SETTINGS_PANEL_HEIGHT);
    // (800-360)/2 = 220
    assert!((r.x - 220.0).abs() < 0.01);
    // 600 >= 560 + 16, so y = top margin
    assert!((r.y - SETTINGS_PANEL_TOP_MARGIN).abs() < 0.01);
}

#[test]
fn panel_rect_saturates_to_zero_when_viewport_too_small() {
    let v = Size {
        width: 200.0,
        height: 320.0,
    };
    let r = settings_panel_rect(v);
    assert!(r.x.abs() < 0.01);
    assert!(r.y.abs() < 0.01);
}

#[test]
fn close_button_lives_in_panel_header() {
    let v = vp();
    let p = settings_panel_rect(v);
    let c = settings_close_button_rect(v);
    // Header band — y close to top, x near the right edge of the panel.
    assert!(c.y >= p.y && c.y < p.y + SETTINGS_HEADER_H);
    assert!(c.right() <= p.right());
    assert!(c.x > p.x + p.width * 0.5);
}

#[test]
fn section_rows_stack_vertically_in_order() {
    let v = vp();
    let stealth = settings_stealth_enabled_rect(v);
    let auto = settings_update_auto_download_rect(v);
    let encryption = settings_encryption_mode_rect(v);
    let locale = settings_switch_button_rect(v);
    let zone = settings_zone_display_mode_rect(v);
    let theme = settings_active_theme_rect(v);
    let updater = settings_update_frequency_rect(v);
    let vault = settings_backup_now_rect(v);
    let modals = settings_keybindings_open_rect(v);
    assert!(stealth.y < auto.y);
    assert!(auto.y < encryption.y);
    assert!(encryption.y < locale.y);
    assert!(locale.y < zone.y);
    assert!(zone.y < theme.y);
    assert!(theme.y < updater.y);
    assert!(updater.y < vault.y);
    assert!(vault.y < modals.y);
}

#[test]
fn updater_row_lays_out_inline_actions_left_to_right_of_dropdown() {
    let v = vp();
    let frequency = settings_update_frequency_rect(v);
    let check = settings_update_check_now_rect(v);
    let action = settings_update_action_rect(v);
    let skip = settings_update_skip_rect(v);
    assert_eq!(frequency.y, check.y);
    assert_eq!(check.y, action.y);
    assert_eq!(action.y, skip.y);
    assert!(skip.right() < action.x);
    assert!(action.right() < check.x);
    assert!(check.right() < frequency.x);
}

#[test]
fn theme_row_lays_out_swatch_import_active_left_to_right() {
    let v = vp();
    let swatch = settings_theme_base_rect(v);
    let import = settings_theme_import_rect(v);
    let active = settings_active_theme_rect(v);
    assert_eq!(swatch.y, import.y);
    assert_eq!(import.y, active.y);
    assert!(swatch.right() < import.x);
    assert!(import.right() < active.x);
}

#[test]
fn vault_row_packs_six_chips_left_to_right() {
    let v = vp();
    let chips = [
        settings_backup_now_rect(v),
        settings_backup_list_rect(v),
        settings_backup_restore_rect(v),
        settings_recovery_create_rect(v),
        settings_recovery_diagnostics_rect(v),
        settings_recovery_restore_rect(v),
    ];
    for w in chips.windows(2) {
        assert!(w[0].right() <= w[1].x, "chips overlap: {:?}", w);
        assert_eq!(w[0].y, w[1].y);
        assert_eq!(w[0].height, w[1].height);
    }
}

// M1h (2026-05-29) — removed `modal_openers_paired_with_keybindings_left_plugins_right`:
// the plugins modal-opener button (`settings_plugins_open_rect`) was deleted
// when the Plugins surface moved inline (§11). The keybindings opener keeps
// its own coverage elsewhere.

#[test]
fn backup_entries_sit_below_vault_row() {
    let v = vp();
    let vault = settings_backup_now_rect(v);
    let entry0 = settings_backup_entry_rect(v, 0);
    let entry1 = settings_backup_entry_rect(v, 1);
    let entry2 = settings_backup_entry_rect(v, 2);
    assert!(entry0.y >= vault.bottom());
    assert!(entry0.right() < entry1.x);
    assert!(entry1.right() < entry2.x);
}

#[test]
fn locale_dropdown_chip_sits_in_locale_row_right_half() {
    let v = vp();
    let p = settings_panel_rect(v);
    let chip = settings_switch_button_rect(v);
    assert_eq!(chip.width, SETTINGS_DROPDOWN_CHIP_W);
    assert_eq!(chip.height, SETTINGS_DROPDOWN_CHIP_H);
    assert!(chip.x >= p.x + p.width * 0.5);
    assert!(chip.right() <= p.right());
}

#[test]
fn toggle_rocker_rect_sits_in_row_right_half() {
    let v = vp();
    let p = settings_panel_rect(v);
    let stealth = settings_stealth_enabled_rect(v);
    let auto = settings_update_auto_download_rect(v);
    for r in [stealth, auto] {
        assert!(r.x >= p.x + p.width * 0.5);
        assert!(r.right() <= p.right());
        assert_eq!(r.width, SETTINGS_SWITCH_BTN_W);
        assert_eq!(r.height, SETTINGS_SWITCH_BTN_H);
    }
}

#[test]
fn panel_shadow_rect_uses_shadow_offsets_and_blur() {
    let panel = Rect {
        x: 20.0,
        y: 10.0,
        width: 360.0,
        height: 580.0,
    };
    let shadow = Shadow {
        offset_x: 2.0,
        offset_y: 5.0,
        blur: 14.0,
        spread: 0.0,
        color: bentodesk_style::Color::from_u8(0, 0, 0, 0x80),
    };
    let shadow_rect = settings_panel_shadow_rect(panel, shadow);
    assert_eq!(shadow_rect.x, 8.0);
    assert_eq!(shadow_rect.y, 1.0);
    assert_eq!(shadow_rect.width, 388.0);
    assert_eq!(shadow_rect.height, 608.0);
}

#[test]
fn keybindings_modal_rows_fit_inside_card() {
    let v = vp();
    let modal = settings_keybindings_modal_rect(v);
    let close = settings_keybindings_close_rect(v);
    let row_0 = settings_keybinding_row_rect(v, 0);
    let row_9 = settings_keybinding_row_rect(v, 9);
    let record = settings_keybinding_record_rect(v, 0);
    let reset = settings_keybinding_reset_rect(v, 0);
    assert!(modal.x >= 0.0);
    assert!(modal.y >= 0.0);
    assert!(close.right() <= modal.right());
    assert!(row_0.y > close.y);
    assert!(row_9.bottom() <= modal.bottom());
    assert!(record.right() < reset.x);
    assert!(reset.right() <= row_0.right());
}

// M1h (2026-05-29) — removed `plugins_modal_rows_and_actions_fit_inside_card`:
// the plugin lifecycle modal geometry it covered was deleted when the
// Plugins surface moved inline. The inline §11 card geometry is covered by
// the `m1h_*` tests in the `m1_tests` dark-shell module above.

#[test]
fn section_row_count_matches_visible_rows() {
    // 9 rows: stealth, auto, encryption, locale, zone, theme, updater,
    // vault, modal openers. Backup entry strip lives between vault and
    // modals (not counted as a section row).
    assert_eq!(SETTINGS_SECTION_ROW_COUNT, 9);
}

#[test]
fn settings_typography_roles_are_compact_and_system_fonted() {
    let global = bentodesk_style::tokens::TYPOGRAPHY;
    assert_eq!(global.font_family, "Segoe UI");
    assert!(SETTINGS_TEXT_LABEL_SIZE < global.md.size_px);
    assert!(SETTINGS_TEXT_VALUE_SIZE < global.md.size_px);
    assert_eq!(SETTINGS_TEXT_LABEL_SIZE, 13.0);
    assert_eq!(SETTINGS_TEXT_LABEL_WEIGHT, global.weight_normal);
    assert_eq!(SETTINGS_TEXT_VALUE_SIZE, 12.0);
    assert_eq!(SETTINGS_TEXT_VALUE_WEIGHT, global.weight_medium);
    assert_eq!(SETTINGS_TEXT_LINE_HEIGHT, 1.0);
    assert_eq!(SETTINGS_GROUP_TITLE_SIZE, 10.0);
    assert_eq!(SETTINGS_GROUP_TITLE_WEIGHT, global.weight_semibold);
    assert_eq!(SETTINGS_GROUP_TITLE_TRACKING, 1.2);
}

#[test]
fn final_modal_row_fits_inside_panel_chrome() {
    let v = vp();
    let p = settings_panel_rect(v);
    let modals = settings_section_row_rect(v, ROW_INDEX_MODALS);
    assert!(modals.bottom() <= p.bottom());
}
