use super::*;
use bento_nano_style::tokens as style_tokens;

#[test]
fn capsule_picker_chrome_from_tauri_tokens_consumes_wave_b_ssot() {
    let chrome = CapsulePickerChrome::from_tauri_tokens(
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
        chrome.selected_background,
        style_tokens::PALETTE_DARK.surface_active
    );
    assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
    assert_eq!(chrome.body_color, style_tokens::PALETTE_DARK.text_primary);
    assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
    assert_eq!(chrome.error_color, style_tokens::PALETTE_DARK.accent_red);
    assert_eq!(
        chrome.panel_radius,
        BorderRadius::all(style_tokens::RADIUS.expanded)
    );
    assert_eq!(
        chrome.row_radius,
        BorderRadius::all(style_tokens::RADIUS.card)
    );
    // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
    assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
}

fn sample_entry(id: &'static str, name: &'static str) -> CapsuleEntry {
    CapsuleEntry::new(id, name, "briefcase", "2026-05-03T12:00:00Z")
}

#[test]
fn capsule_picker_default_chrome_uses_palette_surface() {
    let p = CapsulePicker::new("Context Capsules");
    let palette = theme::current().palette;
    assert_eq!(p.background, palette.surface);
    assert_eq!(p.title_color, palette.text);
    assert_eq!(p.border_radius.top_left, 12.0);
    assert_eq!(p.width, Length::Px(480.0));
    assert_eq!(p.height, Length::Px(600.0));
}

#[test]
fn capsule_picker_chrome_accepts_explicit_active_palette() {
    let mut palette = theme::current().palette;
    palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
    palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
    palette.selection = Color::from_u8(0x44, 0x55, 0x66, 0xCC);
    palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
    palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);
    palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);

    let chrome = CapsulePickerChrome::from_palette(palette);

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
        Color::from_u8(0x44, 0x55, 0x66, 0xCC)
    );
    assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
    assert_eq!(chrome.error_color, Color::from_u8(0xCC, 0x44, 0x44, 0xFF));
}

#[test]
fn capsule_picker_chrome_accepts_explicit_radius_shadow_tokens() {
    let palette = theme::current().palette;
    let radius = RadiusTokens {
        sm: BorderRadius::all(3.0),
        md: BorderRadius::all(7.0),
        lg: BorderRadius::all(11.0),
        xl: BorderRadius::all(17.0),
        full: BorderRadius::all(999.0),
    };
    let mut shadow = shadow::DEFAULT;
    shadow.md = Shadow {
        offset_x: 2.0,
        offset_y: 5.0,
        blur: 13.0,
        spread: 0.0,
        color: Color::from_u8(0x10, 0x20, 0x30, 0x99),
    };

    let chrome = CapsulePickerChrome::from_tokens(palette, radius, shadow);

    assert_eq!(chrome.panel_shadow, shadow.md);
    assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
    assert_eq!(chrome.row_radius, BorderRadius::all(11.0));
}

#[test]
fn capsule_picker_panel_shadow_rect_uses_token_shadow_geometry() {
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

    let rect = capsule_picker_panel_shadow_rect(panel, shadow);

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
fn capsule_picker_state_set_entries_replaces_list() {
    let mut s = CapsulePickerState::new();
    let mut entries = SmallVec::new();
    entries.push(sample_entry("a", "Coding"));
    entries.push(sample_entry("b", "Reading"));
    s.set_entries(entries);
    assert_eq!(s.entries().len(), 2);
    assert_eq!(s.entries()[0].name, SmolStr::new_static("Coding"));
    assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("a"));
}

#[test]
fn capsule_picker_selection_wraps_and_clamps() {
    let mut s = CapsulePickerState::new();
    let mut entries = SmallVec::new();
    entries.push(sample_entry("a", "Coding"));
    entries.push(sample_entry("b", "Reading"));
    s.set_entries(entries);
    assert_eq!(s.selected_index(), 0);
    s.select_next();
    assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("b"));
    s.select_next();
    assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("a"));
    s.select_prev();
    assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("b"));

    let mut one = SmallVec::new();
    one.push(sample_entry("z", "Only"));
    s.set_entries(one);
    assert_eq!(s.selected_index(), 0);
    assert_eq!(s.selected_entry().map(|entry| entry.id.as_str()), Some("z"));
}

#[test]
fn capsule_picker_mouse_actions_and_row_selection_are_reachable() {
    let viewport = Size {
        width: 480.0,
        height: 600.0,
    };
    for (index, expected) in CAPSULE_PICKER_ACTIONS.iter().copied().enumerate() {
        let rect = capsule_picker_action_rect(viewport, index);
        assert_eq!(
            capsule_picker_hit_test(
                viewport,
                2,
                false,
                rect.x + rect.width * 0.5,
                rect.y + rect.height * 0.5,
            ),
            Some(expected)
        );
    }

    let mut state = CapsulePickerState::new();
    let mut entries = SmallVec::new();
    entries.push(sample_entry("a", "Coding"));
    entries.push(sample_entry("b", "Reading"));
    state.set_entries(entries);
    assert!(state.select_index(1));
    assert_eq!(
        state.selected_entry().map(|entry| entry.id.as_str()),
        Some("b")
    );
    assert!(!state.select_index(9));
}

#[test]
fn capsule_picker_click_capture_uses_typed_name() {
    let mut s = CapsulePickerState::new();
    s.set_new_name("My Workflow");
    s.click_capture("ignored fallback");
    assert_eq!(
        s.take_action(),
        Some(CapsulePickerAction::Capture(SmolStr::new_static(
            "My Workflow"
        )))
    );
}

#[test]
fn capsule_picker_click_capture_uses_fallback_when_blank() {
    let mut s = CapsulePickerState::new();
    s.set_new_name("   "); // whitespace only — counts as blank
    s.click_capture("Capsule 2026-05-03 12:00");
    assert_eq!(
        s.take_action(),
        Some(CapsulePickerAction::Capture(SmolStr::new_static(
            "Capsule 2026-05-03 12:00"
        )))
    );
}

#[test]
fn capsule_picker_restore_delete_close_record_actions() {
    let mut s = CapsulePickerState::new();
    s.click_restore("cap-1");
    assert_eq!(
        s.take_action(),
        Some(CapsulePickerAction::Restore(SmolStr::new_static("cap-1")))
    );

    s.click_delete("cap-2");
    assert_eq!(
        s.take_action(),
        Some(CapsulePickerAction::Delete(SmolStr::new_static("cap-2")))
    );

    s.click_close();
    assert_eq!(s.take_action(), Some(CapsulePickerAction::Close));
}

#[test]
fn capsule_picker_take_action_is_one_shot() {
    let mut s = CapsulePickerState::new();
    s.click_close();
    assert!(s.take_action().is_some());
    assert!(s.take_action().is_none());
}

#[test]
fn capsule_picker_busy_and_error_surface_correctly() {
    let mut s = CapsulePickerState::new();
    assert!(!s.is_busy());
    assert!(s.last_error().is_none());

    s.set_busy(true);
    s.set_error(Some(SmolStr::new_static("backend offline")));
    assert!(s.is_busy());
    assert_eq!(s.last_error(), Some("backend offline"));

    s.set_error(None);
    assert!(s.last_error().is_none());
}
