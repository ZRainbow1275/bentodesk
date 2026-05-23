//! Unit tests for `RulesWizardState` + chrome + step navigation.
//!
//! Lives in its own file so `mod.rs` stays under the §15 800-LOC budget.
//! Draft-level tests live next to the types they cover in `drafts.rs`.

use super::*;
use bento_nano_backend::rules::{Action, Condition, ConditionGroup, ConditionNode, Rule, RunMode};

fn seeded() -> RulesWizardState {
    let mut s = RulesWizardState::new();
    s.set_condition_value(0, "invoice-");
    s.set_action_kind(ActionKind::MoveToZone);
    s.set_action_value("inbox");
    s.set_name("Archive invoices");
    s
}

// ─── Constants + chrome ─────────────────────────────────────────

#[test]
fn snap_constants_match_spec() {
    assert_eq!(PANEL_WIDTH, 560.0);
    assert_eq!(PANEL_HEIGHT, 600.0);
    assert_eq!(PANEL_CORNER_RADIUS, 16.0);
    assert_eq!(HEADER_HEIGHT, 56.0);
    assert_eq!(STEP_INDICATOR_HEIGHT, 48.0);
    assert_eq!(FOOTER_HEIGHT, 64.0);
    assert_eq!(BODY_PADDING, 24.0);
    assert_eq!(PANEL_OPEN_DURATION_MS, 200);
    assert_eq!(NAME_MAX_LEN, 64);
    assert_eq!(INTERVAL_MIN_MINUTES, 1);
    assert_eq!(INTERVAL_MAX_MINUTES, 1440);
    assert_eq!(WizardStep::TOTAL, 5);
    assert_eq!(RUNTIME_PANEL_MARGIN_PX, 16.0);
    assert_eq!(RUNTIME_PANEL_INSET_PX, 18.0);
    assert_eq!(RUNTIME_ACTION_BUTTON_HEIGHT_PX, 24.0);
    assert_eq!(RUNTIME_ACTION_BUTTON_ROW_ONE_TOP_PX, 132.0);
    assert_eq!(RUNTIME_ACTION_BUTTON_ROW_TWO_TOP_PX, 162.0);
    assert_eq!(RUNTIME_FORM_TOP_PX, 196.0);
    assert_eq!(RUNTIME_RULE_ROW_TOP_PX, 228.0);
    assert_eq!(RUNTIME_RULE_ROW_HEIGHT_PX, 30.0);
    assert_eq!(RUNTIME_RULE_ROW_STRIDE_PX, 38.0);
    assert_eq!(RUNTIME_VISIBLE_RULE_LIMIT, 6);
}

#[test]
fn wizard_chrome_uses_palette_surface() {
    let w = RulesWizard::new();
    let palette = theme::current().palette;
    assert_eq!(w.background, palette.surface);
    assert_eq!(w.border, palette.border);
    assert_eq!(w.title_color, palette.text);
    assert_eq!(w.border_radius.top_left, PANEL_CORNER_RADIUS);
    assert_eq!(w.width, Length::Px(PANEL_WIDTH));
    assert_eq!(w.height, Length::Px(PANEL_HEIGHT));
}

#[test]
fn rules_wizard_chrome_accepts_explicit_active_palette() {
    let mut palette = theme::current().palette;
    palette.surface = Color::from_u8(0x22, 0x33, 0x44, 0xDD);
    palette.surface_alt = Color::from_u8(0x11, 0x22, 0x33, 0xEE);
    palette.selection = Color::from_u8(0x44, 0xAA, 0xEE, 0x66);
    palette.active_overlay = Color::from_u8(0x33, 0x44, 0x55, 0x99);
    palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
    palette.text_muted = Color::from_u8(0x88, 0x99, 0xAA, 0xFF);
    palette.danger = Color::from_u8(0xCC, 0x44, 0x44, 0xFF);

    let chrome = RulesWizardChrome::from_palette(palette);

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
        Color::from_u8(0x33, 0x44, 0x55, 0x99)
    );
    assert_eq!(chrome.title_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.body_color, Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF));
    assert_eq!(chrome.muted_color, Color::from_u8(0x88, 0x99, 0xAA, 0xFF));
    assert_eq!(chrome.error_color, Color::from_u8(0xCC, 0x44, 0x44, 0xFF));
}

#[test]
fn rules_wizard_chrome_accepts_explicit_radius_shadow_tokens() {
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
        color: Color::from_u8(0x10, 0x20, 0x30, 0x99),
    };

    let chrome = RulesWizardChrome::from_tokens(palette, radius, shadow);

    assert_eq!(chrome.panel_shadow, shadow.md);
    assert_eq!(chrome.panel_radius, BorderRadius::all(17.0));
    assert_eq!(chrome.button_radius, BorderRadius::all(7.0));
    assert_eq!(chrome.row_radius, BorderRadius::all(11.0));
}

#[test]
fn rules_wizard_panel_shadow_rect_uses_token_shadow_geometry() {
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
        color: Color::from_u8(0x10, 0x20, 0x30, 0x40),
    };

    let rect = rules_wizard_panel_shadow_rect(panel, shadow);

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
fn runtime_hit_test_maps_buttons_and_visible_rows() {
    let viewport = Size {
        width: 820.0,
        height: 620.0,
    };
    for spec in RULES_WIZARD_ACTION_BUTTONS {
        let rect = rules_wizard_button_rect(viewport, *spec);
        assert_eq!(
            rules_wizard_hit_test(viewport, 3, 0, 0, 0, rect.x + 1.0, rect.y + 1.0),
            Some(spec.hit)
        );
    }

    let second_row = rules_wizard_rule_row_rect(viewport, 1);
    assert_eq!(
        rules_wizard_hit_test(viewport, 3, 0, 0, 0, second_row.x + 1.0, second_row.y + 1.0),
        Some(RulesWizardPointerHit::Row(1))
    );
    assert_eq!(
        rules_wizard_hit_test(viewport, 1, 0, 0, 0, second_row.x + 1.0, second_row.y + 1.0),
        None
    );
}

#[test]
fn runtime_hit_test_maps_condition_rows_through_visible_window_offset() {
    let viewport = Size {
        width: 820.0,
        height: 620.0,
    };
    let second_displayed = rules_wizard_condition_row_rect(viewport, 1);
    assert_eq!(
        rules_wizard_hit_test(
            viewport,
            0,
            0,
            6,
            2,
            second_displayed.x + 1.0,
            second_displayed.y + 1.0
        ),
        Some(RulesWizardPointerHit::ConditionRow(3))
    );
    assert_eq!(rules_wizard_visible_condition_window_start(5, 6), 2);
    let beyond_displayed = rules_wizard_condition_row_rect(viewport, 4);
    assert_eq!(
        rules_wizard_hit_test(
            viewport,
            0,
            0,
            6,
            2,
            beyond_displayed.x + 1.0,
            beyond_displayed.y + 1.0
        ),
        None
    );
    let disabled_condition_hit = rules_wizard_condition_row_rect(viewport, 0);
    assert_eq!(
        rules_wizard_hit_test(
            viewport,
            0,
            0,
            0,
            0,
            disabled_condition_hit.x + 1.0,
            disabled_condition_hit.y + 1.0
        ),
        None
    );
}

#[test]
fn runtime_visible_rule_window_tracks_cursor_after_first_page() {
    assert_eq!(rules_wizard_visible_rule_window_start(0, 0), 0);
    assert_eq!(rules_wizard_visible_rule_window_start(0, 9), 0);
    assert_eq!(rules_wizard_visible_rule_window_start(5, 9), 0);
    assert_eq!(rules_wizard_visible_rule_window_start(6, 9), 1);
    assert_eq!(rules_wizard_visible_rule_window_start(8, 9), 3);
    assert_eq!(rules_wizard_visible_rule_window_start(40, 9), 3);
}

#[test]
fn runtime_hit_test_maps_rule_rows_through_visible_window_offset() {
    let viewport = Size {
        width: 820.0,
        height: 620.0,
    };
    let first_displayed = rules_wizard_rule_row_rect(viewport, 0);
    assert_eq!(
        rules_wizard_hit_test(
            viewport,
            9,
            3,
            0,
            0,
            first_displayed.x + 1.0,
            first_displayed.y + 1.0
        ),
        Some(RulesWizardPointerHit::Row(3))
    );
    let last_displayed = rules_wizard_rule_row_rect(viewport, 5);
    assert_eq!(
        rules_wizard_hit_test(
            viewport,
            9,
            3,
            0,
            0,
            last_displayed.x + 1.0,
            last_displayed.y + 1.0
        ),
        Some(RulesWizardPointerHit::Row(8))
    );
    let beyond_displayed = rules_wizard_rule_row_rect(viewport, 6);
    assert_eq!(
        rules_wizard_hit_test(
            viewport,
            9,
            3,
            0,
            0,
            beyond_displayed.x + 1.0,
            beyond_displayed.y + 1.0
        ),
        None
    );
}

#[test]
fn runtime_visible_rule_summary_reports_visible_range_only_for_overflow() {
    assert_eq!(rules_wizard_visible_rule_summary(0, 6), None);
    assert_eq!(
        rules_wizard_visible_rule_summary(0, 9).map(|s| s.to_string()),
        Some("Rules 1-6 of 9".to_owned())
    );
    assert_eq!(
        rules_wizard_visible_rule_summary(3, 9).map(|s| s.to_string()),
        Some("Rules 4-9 of 9".to_owned())
    );
    assert_eq!(
        rules_wizard_visible_rule_summary(40, 9).map(|s| s.to_string()),
        Some("Rules 4-9 of 9".to_owned())
    );
}

// ─── WizardStep ────────────────────────────────────────────────

#[test]
fn step_index_round_trip_covers_all_variants() {
    let indices: Vec<u32> = WizardStep::ALL.iter().map(|s| s.index()).collect();
    assert_eq!(indices, vec![1, 2, 3, 4, 5]);
}

#[test]
fn step_navigation_saturates_at_bounds() {
    assert_eq!(WizardStep::Conditions.prev(), WizardStep::Conditions);
    assert_eq!(WizardStep::Review.next(), WizardStep::Review);
}

#[test]
fn step_serde_round_trip() {
    for step in WizardStep::ALL {
        let s = serde_json::to_string(step).unwrap_or_default();
        let back: WizardStep = serde_json::from_str(&s).unwrap_or_default();
        assert_eq!(*step, back);
    }
}

// ─── State navigation ──────────────────────────────────────────

#[test]
fn fresh_state_starts_on_step_1_with_one_condition() {
    let s = RulesWizardState::new();
    assert_eq!(s.step(), WizardStep::Conditions);
    assert_eq!(s.conditions().len(), 1);
    assert_eq!(s.run_mode(), RunModeChoice::OnDemand);
    assert!(s.enabled());
    assert!(s.preview_hits().is_empty());
}

#[test]
fn cannot_advance_without_valid_condition() {
    let s = RulesWizardState::new();
    assert!(!s.can_advance(), "blank condition value blocks advance");
}

#[test]
fn can_advance_with_one_valid_condition() {
    let mut s = RulesWizardState::new();
    s.set_condition_value(0, "invoice-");
    assert!(s.can_advance());
}

#[test]
fn click_next_with_invalid_step_is_noop() {
    let mut s = RulesWizardState::new();
    s.click_next();
    assert_eq!(s.step(), WizardStep::Conditions);
}

#[test]
fn click_next_advances_through_steps() {
    let mut s = seeded();
    s.click_next(); // Conditions → Action
    assert_eq!(s.step(), WizardStep::Action);
    s.click_next(); // Action → Preview (queues PreviewRequest)
    assert_eq!(s.step(), WizardStep::Preview);
    // Drain the PreviewRequest so subsequent click_next doesn't trip
    // over a stale pending_action.
    assert!(matches!(
        s.take_action(),
        Some(RulesWizardAction::PreviewRequest(_))
    ));
    s.click_next(); // Preview → Name
    assert_eq!(s.step(), WizardStep::Name);
    s.click_next(); // Name → Review
    assert_eq!(s.step(), WizardStep::Review);
    s.click_next(); // saturates
    assert_eq!(s.step(), WizardStep::Review);
}

#[test]
fn entering_preview_marks_busy_and_emits_request() {
    let mut s = seeded();
    s.click_next(); // → Action
    s.click_next(); // → Preview
    assert!(s.preview_busy());
    let action = s.take_action().expect("preview request queued");
    assert!(matches!(action, RulesWizardAction::PreviewRequest(_)));
}

#[test]
fn set_preview_hits_clears_busy() {
    let mut s = seeded();
    s.click_next();
    s.click_next();
    let _ = s.take_action();
    s.set_preview_hits(vec!["C:/Desktop/invoice-2026.pdf".into()]);
    assert!(!s.preview_busy());
    assert_eq!(s.preview_hits().len(), 1);
}

#[test]
fn click_back_walks_backwards() {
    let mut s = seeded();
    s.click_next(); // → Action
    s.click_back();
    assert_eq!(s.step(), WizardStep::Conditions);
    s.click_back(); // saturates
    assert_eq!(s.step(), WizardStep::Conditions);
}

// ─── Condition row management ──────────────────────────────────

#[test]
fn add_and_remove_condition_rows() {
    let mut s = RulesWizardState::new();
    s.add_condition();
    s.add_condition();
    assert_eq!(s.conditions().len(), 3);
    assert_eq!(s.condition_cursor(), 2);
    s.remove_condition(1);
    assert_eq!(s.conditions().len(), 2);
    assert_eq!(s.condition_cursor(), 1);
}

#[test]
fn remove_last_condition_is_noop() {
    let mut s = RulesWizardState::new();
    s.remove_condition(0);
    assert_eq!(s.conditions().len(), 1);
    assert_eq!(s.condition_cursor(), 0);
}

#[test]
fn condition_cursor_selects_next_and_clamps_to_existing_rows() {
    let mut s = RulesWizardState::new();
    s.add_condition();
    s.add_condition();
    s.set_condition_cursor(1);
    assert_eq!(s.condition_cursor(), 1);
    s.select_next_condition();
    assert_eq!(s.condition_cursor(), 2);
    s.select_next_condition();
    assert_eq!(s.condition_cursor(), 0);
    s.set_condition_cursor(99);
    assert_eq!(s.condition_cursor(), 2);
    s.remove_condition(2);
    assert_eq!(s.condition_cursor(), 1);
}

#[test]
fn switching_predicate_kind_clears_value_when_no_value_needed() {
    let mut s = RulesWizardState::new();
    s.set_condition_value(0, "anything");
    s.set_condition_kind(0, PredicateKind::OnDesktop);
    assert!(s.conditions()[0].value.is_empty());
}

// ─── Build rule from state ─────────────────────────────────────

#[test]
fn save_with_complete_state_emits_rule() {
    let mut s = seeded();
    s.click_next();
    s.click_next();
    let _ = s.take_action(); // drain PreviewRequest
    s.click_next();
    s.click_next();
    assert_eq!(s.step(), WizardStep::Review);
    s.click_save();
    let action = s.take_action().expect("save action");
    let RulesWizardAction::Save(rule) = action else {
        panic!("expected Save");
    };
    assert_eq!(rule.name, "Archive invoices");
    assert!(rule.enabled);
    assert_eq!(rule.actions.len(), 1);
    assert!(matches!(rule.actions[0], Action::MoveToZone(_)));
    assert_eq!(rule.run_mode, RunMode::OnDemand);
    match &rule.conditions {
        ConditionGroup::All(nodes) => assert_eq!(nodes.len(), 1),
        _ => panic!("expected All"),
    }
}

#[test]
fn save_with_incomplete_state_is_noop() {
    let mut s = RulesWizardState::new();
    s.click_save();
    assert!(s.take_action().is_none());
}

#[test]
fn interval_run_mode_emits_clamped_minutes() {
    let mut s = seeded();
    s.set_run_mode(RunModeChoice::Interval);
    s.set_interval_minutes(99_999);
    s.click_save();
    let RulesWizardAction::Save(rule) = s.take_action().expect("save") else {
        panic!("expected Save");
    };
    assert_eq!(
        rule.run_mode,
        RunMode::Interval {
            minutes: INTERVAL_MAX_MINUTES
        }
    );
}

#[test]
fn any_combine_mode_builds_any_group() {
    let mut s = seeded();
    s.set_combine(CombineMode::Any);
    s.click_save();
    let RulesWizardAction::Save(rule) = s.take_action().expect("save") else {
        panic!("expected Save");
    };
    assert!(matches!(rule.conditions, ConditionGroup::Any(_)));
}

// ─── Edit-mode load_rule round trip ────────────────────────────

#[test]
fn load_rule_round_trip_preserves_visible_state() {
    let mut s = RulesWizardState::new();
    let original = Rule {
        id: SmolStr::new_static("r-1"),
        name: "Sweep tmp".to_string(),
        enabled: false,
        conditions: ConditionGroup::Any(vec![
            ConditionNode::Leaf(Condition::ExtensionIn(vec![SmolStr::new_static("tmp")])),
            ConditionNode::Leaf(Condition::CreatedBefore { days_ago: 7 }),
        ]),
        actions: vec![Action::DeleteToRecycleBin],
        run_mode: RunMode::Interval { minutes: 30 },
        last_run: None,
        run_count: 0,
    };
    s.load_rule(original);
    assert_eq!(s.name(), "Sweep tmp");
    assert!(!s.enabled());
    assert_eq!(s.combine(), CombineMode::Any);
    assert_eq!(s.conditions().len(), 2);
    assert_eq!(s.action().kind, ActionKind::DeleteToRecycleBin);
    assert_eq!(s.run_mode(), RunModeChoice::Interval);
    assert_eq!(s.interval_minutes(), 30);
}

#[test]
fn load_rule_with_nested_not_flattens_to_inner_leaves() {
    let mut s = RulesWizardState::new();
    let nested = Rule {
        id: SmolStr::new_static("r-2"),
        name: "n".to_string(),
        enabled: true,
        conditions: ConditionGroup::Not(Box::new(ConditionGroup::All(vec![ConditionNode::Leaf(
            Condition::OnDesktop,
        )]))),
        actions: vec![Action::Notify("hi".into())],
        run_mode: RunMode::OnDemand,
        last_run: None,
        run_count: 0,
    };
    s.load_rule(nested);
    assert_eq!(s.combine(), CombineMode::All);
    assert_eq!(s.conditions().len(), 1);
    assert_eq!(s.conditions()[0].kind, PredicateKind::OnDesktop);
}

// ─── Cancel + take_action ───────────────────────────────────────

#[test]
fn click_cancel_records_cancel() {
    let mut s = RulesWizardState::new();
    s.click_cancel();
    assert_eq!(s.take_action(), Some(RulesWizardAction::Cancel));
}

#[test]
fn take_action_is_one_shot() {
    let mut s = RulesWizardState::new();
    s.click_cancel();
    assert!(s.take_action().is_some());
    assert!(s.take_action().is_none());
}

// ─── Build subtree ─────────────────────────────────────────────

#[test]
fn build_returns_panel_sized_container() {
    let node = build();
    let layout = node.layout();
    assert!(matches!(layout.width, Length::Px(w) if (w - PANEL_WIDTH).abs() < 0.01));
    assert!(matches!(layout.height, Length::Px(h) if (h - PANEL_HEIGHT).abs() < 0.01));
    assert_eq!(layout.direction, Direction::Column);
}
