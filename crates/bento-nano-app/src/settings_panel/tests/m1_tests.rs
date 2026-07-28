use super::*;

fn vp() -> Size {
    Size {
        width: 800.0,
        height: 600.0,
    }
}

#[test]
fn m1_panel_centred_x_top_anchored_y() {
    let p = settings_panel_rect_m1(vp());
    let expected_x = (vp().width - SETTINGS_PANEL_WIDTH_M1) * 0.5;
    assert!((p.x - expected_x).abs() < 0.01);
    assert!((p.y - SETTINGS_PANEL_TOP_MARGIN).abs() < 0.01);
    assert_eq!(p.width, SETTINGS_PANEL_WIDTH_M1);
    // 600 - 16 * 2 = 568 < max height, so the legacy viewport still clamps.
    assert!((p.height - 568.0).abs() < 0.01);
}

#[test]
fn panel_sized_native_host_is_filled_without_overlay_margin() {
    let viewport = Size {
        width: SETTINGS_PANEL_WIDTH_M1,
        height: 729.3333,
    };
    assert!(settings_panel_fills_host(viewport));
    assert_eq!(
        settings_panel_rect_m1(viewport),
        Rect {
            x: 0.0,
            y: 0.0,
            width: viewport.width,
            height: viewport.height,
        }
    );
    assert_eq!(settings_header_rect(viewport).y, 0.0);
    assert_eq!(settings_footer_rect(viewport).bottom(), viewport.height);
}

#[test]
fn m1_panel_large_overlay_is_tall_and_centered() {
    let viewport = Size {
        width: 1707.0,
        height: 912.0,
    };
    let p = settings_panel_rect_m1(viewport);
    let expected_h = viewport.height * SETTINGS_PANEL_MAX_WORKAREA_FRAC;
    let expected_y = (viewport.height - expected_h) * SETTINGS_PANEL_LARGE_VIEWPORT_Y_FRAC;
    assert!((p.height - expected_h).abs() < 0.01);
    assert!((p.y - expected_y).abs() < 0.01);
    assert_eq!(p.width, SETTINGS_PANEL_WIDTH_M1);
}

#[test]
fn m1_panel_host_margin_controls_height_and_keeps_body_scrollable() {
    let viewport = Size {
        width: 811.0,
        height: 760.0,
    };
    let p = settings_panel_rect_m1(viewport);
    let expected_h = viewport.height * SETTINGS_PANEL_MAX_WORKAREA_FRAC;
    let expected_y = (viewport.height - expected_h) * SETTINGS_PANEL_LARGE_VIEWPORT_Y_FRAC;
    assert!((p.height - expected_h).abs() < 0.01);
    assert!(settings_body_rect(viewport).height > SETTINGS_ROW_H_M1 * 2.0);
    assert!((p.y - expected_y).abs() < 0.01);
    assert_eq!(p.width, SETTINGS_PANEL_WIDTH_M1);
}

#[test]
fn m1_header_sticky_at_top_of_panel() {
    let p = settings_panel_rect_m1(vp());
    let h = settings_header_rect(vp());
    assert_eq!(h.x, p.x);
    assert_eq!(h.y, p.y);
    assert_eq!(h.width, p.width);
    assert_eq!(h.height, SETTINGS_HEADER_H_M1);
}

#[test]
fn m1_header_matches_tauri_settings_panel_css_height() {
    assert_eq!(SETTINGS_HEADER_H_M1, 52.0);
    assert_eq!(SETTINGS_HEADER_H, SETTINGS_HEADER_H_M1);
}

#[test]
fn m1_footer_sticky_at_bottom_of_panel() {
    let p = settings_panel_rect_m1(vp());
    let f = settings_footer_rect(vp());
    assert_eq!(f.x, p.x);
    assert!((f.bottom() - p.bottom()).abs() < 0.01);
    assert_eq!(f.height, SETTINGS_FOOTER_H);
}

#[test]
fn m1_body_sits_between_header_and_footer() {
    let body = settings_body_rect(vp());
    let header = settings_header_rect(vp());
    let footer = settings_footer_rect(vp());
    assert!((body.y - header.bottom()).abs() < 0.01);
    assert!((body.bottom() - footer.y).abs() < 0.01);
}

#[test]
fn m1_close_button_inside_header() {
    let h = settings_header_rect(vp());
    let c = settings_close_button_rect_m1(vp());
    assert!(c.y >= h.y);
    assert!(c.bottom() <= h.bottom());
    assert!(c.right() <= h.right());
    assert_eq!(c.width, SETTINGS_CLOSE_X_SIZE);
    assert_eq!(c.height, SETTINGS_CLOSE_X_SIZE);
}

#[test]
fn m1_close_button_matches_tauri_settings_panel_css_size() {
    assert_eq!(SETTINGS_CLOSE_X_SIZE, 32.0);
}

#[test]
fn m1_footer_buttons_paired_cancel_left_save_right() {
    let cancel = settings_cancel_button_rect(vp());
    let save = settings_save_button_rect(vp());
    assert_eq!(cancel.y, save.y);
    assert_eq!(cancel.width, save.width);
    assert_eq!(save.width, SETTINGS_FOOTER_ACTION_BTN_W);
    assert!(cancel.right() < save.x);
    let f = settings_footer_rect(vp());
    assert!(save.right() <= f.right());
}

#[test]
fn m1_top_toggle_rows_stack_vertically_in_order() {
    let v = vp();
    let r0 = settings_top_toggle_row_rect(v, 0.0, 0);
    let r1 = settings_top_toggle_row_rect(v, 0.0, 1);
    let r4 = settings_top_toggle_row_rect(v, 0.0, 4);
    assert!(r0.y < r1.y);
    assert!(r1.y < r4.y);
    assert!((r1.y - r0.y - SETTINGS_ROW_H_M1).abs() < 0.01);
}

#[test]
fn m1_general_title_and_body_padding_precede_first_toggle() {
    let v = vp();
    let body = settings_body_rect(v);
    let title = settings_general_label_rect(v, 0.0);
    let first = settings_top_toggle_row_rect(v, 0.0, 0);
    assert!((title.y - body.y - 20.0).abs() < 0.01);
    assert!((first.y - body.y - SETTINGS_BODY_TOP_INSET).abs() < 0.01);
    assert!(title.bottom() <= first.y);
}

#[test]
fn m1_top_toggle_hit_rect_sits_in_row_right_half() {
    let v = vp();
    let row = settings_top_toggle_row_rect(v, 0.0, 0);
    let hit = settings_top_toggle_hit_rect(v, 0.0, 0);
    assert!(hit.x >= row.x + row.width * 0.5);
    assert!(hit.right() <= row.right());
    assert_eq!(hit.width, SETTINGS_TOP_TOGGLE_HIT_W);
    assert_eq!(hit.height, SETTINGS_TOP_TOGGLE_HIT_H);
}

#[test]
fn m1_language_row_sits_below_top_5_toggle_rows() {
    let v = vp();
    let last_toggle = settings_top_toggle_row_rect(v, 0.0, 4);
    let lang = settings_language_row_rect(v, 0.0);
    assert!((lang.y - last_toggle.bottom()).abs() < 0.01);
}

#[test]
fn m1_language_chip_inside_language_row_right_half() {
    let v = vp();
    let row = settings_language_row_rect(v, 0.0);
    let chip = settings_language_chip_rect(v, 0.0);
    assert!(chip.x >= row.x + row.width * 0.5);
    assert!(chip.right() <= row.right());
    assert_eq!(chip.width, SETTINGS_LANGUAGE_CHIP_W);
    assert_eq!(chip.height, SETTINGS_LANGUAGE_CHIP_H);
}

#[test]
fn m1_scroll_offset_shifts_rows_up() {
    let v = vp();
    let row0_at_0 = settings_top_toggle_row_rect(v, 0.0, 0);
    let row0_at_50 = settings_top_toggle_row_rect(v, 50.0, 0);
    assert!((row0_at_50.y + 50.0 - row0_at_0.y).abs() < 0.01);
}

/// α4 (Wave I-α) / G3 parity (2026-06-01) — the zone-display-mode picker is
/// now the §4 DisplayMode group (promoted out of the General band), sitting
/// below its own group title which itself sits below the §3 Appearance
/// section. The picker row starts exactly where the §4 group title ends.
#[test]
fn alpha4_zone_display_picker_row_sits_below_display_mode_group_title() {
    let v = vp();
    let title = settings_display_mode_label_rect(v, 0.0);
    let picker = settings_zone_display_mode_picker_row_rect(v, 0.0);
    assert!(
        (picker.y - title.bottom()).abs() < 0.01,
        "picker row must start exactly where the §4 group title ends \
             (title.bottom={}, picker.y={})",
        title.bottom(),
        picker.y,
    );
    assert_eq!(
        picker.height,
        SETTINGS_RADIO_H * SETTINGS_ZONE_DISPLAY_MODE_COUNT as f32
            + SETTINGS_RADIO_GAP * (SETTINGS_ZONE_DISPLAY_MODE_COUNT - 1) as f32
    );
    // §4 DisplayMode sits below §3 Appearance (the appearance accent row),
    // a full section gap clear — the General band no longer contains it.
    let appearance = settings_appearance_label_rect(v, 0.0, &plugin_flags(0));
    assert!(
        title.y > appearance.bottom(),
        "§4 DisplayMode group title (y={}) must sit below §3 Appearance \
             label (bottom={})",
        title.y,
        appearance.bottom(),
    );
    // It is no longer wedged into the General band right under Language.
    let lang = settings_language_row_rect(v, 0.0);
    assert!(
        title.y > lang.bottom() + SETTINGS_SECTION_GAP,
        "§4 DisplayMode must be promoted well below the General band's \
             Language row (title.y={}, lang.bottom={})",
        title.y,
        lang.bottom(),
    );
}

#[test]
fn alpha4_three_radios_stack_top_to_bottom_inside_picker_row() {
    let v = vp();
    let row = settings_zone_display_mode_picker_row_rect(v, 0.0);
    let r0 = settings_zone_display_mode_radio_rect(v, 0.0, 0);
    let r1 = settings_zone_display_mode_radio_rect(v, 0.0, 1);
    let r2 = settings_zone_display_mode_radio_rect(v, 0.0, 2);
    assert_eq!(r0.x, r1.x);
    assert_eq!(r1.x, r2.x);
    assert!((r1.y - r0.bottom() - SETTINGS_RADIO_GAP).abs() < 0.01);
    assert!((r2.y - r1.bottom() - SETTINGS_RADIO_GAP).abs() < 0.01);
    assert_eq!(r0.right(), row.right());
    assert_eq!(r2.bottom(), row.bottom());
    assert_eq!(r0.width, SETTINGS_RADIO_W);
    assert_eq!(r0.height, SETTINGS_RADIO_H);
}

#[test]
fn alpha4_display_mode_copy_stays_left_of_option_stack() {
    let v = vp();
    let label = settings_display_mode_copy_label_rect(v, 0.0);
    let hint = settings_display_mode_hint_rect(v, 0.0);
    let option = settings_zone_display_mode_radio_rect(v, 0.0, 0);
    assert_eq!(label.x, hint.x);
    assert_eq!(label.width, hint.width);
    assert!((hint.y - label.bottom() - SETTINGS_DISPLAY_MODE_HINT_GAP).abs() < 0.01);
    assert!(label.right() + SETTINGS_DISPLAY_MODE_COPY_GAP <= option.x + 0.01);
    assert!(hint.bottom() <= settings_zone_display_mode_picker_row_rect(v, 0.0).bottom());
}

#[test]
fn alpha4_display_mode_content_height_includes_full_vertical_stack() {
    let stack_h = SETTINGS_RADIO_H * SETTINGS_ZONE_DISPLAY_MODE_COUNT as f32
        + SETTINGS_RADIO_GAP * (SETTINGS_ZONE_DISPLAY_MODE_COUNT - 1) as f32;
    assert_eq!(
        settings_display_mode_content_height(),
        SETTINGS_SECTION_LABEL_H + stack_h + SETTINGS_SECTION_GAP
    );
}

#[test]
fn alpha4_radio_inner_dot_sits_inside_outer_circle() {
    let v = vp();
    for index in 0..SETTINGS_ZONE_DISPLAY_MODE_COUNT {
        let outer = settings_zone_display_mode_radio_outer_rect(v, 0.0, index);
        let inner = settings_zone_display_mode_radio_inner_rect(v, 0.0, index);
        assert!(inner.x >= outer.x);
        assert!(inner.y >= outer.y);
        assert!(inner.right() <= outer.right());
        assert!(inner.bottom() <= outer.bottom());
        assert_eq!(inner.width, SETTINGS_RADIO_INNER_D);
        assert_eq!(outer.width, SETTINGS_RADIO_OUTER_D);
    }
}

#[test]
fn alpha4_radio_label_sits_right_of_outer_circle() {
    let v = vp();
    for index in 0..SETTINGS_ZONE_DISPLAY_MODE_COUNT {
        let outer = settings_zone_display_mode_radio_outer_rect(v, 0.0, index);
        let label = settings_zone_display_mode_radio_label_rect(v, 0.0, index);
        assert!(label.x >= outer.right());
    }
}

/// α4 / G3 parity (2026-06-01) — the relationship INVERTED: the §2 Paths
/// sources section now sits ABOVE the §4 DisplayMode picker (Tauri body
/// order General → **Paths** → Appearance → **DisplayMode**). The picker is
/// no longer wedged between the General band and §2 Paths.
#[test]
fn g3_m2_sources_section_sits_above_display_mode_picker() {
    let v = vp();
    let picker = settings_zone_display_mode_picker_row_rect(v, 0.0);
    let sources_label = settings_sources_label_rect(v, 0.0);
    // §2 Paths sources label must sit ABOVE the §4 picker row top.
    assert!(
        sources_label.bottom() <= picker.y,
        "§2 Paths sources label (bottom={}) must sit above the §4 \
             DisplayMode picker row (y={}) post-G3 reorder",
        sources_label.bottom(),
        picker.y,
    );
}

#[test]
fn m1_body_max_scroll_floors_at_zero_when_content_fits() {
    let v = vp();
    let max = settings_body_max_scroll(10.0, v);
    assert_eq!(max, 0.0);
}

#[test]
fn m1_body_max_scroll_returns_overflow_when_content_taller_than_body() {
    let v = vp();
    let body = settings_body_rect(v);
    let max = settings_body_max_scroll(body.height + 120.0, v);
    assert!((max - 120.0).abs() < 0.01);
}

#[test]
fn m1_clamp_scroll_never_goes_negative() {
    let v = vp();
    let f = SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly);
    assert_eq!(settings_clamp_scroll(0.0, -100.0, v, &f), 0.0);
    assert_eq!(settings_clamp_scroll(20.0, -100.0, v, &f), 0.0);
}

#[test]
fn m1_clamp_scroll_caps_at_max() {
    let v = vp();
    let f = SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly);
    let content = settings_body_content_height(v, &f);
    let max = settings_body_max_scroll(content, v);
    assert_eq!(settings_clamp_scroll(0.0, max + 999.0, v, &f), max);
}

#[test]
fn m1_top_toggle_count_pinned() {
    assert_eq!(SETTINGS_TOP_TOGGLE_COUNT, 5);
}

#[test]
// Intentional const guard: asserts the const shadow-alpha stays at 0.0,
// so clippy sees a constant value (that is the regression lock).
#[allow(clippy::assertions_on_constants)]
fn v5_panel_shadow_alpha_locked_at_zero() {
    // V-5 (TL re-issue 2026-05-21) — the 8-DIP hard-edged drop-shadow
    // ring used to paint at 0.45 (v1) / 0.15 (v2). Both reading as a
    // visible "mask ring" on the wallpaper because `fill_rounded_rect`
    // has no gaussian falloff. The re-issued V-5 contract requires
    // "panel 外只露桌面 wallpaper, 不出现任何 BentoDesk-painted overlay
    // 圈" so the alpha is locked at 0.0 (early-returns out of
    // `fill_rounded_rect` in render.rs at `color.a <= 0.0`).
    // Re-introducing any non-zero alpha resurrects the regression until
    // a gaussian-blur drop-shadow API lands (carry-over task #13).
    assert!(
        SETTINGS_PANEL_SHADOW_ALPHA <= 0.0,
        "panel shadow alpha {} would render as a hard-edged halo / \
             mask ring; keep at 0.0 until a gaussian-blur drop-shadow \
             API lands (carry-over task #13)",
        SETTINGS_PANEL_SHADOW_ALPHA,
    );
    assert!((SETTINGS_PANEL_SHADOW_ALPHA - 0.0).abs() < f32::EPSILON);
}

// ── M1h — Plugins §11 inline geometry ──────────────────────────────

/// Helper: a base flag set with the Plugins section anchored after an empty
/// Backup list (the shipped layout while Encryption §10 is deferred).
fn plugin_flags(plugin_rows: usize) -> SettingsBodyFlags {
    SettingsBodyFlags::new(true, true, false, false, UpdaterHeightKind::StatusOnly)
        .with_backup_rows(0)
        .with_plugin_rows(plugin_rows)
}

#[test]
fn m1h_plugins_section_sits_below_encryption_card() {
    // With no status, Plugins follows the hint directly; a visible status
    // inserts exactly one row + gap. Both paths must stay below Backup.
    let v = vp();
    let f = plugin_flags(2);
    let backup_empty = settings_backup_entry_row_rect(v, 0.0, &f, 0);
    let encryption_hint = settings_encryption_hint_rect(v, 0.0, &f);
    let plugin_label = settings_plugins_label_rect(v, 0.0, &f);
    assert!(encryption_hint.y >= backup_empty.bottom());
    assert!((plugin_label.y - encryption_hint.bottom() - SETTINGS_SECTION_GAP).abs() < 0.01);

    let with_status = f.with_encryption_status(true);
    let status = settings_encryption_status_rect(v, 0.0, &with_status);
    let plugin_after_status = settings_plugins_label_rect(v, 0.0, &with_status);
    assert!((plugin_after_status.y - status.bottom() - SETTINGS_SECTION_GAP).abs() < 0.01);
}

#[test]
fn m1h_install_button_full_width_below_title() {
    let v = vp();
    let f = plugin_flags(0);
    let label = settings_plugins_label_rect(v, 0.0, &f);
    let install = settings_plugins_install_button_rect(v, 0.0, &f);
    // Install button sits directly below the title and spans the same
    // (full body) width.
    assert!((install.y - label.bottom()).abs() < 0.01);
    assert_eq!(install.x, label.x);
    assert_eq!(install.width, label.width);
    assert_eq!(install.height, SETTINGS_PLUGIN_INSTALL_BTN_H);
}

#[test]
fn m1h_plugin_cards_stack_vertically_below_install() {
    let v = vp();
    let f = plugin_flags(3);
    let install = settings_plugins_install_button_rect(v, 0.0, &f);
    let card0 = settings_plugin_card_rect(v, 0.0, &f, 0);
    let card1 = settings_plugin_card_rect(v, 0.0, &f, 1);
    let card2 = settings_plugin_card_rect(v, 0.0, &f, 2);
    // First card sits below the install button (plus the leading gap).
    assert!(card0.y >= install.bottom());
    // Cards stack with a fixed step = card height + inter-card gap.
    assert!(card0.y < card1.y);
    assert!(card1.y < card2.y);
    assert!((card1.y - card0.y - (SETTINGS_PLUGIN_CARD_H + SETTINGS_PLUGIN_CARD_GAP)).abs() < 0.01);
    assert_eq!(card0.height, SETTINGS_PLUGIN_CARD_H);
}

#[test]
fn m1h_plugin_status_reflows_cards_and_scroll_height_in_lockstep() {
    let v = vp();
    let without_status = plugin_flags(1);
    let with_status = without_status.with_plugin_status(true);
    let card_without = settings_plugin_card_rect(v, 0.0, &without_status, 0);
    let card_with = settings_plugin_card_rect(v, 0.0, &with_status, 0);
    let expected_shift = SETTINGS_PLUGIN_STATUS_H + SETTINGS_PLUGIN_CARD_GAP;
    assert!((card_with.y - card_without.y - expected_shift).abs() < 0.01);

    let body_without = settings_body_content_height(v, &without_status);
    let body_with = settings_body_content_height(v, &with_status);
    assert!((body_with - body_without - expected_shift).abs() < 0.01);

    let status = settings_plugin_status_rect(v, 0.0, &with_status);
    assert!(status.bottom() + SETTINGS_PLUGIN_CARD_GAP <= card_with.y + 0.01);
}

#[test]
fn m1h_card_controls_fit_inside_card_in_order() {
    let v = vp();
    let f = plugin_flags(1);
    let card = settings_plugin_card_rect(v, 0.0, &f, 0);
    let name = settings_plugin_name_rect(card);
    let badge = settings_plugin_badge_rect(card);
    let toggle = settings_plugin_toggle_hit_rect(card);
    let author = settings_plugin_author_rect(card);
    let desc = settings_plugin_desc_rect(card);
    let uninstall = settings_plugin_uninstall_button_rect(card);
    // Header sub-row: name | badge | toggle, packed left→right inside card.
    assert!(name.right() <= badge.x);
    assert!(badge.right() <= toggle.x);
    assert!(toggle.right() <= card.right() + 0.01);
    // Vertical stack: header → author → desc → actions (uninstall), all
    // inside the card.
    assert!(author.y >= name.y);
    assert!(desc.y >= author.bottom() - 0.01);
    assert!(uninstall.y >= desc.bottom() - 0.01);
    assert!(uninstall.bottom() <= card.bottom() + 0.01);
    assert!(uninstall.right() <= card.right() + 0.01);
}

#[test]
fn m1h_plugins_content_height_grows_with_capped_row_count() {
    // 0 (empty placeholder) < few < cap == over-cap (capped).
    let none = settings_plugins_content_height(0);
    let one = settings_plugins_content_height(1);
    let few = settings_plugins_content_height(3);
    let at_cap = settings_plugins_content_height(SETTINGS_PLUGINS_ROW_VISIBLE_MAX);
    let over_cap = settings_plugins_content_height(SETTINGS_PLUGINS_ROW_VISIBLE_MAX + 4);
    assert!(none > 0.0);
    assert!(one > none);
    assert!(few > one);
    // Over-cap clamps to the cap height (visible-row cap honoured).
    assert!((over_cap - at_cap).abs() < f32::EPSILON);
    // The empty-state height is the title + install + gap + one empty row.
    let expected_empty = SETTINGS_SECTION_LABEL_H
        + SETTINGS_PLUGIN_INSTALL_BTN_H
        + SETTINGS_PLUGIN_CARD_GAP
        + SETTINGS_PLUGIN_EMPTY_ROW_H
        + SETTINGS_SECTION_GAP;
    assert!((none - expected_empty).abs() < 0.01);
}

#[test]
fn m1h_plugin_row_count_feeds_body_height_and_scroll() {
    let v = vp();
    // Adding plugin rows must strictly grow the total body content height
    // (so the scroll clamp lets the user reach the new cards).
    let h0 = settings_body_content_height(v, &plugin_flags(0));
    let h2 = settings_body_content_height(v, &plugin_flags(2));
    assert!(h2 > h0);
    // The growth equals the plugins-section delta exactly (no other section
    // depends on plugin_row_count).
    let delta_section = settings_plugins_content_height(2) - settings_plugins_content_height(0);
    assert!((h2 - h0 - delta_section).abs() < 0.01);
}

// ── M7 — Encryption §10 inline geometry ────────────────────────────────

#[test]
fn m7_encryption_section_ordering() {
    // The §10 card label must sit BELOW the Backup card's last row and
    // ABOVE the Plugins group title (anchored between §9 and §11).
    let v = vp();
    let f = plugin_flags(0);
    let backup_last = settings_backup_entry_row_rect(v, 0.0, &f, 0);
    let enc_label = settings_encryption_label_rect(v, 0.0, &f);
    let plugin_label = settings_plugins_label_rect(v, 0.0, &f);
    assert!(
        enc_label.y >= backup_last.bottom() + SETTINGS_SECTION_GAP - 0.01,
        "encryption label (y={}) must sit a section gap below the backup \
             card's last row (bottom={})",
        enc_label.y,
        backup_last.bottom(),
    );
    assert!(
        enc_label.y < plugin_label.y,
        "encryption label (y={}) must sit above the plugins label (y={})",
        enc_label.y,
        plugin_label.y,
    );
}

#[test]
fn m7_encryption_content_height_is_fixed_and_positive() {
    // Fixed-height card (no variable rows): the helper is a constant and
    // must equal the sum of its laid-out rows.
    let h = settings_encryption_content_height();
    assert!(h > 0.0);
    // Default/no-status card: 6 rows separated by 5 × 10px gaps.
    let expected = SETTINGS_SECTION_LABEL_H
        + SETTINGS_ENCRYPTION_ROW_H
        + SETTINGS_ENCRYPTION_ROW_H
        + SETTINGS_ENCRYPTION_BTN_ROW_H
        + SETTINGS_ENCRYPTION_INPUT_ROW_H
        + SETTINGS_ENCRYPTION_ROW_H
        + SETTINGS_ENCRYPTION_ROW_GAP * 5.0
        + SETTINGS_SECTION_GAP;
    assert!((h - expected).abs() < f32::EPSILON);
    let with_status = settings_encryption_content_height_for_status(true);
    assert!(
        (with_status - h - SETTINGS_ENCRYPTION_ROW_H - SETTINGS_ENCRYPTION_ROW_GAP).abs()
            < f32::EPSILON
    );
}

#[test]
fn m7_settings_body_content_height_includes_encryption() {
    // The total body height must grow by exactly the encryption card's
    // fixed height vs a hypothetical body without it. We assert the live
    // total minus the sum of all OTHER sections equals the encryption
    // term (i.e. the term is actually included once).
    let v = vp();
    let f = plugin_flags(0);
    let total = settings_body_content_height(v, &f);
    // G3 parity (2026-06-01) — the `others` sum now includes the §4
    // DisplayMode group height (promoted out of the General band into its
    // own section). Without it the body total no longer matches the sum of
    // every non-encryption section.
    let others = SETTINGS_BODY_TOP_INSET
        + settings_m2_content_height(v, f.source_row_count)
        + settings_appearance_content_height(v)
        + settings_display_mode_content_height()
        + settings_perf_startup_content_height(
            v,
            f.crash_restart_enabled,
            f.safe_start_after_hibernation,
        )
        + settings_stealth_content_height(f.stealth_has_retry, f.stealth_has_error)
        + settings_updater_content_height(f.updater_kind)
        + settings_backup_content_height(f.backup_row_count)
        + settings_plugins_content_height(f.plugin_row_count)
        + SETTINGS_BODY_BOTTOM_INSET;
    assert!((total - others - settings_encryption_content_height()).abs() < 0.01);
}

#[test]
fn m7_encryption_mode_buttons_wrap_two_plus_one_without_overlap() {
    let v = vp();
    let f = plugin_flags(0);
    let b0 = settings_encryption_mode_button_rect(v, 0.0, &f, 0);
    let b1 = settings_encryption_mode_button_rect(v, 0.0, &f, 1);
    let b2 = settings_encryption_mode_button_rect(v, 0.0, &f, 2);
    // None + DPAPI share row 1; Passphrase wraps to row 2, left aligned.
    assert_eq!(b0.y, b1.y);
    assert!(b2.y >= b0.bottom() + SETTINGS_ENCRYPTION_BTN_GAP - 0.01);
    assert!((b2.x - b0.x).abs() < 0.01);
    assert!(b0.width > 0.0);
    assert!(b1.x >= b0.right() - 0.01);
    let row = settings_encryption_mode_row_rect(v, 0.0, &f);
    assert!(b0.x >= row.x - 0.01);
    assert!(b1.right() <= row.right() + 0.01);
    assert!(b2.bottom() <= row.bottom() + 0.01);
}

#[test]
fn m7_encryption_rows_stack_in_order() {
    // label → desc → current-mode → mode-grid → passphrase input → hint →
    // status, each strictly below the previous.
    let v = vp();
    let f = plugin_flags(0);
    let label = settings_encryption_label_rect(v, 0.0, &f);
    let desc = settings_encryption_desc_rect(v, 0.0, &f);
    let current = settings_encryption_current_mode_rect(v, 0.0, &f);
    let mode_row = settings_encryption_mode_row_rect(v, 0.0, &f);
    let input = settings_encryption_passphrase_input_rect(v, 0.0, &f);
    let hint = settings_encryption_hint_rect(v, 0.0, &f);
    let status = settings_encryption_status_rect(v, 0.0, &f);
    assert!(desc.y >= label.bottom() - 0.01);
    assert!(current.y >= desc.bottom() - 0.01);
    assert!(mode_row.y >= current.bottom() - 0.01);
    assert!(input.y >= mode_row.bottom() - 0.01);
    assert!(hint.y >= input.bottom() - 0.01);
    assert!(status.y >= hint.bottom() - 0.01);
}

/// P13 (#7 fix wave 2026-06-01) — every sibling row of the §10 card is
/// separated by EXACTLY the 10px Tauri `gap` (`.encryption-card { gap:10px }`).
/// Pin each inter-row gap so the rhythm can't silently regress to 0px.
#[test]
fn p13_encryption_rows_separated_by_ten_px_gap() {
    let v = vp();
    let f = plugin_flags(0);
    let label = settings_encryption_label_rect(v, 0.0, &f);
    let desc = settings_encryption_desc_rect(v, 0.0, &f);
    let current = settings_encryption_current_mode_rect(v, 0.0, &f);
    let mode_row = settings_encryption_mode_row_rect(v, 0.0, &f);
    let pass_row = settings_encryption_passphrase_row_rect(v, 0.0, &f);
    let hint = settings_encryption_hint_rect(v, 0.0, &f);
    let status = settings_encryption_status_rect(v, 0.0, &f);
    let g = SETTINGS_ENCRYPTION_ROW_GAP;
    assert!((desc.y - label.bottom() - g).abs() < 0.01);
    assert!((current.y - desc.bottom() - g).abs() < 0.01);
    assert!((mode_row.y - current.bottom() - g).abs() < 0.01);
    assert!((pass_row.y - mode_row.bottom() - g).abs() < 0.01);
    assert!((hint.y - pass_row.bottom() - g).abs() < 0.01);
    assert!((status.y - hint.bottom() - g).abs() < 0.01);
}

/// P4 (#7 fix wave 2026-06-01) — the passphrase row splits into a LEFT label
/// cell + a RIGHT input box (Tauri `justify-content: space-between`). The
/// label sits on the left, the input fills the rest, they don't overlap, and
/// the input no longer spans the full row width (so a click on the label cell
/// is NOT a focus hit).
#[test]
fn p4_passphrase_row_splits_label_and_input() {
    let v = vp();
    let f = plugin_flags(0);
    let row = settings_encryption_passphrase_row_rect(v, 0.0, &f);
    let label = settings_encryption_passphrase_label_rect(v, 0.0, &f);
    let input = settings_encryption_passphrase_input_rect(v, 0.0, &f);
    // Label is the left cell, input is to its right, no overlap.
    assert!(
        (label.x - row.x).abs() < 0.01,
        "label hugs the row's left edge"
    );
    assert!(
        input.x >= label.right() - 0.01,
        "input sits right of the label"
    );
    assert!(input.x > label.right(), "a gap separates label and input");
    // Input ends at the row's right edge (fills the remaining width).
    assert!((input.right() - row.right()).abs() < 0.01);
    // Input is strictly narrower than the full row (label cell + gap removed).
    assert!(input.width < row.width - SETTINGS_ENCRYPTION_PASS_LABEL_W * 0.5);
    // Same vertical band as the row.
    assert!((label.y - row.y).abs() < 0.01 && (input.y - row.y).abs() < 0.01);
}
