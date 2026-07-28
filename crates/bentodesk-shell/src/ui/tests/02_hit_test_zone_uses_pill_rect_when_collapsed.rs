// Wave C (05-20 visual parity) — pill hit-test + DPI-rect tests.

#[test]
fn hit_test_zone_uses_pill_rect_when_collapsed() {
    let zone = Zone::new(ZoneId(42), Cow::Borrowed("Docs"), 100, 100, 240, 180);
    let app = app_with_zones(vec![zone]);
    // Default ZoneDisplayMode::Hover, no hover/select → collapsed pill.
    let layout = bentodesk_app::zone_pill_geometry::pill_layout_for_zone(
        app.zones.get(ZoneId(42)).expect("zone"),
        0,
    );
    let inside_x = layout.rect.x + layout.rect.width * 0.5;
    let inside_y = layout.rect.y + layout.rect.height * 0.5;
    assert_eq!(hit_test_zone(&app, inside_x, inside_y), Some(ZoneId(42)));
    // Beyond the pill but within the legacy 240×180 rect → no hit.
    assert_eq!(hit_test_zone(&app, 100.0 + 180.0, 100.0 + 100.0), None);
}

#[test]
fn hit_test_stack_anchor_uses_stack_capsule_rect_when_collapsed() {
    let anchor = Zone::new(ZoneId(420), Cow::Borrowed("Anchor"), 100, 100, 240, 180);
    let child = Zone::new(ZoneId(421), Cow::Borrowed("Child"), 160, 160, 240, 180);
    let mut app = app_with_zones(vec![anchor, child]);
    assert!(app.zones.stack(ZoneId(420), ZoneId(421)));

    let anchor = app.zones.get(ZoneId(420)).expect("anchor");
    let pill = bentodesk_app::zone_pill_geometry::pill_layout_for_zone(anchor, 2);
    let stack = bentodesk_app::zone_pill_geometry::stack_capsule_layout_for_zone(anchor, 2);
    assert!(stack.rect.width > pill.rect.width);

    let x_inside_stack_only = pill.rect.right() + (stack.rect.right() - pill.rect.right()) * 0.5;
    let y = stack.rect.y + stack.rect.height * 0.5;
    assert_eq!(
        hit_test_zone(&app, x_inside_stack_only, y),
        Some(ZoneId(420))
    );
    assert_eq!(hit_test_zone(&app, stack.rect.right() + 1.0, y), None);
}

#[test]
fn hit_test_zone_uses_full_rect_when_expanded() {
    let zone = Zone::new(ZoneId(43), Cow::Borrowed("Docs"), 100, 100, 240, 180);
    let app = app_with_zones(vec![zone]);
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    // Far corner of expanded rect is now reachable.
    assert_eq!(
        hit_test_zone(&app, 100.0 + 200.0, 100.0 + 150.0),
        Some(ZoneId(43))
    );
}

// V-13 (2026-05-21) — during the pill→expanded morph the hit-rect MUST
// mirror the painted rect, not snap to the full expanded zone box.
//
// Real flow on first hover: tick N enters pill (hovered=None, anim=None,
// pill hit-rect). On the same tick, `update_zone_pill_hover` sets
// `zone_pill_anim_zone = Some(zone), expanding = true, progress = 0.0`.
// Tick N+1 (a few ms later) has progress ~0.05 — the renderer paints
// `morph_pill_to_rect(pill, expanded, eased(0.05))`, basically still
// pill-sized. Pre-fix, `effective_zone_hit_rect` saw `body_visible=true`
// (because `hovered_zone == zone.id`) and returned the FULL 240×180
// box, so clicks/hover triggered in the invisible "phantom" rectangle
// around the visible pill. Post-fix, case 1 fires and the hit-rect
// tracks the morphed rect within 1 DIP.
#[test]
fn hit_test_zone_morph_just_started_uses_pill_sized_rect() {
    let zone = Zone::new(ZoneId(44), Cow::Borrowed("Docs"), 100, 100, 240, 180);
    let app = app_with_zones(vec![zone]);
    // Tick N+1 state: hovered_zone set, morph kicked off with tiny progress.
    app.hovered_zone.set(Some(ZoneId(44)));
    app.zone_pill_anim_zone.set(Some(ZoneId(44)));
    app.zone_pill_anim_expanding.set(true);
    app.zone_pill_anim_progress.set(0.05);
    // Cursor far outside the pill but inside the legacy 240×180 box:
    // must NOT hit. (Pre-fix this returned Some(ZoneId(44)).)
    assert_eq!(hit_test_zone(&app, 100.0 + 200.0, 100.0 + 150.0), None);
    // Center of the actual pill rect still hits.
    let layout = bentodesk_app::zone_pill_geometry::pill_layout_for_zone(
        app.zones.get(ZoneId(44)).expect("zone"),
        0,
    );
    let cx = layout.rect.x + layout.rect.width * 0.5;
    let cy = layout.rect.y + layout.rect.height * 0.5;
    assert_eq!(hit_test_zone(&app, cx, cy), Some(ZoneId(44)));
}

#[test]
fn hit_test_zone_morph_in_flight_uses_interpolated_rect() {
    let zone = Zone::new(ZoneId(45), Cow::Borrowed("Docs"), 100, 100, 240, 180);
    let app = app_with_zones(vec![zone]);
    // Hover + morph half-way through expanding. Renderer paints
    // morph_pill_to_rect(pill, expanded, ease_out_back(0.5)). Hit-rect
    // must mirror that exactly (paint–hit parity within 1 DIP). Note the
    // easeOutBack curve OVERSHOOTS at raw=0.5 (eased ≈ 1.087), so the
    // morphed rect extrapolates ~8.7% PAST the expanded target — the
    // hit-rect tracks that bulge, which is the whole point of the V-13 fix.
    app.hovered_zone.set(Some(ZoneId(45)));
    app.zone_pill_anim_zone.set(Some(ZoneId(45)));
    app.zone_pill_anim_expanding.set(true);
    app.zone_pill_anim_progress.set(0.5);
    let layout = bentodesk_app::zone_pill_geometry::pill_layout_for_zone(
        app.zones.get(ZoneId(45)).expect("zone"),
        0,
    );
    let expanded = bentodesk_style::Rect {
        x: 100.0,
        y: 100.0,
        width: 240.0,
        height: 180.0,
    };
    let eased = bentodesk_app::zone_pill_geometry::ease_out_back_progress(0.5);
    let morphed =
        bentodesk_app::zone_pill_geometry::morph_pill_to_rect(layout.rect, expanded, eased);
    // Inside the morphed rect → hit.
    let cx = morphed.x + morphed.width * 0.5;
    let cy = morphed.y + morphed.height * 0.5;
    assert_eq!(hit_test_zone(&app, cx, cy), Some(ZoneId(45)));
    // Just outside the morphed rect (1 DIP beyond right edge) →
    // no hit. This is the paint-hit parity guarantee.
    assert_eq!(hit_test_zone(&app, morphed.right() + 1.0, cy), None);
}

#[test]
fn hit_test_zone_morph_complete_uses_full_rect() {
    let zone = Zone::new(ZoneId(46), Cow::Borrowed("Docs"), 100, 100, 240, 180);
    let app = app_with_zones(vec![zone]);
    // Morph finished at progress=1.0 with the zone in a settled expanded
    // state — renderer paints the full chrome, so hit-rect is full rect.
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    app.zone_pill_anim_zone.set(Some(ZoneId(46)));
    app.zone_pill_anim_expanding.set(true);
    app.zone_pill_anim_progress.set(1.0);
    // Far corner of full expanded rect → hit.
    assert_eq!(
        hit_test_zone(&app, 100.0 + 200.0, 100.0 + 150.0),
        Some(ZoneId(46))
    );
}

// #5 / Bug A (2026-06-02) — DRAGGING a COLLAPSED pill must keep its hit rect
// the PILL rect (the pill follows the cursor), NOT force the expanded body.
// Pre-fix `pill_body_visible` (and a mirrored hit-rect rule) OR-ed in
// `active_id` (drag OR resize), so a dragged collapsed pill snapped to its
// mostly-empty 240×180 expanded box — the pill appeared to "disappear" into
// the panel that then followed the cursor. The fix narrows the force term to
// RESIZE only.
#[test]
fn hit_test_dragged_collapsed_pill_stays_pill_sized() {
    let zone = Zone::new(ZoneId(47), Cow::Borrowed("Docs"), 100, 100, 240, 180);
    let app = app_with_zones(vec![zone]);
    // Default ZoneDisplayMode::Hover, no hover before mouse-down → collapsed
    // pill. Production mouse-down selects before arming DRAG; the drag-start
    // visual snapshot must keep that selected state from expanding the hit
    // rect mid-gesture.
    app.selected_zone.set(Some(ZoneId(47)));
    app.zone_drag.set(Some((ZoneId(47), 5, 5)));
    app.zone_drag_body_visible_at_start
        .set(Some((ZoneId(47), false)));
    // The legacy 240×180 far corner must NOT hit — a drag does not expand.
    assert_eq!(hit_test_zone(&app, 100.0 + 200.0, 100.0 + 150.0), None);
    // The pill rect centre still hits (the pill itself is the drag target).
    let layout = bentodesk_app::zone_pill_geometry::pill_layout_for_zone(
        app.zones.get(ZoneId(47)).expect("zone"),
        0,
    );
    let cx = layout.rect.x + layout.rect.width * 0.5;
    let cy = layout.rect.y + layout.rect.height * 0.5;
    assert_eq!(hit_test_zone(&app, cx, cy), Some(ZoneId(47)));
}

// #5 / Bug A (2026-06-02) — a RESIZE (only armable on an already-expanded
// panel) keeps forcing the expanded body, so paint==hit during a resize.
#[test]
fn hit_test_resizing_zone_keeps_full_rect() {
    let zone = Zone::new(ZoneId(48), Cow::Borrowed("Docs"), 100, 100, 240, 180);
    let app = app_with_zones(vec![zone]);
    // Resize is only ever armed on an expanded panel; emulate that state.
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Always);
    app.zone_resize.set(Some((ZoneId(48), 240, 180)));
    // Far corner of the expanded rect remains reachable during the resize.
    assert_eq!(
        hit_test_zone(&app, 100.0 + 200.0, 100.0 + 150.0),
        Some(ZoneId(48))
    );
}

// Z-order (2026-06-02) — an EXPANDED panel must occlude (in both paint and
// hit) the COLLAPSED pills of zones that sit inside its footprint. With the
// dense 4×4 grid an expanded zone's 480×432 body overlaps the pills of zones
// a row below; the single-pass resolver returned the buried pill (drawn last,
// reverse-iterated first), so hovering the overlap region mis-targeted the
// pill and the panel collapsed/flickered. The two-layer resolver tests the
// `on_top` (expanded/morphing) layer FIRST, so a point inside the panel
// resolves to the panel.
#[test]
fn hit_test_overlapping_expanded_panel_wins_over_buried_pill() {
    // A = ZoneId(50) will be SELECTED in Click mode.
    // B = ZoneId(51) stays a collapsed pill, declared LATER in zone order so
    // the old single reverse pass would have hit B first.
    let a = Zone::new(ZoneId(50), Cow::Borrowed("Panel"), 100, 100, 240, 180);
    let b = Zone::new(ZoneId(51), Cow::Borrowed("Pill"), 150, 150, 240, 180);
    let app = app_with_zones(vec![a, b]);
    // Click mode makes selection the only structural expansion source:
    // A is the expanded/top layer, B remains a collapsed bottom-layer pill.
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Click);
    app.selected_zone.set(Some(ZoneId(50)));

    // Sanity: the layer predicate splits them as intended.
    assert!(app.zone_on_top(app.zones.get(ZoneId(50)).unwrap()));
    assert!(!app.zone_on_top(app.zones.get(ZoneId(51)).unwrap()));

    // Point P = centre of B's collapsed pill, which sits INSIDE A's expanded
    // 240×180 body (100..340, 100..280).
    let pill = bentodesk_app::zone_pill_geometry::pill_layout_for_zone(
        app.zones.get(ZoneId(51)).expect("b"),
        0,
    );
    let px = pill.rect.x + pill.rect.width * 0.5;
    let py = pill.rect.y + pill.rect.height * 0.5;
    // P really is over B's pill AND inside A's expanded body.
    assert!(px >= pill.rect.x && px < pill.rect.right());
    assert!((100.0..340.0).contains(&px) && (100.0..280.0).contains(&py));

    // The expanded panel A wins, NOT the buried pill B.
    assert_eq!(hit_test_zone(&app, px, py), Some(ZoneId(50)));
}

// Z-order (2026-06-02) — the same invariant for a MORPHING zone (mid pill↔
// panel transition is on the top layer too) so a buried pill never wins
// over the in-flight panel.
#[test]
fn hit_test_overlapping_morphing_panel_wins_over_buried_pill() {
    let a = Zone::new(ZoneId(52), Cow::Borrowed("Morph"), 100, 100, 240, 180);
    let b = Zone::new(ZoneId(53), Cow::Borrowed("Pill"), 150, 150, 240, 180);
    let app = app_with_zones(vec![a, b]);
    // A is mid-morph (expanding, halfway) → top layer via zone_on_top.
    app.hovered_zone.set(Some(ZoneId(52)));
    app.zone_pill_anim_zone.set(Some(ZoneId(52)));
    app.zone_pill_anim_expanding.set(true);
    app.zone_pill_anim_progress.set(0.5);
    assert!(app.zone_on_top(app.zones.get(ZoneId(52)).unwrap()));
    assert!(!app.zone_on_top(app.zones.get(ZoneId(53)).unwrap()));

    // Centre of A's morphed rect (mirrors the painted interpolated rect).
    let pill_a = bentodesk_app::zone_pill_geometry::pill_layout_for_zone(
        app.zones.get(ZoneId(52)).expect("a"),
        0,
    );
    let expanded_a = bentodesk_style::Rect {
        x: 100.0,
        y: 100.0,
        width: 240.0,
        height: 180.0,
    };
    let eased = bentodesk_app::zone_pill_geometry::ease_out_back_progress(0.5);
    let morphed =
        bentodesk_app::zone_pill_geometry::morph_pill_to_rect(pill_a.rect, expanded_a, eased);
    let cx = morphed.x + morphed.width * 0.5;
    let cy = morphed.y + morphed.height * 0.5;
    // The morphing panel A wins over the buried pill B underneath it.
    assert_eq!(hit_test_zone(&app, cx, cy), Some(ZoneId(52)));
}

// Z-order (2026-06-02) — draw-ordering assertion. Given one expanded zone
// and several collapsed pills, the `zone_on_top` layering helper puts ALL
// collapsed indices in the bottom layer and the expanded zone in the top
// layer. Reproduce the exact two-pass order `draw_zones` walks (pass 1 =
// !on_top in zone order, pass 2 = on_top in zone order) and assert every
// collapsed zone is drawn before the expanded zone.
#[test]
fn draw_order_places_all_pills_before_expanded_panel() {
    let zones = vec![
        Zone::new(ZoneId(60), Cow::Borrowed("p0"), 0, 0, 240, 180),
        Zone::new(ZoneId(61), Cow::Borrowed("expanded"), 300, 0, 240, 180),
        Zone::new(ZoneId(62), Cow::Borrowed("p2"), 0, 200, 240, 180),
        Zone::new(ZoneId(63), Cow::Borrowed("p3"), 300, 200, 240, 180),
    ];
    let app = app_with_zones(zones);
    // Click mode; select only ZoneId(61) → it is the sole top-layer zone,
    // while the other three stay collapsed pills (bottom layer).
    app.set_zone_display_mode(bentodesk_app::ZoneDisplayMode::Click);
    app.selected_zone.set(Some(ZoneId(61)));

    // Walk the exact two-pass draw order used by `Renderer::draw_zones`.
    let mut draw_order = Vec::new();
    for on_top_layer in [false, true] {
        for zone in app.zones.iter() {
            if !zone.is_visible() || zone.is_stacked_child() {
                continue;
            }
            if app.zone_on_top(zone) != on_top_layer {
                continue;
            }
            draw_order.push(zone.id);
        }
    }

    // Exactly one expanded zone; it is drawn LAST.
    let expanded_pos = draw_order
        .iter()
        .position(|id| *id == ZoneId(61))
        .expect("expanded drawn");
    assert_eq!(
        expanded_pos,
        draw_order.len() - 1,
        "expanded panel must be last"
    );
    // Every collapsed pill is drawn BEFORE the expanded panel, in zone order.
    assert_eq!(
        &draw_order[..expanded_pos],
        &[ZoneId(60), ZoneId(62), ZoneId(63)],
    );
    // And the layer predicate agrees: only ZoneId(61) is on top.
    for id in [ZoneId(60), ZoneId(62), ZoneId(63)] {
        assert!(
            !app.zone_on_top(app.zones.get(id).unwrap()),
            "{id:?} should be a pill"
        );
    }
    assert!(app.zone_on_top(app.zones.get(ZoneId(61)).unwrap()));
}

#[test]
fn hit_test_zone_item_skipped_in_collapsed_pill_mode() {
    let mut zone = Zone::new(ZoneId(44), Cow::Borrowed("z"), 10, 10, 240, 180);
    let _item = zone
        .add_item(
            Cow::Owned("C:/Users/BentoDeskTest/Desktop/A.lnk".to_owned()),
            Cow::Borrowed("hash-a"),
        )
        .expect("item");
    let app = app_with_zones(vec![zone]);
    // Hover-default + not hovered → pill mode — items not hit-testable.
    assert!(hit_test_zone_item(&app, 24.0, 48.0).is_none());
}

#[test]
fn item_grid_position_for_point_clamps_to_visible_columns() {
    let app = app_with_zones(vec![Zone::new(
        ZoneId(9),
        Cow::Borrowed("grid"),
        10,
        20,
        240,
        180,
    )]);

    assert_eq!(
        item_grid_position_for_point(&app, ZoneId(9), 28.0, 80.0),
        Some((0, 0))
    );
    assert_eq!(
        item_grid_position_for_point(&app, ZoneId(9), 500.0, 200.0),
        Some((2, 1))
    );
    assert_eq!(
        item_grid_position_for_point(&app, ZoneId(99), 28.0, 80.0),
        None
    );
}

#[test]
fn item_grid_position_for_point_uses_zone_grid_columns() {
    let mut zone = Zone::new(ZoneId(10), Cow::Borrowed("grid"), 10, 20, 240, 180);
    zone.set_grid_columns(2);
    let app = app_with_zones(vec![zone]);

    assert_eq!(
        item_grid_position_for_point(&app, ZoneId(10), 220.0, 80.0),
        Some((1, 0))
    );
}

#[test]
fn item_grid_position_for_point_uses_effective_columns_for_narrow_five_column_zones() {
    let mut zone = Zone::new(ZoneId(11), Cow::Borrowed("grid"), 64, 332, 320, 220);
    zone.set_grid_columns(5);
    let app = app_with_zones(vec![zone]);

    assert_eq!(
        item_grid_position_for_point(&app, ZoneId(11), 335.0, 458.0),
        Some((3, 0))
    );
}

#[test]
fn settings_hit_outside_returns_outside() {
    let app = app_with_zones(vec![]);
    // (0,0) is outside the centred Settings panel on a 480×320 viewport.
    assert_eq!(settings_hit(&app, 0.0, 0.0), SettingsHit::Outside);
}

#[test]
fn settings_hit_resolves_buttons_and_body() {
    // Round-2 M1 — the dark shell only routes the new variants. Wave K1
    // rect helpers (locale "switch" chip, encryption mode, zone display,
    // theme chip, vault chips, backup entries, recovery actions, etc.)
    // are intentionally orphan-alive per Ruling B and no longer fire.
    let app = app_with_zones(vec![]);

    // Top 5 toggles map to their ToggleX variants in order.
    let scroll_y = 0.0;
    let r0 =
        bentodesk_app::settings_panel::settings_top_toggle_hit_rect(app.viewport, scroll_y, 0);
    assert_eq!(
        settings_hit(&app, r0.x + r0.width * 0.5, r0.y + r0.height * 0.5),
        SettingsHit::ToggleDesktopEmbed
    );
    let r1 =
        bentodesk_app::settings_panel::settings_top_toggle_hit_rect(app.viewport, scroll_y, 1);
    assert_eq!(
        settings_hit(&app, r1.x + r1.width * 0.5, r1.y + r1.height * 0.5),
        SettingsHit::ToggleAutostart
    );
    let r2 =
        bentodesk_app::settings_panel::settings_top_toggle_hit_rect(app.viewport, scroll_y, 2);
    assert_eq!(
        settings_hit(&app, r2.x + r2.width * 0.5, r2.y + r2.height * 0.5),
        SettingsHit::ToggleShowInTaskbar
    );
    let r3 =
        bentodesk_app::settings_panel::settings_top_toggle_hit_rect(app.viewport, scroll_y, 3);
    assert_eq!(
        settings_hit(&app, r3.x + r3.width * 0.5, r3.y + r3.height * 0.5),
        SettingsHit::ToggleSmartLayout
    );
    let r4 =
        bentodesk_app::settings_panel::settings_top_toggle_hit_rect(app.viewport, scroll_y, 4);
    assert_eq!(
        settings_hit(&app, r4.x + r4.width * 0.5, r4.y + r4.height * 0.5),
        SettingsHit::TogglePortableMode
    );

    // Language chip → OpenLocaleMenu.
    let lang = bentodesk_app::settings_panel::settings_language_chip_rect(app.viewport, scroll_y);
    assert_eq!(
        settings_hit(&app, lang.x + lang.width * 0.5, lang.y + lang.height * 0.5),
        SettingsHit::OpenLocaleMenu
    );

    // Footer Cancel + Save.
    let cancel = bentodesk_app::settings_panel::settings_cancel_button_rect(app.viewport);
    assert_eq!(
        settings_hit(
            &app,
            cancel.x + cancel.width * 0.5,
            cancel.y + cancel.height * 0.5
        ),
        SettingsHit::CancelSettings
    );
    let save = bentodesk_app::settings_panel::settings_save_button_rect(app.viewport);
    assert_eq!(
        settings_hit(&app, save.x + save.width * 0.5, save.y + save.height * 0.5),
        SettingsHit::SaveSettings
    );

    // Close × in the sticky header.
    let close = bentodesk_app::settings_panel::settings_close_button_rect_m1(app.viewport);
    assert_eq!(
        settings_hit(
            &app,
            close.x + close.width * 0.5,
            close.y + close.height * 0.5
        ),
        SettingsHit::Close
    );

    // Inside the body chrome but not over a control → Body.
    let body = bentodesk_app::settings_panel::settings_body_rect(app.viewport);
    assert_eq!(
        settings_hit(&app, body.x + 4.0, body.y + body.height - 4.0),
        SettingsHit::Body
    );

    // Outside the panel rect → Outside.
    let panel = bentodesk_app::settings_panel::settings_panel_rect_m1(app.viewport);
    assert_eq!(
        settings_hit(&app, panel.x - 5.0, panel.y - 5.0),
        SettingsHit::Outside
    );
}

// Round-2 M1 — the K1 `settings_hit_resolves_visible_backup_entry_restore`
// test was retired with the K1 vault row. The dispatch variant
// `SettingsHit::RestoreSettingsBackup` stays orphan-alive until M4's
// 设置备份 section re-introduces the hit path.
#[test]
fn _retired_settings_hit_resolves_visible_backup_entry_restore_in_round_2_m1() {}

/// M1g — reachability: with backup entries seeded, clicking 立即备份 /
/// per-row 恢复 resolves to the backup
/// `SettingsHit` variants. Proves the paint→hit chain is wired — after
/// this chunk no backup button is painted-but-unwired. Builds the SAME
/// `SettingsBodyFlags` (idle updater + capped backup count) the hit-tester
/// derives so the sampled button centres line up with production geometry.
#[test]
fn m1g_settings_hit_resolves_backup_create_and_per_row_restore() {
    use bentodesk_app::SettingsBackupEntry;
    use smol_str::SmolStr;

    let app = app_with_zones(vec![]);
    // Seed two real-shaped entries so the per-row restore path is live.
    app.settings_backup_entries.replace(vec![
        SettingsBackupEntry {
            id: SmolStr::new_static("1748467200-100"),
            file_name: SmolStr::new_static("vault-1748467200-100.bin"),
            size_bytes: 4096,
        },
        SettingsBackupEntry {
            id: SmolStr::new_static("1748460000-100"),
            file_name: SmolStr::new_static("vault-1748460000-100.bin"),
            size_bytes: 8192,
        },
    ]);

    // Rebuild the EXACT flags the hit-tester derives: the live Startup
    // gating bools (both default true in AppState::new) + idle updater
    // (StatusOnly) + the capped visible backup row count. Reading them off
    // `app` (rather than hardcoding) is what makes the test's button rects
    // line up with production geometry.
    let entries = app.settings_backup_entries.borrow();
    let visible =
        bentodesk_app::business::settings::backup_card::backup_visible_row_count(&entries);
    let flags = bentodesk_app::settings_panel::SettingsBodyFlags::new(
        app.crash_restart_enabled.get(),
        app.safe_start_after_hibernation.get(),
        false,
        false,
        bentodesk_app::settings_panel::UpdaterHeightKind::StatusOnly,
    )
    .with_backup_rows(visible);
    drop(entries);

    // M5 cleanup — the Backup §9 card is no longer the bottom of the
    // scrollable content (the M6-UI §3 Appearance grid was appended below
    // §9/§11), so scrolling to `max_scroll` reveals the trailing Appearance
    // grid, not Backup. Scroll precisely so the Backup label sits at the top
    // of the visible body instead. `reserve_delta(0)` is the §2 source fold
    // applied to scroll-space at `scroll_offset_y == 0`.
    let reserve_delta_0 =
        bentodesk_app::settings_panel::settings_sources_reserve_delta(flags.source_row_count);
    let label_unscrolled = bentodesk_app::settings_panel::settings_backup_label_rect(
        app.viewport,
        reserve_delta_0,
        &flags,
    )
    .y;
    let scroll_off = scroll_offset_to_top_of_body(app.viewport, &flags, label_unscrolled);
    app.scroll_offset_y.set(scroll_off);
    // M1i fidelity — `settings_hit` folds the §2 source reserve delta into
    // the scroll for all perf-and-below geometry; this test populates no
    // desktop sources (count 0), so apply the matching fold to the rects we
    // compare against, exactly as production paint/hit does.
    let scroll_y = scroll_off + reserve_delta_0;
    let body = bentodesk_app::settings_panel::settings_body_rect(app.viewport);
    let label =
        bentodesk_app::settings_panel::settings_backup_label_rect(app.viewport, scroll_y, &flags);
    assert!(
        label.y >= body.y && label.y < body.bottom(),
        "backup section must scroll into the visible body (label.y={}, body=[{}, {}])",
        label.y,
        body.y,
        body.bottom(),
    );

    let actions = bentodesk_app::settings_panel::settings_backup_actions_row_rect(
        app.viewport,
        scroll_y,
        &flags,
    );
    let create = bentodesk_app::settings_panel::settings_backup_create_button_rect(actions);
    assert_eq!(
        settings_hit(
            &app,
            create.x + create.width * 0.5,
            create.y + create.height * 0.5
        ),
        SettingsHit::CreateSettingsBackup,
    );
    let refresh = bentodesk_app::settings_panel::settings_backup_refresh_button_rect(actions);
    assert_ne!(
        settings_hit(
            &app,
            refresh.x + refresh.width * 0.5,
            refresh.y + refresh.height * 0.5
        ),
        SettingsHit::ListSettingsBackups,
        "the Tauri-parity card has no visible manual Refresh hit target",
    );
    // Per-row 恢复 — index 0 and index 1 each route to their own index.
    for entry_index in 0..visible {
        let row = bentodesk_app::settings_panel::settings_backup_entry_row_rect(
            app.viewport,
            scroll_y,
            &flags,
            entry_index,
        );
        let restore = bentodesk_app::settings_panel::settings_backup_restore_button_rect(row);
        assert_eq!(
            settings_hit(
                &app,
                restore.x + restore.width * 0.5,
                restore.y + restore.height * 0.5,
            ),
            SettingsHit::RestoreSettingsBackup(entry_index),
            "per-row restore must carry the newest-first list index",
        );
    }
}
