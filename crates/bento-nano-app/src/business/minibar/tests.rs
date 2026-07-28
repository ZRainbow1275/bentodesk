use super::*;
use bento_nano_style::tokens as style_tokens;

#[test]
fn minibar_with_tauri_tokens_consumes_wave_b_ssot() {
    let bar = MiniBar::new("M0 0L1 1", "Docs", 8).with_tauri_tokens(
        style_tokens::PALETTE_DARK,
        style_tokens::RADIUS,
        style_tokens::SPACING,
    );
    // Wave A flagged `RADIUS.minibar` = 14 px (resolved gap).
    assert_eq!(
        bar.border_radius,
        BorderRadius::all(style_tokens::RADIUS.minibar)
    );
    // Wave A: unique minibar gradient distinct from surface_zen.
    assert_eq!(
        bar.background,
        style_tokens::PALETTE_DARK.minibar_gradient_top
    );
    // Wave B: lg = 16 px.
    assert_eq!(bar.padding.left, style_tokens::SPACING.lg);
    assert_eq!(
        bar.unpin_button.tint,
        style_tokens::PALETTE_DARK.text_primary
    );
    assert_eq!(
        bar.unpin_button.hover_background,
        style_tokens::PALETTE_DARK.surface_hover,
    );
}

/// Recording fake — every hibernation call increments a counter so
/// the smoke test can assert the trigger logic fired correctly.
/// Mirrors `Renderer`'s contract: starts resident, release/ensure are
/// idempotent, ensure may report `Err` (we never inject one in this
/// test fixture; the unhappy path is exercised by the unit test below).
#[derive(Debug, Default)]
struct RecordingGate {
    resident: bool,
    release_count: u32,
    ensure_count: u32,
    ensure_should_fail: bool,
    last_ensure_size: Option<(u32, u32)>,
}

impl RecordingGate {
    fn resident_default() -> Self {
        Self {
            resident: true,
            ..Self::default()
        }
    }
}

impl HibernationGate for RecordingGate {
    fn release_swap_chain(&mut self) {
        self.release_count += 1;
        self.resident = false;
    }
    fn ensure_swap_chain(&mut self, w: u32, h: u32) -> Result<(), MiniBarError> {
        self.ensure_count += 1;
        self.last_ensure_size = Some((w, h));
        if self.ensure_should_fail {
            return Err(MiniBarError::SwapChainEnsure("fake gate"));
        }
        self.resident = true;
        Ok(())
    }
    fn is_chain_resident(&self) -> bool {
        self.resident
    }
}

#[test]
fn minibar_controller_hide_releases_chain() {
    let gate = RecordingGate::resident_default();
    let mut ctrl = MiniBarController::new(gate, 280, 80);
    assert!(ctrl.is_resident());
    assert!(ctrl.is_visible());

    ctrl.hide();
    assert!(!ctrl.is_visible(), "hide() must flip visibility");
    assert!(
        !ctrl.is_resident(),
        "T-099: chain MUST be released after hide()"
    );
    assert_eq!(ctrl.gate().release_count, 1);
}

#[test]
fn minibar_controller_show_after_hide_ensures_chain() {
    let gate = RecordingGate::resident_default();
    let mut ctrl = MiniBarController::new(gate, 280, 80);
    ctrl.hide();
    assert!(!ctrl.is_resident());

    let res = ctrl.show();
    assert!(
        res.is_ok(),
        "ensure_swap_chain must succeed in the smoke fake: {res:?}"
    );
    assert!(ctrl.is_visible());
    assert!(
        ctrl.is_resident(),
        "T-099: chain MUST be rebuilt after show()"
    );
    assert_eq!(ctrl.gate().ensure_count, 1);
    assert_eq!(ctrl.gate().last_ensure_size, Some((280, 80)));
}

#[test]
fn minibar_controller_hide_is_idempotent() {
    let gate = RecordingGate::resident_default();
    let mut ctrl = MiniBarController::new(gate, 280, 80);
    ctrl.hide();
    ctrl.hide(); // second hide must NOT call release again
    assert_eq!(ctrl.gate().release_count, 1, "hide() MUST be idempotent");
}

#[test]
fn minibar_controller_show_is_idempotent_when_already_resident() {
    let gate = RecordingGate::resident_default();
    let mut ctrl = MiniBarController::new(gate, 280, 80);
    let res = ctrl.show();
    assert!(res.is_ok(), "no-op show() succeeds: {res:?}");
    assert_eq!(ctrl.gate().ensure_count, 0, "show() MUST be idempotent");
}

#[test]
fn minibar_controller_show_propagates_ensure_failure() {
    let gate = RecordingGate {
        resident: true,
        ensure_should_fail: true,
        ..Default::default()
    };
    let mut ctrl = MiniBarController::new(gate, 280, 80);
    ctrl.hide();
    let res = ctrl.show();
    assert_eq!(res, Err(MiniBarError::SwapChainEnsure("fake gate")));
    // Visibility flag stays `false` so the next show retries from scratch.
    assert!(!ctrl.is_visible());
}

#[test]
fn minibar_roster_caps_at_max_minibars() {
    let mut r = MiniBarRoster::new();
    for i in 0..MAX_MINIBARS as u64 {
        let res = r.pin(i);
        assert_eq!(res, Ok(MAX_MINIBARS - (i as usize + 1)));
    }
    assert_eq!(r.len(), MAX_MINIBARS);
    assert_eq!(r.pin(99), Err(MiniBarError::CapReached));
}

#[test]
fn minibar_roster_rejects_duplicate_pin() {
    let mut r = MiniBarRoster::new();
    assert!(r.pin(7).is_ok());
    assert_eq!(r.pin(7), Err(MiniBarError::AlreadyPinned));
}

#[test]
fn minibar_roster_unpin_removes_id() {
    let mut r = MiniBarRoster::new();
    let _ = r.pin(7);
    assert!(r.contains(7));
    assert_eq!(r.unpin(7), Ok(()));
    assert!(!r.contains(7));
    assert_eq!(r.unpin(7), Err(MiniBarError::NotFound));
}

#[test]
fn minibar_widget_default_dimensions_match_window_kind_default() {
    // `default_size(WindowKind::MiniBar) = (280, 80)` per the platform
    // crate's T-011 surface; the widget descriptor MUST match so the
    // first paint sizes correctly without an extra `resize` round trip.
    let bar = MiniBar::new("M0 0L1 1", "Test Zone", 42);
    assert_eq!(bar.width, Length::Px(280.0));
    assert_eq!(bar.height, Length::Px(80.0));
    assert_eq!(bar.border_radius.top_left, 12.0);
}

#[test]
fn minibar_widget_uses_theme_palette_surface() {
    let bar = MiniBar::new("M0 0L1 1", "X", 1);
    assert_eq!(bar.background, theme::current().palette.surface);
}

#[test]
fn minibar_descriptor_accepts_explicit_active_tokens() {
    let tokens = theme::current();
    let mut palette = tokens.palette;
    let mut radius = tokens.radius;
    let mut spacing = tokens.spacing;
    palette.surface = Color::from_u8(0x24, 0x35, 0x46, 0xDD);
    palette.text = Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF);
    palette.hover_overlay = Color::from_u8(0xAA, 0xBB, 0xCC, 0x22);
    radius.xl = BorderRadius::all(18.0);
    spacing.lg = 15.0;

    let bar = MiniBar::from_tokens("M0 0L1 1", "Active", 88, palette, radius, spacing);

    assert_eq!(bar.background, Color::from_u8(0x24, 0x35, 0x46, 0xDD));
    assert_eq!(bar.border_radius, BorderRadius::all(18.0));
    assert_eq!(bar.padding.left, 15.0);
    assert_eq!(
        bar.unpin_button.tint,
        Color::from_u8(0xEE, 0xDD, 0xCC, 0xFF)
    );
    assert_eq!(
        bar.unpin_button.hover_background,
        Color::from_u8(0xAA, 0xBB, 0xCC, 0x22)
    );
    assert_eq!(bar.unpin_button.on_click_event, 88);
}

#[test]
fn minibar_geometry_maps_icon_label_and_unpin() {
    let bar = MiniBar::new("M0 0L1 1", "Zone", 8);
    let viewport = Size {
        width: 280.0,
        height: 80.0,
    };

    let panel = minibar_panel_rect(viewport);
    assert_eq!(panel.width, 280.0);
    assert_eq!(panel.height, 80.0);

    let icon = minibar_icon_rect(viewport, &bar);
    assert_eq!(icon.x, 12.0);
    assert_eq!(icon.y, 28.0);

    let unpin = minibar_unpin_rect(viewport, &bar);
    assert_eq!(unpin.x, 244.0);
    assert_eq!(unpin.y, 28.0);

    let label = minibar_label_rect(viewport, &bar);
    assert_eq!(label.x, 46.0);
    assert_eq!(label.width, 188.0);
}

#[test]
fn minibar_hit_test_prioritizes_unpin_affordance() {
    let bar = MiniBar::new("M0 0L1 1", "Zone", 8);
    let viewport = Size {
        width: 280.0,
        height: 80.0,
    };

    assert_eq!(
        minibar_hit_test(viewport, &bar, 250.0, 40.0),
        Some(MiniBarHit::Unpin)
    );
    assert_eq!(
        minibar_hit_test(viewport, &bar, 50.0, 40.0),
        Some(MiniBarHit::Body)
    );
    assert_eq!(minibar_hit_test(viewport, &bar, 300.0, 40.0), None);
}

#[test]
fn minibar_item_geometry_maps_visible_source_items_before_body() {
    let bar = MiniBar::new("M0 0L1 1", "Docs", 8);
    let viewport = Size {
        width: 280.0,
        height: 80.0,
    };

    let capacity = minibar_item_capacity(viewport, &bar);
    assert!(capacity > 0);
    let first = minibar_item_rect(viewport, &bar, 0).expect("first item rect");

    assert_eq!(
        minibar_hit_test_with_items(
            viewport,
            &bar,
            3,
            first.x + first.width * 0.5,
            first.y + first.height * 0.5
        ),
        Some(MiniBarHit::Item(0))
    );
    assert_eq!(
        minibar_hit_test_with_items(viewport, &bar, 0, first.x + 1.0, first.y + 1.0),
        Some(MiniBarHit::Body)
    );
}

#[test]
fn minibar_item_geometry_caps_to_tauri_source_item_limit() {
    let bar = MiniBar::new("M0 0L1 1", "Docs", 8);
    let wide_viewport = Size {
        width: 900.0,
        height: 80.0,
    };

    assert!(minibar_item_rect(wide_viewport, &bar, MINIBAR_SOURCE_MAX_ITEMS - 1).is_some());
    assert!(minibar_item_rect(wide_viewport, &bar, MINIBAR_SOURCE_MAX_ITEMS).is_none());
}
