use super::*;

fn vp() -> Size {
    Size {
        width: 800.0,
        height: 800.0,
    }
}

#[test]
fn m2_sources_label_sits_below_language_row() {
    let v = vp();
    let lang = settings_language_row_rect(v, 0.0);
    let paths = settings_paths_label_rect(v, 0.0);
    let sources = settings_sources_label_rect(v, 0.0);
    assert!(paths.y >= lang.bottom() + SETTINGS_SECTION_GAP - 0.01);
    assert!((sources.y - paths.bottom()).abs() < 0.01);
}

#[test]
fn m2_scrollbar_thumb_is_hidden_when_content_fits() {
    let v = vp();
    let body = settings_body_rect(v);
    assert_eq!(settings_scrollbar_thumb_rect(v, body.height, 0.0), None);
}

#[test]
fn m2_scrollbar_thumb_tracks_document_endpoints_inside_body() {
    let v = vp();
    let body = settings_body_rect(v);
    let content_h = body.height * 3.0;
    let max_scroll = settings_body_max_scroll(content_h, v);
    let top = settings_scrollbar_thumb_rect(v, content_h, 0.0).expect("top thumb");
    let bottom = settings_scrollbar_thumb_rect(v, content_h, max_scroll).expect("bottom thumb");

    assert!((top.y - body.y - SETTINGS_SCROLLBAR_INSET_Y).abs() < 0.01);
    assert!((bottom.bottom() - body.bottom() + SETTINGS_SCROLLBAR_INSET_Y).abs() < 0.01);
    assert_eq!(top.width, SETTINGS_SCROLLBAR_W);
    assert_eq!(top.height, bottom.height);
    assert!(top.height >= SETTINGS_SCROLLBAR_MIN_THUMB_H);
    assert!(bottom.y > top.y);
}

#[test]
fn m2_source_rows_stack_vertically_below_label() {
    let v = vp();
    let label = settings_sources_label_rect(v, 0.0);
    let r0 = settings_source_row_rect(v, 0.0, 0);
    let r1 = settings_source_row_rect(v, 0.0, 1);
    assert!(r0.y >= label.bottom() - 0.01);
    assert!((r1.y - r0.bottom() - SETTINGS_SOURCE_GAP).abs() < 0.01);
}

#[test]
fn m1i_sources_refresh_button_is_last_child_below_cards() {
    // M1i fidelity — the refresh button is the LAST child of the list,
    // right-anchored BELOW the live card stack (not on the heading row).
    let v = vp();
    let label = settings_sources_label_rect(v, 0.0);
    let refresh = settings_sources_refresh_button_rect(v, 0.0, 4);
    let last_card = settings_source_row_rect(v, 0.0, 3);
    assert!((refresh.right() - label.right()).abs() < 0.01);
    // Sits below the last card (heading-row anchor would put it at label.y).
    assert!(refresh.y >= last_card.bottom() - 0.01);
    assert!(refresh.y > label.bottom());
    assert_eq!(refresh.width, SETTINGS_SOURCE_REFRESH_BTN_W);
}

#[test]
fn m1i_refresh_button_follows_live_card_count() {
    // Fewer live cards → the refresh button rides up by exactly the height
    // of each missing card slot.
    let v = vp();
    let r4 = settings_sources_refresh_button_rect(v, 0.0, 4);
    let r2 = settings_sources_refresh_button_rect(v, 0.0, 2);
    let per_card = SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP;
    assert!((r4.y - r2.y - 2.0 * per_card).abs() < 0.01);
}

#[test]
fn m2_desktop_path_input_sits_below_last_source() {
    // Existing invariant must still hold at the full 4-card reserve.
    let v = vp();
    let refresh =
        settings_sources_refresh_button_rect(v, 0.0, SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    let label = settings_desktop_path_label_rect(v, 0.0, SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    let input = settings_desktop_path_input_rect(v, 0.0, SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    assert!(label.y >= refresh.bottom() + SETTINGS_SECTION_GAP - 0.01);
    assert!((input.y - label.bottom()).abs() < 0.01);
    assert_eq!(input.height, SETTINGS_INPUT_ROW_H);
}

#[test]
fn m1i_desktop_path_reflows_with_live_source_count() {
    // M1i fidelity — the 桌面路径 row sits HIGHER with 2 sources than with
    // 4, by exactly 2*(card_height + gap) (Tauri's flex column).
    let v = vp();
    let input2 = settings_desktop_path_input_rect(v, 0.0, 2);
    let input4 = settings_desktop_path_input_rect(v, 0.0, 4);
    let per_card = SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP;
    assert!((input4.y - input2.y - 2.0 * per_card).abs() < 0.01);
    assert!(input2.y < input4.y);
}

#[test]
fn m2_watch_textarea_sits_below_path_input() {
    let v = vp();
    let input = settings_desktop_path_input_rect(v, 0.0, 4);
    let label = settings_watch_label_rect(v, 0.0, 4);
    let area = settings_watch_textarea_rect(v, 0.0, 4);
    assert!(label.y >= input.bottom() + SETTINGS_SECTION_GAP - 0.01);
    assert!((area.y - label.bottom()).abs() < 0.01);
    assert_eq!(area.height, SETTINGS_TEXTAREA_H);
}

#[test]
fn m2_content_height_exceeds_body_to_trigger_scroll() {
    let v = vp();
    let body = settings_body_rect(v);
    let content_h = settings_m2_content_height(v, SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    // M2 should make the body scroll on an 800×800 viewport (since the
    // panel caps at 700 DIP height, body band is ~596 DIP). Five toggles
    // + language + 桌面源(4 cards) + 桌面路径 + 监控值 must exceed body.
    assert!(content_h > body.height);
}

#[test]
fn m2_scroll_offset_shifts_m2_sections_up() {
    let v = vp();
    let r_at_0 = settings_sources_label_rect(v, 0.0);
    let r_at_30 = settings_sources_label_rect(v, 30.0);
    assert!((r_at_30.y + 30.0 - r_at_0.y).abs() < 0.01);
}

#[test]
fn m1i_source_cap_is_four() {
    // The §2 list caps at the 4-slot Windows ceiling (User/Public/
    // OneDrive/Custom). Beyond that the live count is clamped.
    assert_eq!(SETTINGS_SOURCE_ROW_VISIBLE_MAX, 4);
}

#[test]
fn m1i_sources_content_height_reflows_with_count() {
    // M1i fidelity — the source-block height now GROWS with the live count
    // (one card_height + gap per card), and is clamped at the cap.
    let at1 = settings_sources_content_height(1);
    let at2 = settings_sources_content_height(2);
    let at_cap = settings_sources_content_height(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    let over = settings_sources_content_height(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize + 7);
    let per_card = SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP;
    assert!((at2 - at1 - per_card).abs() < 0.01);
    assert!(at2 > at1);
    assert!(at_cap > at2);
    // Clamped past the cap.
    assert!((over - at_cap).abs() < 0.01);
}

#[test]
fn m1i_reserve_delta_shrinks_with_live_count() {
    // The scroll-fold delta is 0 at the full reserve and grows as cards
    // are missing — exactly the blank space the old fixed reserve left.
    let d_full = settings_sources_reserve_delta(SETTINGS_SOURCE_ROW_VISIBLE_MAX as usize);
    let d2 = settings_sources_reserve_delta(2);
    let per_card = SETTINGS_SOURCE_ROW_H + SETTINGS_SOURCE_GAP;
    assert!(d_full.abs() < 0.01);
    assert!((d2 - 2.0 * per_card).abs() < 0.01);
}

#[test]
fn m1i_empty_list_uses_placeholder_height() {
    // Empty list: the block reserves one placeholder line + the refresh
    // button, not zero — so downstream sections do not collide upward.
    let empty = settings_sources_content_height(0);
    let label_plus_gap = SETTINGS_SECTION_LABEL_H + SETTINGS_SECTION_GAP;
    let stack =
        SETTINGS_SOURCE_EMPTY_H + SETTINGS_SOURCE_REFRESH_GAP + SETTINGS_SOURCE_REFRESH_BTN_H;
    assert!((empty - (label_plus_gap + stack)).abs() < 0.01);
}
