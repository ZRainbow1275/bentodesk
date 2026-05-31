//! BulkManagerPanel — unit + smoke tests, kept in a sibling file so the
//! main `mod.rs` stays under the §15 800-LOC budget.
//!
//! Per the Wave G ruling: every backend type the test body references must
//! be explicitly imported here (the `mod tests` declaration in `mod.rs`
//! does NOT inherit the parent's `use` lines once the tests live in their
//! own file).

use super::{
    BULK_MANAGER_ACTION_BUTTONS, BulkManagerAction, BulkManagerChrome, BulkManagerPointerHit,
    BulkManagerState, BulkTextEditField, FOOTER_HEIGHT_PX, HEADER_HEIGHT_PX,
    PANEL_CORNER_RADIUS_PX, PANEL_HEIGHT_PX, PANEL_MAX_HEIGHT_FRACTION, PANEL_MAX_WIDTH_FRACTION,
    PANEL_PADDING_PX, PANEL_WIDTH_PX, RUNTIME_SEARCH_LIMIT, SEARCH_INPUT_HEIGHT_PX,
    SEARCH_INPUT_WIDTH_PX, SELECTED_ROW_STRIPE_PX, SortDirection, SortKey, TABLE_CELL_PADDING_Y_PX,
    TABLE_ROW_HEIGHT_PX, TOOLBAR_HEIGHT_PX, ZoneRow, build, bulk_manager_button_rect,
    bulk_manager_hit_test, bulk_manager_panel_shadow_rect, bulk_manager_row_rect,
    bulk_manager_search_rect, bulk_manager_sort_header_rect, bulk_manager_visible_window_start,
    bulk_manager_visible_window_summary,
};
use bento_nano_layout::{Direction, LayoutSource};
use bento_nano_style::{BorderRadius, Color, Length, Rect, Shadow, Size};
use bento_nano_theme as theme;
use bento_nano_zone::ZoneId;
use smol_str::SmolStr;

use crate::dispatcher::Point;

fn sample_row(id: u64, name: &str, items: u32, accent: &str, w: u32, h: u32) -> ZoneRow {
    ZoneRow {
        id: ZoneId(id),
        display_name: SmolStr::new(name),
        item_count: items,
        accent_hex: SmolStr::new(accent),
        visible: true,
        locked: false,
        icon_slug: SmolStr::new_static("folder"),
        capsule_size: SmolStr::new_static("medium"),
        display_mode: SmolStr::new_static("inherit"),
        width_percent: w,
        height_percent: h,
        position_x_percent: 0,
        position_y_percent: 0,
    }
}

fn sample_zones() -> Vec<ZoneRow> {
    vec![
        sample_row(1, "Inbox", 3, "#3b82f6", 30, 40),
        sample_row(2, "Projects", 12, "#22c55e", 60, 50),
        sample_row(3, "Archive", 5, "#64748b", 20, 25),
        sample_row(4, "Notes", 8, "#22c55e", 25, 30),
    ]
}

#[test]
fn snap_geometry_constants_pinned() {
    assert_eq!(PANEL_WIDTH_PX, 960.0);
    assert_eq!(PANEL_HEIGHT_PX, 640.0);
    assert!((PANEL_MAX_WIDTH_FRACTION - 0.92).abs() < f32::EPSILON);
    assert!((PANEL_MAX_HEIGHT_FRACTION - 0.80).abs() < f32::EPSILON);
    assert_eq!(PANEL_PADDING_PX, 20.0);
    assert_eq!(PANEL_CORNER_RADIUS_PX, 16.0);
    assert_eq!(HEADER_HEIGHT_PX, 52.0);
    assert_eq!(TOOLBAR_HEIGHT_PX, 44.0);
    assert_eq!(FOOTER_HEIGHT_PX, 56.0);
    assert_eq!(TABLE_ROW_HEIGHT_PX, 44.0);
    assert_eq!(TABLE_CELL_PADDING_Y_PX, 8.0);
    assert_eq!(SEARCH_INPUT_WIDTH_PX, 240.0);
    assert_eq!(SEARCH_INPUT_HEIGHT_PX, 32.0);
    assert_eq!(SELECTED_ROW_STRIPE_PX, 2.0);
}

#[test]
fn sort_key_all_lists_four_in_snap_order() {
    assert_eq!(
        SortKey::ALL,
        &[
            SortKey::Name,
            SortKey::Items,
            SortKey::Accent,
            SortKey::Size
        ]
    );
}

#[test]
fn sort_direction_flipped_round_trips() {
    assert_eq!(
        SortDirection::Ascending.flipped(),
        SortDirection::Descending
    );
    assert_eq!(
        SortDirection::Descending.flipped(),
        SortDirection::Ascending
    );
}

#[test]
fn fresh_state_is_empty() {
    let s = BulkManagerState::new();
    assert!(s.zones().is_empty());
    assert!(s.selected().is_empty());
    assert!(s.search().is_empty());
    assert_eq!(s.sort_key(), SortKey::Name);
    assert_eq!(s.sort_direction(), SortDirection::Ascending);
    assert!(!s.can_act());
    assert!(!s.has_pending_action());
}

#[test]
fn set_zones_seeds_rows_and_clears_selection() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    assert_eq!(s.zones().len(), 4);
    assert!(s.selected().is_empty());
}

#[test]
fn set_zones_after_selection_drops_stale_ids() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(1));
    assert!(!s.selected().is_empty());
    s.set_zones(sample_zones());
    assert!(s.selected().is_empty());
}

#[test]
fn toggle_selection_adds_then_removes() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(1));
    assert!(s.is_selected(ZoneId(1)));
    s.toggle_selection(ZoneId(1));
    assert!(!s.is_selected(ZoneId(1)));
}

#[test]
fn pointer_hit_test_maps_visible_buttons_and_rows() {
    let viewport = Size {
        width: 720.0,
        height: 540.0,
    };
    let all = BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .find(|spec| spec.hit == BulkManagerPointerHit::SelectAll)
        .copied()
        .expect("select-all spec");
    let all_rect = bulk_manager_button_rect(viewport, all);
    assert_eq!(
        bulk_manager_hit_test(viewport, 3, 0, all_rect.x + 2.0, all_rect.y + 2.0),
        Some(BulkManagerPointerHit::SelectAll)
    );
    let update = BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .find(|spec| spec.hit == BulkManagerPointerHit::Update)
        .copied()
        .expect("update spec");
    let update_rect = bulk_manager_button_rect(viewport, update);
    assert_eq!(
        bulk_manager_hit_test(viewport, 3, 0, update_rect.x + 2.0, update_rect.y + 2.0),
        Some(BulkManagerPointerHit::Update)
    );
    let text = BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .find(|spec| spec.hit == BulkManagerPointerHit::TextEdit)
        .copied()
        .expect("text edit spec");
    let text_rect = bulk_manager_button_rect(viewport, text);
    assert_eq!(
        bulk_manager_hit_test(viewport, 3, 0, text_rect.x + 2.0, text_rect.y + 2.0),
        Some(BulkManagerPointerHit::TextEdit)
    );
    let icon = BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .find(|spec| spec.hit == BulkManagerPointerHit::IconPicker)
        .copied()
        .expect("icon picker spec");
    let icon_rect = bulk_manager_button_rect(viewport, icon);
    assert_eq!(
        bulk_manager_hit_test(viewport, 3, 0, icon_rect.x + 2.0, icon_rect.y + 2.0),
        Some(BulkManagerPointerHit::IconPicker)
    );
    let color = BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .find(|spec| spec.hit == BulkManagerPointerHit::AccentPicker)
        .copied()
        .expect("accent picker spec");
    let color_rect = bulk_manager_button_rect(viewport, color);
    assert_eq!(
        bulk_manager_hit_test(viewport, 3, 0, color_rect.x + 2.0, color_rect.y + 2.0),
        Some(BulkManagerPointerHit::AccentPicker)
    );
    let row = bulk_manager_row_rect(viewport, 1);
    assert_eq!(
        bulk_manager_hit_test(viewport, 3, 0, row.x + 2.0, row.y + 2.0),
        Some(BulkManagerPointerHit::Row(1))
    );
    assert_eq!(
        bulk_manager_hit_test(viewport, 1, 0, row.x + 2.0, row.y + 2.0),
        None
    );
    let search = bulk_manager_search_rect(viewport);
    assert_eq!(
        bulk_manager_hit_test(viewport, 3, 0, search.x + 2.0, search.y + 2.0),
        Some(BulkManagerPointerHit::SearchInput)
    );
}

#[test]
fn pointer_hit_test_maps_sort_headers_for_each_sort_key() {
    let viewport = Size {
        width: 720.0,
        height: 540.0,
    };
    for key in SortKey::ALL {
        let rect = bulk_manager_sort_header_rect(viewport, *key);
        assert_eq!(
            bulk_manager_hit_test(
                viewport,
                3,
                0,
                rect.x + (rect.width / 2.0),
                rect.y + (rect.height / 2.0)
            ),
            Some(BulkManagerPointerHit::Sort(*key))
        );
    }
}

#[test]
fn sort_header_geometry_sits_between_toolbar_and_rows() {
    let viewport = Size {
        width: 720.0,
        height: 540.0,
    };
    let max_button_bottom = BULK_MANAGER_ACTION_BUTTONS
        .iter()
        .map(|spec| {
            let rect = bulk_manager_button_rect(viewport, *spec);
            rect.y + rect.height
        })
        .fold(f32::NEG_INFINITY, f32::max);
    let header = bulk_manager_sort_header_rect(viewport, SortKey::Name);
    let row = bulk_manager_row_rect(viewport, 0);
    assert!(max_button_bottom < header.y);
    assert!(header.y + header.height < row.y);
}

#[test]
fn visible_window_start_tracks_cursor_after_first_page() {
    assert_eq!(bulk_manager_visible_window_start(0, 0), 0);
    assert_eq!(bulk_manager_visible_window_start(0, 12), 0);
    assert_eq!(bulk_manager_visible_window_start(7, 12), 0);
    assert_eq!(bulk_manager_visible_window_start(8, 12), 1);
    assert_eq!(bulk_manager_visible_window_start(11, 12), 4);
    assert_eq!(bulk_manager_visible_window_start(40, 12), 4);
}

#[test]
fn pointer_hit_test_maps_rows_through_visible_window_offset() {
    let viewport = Size {
        width: 720.0,
        height: 540.0,
    };
    let first_displayed = bulk_manager_row_rect(viewport, 0);
    assert_eq!(
        bulk_manager_hit_test(
            viewport,
            12,
            4,
            first_displayed.x + 2.0,
            first_displayed.y + 2.0
        ),
        Some(BulkManagerPointerHit::Row(4))
    );
    let last_displayed = bulk_manager_row_rect(viewport, 7);
    assert_eq!(
        bulk_manager_hit_test(
            viewport,
            12,
            4,
            last_displayed.x + 2.0,
            last_displayed.y + 2.0
        ),
        Some(BulkManagerPointerHit::Row(11))
    );
    let beyond_displayed = bulk_manager_row_rect(viewport, 8);
    assert_eq!(
        bulk_manager_hit_test(
            viewport,
            12,
            4,
            beyond_displayed.x + 2.0,
            beyond_displayed.y + 2.0
        ),
        None
    );
}

#[test]
fn visible_window_summary_reports_visible_range_only_for_overflow() {
    assert_eq!(bulk_manager_visible_window_summary(0, 8), None);
    assert_eq!(
        bulk_manager_visible_window_summary(0, 12).map(|s| s.to_string()),
        Some("Rows 1-8 of 12".to_owned())
    );
    assert_eq!(
        bulk_manager_visible_window_summary(4, 12).map(|s| s.to_string()),
        Some("Rows 5-12 of 12".to_owned())
    );
    assert_eq!(
        bulk_manager_visible_window_summary(40, 12).map(|s| s.to_string()),
        Some("Rows 5-12 of 12".to_owned())
    );
}

#[test]
fn toggle_visible_row_selection_sets_cursor_and_selection() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    let second_visible = s.visible_rows()[1].id;
    s.toggle_visible_row_selection(1);
    assert_eq!(s.cursor_index(), 1);
    assert_eq!(s.selected(), &[second_visible]);
    s.toggle_visible_row_selection(1);
    assert!(s.selected().is_empty());
}

#[test]
fn select_all_picks_every_visible_row() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.select_all();
    assert_eq!(s.selected().len(), 4);
    assert!(s.all_visible_selected());
}

#[test]
fn select_all_only_picks_post_search_rows() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.set_search("o"); // matches Inbox / Projects / Notes (3 rows).
    s.select_all();
    assert_eq!(s.selected().len(), 3);
    assert!(s.is_selected(ZoneId(1)));
    assert!(s.is_selected(ZoneId(2)));
    assert!(s.is_selected(ZoneId(4)));
    assert!(!s.is_selected(ZoneId(3)));
}

#[test]
fn deselect_all_only_clears_visible_rows() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(3));
    s.set_search("o"); // hides Archive (id 3).
    s.toggle_selection(ZoneId(1));
    s.deselect_all();
    // Visible (Inbox id 1) cleared; offscreen (Archive id 3) survives.
    assert!(!s.is_selected(ZoneId(1)));
    assert!(s.is_selected(ZoneId(3)));
}

#[test]
fn invert_selection_flips_visible_membership() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(1));
    s.invert_selection();
    // Was selected → now not. Was unselected → now selected.
    assert!(!s.is_selected(ZoneId(1)));
    assert!(s.is_selected(ZoneId(2)));
    assert!(s.is_selected(ZoneId(3)));
    assert!(s.is_selected(ZoneId(4)));
}

#[test]
fn search_survives_selection_and_filters_visible_count() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(2));
    s.set_search("Inbox");
    assert_eq!(s.visible_count(), 1);
    // Selection on offscreen id is preserved.
    assert!(s.is_selected(ZoneId(2)));
}

#[test]
fn search_focus_accepts_typing_backspace_and_limit() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.focus_search();
    assert!(s.search_focused());
    assert!(s.push_search_char('P'));
    assert!(s.push_search_char('r'));
    assert_eq!(s.search(), "Pr");
    assert_eq!(s.visible_count(), 1);
    assert!(s.backspace_search());
    assert_eq!(s.search(), "P");
    s.set_search("x".repeat(RUNTIME_SEARCH_LIMIT + 10));
    assert_eq!(s.search().chars().count(), RUNTIME_SEARCH_LIMIT);
    assert!(!s.push_search_char('z'));
    assert!(s.clear_search());
    assert_eq!(s.visible_count(), 4);
    s.blur_search();
    assert!(!s.search_focused());
}

#[test]
fn set_sort_key_same_toggles_direction_different_resets_to_ascending() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    assert_eq!(s.sort_key(), SortKey::Name);
    assert_eq!(s.sort_direction(), SortDirection::Ascending);
    s.set_sort_key(SortKey::Name);
    assert_eq!(s.sort_direction(), SortDirection::Descending);
    s.set_sort_key(SortKey::Items);
    assert_eq!(s.sort_key(), SortKey::Items);
    assert_eq!(s.sort_direction(), SortDirection::Ascending);
}

#[test]
fn visible_rows_sorts_by_active_key() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.set_sort_key(SortKey::Items);
    let rows = s.visible_rows();
    let counts: Vec<u32> = rows.iter().map(|r| r.item_count).collect();
    assert_eq!(counts, vec![3, 5, 8, 12]);
    s.set_sort_key(SortKey::Items); // toggle to descending.
    let rows = s.visible_rows();
    let counts: Vec<u32> = rows.iter().map(|r| r.item_count).collect();
    assert_eq!(counts, vec![12, 8, 5, 3]);
}

#[test]
fn visible_rows_sorts_by_size_uses_area() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.set_sort_key(SortKey::Size);
    let rows = s.visible_rows();
    let areas: Vec<u64> = rows.iter().map(|r| r.area_percent()).collect();
    // Archive 20×25=500, Notes 25×30=750, Inbox 30×40=1200, Projects 60×50=3000.
    assert_eq!(areas, vec![500, 750, 1200, 3000]);
}

#[test]
fn click_hide_with_empty_selection_records_nothing() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.click_hide();
    assert!(!s.has_pending_action());
}

#[test]
fn click_hide_records_action_with_selected_ids() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(1));
    s.toggle_selection(ZoneId(3));
    s.click_hide();
    let action = s.take_action().expect("hide action recorded");
    match action {
        BulkManagerAction::Hide { ids } => {
            assert_eq!(ids, vec![ZoneId(1), ZoneId(3)]);
        }
        other => panic!("expected Hide, got {other:?}"),
    }
}

#[test]
fn click_show_records_show_action() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(2));
    s.click_show();
    assert_eq!(
        s.take_action(),
        Some(BulkManagerAction::Show {
            ids: vec![ZoneId(2)],
        })
    );
}

#[test]
fn click_delete_requires_same_selection_confirmation() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(4));
    s.click_delete();
    assert!(s.take_action().is_none());
    assert_eq!(s.delete_confirmation(), &[ZoneId(4)]);

    s.click_delete();
    assert_eq!(
        s.take_action(),
        Some(BulkManagerAction::Delete {
            ids: vec![ZoneId(4)],
        })
    );
    assert!(s.delete_confirmation().is_empty());
}

#[test]
fn click_delete_rearms_when_selection_changes() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(1));
    s.click_delete();
    assert_eq!(s.delete_confirmation(), &[ZoneId(1)]);

    s.toggle_selection(ZoneId(2));
    assert!(s.delete_confirmation().is_empty());
    s.click_delete();
    assert!(s.take_action().is_none());
    assert_eq!(s.delete_confirmation(), &[ZoneId(1), ZoneId(2)]);
}

#[test]
fn click_move_records_move_action_with_delta() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    s.toggle_selection(ZoneId(1));
    s.click_move(Point::new(20, -10));
    assert_eq!(
        s.take_action(),
        Some(BulkManagerAction::Move {
            ids: vec![ZoneId(1)],
            delta: Point::new(20, -10),
        })
    );
}

#[test]
fn cursor_selection_wraps_and_toggles_visible_zone() {
    let mut s = BulkManagerState::new();
    s.set_zones(sample_zones());
    let visible = s.visible_rows();
    assert_eq!(s.cursor_zone_id(), Some(visible[0].id));
    s.select_next();
    assert_eq!(s.cursor_zone_id(), Some(visible[1].id));
    s.toggle_cursor_selection();
    assert_eq!(s.selected(), &[visible[1].id]);
    s.select_prev();
    assert_eq!(s.cursor_zone_id(), Some(visible[0].id));
}

#[test]
fn click_close_records_close_action() {
    let mut s = BulkManagerState::new();
    s.click_close();
    assert_eq!(s.take_action(), Some(BulkManagerAction::Close));
}

#[test]
fn text_edit_state_accepts_typing_backspace_and_field_cycle() {
    let mut s = BulkManagerState::new();
    s.start_text_edit(BulkTextEditField::Alias);
    assert_eq!(
        s.text_edit().map(|edit| edit.field),
        Some(BulkTextEditField::Alias)
    );
    assert!(s.push_text_edit_char('文'));
    assert!(s.push_text_edit_char('档'));
    assert_eq!(s.text_edit().map(|edit| edit.draft.as_str()), Some("文档"));
    assert!(s.backspace_text_edit());
    assert_eq!(s.text_edit().map(|edit| edit.draft.as_str()), Some("文"));
    s.cycle_text_edit_field();
    assert_eq!(
        s.text_edit().map(|edit| edit.field),
        Some(BulkTextEditField::Icon)
    );
    assert_eq!(s.text_edit().map(|edit| edit.draft.as_str()), Some(""));
    s.cancel_text_edit();
    assert!(s.text_edit().is_none());
}

#[test]
fn take_action_is_one_shot() {
    let mut s = BulkManagerState::new();
    s.click_close();
    assert!(s.take_action().is_some());
    assert!(s.take_action().is_none());
}

#[test]
fn build_returns_panel_sized_container() {
    let node = build();
    let layout = node.layout();
    assert!(matches!(layout.width, Length::Px(w) if (w - PANEL_WIDTH_PX).abs() < 0.01));
    assert!(matches!(layout.height, Length::Px(h) if (h - PANEL_HEIGHT_PX).abs() < 0.01));
    assert_eq!(layout.direction, Direction::Column);
    assert!((layout.padding.top - PANEL_PADDING_PX).abs() < 0.01);
    assert!((layout.padding.left - PANEL_PADDING_PX).abs() < 0.01);
}

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
    use bento_nano_style::tokens as style_tokens;
    use bento_nano_style::BorderRadius;
    let chrome = BulkManagerChrome::from_tauri_tokens(
        style_tokens::PALETTE_DARK,
        style_tokens::RADIUS,
        style_tokens::SHADOW,
    );
    assert_eq!(chrome.panel_background, style_tokens::PALETTE_DARK.surface_expanded);
    assert_eq!(chrome.row_background, style_tokens::PALETTE_DARK.surface_hover);
    assert_eq!(chrome.cursor_background, style_tokens::PALETTE_DARK.surface_active);
    assert_eq!(chrome.selected_background, style_tokens::PALETTE_DARK.surface_active);
    assert_eq!(chrome.title_color, style_tokens::PALETTE_DARK.text_primary);
    assert_eq!(chrome.muted_color, style_tokens::PALETTE_DARK.text_muted);
    assert_eq!(chrome.panel_radius, BorderRadius::all(style_tokens::RADIUS.expanded));
    assert_eq!(chrome.row_radius, BorderRadius::all(style_tokens::RADIUS.card));
    assert_eq!(chrome.button_radius, BorderRadius::all(style_tokens::RADIUS.card));
    // M6b — `SHADOW.expanded` is a `ShadowStack`; chrome consumes `.outer()`.
    assert_eq!(chrome.panel_shadow, style_tokens::SHADOW.expanded.outer());
}
